use infer::TypeInferenceBuilder;
use ruff_db::files::File;
use ruff_python_ast as ast;

use crate::module_resolver::file_to_module;
use crate::semantic_index::ast_ids::HasScopedAstId;
use crate::semantic_index::definition::{Definition, DefinitionKind};
use crate::semantic_index::symbol::{ScopeId, ScopedSymbolId};
use crate::semantic_index::{
    global_scope, semantic_index, symbol_table, use_def_map, BindingWithConstraints,
    BindingWithConstraintsIterator,
};
use crate::stdlib::{builtins_symbol_ty, types_symbol_ty, typeshed_symbol_ty};
use crate::types::narrow::narrowing_constraint;
use crate::{Db, FxOrderSet};

pub(crate) use self::builder::{IntersectionBuilder, UnionBuilder};
pub(crate) use self::diagnostic::TypeCheckDiagnostics;
pub(crate) use self::infer::{
    infer_deferred_types, infer_definition_types, infer_expression_types, infer_scope_types,
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
pub(crate) fn symbol_ty_by_id<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol: ScopedSymbolId,
) -> Type<'db> {
    let _span = tracing::trace_span!("symbol_ty_by_id", ?symbol).entered();

    let use_def = use_def_map(db, scope);

    if use_def.has_public_declarations(symbol) {
        UnionType::from_elements(
            db,
            use_def
                .public_declarations(symbol)
                .map(|decl| declaration_ty(db, decl)),
        )
    } else {
        bindings_ty(
            db,
            use_def.public_bindings(symbol),
            use_def
                .public_may_be_unbound(symbol)
                .then_some(Type::Unbound),
        )
    }
}

/// Shorthand for `symbol_ty` that takes a symbol name instead of an ID.
pub(crate) fn symbol_ty<'db>(db: &'db dyn Db, scope: ScopeId<'db>, name: &str) -> Type<'db> {
    let table = symbol_table(db, scope);
    table
        .symbol_id_by_name(name)
        .map(|symbol| symbol_ty_by_id(db, scope, symbol))
        .unwrap_or(Type::Unbound)
}

/// Shorthand for `symbol_ty` that looks up a module-global symbol by name in a file.
pub(crate) fn global_symbol_ty<'db>(db: &'db dyn Db, file: File, name: &str) -> Type<'db> {
    symbol_ty(db, global_scope(db, file), name)
}

/// Infer the type of a binding.
pub(crate) fn binding_ty<'db>(db: &'db dyn Db, definition: Definition<'db>) -> Type<'db> {
    let inference = infer_definition_types(db, definition);
    inference.binding_ty(definition)
}

/// Infer the type of a declaration.
pub(crate) fn declaration_ty<'db>(db: &'db dyn Db, definition: Definition<'db>) -> Type<'db> {
    let inference = infer_definition_types(db, definition);
    inference.declaration_ty(definition)
}

/// Infer the type of a (possibly deferred) sub-expression of a [`Definition`].
///
/// ## Panics
/// If the given expression is not a sub-expression of the given [`Definition`].
pub(crate) fn definition_expression_ty<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    expression: &ast::Expr,
) -> Type<'db> {
    let expr_id = expression.scoped_ast_id(db, definition.scope(db));
    let inference = infer_definition_types(db, definition);
    if let Some(ty) = inference.try_expression_ty(expr_id) {
        ty
    } else {
        infer_deferred_types(db, definition).expression_ty(expr_id)
    }
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
pub(crate) fn bindings_ty<'db>(
    db: &'db dyn Db,
    bindings_with_constraints: BindingWithConstraintsIterator<'_, 'db>,
    unbound_ty: Option<Type<'db>>,
) -> Type<'db> {
    let def_types = bindings_with_constraints.map(
        |BindingWithConstraints {
             binding,
             constraints,
         }| {
            let mut constraint_tys =
                constraints.filter_map(|constraint| narrowing_constraint(db, constraint, binding));
            let binding_ty = binding_ty(db, binding);
            if let Some(first_constraint_ty) = constraint_tys.next() {
                let mut builder = IntersectionBuilder::new(db);
                builder = builder
                    .add_positive(binding_ty)
                    .add_positive(first_constraint_ty);
                for constraint_ty in constraint_tys {
                    builder = builder.add_positive(constraint_ty);
                }
                builder.build()
            } else {
                binding_ty
            }
        },
    );
    let mut all_types = unbound_ty.into_iter().chain(def_types);

    let first = all_types
        .next()
        .expect("bindings_ty should never be called with zero definitions and no unbound_ty.");

    if let Some(second) = all_types.next() {
        UnionType::from_elements(db, [first, second].into_iter().chain(all_types))
    } else {
        first
    }
}

