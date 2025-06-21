There’s actually two different “pairs” of indices you might be thinking of:

---

## 1. Leader-side indices (per follower)

These live **only** on the leader, and are used to drive replication:

* **`nextIndex[f]`**
  For each follower *f*, this is the index of the *next* log entry the leader should send to that follower.

  * If a follower is far behind, the leader will send a batch starting at `nextIndex[f]`.
  * If the follower rejects an AppendEntries (because it has a conflicting entry), the leader will decrement `nextIndex[f]` and retry.

* **`matchIndex[f]`**
  For each follower *f*, this is the highest log index the leader knows is *safely* stored on that follower.

  * Once a follower ack’s entry *N*, the leader bumps `matchIndex[f] = N`.
  * The leader uses the collection of all `matchIndex[·]` to compute its own **commitIndex** (e.g. “what’s the highest *N* replicated on a majority?”).

> **Why two?**
>
> 1. `nextIndex` tells the leader *where to resume* sending.
> 2. `matchIndex` tells the leader *how far* that follower has successfully stored entries.
>
> Together they let the leader converge each follower’s log efficiently and decide when entries are committed.

---

## 2. Follower-side indices (local progress)

Each *server* (leader or follower) also tracks two **local** indices to manage its own state machine:

1. **`commitIndex`**

   * The highest log index that this node knows is *committed* (i.e. safely stored on a majority).
   * Updated on followers when the leader’s AppendEntries RPC carries a new commitIndex.

2. **`lastApplied`**

   * The highest index this node has *actually applied* to its state machine (i.e. performed the Set/Delete in LMDB).
   * You only advance this when you open a read–write LMDB txn, replay the log entry, and commit it.

> **Why two?**
>
> * **Separation of concerns**: you may receive and replicate entries (bumping `commitIndex`) faster than you can apply them to the database.
> * You must **never** apply an entry until it’s committed, so you check `lastApplied < commitIndex` and then apply up through `commitIndex`.
> * Keeping them distinct lets you batch application (e.g. apply ten entries in one LMDB txn) without risking uncommitted state leaking into your cache.

---

### Putting it all together

| Role         | What you track                  | Why                                                     |
| ------------ | ------------------------------- | ------------------------------------------------------- |
| **Leader**   | `nextIndex[f]`, `matchIndex[f]` | Drive and measure per-follower replication              |
| **Any node** | `commitIndex`, `lastApplied`    | Separate ordering/commit from state-machine application |

So on a **follower** you *only* need `commitIndex` and `lastApplied` in memory (and persist `lastApplied` across restarts).  The two-index pattern there ensures you never apply entries before they’re safe, and that you can catch up your LMDB state efficiently once new commits arrive.
