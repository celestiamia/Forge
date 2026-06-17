//! Recursive-descent parser for the Forge language.
//!
//! The public entry point is [`parse_module`], which takes a source string,
//! runs it through the lexer, and returns a [`Module`](crate::ast::Module) or a
//! [`ParseError`].

pub mod parser;

pub use crate::ast::*;
pub use parser::{parse_module, ParseError, Parser};
