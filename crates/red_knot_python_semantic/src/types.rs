use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;

use crate::semantic_index::symbol::{NodeWithScopeKind, PublicSymbolId, ScopeId};
use crate::semantic_index::{public_symbol, root_scope, semantic_index, symbol_table};
use crate::types::infer::{TypeInference, TypeInferenceBuilder};
use crate::{Db, FxOrderSet};

mod display;
mod infer;

/// Infers the type of a public symbol.
///
/// This is a Salsa query to get symbol-level invalidation instead of file-level dependency invalidation.
/// Without this being a query, changing any public type of a module would invalidate the type inference
/// for the module scope of its dependents and the transitive dependents because.
///
/// For example if we have
/// ```python
/// # a.py
/// import x from b
///
/// # b.py
///
/// x = 20
/// ```
///
/// And x is now changed from `x = 20` to `x = 30`. The following happens:
///
/// * The module level types of `b.py` change because `x` now is a `Literal[30]`.
/// * The module level types of `a.py` change because the imported symbol `x` now has a `Literal[30]` type
/// * The module level types of any dependents of `a.py` change because the imported symbol `x` now has a `Literal[30]` type
/// * And so on for all transitive dependencies.
///
/// This being a query ensures that the invalidation short-circuits if the type of this symbol didn't change.
#[salsa::tracked]
pub(crate) fn public_symbol_ty<'db>(db: &'db dyn Db, symbol: PublicSymbolId<'db>) -> Type<'db> {
    let _span = tracing::trace_span!("public_symbol_ty", ?symbol).entered();

    let file = symbol.file(db);
    let scope = root_scope(db, file);

    // TODO switch to inferring just the definition(s), not the whole scope
    let inference = infer_types(db, scope);
    inference.symbol_ty(symbol.scoped_symbol_id(db))
}

/// Shorthand for `public_symbol_ty` that takes a symbol name instead of a [`PublicSymbolId`].
pub(crate) fn public_symbol_ty_by_name<'db>(
    db: &'db dyn Db,
    file: File,
    name: &str,
) -> Option<Type<'db>> {
    let symbol = public_symbol(db, file, name)?;
    Some(public_symbol_ty(db, symbol))
}

/// Infers all types for `scope`.
#[salsa::tracked(return_ref)]
pub(crate) fn infer_types<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> TypeInference<'db> {
    let _span = tracing::trace_span!("infer_types", ?scope).entered();

    let file = scope.file(db);
    // Using the index here is fine because the code below depends on the AST anyway.
    // The isolation of the query is by the return inferred types.
    let index = semantic_index(db, file);

    let node = scope.node(db);

    let mut context = TypeInferenceBuilder::new(db, scope, index);

    match node {
        NodeWithScopeKind::Module => {
            let parsed = parsed_module(db.upcast(), file);
            context.infer_module(parsed.syntax());
        }
        NodeWithScopeKind::Function(function) => context.infer_function_body(function.node()),
        NodeWithScopeKind::Class(class) => context.infer_class_body(class.node()),
        NodeWithScopeKind::ClassTypeParameters(class) => {
            context.infer_class_type_params(class.node());
        }
        NodeWithScopeKind::FunctionTypeParameters(function) => {
            context.infer_function_type_params(function.node());
        }
    }

    context.finish()
}

