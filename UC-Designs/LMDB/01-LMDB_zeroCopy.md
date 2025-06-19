## How to do true zero-copy with LMDB

You must

1. **Avoid** copying the slice out of the txn.
2. **Keep** the read-only transaction alive for as long as you need the slice.
3. Expose the borrowed slice (or wrap it in a small guard that owns the txn).

Here’s a sketch:

```rust
use lmdb::{Environment, Database, RoTransaction};
use std::path::Path;
use std::sync::Arc;

// Holds the txn so the data slice stays valid
pub struct BorrowedValue<'txn> {
    _txn: RoTransaction<'txn>,
    pub data: &'txn [u8],
}

#[derive(Clone)]
pub struct LMDBStorage {
    env: Arc<Environment>,
    db: Database,
}

impl LMDBStorage {
    pub fn new<P: AsRef<Path>>(path: P, max_dbs: u32) -> Result<Self, CacheError> {
        let env = Environment::new()
            .set_max_dbs(max_dbs)
            .open(path.as_ref())
            .map_err(|e| CacheError::Internal(e.to_string()))?;
        let db = env.create_db(None, Default::default())
                    .map_err(|e| CacheError::Internal(e.to_string()))?;
        Ok(Self { env: Arc::new(env), db })
    }

    /// Zero-copy get: returns a guard that holds the txn alive.
    pub fn get_zero_copy<'env>(
        &'env self,
        key: &[u8],
    ) -> Result<Option<BorrowedValue<'env>>, CacheError> {
        let txn = self.env.begin_ro_txn()
            .map_err(|e| CacheError::Internal(e.to_string()))?;
        match txn.get(self.db, key) {
            Ok(data) => Ok(Some(BorrowedValue { _txn: txn, data })),
            Err(lmdb::Error::NotFound) => Ok(None),
            Err(e) => Err(CacheError::Internal(e.to_string())),
        }
    }
}
```

* **No `.to_vec()`**: you get `&[u8]` that lives inside the guard.
* **`BorrowedValue`** owns the `RoTransaction`, so the memory stays mapped (and locked) until you drop it.
* This is fully zero-copy, but **synchronous**: you can’t easily turn it into an `async fn` because the borrow must outlive the `await`.

---

### Trade-offs

* 🔒 **Safety**: You must ensure the `BorrowedValue` isn’t used after the txn is dropped.
* ⚙️ **API shape**: Your `Storage` trait becomes synchronous or callback-based, so you don’t lose the borrow across `await`.
* 🗂 **Single writer, multi-reader**: LMDB limits how many RO txns you can have; you may need pooling.

If you need an **async** API, you can wrap zero-copy calls in `tokio::task::spawn_blocking` and still return the guard — but you’ll still hold the txn on the spawned thread.

---

Let me know if you’d like a full zero-copy, async-friendly wrapper (e.g. using `spawn_blocking` or a custom callback trait), or if you want to see how this integrates with your `Storage` trait!


Simply returning a bare `&[u8]` won’t work, because the slice you get from LMDB is only valid for the lifetime of the transaction that produced it—and that transaction is dropped when your function returns. Any reference into its memory would become dangling.

You *must* keep the `RoTransaction` alive for as long as you need the data. Wrapping the slice together with its owning transaction in a small guard struct (like our `BorrowedValue`) is the standard pattern:

```rust
pub struct BorrowedValue<'txn> {
    _txn: RoTransaction<'txn>,
    pub data: &'txn [u8],
}
```

That way, dropping the `BorrowedValue` (and thus its `_txn`) also correctly unmaps/releases the underlying pages.

---

### Alternatives

1. **Callback API**
   Don’t return the slice directly—take a closure and run it *inside* the transaction lifetime:

   ```rust
   pub fn with_ro_txn<R>(&self, key: &[u8], f: impl FnOnce(&[u8]) -> R) -> Result<Option<R>, CacheError> {
       let txn = self.env.begin_ro_txn()?;
       if let Ok(data) = txn.get(self.db, key) {
           Ok(Some(f(data)))
       } else {
           Ok(None)
       }
   }
   ```

   Here the user-supplied `f` can see the zero-copy bytes, but nothing escapes once the txn is dropped.

2. **Embed a long-lived transaction**
   You could store a single `RoTransaction` inside your `LMDBStorage` and reuse it, allowing you to return `&[u8]` tied to `&self`. But then you lose concurrency (only one RO txn at a time) and you must manage its lifetime carefully.

---

Unless you adopt one of those patterns, you’ll need the guard struct to hold the txn alive.
