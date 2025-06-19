To model your **WAL-based counter or lock systems** for Jepsen-style testing, you need to construct both a **formal test model** and a **test harness** that simulates clients, failures, and verifies behavior against expected guarantees. Below, I'll break this into two sections: one for your **counter**, and one for your **lock**, both backed by a WAL and FlatBuffer entries in your Rust system.

---

## ⚙️ WAL-Based System Assumptions

Since you're using a **Write-Ahead Log (WAL)** with **FlatBuffers**, we’ll assume:

* State changes are **persisted before applied** to in-memory state.
* Each operation has an **idempotent identifier** (e.g. UUID, client id + sequence number).
* Nodes can **replay logs on crash**.
* Logs are **replicated** in the CP case (via Raft) or may diverge in AP case (merged via CRDT or gossip).

---

## 🔢 JEPSEN MODEL: WAL-Backed Counter (Linearizable)

### 💡 Goal:

Test that the counter behaves **atomically** and **linearly** (i.e., no lost increments, and reads always see the latest committed value).

### ✅ Model Behavior:

```clojure
(defrecord CounterModel [value])
```

### 📐 Operation Types:

```clojure
{:type :invoke :f :add :value 1}
{:type :invoke :f :read}
{:type :ok :f :read :value 42}
{:type :fail :f :add :value 1}
```

### 🔬 Validity Rules:

* Reads must return values consistent with a serial execution of adds.
* No values should be skipped or duplicated.
* Retried adds must not double-apply.

### 🔁 WAL Considerations:

* WAL replay should not re-apply already committed ops.
* Operation IDs must be **uniquely trackable** and **deduplicated**.

### 🔥 Failure Scenarios:

* WAL partially written: counter must not increment.
* Crash + recovery: counter should resume from last committed state.
* Partitioned Raft group: counter must reject writes if quorum is missing.

---

## 🔐 JEPSEN MODEL: WAL-Backed Distributed Lock (FencedLock)

### 💡 Goal:

Ensure that **only one client holds the lock at a time**, and locks are **fenced** (lease/versioning increases monotonically).

### ✅ Model Behavior:

```clojure
(defrecord LockModel [owner fence])
```

### 📐 Operation Types:

```clojure
{:type :invoke :f :lock :process "client1"}
{:type :ok :f :lock :value {:fence 1234}}

{:type :invoke :f :unlock :process "client1"}
{:type :fail :f :lock :value {:timeout true}}

{:type :invoke :f :lock :process "client2"}
{:type :ok :f :lock :value {:fence 1235}}
```

### 🔬 Validity Rules:

* Only one lock holder at a time.
* Fence tokens increase strictly per acquisition.
* No stale/unreleased locks are accepted after timeout.
* If the WAL replays an old lock event, it must not override newer ones.

### 🔁 WAL Considerations:

* `lock()` must be atomic with WAL entry + fencing.
* Crashes should not result in zombie lock holders.
* WAL replay must recognize "unlock" and invalidate old holders.

### 🔥 Failure Scenarios:

* WAL not flushed before crash → lock must not be granted.
* Raft split: two leaders → fencing must detect divergence.
* Partition + stale state → retries must fail or timeout.

---

## 🛠️ Jepsen Harness Construction (Language Agnostic)

### 1. **Client Generator**

* Simulate multiple clients issuing random `add`, `read`, `lock`, and `unlock` operations.
* Include retry logic and timeouts.

### 2. **Nemesis (Fault Injector)**

* Partition network between nodes.
* Kill and restart nodes (simulate WAL replay).
* Introduce disk latency or partial I/O failure.

### 3. **Log Validator**

* Collect logs from all nodes.
* Replay WAL independently and compare to Jepsen model.
* Validate **serializability** (for counter) and **mutual exclusion** (for lock).

---

## 🧪 Sample Jepsen Workload Ideas

| System  | Workload                         | Property Asserted               |
| ------- | -------------------------------- | ------------------------------- |
| Counter | Concurrent adds + reads          | Linearizability, no lost writes |
| Lock    | Overlapping lock attempts        | Mutual exclusion, fencing       |
| Lock    | Crash during lock, then restart  | No stale lock ownership         |
| Counter | Partition + adds from both sides | Raft consistency enforcement    |
| Lock    | Fence replay from WAL            | Monotonic fence increases       |

---

## 🧠 Final Tips

* If you're using FlatBuffers, **embed operation UUIDs and timestamps** to support replay validation and idempotency.
* For Jepsen-style validations, you'll need to **instrument the Rust system to expose logs or in-memory state** safely for model checking.
* Use fencing tokens as **causal version vectors**—Jepsen can validate that no client operates on an outdated fence.

---

Would you like a concrete Rust test harness sketch (e.g., using `tokio`, `libp2p`, or `quickcheck`) to simulate this kind of Jepsen-style testing in your own environment?
