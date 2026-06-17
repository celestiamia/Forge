//! Abstract syntax tree for the Forge language.
//!
//! The AST is intentionally plain: paths are represented as `Vec<String>`,
//! recursive nodes use `Box`, and nodes do not carry source spans.  This keeps
//! the parser focused on syntax while still providing enough structure for
//! downstream analysis.

// ---------------------------------------------------------------------------
// Module
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub package: String,
    pub imports: Vec<Import>,
    pub items: Vec<Item>,
}

// ---------------------------------------------------------------------------
// Imports
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Import {
    /// `import std.io` or `import std.io as sio`
    Path {
        path: Vec<String>,
        alias: Option<String>,
    },
    /// `from std.io import println, putchar` or `from std.io import *`
    From {
        path: Vec<String>,
        items: Option<Vec<String>>, // None means wildcard `*`
    },
}

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(Function),
    Struct(Struct),
    Union(Union),
    Enum(Enum),
    Impl(Impl),
    Use(Use),
    ExternFn(ExternFn),
    Const(ConstItem),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub unsafe_kw: bool,
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub body: Option<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Struct {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub name: String,
    pub generics: Vec<String>,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Union {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub name: String,
    pub generics: Vec<String>,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub name: String,
    pub generics: Vec<String>,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: String,
    pub payload: Option<TypeExpr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Impl {
    pub target: TypeExpr,
    pub methods: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Use {
    pub path: Vec<String>,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternFn {
    pub attrs: Vec<Attribute>,
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstItem {
    pub vis: Visibility,
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Visibility {
    #[default]
    Private,
    Public,
}

// ---------------------------------------------------------------------------
// Attributes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Attribute {
    Packed,
    Align(u64),
    Freestanding,
    Extern(String),
    CEnum,
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let(LetStmt),
    Var(VarStmt),
    Assign(AssignStmt),
    Expr(Expr),
    Return(Option<Expr>),
    If(IfStmt),
    For(ForStmt),
    While(WhileStmt),
    Match(MatchStmt),
    UnsafeBlock(Block),
    Loop(Block),
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LetStmt {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub value: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarStmt {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub value: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssignStmt {
    pub target: Expr,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_block: Block,
    pub elifs: Vec<(Expr, Block)>,
    pub else_block: Option<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForStmt {
    pub var: String,
    pub iter: Expr,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhileStmt {
    pub condition: Expr,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchStmt {
    pub scrutinee: Expr,
    pub cases: Vec<MatchCase>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wildcard,
    Literal(Literal),
    Ident(String),
    Tuple(Vec<Pattern>),
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Binary(BinaryExpr),
    Unary(UnaryExpr),
    Literal(Literal),
    Ident(String),
    Call(CallExpr),
    Field(FieldExpr),
    Index(IndexExpr),
    Cast(CastExpr),
    Asm(AsmExpr),
    SizeOf(SizeOfExpr),
    OffsetOf(OffsetOfExpr),
    Deref(DerefExpr),
    Ref(RefExpr),
    RefMut(RefExpr),
    StructLiteral { name: String, fields: Vec<(String, Expr)> },
    Tuple(Vec<Expr>),
    Array(Vec<Expr>),
    Range(RangeExpr),
    If(IfExpr),
    Match(MatchExpr),
    Block(Block),
    UnsafeBlock(Block),
    Loop(Block),
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BinaryExpr {
    pub op: BinOp,
    pub left: Box<Expr>,
    pub right: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnaryExpr {
    pub op: UnOp,
    pub operand: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallExpr {
    pub callee: Box<Expr>,
    pub args: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldExpr {
    pub object: Box<Expr>,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexExpr {
    pub object: Box<Expr>,
    pub index: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CastExpr {
    pub expr: Box<Expr>,
    pub ty: Box<TypeExpr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AsmExpr {
    pub template: String,
    pub inputs: Vec<AsmOperand>,
    pub outputs: Vec<AsmOperand>,
    pub clobbers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AsmOperand {
    pub constraint: String,
    pub expr: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SizeOfExpr {
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OffsetOfExpr {
    pub ty: TypeExpr,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DerefExpr {
    pub expr: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RefExpr {
    pub expr: Box<Expr>,
}

pub type RefMutExpr = RefExpr;

#[derive(Debug, Clone, PartialEq)]
pub struct RangeExpr {
    pub start: Option<Box<Expr>>,
    pub end: Option<Box<Expr>>,
    pub inclusive: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfExpr {
    pub condition: Box<Expr>,
    pub then_block: Block,
    pub else_block: Option<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchExpr {
    pub scrutinee: Box<Expr>,
    pub cases: Vec<MatchCase>,
}

// ---------------------------------------------------------------------------
// Operators and literals
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Power,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Assign,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    BitNot,
    Deref,
    Ref,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    String(String),
    Char(char),
    Bool(bool),
    Null,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Name(String),
    Pointer(Box<TypeExpr>),
    Slice(Box<TypeExpr>),
    Array(Box<TypeExpr>, Box<Expr>),
    Tuple(Vec<TypeExpr>),
    Own(Box<TypeExpr>),
    Ref(Box<TypeExpr>),
    RefMut(Box<TypeExpr>),
    Function {
        params: Vec<TypeExpr>,
        ret: Option<Box<TypeExpr>>,
    },
}
