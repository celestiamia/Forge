use super::*;

pub(super) fn zero_expr(ty: &Type) -> TypedExpr {
    let kind = match ty {
        Type::Bool => TypedExprKind::Literal(Literal::Bool(false)),
        Type::Char => TypedExprKind::Literal(Literal::Char('\0')),
        Type::Pointer { .. } | Type::USize | Type::ISize => TypedExprKind::Literal(Literal::Null),
        _ => TypedExprKind::Literal(Literal::Int(0)),
    };
    TypedExpr::new(kind, ty.clone())
}

pub(crate) fn primitive_type(name: &str) -> Option<Type> {
    match name {
        "void" => Some(Type::Void),
        "bool" => Some(Type::Bool),
        "char" => Some(Type::Char),
        "usize" => Some(Type::USize),
        "isize" => Some(Type::ISize),
        "i8" | "int8" => Some(Type::Int {
            width: 8,
            signed: true,
        }),
        "i16" | "int16" => Some(Type::Int {
            width: 16,
            signed: true,
        }),
        "i32" | "int" | "int32" => Some(Type::Int {
            width: 32,
            signed: true,
        }),
        "i64" | "int64" => Some(Type::Int {
            width: 64,
            signed: true,
        }),
        "i128" | "int128" => Some(Type::Int {
            width: 128,
            signed: true,
        }),
        "u8" | "uint8" | "byte" => Some(Type::Int {
            width: 8,
            signed: false,
        }),
        "u16" | "uint16" => Some(Type::Int {
            width: 16,
            signed: false,
        }),
        "u32" | "uint32" => Some(Type::Int {
            width: 32,
            signed: false,
        }),
        "u64" | "uint64" => Some(Type::Int {
            width: 64,
            signed: false,
        }),
        "u128" | "uint128" => Some(Type::Int {
            width: 128,
            signed: false,
        }),
        "f32" | "float32" => Some(Type::Float { width: 32 }),
        "f64" | "float" | "float64" => Some(Type::Float { width: 64 }),
        _ => None,
    }
}

pub(super) fn literal_type(lit: &Literal, expected: Option<&Type>) -> Type {
    match lit {
        Literal::Int(_) => expected
            .filter(|t| t.is_integer())
            .cloned()
            .unwrap_or(Type::Int {
                width: 32,
                signed: true,
            }),
        Literal::Float(_) => expected
            .filter(|t| t.is_float())
            .cloned()
            .unwrap_or(Type::Float { width: 64 }),
        Literal::Bool(_) => Type::Bool,
        Literal::Char(_) => Type::Char,
        Literal::String(_) => Type::pointer(Type::Char),
        Literal::Null => expected
            .filter(|t| t.is_pointer())
            .cloned()
            .unwrap_or(Type::pointer(Type::Unknown)),
    }
}

pub(super) fn adt_type(info: &AdtInfo) -> Type {
    match info.kind {
        AdtKind::Struct => Type::Struct {
            name: info.name.clone(),
            fields: info.fields.clone(),
        },
        AdtKind::Union => Type::Union {
            name: info.name.clone(),
            fields: info.fields.clone(),
        },
        AdtKind::Enum => Type::Enum {
            name: info.name.clone(),
            variants: info.variants.clone(),
        },
    }
}

pub(super) fn base_type_name(ty: &Type) -> String {
    match ty {
        Type::Struct { name, .. } | Type::Union { name, .. } | Type::Enum { name, .. } => {
            name.clone()
        }
        Type::Pointer { pointee } => base_type_name(pointee),
        Type::Ref { pointee } | Type::RefMut { pointee } | Type::Own { pointee } => {
            base_type_name(pointee)
        }
        _ => String::new(),
    }
}

pub(super) fn base_type_name_from_type_expr(tx: &TypeExpr) -> String {
    match tx {
        TypeExpr::Name(name) => name.clone(),
        TypeExpr::Pointer(inner) | TypeExpr::Slice(inner) => base_type_name_from_type_expr(inner),
        _ => String::new(),
    }
}

pub(super) fn adt_name_from_type(ty: &Type) -> Option<String> {
    match ty {
        Type::Struct { name, .. } | Type::Union { name, .. } | Type::Enum { name, .. } => {
            Some(name.clone())
        }
        Type::Pointer { pointee } => adt_name_from_type(pointee),
        Type::Ref { pointee } | Type::RefMut { pointee } => adt_name_from_type(pointee),
        _ => None,
    }
}

pub(super) fn field_index(ty: &Type, field: &str, ctx: &Context) -> usize {
    let name = base_type_name(ty);
    if let Some(info) = ctx.adts.get(&name) {
        return info
            .fields
            .iter()
            .position(|f| f.name == field)
            .unwrap_or(0);
    }
    0
}

pub(super) fn op_name(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::And => "&&",
        BinOp::Or => "||",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::Assign => "=",
        BinOp::FloorDiv => "//",
        BinOp::Power => "**",
    }
}

