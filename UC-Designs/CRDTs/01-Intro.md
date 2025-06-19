In the context of the **CAP theorem**—which describes trade-offs between **Consistency (C)**, **Availability (A)**, and **Partition tolerance (P)**—**CRDTs (Conflict-free Replicated Data Types)** are most commonly associated with systems that prioritize **Availability and Partition Tolerance (AP systems)**.

### Breakdown:

* **CRDT (Conflict-free Replicated Data Type)** is a data structure designed for **eventual consistency** in **distributed systems**. It enables concurrent updates across replicas without coordination, and ensures all replicas converge to the same state automatically.

* In the **CAP triangle**:

  * **Consistency (C)**: All nodes see the same data at the same time.
  * **Availability (A)**: Every request receives a (non-error) response, even without coordination.
  * **Partition Tolerance (P)**: The system continues to operate despite arbitrary message loss or failure between nodes.

### Where CRDTs fit:

* **AP systems**: CRDTs are ideal in systems that prefer **Availability and Partition tolerance** over strict **Consistency**.

  * When a network partition happens, CRDT-based systems allow replicas to continue accepting updates independently (availability).
  * Once the partition heals, CRDTs use mathematically sound merge functions to **reconcile states** without conflicts (eventual consistency).

### Summary:

> **CRDTs are a data type strategy that supports AP systems under the CAP theorem.**
> They allow highly available and partition-tolerant systems to still eventually converge to a consistent state—**eventual consistency**, not strong consistency.

Let me know if you want examples like counters or sets implemented as CRDTs.
