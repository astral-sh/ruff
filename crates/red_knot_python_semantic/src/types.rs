use ruff_db::parsed::parsed_module;
use ruff_db::vfs::VfsFile;
use ruff_index::newtype_index;
use ruff_python_ast::name::Name;

use crate::semantic_index::symbol::{FileScopeId, NodeWithScopeKind, PublicSymbolId, ScopeId};
use crate::semantic_index::{public_symbol, root_scope, semantic_index, symbol_table};
use crate::types::infer::{TypeInference, TypeInferenceBuilder};
use crate::Db;
use crate::FxIndexSet;

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

    let inference = infer_types(db, scope);
    inference.symbol_ty(symbol.scoped_symbol_id(db))
}

/// Shorthand for [`public_symbol_ty()`] that takes a symbol name instead of a [`PublicSymbolId`].
#[allow(unused)]
pub(crate) fn public_symbol_ty_by_name<'db>(
    db: &'db dyn Db,
    file: VfsFile,
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
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
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
    Function(TypeId<'db, ScopedFunctionTypeId>),
    /// a specific module object
    Module(TypeId<'db, ScopedModuleTypeId>),
    /// a specific class object
    Class(TypeId<'db, ScopedClassTypeId>),
    /// the set of Python objects with the given class in their __class__'s method resolution order
    Instance(TypeId<'db, ScopedClassTypeId>),
    Union(TypeId<'db, ScopedUnionTypeId>),
    Intersection(TypeId<'db, ScopedIntersectionTypeId>),
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

    pub fn member(&self, context: &TypingContext<'db, '_>, name: &Name) -> Option<Type<'db>> {
        match self {
            Type::Any => Some(Type::Any),
            Type::Never => todo!("attribute lookup on Never type"),
            Type::Unknown => Some(Type::Unknown),
            Type::Unbound => todo!("attribute lookup on Unbound type"),
            Type::None => todo!("attribute lookup on None type"),
            Type::Function(_) => todo!("attribute lookup on Function type"),
            Type::Module(module) => module.member(context, name),
            Type::Class(class) => class.class_member(context, name),
            Type::Instance(_) => {
                // TODO MRO? get_own_instance_member, get_instance_member
                todo!("attribute lookup on Instance type")
            }
            Type::Union(union_id) => {
                let _union = union_id.lookup(context);
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

/// ID that uniquely identifies a type in a program.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct TypeId<'db, L> {
    /// The scope in which this type is defined or was created.
    scope: ScopeId<'db>,
    /// The type's local ID in its scope.
    scoped: L,
}

impl<'db, Id> TypeId<'db, Id>
where
    Id: Copy,
{
    pub fn scope(&self) -> ScopeId<'db> {
        self.scope
    }

    pub fn scoped_id(&self) -> Id {
        self.scoped
    }

    /// Resolves the type ID to the actual type.
    pub(crate) fn lookup<'a>(self, context: &'a TypingContext<'db, 'a>) -> &'a Id::Ty<'db>
    where
        Id: ScopedTypeId,
    {
        let types = context.types(self.scope);
        self.scoped.lookup_scoped(types)
    }
}

/// ID that uniquely identifies a type in a scope.
pub(crate) trait ScopedTypeId {
    /// The type that this ID points to.
    type Ty<'db>;

    /// Looks up the type in `index`.
    ///
    /// ## Panics
    /// May panic if this type is from another scope than `index`, or might just return an invalid type.
    fn lookup_scoped<'a, 'db>(self, index: &'a TypeInference<'db>) -> &'a Self::Ty<'db>;
}

/// ID uniquely identifying a function type in a `scope`.
#[newtype_index]
pub struct ScopedFunctionTypeId;

impl ScopedTypeId for ScopedFunctionTypeId {
    type Ty<'db> = FunctionType<'db>;

    fn lookup_scoped<'a, 'db>(self, types: &'a TypeInference<'db>) -> &'a Self::Ty<'db> {
        types.function_ty(self)
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct FunctionType<'a> {
    /// name of the function at definition
    name: Name,
    /// types of all decorators on this function
    decorators: Vec<Type<'a>>,
}

impl<'a> FunctionType<'a> {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    #[allow(unused)]
    pub(crate) fn decorators(&self) -> &[Type<'a>] {
        self.decorators.as_slice()
    }
}

impl<'db> TypeId<'db, ScopedFunctionTypeId> {
    pub fn name<'a>(self, context: &'a TypingContext<'db, 'a>) -> &'a Name {
        let function_ty = self.lookup(context);
        &function_ty.name
    }

    pub fn has_decorator(self, context: &TypingContext, decorator: Type<'db>) -> bool {
        let function_ty = self.lookup(context);
        function_ty.decorators.contains(&decorator)
    }
}

#[newtype_index]
pub struct ScopedClassTypeId;

impl ScopedTypeId for ScopedClassTypeId {
    type Ty<'db> = ClassType<'db>;

    fn lookup_scoped<'a, 'db>(self, types: &'a TypeInference<'db>) -> &'a Self::Ty<'db> {
        types.class_ty(self)
    }
}

impl<'db> TypeId<'db, ScopedClassTypeId> {
    pub fn name<'a>(self, context: &'a TypingContext<'db, 'a>) -> &'a Name {
        let class_ty = self.lookup(context);
        &class_ty.name
    }

    /// Returns the class member of this class named `name`.
    ///
    /// The member resolves to a member of the class itself or any of its bases.
    pub fn class_member(self, context: &TypingContext<'db, '_>, name: &Name) -> Option<Type<'db>> {
        if let Some(member) = self.own_class_member(context, name) {
            return Some(member);
        }

        self.inherited_class_member(context, name)
    }

    /// Returns the inferred type of the class member named `name`.
    pub fn own_class_member(
        self,
        context: &TypingContext<'db, '_>,
        name: &Name,
    ) -> Option<Type<'db>> {
        let class = self.lookup(context);

        let symbols = symbol_table(context.db, class.body_scope);
        let symbol = symbols.symbol_id_by_name(name)?;
        let types = context.types(class.body_scope);

        Some(types.symbol_ty(symbol))
    }

    pub fn inherited_class_member(
        self,
        context: &TypingContext<'db, '_>,
        name: &Name,
    ) -> Option<Type<'db>> {
        let class = self.lookup(context);
        for base in &class.bases {
            if let Some(member) = base.member(context, name) {
                return Some(member);
            }
        }

        None
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ClassType<'db> {
    /// Name of the class at definition
    name: Name,

    /// Types of all class bases
    bases: Vec<Type<'db>>,

    body_scope: ScopeId<'db>,
}

