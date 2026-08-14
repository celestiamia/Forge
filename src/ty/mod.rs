use std::collections::{HashMap, HashSet};
use std::fmt;

/// A field definition for struct/union types.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Field {
    pub name: String,
    pub ty: Type,
}

/// A variant definition for enum types.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Variant {
    pub name: String,
    /// Optional payload type for the variant.
    pub payload: Option<Type>,
}

/// The Forge type system.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Void,
    Bool,
    Int {
        width: u32,
        signed: bool,
    },
    Float {
        width: u32,
    },
    Char,
    USize,
    ISize,
    Pointer {
        pointee: Box<Type>,
    },
    Slice {
        elem: Box<Type>,
    },
    Array {
        elem: Box<Type>,
        size: u64,
    },
    Tuple {
        fields: Vec<Type>,
    },
    Struct {
        name: String,
        fields: Vec<Field>,
    },
    Union {
        name: String,
        fields: Vec<Field>,
    },
    Enum {
        name: String,
        variants: Vec<Variant>,
    },
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    Own {
        pointee: Box<Type>,
    },
    Ref {
        pointee: Box<Type>,
    },
    RefMut {
        pointee: Box<Type>,
    },
    Generic {
        name: String,
    },
    Unknown,
}

impl Type {
    /// Convenience constructor for `Int`.
    pub fn int(width: u32, signed: bool) -> Self {
        Type::Int { width, signed }
    }

    /// Convenience constructor for `Float`.
    #[allow(dead_code)]
    pub fn float(width: u32) -> Self {
        Type::Float { width }
    }

    /// Convenience constructor for `Pointer`.
    pub fn pointer(pointee: Type) -> Self {
        Type::Pointer {
            pointee: Box::new(pointee),
        }
    }

    /// Convenience constructor for `Slice`.
    pub fn slice(elem: Type) -> Self {
        Type::Slice {
            elem: Box::new(elem),
        }
    }

    /// Convenience constructor for `Array`.
    pub fn array(elem: Type, size: u64) -> Self {
        Type::Array {
            elem: Box::new(elem),
            size,
        }
    }

    /// Convenience constructor for `Tuple`.
    pub fn tuple(fields: Vec<Type>) -> Self {
        Type::Tuple { fields }
    }

    /// Convenience constructor for `Function`.
    pub fn function(params: Vec<Type>, ret: Type) -> Self {
        Type::Function {
            params,
            ret: Box::new(ret),
        }
    }

    /// Convenience constructor for `Own`.
    pub fn own(pointee: Type) -> Self {
        Type::Own {
            pointee: Box::new(pointee),
        }
    }

    /// Convenience constructor for `Ref`.
    pub fn refr(pointee: Type) -> Self {
        Type::Ref {
            pointee: Box::new(pointee),
        }
    }

    /// Convenience constructor for `RefMut`.
    pub fn ref_mut(pointee: Type) -> Self {
        Type::RefMut {
            pointee: Box::new(pointee),
        }
    }

    /// Convenience constructor for `Generic`.
    #[allow(dead_code)]
    pub fn generic(name: impl Into<String>) -> Self {
        Type::Generic { name: name.into() }
    }