/// Unique ID for a type.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
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
    /// A string literal
    StringLiteral(StringLiteralType<'db>),
    /// A string known to originate only from literal values, but whose value is not known (unlike
    /// `StringLiteral` above).
    LiteralString,
    /// A bytes literal
    BytesLiteral(BytesLiteralType<'db>),
    /// A heterogeneous tuple type, with elements of the given types in source order.
    // TODO: Support variable length homogeneous tuple type like `tuple[int, ...]`.
    Tuple(TupleType<'db>),
    // TODO protocols, callable types, overloads, generics, type vars
}

impl<'db> Type<'db> {
    pub const fn is_unbound(&self) -> bool {
        matches!(self, Type::Unbound)
    }

    pub const fn is_never(&self) -> bool {
        matches!(self, Type::Never)
    }

    pub const fn into_class_type(self) -> Option<ClassType<'db>> {
        match self {
            Type::Class(class_type) => Some(class_type),
            _ => None,
        }
    }

    pub fn expect_class(self) -> ClassType<'db> {
        self.into_class_type()
            .expect("Expected a Type::Class variant")
    }

    pub const fn into_module_type(self) -> Option<File> {
        match self {
            Type::Module(file) => Some(file),
            _ => None,
        }
    }

    pub fn expect_module(self) -> File {
        self.into_module_type()
            .expect("Expected a Type::Module variant")
    }

    pub const fn into_union_type(self) -> Option<UnionType<'db>> {
        match self {
            Type::Union(union_type) => Some(union_type),
            _ => None,
        }
    }

    pub fn expect_union(self) -> UnionType<'db> {
        self.into_union_type()
            .expect("Expected a Type::Union variant")
    }

    pub const fn into_intersection_type(self) -> Option<IntersectionType<'db>> {
        match self {
            Type::Intersection(intersection_type) => Some(intersection_type),
            _ => None,
        }
    }

    pub fn expect_intersection(self) -> IntersectionType<'db> {
        self.into_intersection_type()
            .expect("Expected a Type::Intersection variant")
    }

    pub const fn into_function_type(self) -> Option<FunctionType<'db>> {
        match self {
            Type::Function(function_type) => Some(function_type),
            _ => None,
        }
    }

    pub fn expect_function(self) -> FunctionType<'db> {
        self.into_function_type()
            .expect("Expected a Type::Function variant")
    }

    pub const fn into_int_literal_type(self) -> Option<i64> {
        match self {
            Type::IntLiteral(value) => Some(value),
            _ => None,
        }
    }

    pub fn expect_int_literal(self) -> i64 {
        self.into_int_literal_type()
            .expect("Expected a Type::IntLiteral variant")
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
            Type::Union(union) => {
                union.map(db, |element| element.replace_unbound_with(db, replacement))
            }
            ty => *ty,
        }
    }

    /// Return true if this type is [assignable to] type `target`.
    ///
    /// [assignable to]: https://typing.readthedocs.io/en/latest/spec/concepts.html#the-assignable-to-or-consistent-subtyping-relation
    pub(crate) fn is_assignable_to(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        if self.is_equivalent_to(db, target) {
            return true;
        }
        match (self, target) {
            (Type::Unknown | Type::Any | Type::Never, _) => true,
            (_, Type::Unknown | Type::Any) => true,
            (Type::IntLiteral(_), Type::Instance(class))
                if class.is_stdlib_symbol(db, "builtins", "int") =>
            {
                true
            }
            (Type::StringLiteral(_), Type::LiteralString) => true,
            (Type::StringLiteral(_) | Type::LiteralString, Type::Instance(class))
                if class.is_stdlib_symbol(db, "builtins", "str") =>
            {
                true
            }
            (Type::BytesLiteral(_), Type::Instance(class))
                if class.is_stdlib_symbol(db, "builtins", "bytes") =>
            {
                true
            }
            // TODO
            _ => false,
        }
    }

    /// Return true if this type is equivalent to type `other`.
    pub(crate) fn is_equivalent_to(self, _db: &'db dyn Db, other: Type<'db>) -> bool {
        // TODO equivalent but not identical structural types, differently-ordered unions and
        // intersections, other cases?
        self == other
    }

    /// Resolve a member access of a type.
    ///
    /// For example, if `foo` is `Type::Instance(<Bar>)`,
    /// `foo.member(&db, "baz")` returns the type of `baz` attributes
    /// as accessed from instances of the `Bar` class.
    ///
    /// TODO: use of this method currently requires manually checking
    /// whether the returned type is `Unknown`/`Unbound`
    /// (or a union with `Unknown`/`Unbound`) in many places.
    /// Ideally we'd use a more type-safe pattern, such as returning
    /// an `Option` or a `Result` from this method, which would force
    /// us to explicitly consider whether to handle an error or propagate
    /// it up the call stack.
    #[must_use]
    pub fn member(&self, db: &'db dyn Db, name: &str) -> Type<'db> {
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
            Type::Module(file) => global_symbol_ty(db, *file, name),
            Type::Class(class) => class.class_member(db, name),
            Type::Instance(_) => {
                // TODO MRO? get_own_instance_member, get_instance_member
                Type::Unknown
            }
            Type::Union(union) => union.map(db, |element| element.member(db, name)),
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
            Type::StringLiteral(_) => {
                // TODO defer to `typing.LiteralString`/`builtins.str` methods
                // from typeshed's stubs
                Type::Unknown
            }
            Type::LiteralString => {
                // TODO defer to `typing.LiteralString`/`builtins.str` methods
                // from typeshed's stubs
                Type::Unknown
            }
            Type::BytesLiteral(_) => {
                // TODO defer to Type::Instance(<bytes from typeshed>).member
                Type::Unknown
            }
            Type::Tuple(_) => {
                // TODO: implement tuple methods
                Type::Unknown
            }
        }
    }

    /// Return the type resulting from calling an object of this type.
    ///
    /// Returns `None` if `self` is not a callable type.
    #[must_use]
    pub fn call(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Type::Function(function_type) => Some(function_type.return_type(db)),

            // TODO annotated return type on `__new__` or metaclass `__call__`
            Type::Class(class) => Some(Type::Instance(*class)),

            // TODO: handle classes which implement the Callable protocol
            Type::Instance(_instance_ty) => Some(Type::Unknown),

            // `Any` is callable, and its return type is also `Any`.
            Type::Any => Some(Type::Any),

            Type::Unknown => Some(Type::Unknown),

            // TODO: union and intersection types, if they reduce to `Callable`
            Type::Union(_) => Some(Type::Unknown),
            Type::Intersection(_) => Some(Type::Unknown),

            _ => None,
        }
    }

    /// Given the type of an object that is iterated over in some way,
    /// return the type of objects that are yielded by that iteration.
    ///
    /// E.g., for the following loop, given the type of `x`, infer the type of `y`:
    /// ```python
    /// for y in x:
    ///     pass
    /// ```
    fn iterate(&self, db: &'db dyn Db) -> IterationOutcome<'db> {
        if let Type::Tuple(tuple_type) = self {
            return IterationOutcome::Iterable {
                element_ty: UnionType::from_elements(db, &**tuple_type.elements(db)),
            };
        }

        // `self` represents the type of the iterable;
        // `__iter__` and `__next__` are both looked up on the class of the iterable:
        let iterable_meta_type = self.to_meta_type(db);

        let dunder_iter_method = iterable_meta_type.member(db, "__iter__");
        if !dunder_iter_method.is_unbound() {
            let Some(iterator_ty) = dunder_iter_method.call(db) else {
                return IterationOutcome::NotIterable {
                    not_iterable_ty: *self,
                };
            };

            let dunder_next_method = iterator_ty.to_meta_type(db).member(db, "__next__");
            return dunder_next_method
                .call(db)
                .map(|element_ty| IterationOutcome::Iterable { element_ty })
                .unwrap_or(IterationOutcome::NotIterable {
                    not_iterable_ty: *self,
                });
        }

        // Although it's not considered great practice,
        // classes that define `__getitem__` are also iterable,
        // even if they do not define `__iter__`.
        //
        // TODO(Alex) this is only valid if the `__getitem__` method is annotated as
        // accepting `int` or `SupportsIndex`
        let dunder_get_item_method = iterable_meta_type.member(db, "__getitem__");

        dunder_get_item_method
            .call(db)
            .map(|element_ty| IterationOutcome::Iterable { element_ty })
            .unwrap_or(IterationOutcome::NotIterable {
                not_iterable_ty: *self,
            })
    }

    #[must_use]
    pub fn to_instance(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::Any => Type::Any,
            Type::Unknown => Type::Unknown,
            Type::Unbound => Type::Unknown,
            Type::Never => Type::Never,
            Type::Class(class) => Type::Instance(*class),
            Type::Union(union) => union.map(db, |element| element.to_instance(db)),
            // TODO: we can probably do better here: --Alex
            Type::Intersection(_) => Type::Unknown,
            // TODO: calling `.to_instance()` on any of these should result in a diagnostic,
            // since they already indicate that the object is an instance of some kind:
            Type::BooleanLiteral(_)
            | Type::BytesLiteral(_)
            | Type::Function(_)
            | Type::Instance(_)
            | Type::Module(_)
            | Type::IntLiteral(_)
            | Type::StringLiteral(_)
            | Type::Tuple(_)
            | Type::LiteralString
            | Type::None => Type::Unknown,
        }
    }

    /// Given a type that is assumed to represent an instance of a class,
    /// return a type that represents that class itself.
    #[must_use]
    pub fn to_meta_type(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::Unbound => Type::Unbound,
            Type::Never => Type::Never,
            Type::Instance(class) => Type::Class(*class),
            Type::Union(union) => union.map(db, |ty| ty.to_meta_type(db)),
            Type::BooleanLiteral(_) => builtins_symbol_ty(db, "bool"),
            Type::BytesLiteral(_) => builtins_symbol_ty(db, "bytes"),
            Type::IntLiteral(_) => builtins_symbol_ty(db, "int"),
            Type::Function(_) => types_symbol_ty(db, "FunctionType"),
            Type::Module(_) => types_symbol_ty(db, "ModuleType"),
            Type::None => typeshed_symbol_ty(db, "NoneType"),
            // TODO not accurate if there's a custom metaclass...
            Type::Class(_) => builtins_symbol_ty(db, "type"),
            // TODO can we do better here? `type[LiteralString]`?
            Type::StringLiteral(_) | Type::LiteralString => builtins_symbol_ty(db, "str"),
            // TODO: `type[Any]`?
            Type::Any => Type::Any,
            // TODO: `type[Unknown]`?
            Type::Unknown => Type::Unknown,
            // TODO intersections
            Type::Intersection(_) => Type::Unknown,
            Type::Tuple(_) => builtins_symbol_ty(db, "tuple"),
        }
    }
}

