# Architecture

High-level overview of the Forge compiler architecture.

## Pipeline Overview

```
┌─────────────┐
│  .dev file  │
└──────┬──────┘
       ▼
┌─────────────┐     ┌─────────────┐
│   Lexer     │────▶│   Parser    │
│ (lexer.rs)  │     │ (parser.rs) │
└─────────────┘     └──────┬──────┘
                           ▼
                    ┌─────────────┐
                    │     AST     │
                    │ (ast/mod.rs)│
                    └──────┬──────┘
                           ▼
                    ┌─────────────┐
                    │   Sema      │
                    │ (check/*)   │
                    └──────┬──────┘
                           ▼
                    ┌─────────────┐
                    │   Typed AST │
                    │ (typed/*)   │
                    └──────┬──────┘
                           ▼
                    ┌─────────────┐
                    │   Lower     │
                    │ (lower/*)   │
                    └──────┬──────┘
                           ▼
                    ┌─────────────┐
                    │     IR      │
                    │ (backend/ir)│
                    └──────┬──────┘
                           ▼
          ┌────────────────┼────────────────┐
          ▼                ▼                ▼
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│  x86_64     │    │  x86_32     │    │  x86_16     │
│  codegen    │    │  codegen    │    │  codegen    │
└──────┬──────┘    └──────┬──────┘    └──────┬──────┘
       ▼                  ▼                  ▼
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│  ELF64      │    │  ELF32      │    │  Flat       │
│  Writer     │    │  Writer     │    │  Writer     │
└─────────────┘    └─────────────┘    └─────────────┘
```

## Source Structure

```
src/
├── main.rs              # CLI entry point
├── lib.rs               # Library root
├── driver/              # Compilation driver
│   ├── mod.rs           # compile() entry
│   └── loader.rs        # Module loading
├── lexer/               # Lexical analysis
│   ├── mod.rs
│   └── lexer.rs
├── parser/              # Parsing
│   ├── mod.rs
│   ├── parser.rs        # Main parser
│   ├── expr.rs          # Expressions
│   ├── stmt.rs          # Statements
│   ├── items.rs         # Items (fn, struct, etc.)
│   └── type.rs          # Type expressions
├── ast/                 # Abstract Syntax Tree
│   └── mod.rs
├── sema/                # Semantic Analysis
│   ├── mod.rs
│   ├── check/           # Type checking
│   │   ├── mod.rs
│   │   ├── expr.rs      # Expression checking
│   │   ├── items.rs     # Item checking
│   │   └── typing.rs    # Type inference
│   ├── typed/           # Typed AST
│   │   └── mod.rs
│   └── error.rs         # Diagnostics
├── lower/               # AST → IR Lowering
│   ├── mod.rs
│   ├── expr.rs
│   └── stmt.rs
├── backend/             # Code Generation
│   ├── mod.rs
│   ├── ir.rs            # Intermediate Representation
│   ├── error.rs         # Backend errors
│   ├── codegen/         # x86_64 backend
│   │   ├── mod.rs
│   │   ├── expr.rs
│   │   ├── stmt.rs
│   │   ├── layout.rs
│   │   ├── runtime.rs
│   │   └── gc.rs
│   ├── codegen32/       # x86_32 backend
│   ├── codegen16/       # x86_16 backend
│   ├── x64/             # x86_64 encoder
│   ├── x86/             # x86_32 encoder
│   └── x16/             # x86_16 encoder
├── obj/                 # Object Writers
│   ├── mod.rs
│   ├── elf.rs           # ELF64
│   ├── elf32.rs         # ELF32
│   └── flat.rs          # Flat binary
├── linker/              # Linker Script System
│   ├── mod.rs
│   ├── config.rs        # LinkerConfig
│   ├── lexer.rs
│   └── parser.rs
└── ty/                  # Type System
    └── mod.rs
```

## Key Components

### Lexer (`src/lexer/lexer.rs`)

- Python-like indentation handling
- Tokenizes: identifiers, keywords, literals, operators
- Produces `Token` stream with positions
- Handles `INDENT`/`DEDENT` tokens for block structure

### Parser (`src/parser/`)