    /// Returns true for the `Unknown` type.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Type::Unknown)
    }

    /// Returns true for integer-like types (signed/unsigned integers, `char`, `bool`, `usize`, `isize`).
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Type::Int { .. } | Type::Char | Type::USize | Type::ISize
        )
    }

    /// Returns true for floating-point types.
    pub fn is_float(&self) -> bool {
        matches!(self, Type::Float { .. })
    }

    /// Returns true for numeric types (integers or floats).
    pub fn is_numeric(&self) -> bool {
        self.is_integer() || self.is_float()
    }

    /// Returns true for signed integer-like types.
    #[allow(dead_code)]
    pub fn is_signed(&self) -> bool {
        matches!(self, Type::Int { signed: true, .. } | Type::ISize)
    }

    /// Returns the bit width for scalar types, if known.
    #[allow(dead_code)]
    pub fn width(&self) -> Option<u32> {
        match self {
            Type::Int { width, .. } | Type::Float { width } => Some(*width),
            Type::Char | Type::Bool => Some(8),
            Type::USize | Type::ISize => Some(64), // target-dependent placeholder
            _ => None,
        }
    }

    /// Returns true for raw pointer types (`Pointer`).
    pub fn is_pointer(&self) -> bool {
        matches!(self, Type::Pointer { .. })
    }

    /// Returns true for reference types (`Ref` or `RefMut`).
    #[allow(dead_code)]
    pub fn is_reference(&self) -> bool {
        matches!(self, Type::Ref { .. } | Type::RefMut { .. })
    }

    /// Returns true for mutable references.
    pub fn is_mutable_reference(&self) -> bool {
        matches!(self, Type::RefMut { .. })
    }

    /// Returns the pointee type for pointer, owned, and reference types.
    #[allow(dead_code)]
    pub fn pointee(&self) -> Option<&Type> {
        match self {
            Type::Pointer { pointee }
            | Type::Own { pointee }
            | Type::Ref { pointee }
            | Type::RefMut { pointee } => Some(pointee),
            _ => None,
        }
    }

    /// Returns true for the `Void` type.
    pub fn is_void(&self) -> bool {
        matches!(self, Type::Void)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Void => write!(f, "void"),
            Type::Bool => write!(f, "bool"),
            Type::Int {
                width,
                signed: true,
            } => write!(f, "i{width}"),
            Type::Int {
                width,
                signed: false,
            } => write!(f, "u{width}"),
            Type::Float { width } => write!(f, "f{width}"),
            Type::Char => write!(f, "char"),
            Type::USize => write!(f, "usize"),
            Type::ISize => write!(f, "isize"),
            Type::Pointer { pointee } => write!(f, "*{pointee}"),
            Type::Slice { elem } => write!(f, "[{elem}]"),
            Type::Array { elem, size } => write!(f, "[{elem}; {size}]"),
            Type::Tuple { fields } if fields.is_empty() => write!(f, "()"),
            Type::Tuple { fields } => {
                write!(f, "(")?;
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{field}")?;
                }
                write!(f, ")")
            }
            Type::Struct { name, fields } => {
                write!(f, "struct {name}")?;
                if !fields.is_empty() {
                    write!(f, " {{ ")?;
                    for (i, field) in fields.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}: {}", field.name, field.ty)?;
                    }
                    write!(f, " }}")?;
                }
                Ok(())
            }
            Type::Union { name, fields } => {
                write!(f, "union {name}")?;
                if !fields.is_empty() {
                    write!(f, " {{ ")?;
                    for (i, field) in fields.iter().enumerate() {
                        if i > 0 {
                            write!(f, " | ")?;
                        }
                        write!(f, "{}: {}", field.name, field.ty)?;
                    }
                    write!(f, " }}")?;
                }
                Ok(())
            }
            Type::Enum { name, variants } => {
                write!(f, "enum {name}")?;
                if !variants.is_empty() {
                    write!(f, " {{ ")?;
                    for (i, variant) in variants.iter().enumerate() {
                        if i > 0 {
                            write!(f, " | ")?;
                        }
                        match &variant.payload {
                            Some(payload) => write!(f, "{}({})", variant.name, payload)?,
                            None => write!(f, "{}", variant.name)?,
                        }
                    }
                    write!(f, " }}")?;
                }
                Ok(())
            }
            Type::Function { params, ret } => {
                write!(f, "fn(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{param}")?;
                }
                write!(f, ") -> {ret}")
            }
            Type::Own { pointee } => write!(f, "own<{pointee}>"),
            Type::Ref { pointee } => write!(f, "&{pointee}"),
            Type::RefMut { pointee } => write!(f, "&mut {pointee}"),
            Type::Generic { name } => write!(f, "{name}"),
            Type::Unknown => write!(f, "?"),
        }
    }
}

/// Interning context for types and name-to-type bindings.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct TypeCtx {
    interner: HashSet<Type>,
    names: HashMap<String, Type>,
}

#[allow(dead_code)]
impl TypeCtx {
    /// Create a new, empty type context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a type, returning the canonical representative.
    ///
    /// If the same type value already exists in the context, the existing value is
    /// returned so that equality comparisons reflect structural sharing.
    pub fn intern(&mut self, ty: Type) -> Type {
        if let Some(existing) = self.interner.get(&ty) {
            existing.clone()
        } else {
            self.interner.insert(ty.clone());
            ty
        }
    }

    /// Look up a type bound to `name`.
    pub fn resolve(&self, name: &str) -> Option<Type> {
        self.names.get(name).cloned()
    }

    /// Bind a name to a type in this context.
    ///
    /// The type is interned before being stored.
    pub fn bind(&mut self, name: impl Into<String>, ty: Type) {
        let canonical = self.intern(ty);
        self.names.insert(name.into(), canonical);
    }

    /// Returns the number of interned types.
    pub fn len(&self) -> usize {
        self.interner.len()
    }

