## Rust conventions

Feedforward controls. Anything here a machine could check instead is already a
sensor — see `.the-office/harness.md`.

- No `unwrap()` or `expect()` outside tests and `main`. In library code, return
  `Result` and let the caller decide. This is the single most common source of
  production panics.
- No `unsafe` without a `// SAFETY:` comment stating the invariant that makes it
  sound. Unsafe without that comment is unreviewable.
- Errors use `thiserror` for libraries and `anyhow` for binaries. Do not mix
  them in one crate.
- Prefer borrowing to cloning. Reach for `clone()` deliberately, and where it is
  in a hot path, say why in a comment.
- Public API takes `&str`/`&[T]` and returns owned types. Taking `String` by
  value at an API boundary forces every caller to allocate.
- Newtype over primitive for domain identifiers. `struct UserId(u64)` makes a
  whole class of argument-order bug into a compile error — this is a
  computational guide you get for free.
- `#[must_use]` on any constructor or builder method whose result being dropped
  is a bug.