impl<'db> ClassType<'db> {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    #[allow(unused)]
    pub(super) fn bases(&self) -> &'db [Type] {
        self.bases.as_slice()
    }
}

#[newtype_index]
pub struct ScopedUnionTypeId;

impl ScopedTypeId for ScopedUnionTypeId {
    type Ty<'db> = UnionType<'db>;

    fn lookup_scoped<'a, 'db>(self, types: &'a TypeInference<'db>) -> &'a Self::Ty<'db> {
        types.union_ty(self)
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct UnionType<'db> {
    // the union type includes values in any of these types
    elements: FxIndexSet<Type<'db>>,
}

struct UnionTypeBuilder<'db, 'a> {
    elements: FxIndexSet<Type<'db>>,
    context: &'a TypingContext<'db, 'a>,
}

impl<'db, 'a> UnionTypeBuilder<'db, 'a> {
    fn new(context: &'a TypingContext<'db, 'a>) -> Self {
        Self {
            context,
            elements: FxIndexSet::default(),
        }
    }

    /// Adds a type to this union.
    fn add(mut self, ty: Type<'db>) -> Self {
        match ty {
            Type::Union(union_id) => {
                let union = union_id.lookup(self.context);
                self.elements.extend(&union.elements);
            }
            _ => {
                self.elements.insert(ty);
            }
        }

        self
    }

    fn build(self) -> UnionType<'db> {
        UnionType {
            elements: self.elements,
        }
    }
}