impl<'db> From<&Type<'db>> for Type<'db> {
    fn from(value: &Type<'db>) -> Self {
        *value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IterationOutcome<'db> {
    Iterable { element_ty: Type<'db> },
    NotIterable { not_iterable_ty: Type<'db> },
}

impl<'db> IterationOutcome<'db> {
    fn unwrap_with_diagnostic(
        self,
        iterable_node: ast::AnyNodeRef,
        inference_builder: &mut TypeInferenceBuilder<'db>,
    ) -> Type<'db> {
        match self {
            Self::Iterable { element_ty } => element_ty,
            Self::NotIterable { not_iterable_ty } => {
                inference_builder.not_iterable_diagnostic(iterable_node, not_iterable_ty);
                Type::Unknown
            }
        }
    }
}

#[salsa::interned]
pub struct FunctionType<'db> {
    /// name of the function at definition
    #[return_ref]
    pub name: ast::name::Name,

    definition: Definition<'db>,

    /// types of all decorators on this function
    decorators: Vec<Type<'db>>,
}

impl<'db> FunctionType<'db> {
    pub fn has_decorator(self, db: &dyn Db, decorator: Type<'_>) -> bool {
        self.decorators(db).contains(&decorator)
    }

    /// inferred return type for this function
    pub fn return_type(&self, db: &'db dyn Db) -> Type<'db> {
        let definition = self.definition(db);
        let DefinitionKind::Function(function_stmt_node) = definition.kind(db) else {
            panic!("Function type definition must have `DefinitionKind::Function`")
        };

        // TODO if a function `bar` is decorated by `foo`,
        // where `foo` is annotated as returning a type `X` that is a subtype of `Callable`,
        // we need to infer the return type from `X`'s return annotation
        // rather than from `bar`'s return annotation
        // in order to determine the type that `bar` returns
        if !function_stmt_node.decorator_list.is_empty() {
            return Type::Unknown;
        }

        function_stmt_node
            .returns
            .as_ref()
            .map(|returns| {
                if function_stmt_node.is_async {
                    // TODO: generic `types.CoroutineType`!
                    Type::Unknown
                } else {
                    definition_expression_ty(db, definition, returns.as_ref())
                }
            })
            .unwrap_or(Type::Unknown)
    }
}

#[salsa::interned]
pub struct ClassType<'db> {
    /// Name of the class at definition
    #[return_ref]
    pub name: ast::name::Name,

