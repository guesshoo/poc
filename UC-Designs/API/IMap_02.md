# Scalling IMAP
LMDB is actually very well suited to high-concurrency read-heavy workloads in a single process, but there are a few things to keep in mind when you’re running a TCP service with many concurrent clients:

---

## 1. Single-Writer, Multiple-Readers

* **Readers** in LMDB are entirely lock-free: each reader gets its own snapshot of the data map and can proceed without blocking writers or other readers. You can safely have hundreds (even thousands) of concurrent reader transactions (`begin_ro_txn`) without contention.
* **Writers** are serialized under a single writer lock. Only one thread at a time may commit a write transaction (`begin_rw_txn`). While the actual window in which the writer lock is held is very small (just during the commit), heavy write throughput can become a bottleneck.

**Implication**: If your workload is mostly `GET`s, you’ll scale almost linearly with client count. If your workload is `PUT`/`DELETE`-heavy, you’ll hit the single-writer contention and start queueing.

---

## 2. Minimizing Write Contention

If you need higher write concurrency:

1. **Batch multiple mutations** into one write transaction.

   ```rust
   let mut txn = env.begin_rw_txn()?;
   for (k, v) in batch { txn.put(db, k, v, WriteFlags::empty())?; }
   txn.commit()?;
   ```

   Fewer commits → less lock contention.

2. **Shard your data** across multiple LMDB environments (i.e. directories).

   * Hash your key to choose one of N shards (e.g. N=4 environments on disk).
   * Each environment has its own single-writer lock, so you get N× the write throughput.

3. **Use a write-behind queue** or batching layer in front of LMDB: aggregate writes in memory and apply on a timer or size threshold, trading a bit of latency for throughput.

---

## 3. WAL Overhead

In our design we prepend each `PUT`/`DELETE` to a per-map WAL:

* WAL append is also serialized, since each `RefCell<WriteAheadLog>` is mutated under `&self`. You’ll similarly have a single serial stream of WAL writes per map.
* However, vector-I/O + `fsync` per operation is relatively fast; if it becomes a hotspot you can batch multiple operations into a single fsync (e.g. group 10 ops or 10 ms of buffering).

---

## 4. Async TCP Server Architecture

Your Tokio server spawns one task per connection. That’s fine for mostly‐idle or read‐heavy connections:

* **Readers** won’t block each other; they only borrow the environment immutably.
* **Writers** will queue at the LMDB writer lock. If you expect many simultaneous writers, consider separating your write path onto a dedicated single‐threaded “writer service” (e.g. an `mpsc` channel) so you avoid each Tokio task fighting for the same lock.

---

## 5. Observed Scaling Strategies

In practice, high-throughput LMDB-backed services:

* **Batch writes** aggressively (amortize one commit across thousands of records).
* **Shard** their environment into multiple files/directories via key hashing.
* **Isolate hot tables** into separate named DBs and environment shards.
* Combine LMDB with an in-memory queue or cache layer to absorb write bursts.

---

### TL;DR

* **Read scale**: excellent, virtually unlimited concurrent readers.
* **Write scale**: limited by a single-writer lock; mitigate via batching, sharding, or a dedicated writer thread.
* **WAL scale**: similar single-stream append; batch fsyncs to reduce I/O contention.

With these patterns—especially batching and sharding—you can easily handle hundreds or thousands of clients doing mixed reads and writes over TCP.
