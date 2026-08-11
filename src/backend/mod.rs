//! Native code generation backend.
//!
//! The backend converts a typed IR (`ir::Program`) into a native executable.
//! It consists of:
//!
//! * `ir`   - a small typed intermediate representation,
//! * `x64`  - a minimal x86-64 assembler,
//! * `x86`  - a minimal 32-bit x86 assembler,
//! * `x16`  - a tiny 16-bit real-mode assembler for boot sectors,
//! * `codegen` - the IR to machine-code translator,
//! * `codegen32` - the 32-bit x86 IR code generator,
//! * `codegen16` - the 16-bit real-mode IR code generator.

pub mod codegen;
pub mod codegen32;
pub mod codegen16;
pub mod error;
pub mod ir;
pub mod x64;
pub mod x86;
pub mod x16;
