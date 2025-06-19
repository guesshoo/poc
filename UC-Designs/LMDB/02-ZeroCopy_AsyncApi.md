Below is a complete **`Storage` trait** that embeds our zero-copy async API, plus an LMDB-backed implementation and two example closures that parse FlatBuffers directly from the mmap’d bytes.

```rust
use async_trait::async_trait;
use lmdb::{Environment, Database};
use std::{path::Path, sync::Arc};
use tokio::task;

// Error type (same as before)
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("LMDB error: {0}")]
    Internal(String),
    #[error("Key not found")]
    NotFound,
}

// Our Storage trait now has three methods:
// 1) `set_async`: async write
// 2) `get_async`: async read that returns an owned Vec<u8>
// 3) `with_async`: zero-copy read via a user closure
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    async fn set_async(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), CacheError>;

    async fn get_async(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, CacheError>;

    /// Zero-copy read: run `f` on the raw bytes inside the RO txn.
    /// Returns `Ok(Some(r))` if the key existed, or `Ok(None)` if not.
    async fn with_async<F, R>(
        &self,
        key: Vec<u8>,
        f: F,
    ) -> Result<Option<R>, CacheError>
    where
        F: FnOnce(&[u8]) -> R + Send + 'static,
        R: Send + 'static;
}

// ================================
// LMDBStorage Implementation
// ================================

#[derive(Clone)]
pub struct LMDBStorage {
    env: Arc<Environment>,
    db: Database,
}

impl LMDBStorage {
    pub fn new<P: AsRef<Path>>(path: P, max_dbs: u32) -> Result<Self, CacheError> {
        let env = Environment::new()
            .set_max_dbs(max_dbs)
            .set_map_size(1024 * 1024 * 1024)
            .open(path.as_ref())
            .map_err(|e| CacheError::Internal(e.to_string()))?;
        let db = env
            .create_db(Some("cache"), Default::default())
            .map_err(|e| CacheError::Internal(e.to_string()))?;
        Ok(Self { env: Arc::new(env), db })
    }
}

#[async_trait]
impl Storage for LMDBStorage {
    async fn set_async(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), CacheError> {
        let env = Arc::clone(&self.env);
        let db = self.db;
        task::spawn_blocking(move || {
            let mut wtxn = env.begin_rw_txn()
                .map_err(|e| CacheError::Internal(e.to_string()))?;
            wtxn.put(db, &key, &value, lmdb::WriteFlags::empty())
                .map_err(|e| CacheError::Internal(e.to_string()))?;
            wtxn.commit().map_err(|e| CacheError::Internal(e.to_string()))
        })
        .await
        .map_err(|e| CacheError::Internal(e.to_string()))?
    }

    async fn get_async(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, CacheError> {
        // Just wrap with_async to copy out the Vec<u8>
        self.with_async(key, |data| data.to_vec()).await
    }

    async fn with_async<F, R>(
        &self,
        key: Vec<u8>,
        f: F,
    ) -> Result<Option<R>, CacheError>
    where
        F: FnOnce(&[u8]) -> R + Send + 'static,
        R: Send + 'static,
    {
        let env = Arc::clone(&self.env);
        let db = self.db;
        let key_clone = key.clone();
        task::spawn_blocking(move || {
            let rtxn = env.begin_ro_txn()
                .map_err(|e| CacheError::Internal(e.to_string()))?;
            match rtxn.get(db, &key_clone) {
                Ok(data) => {
                    let out = f(data);
                    Ok(Some(out))
                }
                Err(lmdb::Error::NotFound) => Ok(None),
                Err(e) => Err(CacheError::Internal(e.to_string())),
            }
        })
        .await
        .map_err(|e| CacheError::Internal(e.to_string()))?
    }
}
```

---

## 📦 Example Closure 1: Deserialize a FlatBuffer "User" Record

Assume you have a schema:

```fbs
table User {
  id:ulong;
  name:string;
}
root_type User;
```

And generated Rust code in `fb_generated::user`:

```rust
use flatbuffers::get_root;
use fb_generated::user::User as FBUser;

// ...
let user_opt: Option<FBUser> = storage
    .with_async(key_bytes, |bytes| {
        // zero-copy parse from the mmap’d slice
        let user_fb = get_root::<FBUser>(bytes);
        user_fb.clone() // or extract fields as you like
    })
    .await?;
```

Or to map into your domain struct:

```rust
#[derive(Debug)]
struct User {
    id: u64,
    name: String,
}

let user_struct_opt: Option<User> = storage
    .with_async(key_bytes, |bytes| {
        let fb = get_root::<FBUser>(bytes);
        User {
            id: fb.id(),
            name: fb.name().unwrap_or("").to_string(),
        }
    })
    .await?;
```

---

## 📦 Example Closure 2: Read a Timestamp Prefix

If you prepend an 8-byte BE timestamp to your value:

```rust
let ts_opt: Option<u64> = storage
    .with_async(key_bytes, |bytes| {
        // bytes[0..8] is our big-endian timestamp
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&bytes[0..8]);
        u64::from_be_bytes(arr)
    })
    .await?;
```

---

With this setup you get:

* **Async-friendly** API (`async fn …`).
* **Zero-copy reads** inside `with_async`.
* **Flexible parsing** of arbitrary on-disk formats (FlatBuffers, custom headers, etc.).

Let me know if you’d like more examples—or integration tips for error handling, batching, or eviction metadata!
