use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::io::{Read, Write};

use crate::Result;

#[derive(Clone, Default)]
pub struct FramedSource {
    data: Vec<ByteBuf>,
}

impl std::fmt::Debug for FramedSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FramedData").finish_non_exhaustive()
    }
}

impl FramedSource {
    pub fn read(mut r: impl Read) -> Result<FramedSource> {
        let mut de = crate::serialize::Deserializer { read: &mut r };

        let mut ret = FramedSource::default();
        loop {
            let len = u64::deserialize(&mut de)?;
            if len == 0 {
                break;
            }
            let mut buf = vec![0; len as usize];
            de.read.read_exact(&mut buf)?;
            // NOTE: the buffers in framed data are *not* padded.
            ret.data.push(ByteBuf::from(buf));
        }
        Ok(ret)
    }

    pub fn write(&self, mut w: impl Write) -> Result<()> {
        let mut ser = crate::serialize::Serializer { write: &mut w };

        for data in &self.data {
            (data.len() as u64).serialize(&mut ser)?;
            ser.write.write_all(data)?;
        }
        (0 as u64).serialize(&mut ser)?;
        Ok(())
    }
}
