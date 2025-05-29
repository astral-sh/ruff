use std::str::FromStr;

use bitflags::bitflags;
use ruff_db::diagnostic::Span;
use ruff_db::files::{File, FileRange};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::module_resolver::{KnownModule, file_to_module};
use crate::semantic_index::ast_ids::HasScopedUseId;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::semantic_index;
use crate::semantic_index::symbol::ScopeId;
use crate::symbol::{Boundness, Symbol, symbol_from_bindings};
use crate::types::generics::GenericContext;
use crate::types::narrow::ClassInfoConstraintFunction;
use crate::types::signatures::{CallableSignature, Signature};
use crate::types::{BoundMethodType, CallableType, Type, TypeMapping, TypeVarInstance};
use crate::{Db, FxOrderSet};

/// A collection of useful spans for annotating functions.
///
/// This can be retrieved via `FunctionType::spans` or
/// `Type::function_spans`.
pub(crate) struct FunctionSpans {
    /// The span of the entire function "signature." This includes
    /// the name, parameter list and return type (if present).
    pub(crate) signature: Span,
    /// The span of the function name. i.e., `foo` in `def foo(): ...`.
    pub(crate) name: Span,
    /// The span of the parameter list, including the opening and
    /// closing parentheses.
    #[expect(dead_code)]
    pub(crate) parameters: Span,
    /// The span of the annotated return type, if present.
    pub(crate) return_type: Option<Span>,
}

bitflags! {
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Hash)]
    pub struct FunctionDecorators: u8 {
        /// `@classmethod`
        const CLASSMETHOD = 1 << 0;
        /// `@typing.no_type_check`
        const NO_TYPE_CHECK = 1 << 1;
        /// `@typing.overload`
        const OVERLOAD = 1 << 2;
        /// `@abc.abstractmethod`
        const ABSTRACT_METHOD = 1 << 3;
        /// `@typing.final`
        const FINAL = 1 << 4;
        /// `@typing.override`
        const OVERRIDE = 1 << 6;
    }
}

bitflags! {
    /// Used for the return type of `dataclass_transform(…)` calls. Keeps track of the
    /// arguments that were passed in. For the precise meaning of the fields, see [1].
    ///
    /// [1]: https://docs.python.org/3/library/typing.html#typing.dataclass_transform
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
    pub struct DataclassTransformerParams: u8 {
        const EQ_DEFAULT = 0b0000_0001;
        const ORDER_DEFAULT = 0b0000_0010;
        const KW_ONLY_DEFAULT = 0b0000_0100;
        const FROZEN_DEFAULT = 0b0000_1000;
    }
}

impl Default for DataclassTransformerParams {
    fn default() -> Self {
        Self::EQ_DEFAULT
    }
}

/// Representation of a function definition in the AST: either a non-generic function, or a generic
/// function that has not been specialized.
///
/// If a function has multiple overloads, each overload is represented by a separate function
/// definition in the AST, and is therefore a separate `OverloadLiteral` instance.
///
/// # Ordering
/// Ordering is based on the function's id assigned by salsa and not on the function literal's
/// values. The id may change between runs, or when the function literal was garbage collected and
/// recreated.
#[salsa::interned(debug)]
#[derive(PartialOrd, Ord)]
pub struct OverloadLiteral<'db> {
    /// Name of the function at definition.
    #[returns(ref)]
    pub name: ast::name::Name,

    /// Is this a function that we special-case somehow? If so, which one?
    pub(crate) known: Option<KnownFunction>,

    /// The scope that's created by the function, in which the function body is evaluated.
    pub(crate) body_scope: ScopeId<'db>,

    /// A set of special decorators that were applied to this function
    pub(crate) decorators: FunctionDecorators,

    /// The arguments to `dataclass_transformer`, if this function was annotated
    /// with `@dataclass_transformer(...)`.
    pub(crate) dataclass_transformer_params: Option<DataclassTransformerParams>,
}

#[salsa::tracked]
impl<'db> OverloadLiteral<'db> {
    fn with_dataclass_transformer_params(
        self,
        db: &'db dyn Db,
        params: DataclassTransformerParams,
    ) -> Self {
        Self::new(
            db,
            self.name(db).clone(),
            self.known(db),
            self.body_scope(db),
            self.decorators(db),
            Some(params),
        )
    }

    fn file(self, db: &'db dyn Db) -> File {
        // NOTE: Do not use `self.definition(db).file(db)` here, as that could create a
        // cross-module dependency on the full AST.
        self.body_scope(db).file(db)
    }

