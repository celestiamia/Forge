//! Typed intermediate representation consumed by the native code generator.
//!
//! This IR is intentionally small and close to the machine model: every
//! expression carries its type, variables live in stack slots, and statements
//! are structured for direct x86-64 emission.

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bool,
    Char,
    Void,
    Ptr(Box<Type>),
    Struct(String),
}

impl Type {
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Type::I8 | Type::I16 | Type::I32 | Type::I64 |
            Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::Char
        )
    }

    pub fn is_signed(&self) -> bool {
        matches!(self, Type::I8 | Type::I16 | Type::I32 | Type::I64)
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Type::F32 | Type::F64)
    }

    pub fn is_numeric(&self) -> bool {
        self.is_integer() || self.is_float()
    }

    pub fn width_bits(&self) -> u32 {
        match self {
            Type::I8 | Type::U8 | Type::Char | Type::Bool => 8,
            Type::I16 | Type::U16 => 16,
            Type::I32 | Type::U32 | Type::F32 => 32,
            Type::I64 | Type::U64 | Type::F64 => 64,
            _ => panic!("width_bits on non-scalar type"),
        }
    }

    pub fn byte_size(&self) -> usize {
        (self.width_bits() / 8) as usize
    }
}

#[derive(Clone, Debug)]
pub struct Program {
    pub name: String,
    pub structs: Vec<StructDef>,
    pub globals: Vec<Global>,
    pub externs: Vec<ExternFunc>,
    pub funcs: Vec<Func>,
    pub hosted: bool,
    pub target: Option<String>,
    pub arch: Option<String>,
    pub obj_format: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<(String, Type)>,
}

impl StructDef {
    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|(n, _)| n == name)
    }
    pub fn field_type(&self, name: &str) -> Option<&Type> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, t)| t)
    }
}

#[derive(Clone, Debug)]
pub struct Global {
    pub name: String,
    pub ty: Type,
    pub init: Literal,
}

#[derive(Clone, Debug)]
pub struct ExternFunc {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub ret: Type,
    pub varargs: bool,
}

#[derive(Clone, Debug)]
pub struct Func {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub ret: Type,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Let { name: String, ty: Type, init: Option<Expr> },
    Assign { lhs: LValue, rhs: Expr },
    Return(Option<Expr>),
    Expr(Expr),
    If { cond: Expr, then: Vec<Stmt>, else_: Option<Vec<Stmt>> },
    While { cond: Expr, body: Vec<Stmt> },
    For { init: Option<Box<Stmt>>, cond: Expr, step: Option<Expr>, body: Vec<Stmt> },
    Unsafe(Vec<Stmt>),
    Break,
    Continue,
    /// Allocate `count` elements of `elem_ty` on the stack and bind `name` to
    /// the address (as `ptr[elem_ty]`).
    StackAlloc { name: String, elem_ty: Type, count: usize },
}

#[derive(Clone, Debug)]
pub enum LValue {
    Var(String),
    Deref(Expr),
    Field { base: Expr, field: usize },
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Type,
}

impl Expr {
    pub fn new(kind: ExprKind, ty: Type) -> Self {
        Self { kind, ty }
    }
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Lit(Literal),
    Var(String),
    Bin { op: BinOp, left: Box<Expr>, right: Box<Expr> },
    Call { func: String, args: Vec<Expr> },
    Cast { expr: Box<Expr>, ty: Type },
    Gep { base: Box<Expr>, field: usize },
    Load(Box<Expr>),
    AddrOf(Box<Expr>),
    /// A statement block used to represent expression-level match and other
    /// multi-statement expressions.  The statements are executed for side
    /// effects and the trailing expression is the value of the block.
    Block(Vec<Stmt>, Box<Expr>),
    Asm { template: String, constraints: String, inputs: Vec<(Expr, String)>, output: Option<(Type, String)>, clobbers: Vec<String> },
    /// `sizeof(T)` — compile-time constant size of a type in bytes.
    SizeOf(Type),
    /// `offsetof(T, field)` — compile-time constant byte offset of a struct field.
    OffsetOf { ty: Type, field: usize },
}

#[derive(Clone, Debug)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(u8),
    String(String),
    Null,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

impl BinOp {
    pub fn is_arithmetic(&self) -> bool {
        matches!(self, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod)
    }
    pub fn is_comparison(&self) -> bool {
        matches!(self, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
    }
    pub fn is_logical(&self) -> bool {
        matches!(self, BinOp::And | BinOp::Or)
    }
}
