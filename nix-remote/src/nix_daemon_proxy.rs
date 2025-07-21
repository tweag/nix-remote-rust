pub use crate::serialize::{NixReadExt, NixWriteExt};
use crate::{serialize::NixSerializer, stderr, worker_op::WorkerOp, Error};
use anyhow::anyhow;
use serde::Deserialize;
use std::io::{Read, Write};

use crate::{NixRead, NixWrite, Result, PROTOCOL_VERSION, WORKER_MAGIC_1, WORKER_MAGIC_2};

pub struct NixDaemonProxy<R, W> {
    rx_from_client: NixRead<R>,
    tx_to_client: NixWrite<W>,
    rx_op_count: u64,
}

impl<R: Read, W: Write> NixDaemonProxy<R, W> {
    pub fn new(r: R, w: W) -> Result<Self> {
        let mut daemon = Self {
            rx_from_client: NixRead { inner: r },
            tx_to_client: NixWrite { inner: w },
            rx_op_count: 0,
        };
        // handshake
        daemon.handshake_with_client().unwrap();
        Ok(daemon)
    }

    #[tracing::instrument(skip(self))]
    fn handshake_with_client(&mut self) -> Result<()> {
        let magic = self.rx_from_client.read_u64()?;
        if magic != WORKER_MAGIC_1 {
            tracing::error!("Got magic {magic:x}, expected magic {WORKER_MAGIC_1:x}");
            todo!("handle error: protocol mismatch 1");
        }

        self.tx_to_client.write_u64(WORKER_MAGIC_2)?;
        self.tx_to_client.write_u64(PROTOCOL_VERSION.into())?;
        self.tx_to_client.flush()?;

        let client_version = self.rx_from_client.read_u64()?;

        if client_version < PROTOCOL_VERSION.into() {
            Err(anyhow!("Client version {client_version} is too old"))?;
        }

        let _obsolete_cpu_affinity = self.rx_from_client.read_u64()?;
        let _obsolete_reserve_space = self.rx_from_client.read_u64()?;
        self.tx_to_client
            .write_string("rust-nix-bazel-0.1.0".as_bytes())?;
        self.tx_to_client.flush()?;

        self.tx_to_client.write_nix(&stderr::Msg::Last(()))?;
        self.tx_to_client.flush()?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn receive_next_op_from_client(&mut self) -> Result<WorkerOp> {
        match self.rx_from_client.inner.read_nix::<WorkerOp>() {
            Err(crate::serialize::Error::Io(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                eprintln!("EOF, closing");
                Err(Error::Deser(crate::serialize::Error::Io(e)))
            }
            Err(e) => {
                eprintln!("EOF, closing");
                Err(Error::Deser(e))
            }
            Ok(worker_op) => {
                self.rx_op_count += 1;
                Ok(worker_op)
            }
        }
    }

    pub fn flush_tx_to_client(&mut self) -> Result<()> {
        self.tx_to_client.inner.flush()?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn send_error_to_client(&mut self, error_msg: &crate::stderr::Msg) -> Result<()> {
        self.tx_to_client.inner.write_nix(error_msg)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), ret)]
    pub fn streaming_length(&mut self) -> Result<usize> {
        let mut de = crate::serialize::NixDeserializer {
            read: &mut self.rx_from_client.inner,
        };
        let len = u64::deserialize(&mut de)? as usize;
        Ok(len)
    }

    pub fn get_stream(&mut self, chunk_len: usize, buff: &mut [u8]) -> Result<()> {
        let de = crate::serialize::NixDeserializer {
            read: &mut self.rx_from_client.inner,
        };
        de.read.read_exact(&mut buff[..chunk_len])?;
        Ok(())
    }

    pub fn write_build_response_to_client<T>(&mut self, resp: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        let mut ser = NixSerializer {
            write: &mut self.tx_to_client.inner,
        };
        let mut dbg_buf = Vec::new();
        let mut dbg_ser = NixSerializer {
            write: &mut dbg_buf,
        };
        resp.serialize(&mut dbg_ser)?;
        resp.serialize(&mut ser)?;
        Ok(())
    }

    pub fn writer(&mut self) -> &mut W {
        &mut self.tx_to_client.inner
    }
}
