# Sample code 

```toml
# Cargo.toml

[dependencies]
tokio = { version = "1", features = ["full"] }
lmdb = "0.8"
anyhow = "1"
```


```rust
//! Cargo.toml dependencies:
//! 
//! [dependencies]
//! tokio = { version = "1", features = ["full"] }
//! lmdb = "0.9"
//! anyhow = "1"

use lmdb::{Environment, Database, WriteFlags, Transaction};
use tokio;
use tokio::{fs, fs::OpenOptions, io::AsyncWriteExt, task};
use tokio::sync::Mutex;
use std::{sync::Arc, path::Path};
use anyhow::Result;

/// A simple async cache service with WAL and LMDB-backed storage.
pub struct CacheService {
    env: Arc<Environment>,
    data_db: Database,
    meta_db: Database,
    wal_path: String,
    wal: Arc<Mutex<tokio::fs::File>>,
}

impl CacheService {
    /// Open or create the LMDB environment directory and WAL file.
    pub async fn new<P: AsRef<Path>>(db_path: P, wal_path: P) -> Result<Self> {
        let db_dir = db_path.as_ref();
        // Ensure the LMDB directory exists
        fs::create_dir_all(db_dir).await?;

        let env = Environment::new()
            .set_max_dbs(2)
            .open(db_dir)?;
        let data_db = env.create_db(Some("data"), lmdb::DatabaseFlags::empty())?;
        let meta_db = env.create_db(Some("meta"), lmdb::DatabaseFlags::empty())?;

        let wal_path_str = wal_path.as_ref().to_string_lossy().into_owned();
        // Ensure parent directory for WAL
        // if let Some(parent) = Path::new(&wal_path_str).parent() {
        //     println!("creating directory : {:?}", parent);
        //     fs::create_dir_all(parent).await.ok();
        // }
        let wal_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal_path_str)
            .await?;

        Ok(Self {
            env: Arc::new(env),
            data_db,
            meta_db,
            wal_path: wal_path_str,
            wal: Arc::new(Mutex::new(wal_file)),
        })
    }

    /// Atomically allocate a new, unique sequence number.
    async fn allocate_seq(&self) -> Result<u64> {
        let env = Arc::clone(&self.env);
        let meta_db = self.meta_db;
        task::spawn_blocking(move || {
            let mut txn = env.begin_rw_txn()?;
            let current = txn.get(meta_db, b"next_seq")
                .map(|bytes| {
                    let mut arr = [0u8; 8];
                    arr.copy_from_slice(bytes);
                    u64::from_be_bytes(arr)
                })
                .unwrap_or(0);
            let next = current + 1;
            txn.put(meta_db, b"next_seq", &next.to_be_bytes(), WriteFlags::empty())?;
            txn.commit()?;
            Ok(current)
        }).await?
    }

    /// Append the operation to the WAL and fsync.
    async fn write_wal(&self, seq: u64, key: &[u8], value: &[u8]) -> std::io::Result<()> {
        let mut wal = self.wal.lock().await;
        // Format: <seq>:<key>=<value>\n
        wal.write_all(seq.to_string().as_bytes()).await?;
        wal.write_all(b":").await?;
        wal.write_all(key).await?;
        wal.write_all(b"=").await?;
        wal.write_all(value).await?;
        wal.write_all(b"\n").await?;
        wal.sync_data().await
    }

    /// Apply the write to LMDB and update last_applied in one transaction.
    async fn apply_store(&self, seq: u64, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let env = Arc::clone(&self.env);
        let data_db = self.data_db;
        let meta_db = self.meta_db;
        task::spawn_blocking(move || {
            let mut txn = env.begin_rw_txn()?;
            txn.put(data_db, &key, &value, WriteFlags::empty())?;
            txn.put(meta_db, b"last_applied", &seq.to_be_bytes(), WriteFlags::empty())?;
            txn.commit()?;
            Ok(())
        }).await?
    }

    /// Public API: write a key/value pair, returning its sequence.
    pub async fn put(&self, key: Vec<u8>, value: Vec<u8>) -> Result<u64> {
        let seq = self.allocate_seq().await?;
        self.write_wal(seq, &key, &value).await?;
        self.apply_store(seq, key, value).await?;
        Ok(seq)
    }

    /// Recover from WAL by replaying entries beyond last_applied.
    pub async fn recover(&self) -> Result<()> {
        // Fetch last applied sequence
        let last_applied: Result<u64> = {
            let env = Arc::clone(&self.env);
            let meta_db = self.meta_db;
            task::spawn_blocking(move || {
                let txn = env.begin_ro_txn()?;
                let val = txn.get(meta_db, b"last_applied")
                    .map(|bytes| {
                        let mut arr = [0u8; 8];
                        arr.copy_from_slice(bytes);
                        u64::from_be_bytes(arr)
                    })
                    .unwrap_or(0);
                Ok(val)
            }).await?
        };
        let last = last_applied?;



        // Read WAL file, skip if missing
        let content = match fs::read_to_string(&self.wal_path).await {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        for line in content.lines() {
            let mut parts = line.splitn(2, ':');
            let seq = parts.next().unwrap().parse::<u64>().unwrap();
            if seq <= last { continue; }
            let kv = parts.next().unwrap();
            let mut kv_iter = kv.splitn(2, '=');
            let key = kv_iter.next().unwrap().as_bytes().to_vec();
            let value = kv_iter.next().unwrap().as_bytes().to_vec();
            self.apply_store(seq, key, value).await?;
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let service = CacheService::new("cache.lmdb", "wal.log").await?;
    service.recover().await?;

    let seq = service.put(b"foo".to_vec(), b"bar".to_vec()).await?;
    println!("Inserted key=foo at seq={}", seq);
    Ok(())
}

```
```rust
//! Cargo.toml dependencies:
//! 
//! [dependencies]
//! tokio = { version = "1", features = ["full"] }
//! lmdb = "0.9"
//! anyhow = "1"

use lmdb::{Environment, Database, WriteFlags, Transaction};
use tokio::{fs, fs::OpenOptions, io::AsyncWriteExt, task};
use tokio::sync::Mutex;
use std::{sync::Arc, path::Path};
use anyhow::Result;

/// A simple async cache service with WAL and LMDB-backed storage.
pub struct CacheService {
    env: Arc<Environment>,
    data_db: Database,
    meta_db: Database,
    wal_path: String,
    wal: Arc<Mutex<tokio::fs::File>>,
}

impl CacheService {
    /// Open or create the LMDB environment directory and WAL file.
    pub async fn new<P: AsRef<Path>>(db_path: P, wal_path: P) -> Result<Self> {
        let db_dir = db_path.as_ref();
        // Ensure the LMDB directory exists
        fs::create_dir_all(db_dir).await?;

        let env = Environment::new()
            .set_max_dbs(2)
            .open(db_dir)?;
        let data_db = env.create_db(Some("data"), lmdb::DatabaseFlags::empty())?;
        let meta_db = env.create_db(Some("meta"), lmdb::DatabaseFlags::empty())?;

        let wal_path_str = wal_path.as_ref().to_string_lossy().into_owned();
        // Ensure parent directory for WAL
        // if let Some(parent) = Path::new(&wal_path_str).parent() {
        //     println!("creating directory : {:?}", parent);
        //     fs::create_dir_all(parent).await.ok();
        // }
        let wal_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal_path_str)
            .await?;

        Ok(Self {
            env: Arc::new(env),
            data_db,
            meta_db,
            wal_path: wal_path_str,
            wal: Arc::new(Mutex::new(wal_file)),
        })
    }

    /// Atomically allocate a new, unique sequence number.
    async fn allocate_seq(&self) -> Result<u64> {
        let env = Arc::clone(&self.env);
        let meta_db = self.meta_db;
        task::spawn_blocking(move || {
            let mut txn = env.begin_rw_txn()?;
            let current = txn.get(meta_db, b"next_seq")
                .map(|bytes| {
                    let mut arr = [0u8; 8];
                    arr.copy_from_slice(bytes);
                    u64::from_be_bytes(arr)
                })
                .unwrap_or(0);
            let next = current + 1;
            txn.put(meta_db, b"next_seq", &next.to_be_bytes(), WriteFlags::empty())?;
            txn.commit()?;
            Ok(current)
        }).await?
    }

    /// Append the operation to the WAL and fsync.
    async fn write_wal(&self, seq: u64, key: &[u8], value: &[u8]) -> std::io::Result<()> {
        let mut wal = self.wal.lock().await;
        // Format: <seq>:<key>=<value>\n
        wal.write_all(seq.to_string().as_bytes()).await?;
        wal.write_all(b":").await?;
        wal.write_all(key).await?;
        wal.write_all(b"=").await?;
        wal.write_all(value).await?;
        wal.write_all(b"\n").await?;
        wal.sync_data().await
    }

    /// Apply the write to LMDB and update last_applied in one transaction.
    async fn apply_store(&self, seq: u64, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let env = Arc::clone(&self.env);
        let data_db = self.data_db;
        let meta_db = self.meta_db;
        task::spawn_blocking(move || {
            let mut txn = env.begin_rw_txn()?;
            txn.put(data_db, &key, &value, WriteFlags::empty())?;
            txn.put(meta_db, b"last_applied", &seq.to_be_bytes(), WriteFlags::empty())?;
            txn.commit()?;
            Ok(())
        }).await?
    }

    /// Public API: write a key/value pair, returning its sequence.
    pub async fn put(&self, key: Vec<u8>, value: Vec<u8>) -> Result<u64> {
        let seq = self.allocate_seq().await?;
        self.write_wal(seq, &key, &value).await?;
        self.apply_store(seq, key, value).await?;
        Ok(seq)
    }

    /// Recover from WAL by replaying entries beyond last_applied.
    pub async fn recover(&self) -> Result<()> {
        // Fetch last applied sequence
        let last_applied: Result<u64> = {
            let env = Arc::clone(&self.env);
            let meta_db = self.meta_db;
            task::spawn_blocking(move || {
                let txn = env.begin_ro_txn()?;
                let val = txn.get(meta_db, b"last_applied")
                    .map(|bytes| {
                        let mut arr = [0u8; 8];
                        arr.copy_from_slice(bytes);
                        u64::from_be_bytes(arr)
                    })
                    .unwrap_or(0);
                Ok(val)
            }).await?
        };
        let last = last_applied?;



        // Read WAL file, skip if missing
        let content = match fs::read_to_string(&self.wal_path).await {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        for line in content.lines() {
            let mut parts = line.splitn(2, ':');
            let seq = parts.next().unwrap().parse::<u64>().unwrap();
            if seq <= last { continue; }
            let kv = parts.next().unwrap();
            let mut kv_iter = kv.splitn(2, '=');
            let key = kv_iter.next().unwrap().as_bytes().to_vec();
            let value = kv_iter.next().unwrap().as_bytes().to_vec();
            self.apply_store(seq, key, value).await?;
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let service = CacheService::new("cache.lmdb", "wal.log").await?;
    service.recover().await?;

    let seq = service.put(b"foo".to_vec(), b"bar".to_vec()).await?;
    println!("Inserted key=foo at seq={}", seq);
    Ok(())
}


```