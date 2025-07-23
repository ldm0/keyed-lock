# Keyed Lock

[![Crates.io](https://img.shields.io/crates/v/keyed-lock.svg)](https://crates.io/crates/keyed-lock)
[![Docs.rs](https://docs.rs/keyed-lock/badge.svg)](https://docs.rs/keyed-lock)

This crate provides a keyed lock, which allows you to lock a resource based on a key. This is useful when you have a collection of resources and you want to lock individual resources by their key.

This crate provides both synchronous and asynchronous implementations.

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
keyed-lock = { version = "0.2", features = ["sync", "async"] }
```

### Synchronous

The `sync` module provides a synchronous `KeyedLock`.

Locks with the same key will block each other, while locks with different keys will not.

```rust
use keyed_lock::sync::KeyedLock;

let lock = KeyedLock::new();

// Lock key "a", this will not block.
let _guard_a = lock.lock("a");

// Lock key "b", this will not block as it's a different key.
let _guard_b = lock.lock("b");

// Try to lock "a" again, this will block until `_guard_a` is dropped.
// let _guard_a2 = lock.lock("a");
```

### Asynchronous

The `async` module provides an asynchronous `KeyedLock`.

Locks with the same key will block each other, while locks with different keys will not.

```rust
use keyed_lock::r#async::KeyedLock;

async fn run() {
    let lock = KeyedLock::new();

    // Lock key "a", this will not block.
    let _guard_a = lock.lock("a").await;

    // Lock key "b", this will not block as it's a different key.
    let _guard_b = lock.lock("b").await;

    // Try to lock "a" again, this will block until `_guard_a` is dropped.
    // let _guard_a2 = lock.lock("a").await;
}
```

## Features

- `sync`: Enables the synchronous `KeyedLock`.
- `async`: Enables the asynchronous `KeyedLock`.

By default, both features are enabled.