//! Recursive-descent parser for the Forge language.
//!
//! The public entry point is [`parse_module`], which takes a source string,
//! runs it through the lexer, and returns a [`Module`](crate::ast::Module) or a
//! [`ParseError`].
//!
//! Submodules:
//! - [`parser`](crate::parser::parser) — token helpers and the entry point.
//! - [`items`](crate::parser::items) — imports, items, functions, and ADTs.
//! - [`stmt`](crate::parser::stmt) — statements, match cases, and patterns.
//! - [`expr`](crate::parser::expr) — expressions, literals, and inline asm.
//! - [`type`](crate::parser::type) — type expression parsing.

pub mod expr;
pub mod items;
pub mod parser;
pub mod stmt;
pub mod r#type;

pub use crate::ast::*;
pub use parser::{parse_module, ParseError, Parser};

#[cfg(test)]
mod tests;