#[newtype_index]
pub struct ScopedIntersectionTypeId;

impl ScopedTypeId for ScopedIntersectionTypeId {
    type Ty<'db> = IntersectionType<'db>;

    fn lookup_scoped<'a, 'db>(self, types: &'a TypeInference<'db>) -> &'a Self::Ty<'db> {
        types.intersection_ty(self)
    }
}

// Negation types aren't expressible in annotations, and are most likely to arise from type
// narrowing along with intersections (e.g. `if not isinstance(...)`), so we represent them
// directly in intersections rather than as a separate type. This sacrifices some efficiency in the
// case where a Not appears outside an intersection (unclear when that could even happen, but we'd
// have to represent it as a single-element intersection if it did) in exchange for better
// efficiency in the within-intersection case.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct IntersectionType<'db> {
    // the intersection type includes only values in all of these types
    positive: FxIndexSet<Type<'db>>,
    // the intersection type does not include any value in any of these types
    negative: FxIndexSet<Type<'db>>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ScopedModuleTypeId;

impl ScopedTypeId for ScopedModuleTypeId {
    type Ty<'db> = ModuleType;

    fn lookup_scoped<'a, 'db>(self, types: &'a TypeInference<'db>) -> &'a Self::Ty<'db> {
        types.module_ty()
    }
}

impl<'db> TypeId<'db, ScopedModuleTypeId> {
    fn member(self, context: &TypingContext<'db, '_>, name: &Name) -> Option<Type<'db>> {
        context.public_symbol_ty(self.scope.file(context.db), name)
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ModuleType {
    file: VfsFile,
}

/// Context in which to resolve types.
///
/// This abstraction is necessary to support a uniform API that can be used
/// while in the process of building the type inference structure for a scope
/// but also when all types should be resolved by querying the db.
pub struct TypingContext<'db, 'inference> {
    db: &'db dyn Db,

    /// The Local type inference scope that is in the process of being built.
    ///
    /// Bypass the `db` when resolving the types for this scope.
    local: Option<(ScopeId<'db>, &'inference TypeInference<'db>)>,
}

impl<'db, 'inference> TypingContext<'db, 'inference> {
    /// Creates a context that resolves all types by querying the db.
    #[allow(unused)]
    pub(super) fn global(db: &'db dyn Db) -> Self {
        Self { db, local: None }
    }

    /// Creates a context that by-passes the `db` when resolving types from `scope_id` and instead uses `types`.
    fn scoped(
        db: &'db dyn Db,
        scope_id: ScopeId<'db>,
        types: &'inference TypeInference<'db>,
    ) -> Self {
        Self {
            db,
            local: Some((scope_id, types)),
        }
    }

    /// Returns the [`TypeInference`] results (not guaranteed to be complete) for `scope_id`.
    fn types(&self, scope_id: ScopeId<'db>) -> &'inference TypeInference<'db> {
        if let Some((scope, local_types)) = self.local {
            if scope == scope_id {
                return local_types;
            }
        }

        infer_types(self.db, scope_id)
    }

    fn module_ty(&self, file: VfsFile) -> Type<'db> {
        let scope = root_scope(self.db, file);

        Type::Module(TypeId {
            scope,
            scoped: ScopedModuleTypeId,
        })
    }

    /// Resolves the public type of a symbol named `name` defined in `file`.
    ///
    /// This function calls [`public_symbol_ty`] if the local scope isn't the module scope of `file`.
    /// It otherwise tries to resolve the symbol type locally.
    fn public_symbol_ty(&self, file: VfsFile, name: &Name) -> Option<Type<'db>> {
        let symbol = public_symbol(self.db, file, name)?;

        if let Some((scope, local_types)) = self.local {
            if scope.file_scope_id(self.db) == FileScopeId::root() && scope.file(self.db) == file {
                return Some(local_types.symbol_ty(symbol.scoped_symbol_id(self.db)));
            }
        }

        Some(public_symbol_ty(self.db, symbol))
    }
}

