In Rust you have a few equally-idiomatic ways to turn a `Result<…>` into a plain integer (defaulting to `0` on error).  Pick whichever fits your style:

---

### 1. `match` + early return

```rust
fn get_something(env: &Environment) -> i32 {
    let txn = match env.get_ro_txn() {
        Ok(txn) => txn,
        Err(err) => {
            eprintln!("failed to open RO txn: {}", err);
            return 0;
        }
    };

    // ... use `txn` to compute your integer ...
    42
}
```

This is the most explicit: you handle the `Err` case up front and immediately return `0`.

---

### 2. Combinators: `map` + `unwrap_or`

If you can turn your `Txn` into an `i32` in one expression, you can do:

```rust
fn get_something(env: &Environment) -> i32 {
    env.get_ro_txn()
        // map the Ok(txn) into your integer
        .map(|txn| {
            // e.g. read some value out of `txn`
            txn.some_int_method()
        })
        // on Err(_) return 0
        .unwrap_or(0)
}
```

Or even more concisely with `map_or`:

```rust
fn get_something(env: &Environment) -> i32 {
    env.get_ro_txn()
        // first arg is the default on Err,
        // second is the closure to run on Ok(txn)
        .map_or(0, |txn| txn.some_int_method())
}
```

---

### 3. When your output type implements `Default`

Integers implement `Default` (which is `0`), so you can also write:

```rust
fn get_something(env: &Environment) -> i32 {
    env.get_ro_txn()
        .and_then(|txn| {
            // if you have further Result-returning calls…
            Ok(txn.some_int_method())
        })
        // uses Default::default() == 0
        .unwrap_or_default()
}
```

---

#### Which to choose?

* Use **`match`** if you want to do logging or more complex error handling and then bail out early.
* Use **`map_or`** / **`unwrap_or`** for quick one-liners when you simply collapse `Err(_) → 0`.
* Prefer returning a `Result<…, E>` with `?` when the caller might want to handle the error—only default to `0` when “error = zero” really makes sense in your domain.
