# Examples

* [Hello World](hello.md)
* [Bootloader](bootloader.md)
* [Multi-Module](multimod.md)
* [ForgeOS (16-bit OS)](os.md)
* [ForgeOS32 (16→32 boot chain)](os32.md)
* [ForgeOS64 (16→32→64 boot chain)](os64.md)

The `examples/` directory also holds fixtures exercised by the integration
test suite:

| Example | Demonstrates |
|---------|--------------|
| `block_expr.dev` | Block expressions `let x = { ... }` and `unsafe { ... }` as expressions on x86_64 and x86_32 |
| `enum.dev` | Enum construction, `match`/`case`, payload destructuring, and exhaustiveness on x86_64 and x86_32 |
| `generics.dev` | Generic functions and structs end-to-end on x86_64 (`id[T]`, `swap[T]`, `Pair[T]`, nested `Pair[Pair[int64]]`) |
| `generics32.dev` | Generics on x86_32 (structs via the sret/by-pointer ABI) |
| `struct_ret.dev` | Struct returns, struct arguments, and chained calls on x86_64 and x86_32 |
| `tuple_ret.dev` | Tuple return types, destructuring, and tuple arguments on x86_64 and x86_32 |
| `test_alloc32.dev` | The x86_32 free-list allocator (block reuse via `free`, first-fit splitting) |
| `test_gc32.dev` | Negative fixture: `std.gc` must be rejected on x86_32 with a clean diagnostic |