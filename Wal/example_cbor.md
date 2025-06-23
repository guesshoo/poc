# Modelling WAL with Rust types and using CBOR for serialization


```rust

/*
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_cbor = "0.11"  
*/


use serde::{Serialize, Deserialize};
use std::fs;
use std::io;

/// 4-byte magic for WAL record validation
pub const FILE_IDENTIFIER: [u8; 4] = *b"WALR";

/// Union of possible operations
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "type", content = "payload")]
pub enum Command {
    Set(SetCommand),
    Delete(DeleteCommand),
    Snapshot(SnapshotCommand),
}

/// WAL record: term, index, and an application command
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct WalRecord {
    pub term: u64,
    pub index: u64,
    pub op: Command,
}

/// Command to set a key to a value
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct SetCommand {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

/// Command to delete a key
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct DeleteCommand {
    pub key: Vec<u8>,
}

/// Snapshot install command with checksum
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct SnapshotCommand {
    pub snapshot_term: u64,
    pub snapshot_index: u64,
    pub checksum: u64,
    pub data: Vec<u8>,
}


/// Serialize a WalRecord to CBOR and write to the given file path
pub fn save_wal_record(path: &str, record: &WalRecord) -> io::Result<()> {
    let bytes = serde_cbor::to_vec(record)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    fs::write(path, bytes)
}

/// Read CBOR data from the given file path and deserialize into a WalRecord
pub fn load_wal_record(path: &str) -> io::Result<WalRecord> {
    let bytes = fs::read(path)?;
    let record = serde_cbor::from_slice(&bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    Ok(record)
}

// Example serialization with CBOR:
// let record = WalRecord { term: 1, index: 42, op: Command::Set(SetCommand { key: vec![1], value: vec![2] }) };
// let bytes = serde_cbor::to_vec(&record)?;
// let decoded: WalRecord = serde_cbor::from_slice(&bytes)?;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cbor_roundtrip_set() {
        let record = WalRecord {
            term: 1,
            index: 42,
            op: Command::Set(SetCommand {
                key: vec![1, 2, 3],
                value: vec![4, 5, 6],
            }),
        };
        let bytes = serde_cbor::to_vec(&record).expect("serialize failed");
        let decoded: WalRecord = serde_cbor::from_slice(&bytes).expect("deserialize failed");
        assert_eq!(decoded, record);
    }

    #[test]
    fn test_cbor_roundtrip_delete() {
        let record = WalRecord {
            term: 2,
            index: 43,
            op: Command::Delete(DeleteCommand {
                key: vec![7, 8, 9],
            }),
        };
        let bytes = serde_cbor::to_vec(&record).expect("serialize failed");
        let decoded: WalRecord = serde_cbor::from_slice(&bytes).expect("deserialize failed");
        assert_eq!(decoded, record);
    }

    #[test]
    fn test_cbor_roundtrip_snapshot() {
        let record = WalRecord {
            term: 3,
            index: 44,
            op: Command::Snapshot(SnapshotCommand {
                snapshot_term: 3,
                snapshot_index: 44,
                checksum: 0xDEADBEEFCAFEBABE,
                data: vec![10, 11, 12],
            }),
        };
        let bytes = serde_cbor::to_vec(&record).expect("serialize failed");
        let decoded: WalRecord = serde_cbor::from_slice(&bytes).expect("deserialize failed");
        assert_eq!(decoded, record);
    }

      #[test]
    fn test_file_roundtrip() {
        let record = WalRecord {
            term: 99,
            index: 100,
            op: Command::Delete(DeleteCommand { key: vec![7, 8, 9] }),
        };
        let path = "test_record.cbor";

        // Save to file
        save_wal_record(path, &record).expect("failed to save");
        // Load from file
        let loaded = load_wal_record(path).expect("failed to load");
        assert_eq!(loaded, record);

        // Clean up
        fs::remove_file(path).expect("failed to remove test file");
    }
}


fn main() {
    println!("hell world");
}
```