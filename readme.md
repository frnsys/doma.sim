Build:

```
cargo build --release
```

Or, to squeeze out more performance where supported:

```
RUSTFLAGS="-C target-cpu=native" cargo build --release
```