pub(super) fn ast_binop_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+=",
        BinOp::Sub => "-=",
        BinOp::Mul => "*=",
        BinOp::Div => "/=",
        BinOp::Mod => "%=",
        BinOp::BitAnd => "&=",
        BinOp::BitOr => "|=",
        BinOp::BitXor => "^=",
        BinOp::Shl => "<<=",
        BinOp::Shr => ">>=",
        _ => "=",
    }
}

pub(super) fn cast_allowed(from: &Type, to: &Type) -> bool {
    if from == to || from.is_unknown() || to.is_unknown() {
        return true;
    }
    match (from, to) {
        (a, b) if a.is_numeric() && b.is_numeric() => true,
        (Type::Pointer { .. }, Type::Pointer { .. }) => true,
        (Type::Pointer { .. }, Type::Int { .. }) | (Type::Int { .. }, Type::Pointer { .. }) => true,
        (Type::Pointer { .. }, Type::USize) | (Type::USize, Type::Pointer { .. }) => true,
        (Type::Pointer { .. }, Type::ISize) | (Type::ISize, Type::Pointer { .. }) => true,
        (Type::Bool, Type::Int { .. }) | (Type::Int { .. }, Type::Bool) => true,
        // Array to pointer decay
        (Type::Array { elem, .. }, Type::Pointer { pointee }) => {
            **elem == **pointee || is_layout_compatible_8bit(elem, pointee)
        }
        _ => false,
    }
}

pub(super) fn compatible(expected: &Type, got: &Type) -> bool {
    if expected == got || expected.is_unknown() || got.is_unknown() {
        return true;
    }
    if matches!(expected, Type::Generic { .. }) {
        return true;
    }
    // Struct/union/enum identity is by monomorphized name; field lists may
    // be empty when the type came from substitution rather than the ADT
    // table.
    if let (Type::Struct { name: en, .. }, Type::Struct { name: gn, .. }) = (expected, got)
        && en == gn
    {
        return true;
    }
    if let (Type::Union { name: en, .. }, Type::Union { name: gn, .. }) = (expected, got)
        && en == gn
    {
        return true;
    }
    if let (Type::Enum { name: en, .. }, Type::Enum { name: gn, .. }) = (expected, got)
        && en == gn
    {
        return true;
    }
    // Allow reference/owned values to coerce to raw pointers, and permit
    // layout-compatible 8-bit pointees (char/uint8/int8) to intermix.
    if let Type::Pointer { pointee: ep } = expected {
        let got_pointee = match got {
            Type::Pointer { pointee } => Some(pointee.as_ref()),
            Type::Ref { pointee } | Type::RefMut { pointee } | Type::Own { pointee } => {
                Some(pointee.as_ref())
            }
            // Array to pointer decay
            Type::Array { elem, .. } => Some(elem.as_ref()),
            _ => None,
        };
        if let Some(gp) = got_pointee {
            if **ep == *gp {
                return true;
            }
            if is_layout_compatible_8bit(ep, gp) {
                return true;
            }
        }
    }
    false
}

pub(super) fn is_layout_compatible_8bit(a: &Type, b: &Type) -> bool {
    let is_8bit_integer =
        |t: &Type| matches!(t, Type::Int { width: 8, .. } | Type::Char | Type::Bool);
    is_8bit_integer(a) && is_8bit_integer(b)
}

pub(crate) fn substitute(ty: &Type, mapping: &HashMap<String, Type>) -> Type {
    match ty {
        Type::Generic { name } => mapping.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::Pointer { pointee } => Type::pointer(substitute(pointee, mapping)),
        Type::Own { pointee } => Type::own(substitute(pointee, mapping)),
        Type::Ref { pointee } => Type::refr(substitute(pointee, mapping)),
        Type::RefMut { pointee } => Type::ref_mut(substitute(pointee, mapping)),
        Type::Slice { elem } => Type::slice(substitute(elem, mapping)),
        Type::Array { elem, size } => Type::array(substitute(elem, mapping), *size),
        Type::Tuple { fields } => {
            Type::tuple(fields.iter().map(|f| substitute(f, mapping)).collect())
        }
        Type::Function { params, ret } => Type::function(
            params.iter().map(|p| substitute(p, mapping)).collect(),
            substitute(ret, mapping),
        ),
        // A generic struct application defers instantiation until its type
        // arguments are concrete; once they are, the monomorphized struct
        // name can be computed.  The concrete fields are registered by the
        // checker (which owns the ADT table) via `finalize_struct_app`.
        Type::StructApp { base, args } => {
            let args: Vec<Type> = args.iter().map(|a| substitute(a, mapping)).collect();
            if args
                .iter()
                .any(|a| a.is_generic() || matches!(a, Type::StructApp { .. }))
            {
                Type::StructApp {
                    base: base.clone(),
                    args,
                }
            } else {
                Type::Struct {
                    name: crate::ty::mono_struct_name(base, &args),
                    fields: Vec::new(),
                }
            }
        }
        _ => ty.clone(),
    }
}

// AdtDefinition trait to share struct/union registration code
