# Parsing

A fence of text is not code.

```text
// Read the next record — or stop at the end.
```

A fence of Rust with no comment holds code, which is never read.

```rust
fn dash() -> &'static str {
    "—"
}
```
