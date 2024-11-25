use std::hash::Hash;

use indexmap::IndexSet;
use itertools::Itertools;

use ruff_db::files::File;
use ruff_python_ast as ast;

pub(crate) use self::builder::{IntersectionBuilder, UnionBuilder};
pub use self::diagnostic::{TypeCheckDiagnostic, TypeCheckDiagnostics};
pub(crate) use self::display::TypeArrayDisplay;
pub(crate) use self::infer::{
    infer_deferred_types, infer_definition_types, infer_expression_types, infer_scope_types,
};
pub(crate) use self::signatures::Signature;
use crate::module_resolver::file_to_module;
use crate::semantic_index::ast_ids::HasScopedExpressionId;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::symbol::{self as symbol, ScopeId, ScopedSymbolId};
use crate::semantic_index::{
    global_scope, semantic_index, symbol_table, use_def_map, BindingWithConstraints,
    BindingWithConstraintsIterator, DeclarationsIterator,
};
use crate::stdlib::{
    builtins_symbol, core_module_symbol, typing_extensions_symbol, CoreStdlibModule,
};
use crate::symbol::{Boundness, Symbol};
use crate::types::diagnostic::TypeCheckDiagnosticsBuilder;
use crate::types::mro::{ClassBase, Mro, MroError, MroIterator};
use crate::types::narrow::narrowing_constraint;
use crate::{Db, FxOrderSet, Module, Program};

mod builder;
mod diagnostic;
mod display;
mod infer;
mod mro;
mod narrow;
mod signatures;
mod string_annotation;
mod unpacker;

#[salsa::tracked(return_ref)]
pub fn check_types(db: &dyn Db, file: File) -> TypeCheckDiagnostics {
    let _span = tracing::trace_span!("check_types", file=?file.path(db)).entered();

    tracing::debug!("Checking file '{path}'", path = file.path(db));

    let index = semantic_index(db, file);
    let mut diagnostics = TypeCheckDiagnostics::default();

    for scope_id in index.scope_ids() {
        let result = infer_scope_types(db, scope_id);
        diagnostics.extend(result.diagnostics());
    }

    diagnostics
}

/// Infer the public type of a symbol (its type as seen from outside its scope).
fn symbol_by_id<'db>(db: &'db dyn Db, scope: ScopeId<'db>, symbol: ScopedSymbolId) -> Symbol<'db> {
    let _span = tracing::trace_span!("symbol_by_id", ?symbol).entered();

    let use_def = use_def_map(db, scope);

    // If the symbol is declared, the public type is based on declarations; otherwise, it's based
    // on inference from bindings.
    if use_def.has_public_declarations(symbol) {
        let declarations = use_def.public_declarations(symbol);
        // If the symbol is undeclared in some paths, include the inferred type in the public type.
        let undeclared_ty = if declarations.may_be_undeclared() {
            Some(
                bindings_ty(db, use_def.public_bindings(symbol))
                    .map(|bindings_ty| Symbol::Type(bindings_ty, use_def.public_boundness(symbol)))
                    .unwrap_or(Symbol::Unbound),
            )
        } else {
            None
        };
        // Intentionally ignore conflicting declared types; that's not our problem, it's the
        // problem of the module we are importing from.

        // TODO: Our handling of boundness currently only depends on bindings, and ignores
        // declarations. This is inconsistent, since we only look at bindings if the symbol
        // may be undeclared. Consider the following example:
        // ```py
        // x: int
        //
        // if flag:
        //     y: int
        // else
        //     y = 3
        // ```
        // If we import from this module, we will currently report `x` as a definitely-bound
        // symbol (even though it has no bindings at all!) but report `y` as possibly-unbound
        // (even though every path has either a binding or a declaration for it.)

        match undeclared_ty {
            Some(Symbol::Type(ty, boundness)) => Symbol::Type(
                declarations_ty(db, declarations, Some(ty)).unwrap_or_else(|(ty, _)| ty),
                boundness,
            ),
            None | Some(Symbol::Unbound) => Symbol::Type(
                declarations_ty(db, declarations, None).unwrap_or_else(|(ty, _)| ty),
                Boundness::Bound,
            ),
        }
    } else {
        bindings_ty(db, use_def.public_bindings(symbol))
            .map(|bindings_ty| Symbol::Type(bindings_ty, use_def.public_boundness(symbol)))
            .unwrap_or(Symbol::Unbound)
    }
}

/// Shorthand for `symbol_by_id` that takes a symbol name instead of an ID.
fn symbol<'db>(db: &'db dyn Db, scope: ScopeId<'db>, name: &str) -> Symbol<'db> {
    let table = symbol_table(db, scope);
    table
        .symbol_id_by_name(name)
        .map(|symbol| symbol_by_id(db, scope, symbol))
        .unwrap_or(Symbol::Unbound)
}

/// Return a list of the symbols that typeshed declares in the body scope of
/// the stub for the class `types.ModuleType`.
///
/// Conceptually this could be a `Set` rather than a list,
/// but the number of symbols declared in this scope is likely to be very small,
/// so the cost of hashing the names is likely to be more expensive than it's worth.
#[salsa::tracked(return_ref)]
fn module_type_symbols<'db>(db: &'db dyn Db) -> smallvec::SmallVec<[ast::name::Name; 8]> {
    let Some(module_type) = KnownClass::ModuleType
        .to_class_literal(db)
        .into_class_literal()
    else {
        // The most likely way we get here is if a user specified a `--custom-typeshed-dir`
        // without a `types.pyi` stub in the `stdlib/` directory
        return smallvec::SmallVec::default();
    };

    let module_type_scope = module_type.class.body_scope(db);
    let module_type_symbol_table = symbol_table(db, module_type_scope);

    // `__dict__` and `__init__` are very special members that can be accessed as attributes
    // on the module when imported, but cannot be accessed as globals *inside* the module.
    //
    // `__getattr__` is even more special: it doesn't exist at runtime, but typeshed includes it
    // to reduce false positives associated with functions that dynamically import modules
    // and return `Instance(types.ModuleType)`. We should ignore it for any known module-literal type.
    module_type_symbol_table
        .symbols()
        .filter(|symbol| symbol.is_declared())
        .map(symbol::Symbol::name)
        .filter(|symbol_name| !matches!(&***symbol_name, "__dict__" | "__getattr__" | "__init__"))
        .cloned()
        .collect()
}

/// Looks up a module-global symbol by name in a file.
pub(crate) fn global_symbol<'db>(db: &'db dyn Db, file: File, name: &str) -> Symbol<'db> {
    let explicit_symbol = symbol(db, global_scope(db, file), name);

    if !explicit_symbol.possibly_unbound() {
        return explicit_symbol;
    }

    // Not defined explicitly in the global scope?
    // All modules are instances of `types.ModuleType`;
    // look it up there (with a few very special exceptions)
    if module_type_symbols(db)
        .iter()
        .any(|module_type_member| &**module_type_member == name)
    {
        // TODO: this should use `.to_instance(db)`. but we don't understand attribute access
        // on instance types yet.
        let module_type_member = KnownClass::ModuleType.to_class_literal(db).member(db, name);
        return explicit_symbol.or_fall_back_to(db, &module_type_member);
    }

    explicit_symbol
}

/// Infer the type of a binding.
pub(crate) fn binding_ty<'db>(db: &'db dyn Db, definition: Definition<'db>) -> Type<'db> {
    let inference = infer_definition_types(db, definition);
    inference.binding_ty(definition)
}

/// Infer the type of a declaration.
fn declaration_ty<'db>(db: &'db dyn Db, definition: Definition<'db>) -> Type<'db> {
    let inference = infer_definition_types(db, definition);
    inference.declaration_ty(definition)
}

/// Infer the type of a (possibly deferred) sub-expression of a [`Definition`].
///
/// Supports expressions that are evaluated within a type-params sub-scope.
///
/// ## Panics
/// If the given expression is not a sub-expression of the given [`Definition`].
fn definition_expression_ty<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    expression: &ast::Expr,
) -> Type<'db> {
    let file = definition.file(db);
    let index = semantic_index(db, file);
    let file_scope = index.expression_scope_id(expression);
    let scope = file_scope.to_scope_id(db, file);
    let expr_id = expression.scoped_expression_id(db, scope);
    if scope == definition.scope(db) {
        // expression is in the definition scope
        let inference = infer_definition_types(db, definition);
        if let Some(ty) = inference.try_expression_ty(expr_id) {
            ty
        } else {
            infer_deferred_types(db, definition).expression_ty(expr_id)
        }
    } else {
        // expression is in a type-params sub-scope
        infer_scope_types(db, scope).expression_ty(expr_id)
    }
}

/// Infer the combined type of an iterator of bindings.
///
/// Will return a union if there is more than one binding.
fn bindings_ty<'db>(
    db: &'db dyn Db,
    bindings_with_constraints: BindingWithConstraintsIterator<'_, 'db>,
) -> Option<Type<'db>> {
    let mut def_types = bindings_with_constraints.map(
        |BindingWithConstraints {
             binding,
             constraints,
         }| {
            let mut constraint_tys = constraints
                .filter_map(|constraint| narrowing_constraint(db, constraint, binding))
                .peekable();

            let binding_ty = binding_ty(db, binding);
            if constraint_tys.peek().is_some() {
                constraint_tys
                    .fold(
                        IntersectionBuilder::new(db).add_positive(binding_ty),
                        IntersectionBuilder::add_positive,
                    )
                    .build()
            } else {
                binding_ty
            }
        },
    );

    if let Some(first) = def_types.next() {
        if let Some(second) = def_types.next() {
            Some(UnionType::from_elements(
                db,
                [first, second].into_iter().chain(def_types),
            ))
        } else {
            Some(first)
        }
    } else {
        None
    }
}

/// The result of looking up a declared type from declarations; see [`declarations_ty`].
type DeclaredTypeResult<'db> = Result<Type<'db>, (Type<'db>, Box<[Type<'db>]>)>;

/// Build a declared type from a [`DeclarationsIterator`].
///
/// If there is only one declaration, or all declarations declare the same type, returns
/// `Ok(declared_type)`. If there are conflicting declarations, returns
/// `Err((union_of_declared_types, conflicting_declared_types))`.
///
/// If undeclared is a possibility, `undeclared_ty` type will be part of the return type (and may
/// conflict with other declarations.)
///
/// # Panics
/// Will panic if there are no declarations and no `undeclared_ty` is provided. This is a logic
/// error, as any symbol with zero live declarations clearly must be undeclared, and the caller
/// should provide an `undeclared_ty`.
fn declarations_ty<'db>(
    db: &'db dyn Db,
    declarations: DeclarationsIterator<'_, 'db>,
    undeclared_ty: Option<Type<'db>>,
) -> DeclaredTypeResult<'db> {
    let decl_types = declarations.map(|declaration| declaration_ty(db, declaration));

    let mut all_types = undeclared_ty.into_iter().chain(decl_types);

    let first = all_types.next().expect(
        "declarations_ty must not be called with zero declarations and no may-be-undeclared",
    );

    let mut conflicting: Vec<Type<'db>> = vec![];
    let declared_ty = if let Some(second) = all_types.next() {
        let mut builder = UnionBuilder::new(db).add(first);
        for other in [second].into_iter().chain(all_types) {
            // Make sure not to emit spurious errors relating to `Type::Todo`,
            // since we only infer this type due to a limitation in our current model.
            //
            // `Unknown` is different here, since we might infer `Unknown`
            // for one of these due to a variable being defined in one possible
            // control-flow branch but not another one.
            if !first.is_equivalent_to(db, other) && !first.is_todo() && !other.is_todo() {
                conflicting.push(other);
            }
            builder = builder.add(other);
        }
        builder.build()
    } else {
        first
    };
    if conflicting.is_empty() {
        Ok(declared_ty)
    } else {
        Err((
            declared_ty,
            [first].into_iter().chain(conflicting).collect(),
        ))
    }
}

/// Meta data for `Type::Todo`, which represents a known limitation in red-knot.
#[cfg(debug_assertions)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TodoType {
    FileAndLine(&'static str, u32),
    Message(&'static str),
}

#[cfg(debug_assertions)]
impl std::fmt::Display for TodoType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TodoType::FileAndLine(file, line) => write!(f, "[{file}:{line}]"),
            TodoType::Message(msg) => write!(f, "({msg})"),
        }
    }
}

#[cfg(not(debug_assertions))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TodoType;

