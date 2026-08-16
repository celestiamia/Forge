# Changelog

All notable changes to Forge are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- **Block expressions** — brace-enclosed statement sequences now evaluate to
  their trailing expression and can be used wherever an expression is expected.
  Both `{ ... }` and `unsafe { ... }` work as expressions. `let`/`var` bindings,
  `if`/`while`/`for`, `match`, and `return` are allowed inside a block; if the
  trailing statement is not an expression the block's value is `void`. Works
  across all three targets: `x86_64-unknown-linux-gnu`, `x86_32-unknown-linux-gnu`,
  and `x86_16-boot`.

- **Enum variants** (end-to-end) — `enum` declarations now support construction,
  `match`/`case`, payload destructuring, and discriminant representation.
  - Unit variants: `Color.Red`, matched with `case Color.Red:`
  - Payload variants: `Option.Some(42)`, destructured with `case Option.Some(x):`
    (payload type is checked; integer literals are cast to the payload type)
  - Exhaustiveness is checked at the `match` site (sema); `case _:` is the
    catch-all.
  - Variants are represented as tagged structs in the IR, so they reuse the
    existing struct codegen path. Supported on `x86_64` and `x86_32`.
  - Note: generic enums (`enum E[T]`) and the `@c_enum` attribute remain
    unsupported/unimplemented for now.

- **Block expression example** (`examples/block_expr.dev`) and **enum example**
  (`examples/enum.dev`) demonstrating the new constructs, plus integration tests
  covering both the `x86_64` and `x86_32` targets.

### Fixed

- **Lowerer enum registration ordering** — enum types were registered *after*
  function signatures were collected, so using an enum type in a parameter or
  return annotation (e.g. `def f(c: Color) -> int32`) failed with
  `unknown type: Color`. Enums are now registered during the early type-collection
  pass alongside structs, mirroring struct registration.
