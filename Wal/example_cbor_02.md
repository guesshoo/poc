use std::{
    fs::OpenOptions,
    io::{self, BufReader, BufWriter, Read, Write},
    path::Path,
};
use serde::{Serialize, Deserialize};

pub const FILE_IDENTIFIER: [u8; 4] = *b"WALR";

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "type", content = "payload")]
pub enum Command { /* … */ }

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct WalRecord { /* … */ }

pub struct WalWriter {
    inner: BufWriter<std::fs::File>,
}

impl WalWriter {
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let f = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self { inner: BufWriter::new(f) })
    }

    pub fn append(&mut self, record: &WalRecord) -> io::Result<()> {
        // serialize to CBOR
        let payload = serde_cbor::to_vec(record)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let len = payload.len() as u64;

        self.inner.write_all(&FILE_IDENTIFIER)?;
        self.inner.write_all(&len.to_be_bytes())?;
        self.inner.write_all(&payload)?;
        self.inner.flush()?;
        Ok(())
    }
}

pub struct WalReader {
    inner: BufReader<std::fs::File>,
}

impl WalReader {
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let f = std::fs::File::open(path)?;
        Ok(Self { inner: BufReader::new(f) })
    }

    /// Returns `Ok(None)` on clean EOF.
    pub fn next(&mut self) -> io::Result<Option<WalRecord>> {
        let mut magic = [0u8; 4];
        match self.inner.read_exact(&mut magic) {
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
            Ok(_) => {}
        }
        if magic != FILE_IDENTIFIER {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad WAL magic"));
        }

        let mut len_buf = [0u8; 8];
        self.inner.read_exact(&mut len_buf)?;
        let len = u64::from_be_bytes(len_buf);

        let mut payload = vec![0; len as usize];
        self.inner.read_exact(&mut payload)?;
        let record: WalRecord = serde_cbor::from_slice(&payload)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Some(record))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn round_trip_cbor() -> io::Result<()> {
        let tmp = NamedTempFile::new()?;
        let p = tmp.path();

        let recs = vec![
            WalRecord { /* term/index/op = Set("a", "1") */ },
            WalRecord { /* term/index/op = Delete("a") */ },
        ];

        {
            let mut w = WalWriter::new(p)?;
            for r in &recs { w.append(r)?; }
        }

        let mut r = WalReader::new(p)?;
        for expected in &recs {
            let got = r.next()?.expect("missing record");
            assert_eq!(&got, expected);
        }
        assert!(r.next()?.is_none());
        Ok(())
    }
}