#[cfg(not(debug_assertions))]
impl std::fmt::Display for TodoType {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

/// Create a `Type::Todo` variant to represent a known limitation in the type system.
///
/// It can be used with a custom message (preferred): `todo_type!("PEP 604 not supported")`,
/// or simply using `todo_type!()`, which will include information about the file and line.
#[cfg(debug_assertions)]
macro_rules! todo_type {
    () => {
        Type::Todo(crate::types::TodoType::FileAndLine(file!(), line!()))
    };
    ($message:literal) => {
        Type::Todo(crate::types::TodoType::Message($message))
    };
}

#[cfg(not(debug_assertions))]
macro_rules! todo_type {
    () => {
        Type::Todo(crate::types::TodoType)
    };
    ($message:literal) => {
        Type::Todo(crate::types::TodoType)
    };
}

pub(crate) use todo_type;

/// Representation of a type: a set of possible values at runtime.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
pub enum Type<'db> {
    /// The dynamic type: a statically unknown set of values
    Any,
    /// Unknown type (either no annotation, or some kind of type error).
    /// Equivalent to Any, or possibly to object in strict mode
    Unknown,
    /// Temporary type for symbols that can't be inferred yet because of missing implementations.
    /// Behaves equivalently to `Any`.
    ///
    /// This variant should eventually be removed once red-knot is spec-compliant.
    ///
    /// General rule: `Todo` should only propagate when the presence of the input `Todo` caused the
    /// output to be unknown. An output should only be `Todo` if fixing all `Todo` inputs to be not
    /// `Todo` would change the output type.
    ///
    /// This variant should be created with the `todo_type!` macro.
    Todo(TodoType),
    /// The empty set of values
    Never,
    /// A specific function object
    FunctionLiteral(FunctionType<'db>),
    /// A specific module object
    ModuleLiteral(File),
    /// A specific class object
    ClassLiteral(ClassLiteralType<'db>),
    // The set of all class objects that are subclasses of the given class (C), spelled `type[C]`.
    SubclassOf(SubclassOfType<'db>),
    /// The set of Python objects with the given class in their __class__'s method resolution order
    Instance(InstanceType<'db>),
    /// A single Python object that requires special treatment in the type system
    KnownInstance(KnownInstanceType<'db>),
    /// The set of objects in any of the types in the union
    Union(UnionType<'db>),
    /// The set of objects in all of the types in the intersection
    Intersection(IntersectionType<'db>),
    /// An integer literal
    IntLiteral(i64),
    /// A boolean literal, either `True` or `False`.
    BooleanLiteral(bool),
    /// A string literal whose value is known
    StringLiteral(StringLiteralType<'db>),
    /// A string known to originate only from literal values, but whose value is not known (unlike
    /// `StringLiteral` above).
    LiteralString,
    /// A bytes literal
    BytesLiteral(BytesLiteralType<'db>),
    /// A slice literal, e.g. `1:5`, `10:0:-1` or `:`
    SliceLiteral(SliceLiteralType<'db>),
    /// A heterogeneous tuple type, with elements of the given types in source order.
    // TODO: Support variable length homogeneous tuple type like `tuple[int, ...]`.
    Tuple(TupleType<'db>),
    // TODO protocols, callable types, overloads, generics, type vars
}

impl<'db> Type<'db> {
    pub const fn is_never(&self) -> bool {
        matches!(self, Type::Never)
    }

    pub const fn is_todo(&self) -> bool {
        matches!(self, Type::Todo(_))
    }

    pub const fn class_literal(class: Class<'db>) -> Self {
        Self::ClassLiteral(ClassLiteralType { class })
    }

    pub const fn into_class_literal(self) -> Option<ClassLiteralType<'db>> {
        match self {
            Type::ClassLiteral(class_type) => Some(class_type),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_class_literal(self) -> ClassLiteralType<'db> {
        self.into_class_literal()
            .expect("Expected a Type::ClassLiteral variant")
    }

    pub const fn is_class_literal(&self) -> bool {
        matches!(self, Type::ClassLiteral(..))
    }

    pub const fn into_module_literal(self) -> Option<File> {
        match self {
            Type::ModuleLiteral(file) => Some(file),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_module_literal(self) -> File {
        self.into_module_literal()
            .expect("Expected a Type::ModuleLiteral variant")
    }

    pub const fn into_union(self) -> Option<UnionType<'db>> {
        match self {
            Type::Union(union_type) => Some(union_type),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_union(self) -> UnionType<'db> {
        self.into_union().expect("Expected a Type::Union variant")
    }

    pub const fn is_union(&self) -> bool {
        matches!(self, Type::Union(..))
    }

    pub const fn into_intersection(self) -> Option<IntersectionType<'db>> {
        match self {
            Type::Intersection(intersection_type) => Some(intersection_type),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_intersection(self) -> IntersectionType<'db> {
        self.into_intersection()
            .expect("Expected a Type::Intersection variant")
    }

    pub const fn into_function_literal(self) -> Option<FunctionType<'db>> {
        match self {
            Type::FunctionLiteral(function_type) => Some(function_type),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_function_literal(self) -> FunctionType<'db> {
        self.into_function_literal()
            .expect("Expected a Type::FunctionLiteral variant")
    }

    pub const fn is_function_literal(&self) -> bool {
        matches!(self, Type::FunctionLiteral(..))
    }

    pub const fn into_int_literal(self) -> Option<i64> {
        match self {
            Type::IntLiteral(value) => Some(value),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_int_literal(self) -> i64 {
        self.into_int_literal()
            .expect("Expected a Type::IntLiteral variant")
    }

    pub const fn into_known_instance(self) -> Option<KnownInstanceType<'db>> {
        match self {
            Type::KnownInstance(known_instance) => Some(known_instance),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_known_instance(self) -> KnownInstanceType<'db> {
        self.into_known_instance()
            .expect("Expected a Type::KnownInstance variant")
    }

    pub const fn is_boolean_literal(&self) -> bool {
        matches!(self, Type::BooleanLiteral(..))
    }

    pub const fn is_literal_string(&self) -> bool {
        matches!(self, Type::LiteralString)
    }

    pub const fn instance(class: Class<'db>) -> Self {
        Self::Instance(InstanceType { class })
    }

    pub const fn subclass_of(class: Class<'db>) -> Self {
        Self::SubclassOf(SubclassOfType { class })
    }

    pub fn string_literal(db: &'db dyn Db, string: &str) -> Self {
        Self::StringLiteral(StringLiteralType::new(db, string))
    }

    pub fn bytes_literal(db: &'db dyn Db, bytes: &[u8]) -> Self {
        Self::BytesLiteral(BytesLiteralType::new(db, bytes))
    }

    pub fn tuple(db: &'db dyn Db, elements: &[Type<'db>]) -> Self {
        Self::Tuple(TupleType::new(db, elements))
    }

    #[must_use]
    pub fn negate(&self, db: &'db dyn Db) -> Type<'db> {
        IntersectionBuilder::new(db).add_negative(*self).build()
    }

    #[must_use]
    pub fn negate_if(&self, db: &'db dyn Db, yes: bool) -> Type<'db> {
        if yes {
            self.negate(db)
        } else {
            *self
        }
    }

    /// Return true if this type is a [subtype of] type `target`.
    ///
    /// [subtype of]: https://typing.readthedocs.io/en/latest/spec/concepts.html#subtype-supertype-and-type-equivalence
    pub(crate) fn is_subtype_of(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        if self.is_equivalent_to(db, target) {
            return true;
        }
        match (self, target) {
            (Type::Unknown | Type::Any | Type::Todo(_), _) => false,
            (_, Type::Unknown | Type::Any | Type::Todo(_)) => false,
            (Type::Never, _) => true,
            (_, Type::Never) => false,
            (_, Type::Instance(InstanceType { class }))
                if class.is_known(db, KnownClass::Object) =>
            {
                true
            }
            (Type::Instance(InstanceType { class }), _)
                if class.is_known(db, KnownClass::Object) =>
            {
                false
            }
            (Type::BooleanLiteral(_), Type::Instance(InstanceType { class }))
                if matches!(class.known(db), Some(KnownClass::Bool | KnownClass::Int)) =>
            {
                true
            }
            (Type::IntLiteral(_), Type::Instance(InstanceType { class }))
                if class.is_known(db, KnownClass::Int) =>
            {
                true
            }
            (Type::StringLiteral(_), Type::LiteralString) => true,
            (
                Type::StringLiteral(_) | Type::LiteralString,
                Type::Instance(InstanceType { class }),
            ) if class.is_known(db, KnownClass::Str) => true,
            (Type::BytesLiteral(_), Type::Instance(InstanceType { class }))
                if class.is_known(db, KnownClass::Bytes) =>
            {
                true
            }
            (Type::Tuple(self_tuple), Type::Tuple(target_tuple)) => {
                let self_elements = self_tuple.elements(db);
                let target_elements = target_tuple.elements(db);
                self_elements.len() == target_elements.len()
                    && self_elements.iter().zip(target_elements).all(
                        |(self_element, target_element)| {
                            self_element.is_subtype_of(db, *target_element)
                        },
                    )
            }
            (Type::ClassLiteral(..), Type::Instance(InstanceType { class }))
                if class.is_known(db, KnownClass::Type) =>
            {
                true
            }
            (Type::ClassLiteral(self_class), Type::SubclassOf(target_class)) => {
                self_class.class.is_subclass_of(db, target_class.class)
            }
            (Type::SubclassOf(self_class), Type::SubclassOf(target_class)) => {
                self_class.class.is_subclass_of(db, target_class.class)
            }
            (
                Type::SubclassOf(SubclassOfType { class: self_class }),
                Type::Instance(InstanceType {
                    class: target_class,
                }),
            ) if self_class
                .metaclass(db)
                .into_class_literal()
                .map(|meta| meta.class.is_subclass_of(db, target_class))
                .unwrap_or(false) =>
            {
                true
            }
            (Type::Union(union), ty) => union
                .elements(db)
                .iter()
                .all(|&elem_ty| elem_ty.is_subtype_of(db, ty)),
            (ty, Type::Union(union)) => union
                .elements(db)
                .iter()
                .any(|&elem_ty| ty.is_subtype_of(db, elem_ty)),
            (Type::Intersection(self_intersection), Type::Intersection(target_intersection)) => {
                // Check that all target positive values are covered in self positive values
                target_intersection
                    .positive(db)
                    .iter()
                    .all(|&target_pos_elem| {
                        self_intersection
                            .positive(db)
                            .iter()
                            .any(|&self_pos_elem| self_pos_elem.is_subtype_of(db, target_pos_elem))
                    })
                    // Check that all target negative values are excluded in self, either by being
                    // subtypes of a self negative value or being disjoint from a self positive value.
                    && target_intersection
                        .negative(db)
                        .iter()
                        .all(|&target_neg_elem| {
                            // Is target negative value is subtype of a self negative value
                            self_intersection.negative(db).iter().any(|&self_neg_elem| {
                                target_neg_elem.is_subtype_of(db, self_neg_elem)
                            // Is target negative value is disjoint from a self positive value?
                            }) || self_intersection.positive(db).iter().any(|&self_pos_elem| {
                                target_neg_elem.is_disjoint_from(db, self_pos_elem)
                            })
                        })
            }
            (Type::Intersection(intersection), ty) => intersection
                .positive(db)
                .iter()
                .any(|&elem_ty| elem_ty.is_subtype_of(db, ty)),
            (ty, Type::Intersection(intersection)) => {
                intersection
                    .positive(db)
                    .iter()
                    .all(|&pos_ty| ty.is_subtype_of(db, pos_ty))
                    && intersection
                        .negative(db)
                        .iter()
                        .all(|&neg_ty| neg_ty.is_disjoint_from(db, ty))
            }
            (Type::KnownInstance(left), right) => {
                left.instance_fallback(db).is_subtype_of(db, right)
            }
            (left, Type::KnownInstance(right)) => {
                left.is_subtype_of(db, right.instance_fallback(db))
            }
            (Type::Instance(left), Type::Instance(right)) => left.is_instance_of(db, right.class),
            // TODO
            _ => false,
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
            (Type::Unknown | Type::Any | Type::Todo(_), _) => true,
            (_, Type::Unknown | Type::Any | Type::Todo(_)) => true,
            (Type::Union(union), ty) => union
                .elements(db)
                .iter()
                .all(|&elem_ty| elem_ty.is_assignable_to(db, ty)),
            (ty, Type::Union(union)) => union
                .elements(db)
                .iter()
                .any(|&elem_ty| ty.is_assignable_to(db, elem_ty)),
            (Type::Tuple(self_tuple), Type::Tuple(target_tuple)) => {
                let self_elements = self_tuple.elements(db);
                let target_elements = target_tuple.elements(db);
                self_elements.len() == target_elements.len()
                    && self_elements.iter().zip(target_elements).all(
                        |(self_element, target_element)| {
                            self_element.is_assignable_to(db, *target_element)
                        },
                    )
            }
            // TODO other types containing gradual forms (e.g. generics containing Any/Unknown)
            _ => self.is_subtype_of(db, target),
        }
    }

    /// Return true if this type is equivalent to type `other`.
    pub(crate) fn is_equivalent_to(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        // TODO equivalent but not identical structural types, differently-ordered unions and
        // intersections, other cases?

        // TODO: Once we have support for final classes, we can establish that
        // `Type::SubclassOf('FinalClass')` is equivalent to `Type::ClassLiteral('FinalClass')`.

        // TODO: The following is a workaround that is required to unify the two different versions
        // of `NoneType` and `NoDefaultType` in typeshed. This should not be required anymore once
        // we understand `sys.version_info` branches.
        self == other
            || matches!((self, other), (Type::Todo(_), Type::Todo(_)))
            || matches!((self, other),
                (
                    Type::Instance(InstanceType { class: self_class }),
                    Type::Instance(InstanceType { class: target_class })
                )
                if {
                    let self_known = self_class.known(db);
                    matches!(self_known, Some(KnownClass::NoneType | KnownClass::NoDefaultType))
                        && self_known == target_class.known(db)
                }
            )
    }

    /// Return true if this type and `other` have no common elements.
    ///
    /// Note: This function aims to have no false positives, but might return
    /// wrong `false` answers in some cases.
    pub(crate) fn is_disjoint_from(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        match (self, other) {
            (Type::Never, _) | (_, Type::Never) => true,

            (Type::Any, _) | (_, Type::Any) => false,
            (Type::Unknown, _) | (_, Type::Unknown) => false,
            (Type::Todo(_), _) | (_, Type::Todo(_)) => false,

            (Type::Union(union), other) | (other, Type::Union(union)) => union
                .elements(db)
                .iter()
                .all(|e| e.is_disjoint_from(db, other)),

            (Type::Intersection(intersection), other)
            | (other, Type::Intersection(intersection)) => {
                if intersection
                    .positive(db)
                    .iter()
                    .any(|p| p.is_disjoint_from(db, other))
                {
                    true
                } else {
                    // TODO we can do better here. For example:
                    // X & ~Literal[1] is disjoint from Literal[1]
                    false
                }
            }

            (
                left @ (Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::BytesLiteral(..)
                | Type::SliceLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::ModuleLiteral(..)
                | Type::ClassLiteral(..)),
                right @ (Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::BytesLiteral(..)
                | Type::SliceLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::ModuleLiteral(..)
                | Type::ClassLiteral(..)),
            ) => left != right,

            (Type::SubclassOf(type_class), Type::ClassLiteral(class_literal))
            | (Type::ClassLiteral(class_literal), Type::SubclassOf(type_class)) => {
                !class_literal.class.is_subclass_of(db, type_class.class)
            }
            (Type::SubclassOf(_), Type::SubclassOf(_)) => false,
            (Type::SubclassOf(_), Type::Instance(_)) | (Type::Instance(_), Type::SubclassOf(_)) => {
                false
            }
            (
                Type::SubclassOf(_),
                Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::BytesLiteral(..)
                | Type::SliceLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::ModuleLiteral(..),
            )
            | (
                Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::BytesLiteral(..)
                | Type::SliceLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::ModuleLiteral(..),
                Type::SubclassOf(_),
            ) => true,
            (Type::SubclassOf(_), _) | (_, Type::SubclassOf(_)) => {
                // TODO: Once we have support for final classes, we can determine disjointness in some cases
                // here. However, note that it might be better to turn `Type::SubclassOf('FinalClass')` into
                // `Type::ClassLiteral('FinalClass')` during construction, instead of adding special cases for
                // final classes inside `Type::SubclassOf` everywhere.
                false
            }
            (Type::KnownInstance(left), Type::KnownInstance(right)) => left != right,
            (Type::KnownInstance(left), right) => {
                left.instance_fallback(db).is_disjoint_from(db, right)
            }
            (left, Type::KnownInstance(right)) => {
                left.is_disjoint_from(db, right.instance_fallback(db))
            }
            (
                Type::Instance(InstanceType { class: class_none }),
                Type::Instance(InstanceType { class: class_other }),
            )
            | (
                Type::Instance(InstanceType { class: class_other }),
                Type::Instance(InstanceType { class: class_none }),
            ) if class_none.is_known(db, KnownClass::NoneType) => !matches!(
                class_other.known(db),
                Some(KnownClass::NoneType | KnownClass::Object)
            ),
            (Type::Instance(InstanceType { class: class_none }), _)
            | (_, Type::Instance(InstanceType { class: class_none }))
                if class_none.is_known(db, KnownClass::NoneType) =>
            {
                true
            }

            (Type::BooleanLiteral(..), Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::BooleanLiteral(..)) => !matches!(
                class.known(db),
                Some(KnownClass::Bool | KnownClass::Int | KnownClass::Object)
            ),
            (Type::BooleanLiteral(..), _) | (_, Type::BooleanLiteral(..)) => true,

            (Type::IntLiteral(..), Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::IntLiteral(..)) => {
                !matches!(class.known(db), Some(KnownClass::Int | KnownClass::Object))
            }
            (Type::IntLiteral(..), _) | (_, Type::IntLiteral(..)) => true,

            (Type::StringLiteral(..), Type::LiteralString)
            | (Type::LiteralString, Type::StringLiteral(..)) => false,
            (Type::StringLiteral(..), Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::StringLiteral(..)) => {
                !matches!(class.known(db), Some(KnownClass::Str | KnownClass::Object))
            }
            (Type::StringLiteral(..), _) | (_, Type::StringLiteral(..)) => true,

            (Type::LiteralString, Type::LiteralString) => false,
            (Type::LiteralString, Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::LiteralString) => {
                !matches!(class.known(db), Some(KnownClass::Str | KnownClass::Object))
            }
            (Type::LiteralString, _) | (_, Type::LiteralString) => true,

            (Type::BytesLiteral(..), Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::BytesLiteral(..)) => !matches!(
                class.known(db),
                Some(KnownClass::Bytes | KnownClass::Object)
            ),
            (Type::BytesLiteral(..), _) | (_, Type::BytesLiteral(..)) => true,

            (Type::SliceLiteral(..), Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::SliceLiteral(..)) => !matches!(
                class.known(db),
                Some(KnownClass::Slice | KnownClass::Object)
            ),
            (Type::SliceLiteral(..), _) | (_, Type::SliceLiteral(..)) => true,

            (Type::ClassLiteral(..), Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::ClassLiteral(..)) => {
                !matches!(class.known(db), Some(KnownClass::Type | KnownClass::Object))
            }
            (Type::FunctionLiteral(..), Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::FunctionLiteral(..)) => !matches!(
                class.known(db),
                Some(KnownClass::FunctionType | KnownClass::Object)
            ),
            (Type::ModuleLiteral(..), Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::ModuleLiteral(..)) => !matches!(
                class.known(db),
                Some(KnownClass::ModuleType | KnownClass::Object)
            ),

            (Type::Instance(..), Type::Instance(..)) => {
                // TODO: once we have support for `final`, there might be some cases where
                // we can determine that two types are disjoint. Once we do this, some cases
                // above (e.g. NoneType) can be removed. For non-final classes, we return
                // false (multiple inheritance).

                // TODO: is there anything specific to do for instances of KnownClass::Type?

                false
            }

            (Type::Tuple(tuple), other) | (other, Type::Tuple(tuple)) => {
                if let Type::Tuple(other_tuple) = other {
                    if tuple.len(db) == other_tuple.len(db) {
                        tuple
                            .elements(db)
                            .iter()
                            .zip(other_tuple.elements(db))
                            .any(|(e1, e2)| e1.is_disjoint_from(db, *e2))
                    } else {
                        true
                    }
                } else {
                    // We can not be sure if the tuple is disjoint from 'other' because:
                    //   - 'other' might be the homogeneous arbitrary-length tuple type
                    //     tuple[T, ...] (which we don't have support for yet); if all of
                    //     our element types are not disjoint with T, this is not disjoint
                    //   - 'other' might be a user subtype of tuple, which, if generic
                    //     over the same or compatible *Ts, would overlap with tuple.
                    //
                    // TODO: add checks for the above cases once we support them

                    false
                }
            }
        }
    }

    /// Return true if there is just a single inhabitant for this type.
    ///
    /// Note: This function aims to have no false positives, but might return `false`
    /// for more complicated types that are actually singletons.
    pub(crate) fn is_singleton(self, db: &'db dyn Db) -> bool {
        match self {
            Type::Any
            | Type::Never
            | Type::Unknown
            | Type::Todo(_)
            | Type::IntLiteral(..)
            | Type::StringLiteral(..)
            | Type::BytesLiteral(..)
            | Type::SliceLiteral(..)
            | Type::LiteralString => {
                // Note: The literal types included in this pattern are not true singletons.
                // There can be multiple Python objects (at different memory locations) that
                // are both of type Literal[345], for example.
                false
            }
            Type::SubclassOf(..) => {
                // TODO once we have support for final classes, we can return `true` for some
                // cases: type[C] is a singleton if C is final.
                false
            }
            Type::BooleanLiteral(_)
            | Type::FunctionLiteral(..)
            | Type::ClassLiteral(..)
            | Type::ModuleLiteral(..)
            | Type::KnownInstance(..) => true,
            Type::Instance(InstanceType { class }) => {
                class.known(db).is_some_and(KnownClass::is_singleton)
            }
            Type::Tuple(..) => {
                // The empty tuple is a singleton on CPython and PyPy, but not on other Python
                // implementations such as GraalPy. Its *use* as a singleton is discouraged and
                // should not be relied on for type narrowing, so we do not treat it as one.
                // See:
                // https://docs.python.org/3/reference/expressions.html#parenthesized-forms
                false
            }
            Type::Union(..) => {
                // A single-element union, where the sole element was a singleton, would itself
                // be a singleton type. However, unions with length < 2 should never appear in
                // our model due to [`UnionBuilder::build`].
                false
            }
            Type::Intersection(..) => {
                // Here, we assume that all intersection types that are singletons would have
                // been reduced to a different form via [`IntersectionBuilder::build`] by now.
                // For example:
                //
                //   bool & ~Literal[False]   = Literal[True]
                //   None & (None | int)      = None | None & int = None
                //
                false
            }
        }
    }

    /// Return true if this type is non-empty and all inhabitants of this type compare equal.
    pub(crate) fn is_single_valued(self, db: &'db dyn Db) -> bool {
        match self {
            Type::FunctionLiteral(..)
            | Type::ModuleLiteral(..)
            | Type::ClassLiteral(..)
            | Type::IntLiteral(..)
            | Type::BooleanLiteral(..)
            | Type::StringLiteral(..)
            | Type::BytesLiteral(..)
            | Type::SliceLiteral(..)
            | Type::KnownInstance(..) => true,

            Type::SubclassOf(..) => {
                // TODO: Same comment as above for `is_singleton`
                false
            }

            Type::Tuple(tuple) => tuple
                .elements(db)
                .iter()
                .all(|elem| elem.is_single_valued(db)),

            Type::Instance(InstanceType { class }) => match class.known(db) {
                Some(
                    KnownClass::NoneType
                    | KnownClass::NoDefaultType
                    | KnownClass::VersionInfo
                    | KnownClass::TypeAliasType,
                ) => true,
                Some(
                    KnownClass::Bool
                    | KnownClass::Object
                    | KnownClass::Bytes
                    | KnownClass::Type
                    | KnownClass::Int
                    | KnownClass::Float
                    | KnownClass::Str
                    | KnownClass::List
                    | KnownClass::Tuple
                    | KnownClass::Set
                    | KnownClass::Dict
                    | KnownClass::Slice
                    | KnownClass::GenericAlias
                    | KnownClass::ModuleType
                    | KnownClass::FunctionType
                    | KnownClass::SpecialForm
                    | KnownClass::TypeVar,
                ) => false,
                None => false,
            },

            Type::Any
            | Type::Never
            | Type::Unknown
            | Type::Todo(_)
            | Type::Union(..)
            | Type::Intersection(..)
            | Type::LiteralString => false,
        }
    }

    /// Resolve a member access of a type.
    ///
    /// For example, if `foo` is `Type::Instance(<Bar>)`,
    /// `foo.member(&db, "baz")` returns the type of `baz` attributes
    /// as accessed from instances of the `Bar` class.
    #[must_use]
    pub(crate) fn member(&self, db: &'db dyn Db, name: &str) -> Symbol<'db> {
        match self {
            Type::Any => Type::Any.into(),
            Type::Never => {
                // TODO: attribute lookup on Never type
                todo_type!().into()
            }
            Type::Unknown => Type::Unknown.into(),
            Type::FunctionLiteral(_) => {
                // TODO: attribute lookup on function type
                todo_type!().into()
            }
            Type::ModuleLiteral(file) => {
                // `__dict__` is a very special member that is never overridden by module globals;
                // we should always look it up directly as an attribute on `types.ModuleType`,
                // never in the global scope of the module.
                if name == "__dict__" {
                    return KnownClass::ModuleType
                        .to_instance(db)
                        .member(db, "__dict__");
                }

                let global_lookup = symbol(db, global_scope(db, *file), name);

                // If it's unbound, check if it's present as an instance on `types.ModuleType`
                // or `builtins.object`.
                //
                // We do a more limited version of this in `global_symbol_ty`,
                // but there are two crucial differences here:
                // - If a member is looked up as an attribute, `__init__` is also available
                //   on the module, but it isn't available as a global from inside the module
                // - If a member is looked up as an attribute, members on `builtins.object`
                //   are also available (because `types.ModuleType` inherits from `object`);
                //   these attributes are also not available as globals from inside the module.
                //
                // The same way as in `global_symbol_ty`, however, we need to be careful to
                // ignore `__getattr__`. Typeshed has a fake `__getattr__` on `types.ModuleType`
                // to help out with dynamic imports; we shouldn't use it for `ModuleLiteral` types
                // where we know exactly which module we're dealing with.
                if name != "__getattr__" && global_lookup.possibly_unbound() {
                    // TODO: this should use `.to_instance()`, but we don't understand instance attribute yet
                    let module_type_instance_member =
                        KnownClass::ModuleType.to_class_literal(db).member(db, name);
                    global_lookup.or_fall_back_to(db, &module_type_instance_member)
                } else {
                    global_lookup
                }
            }
            Type::ClassLiteral(class_ty) => class_ty.member(db, name),
            Type::SubclassOf(subclass_of_ty) => subclass_of_ty.member(db, name),
            Type::KnownInstance(known_instance) => known_instance.member(db, name),
            Type::Instance(InstanceType { class }) => {
                let ty = match (class.known(db), name) {
                    (Some(KnownClass::VersionInfo), "major") => {
                        Type::IntLiteral(Program::get(db).target_version(db).major.into())
                    }
                    (Some(KnownClass::VersionInfo), "minor") => {
                        Type::IntLiteral(Program::get(db).target_version(db).minor.into())
                    }
                    // TODO MRO? get_own_instance_member, get_instance_member
                    _ => todo_type!("instance attributes"),
                };
                ty.into()
            }
            Type::Union(union) => {
                let mut builder = UnionBuilder::new(db);

                let mut all_unbound = true;
                let mut possibly_unbound = false;
                for ty in union.elements(db) {
                    let ty_member = ty.member(db, name);
                    match ty_member {
                        Symbol::Unbound => {
                            possibly_unbound = true;
                        }
                        Symbol::Type(ty_member, member_boundness) => {
                            if member_boundness == Boundness::PossiblyUnbound {
                                possibly_unbound = true;
                            }

                            all_unbound = false;
                            builder = builder.add(ty_member);
                        }
                    }
                }

                if all_unbound {
                    Symbol::Unbound
                } else {
                    Symbol::Type(
                        builder.build(),
                        if possibly_unbound {
                            Boundness::PossiblyUnbound
                        } else {
                            Boundness::Bound
                        },
                    )
                }
            }
            Type::Intersection(_) => {
                // TODO perform the get_member on each type in the intersection
                // TODO return the intersection of those results
                todo_type!().into()
            }
            Type::IntLiteral(_) => {
                // TODO raise error
                todo_type!().into()
            }
            Type::BooleanLiteral(_) => todo_type!().into(),
            Type::StringLiteral(_) => {
                // TODO defer to `typing.LiteralString`/`builtins.str` methods
                // from typeshed's stubs
                todo_type!().into()
            }
            Type::LiteralString => {
                // TODO defer to `typing.LiteralString`/`builtins.str` methods
                // from typeshed's stubs
                todo_type!().into()
            }
            Type::BytesLiteral(_) => {
                // TODO defer to Type::Instance(<bytes from typeshed>).member
                todo_type!().into()
            }
            Type::SliceLiteral(_) => {
                // TODO defer to `builtins.slice` methods
                todo_type!().into()
            }
            Type::Tuple(_) => {
                // TODO: implement tuple methods
                todo_type!().into()
            }
            &todo @ Type::Todo(_) => todo.into(),
        }
    }

    /// Resolves the boolean value of a type.
    ///
    /// This is used to determine the value that would be returned
    /// when `bool(x)` is called on an object `x`.
    fn bool(&self, db: &'db dyn Db) -> Truthiness {
        match self {
            Type::Any | Type::Todo(_) | Type::Never | Type::Unknown => Truthiness::Ambiguous,
            Type::FunctionLiteral(_) => Truthiness::AlwaysTrue,
            Type::ModuleLiteral(_) => Truthiness::AlwaysTrue,
            Type::ClassLiteral(_) => {
                // TODO: lookup `__bool__` and `__len__` methods on the class's metaclass
                // More info in https://docs.python.org/3/library/stdtypes.html#truth-value-testing
                Truthiness::Ambiguous
            }
            Type::SubclassOf(_) => {
                // TODO: see above
                Truthiness::Ambiguous
            }
            instance_ty @ Type::Instance(InstanceType { class }) => {
                if class.is_known(db, KnownClass::NoneType) {
                    Truthiness::AlwaysFalse
                } else {
                    // We only check the `__bool__` method for truth testing, even though at
                    // runtime there is a fallback to `__len__`, since `__bool__` takes precedence
                    // and a subclass could add a `__bool__` method. We don't use
                    // `Type::call_dunder` here because of the need to check for `__bool__ = bool`.

                    // Don't trust a maybe-unbound `__bool__` method.
                    let Symbol::Type(bool_method, Boundness::Bound) =
                        instance_ty.to_meta_type(db).member(db, "__bool__")
                    else {
                        return Truthiness::Ambiguous;
                    };

                    // Check if the class has `__bool__ = bool` and avoid infinite recursion, since
                    // `Type::call` on `bool` will call `Type::bool` on the argument.
                    if bool_method
                        .into_class_literal()
                        .is_some_and(|ClassLiteralType { class }| {
                            class.is_known(db, KnownClass::Bool)
                        })
                    {
                        return Truthiness::Ambiguous;
                    }

                    if let Some(Type::BooleanLiteral(bool_val)) =
                        bool_method.call(db, &[*instance_ty]).return_ty(db)
                    {
                        bool_val.into()
                    } else {
                        Truthiness::Ambiguous
                    }
                }
            }
            Type::KnownInstance(known_instance) => known_instance.bool(),
            Type::Union(union) => {
                let union_elements = union.elements(db);
                let first_element_truthiness = union_elements[0].bool(db);
                if first_element_truthiness.is_ambiguous() {
                    return Truthiness::Ambiguous;
                }
                if !union_elements
                    .iter()
                    .skip(1)
                    .all(|element| element.bool(db) == first_element_truthiness)
                {
                    return Truthiness::Ambiguous;
                }
                first_element_truthiness
            }
            Type::Intersection(_) => {
                // TODO
                Truthiness::Ambiguous
            }
            Type::IntLiteral(num) => Truthiness::from(*num != 0),
            Type::BooleanLiteral(bool) => Truthiness::from(*bool),
            Type::StringLiteral(str) => Truthiness::from(!str.value(db).is_empty()),
            Type::LiteralString => Truthiness::Ambiguous,
            Type::BytesLiteral(bytes) => Truthiness::from(!bytes.value(db).is_empty()),
            Type::SliceLiteral(_) => Truthiness::AlwaysTrue,
            Type::Tuple(items) => Truthiness::from(!items.elements(db).is_empty()),
        }
    }

    /// Return the outcome of calling an object of this type.
    #[must_use]
    fn call(self, db: &'db dyn Db, arg_types: &[Type<'db>]) -> CallOutcome<'db> {
        match self {
            // TODO validate typed call arguments vs callable signature
            Type::FunctionLiteral(function_type) => {
                if function_type.is_known(db, KnownFunction::RevealType) {
                    CallOutcome::revealed(
                        function_type.signature(db).return_ty,
                        *arg_types.first().unwrap_or(&Type::Unknown),
                    )
                } else {
                    CallOutcome::callable(function_type.signature(db).return_ty)
                }
            }

            // TODO annotated return type on `__new__` or metaclass `__call__`
            Type::ClassLiteral(ClassLiteralType { class }) => {
                CallOutcome::callable(match class.known(db) {
                    // If the class is the builtin-bool class (for example `bool(1)`), we try to
                    // return the specific truthiness value of the input arg, `Literal[True]` for
                    // the example above.
                    Some(KnownClass::Bool) => arg_types
                        .first()
                        .map(|arg| arg.bool(db).into_type(db))
                        .unwrap_or(Type::BooleanLiteral(false)),
                    _ => Type::Instance(InstanceType { class }),
                })
            }

            instance_ty @ Type::Instance(_) => {
                let args = std::iter::once(self)
                    .chain(arg_types.iter().copied())
                    .collect::<Vec<_>>();
                match instance_ty.call_dunder(db, "__call__", &args) {
                    CallDunderResult::CallOutcome(CallOutcome::NotCallable { .. }) => {
                        // Turn "`<type of illegal '__call__'>` not callable" into
                        // "`X` not callable"
                        CallOutcome::NotCallable {
                            not_callable_ty: self,
                        }
                    }
                    CallDunderResult::CallOutcome(outcome) => outcome,
                    CallDunderResult::PossiblyUnbound(call_outcome) => {
                        // Turn "possibly unbound object of type `Literal['__call__']`"
                        // into "`X` not callable (possibly unbound `__call__` method)"
                        CallOutcome::PossiblyUnboundDunderCall {
                            called_ty: self,
                            call_outcome: Box::new(call_outcome),
                        }
                    }
                    CallDunderResult::MethodNotAvailable => {
                        // Turn "`X.__call__` unbound" into "`X` not callable"
                        CallOutcome::NotCallable {
                            not_callable_ty: self,
                        }
                    }
                }
            }

            // `Any` is callable, and its return type is also `Any`.
            Type::Any => CallOutcome::callable(Type::Any),

            Type::Todo(_) => CallOutcome::callable(todo_type!()),

            Type::Unknown => CallOutcome::callable(Type::Unknown),

            Type::Union(union) => CallOutcome::union(
                self,
                union
                    .elements(db)
                    .iter()
                    .map(|elem| elem.call(db, arg_types)),
            ),

            // TODO: intersection types
            Type::Intersection(_) => CallOutcome::callable(todo_type!()),

            _ => CallOutcome::not_callable(self),
        }
    }

    /// Look up a dunder method on the meta type of `self` and call it.
    fn call_dunder(
        self,
        db: &'db dyn Db,
        name: &str,
        arg_types: &[Type<'db>],
    ) -> CallDunderResult<'db> {
        match self.to_meta_type(db).member(db, name) {
            Symbol::Type(callable_ty, Boundness::Bound) => {
                CallDunderResult::CallOutcome(callable_ty.call(db, arg_types))
            }
            Symbol::Type(callable_ty, Boundness::PossiblyUnbound) => {
                CallDunderResult::PossiblyUnbound(callable_ty.call(db, arg_types))
            }
            Symbol::Unbound => CallDunderResult::MethodNotAvailable,
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
    fn iterate(self, db: &'db dyn Db) -> IterationOutcome<'db> {
        if let Type::Tuple(tuple_type) = self {
            return IterationOutcome::Iterable {
                element_ty: UnionType::from_elements(db, tuple_type.elements(db)),
            };
        }

        if matches!(self, Type::Unknown | Type::Any | Type::Todo(_)) {
            // Explicit handling of `Unknown` and `Any` necessary until `type[Unknown]` and
            // `type[Any]` are not defined as `Todo` anymore.
            return IterationOutcome::Iterable { element_ty: self };
        }

        let dunder_iter_result = self.call_dunder(db, "__iter__", &[self]);
        match dunder_iter_result {
            CallDunderResult::CallOutcome(ref call_outcome)
            | CallDunderResult::PossiblyUnbound(ref call_outcome) => {
                let Some(iterator_ty) = call_outcome.return_ty(db) else {
                    return IterationOutcome::NotIterable {
                        not_iterable_ty: self,
                    };
                };

                return if let Some(element_ty) = iterator_ty
                    .call_dunder(db, "__next__", &[iterator_ty])
                    .return_ty(db)
                {
                    if matches!(dunder_iter_result, CallDunderResult::PossiblyUnbound(..)) {
                        IterationOutcome::PossiblyUnboundDunderIter {
                            iterable_ty: self,
                            element_ty,
                        }
                    } else {
                        IterationOutcome::Iterable { element_ty }
                    }
                } else {
                    IterationOutcome::NotIterable {
                        not_iterable_ty: self,
                    }
                };
            }
            CallDunderResult::MethodNotAvailable => {}
        }

        // Although it's not considered great practice,
        // classes that define `__getitem__` are also iterable,
        // even if they do not define `__iter__`.
        //
        // TODO(Alex) this is only valid if the `__getitem__` method is annotated as
        // accepting `int` or `SupportsIndex`
        if let Some(element_ty) = self
            .call_dunder(db, "__getitem__", &[self, KnownClass::Int.to_instance(db)])
            .return_ty(db)
        {
            IterationOutcome::Iterable { element_ty }
        } else {
            IterationOutcome::NotIterable {
                not_iterable_ty: self,
            }
        }
    }

    #[must_use]
    pub fn to_instance(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::Any => Type::Any,
            todo @ Type::Todo(_) => *todo,
            Type::Unknown => Type::Unknown,
            Type::Never => Type::Never,
            Type::ClassLiteral(ClassLiteralType { class }) => Type::instance(*class),
            Type::SubclassOf(SubclassOfType { class }) => Type::instance(*class),
            Type::Union(union) => union.map(db, |element| element.to_instance(db)),
            // TODO: we can probably do better here: --Alex
            Type::Intersection(_) => todo_type!(),
            // TODO: calling `.to_instance()` on any of these should result in a diagnostic,
            // since they already indicate that the object is an instance of some kind:
            Type::BooleanLiteral(_)
            | Type::BytesLiteral(_)
            | Type::FunctionLiteral(_)
            | Type::Instance(_)
            | Type::KnownInstance(_)
            | Type::ModuleLiteral(_)
            | Type::IntLiteral(_)
            | Type::StringLiteral(_)
            | Type::SliceLiteral(_)
            | Type::Tuple(_)
            | Type::LiteralString => Type::Unknown,
        }
    }

    /// If we see a value of this type used as a type expression, what type does it name?
    ///
    /// For example, the builtin `int` as a value expression is of type
    /// `Type::ClassLiteral(builtins.int)`, that is, it is the `int` class itself. As a type
    /// expression, it names the type `Type::Instance(builtins.int)`, that is, all objects whose
    /// `__class__` is `int`.
    #[must_use]
    pub fn in_type_expression(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::ClassLiteral(_) | Type::SubclassOf(_) => self.to_instance(db),
            Type::Union(union) => union.map(db, |element| element.in_type_expression(db)),
            Type::Unknown => Type::Unknown,
            // TODO map this to a new `Type::TypeVar` variant
            Type::KnownInstance(KnownInstanceType::TypeVar(_)) => *self,
            Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) => alias.value_ty(db),
            Type::KnownInstance(KnownInstanceType::Never | KnownInstanceType::NoReturn) => {
                Type::Never
            }
            _ => todo_type!(),
        }
    }

    /// The type `NoneType` / `None`
    pub fn none(db: &'db dyn Db) -> Type<'db> {
        KnownClass::NoneType.to_instance(db)
    }

    /// Return the type of `tuple(sys.version_info)`.
    ///
    /// This is not exactly the type that `sys.version_info` has at runtime,
    /// but it's a useful fallback for us in order to infer `Literal` types from `sys.version_info` comparisons.
    fn version_info_tuple(db: &'db dyn Db) -> Self {
        let target_version = Program::get(db).target_version(db);
        let int_instance_ty = KnownClass::Int.to_instance(db);

        // TODO: just grab this type from typeshed (it's a `sys._ReleaseLevel` type alias there)
        let release_level_ty = {
            let elements: Box<[Type<'db>]> = ["alpha", "beta", "candidate", "final"]
                .iter()
                .map(|level| Type::string_literal(db, level))
                .collect();

            // For most unions, it's better to go via `UnionType::from_elements` or use `UnionBuilder`;
            // those techniques ensure that union elements are deduplicated and unions are eagerly simplified
            // into other types where necessary. Here, however, we know that there are no duplicates
            // in this union, so it's probably more efficient to use `UnionType::new()` directly.
            Type::Union(UnionType::new(db, elements))
        };

        let version_info_elements = &[
            Type::IntLiteral(target_version.major.into()),
            Type::IntLiteral(target_version.minor.into()),
            int_instance_ty,
            release_level_ty,
            int_instance_ty,
        ];

        Self::tuple(db, version_info_elements)
    }

    /// Given a type that is assumed to represent an instance of a class,
    /// return a type that represents that class itself.
    #[must_use]
    pub fn to_meta_type(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::Never => Type::Never,
            Type::Instance(InstanceType { class }) => {
                Type::SubclassOf(SubclassOfType { class: *class })
            }
            Type::KnownInstance(known_instance) => known_instance.class().to_class_literal(db),
            Type::Union(union) => union.map(db, |ty| ty.to_meta_type(db)),
            Type::BooleanLiteral(_) => KnownClass::Bool.to_class_literal(db),
            Type::BytesLiteral(_) => KnownClass::Bytes.to_class_literal(db),
            Type::SliceLiteral(_) => KnownClass::Slice.to_class_literal(db),
            Type::IntLiteral(_) => KnownClass::Int.to_class_literal(db),
            Type::FunctionLiteral(_) => KnownClass::FunctionType.to_class_literal(db),
            Type::ModuleLiteral(_) => KnownClass::ModuleType.to_class_literal(db),
            Type::Tuple(_) => KnownClass::Tuple.to_class_literal(db),
            Type::ClassLiteral(ClassLiteralType { class }) => class.metaclass(db),
            Type::SubclassOf(SubclassOfType { class }) => Type::subclass_of(
                class
                    .try_metaclass(db)
                    .ok()
                    .and_then(Type::into_class_literal)
                    .unwrap_or_else(|| KnownClass::Type.to_class_literal(db).expect_class_literal())
                    .class,
            ),
            Type::StringLiteral(_) | Type::LiteralString => KnownClass::Str.to_class_literal(db),
            // TODO: `type[Any]`?
            Type::Any => Type::Any,
            // TODO: `type[Unknown]`?
            Type::Unknown => Type::Unknown,
            // TODO intersections
            Type::Intersection(_) => todo_type!(),
            todo @ Type::Todo(_) => *todo,
        }
    }

    /// Return the string representation of this type when converted to string as it would be
    /// provided by the `__str__` method.
    ///
    /// When not available, this should fall back to the value of `[Type::repr]`.
    /// Note: this method is used in the builtins `format`, `print`, `str.format` and `f-strings`.
    #[must_use]
    pub fn str(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::IntLiteral(_) | Type::BooleanLiteral(_) => self.repr(db),
            Type::StringLiteral(_) | Type::LiteralString => *self,
            Type::KnownInstance(known_instance) => {
                Type::string_literal(db, known_instance.repr(db))
            }
            // TODO: handle more complex types
            _ => KnownClass::Str.to_instance(db),
        }
    }

    /// Return the string representation of this type as it would be provided by the  `__repr__`
    /// method at runtime.
    #[must_use]
    pub fn repr(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::IntLiteral(number) => Type::string_literal(db, &number.to_string()),
            Type::BooleanLiteral(true) => Type::string_literal(db, "True"),
            Type::BooleanLiteral(false) => Type::string_literal(db, "False"),
            Type::StringLiteral(literal) => {
                Type::string_literal(db, &format!("'{}'", literal.value(db).escape_default()))
            }
            Type::LiteralString => Type::LiteralString,
            Type::KnownInstance(known_instance) => {
                Type::string_literal(db, known_instance.repr(db))
            }
            // TODO: handle more complex types
            _ => KnownClass::Str.to_instance(db),
        }
    }
}

impl<'db> From<&Type<'db>> for Type<'db> {
    fn from(value: &Type<'db>) -> Self {
        *value
    }
}

impl<'db> From<Type<'db>> for Symbol<'db> {
    fn from(value: Type<'db>) -> Self {
        Symbol::Type(value, Boundness::Bound)
    }
}

/// Non-exhaustive enumeration of known classes (e.g. `builtins.int`, `typing.Any`, ...) to allow
/// for easier syntax when interacting with very common classes.
///
/// Feel free to expand this enum if you ever find yourself using the same class in multiple
/// places.
/// Note: good candidates are any classes in `[crate::stdlib::CoreStdlibModule]`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnownClass {
    // To figure out where an stdlib symbol is defined, you can go into `crates/red_knot_vendored`
    // and grep for the symbol name in any `.pyi` file.

    // Builtins
    Bool,
    Object,
    Bytes,
    Type,
    Int,
    Float,
    Str,
    List,
    Tuple,
    Set,
    Dict,
    Slice,
    // Types
    GenericAlias,
    ModuleType,
    FunctionType,
    // Typeshed
    NoneType, // Part of `types` for Python >= 3.10
    // Typing
    SpecialForm,
    TypeVar,
    TypeAliasType,
    NoDefaultType,
    // sys
    VersionInfo,
}

impl<'db> KnownClass {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Object => "object",
            Self::Bytes => "bytes",
            Self::Tuple => "tuple",
            Self::Int => "int",
            Self::Float => "float",
            Self::Str => "str",
            Self::Set => "set",
            Self::Dict => "dict",
            Self::List => "list",
            Self::Type => "type",
            Self::Slice => "slice",
            Self::GenericAlias => "GenericAlias",
            Self::ModuleType => "ModuleType",
            Self::FunctionType => "FunctionType",
            Self::NoneType => "NoneType",
            Self::SpecialForm => "_SpecialForm",
            Self::TypeVar => "TypeVar",
            Self::TypeAliasType => "TypeAliasType",
            Self::NoDefaultType => "_NoDefaultType",
            // This is the name the type of `sys.version_info` has in typeshed,
            // which is different to what `type(sys.version_info).__name__` is at runtime.
            // (At runtime, `type(sys.version_info).__name__ == "version_info"`,
            // which is impossible to replicate in the stubs since the sole instance of the class
            // also has that name in the `sys` module.)
            Self::VersionInfo => "_version_info",
        }
    }

    pub fn to_instance(&self, db: &'db dyn Db) -> Type<'db> {
        self.to_class_literal(db).to_instance(db)
    }

    pub fn to_class_literal(self, db: &'db dyn Db) -> Type<'db> {
        core_module_symbol(db, self.canonical_module(), self.as_str())
            .ignore_possibly_unbound()
            .unwrap_or(Type::Unknown)
    }

    /// Return the module in which we should look up the definition for this class
    pub(crate) const fn canonical_module(self) -> CoreStdlibModule {
        match self {
            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Type
            | Self::Int
            | Self::Float
            | Self::Str
            | Self::List
            | Self::Tuple
            | Self::Set
            | Self::Dict
            | Self::Slice => CoreStdlibModule::Builtins,
            Self::VersionInfo => CoreStdlibModule::Sys,
            Self::GenericAlias | Self::ModuleType | Self::FunctionType => CoreStdlibModule::Types,
            Self::NoneType => CoreStdlibModule::Typeshed,
            Self::SpecialForm | Self::TypeVar | Self::TypeAliasType => CoreStdlibModule::Typing,
            // TODO when we understand sys.version_info, we will need an explicit fallback here,
            // because typing_extensions has a 3.13+ re-export for the `typing.NoDefault`
            // singleton, but not for `typing._NoDefaultType`
            Self::NoDefaultType => CoreStdlibModule::TypingExtensions,
        }
    }

    /// Is this class a singleton class?
    ///
    /// A singleton class is a class where it is known that only one instance can ever exist at runtime.
    const fn is_singleton(self) -> bool {
        // TODO there are other singleton types (EllipsisType, NotImplementedType)
        match self {
            Self::NoneType | Self::NoDefaultType | Self::VersionInfo | Self::TypeAliasType => true,
            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Tuple
            | Self::Int
            | Self::Float
            | Self::Str
            | Self::Set
            | Self::Dict
            | Self::List
            | Self::Type
            | Self::Slice
            | Self::GenericAlias
            | Self::ModuleType
            | Self::FunctionType
            | Self::SpecialForm
            | Self::TypeVar => false,
        }
    }

    pub fn try_from_file(db: &dyn Db, file: File, class_name: &str) -> Option<Self> {
        // Note: if this becomes hard to maintain (as rust can't ensure at compile time that all
        // variants of `Self` are covered), we might use a macro (in-house or dependency)
        // See: https://stackoverflow.com/q/39070244
        let candidate = match class_name {
            "bool" => Self::Bool,
            "object" => Self::Object,
            "bytes" => Self::Bytes,
            "tuple" => Self::Tuple,
            "type" => Self::Type,
            "int" => Self::Int,
            "float" => Self::Float,
            "str" => Self::Str,
            "set" => Self::Set,
            "dict" => Self::Dict,
            "list" => Self::List,
            "slice" => Self::Slice,
            "GenericAlias" => Self::GenericAlias,
            "NoneType" => Self::NoneType,
            "ModuleType" => Self::ModuleType,
            "FunctionType" => Self::FunctionType,
            "TypeAliasType" => Self::TypeAliasType,
            "_SpecialForm" => Self::SpecialForm,
            "_NoDefaultType" => Self::NoDefaultType,
            "_version_info" => Self::VersionInfo,
            _ => return None,
        };

        let module = file_to_module(db, file)?;
        candidate.check_module(&module).then_some(candidate)
    }

    /// Return `true` if the module of `self` matches `module_name`
    fn check_module(self, module: &Module) -> bool {
        if !module.search_path().is_standard_library() {
            return false;
        }
        match self {
            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Type
            | Self::Int
            | Self::Float
            | Self::Str
            | Self::List
            | Self::Tuple
            | Self::Set
            | Self::Dict
            | Self::Slice
            | Self::GenericAlias
            | Self::ModuleType
            | Self::VersionInfo
            | Self::FunctionType => module.name() == self.canonical_module().as_str(),
            Self::NoneType => matches!(module.name().as_str(), "_typeshed" | "types"),
            Self::SpecialForm | Self::TypeVar | Self::TypeAliasType | Self::NoDefaultType => {
                matches!(module.name().as_str(), "typing" | "typing_extensions")
            }
        }
    }
}

/// Enumeration of specific runtime that are special enough to be considered their own type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub enum KnownInstanceType<'db> {
    /// The symbol `typing.Literal` (which can also be found as `typing_extensions.Literal`)
    Literal,
    /// The symbol `typing.Optional` (which can also be found as `typing_extensions.Optional`)
    Optional,
    /// The symbol `typing.Union` (which can also be found as `typing_extensions.Union`)
    Union,
    /// The symbol `typing.NoReturn` (which can also be found as `typing_extensions.NoReturn`)
    NoReturn,
    /// The symbol `typing.Never` available since 3.11 (which can also be found as `typing_extensions.Never`)
    Never,
    /// A single instance of `typing.TypeVar`
    TypeVar(TypeVarInstance<'db>),
    /// A single instance of `typing.TypeAliasType` (PEP 695 type alias)
    TypeAliasType(TypeAliasType<'db>),
    // TODO: fill this enum out with more special forms, etc.
}

impl<'db> KnownInstanceType<'db> {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Literal => "Literal",
            Self::Optional => "Optional",
            Self::Union => "Union",
            Self::TypeVar(_) => "TypeVar",
            Self::NoReturn => "NoReturn",
            Self::Never => "Never",
            Self::TypeAliasType(_) => "TypeAliasType",
        }
    }

    /// Evaluate the known instance in boolean context
    pub const fn bool(self) -> Truthiness {
        match self {
            Self::Literal
            | Self::Optional
            | Self::TypeVar(_)
            | Self::Union
            | Self::NoReturn
            | Self::Never
            | Self::TypeAliasType(_) => Truthiness::AlwaysTrue,
        }
    }

    /// Return the repr of the symbol at runtime
    pub fn repr(self, db: &'db dyn Db) -> &'db str {
        match self {
            Self::Literal => "typing.Literal",
            Self::Optional => "typing.Optional",
            Self::Union => "typing.Union",
            Self::NoReturn => "typing.NoReturn",
            Self::Never => "typing.Never",
            Self::TypeVar(typevar) => typevar.name(db),
            Self::TypeAliasType(_) => "typing.TypeAliasType",
        }
    }

    /// Return the [`KnownClass`] which this symbol is an instance of
    pub const fn class(self) -> KnownClass {
        match self {
            Self::Literal => KnownClass::SpecialForm,
            Self::Optional => KnownClass::SpecialForm,
            Self::Union => KnownClass::SpecialForm,
            Self::NoReturn => KnownClass::SpecialForm,
            Self::Never => KnownClass::SpecialForm,
            Self::TypeVar(_) => KnownClass::TypeVar,
            Self::TypeAliasType(_) => KnownClass::TypeAliasType,
        }
    }

    /// Return the instance type which this type is a subtype of.
    ///
    /// For example, the symbol `typing.Literal` is an instance of `typing._SpecialForm`,
    /// so `KnownInstanceType::Literal.instance_fallback(db)`
    /// returns `Type::Instance(InstanceType { class: <typing._SpecialForm> })`.
    pub fn instance_fallback(self, db: &dyn Db) -> Type {
        self.class().to_instance(db)
    }

    pub fn try_from_module_and_symbol(module: &Module, instance_name: &str) -> Option<Self> {
        if !module.search_path().is_standard_library() {
            return None;
        }
        match (module.name().as_str(), instance_name) {
            ("typing" | "typing_extensions", "Literal") => Some(Self::Literal),
            ("typing" | "typing_extensions", "Optional") => Some(Self::Optional),
            ("typing" | "typing_extensions", "Union") => Some(Self::Union),
            ("typing" | "typing_extensions", "NoReturn") => Some(Self::NoReturn),
            ("typing" | "typing_extensions", "Never") => Some(Self::Never),
            _ => None,
        }
    }

    fn member(self, db: &'db dyn Db, name: &str) -> Symbol<'db> {
        let ty = match (self, name) {
            (Self::TypeVar(typevar), "__name__") => Type::string_literal(db, typevar.name(db)),
            (Self::TypeAliasType(alias), "__name__") => Type::string_literal(db, alias.name(db)),
            _ => return self.instance_fallback(db).member(db, name),
        };
        ty.into()
    }
}

/// Data regarding a single type variable.
///
/// This is referenced by `KnownInstanceType::TypeVar` (to represent the singleton type of the
/// runtime `typing.TypeVar` object itself). In the future, it will also be referenced also by a
/// new `Type` variant to represent the type that this typevar represents as an annotation: that
/// is, an unknown set of objects, constrained by the upper-bound/constraints on this type var,
/// defaulting to the default type of this type var when not otherwise bound to a type.
///
/// This must be a tracked struct, not an interned one, because typevar equivalence is by identity,
/// not by value. Two typevars that have the same name, bound/constraints, and default, are still
/// different typevars: if used in the same scope, they may be bound to different types.
#[salsa::tracked]
pub struct TypeVarInstance<'db> {
    /// The name of this TypeVar (e.g. `T`)
    #[return_ref]
    name: ast::name::Name,

    /// The upper bound or constraint on the type of this TypeVar
    bound_or_constraints: Option<TypeVarBoundOrConstraints<'db>>,

    /// The default type for this TypeVar
    default_ty: Option<Type<'db>>,
}

impl<'db> TypeVarInstance<'db> {
    #[allow(unused)]
    pub(crate) fn upper_bound(self, db: &'db dyn Db) -> Option<Type<'db>> {
        if let Some(TypeVarBoundOrConstraints::UpperBound(ty)) = self.bound_or_constraints(db) {
            Some(ty)
        } else {
            None
        }
    }

    #[allow(unused)]
    pub(crate) fn constraints(self, db: &'db dyn Db) -> Option<&[Type<'db>]> {
        if let Some(TypeVarBoundOrConstraints::Constraints(tuple)) = self.bound_or_constraints(db) {
            Some(tuple.elements(db))
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, salsa::Update)]
pub enum TypeVarBoundOrConstraints<'db> {
    UpperBound(Type<'db>),
    Constraints(TupleType<'db>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CallOutcome<'db> {
    Callable {
        return_ty: Type<'db>,
    },
    RevealType {
        return_ty: Type<'db>,
        revealed_ty: Type<'db>,
    },
    NotCallable {
        not_callable_ty: Type<'db>,
    },
    Union {
        called_ty: Type<'db>,
        outcomes: Box<[CallOutcome<'db>]>,
    },
    PossiblyUnboundDunderCall {
        called_ty: Type<'db>,
        call_outcome: Box<CallOutcome<'db>>,
    },
}

impl<'db> CallOutcome<'db> {
    /// Create a new `CallOutcome::Callable` with given return type.
    fn callable(return_ty: Type<'db>) -> CallOutcome<'db> {
        CallOutcome::Callable { return_ty }
    }

    /// Create a new `CallOutcome::NotCallable` with given not-callable type.
    fn not_callable(not_callable_ty: Type<'db>) -> CallOutcome<'db> {
        CallOutcome::NotCallable { not_callable_ty }
    }

    /// Create a new `CallOutcome::RevealType` with given revealed and return types.
    fn revealed(return_ty: Type<'db>, revealed_ty: Type<'db>) -> CallOutcome<'db> {
        CallOutcome::RevealType {
            return_ty,
            revealed_ty,
        }
    }

    /// Create a new `CallOutcome::Union` with given wrapped outcomes.
    fn union(
        called_ty: Type<'db>,
        outcomes: impl IntoIterator<Item = CallOutcome<'db>>,
    ) -> CallOutcome<'db> {
        CallOutcome::Union {
            called_ty,
            outcomes: outcomes.into_iter().collect(),
        }
    }

    /// Get the return type of the call, or `None` if not callable.
    fn return_ty(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::Callable { return_ty } => Some(*return_ty),
            Self::RevealType {
                return_ty,
                revealed_ty: _,
            } => Some(*return_ty),
            Self::NotCallable { not_callable_ty: _ } => None,
            Self::Union {
                outcomes,
                called_ty: _,
            } => outcomes
                .iter()
                // If all outcomes are NotCallable, we return None; if some outcomes are callable
                // and some are not, we return a union including Unknown.
                .fold(None, |acc, outcome| {
                    let ty = outcome.return_ty(db);
                    match (acc, ty) {
                        (None, None) => None,
                        (None, Some(ty)) => Some(UnionBuilder::new(db).add(ty)),
                        (Some(builder), ty) => Some(builder.add(ty.unwrap_or(Type::Unknown))),
                    }
                })
                .map(UnionBuilder::build),
            Self::PossiblyUnboundDunderCall { call_outcome, .. } => call_outcome.return_ty(db),
        }
    }

    /// Get the return type of the call, emitting default diagnostics if needed.
    fn unwrap_with_diagnostic<'a>(
        &self,
        db: &'db dyn Db,
        node: ast::AnyNodeRef,
        diagnostics: &'a mut TypeCheckDiagnosticsBuilder<'db>,
    ) -> Type<'db> {
        match self.return_ty_result(db, node, diagnostics) {
            Ok(return_ty) => return_ty,
            Err(NotCallableError::Type {
                not_callable_ty,
                return_ty,
            }) => {
                diagnostics.add(
                    node,
                    "call-non-callable",
                    format_args!(
                        "Object of type `{}` is not callable",
                        not_callable_ty.display(db)
                    ),
                );
                return_ty
            }
            Err(NotCallableError::UnionElement {
                not_callable_ty,
                called_ty,
                return_ty,
            }) => {
                diagnostics.add(
                    node,
                    "call-non-callable",
                    format_args!(
                        "Object of type `{}` is not callable (due to union element `{}`)",
                        called_ty.display(db),
                        not_callable_ty.display(db),
                    ),
                );
                return_ty
            }
            Err(NotCallableError::UnionElements {
                not_callable_tys,
                called_ty,
                return_ty,
            }) => {
                diagnostics.add(
                    node,
                    "call-non-callable",
                    format_args!(
                        "Object of type `{}` is not callable (due to union elements {})",
                        called_ty.display(db),
                        not_callable_tys.display(db),
                    ),
                );
                return_ty
            }
            Err(NotCallableError::PossiblyUnboundDunderCall {
                callable_ty: called_ty,
                return_ty,
            }) => {
                diagnostics.add(
                    node,
                    "call-non-callable",
                    format_args!(
                        "Object of type `{}` is not callable (possibly unbound `__call__` method)",
                        called_ty.display(db)
                    ),
                );
                return_ty
            }
        }
    }

    /// Get the return type of the call as a result.
    fn return_ty_result<'a>(
        &self,
        db: &'db dyn Db,
        node: ast::AnyNodeRef,
        diagnostics: &'a mut TypeCheckDiagnosticsBuilder<'db>,
    ) -> Result<Type<'db>, NotCallableError<'db>> {
        match self {
            Self::Callable { return_ty } => Ok(*return_ty),
            Self::RevealType {
                return_ty,
                revealed_ty,
            } => {
                diagnostics.add(
                    node,
                    "revealed-type",
                    format_args!("Revealed type is `{}`", revealed_ty.display(db)),
                );
                Ok(*return_ty)
            }
            Self::NotCallable { not_callable_ty } => Err(NotCallableError::Type {
                not_callable_ty: *not_callable_ty,
                return_ty: Type::Unknown,
            }),
            Self::PossiblyUnboundDunderCall {
                called_ty,
                call_outcome,
            } => Err(NotCallableError::PossiblyUnboundDunderCall {
                callable_ty: *called_ty,
                return_ty: call_outcome.return_ty(db).unwrap_or(Type::Unknown),
            }),
            Self::Union {
                outcomes,
                called_ty,
            } => {
                let mut not_callable = vec![];
                let mut union_builder = UnionBuilder::new(db);
                let mut revealed = false;
                for outcome in outcomes {
                    let return_ty = match outcome {
                        Self::NotCallable { not_callable_ty } => {
                            not_callable.push(*not_callable_ty);
                            Type::Unknown
                        }
                        Self::RevealType {
                            return_ty,
                            revealed_ty: _,
                        } => {
                            if revealed {
                                *return_ty
                            } else {
                                revealed = true;
                                outcome.unwrap_with_diagnostic(db, node, diagnostics)
                            }
                        }
                        _ => outcome.unwrap_with_diagnostic(db, node, diagnostics),
                    };
                    union_builder = union_builder.add(return_ty);
                }
                let return_ty = union_builder.build();
                match not_callable[..] {
                    [] => Ok(return_ty),
                    [elem] => Err(NotCallableError::UnionElement {
                        not_callable_ty: elem,
                        called_ty: *called_ty,
                        return_ty,
                    }),
                    _ if not_callable.len() == outcomes.len() => Err(NotCallableError::Type {
                        not_callable_ty: *called_ty,
                        return_ty,
                    }),
                    _ => Err(NotCallableError::UnionElements {
                        not_callable_tys: not_callable.into_boxed_slice(),
                        called_ty: *called_ty,
                        return_ty,
                    }),
                }
            }
        }
    }
}

enum CallDunderResult<'db> {
    CallOutcome(CallOutcome<'db>),
    PossiblyUnbound(CallOutcome<'db>),
    MethodNotAvailable,
}

impl<'db> CallDunderResult<'db> {
    fn return_ty(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::CallOutcome(outcome) => outcome.return_ty(db),
            Self::PossiblyUnbound { .. } => None,
            Self::MethodNotAvailable => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NotCallableError<'db> {
    /// The type is not callable.
    Type {
        not_callable_ty: Type<'db>,
        return_ty: Type<'db>,
    },
    /// A single union element is not callable.
    UnionElement {
        not_callable_ty: Type<'db>,
        called_ty: Type<'db>,
        return_ty: Type<'db>,
    },
    /// Multiple (but not all) union elements are not callable.
    UnionElements {
        not_callable_tys: Box<[Type<'db>]>,
        called_ty: Type<'db>,
        return_ty: Type<'db>,
    },
    PossiblyUnboundDunderCall {
        callable_ty: Type<'db>,
        return_ty: Type<'db>,
    },
}

impl<'db> NotCallableError<'db> {
    /// The return type that should be used when a call is not callable.
    fn return_ty(&self) -> Type<'db> {
        match self {
            Self::Type { return_ty, .. } => *return_ty,
            Self::UnionElement { return_ty, .. } => *return_ty,
            Self::UnionElements { return_ty, .. } => *return_ty,
            Self::PossiblyUnboundDunderCall { return_ty, .. } => *return_ty,
        }
    }

    /// The resolved type that was not callable.
    ///
    /// For unions, returns the union type itself, which may contain a mix of callable and
    /// non-callable types.
    fn called_ty(&self) -> Type<'db> {
        match self {
            Self::Type {
                not_callable_ty, ..
            } => *not_callable_ty,
            Self::UnionElement { called_ty, .. } => *called_ty,
            Self::UnionElements { called_ty, .. } => *called_ty,
            Self::PossiblyUnboundDunderCall {
                callable_ty: called_ty,
                ..
            } => *called_ty,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IterationOutcome<'db> {
    Iterable {
        element_ty: Type<'db>,
    },
    NotIterable {
        not_iterable_ty: Type<'db>,
    },
    PossiblyUnboundDunderIter {
        iterable_ty: Type<'db>,
        element_ty: Type<'db>,
    },
}

impl<'db> IterationOutcome<'db> {
    fn unwrap_with_diagnostic(
        self,
        iterable_node: ast::AnyNodeRef,
        diagnostics: &mut TypeCheckDiagnosticsBuilder<'db>,
    ) -> Type<'db> {
        match self {
            Self::Iterable { element_ty } => element_ty,
            Self::NotIterable { not_iterable_ty } => {
                diagnostics.add_not_iterable(iterable_node, not_iterable_ty);
                Type::Unknown
            }
            Self::PossiblyUnboundDunderIter {
                iterable_ty,
                element_ty,
            } => {
                diagnostics.add_not_iterable_possibly_unbound(iterable_node, iterable_ty);
                element_ty
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Truthiness {
    /// For an object `x`, `bool(x)` will always return `True`
    AlwaysTrue,
    /// For an object `x`, `bool(x)` will always return `False`
    AlwaysFalse,
    /// For an object `x`, `bool(x)` could return either `True` or `False`
    Ambiguous,
}

impl Truthiness {
    const fn is_ambiguous(self) -> bool {
        matches!(self, Truthiness::Ambiguous)
    }

    const fn negate(self) -> Self {
        match self {
            Self::AlwaysTrue => Self::AlwaysFalse,
            Self::AlwaysFalse => Self::AlwaysTrue,
            Self::Ambiguous => Self::Ambiguous,
        }
    }

    fn into_type(self, db: &dyn Db) -> Type {
        match self {
            Self::AlwaysTrue => Type::BooleanLiteral(true),
            Self::AlwaysFalse => Type::BooleanLiteral(false),
            Self::Ambiguous => KnownClass::Bool.to_instance(db),
        }
    }
}

impl From<bool> for Truthiness {
    fn from(value: bool) -> Self {
        if value {
            Truthiness::AlwaysTrue
        } else {
            Truthiness::AlwaysFalse
        }
    }
}

#[salsa::interned]
pub struct FunctionType<'db> {
    /// name of the function at definition
    #[return_ref]
    pub name: ast::name::Name,

    /// Is this a function that we special-case somehow? If so, which one?
    known: Option<KnownFunction>,

    body_scope: ScopeId<'db>,

    /// types of all decorators on this function
    decorators: Box<[Type<'db>]>,
}

#[salsa::tracked]
impl<'db> FunctionType<'db> {
    pub fn has_decorator(self, db: &dyn Db, decorator: Type<'_>) -> bool {
        self.decorators(db).contains(&decorator)
    }

    /// Typed externally-visible signature for this function.
    ///
    /// This is the signature as seen by external callers, possibly modified by decorators and/or
    /// overloaded.
    ///
    /// ## Why is this a salsa query?
    ///
    /// This is a salsa query to short-circuit the invalidation
    /// when the function's AST node changes.
    ///
    /// Were this not a salsa query, then the calling query
    /// would depend on the function's AST and rerun for every change in that file.
    #[salsa::tracked(return_ref)]
    pub fn signature(self, db: &'db dyn Db) -> Signature<'db> {
        let function_stmt_node = self.body_scope(db).node(db).expect_function();
        let internal_signature = self.internal_signature(db);
        if function_stmt_node.decorator_list.is_empty() {
            return internal_signature;
        }
        // TODO process the effect of decorators on the signature
        Signature::todo()
    }

    /// Typed internally-visible signature for this function.
    ///
    /// This represents the annotations on the function itself, unmodified by decorators and
    /// overloads.
    ///
    /// These are the parameter and return types that should be used for type checking the body of
    /// the function.
    ///
    /// Don't call this when checking any other file; only when type-checking the function body
    /// scope.
    fn internal_signature(self, db: &'db dyn Db) -> Signature<'db> {
        let scope = self.body_scope(db);
        let function_stmt_node = scope.node(db).expect_function();
        let definition = semantic_index(db, scope.file(db)).definition(function_stmt_node);
        Signature::from_function(db, definition, function_stmt_node)
    }

    pub fn is_known(self, db: &'db dyn Db, known_function: KnownFunction) -> bool {
        self.known(db) == Some(known_function)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnownConstraintFunction {
    /// `builtins.isinstance`
    IsInstance,
    /// `builtins.issubclass`
    IsSubclass,
}

/// Non-exhaustive enumeration of known functions (e.g. `builtins.reveal_type`, ...) that might
/// have special behavior.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum KnownFunction {
    ConstraintFunction(KnownConstraintFunction),
    /// `builtins.reveal_type`, `typing.reveal_type` or `typing_extensions.reveal_type`
    RevealType,
}

impl KnownFunction {
    pub fn constraint_function(self) -> Option<KnownConstraintFunction> {
        match self {
            Self::ConstraintFunction(f) => Some(f),
            Self::RevealType => None,
        }
    }

    fn from_definition<'db>(
        db: &'db dyn Db,
        definition: Definition<'db>,
        name: &str,
    ) -> Option<Self> {
        match name {
            "reveal_type" if definition.is_typing_definition(db) => Some(KnownFunction::RevealType),
            "isinstance" if definition.is_builtin_definition(db) => Some(
                KnownFunction::ConstraintFunction(KnownConstraintFunction::IsInstance),
            ),
            "issubclass" if definition.is_builtin_definition(db) => Some(
                KnownFunction::ConstraintFunction(KnownConstraintFunction::IsSubclass),
            ),
            _ => None,
        }
    }
}

/// Representation of a runtime class object.
///
/// Does not in itself represent a type,
/// but is used as the inner data for several structs that *do* represent types.
#[salsa::interned]
pub struct Class<'db> {
    /// Name of the class at definition
    #[return_ref]
    pub name: ast::name::Name,

    body_scope: ScopeId<'db>,

    known: Option<KnownClass>,
}

#[salsa::tracked]
impl<'db> Class<'db> {
    /// Return `true` if this class represents `known_class`
    pub fn is_known(self, db: &'db dyn Db, known_class: KnownClass) -> bool {
        self.known(db) == Some(known_class)
    }

    /// Return an iterator over the inferred types of this class's *explicit* bases.
    ///
    /// Note that any class (except for `object`) that has no explicit
    /// bases will implicitly inherit from `object` at runtime. Nonetheless,
    /// this method does *not* include `object` in the bases it iterates over.
    ///
    /// ## Why is this a salsa query?
    ///
    /// This is a salsa query to short-circuit the invalidation
    /// when the class's AST node changes.
    ///
    /// Were this not a salsa query, then the calling query
    /// would depend on the class's AST and rerun for every change in that file.
    fn explicit_bases(self, db: &'db dyn Db) -> &[Type<'db>] {
        self.explicit_bases_query(db)
    }

    /// Iterate over this class's explicit bases, filtering out any bases that are not class objects.
    fn fully_static_explicit_bases(self, db: &'db dyn Db) -> impl Iterator<Item = Class<'db>> {
        self.explicit_bases(db)
            .iter()
            .copied()
            .filter_map(Type::into_class_literal)
            .map(|ClassLiteralType { class }| class)
    }

    #[salsa::tracked(return_ref)]
    fn explicit_bases_query(self, db: &'db dyn Db) -> Box<[Type<'db>]> {
        let class_stmt = self.node(db);

        let class_definition = semantic_index(db, self.file(db)).definition(class_stmt);

        class_stmt
            .bases()
            .iter()
            .map(|base_node| definition_expression_ty(db, class_definition, base_node))
            .collect()
    }

    fn file(self, db: &dyn Db) -> File {
        self.body_scope(db).file(db)
    }

    /// Return the original [`ast::StmtClassDef`] node associated with this class
    ///
    /// ## Note
    /// Only call this function from queries in the same file or your
    /// query depends on the AST of another file (bad!).
    fn node(self, db: &'db dyn Db) -> &'db ast::StmtClassDef {
        self.body_scope(db).node(db).expect_class()
    }

    /// Attempt to resolve the [method resolution order] ("MRO") for this class.
    /// If the MRO is unresolvable, return an error indicating why the class's MRO
    /// cannot be accurately determined. The error returned contains a fallback MRO
    /// that will be used instead for the purposes of type inference.
    ///
    /// The MRO is the tuple of classes that can be retrieved as the `__mro__`
    /// attribute on a class at runtime.
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    #[salsa::tracked(return_ref)]
    fn try_mro(self, db: &'db dyn Db) -> Result<Mro<'db>, MroError<'db>> {
        Mro::of_class(db, self)
    }

    /// Iterate over the [method resolution order] ("MRO") of the class.
    ///
    /// If the MRO could not be accurately resolved, this method falls back to iterating
    /// over an MRO that has the class directly inheriting from `Unknown`. Use
    /// [`Class::try_mro`] if you need to distinguish between the success and failure
    /// cases rather than simply iterating over the inferred resolution order for the class.
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    fn iter_mro(self, db: &'db dyn Db) -> impl Iterator<Item = ClassBase<'db>> {
        MroIterator::new(db, self)
    }

    /// Return `true` if `other` is present in this class's MRO.
    pub fn is_subclass_of(self, db: &'db dyn Db, other: Class) -> bool {
        // `is_subclass_of` is checking the subtype relation, in which gradual types do not
        // participate, so we should not return `True` if we find `Any/Unknown` in the MRO.
        self.iter_mro(db).contains(&ClassBase::Class(other))
    }

    /// Return the explicit `metaclass` of this class, if one is defined.
    ///
    /// ## Note
    /// Only call this function from queries in the same file or your
    /// query depends on the AST of another file (bad!).
    fn explicit_metaclass(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let class_stmt = self.node(db);
        let metaclass_node = &class_stmt
            .arguments
            .as_ref()?
            .find_keyword("metaclass")?
            .value;
        let class_definition = semantic_index(db, self.file(db)).definition(class_stmt);
        let metaclass_ty = definition_expression_ty(db, class_definition, metaclass_node);
        Some(metaclass_ty)
    }

    /// Return the metaclass of this class, or `Unknown` if the metaclass cannot be inferred.
    pub(crate) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        // TODO: `type[Unknown]` would be a more precise fallback
        self.try_metaclass(db).unwrap_or(Type::Unknown)
    }

    /// Return the metaclass of this class, or an error if the metaclass cannot be inferred.
    #[salsa::tracked]
    pub(crate) fn try_metaclass(self, db: &'db dyn Db) -> Result<Type<'db>, MetaclassError<'db>> {
        // Identify the class's own metaclass (or take the first base class's metaclass).
        let mut base_classes = self.fully_static_explicit_bases(db).peekable();

        if base_classes.peek().is_some() && self.is_cyclically_defined(db) {
            // We emit diagnostics for cyclic class definitions elsewhere.
            // Avoid attempting to infer the metaclass if the class is cyclically defined:
            // it would be easy to enter an infinite loop.
            //
            // TODO: `type[Unknown]` might be better here?
            return Ok(Type::Unknown);
        }

        let explicit_metaclass = self.explicit_metaclass(db);
        let (metaclass, class_metaclass_was_from) = if let Some(metaclass) = explicit_metaclass {
            (metaclass, self)
        } else if let Some(base_class) = base_classes.next() {
            (base_class.metaclass(db), base_class)
        } else {
            (KnownClass::Type.to_class_literal(db), self)
        };

        let mut candidate = if let Type::ClassLiteral(metaclass_ty) = metaclass {
            MetaclassCandidate {
                metaclass: metaclass_ty.class,
                explicit_metaclass_of: class_metaclass_was_from,
            }
        } else {
            // TODO: If the metaclass is not a class, we should verify that it's a callable
            // which accepts the same arguments as `type.__new__` (otherwise error), and return
            // the meta-type of its return type. (And validate that is a class type?)
            return Ok(todo_type!("metaclass not a class"));
        };

        // Reconcile all base classes' metaclasses with the candidate metaclass.
        //
        // See:
        // - https://docs.python.org/3/reference/datamodel.html#determining-the-appropriate-metaclass
        // - https://github.com/python/cpython/blob/83ba8c2bba834c0b92de669cac16fcda17485e0e/Objects/typeobject.c#L3629-L3663
        for base_class in base_classes {
            let metaclass = base_class.metaclass(db);
            let Type::ClassLiteral(metaclass) = metaclass else {
                continue;
            };
            if metaclass.class.is_subclass_of(db, candidate.metaclass) {
                candidate = MetaclassCandidate {
                    metaclass: metaclass.class,
                    explicit_metaclass_of: base_class,
                };
                continue;
            }
            if candidate.metaclass.is_subclass_of(db, metaclass.class) {
                continue;
            }
            return Err(MetaclassError {
                kind: MetaclassErrorKind::Conflict {
                    candidate1: candidate,
                    candidate2: MetaclassCandidate {
                        metaclass: metaclass.class,
                        explicit_metaclass_of: base_class,
                    },
                    candidate1_is_base_class: explicit_metaclass.is_none(),
                },
            });
        }

        Ok(Type::class_literal(candidate.metaclass))
    }

    /// Returns the class member of this class named `name`.
    ///
    /// The member resolves to a member on the class itself or any of its proper superclasses.
    pub(crate) fn class_member(self, db: &'db dyn Db, name: &str) -> Symbol<'db> {
        if name == "__mro__" {
            let tuple_elements: Vec<Type<'db>> = self.iter_mro(db).map(Type::from).collect();
            return Type::tuple(db, &tuple_elements).into();
        }

        if name == "__class__" {
            return self.metaclass(db).into();
        }

        for superclass in self.iter_mro(db) {
            match superclass {
                // TODO we may instead want to record the fact that we encountered dynamic, and intersect it with
                // the type found on the next "real" class.
                ClassBase::Any | ClassBase::Unknown | ClassBase::Todo => {
                    return Type::from(superclass).member(db, name)
                }
                ClassBase::Class(class) => {
                    let member = class.own_class_member(db, name);
                    if !member.is_unbound() {
                        return member;
                    }
                }
            }
        }

        Symbol::Unbound
    }

    /// Returns the inferred type of the class member named `name`.
    ///
    /// Returns [`Symbol::Unbound`] if `name` cannot be found in this class's scope
    /// directly. Use [`Class::class_member`] if you require a method that will
    /// traverse through the MRO until it finds the member.
    pub(crate) fn own_class_member(self, db: &'db dyn Db, name: &str) -> Symbol<'db> {
        let scope = self.body_scope(db);
        symbol(db, scope, name)
    }

    /// Return `true` if this class appears to be a cyclic definition,
    /// i.e., it inherits either directly or indirectly from itself.
    ///
    /// A class definition like this will fail at runtime,
    /// but we must be resilient to it or we could panic.
    #[salsa::tracked]
    fn is_cyclically_defined(self, db: &'db dyn Db) -> bool {
        fn is_cyclically_defined_recursive<'db>(
            db: &'db dyn Db,
            class: Class<'db>,
            classes_to_watch: &mut IndexSet<Class<'db>>,
        ) -> bool {
            if !classes_to_watch.insert(class) {
                return true;
            }
            for explicit_base_class in class.fully_static_explicit_bases(db) {
                // Each base must be considered in isolation.
                // This is due to the fact that if a class uses multiple inheritance,
                // there could easily be a situation where two bases have the same class in their MROs;
                // that isn't enough to constitute the class being cyclically defined.
                let classes_to_watch_len = classes_to_watch.len();
                if is_cyclically_defined_recursive(db, explicit_base_class, classes_to_watch) {
                    return true;
                }
                classes_to_watch.truncate(classes_to_watch_len);
            }
            false
        }

        self.fully_static_explicit_bases(db)
            .any(|base_class| is_cyclically_defined_recursive(db, base_class, &mut IndexSet::new()))
    }
}

#[salsa::interned]
pub struct TypeAliasType<'db> {
    #[return_ref]
    pub name: ast::name::Name,

    rhs_scope: ScopeId<'db>,
}

#[salsa::tracked]
impl<'db> TypeAliasType<'db> {
    #[salsa::tracked]
    pub fn value_ty(self, db: &'db dyn Db) -> Type<'db> {
        let scope = self.rhs_scope(db);

        let type_alias_stmt_node = scope.node(db).expect_type_alias();
        let definition = semantic_index(db, scope.file(db)).definition(type_alias_stmt_node);

        definition_expression_ty(db, definition, &type_alias_stmt_node.value)
    }
}

/// Either the explicit `metaclass=` keyword of the class, or the inferred metaclass of one of its base classes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MetaclassCandidate<'db> {
    metaclass: Class<'db>,
    explicit_metaclass_of: Class<'db>,
}

/// A singleton type representing a single class object at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub struct ClassLiteralType<'db> {
    class: Class<'db>,
}

impl<'db> ClassLiteralType<'db> {
    fn member(self, db: &'db dyn Db, name: &str) -> Symbol<'db> {
        self.class.class_member(db, name)
    }
}

impl<'db> From<ClassLiteralType<'db>> for Type<'db> {
    fn from(value: ClassLiteralType<'db>) -> Self {
        Self::ClassLiteral(value)
    }
}

/// A type that represents `type[C]`, i.e. the class literal `C` and class literals that are subclasses of `C`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub struct SubclassOfType<'db> {
    class: Class<'db>,
}

impl<'db> SubclassOfType<'db> {
    fn member(self, db: &'db dyn Db, name: &str) -> Symbol<'db> {
        self.class.class_member(db, name)
    }
}

/// A type representing the set of runtime objects which are instances of a certain class.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update)]
pub struct InstanceType<'db> {
    class: Class<'db>,
}

impl<'db> InstanceType<'db> {
    /// Return `true` if members of this type are instances of the class `class` at runtime.
    pub fn is_instance_of(self, db: &'db dyn Db, class: Class<'db>) -> bool {
        self.class.is_subclass_of(db, class)
    }
}

impl<'db> From<InstanceType<'db>> for Type<'db> {
    fn from(value: InstanceType<'db>) -> Self {
        Self::Instance(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MetaclassError<'db> {
    kind: MetaclassErrorKind<'db>,
}

impl<'db> MetaclassError<'db> {
    /// Return an [`MetaclassErrorKind`] variant describing why we could not resolve the metaclass for this class.
    pub(super) fn reason(&self) -> &MetaclassErrorKind<'db> {
        &self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum MetaclassErrorKind<'db> {
    /// The class has incompatible metaclasses in its inheritance hierarchy.
    ///
    /// The metaclass of a derived class must be a (non-strict) subclass of the metaclasses of all
    /// its bases.
    Conflict {
        /// `candidate1` will either be the explicit `metaclass=` keyword in the class definition,
        /// or the inferred metaclass of a base class
        candidate1: MetaclassCandidate<'db>,

        /// `candidate2` will always be the inferred metaclass of a base class
        candidate2: MetaclassCandidate<'db>,

        /// Flag to indicate whether `candidate1` is the explicit `metaclass=` keyword or the
        /// inferred metaclass of a base class. This helps us give better error messages in diagnostics.
        candidate1_is_base_class: bool,
    },
}

#[salsa::interned]
pub struct UnionType<'db> {
    /// The union type includes values in any of these types.
    #[return_ref]
    elements_boxed: Box<[Type<'db>]>,
}

impl<'db> UnionType<'db> {
    fn elements(self, db: &'db dyn Db) -> &'db [Type<'db>] {
        self.elements_boxed(db)
    }

    /// Create a union from a list of elements
    /// (which may be eagerly simplified into a different variant of [`Type`] altogether).
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
    /// and create a new union from the resulting set of types.
    pub fn map(
        &self,
        db: &'db dyn Db,
        transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        Self::from_elements(db, self.elements(db).iter().map(transform_fn))
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

impl<'db> StringLiteralType<'db> {
    pub fn len(&self, db: &'db dyn Db) -> usize {
        self.value(db).len()
    }
}

#[salsa::interned]
pub struct BytesLiteralType<'db> {
    #[return_ref]
    value: Box<[u8]>,
}

#[salsa::interned]
pub struct SliceLiteralType<'db> {
    start: Option<i32>,
    stop: Option<i32>,
    step: Option<i32>,
}

impl<'db> SliceLiteralType<'db> {
    fn as_tuple(self, db: &dyn Db) -> (Option<i32>, Option<i32>, Option<i32>) {
        (self.start(db), self.stop(db), self.step(db))
    }
}
#[salsa::interned]
pub struct TupleType<'db> {
    #[return_ref]
    elements: Box<[Type<'db>]>,
}

impl<'db> TupleType<'db> {
    pub fn get(&self, db: &'db dyn Db, index: usize) -> Option<Type<'db>> {
        self.elements(db).get(index).copied()
    }

    pub fn len(&self, db: &'db dyn Db) -> usize {
        self.elements(db).len()
    }
}

// Make sure that the `Type` enum does not grow unexpectedly.
#[cfg(not(debug_assertions))]
#[cfg(target_pointer_width = "64")]
static_assertions::assert_eq_size!(Type, [u8; 16]);

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::tests::TestDb;
    use crate::program::{Program, SearchPathSettings};
    use crate::python_version::PythonVersion;
    use crate::stdlib::typing_symbol;
    use crate::ProgramSettings;
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
    use ruff_db::testing::assert_function_query_was_not_run;
    use ruff_python_ast as ast;
    use test_case::test_case;

    pub(crate) fn setup_db() -> TestDb {
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
    #[derive(Debug, Clone)]
    enum Ty {
        Never,
        Unknown,
        None,
        Any,
        Todo,
        IntLiteral(i64),
        BooleanLiteral(bool),
        StringLiteral(&'static str),
        LiteralString,
        BytesLiteral(&'static str),
        // BuiltinInstance("str") corresponds to an instance of the builtin `str` class
        BuiltinInstance(&'static str),
        TypingInstance(&'static str),
        TypingLiteral,
        // BuiltinClassLiteral("str") corresponds to the builtin `str` class object itself
        BuiltinClassLiteral(&'static str),
        KnownClassInstance(KnownClass),
        Union(Vec<Ty>),
        Intersection { pos: Vec<Ty>, neg: Vec<Ty> },
        Tuple(Vec<Ty>),
    }

    impl Ty {
        fn into_type(self, db: &TestDb) -> Type<'_> {
            match self {
                Ty::Never => Type::Never,
                Ty::Unknown => Type::Unknown,
                Ty::None => Type::none(db),
                Ty::Any => Type::Any,
                Ty::Todo => todo_type!("Ty::Todo"),
                Ty::IntLiteral(n) => Type::IntLiteral(n),
                Ty::StringLiteral(s) => Type::string_literal(db, s),
                Ty::BooleanLiteral(b) => Type::BooleanLiteral(b),
                Ty::LiteralString => Type::LiteralString,
                Ty::BytesLiteral(s) => Type::bytes_literal(db, s.as_bytes()),
                Ty::BuiltinInstance(s) => builtins_symbol(db, s).expect_type().to_instance(db),
                Ty::TypingInstance(s) => typing_symbol(db, s).expect_type().to_instance(db),
                Ty::TypingLiteral => Type::KnownInstance(KnownInstanceType::Literal),
                Ty::BuiltinClassLiteral(s) => builtins_symbol(db, s).expect_type(),
                Ty::KnownClassInstance(known_class) => known_class.to_instance(db),
                Ty::Union(tys) => {
                    UnionType::from_elements(db, tys.into_iter().map(|ty| ty.into_type(db)))
                }
                Ty::Intersection { pos, neg } => {
                    let mut builder = IntersectionBuilder::new(db);
                    for p in pos {
                        builder = builder.add_positive(p.into_type(db));
                    }
                    for n in neg {
                        builder = builder.add_negative(n.into_type(db));
                    }
                    builder.build()
                }
                Ty::Tuple(tys) => {
                    let elements: Vec<Type> = tys.into_iter().map(|ty| ty.into_type(db)).collect();
                    Type::tuple(db, &elements)
                }
            }
        }
    }

    #[test_case(Ty::BuiltinInstance("str"), Ty::BuiltinInstance("object"))]
    #[test_case(Ty::BuiltinInstance("int"), Ty::BuiltinInstance("object"))]
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
    #[test_case(Ty::IntLiteral(1), Ty::Union(vec![Ty::BuiltinInstance("int"), Ty::BuiltinInstance("str")]))]
    #[test_case(Ty::IntLiteral(1), Ty::Union(vec![Ty::Unknown, Ty::BuiltinInstance("str")]))]
    #[test_case(Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]), Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]))]
    #[test_case(
        Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]),
        Ty::BuiltinInstance("int")
    )]
    #[test_case(
        Ty::Union(vec![Ty::IntLiteral(1), Ty::None]),
        Ty::Union(vec![Ty::BuiltinInstance("int"), Ty::None])
    )]
    #[test_case(Ty::Tuple(vec![Ty::Todo]), Ty::Tuple(vec![Ty::IntLiteral(2)]))]
    #[test_case(Ty::Tuple(vec![Ty::IntLiteral(2)]), Ty::Tuple(vec![Ty::Todo]))]
    fn is_assignable_to(from: Ty, to: Ty) {
        let db = setup_db();
        assert!(from.into_type(&db).is_assignable_to(&db, to.into_type(&db)));
    }

    #[test_case(Ty::BuiltinInstance("object"), Ty::BuiltinInstance("int"))]
    #[test_case(Ty::IntLiteral(1), Ty::BuiltinInstance("str"))]
    #[test_case(Ty::BuiltinInstance("int"), Ty::BuiltinInstance("str"))]
    #[test_case(Ty::BuiltinInstance("int"), Ty::IntLiteral(1))]
    #[test_case(
        Ty::Union(vec![Ty::IntLiteral(1), Ty::None]),
        Ty::BuiltinInstance("int")
    )]
    #[test_case(
        Ty::Union(vec![Ty::IntLiteral(1), Ty::None]),
        Ty::Union(vec![Ty::BuiltinInstance("str"), Ty::None])
    )]
    fn is_not_assignable_to(from: Ty, to: Ty) {
        let db = setup_db();
        assert!(!from.into_type(&db).is_assignable_to(&db, to.into_type(&db)));
    }

    #[test_case(Ty::BuiltinInstance("str"), Ty::BuiltinInstance("object"))]
    #[test_case(Ty::BuiltinInstance("int"), Ty::BuiltinInstance("object"))]
    #[test_case(Ty::BuiltinInstance("bool"), Ty::BuiltinInstance("object"))]
    #[test_case(Ty::BuiltinInstance("bool"), Ty::BuiltinInstance("int"))]
    #[test_case(Ty::Never, Ty::IntLiteral(1))]
    #[test_case(Ty::IntLiteral(1), Ty::BuiltinInstance("int"))]
    #[test_case(Ty::IntLiteral(1), Ty::BuiltinInstance("object"))]
    #[test_case(Ty::BooleanLiteral(true), Ty::BuiltinInstance("bool"))]
    #[test_case(Ty::BooleanLiteral(true), Ty::BuiltinInstance("int"))]
    #[test_case(Ty::BooleanLiteral(true), Ty::BuiltinInstance("object"))]
    #[test_case(Ty::StringLiteral("foo"), Ty::BuiltinInstance("str"))]
    #[test_case(Ty::StringLiteral("foo"), Ty::BuiltinInstance("object"))]
    #[test_case(Ty::StringLiteral("foo"), Ty::LiteralString)]
    #[test_case(Ty::LiteralString, Ty::BuiltinInstance("str"))]
    #[test_case(Ty::LiteralString, Ty::BuiltinInstance("object"))]
    #[test_case(Ty::BytesLiteral("foo"), Ty::BuiltinInstance("bytes"))]
    #[test_case(Ty::BytesLiteral("foo"), Ty::BuiltinInstance("object"))]
    #[test_case(Ty::IntLiteral(1), Ty::Union(vec![Ty::BuiltinInstance("int"), Ty::BuiltinInstance("str")]))]
    #[test_case(Ty::Union(vec![Ty::BuiltinInstance("str"), Ty::BuiltinInstance("int")]), Ty::BuiltinInstance("object"))]
    #[test_case(Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]), Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2), Ty::IntLiteral(3)]))]
    #[test_case(Ty::BuiltinInstance("TypeError"), Ty::BuiltinInstance("Exception"))]
    #[test_case(Ty::Tuple(vec![]), Ty::Tuple(vec![]))]
    #[test_case(Ty::Tuple(vec![Ty::IntLiteral(42)]), Ty::Tuple(vec![Ty::BuiltinInstance("int")]))]
    #[test_case(Ty::Tuple(vec![Ty::IntLiteral(42), Ty::StringLiteral("foo")]), Ty::Tuple(vec![Ty::BuiltinInstance("int"), Ty::BuiltinInstance("str")]))]
    #[test_case(Ty::Tuple(vec![Ty::BuiltinInstance("int"), Ty::StringLiteral("foo")]), Ty::Tuple(vec![Ty::BuiltinInstance("int"), Ty::BuiltinInstance("str")]))]
    #[test_case(Ty::Tuple(vec![Ty::IntLiteral(42), Ty::BuiltinInstance("str")]), Ty::Tuple(vec![Ty::BuiltinInstance("int"), Ty::BuiltinInstance("str")]))]
    #[test_case(
        Ty::BuiltinInstance("FloatingPointError"),
        Ty::BuiltinInstance("Exception")
    )]
    #[test_case(Ty::Intersection{pos: vec![Ty::BuiltinInstance("int")], neg: vec![Ty::IntLiteral(2)]}, Ty::BuiltinInstance("int"))]
    #[test_case(Ty::Intersection{pos: vec![Ty::BuiltinInstance("int")], neg: vec![Ty::IntLiteral(2)]}, Ty::Intersection{pos: vec![], neg: vec![Ty::IntLiteral(2)]})]
    #[test_case(Ty::Intersection{pos: vec![], neg: vec![Ty::BuiltinInstance("int")]}, Ty::Intersection{pos: vec![], neg: vec![Ty::IntLiteral(2)]})]
    #[test_case(Ty::IntLiteral(1), Ty::Intersection{pos: vec![Ty::BuiltinInstance("int")], neg: vec![Ty::IntLiteral(2)]})]
    #[test_case(Ty::Intersection{pos: vec![Ty::BuiltinInstance("str")], neg: vec![Ty::StringLiteral("foo")]}, Ty::Intersection{pos: vec![], neg: vec![Ty::IntLiteral(2)]})]
    #[test_case(Ty::BuiltinClassLiteral("int"), Ty::BuiltinClassLiteral("int"))]
    #[test_case(Ty::BuiltinClassLiteral("int"), Ty::BuiltinInstance("object"))]
    #[test_case(Ty::TypingLiteral, Ty::TypingInstance("_SpecialForm"))]
    #[test_case(Ty::TypingLiteral, Ty::BuiltinInstance("object"))]
    fn is_subtype_of(from: Ty, to: Ty) {
        let db = setup_db();
        assert!(from.into_type(&db).is_subtype_of(&db, to.into_type(&db)));
    }

    #[test_case(Ty::BuiltinInstance("object"), Ty::BuiltinInstance("int"))]
    #[test_case(Ty::Unknown, Ty::IntLiteral(1))]
    #[test_case(Ty::Any, Ty::IntLiteral(1))]
    #[test_case(Ty::IntLiteral(1), Ty::Unknown)]
    #[test_case(Ty::IntLiteral(1), Ty::Any)]
    #[test_case(Ty::IntLiteral(1), Ty::Union(vec![Ty::Unknown, Ty::BuiltinInstance("str")]))]
    #[test_case(Ty::IntLiteral(1), Ty::BuiltinInstance("str"))]
    #[test_case(Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]), Ty::IntLiteral(1))]
    #[test_case(Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]), Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(3)]))]
    #[test_case(Ty::BuiltinInstance("int"), Ty::BuiltinInstance("str"))]
    #[test_case(Ty::BuiltinInstance("int"), Ty::IntLiteral(1))]
    #[test_case(Ty::Tuple(vec![]), Ty::Tuple(vec![Ty::IntLiteral(1)]))]
    #[test_case(Ty::Tuple(vec![Ty::IntLiteral(42)]), Ty::Tuple(vec![Ty::BuiltinInstance("str")]))]
    #[test_case(Ty::Tuple(vec![Ty::Todo]), Ty::Tuple(vec![Ty::IntLiteral(2)]))]
    #[test_case(Ty::Tuple(vec![Ty::IntLiteral(2)]), Ty::Tuple(vec![Ty::Todo]))]
    #[test_case(Ty::Intersection{pos: vec![Ty::BuiltinInstance("int")], neg: vec![Ty::IntLiteral(2)]}, Ty::Intersection{pos: vec![Ty::BuiltinInstance("int")], neg: vec![Ty::IntLiteral(3)]})]
    #[test_case(Ty::Intersection{pos: vec![], neg: vec![Ty::IntLiteral(2)]}, Ty::Intersection{pos: vec![], neg: vec![Ty::IntLiteral(3)]})]
    #[test_case(Ty::Intersection{pos: vec![], neg: vec![Ty::IntLiteral(2)]}, Ty::Intersection{pos: vec![], neg: vec![Ty::BuiltinInstance("int")]})]
    #[test_case(Ty::BuiltinInstance("int"), Ty::Intersection{pos: vec![], neg: vec![Ty::IntLiteral(3)]})]
    #[test_case(Ty::IntLiteral(1), Ty::Intersection{pos: vec![Ty::BuiltinInstance("int")], neg: vec![Ty::IntLiteral(1)]})]
    #[test_case(Ty::BuiltinClassLiteral("int"), Ty::BuiltinClassLiteral("object"))]
    #[test_case(Ty::BuiltinInstance("int"), Ty::BuiltinClassLiteral("int"))]
    fn is_not_subtype_of(from: Ty, to: Ty) {
        let db = setup_db();
        assert!(!from.into_type(&db).is_subtype_of(&db, to.into_type(&db)));
    }

    #[test]
    fn is_subtype_of_class_literals() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/module.py",
            "
            class Base: ...
            class Derived(Base): ...
            class Unrelated: ...
            U = Base if flag else Unrelated
        ",
        )
        .unwrap();
        let module = ruff_db::files::system_path_to_file(&db, "/src/module.py").unwrap();

        // `literal_base` represents `Literal[Base]`.
        let literal_base = super::global_symbol(&db, module, "Base").expect_type();
        let literal_derived = super::global_symbol(&db, module, "Derived").expect_type();
        let u = super::global_symbol(&db, module, "U").expect_type();

        assert!(literal_base.is_class_literal());
        assert!(literal_base.is_subtype_of(&db, Ty::BuiltinInstance("type").into_type(&db)));
        assert!(literal_base.is_subtype_of(&db, Ty::BuiltinInstance("object").into_type(&db)));

        assert!(literal_derived.is_class_literal());

        // `subclass_of_base` represents `Type[Base]`.
        let subclass_of_base = Type::subclass_of(literal_base.expect_class_literal().class);
        assert!(literal_base.is_subtype_of(&db, subclass_of_base));
        assert!(literal_derived.is_subtype_of(&db, subclass_of_base));

        let subclass_of_derived = Type::subclass_of(literal_derived.expect_class_literal().class);
        assert!(literal_derived.is_subtype_of(&db, subclass_of_derived));
        assert!(!literal_base.is_subtype_of(&db, subclass_of_derived));

        // Type[Derived] <: Type[Base]
        assert!(subclass_of_derived.is_subtype_of(&db, subclass_of_base));

        assert!(u.is_union());
        assert!(u.is_subtype_of(&db, Ty::BuiltinInstance("type").into_type(&db)));
        assert!(u.is_subtype_of(&db, Ty::BuiltinInstance("object").into_type(&db)));
    }

    #[test]
    fn is_subtype_of_intersection_of_class_instances() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/module.py",
            "
            class A: ...
            a = A()
            class B: ...
            b = B()
        ",
        )
        .unwrap();
        let module = ruff_db::files::system_path_to_file(&db, "/src/module.py").unwrap();

        let a_ty = super::global_symbol(&db, module, "a").expect_type();
        let b_ty = super::global_symbol(&db, module, "b").expect_type();
        let intersection = IntersectionBuilder::new(&db)
            .add_positive(a_ty)
            .add_positive(b_ty)
            .build();

        assert_eq!(intersection.display(&db).to_string(), "A & B");
        assert!(!a_ty.is_subtype_of(&db, b_ty));
        assert!(intersection.is_subtype_of(&db, b_ty));
        assert!(intersection.is_subtype_of(&db, a_ty));
    }

    #[test_case(
        Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]),
        Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)])
    )]
    fn is_equivalent_to(from: Ty, to: Ty) {
        let db = setup_db();

        assert!(from.into_type(&db).is_equivalent_to(&db, to.into_type(&db)));
    }

    #[test_case(Ty::Never, Ty::Never)]
    #[test_case(Ty::Never, Ty::None)]
    #[test_case(Ty::Never, Ty::BuiltinInstance("int"))]
    #[test_case(Ty::None, Ty::BooleanLiteral(true))]
    #[test_case(Ty::None, Ty::IntLiteral(1))]
    #[test_case(Ty::None, Ty::StringLiteral("test"))]
    #[test_case(Ty::None, Ty::BytesLiteral("test"))]
    #[test_case(Ty::None, Ty::LiteralString)]
    #[test_case(Ty::None, Ty::BuiltinInstance("int"))]
    #[test_case(Ty::None, Ty::Tuple(vec![Ty::None]))]
    #[test_case(Ty::BooleanLiteral(true), Ty::BooleanLiteral(false))]
    #[test_case(Ty::BooleanLiteral(true), Ty::Tuple(vec![Ty::None]))]
    #[test_case(Ty::BooleanLiteral(true), Ty::IntLiteral(1))]
    #[test_case(Ty::BooleanLiteral(false), Ty::IntLiteral(0))]
    #[test_case(Ty::IntLiteral(1), Ty::IntLiteral(2))]
    #[test_case(Ty::IntLiteral(1), Ty::Tuple(vec![Ty::None]))]
    #[test_case(Ty::StringLiteral("a"), Ty::StringLiteral("b"))]
    #[test_case(Ty::StringLiteral("a"), Ty::Tuple(vec![Ty::None]))]
    #[test_case(Ty::LiteralString, Ty::BytesLiteral("a"))]
    #[test_case(Ty::BytesLiteral("a"), Ty::BytesLiteral("b"))]
    #[test_case(Ty::BytesLiteral("a"), Ty::Tuple(vec![Ty::None]))]
    #[test_case(Ty::BytesLiteral("a"), Ty::StringLiteral("a"))]
    #[test_case(Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]), Ty::IntLiteral(3))]
    #[test_case(Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]), Ty::Union(vec![Ty::IntLiteral(3), Ty::IntLiteral(4)]))]
    #[test_case(Ty::Intersection{pos: vec![Ty::BuiltinInstance("int"),  Ty::IntLiteral(1)], neg: vec![]}, Ty::IntLiteral(2))]
    #[test_case(Ty::Tuple(vec![Ty::IntLiteral(1)]), Ty::Tuple(vec![Ty::IntLiteral(2)]))]
    #[test_case(Ty::Tuple(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]), Ty::Tuple(vec![Ty::IntLiteral(1)]))]
    #[test_case(Ty::Tuple(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]), Ty::Tuple(vec![Ty::IntLiteral(1), Ty::IntLiteral(3)]))]
    fn is_disjoint_from(a: Ty, b: Ty) {
        let db = setup_db();
        let a = a.into_type(&db);
        let b = b.into_type(&db);

        assert!(a.is_disjoint_from(&db, b));
        assert!(b.is_disjoint_from(&db, a));
    }

    #[test_case(Ty::Any, Ty::BuiltinInstance("int"))]
    #[test_case(Ty::None, Ty::None)]
    #[test_case(Ty::None, Ty::BuiltinInstance("object"))]
    #[test_case(Ty::BuiltinInstance("int"), Ty::BuiltinInstance("int"))]
    #[test_case(Ty::BuiltinInstance("str"), Ty::LiteralString)]
    #[test_case(Ty::BooleanLiteral(true), Ty::BooleanLiteral(true))]
    #[test_case(Ty::BooleanLiteral(false), Ty::BooleanLiteral(false))]
    #[test_case(Ty::BooleanLiteral(true), Ty::BuiltinInstance("bool"))]
    #[test_case(Ty::BooleanLiteral(true), Ty::BuiltinInstance("int"))]
    #[test_case(Ty::IntLiteral(1), Ty::IntLiteral(1))]
    #[test_case(Ty::StringLiteral("a"), Ty::StringLiteral("a"))]
    #[test_case(Ty::StringLiteral("a"), Ty::LiteralString)]
    #[test_case(Ty::StringLiteral("a"), Ty::BuiltinInstance("str"))]
    #[test_case(Ty::LiteralString, Ty::LiteralString)]
    #[test_case(Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]), Ty::IntLiteral(2))]
    #[test_case(Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]), Ty::Union(vec![Ty::IntLiteral(2), Ty::IntLiteral(3)]))]
    #[test_case(Ty::Intersection{pos: vec![Ty::BuiltinInstance("int"), Ty::IntLiteral(2)], neg: vec![]}, Ty::IntLiteral(2))]
    #[test_case(Ty::Tuple(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]), Ty::Tuple(vec![Ty::IntLiteral(1), Ty::BuiltinInstance("int")]))]
    #[test_case(Ty::BuiltinClassLiteral("str"), Ty::BuiltinInstance("type"))]
    fn is_not_disjoint_from(a: Ty, b: Ty) {
        let db = setup_db();
        let a = a.into_type(&db);
        let b = b.into_type(&db);

        assert!(!a.is_disjoint_from(&db, b));
        assert!(!b.is_disjoint_from(&db, a));
    }

    #[test]
    fn is_disjoint_from_union_of_class_types() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/module.py",
            "
            class A: ...
            class B: ...
            U = A if flag else B
        ",
        )
        .unwrap();
        let module = ruff_db::files::system_path_to_file(&db, "/src/module.py").unwrap();

        let type_a = super::global_symbol(&db, module, "A").expect_type();
        let type_u = super::global_symbol(&db, module, "U").expect_type();

        assert!(type_a.is_class_literal());
        assert!(type_u.is_union());

        assert!(!type_a.is_disjoint_from(&db, type_u));
    }

