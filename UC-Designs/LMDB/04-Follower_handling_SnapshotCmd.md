Here’s a step-by-step of what a follower should do when it receives a `SnapshotCommand` record:

---

## 1. Pre-Installation Checks

1. **Lock out new commands.**
   Acquire your storage/WAL mutex (or otherwise pause the normal log-apply path) so nothing else races with snapshot installation.
2. **Staleness check.**
   If `cmd.snapshot_index <= last_snapshot_index` (i.e. you’ve already installed that snapshot or a newer one), simply ignore the command and resume normal log replication.

---

## 2. Persist the Snapshot Blob

You’ve defined `SnapshotCommand.data` as a raw LMDB snapshot image.  Atomically replace your on-disk environment with it:

```rust
// Pseudocode
let tmp = data_dir.join("env.mdb.tmp");
// 1. Write blob to temp file
std::fs::write(&tmp, &cmd.data)?;
// 2. Ensure it’s on disk
let f = std::fs::OpenOptions::new().write(true).open(&tmp)?;
f.sync_all()?;
// 3. Atomically swap in place
std::fs::rename(&tmp, data_dir.join("env.mdb"))?;
```

* **Why atomic rename?** ensures that on crash you never end up with a half-written DB file.

---

## 3. Reopen / Reinitialize LMDB

Drop the old environment handle and open a fresh one against the new file:

```rust
// Pseudocode
storage.env.close();
storage.env = Environment::new()
    .set_map_size(config.map_size)
    .open(data_dir)?;
```

This gives you exactly the state up through `snapshot_index`.

---

## 4. Update In-Memory State

```rust
storage.last_snapshot_index = cmd.snapshot_index;
storage.last_snapshot_term  = cmd.snapshot_term;
storage.commit_index        = cmd.snapshot_index;
storage.last_applied        = cmd.snapshot_index;
```

* **`last_applied`** is what your state machine considers “executed.”
* **`commit_index`** is how far your durable, replicated log has progressed.

---

## 5. Truncate the WAL

Since the snapshot covers all entries ≤ `snapshot_index`, remove those from your on-disk log:

```rust
// Pseudocode
storage.wal.truncate_up_to(cmd.snapshot_index)?;
```

Strategies here include:

* Deleting old log segments.
* Updating a “start\_index” pointer so replay skips them.

---

## 6. Resume Normal Replication

1. Un-pause your storage lock so you can accept/inject new `AppendEntries` or `InstallSnapshot` RPCs again.
2. Request from the leader “give me entries starting at `commit_index + 1`.”
3. As new `WalRecord`s arrive, apply them in order (with fsync + LMDB txn as before).

---

## 7. Send Acknowledgment

Once you’ve installed the snapshot and are ready for entries > `snapshot_index`, reply to the leader’s `InstallSnapshot` RPC with your new `last_applied` (or some “OK” marker).  That lets the leader know it can advance its match index for this follower.

---

## 8. Error Handling & Edge Cases

* **Partial failures** during write or rename: roll back to the previous env file, keep using your old state, and signal an error to the leader so it can retry.
* **Concurrent reads**: either block reads during swap or open your LMDB env in read-only mode until the swap completes.
* **Busy environments**: if you can’t close LMDB (because of in-flight readers), use LMDB’s copy API to snapshot into a new directory and then atomically switch directories.

---

### Summary

When a follower gets a `SnapshotCommand`:

1. Pause normal log apply; check index staleness.
2. Atomically write the snapshot blob to disk.
3. Reopen LMDB on the new data file.
4. Advance your in-memory indices (`last_snapshot_index`, `commit_index`, `last_applied`).
5. Truncate the on-disk WAL up to that index.
6. Resume replication, asking for entries starting at `snapshot_index + 1`.
7. Acknowledge to the leader.

This ensures the follower’s state jumps forward in one crash-safe, atomic operation—and then seamlessly continues log-based replication from that point.


---

# Calculating CheckSum

use std::fs::File;
use std::io::{self, BufReader, Read};
use crc::{Crc, Digest};

// Pick your CRC-64 polynomial; e.g. ISO-3309
const CRC_64_ISO: Crc<u64> = Crc::<u64>::new(&crc::CRC_64_ISO);

/// Compute CRC-64 over the contents of `path` by streaming.
fn compute_snapshot_checksum_streamed(path: &std::path::Path) -> io::Result<u64> {
    // Open file and wrap in a buffered reader
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Create a CRC digest context
    let mut digest: Digest<u64> = CRC_64_ISO.digest();

    // Buffer to read chunks
    let mut buf = [0u8; 8 * 1024];  // e.g., 8 KB chunks
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        // Update CRC with the slice we just read
        digest.update(&buf[..n]);
    }

    // Finalize and return
    Ok(digest.finalize())
}

fn main() -> io::Result<()> {
    let path = std::path::Path::new("snapshot.bin");
    let checksum = compute_snapshot_checksum_streamed(path)?;
    println!("CRC-64 checksum: {:#016x}", checksum);
    Ok(())
}


What’s happening:
File::open + BufReader — wraps the OS file descriptor in a buffered reader so you pay for I/O in large chunks (here, 8 KB at a time).

Loop & read(&mut buf) — reads up to buf.len() bytes; read returns how many bytes were actually filled.

digest.update(&buf[..n]) — incrementally folds each chunk into the CRC calculation.

When read returns 0, you’re at EOF; call digest.finalize() to get the u64 checksum.

This pattern scales to arbitrarily large snapshot files, bounded only by disk I/O rather than RAM.

--

## CRC-32 vs 64
CRC-32 and CRC-64 are both cyclic redundancy checks—linear polynomials over GF(2)—but differ in size, error-detection strength, and performance trade-offs:

1. **Checksum Width**

   * **CRC-32** produces a 32-bit (4-byte) checksum.
   * **CRC-64** produces a 64-bit (8-byte) checksum.
     Larger width means a much lower chance of undetected errors purely by collision (i.e. two different blobs sharing the same CRC).

2. **Collision Probability**

   * With CRC-32, random collisions occur with probability \~1 in 2³² (\~4 billion).
   * With CRC-64, collisions drop to \~1 in 2⁶⁴ (\~1.8 × 10¹⁹).
     If your snapshots are large or you need a very low silent-corruption risk, CRC-64 is vastly safer.

3. **Error-Detection Properties**
   Both can detect:

   * All single-bit errors.
   * All double-bit errors (up to their polynomial degree).
   * All burst errors shorter than the checksum length.
     But because CRC-64’s polynomial degree is twice as big, it detects longer bursts and combined error patterns that CRC-32 could miss.

4. **Performance & Storage**

   * **CRC-32**: faster on 32-bit hardware (wider CPU registers, optimized instruction support like Intel’s `crc32`), and smaller storage/overhead.
   * **CRC-64**: slower, more compute per byte, and doubles the checksum storage. On modern 64-bit CPUs you can optimize it, but it’s typically a bit heavier than CRC-32.

5. **Common Use-Cases**

   * **CRC-32** is ubiquitous (Ethernet, PNG, ZIP) where 2⁻³² risk is acceptable.
   * **CRC-64** sees use in high-reliability archiving (e.g. large-scale backups, long-lived storage) where the lower collision rate justifies the cost.

**In your SnapshotCommand**: if you expect very large snapshot blobs, long retention periods, or mission-critical integrity, CRC-64 is prudent. If snapshots are small, or you prioritize speed and minimal overhead, CRC-32 might suffice.
