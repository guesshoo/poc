Many common data structures can be implemented as **CRDTs**, thanks to their mathematical foundation (commutativity, associativity, idempotency). Here’s a categorized list of some widely used CRDT data types:

---

### 🧮 **Counters**

* **G-Counter (Grow-only Counter)**:

  * Only increments are allowed.
  * State is a map of replica IDs to integers.
* **PN-Counter (Positive-Negative Counter)**:

  * Supports both increment and decrement operations.
  * Internally uses two G-Counters: one for increments, one for decrements.

---

### 📦 **Registers (single-value storage)**

* **LWW-Register (Last-Write-Wins Register)**:

  * Stores a single value with an associated timestamp.
  * Latest timestamp wins.
* **MV-Register (Multi-Value Register)**:

  * Stores all concurrent values written during a partition, letting the application resolve conflicts.

---

### 🗂️ **Sets**

* **G-Set (Grow-only Set)**:

  * Only supports adding elements.
* **2P-Set (Two-Phase Set)**:

  * Supports add and remove.
  * Two internal sets: one for additions, one for removals (once removed, cannot be re-added).
* **OR-Set (Observed-Remove Set)** or **Add-Wins Set**:

  * Allows add and remove.
  * Tracks unique additions using tags (e.g., operation IDs).
  * Resolves conflicts in favor of adds or removes depending on variant.
* **LWW-Element-Set**:

  * Uses timestamps to resolve add/remove conflicts (last-write-wins semantics).

---

### 🗃️ **Maps (Dictionaries)**

* **CRDT Map** (a.k.a. delta map, OR-Map):

  * Maps keys to CRDTs (e.g., counters, sets).
  * Each entry is itself a CRDT, enabling nested merge behavior.

---

### 📝 **Sequences / Lists**

* **RGA (Replicated Growable Array)**:

  * Used in collaborative text editing (e.g., each character has a position).
* **WOOT, LSEQ, Logoot**:

  * Other list CRDTs designed to support concurrent insertions/deletions with a total order.

---

### 🌳 **Graphs**

* **CRDT Graphs**:

  * Typically built on top of CRDT sets and maps for vertices and edges.
  * Useful for collaborative graph editing.

---

### 📊 **Flags**

* **Enable-Wins / Disable-Wins Flag**:

  * Boolean state where the result depends on which operation wins in conflict resolution.

---

### Summary Table:

| Data Structure | CRDT Variants                  |
| -------------- | ------------------------------ |
| Counter        | G-Counter, PN-Counter          |
| Register       | LWW-Register, MV-Register      |
| Set            | G-Set, 2P-Set, OR-Set, LWW-Set |
| Map            | OR-Map / CRDT-Map              |
| Sequence       | RGA, Logoot, WOOT, LSEQ        |
| Graph          | Vertex/Edge-based CRDTs        |
| Flag           | Enable-Wins / Disable-Wins     |

---

Would you like code examples for any of these, or a more detailed comparison of how their merge logic works?
