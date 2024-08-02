use ruff_db::files::File;
use ruff_python_ast::name::Name;

use crate::builtins::builtins_scope;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::symbol::{ScopeId, ScopedSymbolId};
use crate::semantic_index::{global_scope, symbol_table, use_def_map};
use crate::{Db, FxOrderSet};

mod display;
mod infer;

pub(crate) use self::infer::{infer_definition_types, infer_scope_types};

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
        use_def
            .public_may_be_unbound(symbol)
            .then_some(Type::Unbound),
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

/// Shorthand for `symbol_ty` that looks up a module-global symbol by name in a file.
pub(crate) fn global_symbol_ty_by_name<'db>(db: &'db dyn Db, file: File, name: &str) -> Type<'db> {
    symbol_ty_by_name(db, global_scope(db, file), name)
}

/// Shorthand for `symbol_ty` that looks up a symbol in the builtins.
///
/// Returns `Unbound` if the builtins module isn't available for some reason.
pub(crate) fn builtins_symbol_ty_by_name<'db>(db: &'db dyn Db, name: &str) -> Type<'db> {
    builtins_scope(db)
        .map(|builtins| symbol_ty_by_name(db, builtins, name))
        .unwrap_or(Type::Unbound)
}

/// Infer the type of a [`Definition`].
pub(crate) fn definition_ty<'db>(db: &'db dyn Db, definition: Definition<'db>) -> Type<'db> {
    let inference = infer_definition_types(db, definition);
    inference.definition_ty(definition)
}

/// Infer the combined type of an array of [`Definition`]s, plus one optional "unbound type".
///
/// Will return a union if there is more than one definition, or at least one plus an unbound
/// type.
///
/// The "unbound type" represents the type in case control flow may not have passed through any
/// definitions in this scope. If this isn't possible, then it will be `None`. If it is possible,
/// and the result in that case should be Unbound (e.g. an unbound function local), then it will be
/// `Some(Type::Unbound)`. If it is possible and the result should be something else (e.g. an
/// implicit global lookup), then `unbound_type` will be `Some(the_global_symbol_type)`.
///
/// # Panics
/// Will panic if called with zero definitions and no `unbound_ty`. This is a logic error,
/// as any symbol with zero visible definitions clearly may be unbound, and the caller should
/// provide an `unbound_ty`.
pub(crate) fn definitions_ty<'db>(
    db: &'db dyn Db,
    definitions: &[Definition<'db>],
    unbound_ty: Option<Type<'db>>,
) -> Type<'db> {
    let def_types = definitions.iter().map(|def| definition_ty(db, *def));
    let mut all_types = unbound_ty.into_iter().chain(def_types);

    let Some(first) = all_types.next() else {
        panic!("definitions_ty should never be called with zero definitions and no unbound_ty.")
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
            Type::Module(file) => global_symbol_ty_by_name(db, *file, name),
            Type::Class(class) => class.class_member(db, name),
            Type::Instance(_) => {
                // TODO MRO? get_own_instance_member, get_instance_member
                todo!("attribute lookup on Instance type")
            }
            Type::Union(union) => Type::Union(
                union
                    .elements(db)
                    .iter()
                    .fold(UnionTypeBuilder::new(db), |builder, element_ty| {
                        builder.add(element_ty.member(db, name))
                    })
                    .build(),
            ),
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

impl<'db> UnionType<'db> {
    pub fn contains(&self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        self.elements(db).contains(&ty)
    }
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
