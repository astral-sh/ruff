use ruff_db::files::File;
use ruff_python_ast::name::Name;

use crate::semantic_index::definition::Definition;
use crate::semantic_index::symbol::{ScopeId, ScopedSymbolId};
use crate::semantic_index::{module_global_scope, symbol_table, use_def_map};
use crate::{Db, FxOrderSet};

mod display;
mod infer;

pub(crate) use self::infer::{infer_definition_types, infer_expression_types, infer_scope_types};

/// Infer the public type of a symbol (its type as seen from outside its scope).
pub(crate) fn symbol_ty<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol: ScopedSymbolId,
) -> Type<'db> {
    let _span = tracing::trace_span!("symbol_ty", ?symbol).entered();

    let use_def = use_def_map(db, scope);
    definitions_ty(
        db,
        use_def.public_definitions(symbol),
        use_def.public_may_be_unbound(symbol),
    )
}

/// Shorthand for `symbol_ty` that takes a symbol name instead of an ID.
pub(crate) fn symbol_ty_by_name<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
) -> Type<'db> {
    let table = symbol_table(db, scope);
    table
        .symbol_id_by_name(name)
        .map(|symbol| symbol_ty(db, scope, symbol))
        .unwrap_or(Type::Unbound)
}

/// Shorthand for `symbol_ty` that looks up a module-global symbol in a file.
pub(crate) fn module_global_symbol_ty_by_name<'db>(
    db: &'db dyn Db,
    file: File,
    name: &str,
) -> Type<'db> {
    symbol_ty_by_name(db, module_global_scope(db, file), name)
}

/// Infer the type of a [`Definition`].
pub(crate) fn definition_ty<'db>(db: &'db dyn Db, definition: Definition<'db>) -> Type<'db> {
    let inference = infer_definition_types(db, definition);
    inference.definition_ty(definition)
}

/// Infer the combined type of an array of [`Definition`].
/// Will return a union if there are more than definition, or at least one plus the possibility of
/// Unbound.
pub(crate) fn definitions_ty<'db>(
    db: &'db dyn Db,
    definitions: &[Definition<'db>],
    may_be_unbound: bool,
) -> Type<'db> {
    let unbound_iter = if may_be_unbound {
        [Type::Unbound].iter()
    } else {
        [].iter()
    };
    let def_types = definitions.iter().map(|def| definition_ty(db, *def));
    let mut all_types = unbound_iter.copied().chain(def_types);

    let Some(first) = all_types.next() else {
        return Type::Unbound;
    };

    if let Some(second) = all_types.next() {
        let mut builder = UnionTypeBuilder::new(db);
        builder = builder.add(first).add(second);

        for variant in all_types {
            builder = builder.add(variant);
        }

        Type::Union(builder.build())
    } else {
        first
    }
}

