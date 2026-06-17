//! Re-export of the crate-wide AST for the semantic analyzer.
//!
//! The original analyzer expected a local copy; this shim lets it keep using
//! the same names while the actual AST lives in `crate::ast`.

pub use crate::ast::*;