/// unique ID for a type
#[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub enum Type<'db> {
    /// the dynamic type: a statically-unknown set of values
    Any,
    /// the empty set of values
    Never,
    /// unknown type (no annotation)
    /// equivalent to Any, or to object in strict mode
    Unknown,
    /// name is not bound to any value
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

    pub fn member(&self, db: &'db dyn Db, name: &Name) -> Option<Type<'db>> {
        match self {
            Type::Any => Some(Type::Any),
            Type::Never => todo!("attribute lookup on Never type"),
            Type::Unknown => Some(Type::Unknown),
            Type::Unbound => todo!("attribute lookup on Unbound type"),
            Type::None => todo!("attribute lookup on None type"),
            Type::Function(_) => todo!("attribute lookup on Function type"),
            Type::Module(file) => public_symbol_ty_by_name(db, *file, name),
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
                Some(Type::Unknown)
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
    pub fn class_member(self, db: &'db dyn Db, name: &Name) -> Option<Type<'db>> {
        if let Some(member) = self.own_class_member(db, name) {
            return Some(member);
        }

        self.inherited_class_member(db, name)
    }

    /// Returns the inferred type of the class member named `name`.
    pub fn own_class_member(self, db: &'db dyn Db, name: &Name) -> Option<Type<'db>> {
        let scope = self.body_scope(db);
        let symbols = symbol_table(db, scope);
        let symbol = symbols.symbol_id_by_name(name)?;
        let types = infer_types(db, scope);

        Some(types.symbol_ty(symbol))
    }

    pub fn inherited_class_member(self, db: &'db dyn Db, name: &Name) -> Option<Type<'db>> {
        for base in self.bases(db) {
            if let Some(member) = base.member(db, name) {
                return Some(member);
            }
        }

        None
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

#[cfg(test)]
mod tests {
    use red_knot_module_resolver::{
        set_module_resolution_settings, RawModuleResolutionSettings, TargetVersion,
    };
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};

    use crate::db::tests::{
        assert_will_not_run_function_query, assert_will_run_function_query, TestDb,
    };
    use crate::semantic_index::root_scope;
    use crate::types::{infer_types, public_symbol_ty_by_name};
    use crate::{HasTy, SemanticModel};

    fn setup_db() -> TestDb {
        let mut db = TestDb::new();
        set_module_resolution_settings(
            &mut db,
            RawModuleResolutionSettings {
                target_version: TargetVersion::Py38,
                extra_paths: vec![],
                workspace_root: SystemPathBuf::from("/src"),
                site_packages: None,
                custom_typeshed: None,
            },
        );

        db
    }

    #[test]
    fn local_inference() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("/src/a.py", "x = 10")?;
        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        let parsed = parsed_module(&db, a);

        let statement = parsed.suite().first().unwrap().as_assign_stmt().unwrap();
        let model = SemanticModel::new(&db, a);

        let literal_ty = statement.value.ty(&model);

        assert_eq!(format!("{}", literal_ty.display(&db)), "Literal[10]");

        Ok(())
    }

    #[test]
    fn dependency_public_symbol_type_change() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("/src/a.py", "from foo import x"),
            ("/src/foo.py", "x = 10\ndef foo(): ..."),
        ])?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let x_ty = public_symbol_ty_by_name(&db, a, "x").unwrap();

        assert_eq!(x_ty.display(&db).to_string(), "Literal[10]");

        // Change `x` to a different value
        db.write_file("/src/foo.py", "x = 20\ndef foo(): ...")?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        db.clear_salsa_events();
        let x_ty_2 = public_symbol_ty_by_name(&db, a, "x").unwrap();

        assert_eq!(x_ty_2.display(&db).to_string(), "Literal[20]");

        let events = db.take_salsa_events();

        let a_root_scope = root_scope(&db, a);
        assert_will_run_function_query::<infer_types, _, _>(
            &db,
            |ty| &ty.function,
            &a_root_scope,
            &events,
        );

        Ok(())
    }

    #[test]
    fn dependency_non_public_symbol_change() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("/src/a.py", "from foo import x"),
            ("/src/foo.py", "x = 10\ndef foo(): y = 1"),
        ])?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let x_ty = public_symbol_ty_by_name(&db, a, "x").unwrap();

        assert_eq!(x_ty.display(&db).to_string(), "Literal[10]");

        db.write_file("/src/foo.py", "x = 10\ndef foo(): pass")?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        db.clear_salsa_events();

        let x_ty_2 = public_symbol_ty_by_name(&db, a, "x").unwrap();

        assert_eq!(x_ty_2.display(&db).to_string(), "Literal[10]");

        let events = db.take_salsa_events();

        let a_root_scope = root_scope(&db, a);

        assert_will_not_run_function_query::<infer_types, _, _>(
            &db,
            |ty| &ty.function,
            &a_root_scope,
            &events,
        );

        Ok(())
    }

    #[test]
    fn dependency_unrelated_public_symbol() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("/src/a.py", "from foo import x"),
            ("/src/foo.py", "x = 10\ny = 20"),
        ])?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let x_ty = public_symbol_ty_by_name(&db, a, "x").unwrap();

        assert_eq!(x_ty.display(&db).to_string(), "Literal[10]");

        db.write_file("/src/foo.py", "x = 10\ny = 30")?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        db.clear_salsa_events();

        let x_ty_2 = public_symbol_ty_by_name(&db, a, "x").unwrap();

        assert_eq!(x_ty_2.display(&db).to_string(), "Literal[10]");

        let events = db.take_salsa_events();

        let a_root_scope = root_scope(&db, a);
        assert_will_not_run_function_query::<infer_types, _, _>(
            &db,
            |ty| &ty.function,
            &a_root_scope,
            &events,
        );
        Ok(())
    }
}
