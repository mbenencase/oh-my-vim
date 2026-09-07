## Python conventions

These are feedforward controls: they steer before code is written. Everything
here that a machine could check instead is already a sensor — see
`.the-office/harness.md` for which.

- Type annotations on every public function signature. Internal helpers may be
  inferred; anything importable across a module boundary may not.
- Prefer `pathlib.Path` over `os.path`. String paths are a source of
  platform-specific bugs the test suite will not catch on one platform.
- Resources that hold an OS handle — files, sockets, DB sessions, subprocesses —
  are acquired in a `with` block or an `async with` block. Never bare `.close()`
  in a `finally`.
- Exceptions carry context. `raise ValueError(f"expected {kind}, got {actual}")`,
  never a bare `raise ValueError`.
- No mutable default arguments. This is enforced by ruff (B006) once the `B`
  rule set is enabled; until then it is on you.
- Tests live in `tests/`, mirroring the package layout. One assertion concept
  per test; a test that asserts five unrelated things reports one failure and
  hides four.