#[cfg(test)]
mod tests {
    use red_knot_module_resolver::{set_module_resolution_settings, ModuleResolutionSettings};
    use ruff_db::file_system::FileSystemPathBuf;
    use ruff_db::parsed::parsed_module;
    use ruff_db::vfs::system_path_to_file;

    use crate::db::tests::{
        assert_will_not_run_function_query, assert_will_run_function_query, TestDb,
    };
    use crate::semantic_index::root_scope;
    use crate::types::{infer_types, public_symbol_ty_by_name, TypingContext};
    use crate::{HasTy, SemanticModel};

    fn setup_db() -> TestDb {
        let mut db = TestDb::new();
        set_module_resolution_settings(
            &mut db,
            ModuleResolutionSettings {
                extra_paths: vec![],
                workspace_root: FileSystemPathBuf::from("/src"),
                site_packages: None,
                custom_typeshed: None,
            },
        );

        db
    }

    #[test]
    fn local_inference() -> anyhow::Result<()> {
        let db = setup_db();

        db.memory_file_system().write_file("/src/a.py", "x = 10")?;
        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        let parsed = parsed_module(&db, a);

        let statement = parsed.suite().first().unwrap().as_assign_stmt().unwrap();
        let model = SemanticModel::new(&db, a);

        let literal_ty = statement.value.ty(&model);

        assert_eq!(
            format!("{}", literal_ty.display(&TypingContext::global(&db))),
            "Literal[10]"
        );

        Ok(())
    }

    #[test]
    fn dependency_public_symbol_type_change() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.memory_file_system().write_files([
            ("/src/a.py", "from foo import x"),
            ("/src/foo.py", "x = 10\ndef foo(): ..."),
        ])?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let x_ty = public_symbol_ty_by_name(&db, a, "x").unwrap();

        assert_eq!(
            x_ty.display(&TypingContext::global(&db)).to_string(),
            "Literal[10]"
        );

        // Change `x` to a different value
        db.memory_file_system()
            .write_file("/src/foo.py", "x = 20\ndef foo(): ...")?;

        let foo = system_path_to_file(&db, "/src/foo.py").unwrap();
        foo.touch(&mut db);

        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        db.clear_salsa_events();
        let x_ty_2 = public_symbol_ty_by_name(&db, a, "x").unwrap();

        assert_eq!(
            x_ty_2.display(&TypingContext::global(&db)).to_string(),
            "Literal[20]"
        );

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

        db.memory_file_system().write_files([
            ("/src/a.py", "from foo import x"),
            ("/src/foo.py", "x = 10\ndef foo(): y = 1"),
        ])?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let x_ty = public_symbol_ty_by_name(&db, a, "x").unwrap();

        assert_eq!(
            x_ty.display(&TypingContext::global(&db)).to_string(),
            "Literal[10]"
        );

        db.memory_file_system()
            .write_file("/src/foo.py", "x = 10\ndef foo(): pass")?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let foo = system_path_to_file(&db, "/src/foo.py").unwrap();

        foo.touch(&mut db);

        db.clear_salsa_events();

        let x_ty_2 = public_symbol_ty_by_name(&db, a, "x").unwrap();

        assert_eq!(
            x_ty_2.display(&TypingContext::global(&db)).to_string(),
            "Literal[10]"
        );

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

        db.memory_file_system().write_files([
            ("/src/a.py", "from foo import x"),
            ("/src/foo.py", "x = 10\ny = 20"),
        ])?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let x_ty = public_symbol_ty_by_name(&db, a, "x").unwrap();

        assert_eq!(
            x_ty.display(&TypingContext::global(&db)).to_string(),
            "Literal[10]"
        );

        db.memory_file_system()
            .write_file("/src/foo.py", "x = 10\ny = 30")?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let foo = system_path_to_file(&db, "/src/foo.py").unwrap();

        foo.touch(&mut db);

        db.clear_salsa_events();

        let x_ty_2 = public_symbol_ty_by_name(&db, a, "x").unwrap();

        assert_eq!(
            x_ty_2.display(&TypingContext::global(&db)).to_string(),
            "Literal[10]"
        );

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
