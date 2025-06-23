// src/lib.rs

// --- Cargo.toml dependencies ---
// [dependencies]
// serde       = { version = "1.0", features = ["derive"] }
// serde_cbor  = "0.11"
// tempfile    = "3"

use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    io::{self, BufReader, BufWriter, Read, Write},
    path::Path,
};

/// 4-byte magic at the start of each record, for quick corruption checks.
pub const FILE_IDENTIFIER: [u8; 4] = *b"WALR";

/// Union of possible operations stored in the WAL.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "type", content = "payload")]
pub enum Command {
    /// Put a key/value pair.
    Set(SetCommand),
    /// Remove a key.
    Delete(DeleteCommand),
    /// Install a snapshot.
    Snapshot(SnapshotCommand),
}

/// A single WAL entry: Raft term & index + an application command.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct WalRecord {
    pub term: u64,
    pub index: u64,
    pub op: Command,
}

/// Command to set a key to a value.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct SetCommand {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

/// Command to delete a key.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct DeleteCommand {
    pub key: Vec<u8>,
}

/// Snapshot install command: term, index, checksum, and raw snapshot data.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct SnapshotCommand {
    pub snapshot_term: u64,
    pub snapshot_index: u64,
    pub checksum: u64,
    pub data: Vec<u8>,
}

/// WAL writer: appends records to a file on disk.
/// Each record is written as:
///   [4-byte magic][8-byte BE length][CBOR-encoded payload]
pub struct WalWriter {
    inner: BufWriter<std::fs::File>,
}

impl WalWriter {
    /// Open (or create) the WAL file in append mode.
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(WalWriter {
            inner: BufWriter::new(file),
        })
    }

    /// Serialize & append a single record, then flush.
    pub fn append(&mut self, record: &WalRecord) -> io::Result<()> {
        // 1) Serialize to CBOR
        let payload = serde_cbor::to_vec(record)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let len = payload.len() as u64;

        // 2) Write magic
        self.inner.write_all(&FILE_IDENTIFIER)?;
        // 3) Write length prefix (big-endian u64)
        self.inner.write_all(&len.to_be_bytes())?;
        // 4) Write payload
        self.inner.write_all(&payload)?;
        // 5) Ensure it's on disk
        self.inner.flush()?;
        Ok(())
    }
}

/// WAL reader: sequentially reads records from the file.
/// On EOF, `next()` returns `Ok(None)`. On corruption, returns `Err`.
pub struct WalReader {
    inner: BufReader<std::fs::File>,
}

impl WalReader {
    /// Open an existing WAL file for reading.
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = std::fs::File::open(path)?;
        Ok(WalReader {
            inner: BufReader::new(file),
        })
    }

    /// Read the next record. Returns `Ok(None)` on clean EOF.
    pub fn next(&mut self) -> io::Result<Option<WalRecord>> {
        // 1) Read magic
        let mut magic = [0u8; 4];
        match self.inner.read_exact(&mut magic) {
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
            Ok(_) => {}
        }
        if magic != FILE_IDENTIFIER {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid magic: expected {:?}, got {:?}",
                    FILE_IDENTIFIER, magic
                ),
            ));
        }

        // 2) Read length prefix
        let mut len_buf = [0u8; 8];
        self.inner.read_exact(&mut len_buf)?;
        let len = u64::from_be_bytes(len_buf);

        // 3) Read CBOR payload
        let mut payload = vec![0u8; len as usize];
        self.inner.read_exact(&mut payload)?;

        // 4) Deserialize
        let record: WalRecord = serde_cbor::from_slice(&payload)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Some(record))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    /// Helper: build a sample Set record.
    fn make_set(term: u64, index: u64, k: &[u8], v: &[u8]) -> WalRecord {
        WalRecord {
            term,
            index,
            op: Command::Set(SetCommand {
                key: k.to_vec(),
                value: v.to_vec(),
            }),
        }
    }

    /// Helper: build a sample Delete record.
    fn make_delete(term: u64, index: u64, k: &[u8]) -> WalRecord {
        WalRecord {
            term,
            index,
            op: Command::Delete(DeleteCommand {
                key: k.to_vec(),
            }),
        }
    }

    #[test]
    fn round_trip_cbor_records() -> io::Result<()> {
        // Create a temp file
        let temp = NamedTempFile::new()?;
        let path = temp.path();

        // Prepare some records
        let records = vec![
            make_set(1, 1, b"foo", b"bar"),
            make_delete(1, 2, b"foo"),
            WalRecord {
                term: 2,
                index: 3,
                op: Command::Snapshot(SnapshotCommand {
                    snapshot_term: 2,
                    snapshot_index: 3,
                    checksum: 0xDEADBEEF,
                    data: vec![1, 2, 3, 4, 5],
                }),
            },
        ];

        // Append them
        {
            let mut writer = WalWriter::new(path)?;
            for rec in &records {
                writer.append(rec)?;
            }
        }

        // Read them back
        let mut reader = WalReader::new(path)?;
        for expected in records.iter() {
            let got = reader
                .next()?
                .expect("expected another WalRecord");
            assert_eq!(&got, expected, "record mismatch");
        }

        // Ensure EOF
        assert!(reader.next()?.is_none(), "expected no more records");
        Ok(())
    }

    #[test]
    fn corrupt_magic_detected() -> io::Result<()> {
        // Write a bad file (wrong magic)
        let temp = NamedTempFile::new()?;
        std::fs::write(temp.path(), b"BAD!........")?;

        let mut reader = WalReader::new(temp.path())?;
        let err = reader.next().unwrap_err();
        assert!(matches!(err.kind(), io::ErrorKind::InvalidData));
        Ok(())
    }
}
