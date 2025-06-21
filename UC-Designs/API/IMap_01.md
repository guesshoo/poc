## Structuring LMDB dbs for IMap
---

## 1. One named DB per map

**Pros:**

* **Isolation:** No risk of accidental key collisions or prefix-brute bugs.
* **Statistics & tuning:** You can tune e.g. page size or flags per DB if needed.
* **Logical clarity:** `get_map("users")` really is a different “database”.

**Cons:**

* **Max DBs limit:** LMDB requires you to set `max_dbs` up front. If you suddenly need more maps than you anticipated, you must reopen the environment with a higher limit.
* **Resource overhead:** Each named DB adds some metadata; hundreds or thousands of maps can bloat your environment.
* **Complexity in management:** You have to track and garbage-collect DB handles when maps go out of scope.

---

## 2. Single DB + key prefixes

An alternative is to keep exactly one LMDB database in the environment and namespace your maps by prefixing keys:

```text
<map_name>::<user_key> → value
```

```rust
fn prefixed_key(map: &str, key: &[u8]) -> Vec<u8> {
    let mut p = map.as_bytes().to_vec();
    p.extend_from_slice(b"::");
    p.extend_from_slice(key);
    p
}

// Usage:
store.put(&prefixed_key("user", b"id:42"), b"{ ... }")?;
```

**Pros:**

* **Unlimited maps:** No need to manage a `max_dbs` limit or reopen the env just to add a new map.
* **Lower overhead:** Only one DB handle and one set of metadata.
* **Dynamic maps:** You can create or drop maps on the fly simply by starting to use a new prefix (or scanning/deleting prefixes).

**Cons:**

* **Manual isolation:** You must be diligent to always prefix your keys correctly.
* **Scanning costs:** To list all maps you need to maintain a directory of map names separately (or scan all keys and deduce prefixes).
* **Single-DB tuning:** All maps share the same LMDB parameters.

---

## 3. Hybrid: “directory” DB + individual DBs

You could combine both:

1. **Directory DB**—stores a list of active map names, plus metadata per map.
2. **Prefix or named-DB access**—depending on your use case you either open a named DB or use prefix access.

This gives you dynamic-map discovery while still isolating hot-tables into their own DB when needed.

---

### Recommendation

* If you expect a **small, bounded** number of maps (e.g. 5–20), named DBs are fine—pick a reasonable `max_dbs` and stick with `MapService::get_map(...)`.
* If you need **highly dynamic** map creation or expect **hundreds/thousands** of maps, a single-DB with key prefixes will avoid hitting the `max_dbs` limit and keep your environment lean.
* For **very large-scale** use cases, consider a hybrid: keep a “catalog” in one DB and spin up per-map DBs only for the busiest maps.

Either approach works; it’s really a tradeoff between isolation/resource-usage (named DBs) and flexibility/simplicity (prefixing).

===


### 1. “Directory” Database

* You reserve **one** LMDB named database (e.g. called `"__catalog__"`) whose job is simply to track the *names* of all active maps and any global metadata.
* For each map you create or drop, you atomically insert or remove its entry in the catalog.
* You can even store per-map configuration (e.g. creation timestamp, custom flags) right alongside the name.

**Example catalog entry**

| key (bytes) | value (bytes)            |
| ----------- | ------------------------ |
| `"user"`    | `{"created":1638320000}` |
| `"orders"`  | `{"created":1638406400}` |

When your service starts up, it opens the catalog DB and reads out the full list of maps.

---

### 2. On-Demand Named Databases

* Instead of pre-allocating *all* possible named DBs up front, you only call `env.create_db(Some(map_name))` when a map becomes “hot” or is first accessed.
* That returns you a `Database` handle scoped to that map; you wrap it in your usual `LmdbStorage { env, db }`.
* Since the catalog has the master list, once you’ve created a handle you also record `map_name` in the catalog (if not already present).

This avoids needing to give LMDB a large `max_dbs` at startup and limits resource usage to only those maps you actually use.

---

### 3. Fallback to Prefix or In-Memory for Cold Maps

* For very ephemeral or lightly used maps—where you might not even write anything—you can continue to use the **single-DB + key-prefix** pattern, avoiding both WAL files *and* extra DB handles.
* Periodically (or on shutdown), you can “promote” a prefix-based map into its own named DB if it grows beyond a size threshold or you want faster isolation.

---

## Why Hybrid?

| Goal                   | Catalog-only | Prefix-only       | Named-DB-only | **Hybrid**                |
| ---------------------- | ------------ | ----------------- | ------------- | ------------------------- |
| Discover all maps      | ✅            | ❌ (scan prefixes) | ✅             | ✅ (read catalog)          |
| Low `max_dbs` pressure | ✅            | ✅                 | ❌             | ✅ (only hot maps)         |
| Strong isolation       | ❌ (prefixes) | ❌                 | ✅             | ✅ (hot maps get own DB)   |
| Dynamic map lifecycle  | ✅            | ✅                 | ❌ (fixed max) | ✅ (catalog drives create) |
| Per-map tuning         | ❌            | ❌                 | ✅             | ✅ (on-demand create\_db)  |

---

### Rough Workflow

1. **Startup**

   * Open `env` with a modest `max_dbs` (e.g. 32).
   * Open the catalog DB and `txn.get("__catalog__")` to load the active map list.
2. **Accessing a map**

   * If `map_name` is in catalog:

     * If you already have an open `LmdbStorage` for it, use it.
     * Otherwise call `LmdbStorage::from_env(env.clone(), map_name)`, cache the handle.
   * If not in catalog yet:

     * Insert `map_name` into catalog (so everyone else sees it).
     * Then `create_db(...)` and cache handle.
3. **Creating a map**

   * Same as above, but you can also initialize default values.
4. **Deleting a map**

   * Remove its entry from catalog.
   * Call `env.drop_db(db_handle)` to free the named DB.
   * Optionally delete or archive its WAL file and on-disk data.

This hybrid design lets you support **unbounded** map names (via the catalog), keep your LMDB environment lean (only create as many DB handles as active maps), and preserve per-map isolation and performance when you need it.