Recursive descent parser:
- `parser.rs` - Main driver, module parsing
- `expr.rs` - Expression parsing (precedence climbing)
- `stmt.rs` - Statement parsing
- `items.rs` - Items (functions, structs, etc.)
- `type.rs` - Type expression parsing

Outputs `ast::Module`

### AST (`src/ast/mod.rs`)

Untyped abstract syntax tree:
- `Module`, `Item`, `Stmt`, `Expr`, `TypeExpr`, `Pattern`
- No type information
- Source location tracking (`Span`)

### Semantic Analysis (`src/sema/check/`)

Three-pass process:
1. **Name Resolution** - Build symbol tables
2. **Type Inference** - Infer types, unify constraints
3. **Type Checking** - Verify types match, emit errors

Outputs `typed::TypedModule` with resolved types on every expression.

### Typed AST (`src/sema/typed/`)

AST with type annotations:
- Every `TypedExpr` has `ty: Type`
- Monomorphization info for generics
- Method resolution tables

### Lowering (`src/lower/`)

Typed AST → IR translation:
- `expr.rs` - Expressions to IR
- `stmt.rs` - Statements to IR
- Allocates stack slots, generates IR instructions

### IR (`src/backend/ir.rs`)

Machine-level intermediate representation:
- `Type` - Scalar, pointer, struct, slice
- `Expr` - Low-level ops (load, store, call, binop)
- `Stmt` - Control flow (jump, cond, label)
- `Program` - Functions, structs, globals

### Codegen (`src/backend/codegen/`)

Target-specific machine code generation:
- `codegen/` - x86_64 (System V AMD64)
- `codegen32/` - x86_32 (cdecl)
- `codegen16/` - x86_16 (real mode)

Each backend:
- Register allocation (linear scan)
- Instruction selection
- Stack frame layout
- Runtime emission

### Encoders (`src/backend/x64/`, `x86/`, `x16/`)

Machine code encoding:
- Instruction encoding
- Register allocation helpers
- ModR/M, SIB, REX prefixes
- Relocation fixups

### Object Writers (`src/obj/`)

- `elf.rs` - ELF64 writer
- `elf32.rs` - ELF32 writer
- `flat.rs` - Flat binary (boot sector)

### Linker Scripts (`src/linker/`)

Extensible target specification:
- `.fld` files describe targets
- Memory regions, sections, runtime
- Replaces hardcoded target triples

## Data Flow

```
Source (.dev)
    │
    ▼
Tokens (Lexer)
    │
    ▼
AST (Parser)
    │
    ▼
Typed AST (Sema)
    │
    ▼
IR (Lower)
    │
    ▼
Machine Code (Codegen + Encoder)
    │
    ▼
Object File (ELF/Flat Writer)
```

## Memory Management

### Compiler (Rust)

- All Rust code uses RAII
- Arena allocation for AST/IR (optional)
- No manual memory management

### Generated Code

| Target | Allocator | GC |
|--------|-----------|-----|
| x86_64 | Bump (64 KiB) + Mark-Sweep | ✅ |
| x86_32 | Bump (64 KiB) | ❌ |
| x86_16 | None | ❌ |

## Error Handling

- `anyhow::Error` for internal errors
- Structured diagnostics with `Loc` (file/line/col)
- Error codes for categorization
- Recovery in parser (panic mode)

## Testing Architecture

```
tests/
├── integration.rs      # End-to-end compilation + execution
├── lexer_tests.rs      # Lexer unit tests
├── parser_tests.rs     # Parser unit tests
```

Integration tests:
1. Compile example with `forgec`
2. Run resulting binary
3. Check stdout/exit code

## Performance Characteristics

| Phase | Complexity | Notes |
|-------|------------|-------|
| Lexing | O(n) | Single pass |
| Parsing | O(n) | Recursive descent |
| Sema | O(n²) worst | Unification |
| Lowering | O(n) | Tree walk |
| Codegen | O(n) | Linear scan regalloc |
| Encoding | O(n) | Single pass |

## Extensibility Points

| Extension | Location |
|-----------|----------|
| New syntax | `parser/`, `lexer/`, `ast/` |
| New type | `ty/`, `sema/check/typing.rs`, `ir.rs` |
| New target | `codegen*/`, `obj/`, `linker/config.rs` |
| New stdlib | `core/*.dev` |
| New optimization | `codegen/`, `lower/` |