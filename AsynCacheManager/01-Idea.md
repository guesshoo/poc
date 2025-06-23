You can think of “turn” (your global, ever‐increasing write number) and “index” (the point up to which you’ve safely applied into your on-disk store) as two sides of the same coin—and the single source of truth for both should live in your WAL layer’s sequencer.  Here’s a sketch of how it hangs together:

---

### 1. A metadata sub-database in your storage engine

Whether you pick LMDB or sled, carve out a tiny “meta” database (or tree) alongside your data.  In it you’ll keep at least two keys:

* **next\_seq**:  the next sequence number to hand out
* **last\_applied**: the highest sequence you’ve ever durably folded into your data store

Because both LMDB and sled let you do *atomic* get-and-set or CAS, you can treat `next_seq` as your on-disk counter.

---

### 2. The single “LogManager” (or Raft leader) is your sequencer

All writers go through one module whose job is:

1. **Allocate**

   ```rust
   // LMDB-style pseudocode
   let seq = txn.get(meta_db, b"next_seq")? as u64;
   txn.put(meta_db, b"next_seq", &(seq+1).to_be_bytes(), 0)?;
   // commit just the bump to disk (or sled’s CAS+flush)
   txn.commit()?;
   ```

   Now you know your unique sequence `seq`.

2. **Write-Ahead**
   Append `(seq, op)` to your WAL file, fsync the file.

3. **Apply to Store**
   Open a fresh LMDB/sled transaction and:

   * Write your actual key–value change into the data DB.
   * Atomically `put(meta_db, b"last_applied", &seq.to_be_bytes())` in the same txn.
   * Commit & sync.

Because step 2 always happens *before* step 3, and step 3’s metadata bump is in the *same* storage-engine txn as your data write, you can always recover by:

* Reading `last_applied` from meta on startup.
* Replay WAL entries with `seq > last_applied`.

---

### 3. Cluster-wide consistency via Raft (optional)

If you’re truly distributed, wrap the above in a Raft group:

* **Leader** assigns the Raft log index → that index *is* your `seq`.
* It replicates the `(op)` payload under that index to followers.
* Each node, on “apply(index, op)”, uses the exact same recipe above (with its local LMDB/sled): apply data + bump `last_applied=index`.

Raft gives you a totally consistent, cluster-wide “turn” generator, and each node’s metadata table tracks exactly how far it’s gotten in the log.

---

#### Why this works

* **Single source of truth** for sequencing lives in the LogManager (or Raft leader).
* **Atomic on-disk bump** in your meta-DB means you never “lose” a turn.
* **Crash-safe replay** via `last_applied` + WAL scan.

With just one tiny metadata table and a little bit of transaction choreography, your WAL and your LMDB/sled store stay in perfect lock-step.
