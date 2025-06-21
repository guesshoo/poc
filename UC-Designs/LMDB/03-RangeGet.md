## Supporting Range Gets
LMDB itself doesn’t expose a single “range\_get” call, but since it stores keys in **lexicographic order**, you can do an efficient *prefix scan* with a cursor. Here’s the pattern in Rust:

```rust
use lmdb::{Environment, Database, RoCursor, Cursor, Transaction};

fn scan_prefix(
    env: &Environment,
    db: Database,
    prefix: &[u8],
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, lmdb::Error> {
    let txn = env.begin_ro_txn()?;
    let mut cursor = txn.open_ro_cursor(db)?;
    let mut result = Vec::new();

    // Position cursor at the first key ≥ prefix
    for (key, value) in cursor.iter_from(prefix) {
        if !key.starts_with(prefix) {
            break;
        }
        result.push((key.to_vec(), value.to_vec()));
    }

    // cursor and txn drop here
    Ok(result)
}
```

### Explanation

1. **`iter_from(prefix)`**
   Internally calls `mdb_cursor_get(..., MDB_SET_RANGE)` which seeks to the first key ≥ `prefix`.
2. **Lexicographic ordering**
   Because `"user::noel::address"` and `"user::noel::moreInfo"` share the common prefix `"user::noel::"`, they’ll be adjacent in the key order.
3. **Stopping condition**
   Once the cursor yields a key that *doesn’t* start with your prefix, you can safely break out—no further keys will match.

### Complete example in context

```rust
use lmdb::{Environment, DatabaseFlags};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = Environment::new()
        .set_max_dbs(1)
        .open("mydb")?;
    let db = env.create_db(Some("kv"), DatabaseFlags::empty())?;

    // insert two keys
    {
        let mut wtxn = env.begin_rw_txn()?;
        wtxn.put(db, b"user::noel::address", b"123 Maple St.", Default::default())?;
        wtxn.put(db, b"user::noel::moreInfo", b"Likes Rust", Default::default())?;
        wtxn.commit()?;
    }

    // scan them back out
    let found = scan_prefix(&env, db, b"user::noel::")?;
    for (k, v) in found {
        println!("{} = {}", String::from_utf8_lossy(&k), String::from_utf8_lossy(&v));
    }

    Ok(())
}
```

This will print:

```
user::noel::address = 123 Maple St.
user::noel::moreInfo = Likes Rust
```

so you can efficiently “range‐get” any lexicographic slice of your keyspace.