/// Unique ID for a type.
#[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub enum Type<'db> {
    /// the dynamic type: a statically-unknown set of values
    Any,
    /// the empty set of values
    Never,
    /// unknown type (no annotation)
    /// equivalent to Any, or possibly to object in strict mode
    Unknown,
    /// name does not exist or is not bound to any value (this represents an error, but with some
    /// leniency options it could be silently resolved to Unknown in some cases)
    Unbound,
    /// the None object (TODO remove this in favor of Instance(types.NoneType)
    None,
    /// a specific function object
    Function(FunctionType<'db>),
    /// a specific module object
    Module(File),
    /// a specific class object
    Class(ClassType<'db>),
    /// the set of Python objects with the given class in their __class__'s method resolution order
    Instance(ClassType<'db>),
    Union(UnionType<'db>),
    Intersection(IntersectionType<'db>),
    IntLiteral(i64),
    // TODO protocols, callable types, overloads, generics, type vars
}

impl<'db> Type<'db> {
    pub const fn is_unbound(&self) -> bool {
        matches!(self, Type::Unbound)
    }

    pub const fn is_unknown(&self) -> bool {
        matches!(self, Type::Unknown)
    }

    #[must_use]
    pub fn member(&self, db: &'db dyn Db, name: &Name) -> Type<'db> {
        match self {
            Type::Any => Type::Any,
            Type::Never => todo!("attribute lookup on Never type"),
            Type::Unknown => Type::Unknown,
            Type::Unbound => Type::Unbound,
            Type::None => todo!("attribute lookup on None type"),
            Type::Function(_) => todo!("attribute lookup on Function type"),
            Type::Module(file) => module_global_symbol_ty_by_name(db, *file, name),
            Type::Class(class) => class.class_member(db, name),
            Type::Instance(_) => {
                // TODO MRO? get_own_instance_member, get_instance_member
                todo!("attribute lookup on Instance type")
            }
            Type::Union(_) => {
                // TODO perform the get_member on each type in the union
                // TODO return the union of those results
                // TODO if any of those results is `None` then include Unknown in the result union
                todo!("attribute lookup on Union type")
            }
            Type::Intersection(_) => {
                // TODO perform the get_member on each type in the intersection
                // TODO return the intersection of those results
                todo!("attribute lookup on Intersection type")
            }
            Type::IntLiteral(_) => {
                // TODO raise error
                Type::Unknown
            }
        }
    }
}

#[salsa::interned]
pub struct FunctionType<'db> {
    /// name of the function at definition
    pub name: Name,

    /// types of all decorators on this function
    decorators: Vec<Type<'db>>,
}

impl<'db> FunctionType<'db> {
    pub fn has_decorator(self, db: &dyn Db, decorator: Type<'_>) -> bool {
        self.decorators(db).contains(&decorator)
    }
}

#[salsa::interned]
pub struct ClassType<'db> {
    /// Name of the class at definition
    pub name: Name,

    /// Types of all class bases
    bases: Vec<Type<'db>>,

    body_scope: ScopeId<'db>,
}

impl<'db> ClassType<'db> {
    /// Returns the class member of this class named `name`.
    ///
    /// The member resolves to a member of the class itself or any of its bases.
    pub fn class_member(self, db: &'db dyn Db, name: &Name) -> Type<'db> {
        let member = self.own_class_member(db, name);
        if !member.is_unbound() {
            return member;
        }

        self.inherited_class_member(db, name)
    }

    /// Returns the inferred type of the class member named `name`.
    pub fn own_class_member(self, db: &'db dyn Db, name: &Name) -> Type<'db> {
        let scope = self.body_scope(db);
        symbol_ty_by_name(db, scope, name)
    }

    pub fn inherited_class_member(self, db: &'db dyn Db, name: &Name) -> Type<'db> {
        for base in self.bases(db) {
            let member = base.member(db, name);
            if !member.is_unbound() {
                return member;
            }
        }

        Type::Unbound
    }
}

#[salsa::interned]
pub struct UnionType<'db> {
    /// the union type includes values in any of these types
    elements: FxOrderSet<Type<'db>>,
}

struct UnionTypeBuilder<'db> {
    elements: FxOrderSet<Type<'db>>,
    db: &'db dyn Db,
}

impl<'db> UnionTypeBuilder<'db> {
    fn new(db: &'db dyn Db) -> Self {
        Self {
            db,
            elements: FxOrderSet::default(),
        }
    }

    /// Adds a type to this union.
    fn add(mut self, ty: Type<'db>) -> Self {
        match ty {
            Type::Union(union) => {
                self.elements.extend(&union.elements(self.db));
            }
            _ => {
                self.elements.insert(ty);
            }
        }

        self
    }

    fn build(self) -> UnionType<'db> {
        UnionType::new(self.db, self.elements)
    }
}

// Negation types aren't expressible in annotations, and are most likely to arise from type
// narrowing along with intersections (e.g. `if not isinstance(...)`), so we represent them
// directly in intersections rather than as a separate type. This sacrifices some efficiency in the
// case where a Not appears outside an intersection (unclear when that could even happen, but we'd
// have to represent it as a single-element intersection if it did) in exchange for better
// efficiency in the within-intersection case.
#[salsa::interned]
pub struct IntersectionType<'db> {
    // the intersection type includes only values in all of these types
    positive: FxOrderSet<Type<'db>>,
    // the intersection type does not include any value in any of these types
    negative: FxOrderSet<Type<'db>>,
}