    pub(crate) fn has_known_decorator(self, db: &dyn Db, decorator: FunctionDecorators) -> bool {
        self.decorators(db).contains(decorator)
    }

    pub(crate) fn is_overload(self, db: &dyn Db) -> bool {
        self.has_known_decorator(db, FunctionDecorators::OVERLOAD)
    }

    fn node(self, db: &'db dyn Db, file: File) -> &'db ast::StmtFunctionDef {
        debug_assert_eq!(
            file,
            self.file(db),
            "OverloadLiteral::node() must be called with the same file as the one where \
            the function is defined."
        );

        self.body_scope(db).node(db).expect_function()
    }

    /// Returns the [`FileRange`] of the function's name.
    pub(crate) fn focus_range(self, db: &dyn Db) -> FileRange {
        FileRange::new(
            self.file(db),
            self.body_scope(db).node(db).expect_function().name.range,
        )
    }

    /// Returns the [`Definition`] of this function.
    ///
    /// ## Warning
    ///
    /// This uses the semantic index to find the definition of the function. This means that if the
    /// calling query is not in the same file as this function is defined in, then this will create
    /// a cross-module dependency directly on the full AST which will lead to cache
    /// over-invalidation.
    fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let body_scope = self.body_scope(db);
        let index = semantic_index(db, body_scope.file(db));
        index.expect_single_definition(body_scope.node(db).expect_function())
    }

    /// Returns the overload immediately before this one in the AST. Returns `None` if there is no
    /// previous overload.
    fn previous_overload(self, db: &'db dyn Db) -> Option<FunctionLiteral<'db>> {
        // The semantic model records a use for each function on the name node. This is used
        // here to get the previous function definition with the same name.
        let scope = self.definition(db).scope(db);
        let use_def = semantic_index(db, scope.file(db)).use_def_map(scope.file_scope_id(db));
        let use_id = self
            .body_scope(db)
            .node(db)
            .expect_function()
            .name
            .scoped_use_id(db, scope);

        let Symbol::Type(Type::FunctionLiteral(previous_type), Boundness::Bound) =
            symbol_from_bindings(db, use_def.bindings_at_use(use_id))
        else {
            return None;
        };

        let previous_literal = previous_type.literal(db);
        let previous_overload = previous_literal.current_overload(db);
        if !previous_overload.is_overload(db) {
            return None;
        }

        Some(previous_literal)
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
    pub(crate) fn signature(
        self,
        db: &'db dyn Db,
        inherited_generic_context: Option<GenericContext<'db>>,
    ) -> Signature<'db> {
        let scope = self.body_scope(db);
        let function_stmt_node = scope.node(db).expect_function();
        let definition = self.definition(db);
        let generic_context = function_stmt_node.type_params.as_ref().map(|type_params| {
            let index = semantic_index(db, scope.file(db));
            GenericContext::from_type_params(db, index, type_params)
        });
        Signature::from_function(
            db,
            generic_context,
            inherited_generic_context,
            definition,
            function_stmt_node,
        )
    }

    fn parameter_span(
        self,
        db: &'db dyn Db,
        parameter_index: Option<usize>,
    ) -> Option<(Span, Span)> {
        let function_scope = self.body_scope(db);
        let span = Span::from(function_scope.file(db));
        let node = function_scope.node(db);
        let func_def = node.as_function()?;
        let range = parameter_index
            .and_then(|parameter_index| {
                func_def
                    .parameters
                    .iter()
                    .nth(parameter_index)
                    .map(|param| param.range())
            })
            .unwrap_or(func_def.parameters.range);
        let name_span = span.clone().with_range(func_def.name.range);
        let parameter_span = span.with_range(range);
        Some((name_span, parameter_span))
    }

    pub(crate) fn spans(self, db: &'db dyn Db) -> Option<FunctionSpans> {
        let function_scope = self.body_scope(db);
        let span = Span::from(function_scope.file(db));
        let node = function_scope.node(db);
        let func_def = node.as_function()?;
        let return_type_range = func_def.returns.as_ref().map(|returns| returns.range());
        let mut signature = func_def.name.range.cover(func_def.parameters.range);
        if let Some(return_type_range) = return_type_range {
            signature = signature.cover(return_type_range);
        }
        Some(FunctionSpans {
            signature: span.clone().with_range(signature),
            name: span.clone().with_range(func_def.name.range),
            parameters: span.clone().with_range(func_def.parameters.range),
            return_type: return_type_range.map(|range| span.clone().with_range(range)),
        })
    }
}

/// Representation of a function definition in the AST, along with any previous overloads of the
/// function. Each overload can be separately generic or not, and each generic overload uses
/// distinct typevars.
///
/// # Ordering
/// Ordering is based on the function's id assigned by salsa and not on the function literal's
/// values. The id may change between runs, or when the function literal was garbage collected and
/// recreated.
#[salsa::interned(debug)]
#[derive(PartialOrd, Ord)]
pub struct FunctionLiteral<'db> {
    pub(crate) current_overload: OverloadLiteral<'db>,

