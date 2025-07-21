pub use crate::serialize::{NixReadExt, NixWriteExt};
use crate::{
    serialize::NixDeserializer,
    stderr::{self, Msg},
    worker_op::{Resp, WorkerOp},
    NixString,
};
use anyhow::anyhow;
use serde::Serialize;
use std::{
    fmt::Debug,
    io::{BufRead, Read, Write},
};

use crate::{NixRead, NixWrite, Result, PROTOCOL_VERSION, WORKER_MAGIC_1, WORKER_MAGIC_2};

pub struct NixDaemonClient<R, W> {
    rx_from_daemon: NixRead<R>,
    tx_to_daemon: NixWrite<W>,
    tx_op_count: u64,
}

impl<R: Read, W: Write> NixDaemonClient<R, W> {
    pub fn new(r: R, w: W) -> Result<Self> {
        let mut daemon = Self {
            rx_from_daemon: NixRead { inner: r },
            tx_to_daemon: NixWrite { inner: w },
            tx_op_count: 0,
        };
        // handshake
        daemon.handshake_with_daemon().unwrap();
        Ok(daemon)
    }

    fn handshake_with_daemon(&mut self) -> Result<()> {
        self.tx_to_daemon.inner.write_nix(&WORKER_MAGIC_1)?;
        self.tx_to_daemon.inner.flush()?;
        let magic: u64 = self.rx_from_daemon.inner.read_nix()?;
        if magic != WORKER_MAGIC_2 {
            Err(anyhow!("unexpected WORKER_MAGIC_2: got {magic:x}"))?;
        }
        let protocol_version: u64 = self.rx_from_daemon.inner.read_nix()?;
        if protocol_version < PROTOCOL_VERSION.into() {
            Err(anyhow!(
                "unexpected protocol version: got {protocol_version}"
            ))?;
        }
        let protocol_version: u64 = PROTOCOL_VERSION.into();
        self.tx_to_daemon.inner.write_nix(&protocol_version)?;
        self.tx_to_daemon.inner.write_nix(&0u64)?; // cpu affinity, obsolete
        self.tx_to_daemon.inner.write_nix(&0u64)?; // reserve space, obsolete
        self.tx_to_daemon.inner.flush()?;
        let proxy_daemon_version: NixString = self.rx_from_daemon.inner.read_nix()?;
        eprintln!(
            "Proxy daemon is: {}",
            String::from_utf8_lossy(proxy_daemon_version.0.as_ref())
        );
        loop {
            let err_msg = self.read_error_msg().unwrap();
            if err_msg == Msg::Last(()) {
                // we are done
                break;
            }
        }
        Ok(())
    }

    pub fn streaming_write_buff(&mut self, buf: &[u8], chunk_len: usize) -> Result<()> {
        let ser = crate::serialize::NixSerializer {
            write: &mut self.tx_to_daemon.inner,
        };
        ser.write.write_all(&buf[..chunk_len])?;
        Ok(())
    }

    pub fn streaming_write_len(&mut self, len: u64) -> Result<()> {
        let mut ser = crate::serialize::NixSerializer {
            write: &mut self.tx_to_daemon.inner,
        };
        len.serialize(&mut ser)?;
        Ok(())
    }

    pub fn send_worker_op_to_daemon(&mut self, worker_op: &WorkerOp) -> Result<()> {
        self.tx_op_count += 1;
        self.tx_to_daemon.inner.write_nix(&worker_op)?;
        self.tx_to_daemon.inner.flush()?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.tx_to_daemon.flush()?;
        Ok(())
    }

    pub fn read_error_msg(&mut self) -> Result<stderr::Msg> {
        let msg: stderr::Msg = self.rx_from_daemon.inner.read_nix()?;
        Ok(msg)
    }

    pub fn read_build_response_from_daemon<T>(&mut self, resp: &Resp<T>) -> Result<T>
    where
        T: Debug + for<'a> serde::Deserialize<'a>,
    {
        let mut deser: NixDeserializer = NixDeserializer {
            read: &mut self.rx_from_daemon.inner,
        };

        let reply: T = resp.ty(T::deserialize(&mut deser)?);
        Ok(reply)
    }

    pub fn reader(&mut self) -> &mut R {
        &mut self.rx_from_daemon.inner
    }

    pub fn writer(&mut self) -> &mut W {
        &mut self.tx_to_daemon.inner
    }
}

impl<R: BufRead, W: Write> NixDaemonClient<R, W> {
    /// Reads and returns the next `stderr` message from the nix daemon.
    ///
    /// Returns `Ok(None)` if the daemon has already closed the stream. (If the
    /// daemon closes the stream mid-message, that's an error.)
    pub fn read_error_msg_or_eof(&mut self) -> Result<Option<stderr::Msg>> {
        if self.rx_from_daemon.inner.fill_buf()?.is_empty() {
            Ok(None)
        } else {
            let msg: stderr::Msg = self.rx_from_daemon.inner.read_nix()?;
            Ok(Some(msg))
        }
    }
}