    definition: Definition<'db>,

    body_scope: ScopeId<'db>,
}

impl<'db> ClassType<'db> {
    /// Return true if this class is a standard library type with given module name and name.
    pub(crate) fn is_stdlib_symbol(self, db: &'db dyn Db, module_name: &str, name: &str) -> bool {
        name == self.name(db)
            && file_to_module(db, self.body_scope(db).file(db)).is_some_and(|module| {
                module.search_path().is_standard_library() && module.name() == module_name
            })
    }

    /// Return an iterator over the types of this class's bases.
    ///
    /// # Panics:
    /// If `definition` is not a `DefinitionKind::Class`.
    pub fn bases(&self, db: &'db dyn Db) -> impl Iterator<Item = Type<'db>> {
        let definition = self.definition(db);
        let DefinitionKind::Class(class_stmt_node) = definition.kind(db) else {
            panic!("Class type definition must have DefinitionKind::Class");
        };
        class_stmt_node
            .bases()
            .iter()
            .map(move |base_expr| definition_expression_ty(db, definition, base_expr))
    }

    /// Returns the class member of this class named `name`.
    ///
    /// The member resolves to a member of the class itself or any of its bases.
    pub fn class_member(self, db: &'db dyn Db, name: &str) -> Type<'db> {
        let member = self.own_class_member(db, name);
        if !member.is_unbound() {
            return member;
        }

        self.inherited_class_member(db, name)
    }

    /// Returns the inferred type of the class member named `name`.
    pub fn own_class_member(self, db: &'db dyn Db, name: &str) -> Type<'db> {
        let scope = self.body_scope(db);
        symbol_ty(db, scope, name)
    }

    pub fn inherited_class_member(self, db: &'db dyn Db, name: &str) -> Type<'db> {
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
    #[return_ref]
    elements: FxOrderSet<Type<'db>>,
}

impl<'db> UnionType<'db> {
    pub fn contains(&self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        self.elements(db).contains(&ty)
    }

    /// Create a union from a list of elements
    /// (which may be eagerly simplified into a different variant of [`Type`] altogether)
    pub fn from_elements<T: Into<Type<'db>>>(
        db: &'db dyn Db,
        elements: impl IntoIterator<Item = T>,
    ) -> Type<'db> {
        elements
            .into_iter()
            .fold(UnionBuilder::new(db), |builder, element| {
                builder.add(element.into())
            })
            .build()
    }

    /// Apply a transformation function to all elements of the union,
    /// and create a new union from the resulting set of types
    pub fn map(
        &self,
        db: &'db dyn Db,
        transform_fn: impl Fn(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        Self::from_elements(db, self.elements(db).into_iter().map(transform_fn))
    }
}

#[salsa::interned]
pub struct IntersectionType<'db> {
    /// The intersection type includes only values in all of these types.
    #[return_ref]
    positive: FxOrderSet<Type<'db>>,

    /// The intersection type does not include any value in any of these types.
    ///
    /// Negation types aren't expressible in annotations, and are most likely to arise from type
    /// narrowing along with intersections (e.g. `if not isinstance(...)`), so we represent them
    /// directly in intersections rather than as a separate type.
    #[return_ref]
    negative: FxOrderSet<Type<'db>>,
}

#[salsa::interned]
pub struct StringLiteralType<'db> {
    #[return_ref]
    value: Box<str>,
}

#[salsa::interned]
pub struct BytesLiteralType<'db> {
    #[return_ref]
    value: Box<[u8]>,
}

#[salsa::interned]
pub struct TupleType<'db> {
    #[return_ref]
    elements: Box<[Type<'db>]>,
}

#[cfg(test)]
mod tests {
    use super::{builtins_symbol_ty, BytesLiteralType, StringLiteralType, Type, UnionType};
    use crate::db::tests::TestDb;
    use crate::program::{Program, SearchPathSettings};
    use crate::python_version::PythonVersion;
    use crate::ProgramSettings;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
    use test_case::test_case;

