//! The Forge standard library, embedded in the compiler binary.
//!
//! `from std.<name> import ...` resolves to `core/<name>.dev` on disk by
//! walking up from the entry source (so a project can vendor or override its
//! own `core/`).  When no on-disk copy is found, the loader falls back to the
//! stdlib compiled into the binary — keeping `forgec` a single self-contained
//! binary with no data files to install or ship alongside.

/// Returns the embedded source of the stdlib module `name` (e.g. `"io"`),
/// or `None` if no such stdlib module is embedded.
pub fn stdlib_source(name: &str) -> Option<&'static str> {
    Some(match name {
        "alloc" => include_str!("../core/alloc.dev"),
        "fmt" => include_str!("../core/fmt.dev"),
        "gc" => include_str!("../core/gc.dev"),
        "hal" => include_str!("../core/hal.dev"),
        "io" => include_str!("../core/io.dev"),
        "math" => include_str!("../core/math.dev"),
        "mem" => include_str!("../core/mem.dev"),
        "runtime" => include_str!("../core/runtime.dev"),
        "string" => include_str!("../core/string.dev"),
        "volatile" => include_str!("../core/volatile.dev"),
        _ => return None,
    })
}
