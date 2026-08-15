//! Typed AST produced by semantic analysis.
//!
//! Every expression node carries its resolved Forge type, and every statement
//! and top-level item uses resolved `ty::Type` values instead of raw type
//! expressions.

use crate::sema::ast;
use crate::ty::{Field, Type, Variant};

/// The result of analyzing a module.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedModule {
    pub package: String,
    pub imports: Vec<ast::Import>,
    pub items: Vec<TypedItem>,
    /// Monomorphized generic function instances discovered during type checking.
    pub mono_instances: Vec<MonoInstance>,
    /// Diagnostics reported during analysis.
    pub errors: Vec<super::Error>,
}

/// A monomorphized instance of a generic function.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MonoInstance {
    /// Original generic function name.
    pub function_name: String,
    /// Concrete generic arguments, in the order of the function's generic parameters.
    pub generic_args: Vec<Type>,
    /// A stable mangled name for the specialized function.
    pub mangled_name: String,
}

impl MonoInstance {
    pub fn new(function_name: impl Into<String>, generic_args: Vec<Type>) -> Self {
        let name = function_name.into();
        let mut mangled = name.clone();
        for arg in &generic_args {
            mangled.push('$');
            mangled.push_str(
                &arg.to_string()
                    .replace([' ', '*', '&', '<', '>', ':', ';', ',', '(', ')', '-'], "_"),
            );
        }
        Self {
            function_name: name,
            generic_args,
            mangled_name: mangled,
        }
    }
}

/// A typed top-level item.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedItem {
    Function(TypedFunction),
    Struct {
        name: String,
        generics: Vec<String>,
        fields: Vec<Field>,
    },
    Union {
        name: String,
        generics: Vec<String>,
        fields: Vec<Field>,
    },
    Enum {
        name: String,
        generics: Vec<String>,
        variants: Vec<Variant>,
    },
    ExternFn {
        name: String,
        generics: Vec<String>,
        params: Vec<(String, Type)>,
        ret: Type,
    },
    Const {
        name: String,
        ty: Type,
        value: TypedExpr,
    },
    Embed {
        name: String,
        len: usize,
    },
    Use {
        path: Vec<String>,
        alias: Option<String>,
    },
    Impl {
        target: Type,
        methods: Vec<TypedFunction>,
    },
}

/// A typed function definition or method.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedFunction {
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<(String, Type)>,
    pub ret: Type,
    pub body: Option<TypedBlock>,
    pub is_unsafe: bool,
}

/// A typed block expression / statement block.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedBlock {
    pub stmts: Vec<TypedStmt>,
    /// The type of the block's trailing expression, or `Void`.
    pub ty: Type,
}

/// A typed statement.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedStmt {
    Let {
        name: String,
        ty: Type,
        init: TypedExpr,
        mutable: bool,
    },
    Var {
        name: String,
        ty: Type,
        init: TypedExpr,
    },
    Assign {
        target: TypedExpr,
        value: TypedExpr,
    },
    CompoundAssign {
        target: TypedExpr,
        op: ast::BinOp,
        value: TypedExpr,
    },
    Expr(TypedExpr),
    Return(Option<TypedExpr>),
    If {
        cond: TypedExpr,
        then_block: TypedBlock,
        elifs: Vec<(TypedExpr, TypedBlock)>,
        else_block: Option<TypedBlock>,
    },
    For {
        var: String,
        iter: TypedExpr,
        body: TypedBlock,
    },
    While {
        cond: TypedExpr,
        body: TypedBlock,
    },
    Match {
        scrutinee: TypedExpr,
        cases: Vec<TypedMatchCase>,
    },
    UnsafeBlock(TypedBlock),
    Loop(TypedBlock),
    Break,
    Continue,
}

/// A typed expression, annotated with its resolved Forge type.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedExpr {
    pub kind: TypedExprKind,
    pub ty: Type,
}

impl TypedExpr {
    pub fn new(kind: TypedExprKind, ty: Type) -> Self {
        Self { kind, ty }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypedExprKind {
    Binary {
        op: ast::BinOp,
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
    },
    Unary {
        op: ast::UnOp,
        operand: Box<TypedExpr>,
    },
    Literal(ast::Literal),
    Ident(String),
    Call {
        callee: Box<TypedExpr>,
        args: Vec<TypedExpr>,
        /// Resolved concrete generic arguments, if the callee is a generic function.
        generic_args: Option<Vec<Type>>,
        /// Mangled name for a monomorphized instance, when applicable.
        mangled_name: Option<String>,
    },
    Field {
        object: Box<TypedExpr>,
        field: String,
        field_index: usize,
    },
    Index {
        object: Box<TypedExpr>,
        index: Box<TypedExpr>,
    },
    Cast {
        expr: Box<TypedExpr>,
        ty: Type,
    },
    SizeOf(Type),
    OffsetOf {
        ty: Type,
        field: String,
        field_index: usize,
    },
    Deref(Box<TypedExpr>),
    Ref(Box<TypedExpr>),
    RefMut(Box<TypedExpr>),
    Tuple(Vec<TypedExpr>),
    Array(Vec<TypedExpr>),
    Range {
        start: Option<Box<TypedExpr>>,
        end: Option<Box<TypedExpr>>,
        inclusive: bool,
    },
    If {
        cond: Box<TypedExpr>,
        then_block: TypedBlock,
        else_block: Option<TypedBlock>,
    },
    Match {
        scrutinee: Box<TypedExpr>,
        cases: Vec<TypedMatchCase>,
    },
    Block(TypedBlock),
    Loop(TypedBlock),
    Break,
    Continue,
    StructLiteral {
        name: String,
        fields: Vec<(String, TypedExpr)>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypedMatchCase {
    pub pattern: TypedPattern,
    pub body: TypedBlock,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypedPattern {
    Wildcard,
    Literal(ast::Literal),
    Ident(String),
    Tuple(Vec<TypedPattern>),
}