    /// The inherited generic context, if this function is a class method being used to infer the
    /// specialization of its generic class. If any of the method's overloads are themselves
    /// generic, this is in addition to those per-overload generic contexts.
    inherited_generic_context: Option<GenericContext<'db>>,
}

#[salsa::tracked]
impl<'db> FunctionLiteral<'db> {
    fn with_inherited_generic_context(
        self,
        db: &'db dyn Db,
        inherited_generic_context: GenericContext<'db>,
    ) -> Self {
        // A function cannot inherit more than one generic context from its containing class.
        debug_assert!(self.inherited_generic_context(db).is_none());
        Self::new(
            db,
            self.current_overload(db),
            Some(inherited_generic_context),
        )
    }

    fn update_current_overload(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(OverloadLiteral<'db>) -> OverloadLiteral<'db>,
    ) -> Self {
        Self::new(
            db,
            f(self.current_overload(db)),
            self.inherited_generic_context(db),
        )
    }

    fn name(self, db: &'db dyn Db) -> &'db ast::name::Name {
        // All of the overloads of a function literal should have the same name.
        self.current_overload(db).name(db)
    }

    fn known(self, db: &'db dyn Db) -> Option<KnownFunction> {
        // Whether a function is known is based on its name (and its containing module's name), so
        // all overloads should be known (or not) equivalently.
        self.current_overload(db).known(db)
    }

    fn has_known_decorator(self, db: &dyn Db, decorator: FunctionDecorators) -> bool {
        self.iter_overloads_and_implementation(db)
            .any(|overload| overload.decorators(db).contains(decorator))
    }

    fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        self.current_overload(db).definition(db)
    }

    fn parameter_span(
        self,
        db: &'db dyn Db,
        parameter_index: Option<usize>,
    ) -> Option<(Span, Span)> {
        self.current_overload(db)
            .parameter_span(db, parameter_index)
    }

    fn spans(self, db: &'db dyn Db) -> Option<FunctionSpans> {
        self.current_overload(db).spans(db)
    }

    #[salsa::tracked(returns(ref))]
    fn overloads_and_implementation(
        self,
        db: &'db dyn Db,
    ) -> (Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>) {
        let self_overload = self.current_overload(db);
        let mut current = self_overload;
        let mut overloads = vec![];

        while let Some(previous) = current.previous_overload(db) {
            let overload = previous.current_overload(db);
            overloads.push(overload);
            current = overload;
        }

        // Overloads are inserted in reverse order, from bottom to top.
        overloads.reverse();

        let implementation = if self_overload.is_overload(db) {
            overloads.push(self_overload);
            None
        } else {
            Some(self_overload)
        };

        (overloads.into_boxed_slice(), implementation)
    }

    fn iter_overloads_and_implementation(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = OverloadLiteral<'db>> + 'db {
        let (implementation, overloads) = self.overloads_and_implementation(db);
        overloads.iter().chain(implementation).copied()
    }

    /// Typed externally-visible signature for this function.
    ///
    /// This is the signature as seen by external callers, possibly modified by decorators and/or
    /// overloaded.
    fn signature<'a>(
        self,
        db: &'db dyn Db,
        type_mappings: &'a [TypeMapping<'a, 'db>],
    ) -> CallableSignature<'db>
    where
        'db: 'a,
    {
        // We only include an implementation (i.e. a definition not decorated with `@overload`) if
        // it's the only definition.
        let inherited_generic_context = self.inherited_generic_context(db);
        let (overloads, implementation) = self.overloads_and_implementation(db);
        if let Some(implementation) = implementation {
            if overloads.is_empty() {
                return CallableSignature::single(type_mappings.iter().fold(
                    implementation.signature(db, inherited_generic_context),
                    |ty, mapping| ty.apply_type_mapping(db, mapping),
                ));
            }
        }

        CallableSignature::from_overloads(overloads.iter().map(|overload| {
            type_mappings.iter().fold(
                overload.signature(db, inherited_generic_context),
                |ty, mapping| ty.apply_type_mapping(db, mapping),
            )
        }))
    }

    fn normalized(self, db: &'db dyn Db) -> Self {
        let context = self
            .inherited_generic_context(db)
            .map(|ctx| ctx.normalized(db));
        Self::new(db, self.current_overload(db), context)
    }
}

/// Represents a function type, which might be a non-generic function, or a specialization of a
/// generic function.
#[salsa::interned(debug)]
#[derive(PartialOrd, Ord)]
pub struct FunctionType<'db> {
    pub(crate) literal: FunctionLiteral<'db>,

