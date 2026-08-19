//! Forge compiler library.
//!
//! Exposes the Python-like frontend, the native backend, and the driver.

pub mod ast;
pub mod backend;
pub mod driver;
pub mod embed;
pub mod lexer;
pub mod linker;
pub mod lower;
pub mod obj;
pub mod parser;
pub mod sema;
pub mod ty;