    fn setup_db() -> TestDb {
        let db = TestDb::new();

        let src_root = SystemPathBuf::from("/src");
        db.memory_file_system()
            .create_directory_all(&src_root)
            .unwrap();

        Program::from_settings(
            &db,
            &ProgramSettings {
                target_version: PythonVersion::default(),
                search_paths: SearchPathSettings::new(src_root),
            },
        )
        .expect("Valid search path settings");

        db
    }

    /// A test representation of a type that can be transformed unambiguously into a real Type,
    /// given a db.
    #[derive(Debug)]
    enum Ty {
        Never,
        Unknown,
        Any,
        IntLiteral(i64),
        StringLiteral(&'static str),
        LiteralString,
        BytesLiteral(&'static str),
        BuiltinInstance(&'static str),
        Union(Vec<Ty>),
    }

    impl Ty {
        fn into_type(self, db: &TestDb) -> Type<'_> {
            match self {
                Ty::Never => Type::Never,
                Ty::Unknown => Type::Unknown,
                Ty::Any => Type::Any,
                Ty::IntLiteral(n) => Type::IntLiteral(n),
                Ty::StringLiteral(s) => {
                    Type::StringLiteral(StringLiteralType::new(db, (*s).into()))
                }
                Ty::LiteralString => Type::LiteralString,
                Ty::BytesLiteral(s) => {
                    Type::BytesLiteral(BytesLiteralType::new(db, s.as_bytes().into()))
                }
                Ty::BuiltinInstance(s) => builtins_symbol_ty(db, s).to_instance(db),
                Ty::Union(tys) => {
                    UnionType::from_elements(db, tys.into_iter().map(|ty| ty.into_type(db)))
                }
            }
        }
    }

    #[test_case(Ty::Unknown, Ty::IntLiteral(1))]
    #[test_case(Ty::Any, Ty::IntLiteral(1))]
    #[test_case(Ty::Never, Ty::IntLiteral(1))]
    #[test_case(Ty::IntLiteral(1), Ty::Unknown)]
    #[test_case(Ty::IntLiteral(1), Ty::Any)]
    #[test_case(Ty::IntLiteral(1), Ty::BuiltinInstance("int"))]
    #[test_case(Ty::StringLiteral("foo"), Ty::BuiltinInstance("str"))]
    #[test_case(Ty::StringLiteral("foo"), Ty::LiteralString)]
    #[test_case(Ty::LiteralString, Ty::BuiltinInstance("str"))]
    #[test_case(Ty::BytesLiteral("foo"), Ty::BuiltinInstance("bytes"))]
    fn is_assignable_to(from: Ty, to: Ty) {
        let db = setup_db();
        assert!(from.into_type(&db).is_assignable_to(&db, to.into_type(&db)));
    }

    #[test_case(Ty::IntLiteral(1), Ty::BuiltinInstance("str"))]
    #[test_case(Ty::BuiltinInstance("int"), Ty::BuiltinInstance("str"))]
    #[test_case(Ty::BuiltinInstance("int"), Ty::IntLiteral(1))]
    fn is_not_assignable_to(from: Ty, to: Ty) {
        let db = setup_db();
        assert!(!from.into_type(&db).is_assignable_to(&db, to.into_type(&db)));
    }

    #[test_case(
        Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]),
        Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)])
    )]
    fn is_equivalent_to(from: Ty, to: Ty) {
        let db = setup_db();

        assert!(from.into_type(&db).is_equivalent_to(&db, to.into_type(&db)));
    }
}