    /// Type mappings that should be applied to the function's parameter and return types. This
    /// might include specializations of enclosing generic contexts (e.g. for non-generic methods
    /// of a specialized generic class).
    #[returns(ref)]
    type_mappings: Box<[TypeMapping<'db, 'db>]>,
}

#[salsa::tracked]
impl<'db> FunctionType<'db> {
    pub(crate) fn with_inherited_generic_context(
        self,
        db: &'db dyn Db,
        inherited_generic_context: GenericContext<'db>,
    ) -> Self {
        let literal = self
            .literal(db)
            .with_inherited_generic_context(db, inherited_generic_context);
        Self::new(db, literal, self.type_mappings(db))
    }

    pub(crate) fn with_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
        let type_mappings: Box<[_]> = self
            .type_mappings(db)
            .iter()
            .cloned()
            .chain(std::iter::once(type_mapping.to_owned()))
            .collect();
        Self::new(db, self.literal(db), type_mappings)
    }

    pub(crate) fn with_dataclass_transformer_params(
        self,
        db: &'db dyn Db,
        params: DataclassTransformerParams,
    ) -> Self {
        // A decorator only applies to the specific overload that it is attached, not to all
        // previous overloads.
        Self::new(
            db,
            self.literal(db).update_current_overload(db, |overload| {
                overload.with_dataclass_transformer_params(db, params)
            }),
            self.type_mappings(db),
        )
    }

    /// Returns the [`File`] in which this function is defined.
    pub(crate) fn file(self, db: &'db dyn Db) -> File {
        self.literal(db).current_overload(db).file(db)
    }

    /// Returns the AST node for this function.
    pub(crate) fn node(self, db: &'db dyn Db, file: File) -> &'db ast::StmtFunctionDef {
        self.literal(db).current_overload(db).node(db, file)
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db ast::name::Name {
        self.literal(db).name(db)
    }

    pub(crate) fn known(self, db: &'db dyn Db) -> Option<KnownFunction> {
        self.literal(db).known(db)
    }

    pub(crate) fn is_known(self, db: &'db dyn Db, known_function: KnownFunction) -> bool {
        self.known(db) == Some(known_function)
    }

    /// Returns if any of the overloads of this function have a particular decorator.
    ///
    /// Some decorators are expected to appear on every overload; others are expected to appear
    /// only the implementation or first overload. This method does not check either of those
    /// conditions.
    pub(crate) fn has_known_decorator(self, db: &dyn Db, decorator: FunctionDecorators) -> bool {
        self.literal(db).has_known_decorator(db, decorator)
    }

    /// Returns the [`Definition`] of the implementation or first overload of this function.
    ///
    /// ## Warning
    ///
    /// This uses the semantic index to find the definition of the function. This means that if the
    /// calling query is not in the same file as this function is defined in, then this will create
    /// a cross-module dependency directly on the full AST which will lead to cache
    /// over-invalidation.
    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        self.literal(db).definition(db)
    }

    /// Returns a tuple of two spans. The first is
    /// the span for the identifier of the function
    /// definition for `self`. The second is
    /// the span for the parameter in the function
    /// definition for `self`.
    ///
    /// If there are no meaningful spans, then this
    /// returns `None`. For example, when this type
    /// isn't callable.
    ///
    /// When `parameter_index` is `None`, then the
    /// second span returned covers the entire parameter
    /// list.
    ///
    /// # Performance
    ///
    /// Note that this may introduce cross-module
    /// dependencies. This can have an impact on
    /// the effectiveness of incremental caching
    /// and should therefore be used judiciously.
    ///
    /// An example of a good use case is to improve
    /// a diagnostic.
    pub(crate) fn parameter_span(
        self,
        db: &'db dyn Db,
        parameter_index: Option<usize>,
    ) -> Option<(Span, Span)> {
        self.literal(db).parameter_span(db, parameter_index)
    }

    /// Returns a collection of useful spans for a
    /// function signature. These are useful for
    /// creating annotations on diagnostics.
    ///
    /// # Performance
    ///
    /// Note that this may introduce cross-module
    /// dependencies. This can have an impact on
    /// the effectiveness of incremental caching
    /// and should therefore be used judiciously.
    ///
    /// An example of a good use case is to improve
    /// a diagnostic.
    pub(crate) fn spans(self, db: &'db dyn Db) -> Option<FunctionSpans> {
        self.literal(db).spans(db)
    }

    /// Returns all of the overload signatures and the implementation definition, if any, of this
    /// function. The overload signatures will be in source order.
    pub(crate) fn overloads_and_implementation(
        self,
        db: &'db dyn Db,
    ) -> &'db (Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>) {
        self.literal(db).overloads_and_implementation(db)
    }

    /// Returns an iterator of all of the definitions of this function, including both overload
    /// signatures and any implementation, all in source order.
    pub(crate) fn iter_overloads_and_implementation(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = OverloadLiteral<'db>> + 'db {
        self.literal(db).iter_overloads_and_implementation(db)
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
    #[salsa::tracked(returns(ref), cycle_fn=signature_cycle_recover, cycle_initial=signature_cycle_initial)]
    pub(crate) fn signature(self, db: &'db dyn Db) -> CallableSignature<'db> {
        self.literal(db).signature(db, self.type_mappings(db))
    }

    /// Convert the `FunctionType` into a [`Type::Callable`].
    pub(crate) fn into_callable_type(self, db: &'db dyn Db) -> Type<'db> {
        Type::Callable(CallableType::new(db, self.signature(db), false))
    }

    /// Convert the `FunctionType` into a [`Type::BoundMethod`].
    pub(crate) fn into_bound_method_type(
        self,
        db: &'db dyn Db,
        self_instance: Type<'db>,
    ) -> Type<'db> {
        Type::BoundMethod(BoundMethodType::new(db, self, self_instance))
    }

    pub(crate) fn is_subtype_of(self, db: &'db dyn Db, other: Self) -> bool {
        // A function type is the subtype of itself, and not of any other function type. However,
        // our representation of a function type includes any specialization that should be applied
        // to the signature. Different specializations of the same function type are only subtypes
        // of each other if they result in subtype signatures.
        self.normalized(db) == other.normalized(db)
            || (self.literal(db) == other.literal(db)
                && self.signature(db).is_subtype_of(db, other.signature(db)))
    }

    pub(crate) fn is_assignable_to(self, db: &'db dyn Db, other: Self) -> bool {
        // A function type is assignable to itself, and not to any other function type. However,
        // our representation of a function type includes any specialization that should be applied
        // to the signature. Different specializations of the same function type are only
        // assignable to each other if they result in assignable signatures.
        self.literal(db) == other.literal(db)
            && self.signature(db).is_assignable_to(db, other.signature(db))
    }

    pub(crate) fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.normalized(db) == other.normalized(db)
            || (self.literal(db) == other.literal(db)
                && self.signature(db).is_equivalent_to(db, other.signature(db)))
    }

    pub(crate) fn is_gradual_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.literal(db) == other.literal(db)
            && self
                .signature(db)
                .is_gradual_equivalent_to(db, other.signature(db))
    }

    pub(crate) fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        let signatures = self.signature(db);
        for signature in &signatures.overloads {
            signature.find_legacy_typevars(db, typevars);
        }
    }

    pub(crate) fn normalized(self, db: &'db dyn Db) -> Self {
        let mappings: Box<_> = self
            .type_mappings(db)
            .iter()
            .map(|mapping| mapping.normalized(db))
            .collect();
        Self::new(db, self.literal(db).normalized(db), mappings)
    }
}

