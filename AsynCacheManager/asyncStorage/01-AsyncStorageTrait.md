
# Cargo.toml
```toml
[dependencies]
anyhow = "1.0.98"
async-trait = "0.1.88"
bytes = "1.10.1"
dashmap = "6.1.0"
lmdb = "0.8.0"
tempfile = "3.20.0"
thiserror = "2.0.12"
tokio = { version = "1", features = ["rt", "macros", "sync"] }

```


# Code
```rust
use std::{collections::HashMap, path::Path, sync::Arc};
use async_trait::async_trait;
use tokio::sync::RwLock;
use anyhow::Result;
use lmdb::{Environment, Database, WriteFlags, Transaction, RoTransaction, RwTransaction, Error as LmdbError};

/// Async storage trait for byte-keyed values.
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    /// Get the value for `key`, or `None` if absent.
    async fn get(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>>;

    /// Set `value` for `key`.
    async fn set(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()>;

    /// Remove the entry for `key`.
    async fn remove(&self, key: Vec<u8>) -> Result<()>;
}

/// In-memory implementation using RwLock<HashMap>.
pub struct InMemoryStorage {
    inner: Arc<RwLock<HashMap<Vec<u8>, Vec<u8>>>>,
}

impl InMemoryStorage {
    /// Create a new in-memory store.
    pub fn new() -> Self {
        InMemoryStorage { inner: Arc::new(RwLock::new(HashMap::new())) }
    }
}

#[async_trait]
impl Storage for InMemoryStorage {
    async fn get(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>> {
        let map = self.inner.read().await;
        Ok(map.get(&key).cloned())
    }

    async fn set(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let mut map = self.inner.write().await;
        map.insert(key, value);
        Ok(())
    }

    async fn remove(&self, key: Vec<u8>) -> Result<()> {
        let mut map = self.inner.write().await;
        map.remove(&key);
        Ok(())
    }
}

/// LMDB-backed implementation.
pub struct LmdbStorage {
    env: Arc<Environment>,
    db: Database,
}

impl LmdbStorage {
    /// Open (or create) an LMDB environment and database at `path`.
    pub fn new(path: &Path) -> Result<Self> {
        let env = Environment::new()
            .set_max_dbs(1)
            .open(path)?;
        let db = env.create_db(None, lmdb::DatabaseFlags::empty())?;
        Ok(LmdbStorage { env: Arc::new(env), db })
    }
}

#[async_trait]
impl Storage for LmdbStorage {
    async fn get(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>> {
        let env = self.env.clone();
        let db = self.db;
        tokio::task::spawn_blocking(move || {
            let txn = env.begin_ro_txn()?;
            match txn.get(db, &key) {
                Ok(bytes) => Ok(Some(bytes.to_vec())),
                Err(LmdbError::NotFound) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await?
    }

    async fn set(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let env = self.env.clone();
        let db = self.db;
        tokio::task::spawn_blocking(move || {
            let mut wtxn = env.begin_rw_txn()?;
            wtxn.put(db, &key, &value, WriteFlags::empty())?;
            wtxn.commit()?;
            Ok(())
        })
        .await?
    }

    async fn remove(&self, key: Vec<u8>) -> Result<()> {
        let env = self.env.clone();
        let db = self.db;
        tokio::task::spawn_blocking(move || {
            let mut wtxn = env.begin_rw_txn()?;
            // ignore NotFound
            let _ = wtxn.del(db, &key, None);
            wtxn.commit()?;
            Ok(())
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_inmemory_storage() {
        let store = InMemoryStorage::new();
        // Set and get
        store.set(b"foo".to_vec(), b"bar".to_vec()).await.unwrap();
        assert_eq!(store.get(b"foo".to_vec()).await.unwrap(), Some(b"bar".to_vec()));
        // Remove
        store.remove(b"foo".to_vec()).await.unwrap();
        assert_eq!(store.get(b"foo".to_vec()).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_lmdb_storage() {
        let dir = tempdir().unwrap();
        let store = LmdbStorage::new(dir.path()).unwrap();
        // Set and get
        store.set(b"key".to_vec(), b"value".to_vec()).await.unwrap();
        assert_eq!(store.get(b"key".to_vec()).await.unwrap(), Some(b"value".to_vec()));
        // Remove
        store.remove(b"key".to_vec()).await.unwrap();
        assert_eq!(store.get(b"key".to_vec()).await.unwrap(), None);
    }
}

```