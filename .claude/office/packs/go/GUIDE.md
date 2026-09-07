## Go conventions

Feedforward controls. Anything here a machine could check instead is already a
sensor — see `.the-office/harness.md`.

- Every returned error is checked. `errcheck` enforces this on new code; on old
  code it is on you until the backlog clears.
- Errors are wrapped with context on the way up: `fmt.Errorf("loading %s: %w",
  name, err)`. A bare `return err` three frames deep produces a message no one
  can act on.
- Accept interfaces, return structs. Define the interface in the consuming
  package, not the producing one.
- `context.Context` is the first parameter of any function that does I/O, and it
  is threaded through, never `context.Background()` mid-call-stack.
- No naked returns in functions longer than a few lines.
- Package names are singular, lowercase, no underscores, and are not `util`,
  `common`, or `helpers` — those names are how boundary rot starts.
- Goroutines have a defined owner responsible for their shutdown, and a way to
  signal it. A goroutine with no exit path is a leak the test suite will not see.
- Table-driven tests for anything with more than two cases.