fn signature_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &CallableSignature<'db>,
    _count: u32,
    _function: FunctionType<'db>,
) -> salsa::CycleRecoveryAction<CallableSignature<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn signature_cycle_initial<'db>(
    db: &'db dyn Db,
    _function: FunctionType<'db>,
) -> CallableSignature<'db> {
    CallableSignature::single(Signature::bottom(db))
}

/// Non-exhaustive enumeration of known functions (e.g. `builtins.reveal_type`, ...) that might
/// have special behavior.
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, strum_macros::EnumString, strum_macros::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
#[cfg_attr(test, derive(strum_macros::EnumIter))]
pub enum KnownFunction {
    /// `builtins.isinstance`
    #[strum(serialize = "isinstance")]
    IsInstance,
    /// `builtins.issubclass`
    #[strum(serialize = "issubclass")]
    IsSubclass,
    /// `builtins.hasattr`
    #[strum(serialize = "hasattr")]
    HasAttr,
    /// `builtins.reveal_type`, `typing.reveal_type` or `typing_extensions.reveal_type`
    RevealType,
    /// `builtins.len`
    Len,
    /// `builtins.repr`
    Repr,
    /// `typing(_extensions).final`
    Final,

    /// [`typing(_extensions).no_type_check`](https://typing.python.org/en/latest/spec/directives.html#no-type-check)
    NoTypeCheck,

