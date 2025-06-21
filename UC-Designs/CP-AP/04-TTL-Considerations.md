You generally want TTL to be a **local eviction mechanism**, not a centralized “leader-only” task.  Here’s a sketch of how to fit TTL into our existing design:

---

### 1. Storing the TTL

Embed an expiration timestamp alongside every value in your WAL + backend.  For example, your `Operation::Put` record becomes:

```rust
enum Operation {
    Put {
      key: Vec<u8>,
      value: Vec<u8>,
      expires_at: Option<u64>, // Unix millis, or None for no TTL
    },
    Delete { key: Vec<u8> },
}
```

In LMDB / Sled / InMemory you store a small header before the payload:

```
[expires_at: i64][value bytes…]
```

---

### 2. Lazy vs. Active Expiration

1. **Lazy (on-access) eviction**

   * On every `get(key)`, read the stored `expires_at`.
   * If it’s in the past, treat it as missing: delete it immediately and return `None`.
   * Pros: zero background work, eviction only when needed.
   * Cons: stale entries linger until someone touches them.

2. **Active (background sweeper)**

   * Maintain a min-heap (or time wheel) of keys by expiration time.
   * Spawn a background task on each node that wakes up at the next expiration, deletes it, then sleeps until the following one.
   * Pros: timely eviction even for cold keys.
   * Cons: some CPU overhead for maintenance.

You can actually **combine** both: lazy on-access plus a coarse sweeper to clear truly cold “zombie” entries.

---

### 3. Per-Node vs. Leader-Only

* **Per-Node**

  * Each cache node is responsible for evicting its own data.
  * TTL is purely a local cache eviction policy; no need to coordinate with the cluster’s CP or AP layer.
  * Simplest: every node runs its own sweeper + lazy checks.

* **Leader-Only** (for CP mode)

  * If you’re using a Raft leader to serialize *all* mutations, you *could* have the leader decide “this key has expired” and propose a `Delete` through the Raft log.
  * Pros: keeps every node in strict lock‐step eviction order (identical state at all times).
  * Cons: needless Raft overhead just to expire cache entries; evictions happen only as fast as the leader pipeline.

In almost all caching use-cases, you want **availability and locality** for TTL: each node independently kicks out its own expired entries.  Let the cluster protocol worry about replicating real data mutations; treat TTL purely as a local eviction mechanism.

---

### 4. Putting It All Together

```text
Client ─┐                ┌─> Sweeper (background thread evicting by min-heap)
        │                │
        ├─> node.get(k) ─┤─┐  // reads expires_at + value
        │                │ └─ if expired { delete; return None }
        │                │
        └─> node.put(k, v, ttl=5s) ──> WAL( Put{key,value,expires_at=now+5s} ) + store
```

* **WAL** now carries the expiration metadata for crash-recovery.
* **Lazy checks** guarantee no stale reads.
* **Background sweeper** keeps the store trim even for cold keys.
* **No leader involvement** keeps eviction fast and local.

---

#### TL;DR

> **TTL is an eviction policy, not a core data mutation.**
> Store the expiration timestamp with each entry, and have each node independently:
>
> 1. Delete on access if expired, and
> 2. (Optionally) run a light background sweeper for truly idle keys.

That gives you both correctness (never return expired data) and scalability (no centralized eviction bottleneck).
