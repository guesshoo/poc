To create a live, consistent snapshot of your LMDB cache in Rust, you can lean on LMDB’s built-in “copy” APIs—which use an internal read-only transaction to grab a point-in-time view of the data—and (optionally) request compaction to strip out unused pages:

```rust
use liblmdb::{mdb_env_copy2, MDB_env, MDB_CP_COMPACT};
use std::ffi::CString;

/// Writes a compacted snapshot of the environment to `snapshot_dir`.
/// This omits free pages and renumbers the rest, so the output file
/// is roughly “just the bytes you’ve actually written” plus LMDB overhead.
fn create_compacted_snapshot(env: &Environment, snapshot_dir: &str) -> Result<(), lmdb::Error> {
    let c_path = CString::new(snapshot_dir).unwrap();
    // MDB_CP_COMPACT -> drop unused pages, renumber all pages
    let flags = MDB_CP_COMPACT;
    let rc = unsafe { mdb_env_copy2(env.env(), c_path.as_ptr(), flags) };
    if rc != 0 {
        return Err(lmdb::Error::from(rc));
    }
    Ok(())
}
```

* This calls the C API `mdb_env_copy2` under the hood, with `MDB_CP_COMPACT` to drop free pages and sequentially renumber all pages in the output ([docs.rs][1], [lmdb.tech][2]).
* If you omit the `MDB_CP_COMPACT` flag (i.e. pass `flags = 0`), you get a raw backup that’s essentially a byte-for-byte copy of your current data file ([docs.rs][1]).

---

### Will that snapshot be “small enough” for Raft?

* **On Linux**, LMDB files are typically **sparse**: although you set a large `map_size` (say 4 GB), the on-disk file only occupies as many bytes as you’ve actually written. Tools like `du` will report the real used size, not the full map size ([superuser.com][3]).
* **On Windows**, however, LMDB preallocates the entire `map_size` immediately, so without compaction you’d be copying a giant file even if most of it is empty ([github.com][4]).

By using `MDB_CP_COMPACT`, the copy process **collapses** the file down to only the pages that hold real data (plus a handful of metadata pages). In other words, your snapshot will be roughly the same size as your live data footprint, making it far more practical to ship as a Raft snapshot—so long as your data set remains in the low-hundreds-of-megabytes (or low single-digit gigabytes, if your network can handle it). If you push into tens of gigabytes, you may want to:

1. **Compress** the snapshot stream (gzip, brotli…).
2. **Chunk** it out and stream incrementally.
3. **Tune** your `map_size` to match expected usage, so you never over-allocate huge empty files.

But for modestly-sized LMDB caches, a compact snapshot is both trivial to produce in Rust and small enough to send to follower nodes as part of your Raft install-snapshot procedure.

[1]: https://docs.rs/liblmdb/latest/liblmdb/fn.mdb_env_copy2.html "mdb_env_copy2 in liblmdb - Rust"
[2]: https://www.lmdb.tech/doc/group__mdb__copy.html?utm_source=chatgpt.com "LMDB: Copy Flags"
[3]: https://superuser.com/questions/970644/remote-copy-of-lmdb?utm_source=chatgpt.com "linux - Remote copy of LMDB - Super User"
[4]: https://github.com/erigontech/erigon/issues/1134?utm_source=chatgpt.com "Lmdb size on Windows · Issue #1134 · erigontech/erigon - GitHub"


When a follower gets an LMDB snapshot (as the payload of Raft’s InstallSnapshot RPC), its job is to replace its local database with that snapshot and reset its log-and-state so that it “starts” from the snapshot’s last included index.  Roughly, you:

1. **Receive the snapshot metadata + bytes**
   The RPC carries both:

   ```rust
   struct InstallSnapshotRequest {
     term: u64,
     leader_id: u64,
     snapshot: Snapshot,    // metadata.index, metadata.term, data: Vec<u8>
   }
   ```

2. **Write the bytes to a temp file**
   Don’t overwrite your live DB in place until you’ve fully written and fsynced the new file:

   ```rust
   let tmp_path = env_path.join("data.mdb.tmp");
   {
     let mut f = File::create(&tmp_path)?;
     f.write_all(&req.snapshot.data)?;
     f.sync_all()?;
   }
   ```

3. **Atomically swap in the snapshot**
   Close your current LMDB environment, rename the temp file over the old one, then re-open.  This ensures any readers crash if they try to use it mid-swap, but on reopen you get the new state:

   ```rust
   // 1. Close existing handles
   drop(env);  

   // 2. Replace the file
   fs::rename(&tmp_path, env_path.join("data.mdb"))?;

   // 3. Re-open env pointing at the same path
   let env = Environment::new()
                  .set_max_dbs(1)
                  .open(&env_path)?;
   ```

4. **Update your Raft storage state**
   Now that your on-disk DB reflects “up through” `snapshot.metadata.index`, you must:

   * **Discard** any in-memory or on-disk log entries with index ≤ `last_included_index`.
   * **Persist** the new “last applied” index/term in your stable state (so on restart you don’t re-apply).
   * **Reset** your commit index and last applied index to `last_included_index`.

   In code, if you’re using the `raft` crate’s `MemStorage`-style API, it looks like:

   ```rust
   storage.apply_snapshot(req.snapshot.clone())?;
   ```

   where `apply_snapshot` does exactly the file-swap above and then:

   ```rust
   self.hard_state.set_commit(snapshot.metadata.index);
   self.entries.retain(|e| e.index > snapshot.metadata.index);
   ```

5. **Resume normal replication**
   After InstallSnapshot returns success, the leader will send you AppendEntries for any entries **after** `last_included_index`.  You open a write-transaction on the newly opened LMDB env, apply any further cache updates, and continue as a normal Raft follower.

---

### Why this works

* **Atomic swap** guarantees you either use the old DB or the new one—never a half-written file.
* **Dropping and re-opening** the LMDB `Environment` forces LMDB to re-read its meta pages and page tables from the snapshot.
* **Log truncation** in the Raft storage ensures you don’t try to replay old entries you’ve already “included” in the snapshot.

With that in place, your follower’s cache is a byte-for-byte match of the leader’s at the snapshot point—and future log entries will be appended on top of it exactly as if you’d been streaming them all along.
