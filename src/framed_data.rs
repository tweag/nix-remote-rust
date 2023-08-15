use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::io::{Read, Write};

use crate::Result;

/// Nix has `FramedSource` and `FramedSink` for streaming large amounts of miscellaneous
/// data. They represent lists of byte buffers, and the wire format is:
/// - each byte buffer is represented as a length followed by a buffer of that length.
///   The buffer is *NOT* padded, unlike everything else in this protocol.
/// - the list is terminated by an empty buffer (which is represented on the wire as
///   a length of zero, followed by nothing).
///
/// The whole point of this is that it is big enough that we don't want to hold it in
/// memory all at once. But as a stop-gap, that's what we'll do here.
#[derive(Clone, Default)]
pub struct FramedData {
    pub data: Vec<ByteBuf>,
}

impl std::fmt::Debug for FramedData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FramedData").finish_non_exhaustive()
    }
}

impl FramedData {
    pub fn read(mut r: impl Read) -> Result<FramedData> {
        let mut de = crate::serialize::NixDeserializer { read: &mut r };

        let mut ret = FramedData::default();
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
        let mut ser = crate::serialize::NixSerializer { write: &mut w };

        for data in &self.data {
            (data.len() as u64).serialize(&mut ser)?;
            ser.write.write_all(data)?;
        }
        0_u64.serialize(&mut ser)?;
        Ok(())
    }
}