    #[test]
    fn is_disjoint_type_subclass_of() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/module.py",
            "
            class A: ...
            class B: ...
        ",
        )
        .unwrap();
        let module = ruff_db::files::system_path_to_file(&db, "/src/module.py").unwrap();

        let literal_a = super::global_symbol(&db, module, "A").expect_type();
        let literal_b = super::global_symbol(&db, module, "B").expect_type();

        let subclass_of_a = Type::subclass_of(literal_a.expect_class_literal().class);
        let subclass_of_b = Type::subclass_of(literal_b.expect_class_literal().class);

        // Class literals are always disjoint. They are singleton types
        assert!(literal_a.is_disjoint_from(&db, literal_b));

        // The class A is a subclass of A, so A is not disjoint from type[A]
        assert!(!literal_a.is_disjoint_from(&db, subclass_of_a));

        // The class A is disjoint from type[B] because it's not a subclass
        // of B:
        assert!(literal_a.is_disjoint_from(&db, subclass_of_b));

        // However, type[A] is not disjoint from type[B], as there could be
        // classes that inherit from both A and B:
        assert!(!subclass_of_a.is_disjoint_from(&db, subclass_of_b));
    }

    #[test]
    fn is_disjoint_module_literals() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/module.py",
            "
            import random
            import math
        ",
        )
        .unwrap();

        let module = ruff_db::files::system_path_to_file(&db, "/src/module.py").unwrap();

        let module_literal_random = super::global_symbol(&db, module, "random").expect_type();
        let module_literal_math = super::global_symbol(&db, module, "math").expect_type();

        assert!(module_literal_random.is_disjoint_from(&db, module_literal_math));

        assert!(!module_literal_random.is_disjoint_from(
            &db,
            Ty::KnownClassInstance(KnownClass::ModuleType).into_type(&db)
        ));
        assert!(!module_literal_random.is_disjoint_from(
            &db,
            Ty::KnownClassInstance(KnownClass::Object).into_type(&db)
        ));
    }

    #[test]
    fn is_disjoint_function_literals() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/module.py",
            "
            def f(): ...
            def g(): ...
        ",
        )
        .unwrap();

        let module = ruff_db::files::system_path_to_file(&db, "/src/module.py").unwrap();

        let function_literal_f = super::global_symbol(&db, module, "f").expect_type();
        let function_literal_g = super::global_symbol(&db, module, "g").expect_type();

        assert!(function_literal_f.is_disjoint_from(&db, function_literal_g));

        assert!(!function_literal_f.is_disjoint_from(
            &db,
            Ty::KnownClassInstance(KnownClass::FunctionType).into_type(&db)
        ));
        assert!(!function_literal_f.is_disjoint_from(
            &db,
            Ty::KnownClassInstance(KnownClass::Object).into_type(&db)
        ));
    }

    #[test_case(Ty::None)]
    #[test_case(Ty::BooleanLiteral(true))]
    #[test_case(Ty::BooleanLiteral(false))]
    #[test_case(Ty::KnownClassInstance(KnownClass::NoDefaultType))]
    fn is_singleton(from: Ty) {
        let db = setup_db();

        assert!(from.into_type(&db).is_singleton(&db));
    }

    #[test_case(Ty::None)]
    #[test_case(Ty::BooleanLiteral(true))]
    #[test_case(Ty::IntLiteral(1))]
    #[test_case(Ty::StringLiteral("abc"))]
    #[test_case(Ty::BytesLiteral("abc"))]
    #[test_case(Ty::Tuple(vec![]))]
    #[test_case(Ty::Tuple(vec![Ty::BooleanLiteral(true), Ty::IntLiteral(1)]))]
    fn is_single_valued(from: Ty) {
        let db = setup_db();

        assert!(from.into_type(&db).is_single_valued(&db));
    }

    #[test_case(Ty::Never)]
    #[test_case(Ty::Any)]
    #[test_case(Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]))]
    #[test_case(Ty::Tuple(vec![Ty::None, Ty::BuiltinInstance("int")]))]
    #[test_case(Ty::BuiltinInstance("str"))]
    #[test_case(Ty::LiteralString)]
    fn is_not_single_valued(from: Ty) {
        let db = setup_db();

        assert!(!from.into_type(&db).is_single_valued(&db));
    }

    #[test_case(Ty::Never)]
    #[test_case(Ty::IntLiteral(345))]
    #[test_case(Ty::BuiltinInstance("str"))]
    #[test_case(Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]))]
    #[test_case(Ty::Tuple(vec![]))]
    #[test_case(Ty::Tuple(vec![Ty::None]))]
    #[test_case(Ty::Tuple(vec![Ty::None, Ty::BooleanLiteral(true)]))]
    fn is_not_singleton(from: Ty) {
        let db = setup_db();

        assert!(!from.into_type(&db).is_singleton(&db));
    }

    #[test_case(Ty::IntLiteral(1); "is_int_literal_truthy")]
    #[test_case(Ty::IntLiteral(-1))]
    #[test_case(Ty::StringLiteral("foo"))]
    #[test_case(Ty::Tuple(vec![Ty::IntLiteral(0)]))]
    #[test_case(Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(2)]))]
    fn is_truthy(ty: Ty) {
        let db = setup_db();
        assert_eq!(ty.into_type(&db).bool(&db), Truthiness::AlwaysTrue);
    }

    #[test_case(Ty::Tuple(vec![]))]
    #[test_case(Ty::IntLiteral(0))]
    #[test_case(Ty::StringLiteral(""))]
    #[test_case(Ty::Union(vec![Ty::IntLiteral(0), Ty::IntLiteral(0)]))]
    fn is_falsy(ty: Ty) {
        let db = setup_db();
        assert_eq!(ty.into_type(&db).bool(&db), Truthiness::AlwaysFalse);
    }

    #[test_case(Ty::BuiltinInstance("str"))]
    #[test_case(Ty::Union(vec![Ty::IntLiteral(1), Ty::IntLiteral(0)]))]
    #[test_case(Ty::Union(vec![Ty::BuiltinInstance("str"), Ty::IntLiteral(0)]))]
    #[test_case(Ty::Union(vec![Ty::BuiltinInstance("str"), Ty::IntLiteral(1)]))]
    fn boolean_value_is_unknown(ty: Ty) {
        let db = setup_db();
        assert_eq!(ty.into_type(&db).bool(&db), Truthiness::Ambiguous);
    }

    #[test_case(Ty::IntLiteral(1), Ty::StringLiteral("1"))]
    #[test_case(Ty::BooleanLiteral(true), Ty::StringLiteral("True"))]
    #[test_case(Ty::BooleanLiteral(false), Ty::StringLiteral("False"))]
    #[test_case(Ty::StringLiteral("ab'cd"), Ty::StringLiteral("ab'cd"))] // no quotes
    #[test_case(Ty::LiteralString, Ty::LiteralString)]
    #[test_case(Ty::BuiltinInstance("int"), Ty::BuiltinInstance("str"))]
    fn has_correct_str(ty: Ty, expected: Ty) {
        let db = setup_db();

        assert_eq!(ty.into_type(&db).str(&db), expected.into_type(&db));
    }

    #[test_case(Ty::IntLiteral(1), Ty::StringLiteral("1"))]
    #[test_case(Ty::BooleanLiteral(true), Ty::StringLiteral("True"))]
    #[test_case(Ty::BooleanLiteral(false), Ty::StringLiteral("False"))]
    #[test_case(Ty::StringLiteral("ab'cd"), Ty::StringLiteral("'ab\\'cd'"))] // single quotes
    #[test_case(Ty::LiteralString, Ty::LiteralString)]
    #[test_case(Ty::BuiltinInstance("int"), Ty::BuiltinInstance("str"))]
    fn has_correct_repr(ty: Ty, expected: Ty) {
        let db = setup_db();

        assert_eq!(ty.into_type(&db).repr(&db), expected.into_type(&db));
    }

    #[test]
    fn typing_vs_typeshed_no_default() {
        let db = setup_db();

        let typing_no_default = typing_symbol(&db, "NoDefault").expect_type();
        let typing_extensions_no_default = typing_extensions_symbol(&db, "NoDefault").expect_type();

        assert_eq!(typing_no_default.display(&db).to_string(), "NoDefault");
        assert_eq!(
            typing_extensions_no_default.display(&db).to_string(),
            "NoDefault"
        );
    }

    #[test]
    fn module_type_symbols_includes_declared_types_but_not_referenced_types() {
        let db = setup_db();
        let symbol_names = module_type_symbols(&db);

        let dunder_name_symbol_name = ast::name::Name::new_static("__name__");
        assert!(symbol_names.contains(&dunder_name_symbol_name));

        let property_symbol_name = ast::name::Name::new_static("property");
        assert!(!symbol_names.contains(&property_symbol_name));
    }

    /// Inferring the result of a call-expression shouldn't need to re-run after
    /// a trivial change to the function's file (e.g. by adding a docstring to the function).
    #[test]
    fn call_type_doesnt_rerun_when_only_callee_changed() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/foo.py",
            r#"
            def foo() -> int:
                return 5
        "#,
        )?;
        db.write_dedented(
            "src/bar.py",
            r#"
            from foo import foo

            a = foo()
            "#,
        )?;

        let bar = system_path_to_file(&db, "src/bar.py")?;
        let a = global_symbol(&db, bar, "a");

        assert_eq!(a.expect_type(), KnownClass::Int.to_instance(&db));

        // Add a docstring to foo to trigger a re-run.
        // The bar-call site of foo should not be re-run because of that
        db.write_dedented(
            "src/foo.py",
            r#"
            def foo() -> int:
                "Computes a value"
                return 5
            "#,
        )?;
        db.clear_salsa_events();

        let a = global_symbol(&db, bar, "a");

        assert_eq!(a.expect_type(), KnownClass::Int.to_instance(&db));
        let events = db.take_salsa_events();

        let call = &*parsed_module(&db, bar).syntax().body[1]
            .as_assign_stmt()
            .unwrap()
            .value;
        let foo_call = semantic_index(&db, bar).expression(call);

        assert_function_query_was_not_run(&db, infer_expression_types, foo_call, &events);

        Ok(())
    }

    #[test]
    fn type_alias_types() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/mod.py",
            r#"
            type Alias1 = int
            type Alias2 = int
        "#,
        )?;

        let mod_py = system_path_to_file(&db, "src/mod.py")?;
        let ty_alias1 = global_symbol(&db, mod_py, "Alias1").expect_type();
        let ty_alias2 = global_symbol(&db, mod_py, "Alias2").expect_type();

        let Type::KnownInstance(KnownInstanceType::TypeAliasType(alias1)) = ty_alias1 else {
            panic!("Expected TypeAliasType, got {ty_alias1:?}");
        };
        assert_eq!(alias1.name(&db), "Alias1");
        assert_eq!(alias1.value_ty(&db), KnownClass::Int.to_instance(&db));

        // Two type aliases are distinct and disjoint, even if they refer to the same type
        assert!(!ty_alias1.is_equivalent_to(&db, ty_alias2));
        assert!(ty_alias1.is_disjoint_from(&db, ty_alias2));

        Ok(())
    }

    /// All other tests also make sure that `Type::Todo` works as expected. This particular
    /// test makes sure that we handle `Todo` types correctly, even if they originate from
    /// different sources.
    #[test]
    fn todo_types() {
        let db = setup_db();

        let todo1 = todo_type!("1");
        let todo2 = todo_type!("2");
        let todo3 = todo_type!();
        let todo4 = todo_type!();

        assert!(todo1.is_equivalent_to(&db, todo2));
        assert!(todo3.is_equivalent_to(&db, todo4));
        assert!(todo1.is_equivalent_to(&db, todo3));

        assert!(todo1.is_subtype_of(&db, todo2));
        assert!(todo2.is_subtype_of(&db, todo1));

        assert!(todo3.is_subtype_of(&db, todo4));
        assert!(todo4.is_subtype_of(&db, todo3));

        assert!(todo1.is_subtype_of(&db, todo3));
        assert!(todo3.is_subtype_of(&db, todo1));

        let int = KnownClass::Int.to_instance(&db);

        assert!(int.is_assignable_to(&db, todo1));
        assert!(int.is_assignable_to(&db, todo3));

        assert!(todo1.is_assignable_to(&db, int));
        assert!(todo3.is_assignable_to(&db, int));

        // We lose information when combining several `Todo` types. This is an
        // acknowledged limitation of the current implementation. We can not
        // easily store the meta information of several `Todo`s in a single
        // variant, as `TodoType` needs to implement `Copy`, meaning it can't
        // contain `Vec`/`Box`/etc., and can't be boxed itself.
        //
        // Lifting this restriction would require us to intern `TodoType` in
        // salsa, but that would mean we would have to pass in `db` everywhere.

        // A union of several `Todo` types collapses to a single `Todo` type:
        assert!(UnionType::from_elements(&db, vec![todo1, todo2, todo3, todo4]).is_todo());

        // And similar for intersection types:
        assert!(IntersectionBuilder::new(&db)
            .add_positive(todo1)
            .add_positive(todo2)
            .add_positive(todo3)
            .add_positive(todo4)
            .build()
            .is_todo());
        assert!(IntersectionBuilder::new(&db)
            .add_positive(todo1)
            .add_negative(todo2)
            .add_positive(todo3)
            .add_negative(todo4)
            .build()
            .is_todo());
    }
}
