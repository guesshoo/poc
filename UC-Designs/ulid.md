## ULID oveview
The `ulid` crate—a Rust implementation of the [ULID](https://github.com/ulid/spec) (Universally Unique Lexicographically Sortable Identifier) specification:

### Crate and Version

* **Name:** `ulid` (on crates.io)
* **Latest Released Version:** 1.2.1 (as of June 2025) ([docs.rs][1])
* **License:** MIT
* **Repository:** [dylanhart/ulid-rs on GitHub](https://github.com/dylanhart/ulid-rs) ([docs.rs][1])

### What Is a ULID?

A ULID is a 128-bit identifier where:

* The **first 48 bits** encode a millisecond-precision UNIX timestamp (for lexicographic sortability).
* The **remaining 80 bits** are random (for uniqueness).
* Canonically represented as a 26-character Crockford Base32 string ([docs.rs][2]).


### Potential Usage

* When naming Write-Ahead Log (WAL) files. Easy sorting.


### Quickstart Usage

```rust
use ulid::Ulid;

// Generate a new ULID with the current timestamp + random bits
let ulid = Ulid::new();

// Convert to the 26-char Base32 string
let s = ulid.to_string();

// Parse back from the string
let parsed = s.parse::<Ulid>().unwrap();
assert_eq!(ulid, parsed);
```

You can also explicitly `Ulid::from_string(&s)` or use the `FromStr` trait ([docs.rs][3]).

### Key Types & Modules

```text
Ulid           – The core 128-bit ULID type  
Generator      – Produces monotonically increasing ULIDs when multiple are generated in the same ms  
Archived Ulid  – For safe storage/resolution of an existing ULID  
Ulid Resolver  – Resolve archived ULIDs back to their runtime form  
```

Error enums cover Base32 decode/encode errors and monotonic-increment failures ([docs.rs][3]).

### Feature Flags

The crate is highly configurable via Cargo features ([docs.rs][1]):

* **std** (default): enables use of the standard library and the `rand` crate.
* **serde**: adds Serde `Serialize`/`Deserialize` support (with optional `ulid_as_u128` for binary U128 encoding).
* **uuid**: infallible conversions to/from `uuid::Uuid` via `From`.
* Plus optional support for `bytes`, `postgres-types`, `rkyv`, and more.

### Monotonic Generation

To avoid duplicate ULIDs when creating many in the same millisecond, use the `Generator`:

```rust
use ulid::Generator;

let mut gen = Generator::new();
let first = gen.generate().unwrap();
let second = gen.generate().unwrap();
// second will be incremented lexicographically if in the same ms
```

Errors in monotonic increments are represented by `MonotonicError` ([docs.rs][3]).

---

This should give you everything you need to get started with the `ulid` crate in Rust—feel free to dive into the docs for deeper API details!

[1]: https://docs.rs/crate/ulid/latest "ulid 1.2.1 - Docs.rs"
[2]: https://docs.rs/ulid/latest/ulid/struct.Ulid.html?utm_source=chatgpt.com "Ulid in ulid - Rust - Docs.rs"
[3]: https://docs.rs/ulid/latest/ulid/ "ulid - Rust"
