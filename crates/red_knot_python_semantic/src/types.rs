use ruff_db::files::File;
use ruff_python_ast::name::Name;

use crate::builtins::builtins_scope;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::symbol::{ScopeId, ScopedSymbolId};
use crate::semantic_index::{
    global_scope, semantic_index, symbol_table, use_def_map, DefinitionWithConstraints,
    DefinitionWithConstraintsIterator,
};
use crate::types::narrow::narrowing_constraint;
use crate::{Db, FxOrderSet};

pub(crate) use self::builder::{IntersectionBuilder, UnionBuilder};
pub(crate) use self::diagnostic::TypeCheckDiagnostics;
pub(crate) use self::infer::{
    infer_definition_types, infer_expression_types, infer_scope_types, TypeInference,
};

mod builder;
mod diagnostic;
mod display;
mod infer;
mod narrow;

pub fn check_types(db: &dyn Db, file: File) -> TypeCheckDiagnostics {
    let _span = tracing::trace_span!("check_types", file=?file.path(db)).entered();

    let index = semantic_index(db, file);
    let mut diagnostics = TypeCheckDiagnostics::new();

    for scope_id in index.scope_ids() {
        let result = infer_scope_types(db, scope_id);
        diagnostics.extend(result.diagnostics());
    }

    diagnostics
}

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
    definitions_with_constraints: DefinitionWithConstraintsIterator<'_, 'db>,
    unbound_ty: Option<Type<'db>>,
) -> Type<'db> {
    let def_types = definitions_with_constraints.map(
        |DefinitionWithConstraints {
             definition,
             constraints,
         }| {
            let mut constraint_tys =
                constraints.filter_map(|test| narrowing_constraint(db, test, definition));
            let definition_ty = definition_ty(db, definition);
            if let Some(first_constraint_ty) = constraint_tys.next() {
                let mut builder = IntersectionBuilder::new(db);
                builder = builder
                    .add_positive(definition_ty)
                    .add_positive(first_constraint_ty);
                for constraint_ty in constraint_tys {
                    builder = builder.add_positive(constraint_ty);
                }
                builder.build()
            } else {
                definition_ty
            }
        },
    );
    let mut all_types = unbound_ty.into_iter().chain(def_types);

    let Some(first) = all_types.next() else {
        panic!("definitions_ty should never be called with zero definitions and no unbound_ty.")
    };

    if let Some(second) = all_types.next() {
        let mut builder = UnionBuilder::new(db);
        builder = builder.add(first).add(second);

        for variant in all_types {
            builder = builder.add(variant);
        }

        builder.build()
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
    /// unknown type (either no annotation, or some kind of type error)
    /// equivalent to Any, or possibly to object in strict mode
    Unknown,
    /// name does not exist or is not bound to any value (this represents an error, but with some
    /// leniency options it could be silently resolved to Unknown in some cases)
    Unbound,
    /// the None object -- TODO remove this in favor of Instance(types.NoneType)
    None,
    /// a specific function object
    Function(FunctionType<'db>),
    /// a specific module object
    Module(File),
    /// a specific class object
    Class(ClassType<'db>),
    /// the set of Python objects with the given class in their __class__'s method resolution order
    Instance(ClassType<'db>),
    /// the set of objects in any of the types in the union
    Union(UnionType<'db>),
    /// the set of objects in all of the types in the intersection
    Intersection(IntersectionType<'db>),
    /// An integer literal
    IntLiteral(i64),
    /// A boolean literal, either `True` or `False`.
    BooleanLiteral(bool),
    // TODO protocols, callable types, overloads, generics, type vars
}

impl<'db> Type<'db> {
    pub const fn is_unbound(&self) -> bool {
        matches!(self, Type::Unbound)
    }

    pub const fn is_unknown(&self) -> bool {
        matches!(self, Type::Unknown)
    }

    pub const fn is_never(&self) -> bool {
        matches!(self, Type::Never)
    }

    pub fn may_be_unbound(&self, db: &'db dyn Db) -> bool {
        match self {
            Type::Unbound => true,
            Type::Union(union) => union.contains(db, Type::Unbound),
            // Unbound can't appear in an intersection, because an intersection with Unbound
            // simplifies to just Unbound.
            _ => false,
        }
    }

    #[must_use]
    pub fn replace_unbound_with(&self, db: &'db dyn Db, replacement: Type<'db>) -> Type<'db> {
        match self {
            Type::Unbound => replacement,
            Type::Union(union) => union
                .elements(db)
                .into_iter()
                .fold(UnionBuilder::new(db), |builder, ty| {
                    builder.add(ty.replace_unbound_with(db, replacement))
                })
                .build(),
            ty => *ty,
        }
    }

    #[must_use]
    pub fn member(&self, db: &'db dyn Db, name: &Name) -> Type<'db> {
        match self {
            Type::Any => Type::Any,
            Type::Never => {
                // TODO: attribute lookup on Never type
                Type::Unknown
            }
            Type::Unknown => Type::Unknown,
            Type::Unbound => Type::Unbound,
            Type::None => {
                // TODO: attribute lookup on None type
                Type::Unknown
            }
            Type::Function(_) => {
                // TODO: attribute lookup on function type
                Type::Unknown
            }
            Type::Module(file) => global_symbol_ty_by_name(db, *file, name),
            Type::Class(class) => class.class_member(db, name),
            Type::Instance(_) => {
                // TODO MRO? get_own_instance_member, get_instance_member
                Type::Unknown
            }
            Type::Union(union) => union
                .elements(db)
                .iter()
                .fold(UnionBuilder::new(db), |builder, element_ty| {
                    builder.add(element_ty.member(db, name))
                })
                .build(),
            Type::Intersection(_) => {
                // TODO perform the get_member on each type in the intersection
                // TODO return the intersection of those results
                Type::Unknown
            }
            Type::IntLiteral(_) => {
                // TODO raise error
                Type::Unknown
            }
            Type::BooleanLiteral(_) => Type::Unknown,
        }
    }

    #[must_use]
    pub fn instance(&self) -> Type<'db> {
        match self {
            Type::Any => Type::Any,
            Type::Unknown => Type::Unknown,
            Type::Class(class) => Type::Instance(*class),
            _ => Type::Unknown, // TODO type errors
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
    /// The union type includes values in any of these types.
    elements: FxOrderSet<Type<'db>>,
}

impl<'db> UnionType<'db> {
    pub fn contains(&self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        self.elements(db).contains(&ty)
    }
}

#[salsa::interned]
pub struct IntersectionType<'db> {
    /// The intersection type includes only values in all of these types.
    positive: FxOrderSet<Type<'db>>,

    /// The intersection type does not include any value in any of these types.
    ///
    /// Negation types aren't expressible in annotations, and are most likely to arise from type
    /// narrowing along with intersections (e.g. `if not isinstance(...)`), so we represent them
    /// directly in intersections rather than as a separate type.
    negative: FxOrderSet<Type<'db>>,
}

#[cfg(test)]
mod tests {
    use anyhow::Context;

    use ruff_db::files::system_path_to_file;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};

    use crate::db::tests::TestDb;
    use crate::{Program, ProgramSettings, PythonVersion, SearchPathSettings};

    use super::TypeCheckDiagnostics;

    fn setup_db() -> TestDb {
        let db = TestDb::new();
        db.memory_file_system()
            .create_directory_all("/src")
            .unwrap();

        Program::from_settings(
            &db,
            ProgramSettings {
                target_version: PythonVersion::default(),
                search_paths: SearchPathSettings {
                    extra_paths: Vec::new(),
                    src_root: SystemPathBuf::from("/src"),
                    site_packages: vec![],
                    custom_typeshed: None,
                },
            },
        )
        .expect("Valid search path settings");

        db
    }

    fn assert_diagnostic_messages(diagnostics: &TypeCheckDiagnostics, expected: &[&str]) {
        let messages: Vec<&str> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message())
            .collect();
        assert_eq!(&messages, expected);
    }

    #[test]
    fn check_types() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("src/foo.py", "import bar\n")
            .context("Failed to write foo.py")?;

        let foo = system_path_to_file(&db, "src/foo.py").context("Failed to resolve foo.py")?;

        let diagnostics = super::check_types(&db, foo);
        assert_diagnostic_messages(&diagnostics, &["Import 'bar' could not be resolved."]);

        Ok(())
    }

    #[test]
    fn unresolved_import() {
        let mut db = setup_db();

        db.write_files([
            ("/src/a.py", "import foo as foo"),
            ("/src/b.py", "from a import foo"),
        ])
        .unwrap();

        let a_file = system_path_to_file(&db, "/src/a.py").expect("Expected `a.py` to exist!");
        let a_file_diagnostics = super::check_types(&db, a_file);
        assert_diagnostic_messages(
            &a_file_diagnostics,
            &["Import 'foo' could not be resolved."],
        );

        // Importing the unresolved import into a second first-party file does not trigger
        // an additional "unresolved import" violation
        let b_file = system_path_to_file(&db, "/src/b.py").expect("Expected `by.py` to exist!");
        let b_file_diagnostics = super::check_types(&db, b_file);
        assert_eq!(&*b_file_diagnostics, &[]);
    }

    #[test]
    fn unresolved_import_from_resolved_module() {
        let mut db = setup_db();

        db.write_files([("/src/a.py", ""), ("/src/b.py", "from a import thing")])
            .unwrap();

        let b_file = system_path_to_file(&db, "/src/b.py").expect("Expected `b.py` to exist");
        let b_file_diagnostics = super::check_types(&db, b_file);
        assert_diagnostic_messages(
            &b_file_diagnostics,
            &["Could not resolve import of 'thing' from 'a'"],
        );
    }
}
