//! Semantic analysis for the Forge language.
//!
//! The public API is [`check`] (or [`check_with_file`]), which consumes an
//! `ast::Module` and returns a [`TypedModule`] where every expression carries a
//! resolved [`Type`].
//!
//! Submodules:
//! - [`error`](crate::sema::error) — diagnostics with optional source locations.
//! - [`typed`](crate::sema::typed) — typed AST nodes and monomorphization data.
//! - [`check`](crate::sema::check) — name resolution, type inference/checking,
//!   mutability checking, and unsafe rule enforcement.

pub mod ast;
pub mod check;
pub mod error;
pub mod typed;

pub use check::{check, check_with_file};
pub use error::{Error, Loc};
pub use typed::{MonoInstance, TypedBlock, TypedExpr, TypedExprKind, TypedFunction, TypedItem, TypedMatchCase, TypedModule, TypedPattern, TypedStmt};