    /// `typing(_extensions).assert_type`
    AssertType,
    /// `typing(_extensions).assert_never`
    AssertNever,
    /// `typing(_extensions).cast`
    Cast,
    /// `typing(_extensions).overload`
    Overload,
    /// `typing(_extensions).override`
    Override,
    /// `typing(_extensions).is_protocol`
    IsProtocol,
    /// `typing(_extensions).get_protocol_members`
    GetProtocolMembers,
    /// `typing(_extensions).runtime_checkable`
    RuntimeCheckable,
    /// `typing(_extensions).dataclass_transform`
    DataclassTransform,

    /// `abc.abstractmethod`
    #[strum(serialize = "abstractmethod")]
    AbstractMethod,

    /// `dataclasses.dataclass`
    Dataclass,

    /// `inspect.getattr_static`
    GetattrStatic,

    /// `ty_extensions.static_assert`
    StaticAssert,
    /// `ty_extensions.is_equivalent_to`
    IsEquivalentTo,
    /// `ty_extensions.is_subtype_of`
    IsSubtypeOf,
    /// `ty_extensions.is_assignable_to`
    IsAssignableTo,
    /// `ty_extensions.is_disjoint_from`
    IsDisjointFrom,
    /// `ty_extensions.is_gradual_equivalent_to`
    IsGradualEquivalentTo,
    /// `ty_extensions.is_fully_static`
    IsFullyStatic,
    /// `ty_extensions.is_singleton`
    IsSingleton,
    /// `ty_extensions.is_single_valued`
    IsSingleValued,
    /// `ty_extensions.generic_context`
    GenericContext,
    /// `ty_extensions.dunder_all_names`
    DunderAllNames,
}

impl KnownFunction {
    pub fn into_classinfo_constraint_function(self) -> Option<ClassInfoConstraintFunction> {
        match self {
            Self::IsInstance => Some(ClassInfoConstraintFunction::IsInstance),
            Self::IsSubclass => Some(ClassInfoConstraintFunction::IsSubclass),
            _ => None,
        }
    }

    pub(crate) fn try_from_definition_and_name<'db>(
        db: &'db dyn Db,
        definition: Definition<'db>,
        name: &str,
    ) -> Option<Self> {
        let candidate = Self::from_str(name).ok()?;
        candidate
            .check_module(file_to_module(db, definition.file(db))?.known()?)
            .then_some(candidate)
    }

    /// Return `true` if `self` is defined in `module` at runtime.
    const fn check_module(self, module: KnownModule) -> bool {
        match self {
            Self::IsInstance | Self::IsSubclass | Self::HasAttr | Self::Len | Self::Repr => {
                module.is_builtins()
            }
            Self::AssertType
            | Self::AssertNever
            | Self::Cast
            | Self::Overload
            | Self::Override
            | Self::RevealType
            | Self::Final
            | Self::IsProtocol
            | Self::GetProtocolMembers
            | Self::RuntimeCheckable
            | Self::DataclassTransform
            | Self::NoTypeCheck => {
                matches!(module, KnownModule::Typing | KnownModule::TypingExtensions)
            }
            Self::AbstractMethod => {
                matches!(module, KnownModule::Abc)
            }
            Self::Dataclass => {
                matches!(module, KnownModule::Dataclasses)
            }
            Self::GetattrStatic => module.is_inspect(),
            Self::IsAssignableTo
            | Self::IsDisjointFrom
            | Self::IsEquivalentTo
            | Self::IsGradualEquivalentTo
            | Self::IsFullyStatic
            | Self::IsSingleValued
            | Self::IsSingleton
            | Self::IsSubtypeOf
            | Self::GenericContext
            | Self::DunderAllNames
            | Self::StaticAssert => module.is_ty_extensions(),
        }
    }
}