    /// Returns true if no types have been interned.
    pub fn is_empty(&self) -> bool {
        self.interner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_primitives() {
        assert_eq!(Type::Void.to_string(), "void");
        assert_eq!(Type::Bool.to_string(), "bool");
        assert_eq!(Type::int(32, true).to_string(), "i32");
        assert_eq!(Type::int(32, false).to_string(), "u32");
        assert_eq!(Type::float(64).to_string(), "f64");
        assert_eq!(Type::Char.to_string(), "char");
        assert_eq!(Type::USize.to_string(), "usize");
        assert_eq!(Type::ISize.to_string(), "isize");
        assert_eq!(Type::Unknown.to_string(), "?");
    }

    #[test]
    fn display_composite() {
        let ptr = Type::pointer(Type::int(8, true));
        assert_eq!(ptr.to_string(), "*i8");

        let slice = Type::slice(Type::int(8, false));
        assert_eq!(slice.to_string(), "[u8]");

        let arr = Type::array(Type::int(32, true), 10);
        assert_eq!(arr.to_string(), "[i32; 10]");

        let tup = Type::tuple(vec![Type::Bool, Type::int(32, true)]);
        assert_eq!(tup.to_string(), "(bool, i32)");
        assert_eq!(Type::tuple(vec![]).to_string(), "()");
    }

    #[test]
    fn display_ownership() {
        let own = Type::own(Type::int(32, true));
        assert_eq!(own.to_string(), "own<i32>");

        let refr = Type::refr(Type::int(32, true));
        assert_eq!(refr.to_string(), "&i32");

        let rmut = Type::ref_mut(Type::int(32, true));
        assert_eq!(rmut.to_string(), "&mut i32");
    }

    #[test]
    fn display_function() {
        let f = Type::function(vec![Type::int(32, true), Type::Bool], Type::Void);
        assert_eq!(f.to_string(), "fn(i32, bool) -> void");
    }

    #[test]
    fn display_generic() {
        let g = Type::generic("T");
        assert_eq!(g.to_string(), "T");
    }

    #[test]
    fn display_adt() {
        let s = Type::Struct {
            name: "Point".into(),
            fields: vec![
                Field {
                    name: "x".into(),
                    ty: Type::int(32, true),
                },
                Field {
                    name: "y".into(),
                    ty: Type::int(32, true),
                },
            ],
        };
        assert_eq!(s.to_string(), "struct Point { x: i32, y: i32 }");

        let u = Type::Union {
            name: "Value".into(),
            fields: vec![
                Field {
                    name: "i".into(),
                    ty: Type::int(32, true),
                },
                Field {
                    name: "b".into(),
                    ty: Type::Bool,
                },
            ],
        };
        assert_eq!(u.to_string(), "union Value { i: i32 | b: bool }");

        let e = Type::Enum {
            name: "Option".into(),
            variants: vec![
                Variant {
                    name: "None".into(),
                    payload: None,
                },
                Variant {
                    name: "Some".into(),
                    payload: Some(Type::generic("T")),
                },
            ],
        };
        assert_eq!(e.to_string(), "enum Option { None | Some(T) }");
    }

    #[test]
    fn interning_deduplicates() {
        let mut ctx = TypeCtx::new();

        let t1 = ctx.intern(Type::int(32, true));
        let t2 = ctx.intern(Type::int(32, true));
        assert_eq!(t1, t2);
        assert_eq!(ctx.len(), 1);

        let t3 = ctx.intern(Type::int(64, true));
        assert_ne!(t1, t3);
        assert_eq!(ctx.len(), 2);

        let ptr1 = ctx.intern(Type::pointer(t1.clone()));
        let ptr2 = ctx.intern(Type::pointer(t2.clone()));
        assert_eq!(ptr1, ptr2);
        assert_eq!(ctx.len(), 3);
    }

    #[test]
    fn name_resolution() {
        let mut ctx = TypeCtx::new();
        ctx.bind("I32", Type::int(32, true));
        ctx.bind("Bool", Type::Bool);

        assert_eq!(ctx.resolve("I32"), Some(Type::int(32, true)));
        assert_eq!(ctx.resolve("Bool"), Some(Type::Bool));
        assert_eq!(ctx.resolve("Missing"), None);
    }

    #[test]
    fn hash_and_eq_for_interning() {
        let a = Type::pointer(Type::int(32, true));
        let b = Type::pointer(Type::int(32, true));
        assert_eq!(a, b);

        // Verify that the derived Hash is consistent with Eq by using a HashSet.
        let mut set = HashSet::new();
        set.insert(a.clone());
        assert!(set.contains(&b));
    }
}
