//! Contains representations of function literals. There are several complicating factors:
//!
//! - Functions can be generic, and can have specializations applied to them. These are not the
//!   same thing! For instance, a method of a generic class might not itself be generic, but it can
//!   still have the class's specialization applied to it.
//!
//! - Functions can be overloaded, and each overload can be independently generic or not, with
//!   different sets of typevars for different generic overloads. In some cases we need to consider
//!   each overload separately; in others we need to consider all of the overloads (and any
//!   implementation) as a single collective entity.
//!
//! - Certain “known” functions need special treatment — for instance, inferring a special return
//!   type, or raising custom diagnostics.
//!
//! - TODO: Some functions don't correspond to a function definition in the AST, and are instead
//!   synthesized as we mimic the behavior of the Python interpreter. Even though they are
//!   synthesized, and are “implemented” as Rust code, they are still functions from the POV of the
//!   rest of the type system.
//!
//! Given these constraints, we have the following representation: a function is a list of one or
//! more overloads, with zero or more specializations (more specifically, “type mappings”) applied
//! to it. [`FunctionType`] is the outermost type, which is what [`Type::FunctionLiteral`] wraps.
//! It contains the list of type mappings to apply. It wraps a [`FunctionLiteral`], which collects
//! together all of the overloads (and implementation) of an overloaded function. An
//! [`OverloadLiteral`] represents an individual function definition in the AST — that is, each
//! overload (and implementation) of an overloaded function, or the single definition of a
//! non-overloaded function.
//!
//! Technically, each `FunctionLiteral` wraps a particular overload and all _previous_ overloads.
//! So it's only true that it wraps _all_ overloads if you are looking at the last definition. For
//! instance, in
//!
//! ```py
//! @overload
//! def f(x: int) -> None: ...
//! # <-- 1
//!
//! @overload
//! def f(x: str) -> None: ...
//! # <-- 2
//!
//! def f(x): pass
//! # <-- 3
//! ```
//!
//! resolving `f` at each of the three numbered positions will give you a `FunctionType`, which
//! wraps a `FunctionLiteral`, which contain `OverloadLiteral`s only for the definitions that
//! appear before that position. We rely on the fact that later definitions shadow earlier ones, so
//! the public type of `f` is resolved at position 3, correctly giving you all of the overloads
//! (and the implementation).

use std::future::{Future, ready};
use std::{borrow::Cow, convert::Infallible, str::FromStr};

use bitflags::bitflags;
use itertools::Either;
use ruff_db::PythonFile;
use ruff_db::diagnostic::{Annotation, DiagnosticId, Severity, Span};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_db::source::source_text;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast as ast;
use ruff_python_ast::find_node::covering_node;
use ruff_python_edits::unwrapped_call_argument;
use ruff_text_size::{Ranged, TextRange};
use salsa::execution_probe::FieldRequest;
use salsa::plumbing::AsId;
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::function::IngredientImpl;
use ty_module_resolver::{ImportingFile, KnownModule, ModuleName, file_to_module, resolve_module};

use crate::place::{DefinedPlace, Definedness, Place, place_from_bindings};
use crate::types::call::{Binding, CallArguments};
use crate::types::callable::CallableTypeKind;
use crate::types::callable::conversion::FunctionConversionEffects;
use crate::types::constraints::ConstraintSet;
use crate::types::context::InferContext;
use crate::types::cyclic::ActiveRecursionDetector;
use crate::types::diagnostic::{
    ASSERT_TYPE_UNSPELLABLE_SUBTYPE, DISJOINT_CAST, INVALID_ARGUMENT_TYPE, REDUNDANT_CAST,
    STATIC_ASSERT_ERROR, TYPE_ASSERTION_FAILURE, report_bad_argument_to_get_protocol_members,
    report_bad_argument_to_protocol_interface, report_invalid_total_ordering_call,
    report_issubclass_check_against_protocol_with_non_method_members,
    report_runtime_check_against_non_runtime_checkable_protocol,
    report_runtime_check_against_typed_dict,
};
use crate::types::display::DisplaySettings;
use crate::types::generics::GenericContext;
use crate::types::infer::infer_definition_types;
use crate::types::known_instance::DeprecatedInstance;
use crate::types::list_members::all_members;
use crate::types::narrow::ClassInfoConstraintFunction;
use crate::types::relation::TypeRelationChecker;
use crate::types::signatures::effects::legacy_inline;
use crate::types::signatures::source::InlineSignatureSourceEffects;
use crate::types::signatures::{CallableSignature, ReturnCallableTypeVarScope, Signature};
use crate::types::tuple::TupleSpec;
use crate::types::variance::{VarianceInferable, VarianceOrigin, VarianceTerm};
use crate::types::visitor::non_any_dynamic_content;
use crate::types::{
    ApplyTypeMappingVisitor, BoundMethodType, BoundTypeVarIdentity, BoundTypeVarInstance,
    CallableType, ClassBase, ClassLiteral, ClassType, FindLegacyTypeVarsVisitor,
    IntersectionBuilder, KnownClass, KnownInstanceType, SpecialFormType, Truthiness, Type,
    TypeContext, TypeMapping, TypeVarBoundOrConstraints, UnionBuilder, UnionType, binding_type,
    definition_expression_type, walk_signature,
};
use crate::{Db, FxIndexMap, FxOrderSet, ProgramEnvironment};
use ty_python_core::ast_ids::HasScopedUseId;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::ScopeId;
use ty_python_core::{ProgramFile, SemanticIndex, semantic_index};

pub(in crate::types) mod descriptor;
pub(in crate::types) mod inherited_context;
pub(in crate::types) mod last_signature;
pub(in crate::types) mod mapping;
pub(in crate::types) mod overloads;
pub(in crate::types) mod source;

#[cfg(feature = "experimental-analysis")]
mod runtime;
#[cfg(feature = "experimental-analysis")]
pub(in crate::types) use runtime::{
    FunctionMemoSchema, OverloadMemoSchema, register_dataclass_transformer_values,
    register_function_values,
};

pub(in crate::types) mod identity_sealed {
    pub(in crate::types) trait Sealed {}
}

/// Source prerequisites for constructing a function identity without evaluating its signature.
pub(in crate::types) trait FunctionIdentityEffects<'db>:
    identity_sealed::Sealed
{
    type Error;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error>;

    async fn names_equal(
        &self,
        left: &ast::name::Name,
        right: &ast::name::Name,
    ) -> Result<bool, Self::Error>;

    async fn definition(
        &self,
        db: &'db dyn Db,
        function: OverloadLiteral<'db>,
    ) -> Result<Definition<'db>, Self::Error>;

    /// Reads the recorded name use immediately preceding this function definition.
    async fn preceding_bindings(
        &self,
        db: &'db dyn Db,
        function: OverloadLiteral<'db>,
        definition: Definition<'db>,
    ) -> Result<Place<'db>, Self::Error>;

    async fn callable_definition(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<Option<FunctionLiteral<'db>>, Self::Error>;
}

pub(in crate::types) struct LegacyFunctionIdentityEffects;

/// Metadata that requires walking the preceding overload definitions.
pub(in crate::types) trait FunctionMetadataEffects<'db>:
    identity_sealed::Sealed
{
    type Error;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error>;

    async fn overloads_and_implementation(
        &self,
        db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
    ) -> Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), Self::Error>;
}

impl identity_sealed::Sealed for LegacyFunctionIdentityEffects {}

impl<'db> FunctionMetadataEffects<'db> for LegacyFunctionIdentityEffects {
    type Error = Infallible;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    fn overloads_and_implementation(
        &self,
        db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
    ) -> impl Future<
        Output = Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), Self::Error>,
    > {
        ready(Ok(FunctionLiteral::overloaded_definitions(
            db,
            last_definition,
        )))
    }
}

impl<'db> FunctionIdentityEffects<'db> for LegacyFunctionIdentityEffects {
    type Error = Infallible;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    async fn names_equal(
        &self,
        left: &ast::name::Name,
        right: &ast::name::Name,
    ) -> Result<bool, Self::Error> {
        Ok(left == right)
    }

    fn definition(
        &self,
        db: &'db dyn Db,
        function: OverloadLiteral<'db>,
    ) -> impl Future<Output = Result<Definition<'db>, Self::Error>> {
        ready(Ok(function.definition(db)))
    }

    fn preceding_bindings(
        &self,
        db: &'db dyn Db,
        function: OverloadLiteral<'db>,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<Place<'db>, Self::Error>> {
        ready({
            let scope = definition.scope(db);
            let module = parsed_module(db, function.python_file(db)).load(db);
            let use_def =
                semantic_index(db, scope.program_file(db)).use_def_map(scope.file_scope_id(db));
            let use_id = function
                .body_scope(db)
                .node(db)
                .expect_function()
                .node(&module)
                .name
                .scoped_use_id(db, function.program_file(db));
            let env = ProgramEnvironment::from_scope(scope);
            Ok(place_from_bindings(db, &env, use_def.bindings_at_use(use_id)).place)
        })
    }

    fn callable_definition(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<Option<FunctionLiteral<'db>>, Self::Error>> {
        ready(Ok(infer_definition_types(db, definition)
            .function_type(definition)
            .map(|function| function.literal(db))))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RecursiveTypeNormalizationKey {
    function_literal: salsa::Id,
    nested: bool,
}

// Keep this detector thread-local rather than threading it through every recursive
// normalization helper. Passing an explicit visitor would model the recursion state more
// directly, but it would touch a wide slice of the type-normalization plumbing for no current
// behavioral benefit. This works because recursive normalization currently stays on a single
// thread/query stack; if that ever changes, revisit this and prefer explicit visitor propagation.
std::thread_local! {
    static ACTIVE_RECURSIVE_TYPE_NORMALIZATIONS: ActiveRecursionDetector<RecursiveTypeNormalizationKey> =
        ActiveRecursionDetector::default();
}

/// Runs recursive type normalization under a scoped guard keyed by function literal identity.
///
/// `TypeOf` can make a function signature refer back to the same function through many different
/// type components. Keeping this guard scoped here lets those components keep their ordinary
/// `recursive_type_normalized_impl(db, div, nested)` signatures.
fn visit_recursive_type_normalization<R>(
    function_literal: FunctionLiteral<'_>,
    nested: bool,
    on_cycle: impl FnOnce() -> R,
    func: impl FnOnce() -> R,
) -> R {
    ACTIVE_RECURSIVE_TYPE_NORMALIZATIONS.with(|detector| {
        detector.visit(
            &RecursiveTypeNormalizationKey {
                function_literal: function_literal.last_definition.as_id(),
                nested,
            },
            on_cycle,
            func,
        )
    })
}

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
    pub(crate) parameters: Span,
    /// The span of the annotated return type, if present.
    pub(crate) return_type: Option<Span>,
    /// A span that starts at the beginning of the first decorator (if any),
    /// and ends at the end of the function signature (either the last parameter,
    /// or the return type if present).
    pub(crate) decorators_and_header: Span,
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
        /// `@staticmethod`
        const STATICMETHOD = 1 << 5;
        /// `@typing.override`
        const OVERRIDE = 1 << 6;
        /// `@typing.type_check_only`
        const TYPE_CHECK_ONLY = 1 << 7;
    }
}

impl get_size2::GetSize for FunctionDecorators {}

impl FunctionDecorators {
    pub(super) fn from_decorator_type(db: &dyn Db, decorator_type: Type) -> Self {
        match decorator_type {
            Type::FunctionLiteral(function) => {
                FunctionDecoratorKind::from_known_function(function.known(db))
            }
            Type::ClassLiteral(class) => {
                FunctionDecoratorKind::from_known_class(class.known(db))
            }
            _ => FunctionDecoratorKind::Unknown,
        }
        .flags()
    }
}

/// Classifies function decorators, including `property`, which has no decorator flag.
#[derive(Clone, Copy)]
pub(super) enum FunctionDecoratorKind {
    Known(FunctionDecorators),
    Property,
    Unknown,
}

impl FunctionDecoratorKind {
    pub(super) fn from_known_function(function: Option<KnownFunction>) -> Self {
        match function {
            Some(KnownFunction::NoTypeCheck) => Self::Known(FunctionDecorators::NO_TYPE_CHECK),
            Some(KnownFunction::Overload) => Self::Known(FunctionDecorators::OVERLOAD),
            Some(KnownFunction::AbstractMethod) => Self::Known(FunctionDecorators::ABSTRACT_METHOD),
            Some(KnownFunction::Final) => Self::Known(FunctionDecorators::FINAL),
            Some(KnownFunction::Override) => Self::Known(FunctionDecorators::OVERRIDE),
            Some(KnownFunction::TypeCheckOnly) => Self::Known(FunctionDecorators::TYPE_CHECK_ONLY),
            _ => Self::Unknown,
        }
    }

    pub(super) fn from_known_class(class: Option<KnownClass>) -> Self {
        match class {
            Some(KnownClass::Classmethod) => Self::Known(FunctionDecorators::CLASSMETHOD),
            Some(KnownClass::Staticmethod) => Self::Known(FunctionDecorators::STATICMETHOD),
            Some(KnownClass::Property) => Self::Property,
            _ => Self::Unknown,
        }
    }

    pub(super) fn flags(self) -> FunctionDecorators {
        match self {
            Self::Known(flags) => flags,
            Self::Property | Self::Unknown => FunctionDecorators::empty(),
        }
    }

    pub(super) fn is_unknown(self) -> bool {
        matches!(self, Self::Unknown)
    }
}

bitflags! {
    /// Used for the return type of `dataclass_transform(…)` calls. Keeps track of the
    /// arguments that were passed in. For the precise meaning of the fields, see [1].
    ///
    /// [1]: https://docs.python.org/3/library/typing.html#typing.dataclass_transform
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct DataclassTransformerFlags: u8 {
        const EQ_DEFAULT = 1 << 0;
        const ORDER_DEFAULT = 1 << 1;
        const KW_ONLY_DEFAULT = 1 << 2;
        const FROZEN_DEFAULT = 1 << 3;
    }
}

impl get_size2::GetSize for DataclassTransformerFlags {}

impl Default for DataclassTransformerFlags {
    fn default() -> Self {
        Self::EQ_DEFAULT
    }
}

/// Metadata for a dataclass-transformer. Stored inside a `Type::DataclassTransformer(…)`
/// instance that we use as the return type for `dataclass_transform(…)` calls.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct DataclassTransformerParams<'db> {
    #[returns(copy)]
    pub flags: DataclassTransformerFlags,

    #[returns(deref)]
    pub field_specifiers: Box<[Type<'db>]>,
}

impl get_size2::GetSize for DataclassTransformerParams<'_> {}

/// Whether a function should implicitly be treated as a staticmethod based on its name.
pub(crate) fn is_implicit_staticmethod(function_name: &str) -> bool {
    matches!(function_name, "__new__")
}

/// Whether a function should implicitly be treated as a classmethod based on its name.
pub(crate) fn is_implicit_classmethod(function_name: &str) -> bool {
    matches!(function_name, "__init_subclass__" | "__class_getitem__")
}

/// Representation of a function definition in the AST: either a non-generic function, or a generic
/// function that has not been specialized.
///
/// If a function has multiple overloads, each overload is represented by a separate function
/// definition in the AST, and is therefore a separate `OverloadLiteral` instance.
#[salsa::interned(field_requests = field_requests, debug, heap_size=ruff_memory_usage::heap_size)]
pub struct OverloadLiteral<'db> {
    /// Name of the function at definition.
    #[returns(ref)]
    pub name: ast::name::Name,

    /// Is this a function that we special-case somehow? If so, which one?
    #[returns(copy)]
    pub(crate) known: Option<KnownFunction>,

    /// The scope that's created by the function, in which the function body is evaluated.
    #[returns(copy)]
    pub(crate) body_scope: ScopeId<'db>,

    /// A set of special decorators that were applied to this function
    #[returns(copy)]
    pub(crate) decorators: FunctionDecorators,

    /// If `Some` then contains the `@warnings.deprecated`
    #[returns(copy)]
    pub(crate) deprecated: Option<DeprecatedInstance<'db>>,

    /// The arguments to `dataclass_transformer`, if this function was annotated
    /// with `@dataclass_transformer(...)`.
    #[returns(copy)]
    pub(crate) dataclass_transformer_params: Option<DataclassTransformerParams<'db>>,

    /// Whether this overload or implementation has an explicit return annotation.
    #[returns(copy)]
    pub(crate) has_explicit_return_annotation: bool,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for OverloadLiteral<'_> {}

#[salsa::tracked]
impl<'db> OverloadLiteral<'db> {
    pub(super) fn with_deprecated(
        self,
        db: &'db dyn Db,
        deprecated: DeprecatedInstance<'db>,
    ) -> Self {
        Self::new(
            db,
            self.name(db),
            self.known(db),
            self.body_scope(db),
            self.decorators(db),
            Some(deprecated),
            self.dataclass_transformer_params(db),
            self.has_explicit_return_annotation(db),
        )
    }

    fn with_dataclass_transformer_params(
        self,
        db: &'db dyn Db,
        params: DataclassTransformerParams<'db>,
    ) -> Self {
        Self::new(
            db,
            self.name(db),
            self.known(db),
            self.body_scope(db),
            self.decorators(db),
            self.deprecated(db),
            Some(params),
            self.has_explicit_return_annotation(db),
        )
    }

    fn file(self, db: &'db dyn Db) -> File {
        // NOTE: Do not use `self.definition(db).file(db)` here, as that could create a
        // cross-module dependency on the full AST.
        self.body_scope(db).file(db)
    }

    pub(crate) fn python_file(self, db: &'db dyn Db) -> PythonFile<'db> {
        self.body_scope(db).python_file(db)
    }

    pub(in crate::types) fn program_file(self, db: &'db dyn Db) -> ProgramFile<'db> {
        self.body_scope(db).program_file(db)
    }

    pub(crate) fn has_known_decorator(self, db: &dyn Db, decorator: FunctionDecorators) -> bool {
        self.decorators(db).contains(decorator)
    }

    pub(crate) fn is_overload(self, db: &dyn Db) -> bool {
        self.has_known_decorator(db, FunctionDecorators::OVERLOAD)
    }

    pub(in crate::types) async fn is_overload_with<E: FunctionMetadataEffects<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<bool, E::Error> {
        Ok(effects
            .field(self.field_requests(db).decorators())
            .await?
            .contains(FunctionDecorators::OVERLOAD))
    }

    /// Returns true if this overload is decorated with `@staticmethod`, or if it is implicitly a
    /// staticmethod.
    fn is_staticmethod(self, db: &dyn Db) -> bool {
        legacy_inline(self.is_staticmethod_with(db, &LegacyFunctionIdentityEffects))
    }

    pub(in crate::types) async fn is_staticmethod_with<E: FunctionMetadataEffects<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<bool, E::Error> {
        Ok(effects
            .field(self.field_requests(db).decorators())
            .await?
            .contains(FunctionDecorators::STATICMETHOD)
            || is_implicit_staticmethod(effects.field(self.field_requests(db).name()).await?))
    }

    /// Returns true if this overload is decorated with `@classmethod`, or if it is implicitly a
    /// classmethod.
    fn is_classmethod(self, db: &dyn Db) -> bool {
        legacy_inline(self.is_classmethod_with(db, &LegacyFunctionIdentityEffects))
    }

    pub(in crate::types) async fn is_classmethod_with<E: FunctionMetadataEffects<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<bool, E::Error> {
        Ok(effects
            .field(self.field_requests(db).decorators())
            .await?
            .contains(FunctionDecorators::CLASSMETHOD)
            || is_implicit_classmethod(effects.field(self.field_requests(db).name()).await?))
    }

    /// Returns true if this overload has an implicit `self` or `cls` receiver parameter.
    pub(crate) fn has_implicit_receiver(self, db: &'db dyn Db) -> bool {
        self.body_scope(db).is_method_scope(db) && !self.is_staticmethod(db)
    }

    pub(crate) fn node<'ast>(
        self,
        db: &dyn Db,
        file: File,
        module: &'ast ParsedModuleRef,
    ) -> &'ast ast::StmtFunctionDef {
        debug_assert_eq!(
            file,
            self.file(db),
            "OverloadLiteral::node() must be called with the same file as the one where \
            the function is defined."
        );

        self.body_scope(db).node(db).expect_function().node(module)
    }

    /// Iterate through the decorators on this function, returning the span of the first one
    /// that matches the given predicate.
    fn find_decorator_span(
        self,
        db: &'db dyn Db,
        predicate: impl Fn(Type<'db>) -> bool,
    ) -> Option<Span> {
        let definition = self.definition(db);
        let file = definition.file(db);
        self.node(
            db,
            file,
            &parsed_module(db, definition.python_file(db)).load(db),
        )
        .decorator_list
        .iter()
        .find(|decorator| {
            predicate(definition_expression_type(
                db,
                definition,
                &decorator.expression,
            ))
        })
        .map(|decorator| Span::from(file).with_range(decorator.range))
    }

    /// Iterate through the decorators on this function, returning the span of the first one
    /// that matches the given [`KnownFunction`].
    pub(super) fn find_known_decorator_span(
        self,
        db: &'db dyn Db,
        needle: KnownFunction,
    ) -> Option<Span> {
        self.find_decorator_span(db, |ty| {
            ty.as_function_literal()
                .is_some_and(|f| f.is_known(db, needle))
        })
    }

    /// Returns the [`FileRange`] of the function's name.
    pub(crate) fn focus_range(self, db: &dyn Db, module: &ParsedModuleRef) -> FileRange {
        FileRange::new(
            self.file(db),
            self.body_scope(db)
                .node(db)
                .expect_function()
                .node(module)
                .name
                .range,
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
    pub(in crate::types) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let body_scope = self.body_scope(db);
        let index = semantic_index(db, body_scope.program_file(db));
        index.expect_single_definition(body_scope.node(db).expect_function())
    }

    /// Returns the overload immediately before this one in the AST. Returns `None` if there is no
    /// previous overload.
    pub(in crate::types) async fn previous_overload_with<E: FunctionIdentityEffects<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<Option<FunctionLiteral<'db>>, E::Error> {
        // The semantic model records a use for each function on the name node. This is used
        // here to get the previous function definition with the same name.
        let definition = effects.definition(db, self).await?;
        let scope = effects.field(definition.read_fields(db).scope_id()).await?;
        let Place::Defined(DefinedPlace {
            ty: previous_type,
            definedness: Definedness::AlwaysDefined,
            provenance,
            ..
        }) = effects.preceding_bindings(db, self, definition).await?
        else {
            return Ok(None);
        };

        let previous_literal = match previous_type {
            Type::FunctionLiteral(previous_type) => {
                effects
                    .field(previous_type.field_requests(db).literal())
                    .await?
            }
            Type::Callable(_) => {
                let Some(definition) = provenance.definition() else {
                    return Ok(None);
                };
                let Some(function) = effects.callable_definition(db, definition).await? else {
                    return Ok(None);
                };
                function
            }
            _ => return Ok(None),
        };
        let previous_overload = previous_literal.last_definition;
        if !effects
            .field(previous_overload.field_requests(db).decorators())
            .await?
            .contains(FunctionDecorators::OVERLOAD)
        {
            return Ok(None);
        }

        // These can both happen in edge cases where a definition created with a `def`
        // statement shadows a non-`def` symbol with the same name.
        let previous_name = effects
            .field(previous_overload.field_requests(db).name())
            .await?;
        let name = effects.field(self.field_requests(db).name()).await?;
        if !effects.names_equal(previous_name, name).await? {
            return Ok(None);
        }
        let previous_definition = effects.definition(db, previous_overload).await?;
        if effects
            .field(previous_definition.read_fields(db).scope_id())
            .await?
            != scope
        {
            return Ok(None);
        }

        Ok(Some(previous_literal))
    }

    /// Typed internally-visible signature for this function.
    ///
    /// This represents the annotations on the function itself, unmodified by decorators and
    /// overloads.
    ///
    /// ## Warning
    ///
    /// This uses the semantic index to find the definition of the function. This means that if the
    /// calling query is not in the same file as this function is defined in, then this will create
    /// a cross-module dependency directly on the full AST which will lead to cache
    /// over-invalidation.
    pub(crate) fn signature(self, db: &'db dyn Db) -> Signature<'db> {
        legacy_inline(self.signature_with(db, &InlineSignatureSourceEffects))
    }

    /// Returns the effective signatures of this overload after applying decorators.
    pub(crate) fn decorated_signatures(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = Signature<'db>> + Clone + 'db {
        match binding_type(db, self.definition(db)) {
            Type::Callable(callable) => {
                Either::Left(callable.signatures(db).overloads.iter().cloned())
            }
            _ => Either::Right(std::iter::once(self.signature(db))),
        }
    }

    /// Typed internally-visible "raw" signature for this function.
    /// That is, the return types of async functions are not wrapped in `CoroutineType[...]`.
    /// The `return_callable_typevar_scope` controls whether type variables that only appear in a
    /// return-position `Callable` stay bound to the function or move to the returned callable.
    ///
    /// ## Warning
    ///
    /// This uses the semantic index to find the definition of the function. This means that if the
    /// calling query is not in the same file as this function is defined in, then this will create
    /// a cross-module dependency directly on the full AST which will lead to cache
    /// over-invalidation.
    pub(super) fn raw_signature(
        self,
        db: &'db dyn Db,
        return_callable_typevar_scope: ReturnCallableTypeVarScope,
    ) -> Signature<'db> {
        legacy_inline(self.raw_signature_with(
            db,
            return_callable_typevar_scope,
            &InlineSignatureSourceEffects,
        ))
    }

    pub(crate) fn parameter_span(
        self,
        db: &'db dyn Db,
        parameter_index: Option<usize>,
    ) -> (Span, Span) {
        let file = self.file(db);
        let span = Span::from(file);
        let module = parsed_module(db, self.python_file(db)).load(db);
        let func_def = self.node(db, file, &module);
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
        (name_span, parameter_span)
    }

    /// Returns the range covering a function's name, parameters and optional return annotation.
    pub(in crate::types) fn signature_range_from_node(function: &ast::StmtFunctionDef) -> TextRange {
        let signature = function.name.range.cover(function.parameters.range);
        match &function.returns {
            Some(returns) => signature.cover(returns.range()),
            None => signature,
        }
    }

    pub(crate) fn spans(self, db: &'db dyn Db) -> FunctionSpans {
        let file = self.file(db);
        let span = Span::from(file);
        let module = parsed_module(db, self.python_file(db)).load(db);
        let func_def = self.node(db, file, &module);
        let return_type_range = func_def.returns.as_ref().map(|returns| returns.range());
        let signature = Self::signature_range_from_node(func_def);
        FunctionSpans {
            signature: span.clone().with_range(signature),
            name: span.clone().with_range(func_def.name.range),
            parameters: span.clone().with_range(func_def.parameters.range),
            return_type: return_type_range.map(|range| span.clone().with_range(range)),
            decorators_and_header: span.with_range(signature.cover_offset(func_def.start())),
        }
    }
}

/// Representation of a function definition in the AST, along with any previous overloads of the
/// function. Each overload can be separately generic or not, and each generic overload uses
/// distinct typevars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct FunctionLiteral<'db> {
    pub(crate) last_definition: OverloadLiteral<'db>,
    overloaded: bool,
}

impl<'db> FunctionLiteral<'db> {
    pub(super) fn new(db: &'db dyn Db, last_definition: OverloadLiteral<'db>) -> Self {
        legacy_inline(Self::new_with(
            db,
            last_definition,
            &LegacyFunctionIdentityEffects,
        ))
    }

    pub(in crate::types) async fn new_with<E: FunctionIdentityEffects<'db>>(
        db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
        effects: &E,
    ) -> Result<Self, E::Error> {
        Ok(Self {
            last_definition,
            overloaded: effects
                .field(last_definition.field_requests(db).decorators())
                .await?
                .contains(FunctionDecorators::OVERLOAD)
                || last_definition
                    .previous_overload_with(db, effects)
                    .await?
                    .is_some(),
        })
    }

    /// Ignore previous overloads when applying decorators to an individual definition.
    pub(super) const fn without_overloads(self) -> Self {
        Self {
            overloaded: false,
            ..self
        }
    }

    /// Preserve the overload set and last-definition identity while updating decorator metadata.
    pub(super) fn with_last_definition_metadata(
        self,
        db: &'db dyn Db,
        decorated: OverloadLiteral<'db>,
    ) -> Self {
        let definition = self.last_definition;
        Self {
            last_definition: OverloadLiteral::new(
                db,
                definition.name(db),
                definition.known(db),
                definition.body_scope(db),
                definition.decorators(db),
                decorated.deprecated(db),
                decorated.dataclass_transformer_params(db),
                definition.has_explicit_return_annotation(db),
            ),
            ..self
        }
    }

    fn name(self, db: &'db dyn Db) -> &'db ast::name::Name {
        // All of the overloads of a function literal should have the same name.
        self.last_definition.name(db)
    }

    fn known(self, db: &'db dyn Db) -> Option<KnownFunction> {
        // Whether a function is known is based on its name (and its containing module's name), so
        // all overloads should be known (or not) equivalently.
        self.last_definition.known(db)
    }

    fn has_known_decorator(self, db: &'db dyn Db, decorator: FunctionDecorators) -> bool {
        self.iter_overloads_and_implementation(db)
            .any(|overload| overload.decorators(db).contains(decorator))
    }

    /// If the implementation of this function is deprecated, returns the `@warnings.deprecated`.
    ///
    /// Checking if an overload is deprecated requires deeper call analysis.
    fn implementation_deprecated(self, db: &'db dyn Db) -> Option<DeprecatedInstance<'db>> {
        legacy_inline(self.implementation_deprecated_with(db, &LegacyFunctionIdentityEffects))
    }

    async fn implementation_deprecated_with<E: FunctionMetadataEffects<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<Option<DeprecatedInstance<'db>>, E::Error> {
        let (_overloads, implementation) =
            self.overloads_and_implementation_with(db, effects).await?;
        match implementation {
            Some(overload) => {
                effects
                    .field(overload.field_requests(db).deprecated())
                    .await
            }
            None => Ok(None),
        }
    }

    fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        self.last_definition.definition(db)
    }

    fn parameter_span(self, db: &'db dyn Db, parameter_index: Option<usize>) -> (Span, Span) {
        self.last_definition.parameter_span(db, parameter_index)
    }

    fn spans(self, db: &'db dyn Db) -> FunctionSpans {
        self.last_definition.spans(db)
    }

    fn overloads_and_implementation(
        self,
        db: &'db dyn Db,
    ) -> (&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>) {
        legacy_inline(self.overloads_and_implementation_with(db, &LegacyFunctionIdentityEffects))
    }

    pub(in crate::types) async fn overloads_and_implementation_with<
        E: FunctionMetadataEffects<'db>,
    >(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), E::Error> {
        if !self.overloaded {
            return Ok((&[], Some(self.last_definition)));
        }
        effects
            .overloads_and_implementation(db, self.last_definition)
            .await
    }

    fn overloaded_definitions(
        db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
    ) -> (&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>) {
        let (overloads, implementation) = overloads_and_implementation_inner(db, last_definition);
        (overloads.as_ref(), *implementation)
    }

    pub(super) fn has_separate_implementation(self, db: &'db dyn Db) -> bool {
        legacy_inline(self.has_separate_implementation_with(db, &LegacyFunctionIdentityEffects))
    }

    pub(in crate::types) async fn has_separate_implementation_with<
        E: FunctionMetadataEffects<'db>,
    >(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<bool, E::Error> {
        Ok(self.overloaded && !self.last_definition.is_overload_with(db, effects).await?)
    }

    fn iter_overloads_and_implementation(
        self,
        db: &'db dyn Db,
    ) -> impl DoubleEndedIterator<Item = OverloadLiteral<'db>> + 'db {
        let (overloads, implementation) = self.overloads_and_implementation(db);
        overloads.iter().copied().chain(implementation)
    }

    /// Typed externally-visible signature for this function.
    ///
    /// This is the signature as seen by external callers, possibly modified by decorators and/or
    /// overloaded.
    ///
    /// ## Warning
    ///
    /// This uses the semantic index to find the definition of the function. This means that if the
    /// calling query is not in the same file as this function is defined in, then this will create
    /// a cross-module dependency directly on the full AST which will lead to cache
    /// over-invalidation.
    fn signature(self, db: &'db dyn Db) -> CallableSignature<'db> {
        legacy_inline(self.signature_with(db, &InlineSignatureSourceEffects))
    }

    /// Typed externally-visible signature of the last overload or implementation of this function.
    ///
    /// ## Warning
    ///
    /// This uses the semantic index to find the definition of the function. This means that if the
    /// calling query is not in the same file as this function is defined in, then this will create
    /// a cross-module dependency directly on the full AST which will lead to cache
    /// over-invalidation.
    fn last_definition_signature(self, db: &'db dyn Db) -> Signature<'db> {
        self.last_definition.signature(db)
    }

    /// Typed externally-visible "raw" signature of the last overload or implementation of this function.
    /// The `return_callable_typevar_scope` controls whether type variables that only appear in a
    /// return-position `Callable` stay bound to the function or move to the returned callable.
    ///
    /// ## Warning
    ///
    /// This uses the semantic index to find the definition of the function. This means that if the
    /// calling query is not in the same file as this function is defined in, then this will create
    /// a cross-module dependency directly on the full AST which will lead to cache
    /// over-invalidation.
    fn last_definition_raw_signature(
        self,
        db: &'db dyn Db,
        return_callable_typevar_scope: ReturnCallableTypeVarScope,
    ) -> Signature<'db> {
        self.last_definition
            .raw_signature(db, return_callable_typevar_scope)
    }

    /// Return `Some()` if this function is an abstract method.
    ///
    /// A method can be abstract if it is explicitly decorated with `@abstractmethod`,
    /// or if it is an overloaded `Protocol` method without an implementation,
    /// or if it is a `Protocol` method with a body that solely consists of `pass`/`...`
    /// statements, or if it is a `Protocol` method that only has a docstring,
    /// or if it is a `Protocol` method whose body only consists of a single
    /// `raise NotImplementedError` statement.
    fn as_abstract_method(
        self,
        db: &'db dyn Db,
        enclosing_class: ClassType<'db>,
    ) -> Option<AbstractMethodKind> {
        if self.has_known_decorator(db, FunctionDecorators::ABSTRACT_METHOD) {
            return Some(AbstractMethodKind::Explicit);
        }
        if self.definition(db).file(db).is_stub(db) {
            return None;
        }
        if !enclosing_class.is_protocol(db) {
            return None;
        }
        match self.body_kind(db) {
            FunctionBodyKind::Stub => Some(AbstractMethodKind::ImplicitDueToStubBody),
            FunctionBodyKind::AlwaysRaisesNotImplementedError => {
                Some(AbstractMethodKind::ImplicitDueToAlwaysRaising)
            }
            FunctionBodyKind::Regular => None,
        }
    }

    /// Returns the [`FunctionBodyKind`] of this function.
    ///
    /// For functions without an implementation (e.g., overloaded functions),
    /// returns [`FunctionBodyKind::Stub`].
    fn body_kind(self, db: &'db dyn Db) -> FunctionBodyKind {
        let (_, implementation) = self.overloads_and_implementation(db);
        let Some(implementation) = implementation else {
            return FunctionBodyKind::Stub;
        };
        implementation_body_kind(db, implementation)
    }

    /// Returns `true` if this function has a trivial body.
    ///
    /// A trivial body is one that consists only of `...`, `pass`, or
    /// `raise NotImplementedError`.
    ///
    /// Methods defined in stub files are never considered to have trivial bodies,
    /// since stubs use `...` as a placeholder regardless of the runtime implementation.
    fn has_trivial_body(self, db: &'db dyn Db) -> bool {
        !self.definition(db).file(db).is_stub(db)
            && matches!(
                self.body_kind(db),
                FunctionBodyKind::Stub | FunctionBodyKind::AlwaysRaisesNotImplementedError
            )
    }
}

/// ## Warning
///
/// This uses the semantic index to find the definition of the function. This means that if the
/// calling query is not in the same file as this function is defined in, then this will create
/// a cross-module dependency directly on the full AST which will lead to cache
/// over-invalidation. Cross-module callers should use the tracked
/// [`FunctionType::last_definition_raw_signature`] query instead.
pub(super) fn same_module_uncached_raw_signature<'db>(
    db: &'db dyn Db,
    function: FunctionType<'db>,
    return_callable_typevar_scope: ReturnCallableTypeVarScope,
) -> Signature<'db> {
    function
        .literal(db)
        .last_definition_raw_signature(db, return_callable_typevar_scope)
}

/// Indicates whether a method is explicitly or implicitly abstract.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(super) enum AbstractMethodKind {
    /// The method is explicitly marked as abstract using `@abstractmethod`.
    Explicit,
    /// The method is implicitly abstract due to being in a `Protocol` class without an
    /// implementation.
    ImplicitDueToStubBody,
    /// The method is implicitly abstract due to being in a `Protocol` class with a body that
    /// solely consists of `raise NotImplementedError` statements.
    ImplicitDueToAlwaysRaising,
}

impl AbstractMethodKind {
    pub(super) const fn is_explicit(self) -> bool {
        matches!(self, AbstractMethodKind::Explicit)
    }

    pub(super) const fn is_implicit_due_to_stub_body(self) -> bool {
        matches!(self, AbstractMethodKind::ImplicitDueToStubBody)
    }
}

#[salsa::tracked(configuration = (pub(in crate::types) OverloadsAndImplementationInnerConfiguration), attempt = ReturnOnly,
    returns(ref),
    cycle_initial=|_, _, _| (Box::default(), None),
    heap_size=ruff_memory_usage::heap_size,
)]
fn overloads_and_implementation_inner<'db>(
    db: &'db dyn Db,
    self_overload: OverloadLiteral<'db>,
) -> (Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>) {
    legacy_inline(overloads::collect_overloads_with(
        db,
        self_overload,
        &LegacyFunctionIdentityEffects,
    ))
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn overloads_and_implementation_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<OverloadsAndImplementationInnerConfiguration> {
    overloads_and_implementation_inner::fn_ingredient_(db, db.zalsa())
}

#[salsa::tracked(configuration = (pub(in crate::types) ImplementationBodyKindConfiguration), returns(copy))]
fn implementation_body_kind<'db>(
    db: &'db dyn Db,
    implementation: OverloadLiteral<'db>,
) -> FunctionBodyKind {
    let definition = implementation.definition(db);
    let program_file = definition.program_file(db);
    let python_file = program_file.python_file(db);
    let env = ProgramEnvironment::from_file(program_file);
    let file = python_file.file(db);
    let module = parsed_module(db, python_file).load(db);
    let node = implementation.node(db, file, &module);
    function_body_kind(db, &env, node, |expr| {
        definition_expression_type(db, definition, expr)
    })
}

#[salsa::tracked(configuration = (pub(in crate::types) FunctionLiteralSignatureConfiguration), attempt = ReturnOnly, self_ty = FunctionType<'db>,
    returns(ref),
    cycle_initial=|db, id, function: FunctionType<'db>| {
        let env = ProgramEnvironment::from_scope(
            function.literal(db).last_definition.body_scope(db),
        );
        CallableSignature::cycle_initial(db, &env, id)
    },
    cycle_fn=|db, cycle, previous, value: CallableSignature<'db>, function: FunctionType<'db>| {
        let env = ProgramEnvironment::from_scope(
            function.literal(db).last_definition.body_scope(db),
        );
        value.cycle_normalized(db, &env, previous, cycle)
    },
    heap_size=ruff_memory_usage::heap_size,
)]
fn function_literal_signature<'db>(
    db: &'db dyn Db,
    function: FunctionType<'db>,
) -> CallableSignature<'db> {
    function.literal(db).signature(db)
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn function_literal_signature_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<FunctionLiteralSignatureConfiguration> {
    function_literal_signature::fn_ingredient_(db, db.zalsa())
}

#[salsa::tracked(configuration = (pub(in crate::types) FunctionLastDefinitionSignatureConfiguration), attempt = ReturnOnly, self_ty = FunctionType<'db>,
    returns(ref),
    cycle_initial=|_, _, _|Signature::bottom(),
    heap_size=ruff_memory_usage::heap_size,
)]
fn function_last_definition_signature<'db>(
    db: &'db dyn Db,
    function: FunctionType<'db>,
) -> Signature<'db> {
    legacy_inline(function.last_definition_signature_with(
        db,
        &last_signature::InlineFunctionLastSignatureEffects,
    ))
}

/// Returns the existing canonical query ingredient for a function's last definition signature.
#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn function_last_definition_signature_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<FunctionLastDefinitionSignatureConfiguration> {
    function_last_definition_signature::fn_ingredient_(db, db.zalsa())
}

/// Contains potentially modified signatures for a function literal.
///
/// This uncommon payload is boxed to keep ordinary function types small.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct UpdatedFunctionSignatures<'db> {
    /// Contains a potentially modified signature for this function literal, in case certain
    /// operations (like type mappings) have been applied to it.
    ///
    /// See also: [`FunctionLiteral::signature`].
    signature: Option<CallableSignature<'db>>,

    /// Contains the potentially modified callables for the implementation of an overloaded
    /// function, in case decorators or type mappings have been applied to it. Each callable can
    /// itself be overloaded.
    ///
    /// See also: [`FunctionLiteral::last_definition_signature`].
    implementation_callables: Option<Box<[CallableType<'db>]>>,
}

impl<'db> UpdatedFunctionSignatures<'db> {
    /// Bounds the metadata visits needed to quote cloning and retiring this stored payload.
    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn clone_inspection_work(&self) -> Option<usize> {
        self.signature
            .as_ref()
            .map(|signature| signature.overloads.len())
            .unwrap_or(0)
            .checked_mul(2)?
            .checked_add(6)
    }

    /// Quotes an owned clone, including cleanup if its next interning operation is interrupted.
    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn clone_storage_quote(
        &self,
    ) -> Option<crate::types::storage_quote::StorageQuote> {
        let mut work = 6usize;
        let mut bytes = size_of::<Self>();
        if let Some(signature) = &self.signature {
            work = work
                .checked_add(signature.overloads.len().checked_mul(4)?)?
                .checked_add(signature.retirement_work()?)?;
            bytes = bytes.checked_add(signature.clone_requested_bytes()?)?;
        }
        if let Some(callables) = &self.implementation_callables {
            work = work.checked_add(callables.len().checked_mul(3)?.checked_add(2)?)?;
            bytes = bytes.checked_add(
                std::alloc::Layout::array::<CallableType<'db>>(callables.len())
                    .ok()?
                    .size(),
            )?;
        }
        Some(crate::types::storage_quote::StorageQuote { work, bytes })
    }

    fn new(
        signature: Option<CallableSignature<'db>>,
        implementation_callables: Option<Box<[CallableType<'db>]>>,
    ) -> Option<Box<Self>> {
        (signature.is_some() || implementation_callables.is_some()).then(|| {
            Box::new(Self {
                signature,
                implementation_callables,
            })
        })
    }
}

/// Represents a function type, which might be a non-generic function, or a specialization of a
/// generic function.
#[salsa::interned(field_view = read_fields, field_requests = field_requests, debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct FunctionType<'db> {
    #[returns(copy)]
    pub(crate) literal: FunctionLiteral<'db>,

    #[returns(ref)]
    updated_signatures: Option<Box<UpdatedFunctionSignatures<'db>>>,

    /// This field is used to override the descriptor kind inferred from the function's declaration.
    /// When it is set to `None`, the kind is inferred from the decorators on the function definition
    /// (e.g. `@classmethod`). This field is set to `Some(..)` to override that kind after applying
    /// decorators or descriptor access; for example, extracting a classmethod's `__func__` sets it
    /// to `Some(CallableTypeKind::FunctionLike)`.
    #[returns(copy)]
    pub(super) descriptor_kind: Option<CallableTypeKind>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for FunctionType<'_> {}

pub(super) fn walk_function_type<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    function: FunctionType<'db>,
    visitor: &V,
) {
    if let Some(callable_signature) = function.updated_signature(db) {
        for signature in &callable_signature.overloads {
            walk_signature(db, signature, visitor);
        }
    }
    if let Some(callables) = function.updated_implementation_callables(db) {
        for callable in callables {
            visitor.visit_callable_type(db, *callable);
        }
    }
}

#[salsa::tracked]
impl<'db> FunctionType<'db> {
    pub(crate) fn new(
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
        updated_signatures: Option<Box<UpdatedFunctionSignatures<'db>>>,
    ) -> Self {
        Self::new_internal(db, literal, updated_signatures, None)
    }

    pub(super) fn underlying_function(self, db: &'db dyn Db) -> Self {
        descriptor::underlying_function_sync(self, &descriptor::OrdinaryFunctionDescriptor { db })
            .unwrap_or_else(|never| match never {})
    }

    pub(super) fn with_descriptor_kind(self, db: &'db dyn Db, kind: CallableTypeKind) -> Self {
        descriptor::with_descriptor_kind_sync(
            self,
            kind,
            &descriptor::OrdinaryFunctionDescriptor { db },
        )
        .unwrap_or_else(|never| match never {})
    }

    pub(super) fn without_updated_signatures(self, db: &'db dyn Db) -> Self {
        Self::new_internal(db, self.literal(db), None, self.descriptor_kind(db))
    }

    pub(super) fn updated_signature(self, db: &'db dyn Db) -> Option<&'db CallableSignature<'db>> {
        self.updated_signature_with_fields(salsa::FieldReads::new(db))
    }

    pub(in crate::types) fn updated_signature_with_fields(
        self,
        fields: salsa::FieldReads<'db>,
    ) -> Option<&'db CallableSignature<'db>> {
        self.read_fields(fields)
            .updated_signatures()
            .as_deref()
            .and_then(|updated| updated.signature.as_ref())
    }

    /// Reads the complete retained signature payload without cloning it or inferring signatures.
    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) async fn read_updated_signatures(
        self,
        endpoint: &salsa::execution_probe::TaskEndpoint<'_, 'db>,
    ) -> &'db Option<Box<UpdatedFunctionSignatures<'db>>> {
        endpoint
            .read_field(
                self.field_requests(endpoint.field_request_context())
                    .updated_signatures(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await
    }

    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) async fn read_updated_signature(
        self,
        endpoint: &salsa::execution_probe::TaskEndpoint<'_, 'db>,
    ) -> Option<&'db CallableSignature<'db>> {
        self.read_updated_signatures(endpoint)
            .await
            .as_deref()
            .and_then(|updated| updated.signature.as_ref())
    }

    pub(in crate::types) fn updated_implementation_callables_with_fields(
        self,
        fields: salsa::FieldReads<'db>,
    ) -> Option<&'db [CallableType<'db>]> {
        self.read_fields(fields)
            .updated_signatures()
            .as_deref()
            .and_then(|updated| updated.implementation_callables.as_deref())
    }

    pub(super) fn updated_implementation_callables(
        self,
        db: &'db dyn Db,
    ) -> Option<&'db [CallableType<'db>]> {
        self.updated_implementation_callables_with_fields(salsa::FieldReads::new(db))
    }

    /// Return all effective implementation callables, falling back to the raw implementation.
    pub(super) fn implementation_callables(self, db: &'db dyn Db) -> Cow<'db, [CallableType<'db>]> {
        self.updated_implementation_callables(db).map_or_else(
            || {
                Cow::Owned(vec![CallableType::single(
                    db,
                    self.last_definition_signature(db).clone(),
                )])
            },
            Cow::Borrowed,
        )
    }

    /// Retain decorated implementation callables without changing the caller-visible overloads.
    pub(super) fn with_implementation_callables(
        self,
        db: &'db dyn Db,
        implementation_callables: Box<[CallableType<'db>]>,
    ) -> Self {
        Self::new_internal(
            db,
            self.literal(db),
            UpdatedFunctionSignatures::new(
                self.updated_signature(db).cloned(),
                Some(implementation_callables),
            ),
            self.descriptor_kind(db),
        )
    }

    pub(crate) fn with_inherited_generic_context(
        self,
        db: &'db dyn Db,
        inherited_generic_context: GenericContext<'db>,
    ) -> Self {
        inherited_context::with_inherited_generic_context_sync(
            self,
            inherited_generic_context,
            &inherited_context::OrdinaryFunctionInheritedContext { db },
        )
        .unwrap_or_else(|never| match never {})
    }

    pub(crate) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        legacy_inline(mapping::map_function_with(
            db,
            self,
            type_mapping,
            tcx,
            visitor,
            &mapping::InlineFunctionMappingEffects,
        ))
    }

    pub(crate) fn with_dataclass_transformer_params(
        self,
        db: &'db dyn Db,
        params: DataclassTransformerParams<'db>,
    ) -> Self {
        // A decorator only applies to the specific overload that it is attached to, not to all
        // previous overloads.
        let literal = self.literal(db);
        let literal = FunctionLiteral {
            last_definition: literal
                .last_definition
                .with_dataclass_transformer_params(db, params),
            ..literal
        };
        Self::new_internal(db, literal, None, self.descriptor_kind(db))
    }

    pub(crate) fn with_deprecated(
        self,
        db: &'db dyn Db,
        deprecated: DeprecatedInstance<'db>,
    ) -> Self {
        // A decorator only applies to the specific overload that it is attached to, not to all
        // previous overloads.
        let literal = self.literal(db);
        let literal = FunctionLiteral {
            last_definition: literal.last_definition.with_deprecated(db, deprecated),
            ..literal
        };
        Self::new_internal(
            db,
            literal,
            self.updated_signatures(db),
            self.descriptor_kind(db),
        )
    }

    /// Returns the [`File`] in which this function is defined.
    pub(crate) fn file(self, db: &'db dyn Db) -> File {
        self.literal(db).last_definition.file(db)
    }

    pub(crate) fn python_file(self, db: &'db dyn Db) -> PythonFile<'db> {
        self.literal(db).last_definition.python_file(db)
    }

    pub(crate) fn program_file(self, db: &'db dyn Db) -> ProgramFile<'db> {
        self.literal(db).last_definition.program_file(db)
    }

    /// Returns the AST node for this function.
    pub(super) fn node<'ast>(
        self,
        db: &dyn Db,
        file: File,
        module: &'ast ParsedModuleRef,
    ) -> &'ast ast::StmtFunctionDef {
        self.literal(db).last_definition.node(db, file, module)
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
    pub(crate) fn has_known_decorator(
        self,
        db: &'db dyn Db,
        decorator: FunctionDecorators,
    ) -> bool {
        self.literal(db).has_known_decorator(db, decorator)
    }

    /// Returns true if every definition of this method uses `@classmethod`, or is implicitly a
    /// classmethod. An inconsistently applied decorator does not affect method binding.
    pub(crate) fn is_classmethod(self, db: &'db dyn Db) -> bool {
        descriptor::function_is_classmethod_sync(
            self,
            &descriptor::OrdinaryFunctionDescriptor { db },
        )
        .unwrap_or_else(|never| match never {})
    }

    /// Returns true if every definition of this method uses `@staticmethod`, or is implicitly a
    /// static method. An inconsistently applied decorator does not affect method binding.
    pub(crate) fn is_staticmethod(self, db: &'db dyn Db) -> bool {
        descriptor::function_is_staticmethod_sync(
            self,
            &descriptor::OrdinaryFunctionDescriptor { db },
        )
        .unwrap_or_else(|never| match never {})
    }

    /// Whether this function was declared as a staticmethod, even if descriptor access has
    /// already exposed the ordinary function. Diagnostics can still use its declaration kind.
    pub(super) fn has_staticmethod_declaration(self, db: &'db dyn Db) -> bool {
        legacy_inline(self.has_staticmethod_declaration_with(db, &LegacyFunctionIdentityEffects))
    }

    pub(in crate::types) async fn has_staticmethod_declaration_with<
        E: FunctionMetadataEffects<'db>,
    >(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<bool, E::Error> {
        let (overloads, implementation) =
            self.overloads_and_implementation_with(db, effects).await?;
        let mut overloads = overloads.iter().copied().chain(implementation);
        // Overload discovery can return no definitions during cycle recovery.
        let Some(first) = overloads.next() else {
            return Ok(false);
        };
        if !first.is_staticmethod_with(db, effects).await? {
            return Ok(false);
        }
        for overload in overloads {
            if !overload.is_staticmethod_with(db, effects).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Returns true if this function has an implicit `self` or `cls` receiver parameter.
    pub(crate) fn has_implicit_receiver(self, db: &'db dyn Db) -> bool {
        self.literal(db).last_definition.has_implicit_receiver(db)
    }

    /// If the implementation of this function is deprecated, returns the `@warnings.deprecated`.
    ///
    /// Checking if an overload is deprecated requires deeper call analysis.
    pub(crate) fn implementation_deprecated(
        self,
        db: &'db dyn Db,
    ) -> Option<DeprecatedInstance<'db>> {
        self.literal(db).implementation_deprecated(db)
    }

    pub(in crate::types) async fn implementation_deprecated_with<
        E: FunctionMetadataEffects<'db>,
    >(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<Option<DeprecatedInstance<'db>>, E::Error> {
        let literal = effects.field(self.field_requests(db).literal()).await?;
        literal.implementation_deprecated_with(db, effects).await
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
        legacy_inline(self.definition_with(db, &LegacyFunctionIdentityEffects))
    }

    pub(in crate::types) async fn definition_with<E: FunctionIdentityEffects<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<Definition<'db>, E::Error> {
        let literal = effects.field(self.field_requests(db).literal()).await?;
        effects.definition(db, literal.last_definition).await
    }

    /// Returns `true` if this function's last definition uses the same place as `other`.
    pub(crate) fn has_same_place_as(self, db: &'db dyn Db, other: FunctionType<'db>) -> bool {
        self.last_definition(db).place(db) == other.last_definition(db).place(db)
    }

    /// Returns the [`Definition`] for the last overload or implementation in this function.
    pub(crate) fn last_definition(self, db: &'db dyn Db) -> Definition<'db> {
        self.literal(db).last_definition.definition(db)
    }

    /// Returns `true` if this function includes `definition` as one of its overload signatures or
    /// implementation.
    pub(crate) fn contains_definition(self, db: &'db dyn Db, definition: Definition<'db>) -> bool {
        self.iter_overloads_and_implementation(db)
            .any(|overload| overload.definition(db) == definition)
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
    ) -> (Span, Span) {
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
    pub(crate) fn spans(self, db: &'db dyn Db) -> FunctionSpans {
        self.literal(db).spans(db)
    }

    /// Returns `true` if this function has a trivial body.
    pub(crate) fn has_trivial_body(self, db: &'db dyn Db) -> bool {
        self.literal(db).has_trivial_body(db)
    }

    /// Returns `true` if any overload or implementation has an explicit return annotation.
    ///
    /// This distinguishes untyped decorators that infer an unknown return from decorators that
    /// explicitly promise a replacement type:
    /// ```python
    /// def identity(cls):
    ///     return cls
    ///
    /// def replace(cls) -> object:
    ///     return object()
    /// ```
    pub(crate) fn has_explicit_return_annotation(self, db: &'db dyn Db) -> bool {
        self.iter_overloads_and_implementation(db)
            .any(|overload| overload.has_explicit_return_annotation(db))
    }

    /// Returns all of the overload signatures and the implementation definition, if any, of this
    /// function. The overload signatures will be in source order.
    pub(crate) fn overloads_and_implementation(
        self,
        db: &'db dyn Db,
    ) -> (&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>) {
        self.literal(db).overloads_and_implementation(db)
    }

    pub(in crate::types) async fn overloads_and_implementation_with<
        E: FunctionMetadataEffects<'db>,
    >(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), E::Error> {
        let literal = effects.field(self.field_requests(db).literal()).await?;
        literal.overloads_and_implementation_with(db, effects).await
    }

    /// Returns an iterator of all of the definitions of this function, including both overload
    /// signatures and any implementation, all in source order.
    pub(crate) fn iter_overloads_and_implementation(
        self,
        db: &'db dyn Db,
    ) -> impl DoubleEndedIterator<Item = OverloadLiteral<'db>> + 'db {
        self.literal(db).iter_overloads_and_implementation(db)
    }

    pub(crate) fn first_overload_or_implementation(self, db: &'db dyn Db) -> OverloadLiteral<'db> {
        self.iter_overloads_and_implementation(db)
            .next()
            .expect("A function must have at least one overload/implementation")
    }

    /// Typed externally-visible signature for this function.
    ///
    /// This is the signature as seen by external callers, possibly modified by decorators and/or
    /// overloaded.
    pub(crate) fn signature(self, db: &'db dyn Db) -> &'db CallableSignature<'db> {
        self.updated_signature(db)
            .unwrap_or_else(|| self.literal_signature(db))
    }

    /// This query isolates the function's AST dependency, so callers only invalidate when the
    /// computed signature changes. Updated signatures are already stored on the interned function.
    fn literal_signature(self, db: &'db dyn Db) -> &'db CallableSignature<'db> {
        function_literal_signature(db, self)
    }

    /// Refer to this signature's equation, including recursive `TypeOf` references to itself.
    pub(crate) fn variance_of(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        VarianceTerm::variable(db, VarianceOrigin::Function(self), typevar)
    }

    /// Build the signature's equation in the function's defining environment, independent of
    /// the caller's environment. Recursive `TypeOf` annotations remain named references.
    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, _, _, _| VarianceTerm::BIVARIANT,
        heap_size=ruff_memory_usage::heap_size,
    )]
    pub(in crate::types) fn variance_equation(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        let env = ProgramEnvironment::from_scope(self.literal(db).last_definition.body_scope(db));
        self.signature(db).variance_of(db, &env, typevar)
    }

    /// Typed externally-visible signature of the last overload or implementation of this function.
    ///
    /// ## Why is this a salsa query?
    ///
    /// This is a salsa query to short-circuit the invalidation
    /// when the function's AST node changes.
    ///
    /// Were this not a salsa query, then the calling query
    /// would depend on the function's AST and rerun for every change in that file.
    pub(crate) fn last_definition_signature(self, db: &'db dyn Db) -> &'db Signature<'db> {
        function_last_definition_signature(db, self)
    }

    /// Typed externally-visible "raw" signature of the last overload or implementation of this function.
    /// The `return_callable_typevar_scope` controls whether type variables that only appear in a
    /// return-position `Callable` stay bound to the function or move to the returned callable.
    #[salsa::tracked(attempt = ReturnOnly,
        returns(ref),
        cycle_initial=|_, _, _, _|Signature::bottom(),
        heap_size=ruff_memory_usage::heap_size,
    )]
    pub(super) fn last_definition_raw_signature(
        self,
        db: &'db dyn Db,
        return_callable_typevar_scope: ReturnCallableTypeVarScope,
    ) -> Signature<'db> {
        self.literal(db)
            .last_definition_raw_signature(db, return_callable_typevar_scope)
    }

    /// Return the kind for this function when it is converted into a [`CallableType`].
    pub(crate) fn callable_type_kind(self, db: &'db dyn Db) -> CallableTypeKind {
        legacy_inline(self.callable_type_kind_with(db, &LegacyFunctionIdentityEffects))
    }

    pub(in crate::types) async fn callable_type_kind_with<E: FunctionConversionEffects<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<CallableTypeKind, E::Error> {
        if let Some(kind) = effects
            .field(self.field_requests(db).descriptor_kind())
            .await?
        {
            return Ok(match kind {
                CallableTypeKind::ClassMethodLike | CallableTypeKind::StaticMethodLike => kind,
                _ => CallableTypeKind::FunctionLike,
            });
        }
        let (overloads, implementation) =
            self.overloads_and_implementation_with(db, effects).await?;
        effects
            .local(
                overloads
                    .len()
                    .checked_add(2)
                    .and_then(|n| n.checked_mul(2)),
                || (),
            )
            .await?;
        // A descriptor kind applies only when every definition agrees. Cycle
        // recovery can expose no definitions, which retains the function kind.
        let definitions = || overloads.iter().copied().chain(implementation);
        let mut all_classmethods = false;
        for overload in definitions() {
            all_classmethods = overload.is_classmethod_with(db, effects).await?;
            if !all_classmethods {
                break;
            }
        }
        if all_classmethods {
            return Ok(CallableTypeKind::ClassMethodLike);
        }
        let mut all_staticmethods = false;
        for overload in definitions() {
            all_staticmethods = overload.is_staticmethod_with(db, effects).await?;
            if !all_staticmethods {
                break;
            }
        }
        Ok(if all_staticmethods {
            CallableTypeKind::StaticMethodLike
        } else {
            CallableTypeKind::FunctionLike
        })
    }

    pub(super) fn runtime_class(self, db: &'db dyn Db) -> KnownClass {
        legacy_inline(self.runtime_class_with(db, &LegacyFunctionIdentityEffects))
    }

    pub(in crate::types) async fn runtime_class_with<E: FunctionConversionEffects<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<KnownClass, E::Error> {
        Ok(match self.callable_type_kind_with(db, effects).await? {
            CallableTypeKind::ClassMethodLike => KnownClass::Classmethod,
            CallableTypeKind::StaticMethodLike => KnownClass::Staticmethod,
            _ => KnownClass::FunctionType,
        })
    }

    /// Convert the `FunctionType` into a [`CallableType`].
    pub(crate) fn into_callable_type(self, db: &'db dyn Db) -> CallableType<'db> {
        legacy_inline(self.into_callable_type_with(db, &LegacyFunctionIdentityEffects))
    }

    pub(in crate::types) async fn into_callable_type_with<E: FunctionConversionEffects<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<CallableType<'db>, E::Error> {
        let signatures = effects.signature(db, self).await?;
        let kind = self.callable_type_kind_with(db, effects).await?;
        effects.callable(db, signatures, kind).await
    }

    pub(crate) fn into_bound_method_type(
        self,
        db: &'db dyn Db,
        self_instance: Type<'db>,
    ) -> BoundMethodType<'db> {
        BoundMethodType::new(db, self, self_instance)
    }

    pub(crate) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        self.signature(db)
            .find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
    }

    pub(crate) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        visit_recursive_type_normalization(
            self.literal(db),
            nested,
            || None,
            || {
                let literal = self.literal(db);
                let updated_signature = match self.updated_signature(db) {
                    Some(signature) => {
                        Some(signature.recursive_type_normalized_impl(db, env, div, nested)?)
                    }
                    None => None,
                };
                let updated_implementation_callables =
                    match self.updated_implementation_callables(db) {
                        Some(callables) => Some(
                            callables
                                .iter()
                                .map(|callable| {
                                    callable.recursive_type_normalized_impl(db, env, div, nested)
                                })
                                .collect::<Option<Box<_>>>()?,
                        ),
                        None => None,
                    };
                Some(Self::new_internal(
                    db,
                    literal,
                    UpdatedFunctionSignatures::new(
                        updated_signature,
                        updated_implementation_callables,
                    ),
                    self.descriptor_kind(db),
                ))
            },
        )
    }

    pub(super) fn as_abstract_method(
        self,
        db: &'db dyn Db,
        enclosing_class: ClassType<'db>,
    ) -> Option<AbstractMethodKind> {
        self.literal(db).as_abstract_method(db, enclosing_class)
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    pub(super) fn check_function_pair(
        &self,
        db: &'db dyn Db,
        source: FunctionType<'db>,
        target: FunctionType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if source.literal(db) != target.literal(db)
            || source.descriptor_kind(db) != target.descriptor_kind(db)
        {
            return self.never();
        }
        self.check_callable_signature_pair(db, source.signature(db), target.signature(db))
    }
}

/// Check the second argument to `isinstance()` or `issubclass()` for types that cannot be used
/// at runtime (protocol classes, typed dicts, `typing.Any` in `isinstance`, and invalid
/// `UnionType` elements). Handles class literals, tuples (including nested tuples), and
/// recursively validates each element.
///
/// `classinfo_expr` is the AST expression corresponding to `classinfo`, if available. It is
/// used for precise annotation spans (e.g., highlighting just the `UnionType` inside a tuple
/// rather than the whole tuple). It may be `None` when the tuple is not a literal in the AST
/// (e.g., when it's stored in a variable).
fn check_classinfo_in_isinstance<'db>(
    db: &'db dyn Db,
    context: &InferContext<'db, '_>,
    call_expression: &ast::ExprCall,
    function: KnownFunction,
    classinfo: Type<'db>,
    classinfo_expr: Option<&ast::Expr>,
) {
    match classinfo {
        Type::ClassLiteral(class) => {
            if class.is_typed_dict(db) {
                report_runtime_check_against_typed_dict(context, call_expression, class, function);
            } else if let Some(protocol_class) = class.into_protocol_class(db) {
                if !protocol_class.is_runtime_checkable(db) {
                    report_runtime_check_against_non_runtime_checkable_protocol(
                        context,
                        call_expression,
                        protocol_class,
                        function,
                    );
                } else if function == KnownFunction::IsSubclass {
                    let non_method_members = protocol_class.interface(db).non_method_members(db);
                    if !non_method_members.is_empty() {
                        report_issubclass_check_against_protocol_with_non_method_members(
                            context,
                            call_expression,
                            protocol_class,
                            &non_method_members,
                        );
                    }
                }
            }
        }
        Type::SpecialForm(SpecialFormType::Any) if function == KnownFunction::IsInstance => {
            let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, call_expression) else {
                return;
            };
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "`typing.Any` cannot be used with `isinstance()`"
            ));
            diagnostic
                .set_primary_annotation_message("This call will raise `TypeError` at runtime");
        }
        Type::KnownInstance(KnownInstanceType::UnionType(_)) => {
            report_invalid_union_type_elements(
                db,
                context,
                call_expression,
                function,
                classinfo,
                classinfo_expr,
            );
        }
        Type::NominalInstance(nominal)
            if let Some(tuple_spec) = nominal.tuple_spec(db, context.program_environment()) =>
        {
            let element_exprs = match classinfo_expr {
                Some(ast::Expr::Tuple(tuple_expr)) => Some(&tuple_expr.elts),
                _ => None,
            };
            for (index, element) in tuple_spec.iter_element_types(db).enumerate() {
                let element_expr = element_exprs.and_then(|elts| elts.get(index));
                check_classinfo_in_isinstance(
                    db,
                    context,
                    call_expression,
                    function,
                    element,
                    element_expr,
                );
            }
        }

        _ => {}
    }
}

/// Report an error if a `types.UnionType` instance passed to `isinstance()`/`issubclass()`
/// contains elements that are not class objects.
fn report_invalid_union_type_elements<'db>(
    db: &'db dyn Db,
    context: &InferContext<'db, '_>,
    call_expression: &ast::ExprCall,
    function: KnownFunction,
    union_type: Type<'db>,
    union_type_expr: Option<&ast::Expr>,
) {
    fn find_invalid_elements<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        function: KnownFunction,
        ty: Type<'db>,
        invalid_elements: &mut Vec<Type<'db>>,
    ) {
        match ty {
            Type::ClassLiteral(_) => {}
            Type::NominalInstance(instance)
                if instance.has_known_class(db, KnownClass::NoneType) => {}
            Type::SpecialForm(special_form) if special_form.is_valid_isinstance_target() => {}
            // `Any` can be used in `issubclass()` calls but not `isinstance()` calls
            Type::SpecialForm(SpecialFormType::Any) if function == KnownFunction::IsSubclass => {}
            Type::KnownInstance(KnownInstanceType::UnionType(instance)) => {
                match instance.value_expression_types(db, env) {
                    Ok(value_expression_types) => {
                        for element in value_expression_types {
                            find_invalid_elements(db, env, function, element, invalid_elements);
                        }
                    }
                    Err(_) => {
                        invalid_elements.push(ty);
                    }
                }
            }
            _ => invalid_elements.push(ty),
        }
    }

    let mut invalid_elements = vec![];
    let env = context.program_environment();
    find_invalid_elements(db, env, function, union_type, &mut invalid_elements);

    let Some((first_invalid_element, other_invalid_elements)) = invalid_elements.split_first()
    else {
        return;
    };

    let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, call_expression) else {
        return;
    };

    let function_name: &str = function.into();

    let mut diagnostic =
        builder.into_diagnostic(format_args!("Invalid second argument to `{function_name}`"));
    diagnostic.info(format_args!(
        "A `UnionType` instance can only be used as the second argument to \
        `{function_name}` if all elements are class objects"
    ));
    if let Some(union_type_expr) = union_type_expr {
        diagnostic.annotate(
            Annotation::secondary(context.span(union_type_expr))
                .message("This `UnionType` instance contains non-class elements"),
        );
    }

    // When we have a secondary annotation pointing at the UnionType expression,
    // "the union" is unambiguous. Otherwise, spell out the union type in the message.
    let env = context.program_environment();
    let union_suffix = match (&union_type_expr, union_type) {
        (None, Type::KnownInstance(KnownInstanceType::UnionType(instance))) => {
            match instance.union_type(db) {
                Ok(ty) => format!(" `{}`", ty.display(db, env)),
                Err(_) => String::new(),
            }
        }
        _ => String::new(),
    };

    match other_invalid_elements {
        [] => diagnostic.info(format_args!(
            "Element `{}` in the union{union_suffix} is not a class object",
            first_invalid_element.display(db, env)
        )),
        [single] => diagnostic.info(format_args!(
            "Elements `{}` and `{}` in the union{union_suffix} are not class objects",
            first_invalid_element.display(db, env),
            single.display(db, env),
        )),
        _ => diagnostic.info(format_args!(
            "Element `{}` in the union{union_suffix}, and {} more elements, are not class objects",
            first_invalid_element.display(db, env),
            other_invalid_elements.len(),
        )),
    }
}

/// Evaluate an `isinstance` call. Return `Truthiness::AlwaysTrue` if we can definitely infer that
/// this will return `True` at runtime, `Truthiness::AlwaysFalse` if we can definitely infer
/// that this will return `False` at runtime, or `Truthiness::Ambiguous` if we should infer `bool`
/// instead.
fn is_instance_truthiness<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    class: ClassLiteral<'db>,
) -> Truthiness {
    let is_instance = |ty: &Type<'_>| {
        ty.as_nominal_instance().is_some_and(|instance| {
            instance
                .class(db, env)
                .is_subtype_of_class_literal(db, class)
        })
    };

    let always_true_if = |test: bool| {
        if test {
            Truthiness::AlwaysTrue
        } else {
            Truthiness::Ambiguous
        }
    };

    match ty {
        Type::Recursive(recursive) => recursive
            .unfold(db, env)
            .map(|unfolded| is_instance_truthiness(db, env, unfolded, class))
            .unwrap_or(Truthiness::Ambiguous),
        Type::RecursiveVar(_) => {
            unreachable!("semantic operation on an unbound recursive variable")
        }
        Type::Union(..) => {
            // We do not handle unions specifically here, because something like `A | SubclassOfA` would
            // have been simplified to `A` anyway
            Truthiness::Ambiguous
        }

        // Create a new intersection that maps type variables to their upper bounds,
        // and evaluate the truthiness of the `isinstance()` check with that type.
        // Along the way, short-circuit to `AlwaysTrue` if we find any positive element
        // that is always true.
        Type::Intersection(intersection) => {
            let mut effective = IntersectionBuilder::new(db, env);
            let mut found_tvars_or_newtypes = false;

            for &positive in intersection.positive(db) {
                if is_instance_truthiness(db, env, positive, class).is_always_true() {
                    return Truthiness::AlwaysTrue;
                } else if let Type::TypeVar(tvar) = positive {
                    match tvar.require_bound_or_constraints(db, env) {
                        TypeVarBoundOrConstraints::UpperBound(bound) => {
                            effective.add_positive_in_place(bound);
                        }
                        TypeVarBoundOrConstraints::Constraints(constraints) => {
                            effective.add_positive_in_place(constraints.as_type(db, env));
                        }
                    }
                    found_tvars_or_newtypes = true;
                } else if let Type::NewTypeInstance(newtype) = positive {
                    found_tvars_or_newtypes = true;
                    effective.add_positive_in_place(newtype.concrete_base_type(db));
                } else {
                    effective.add_positive_in_place(positive);
                }
            }

            if !found_tvars_or_newtypes {
                return Truthiness::Ambiguous;
            }

            for &negative in intersection.negative(db) {
                if is_instance_truthiness(db, env, negative, class).is_always_true() {
                    return Truthiness::AlwaysFalse;
                }
                effective.add_negative_in_place(negative);
            }

            let effective = effective.build();

            if effective == ty {
                Truthiness::Ambiguous
            } else {
                is_instance_truthiness(db, env, effective, class)
            }
        }

        Type::EnumComplement(complement) => {
            is_instance_truthiness(db, env, complement.to_intersection(db, env), class)
        }

        Type::NominalInstance(..) => always_true_if(is_instance(&ty)),

        Type::NewTypeInstance(newtype) => {
            always_true_if(is_instance(&newtype.concrete_base_type(db)))
        }

        Type::LiteralValue(..) | Type::ModuleLiteral(..) | Type::FunctionLiteral(..) => {
            always_true_if(
                ty.literal_fallback_instance(db, env)
                    .as_ref()
                    .is_some_and(is_instance),
            )
        }

        Type::ClassLiteral(..) => {
            always_true_if(is_instance(&KnownClass::Type.to_instance(db, env)))
        }

        Type::TypeAlias(alias) => is_instance_truthiness(db, env, alias.value_type(db), class),

        Type::TypeVar(bound_typevar) => match bound_typevar.require_bound_or_constraints(db, env) {
            TypeVarBoundOrConstraints::UpperBound(bound) => {
                is_instance_truthiness(db, env, bound, class)
            }
            TypeVarBoundOrConstraints::Constraints(constraints) => always_true_if(
                constraints
                    .elements(db)
                    .iter()
                    .all(|c| is_instance_truthiness(db, env, *c, class).is_always_true()),
            ),
        },

        Type::BoundMethod(..)
        | Type::KnownBoundMethod(..)
        | Type::WrapperDescriptor(..)
        | Type::DataclassDecorator(..)
        | Type::DataclassTransformer(..)
        | Type::GenericAlias(..)
        | Type::SubclassOf(..)
        | Type::ProtocolInstance(..)
        | Type::SpecialForm(..)
        | Type::KnownInstance(..)
        | Type::PropertyInstance(..)
        | Type::SlotDescriptor(..)
        | Type::AlwaysTruthy
        | Type::AlwaysFalsy
        | Type::BoundSuper(..)
        | Type::TypeIs(..)
        | Type::TypeGuard(..)
        | Type::TypeForm(..)
        | Type::Callable(..)
        | Type::Dynamic(..)
        | Type::Divergent(_)
        | Type::Never
        | Type::TypedDict(_) => {
            // We could probably try to infer more precise types in some of these cases, but it's unclear
            // if it's worth the effort.
            Truthiness::Ambiguous
        }
    }
}

/// Return whether a fixed `isinstance` tuple covers every possible type of an input.
///
/// Each class in the tuple uses the same truthiness inference as a single-class `isinstance` check.
///
/// ```python
/// def f(x: A | B) -> bool:
///     if isinstance(x, (A, B)):
///         return True
/// ```
fn is_instance_tuple_exhaustive<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    classinfo: Type<'db>,
) -> bool {
    let Some(tuple) = classinfo.tuple_instance_spec(db, env) else {
        return false;
    };
    if tuple.is_variadic() {
        return false;
    }

    is_instance_tuple_covers(db, env, &tuple, ty, &ActiveRecursionDetector::default())
}

fn is_instance_tuple_covers<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    tuple: &TupleSpec<'db>,
    ty: Type<'db>,
    recursion_guard: &ActiveRecursionDetector<Type<'db>>,
) -> bool {
    match ty {
        Type::TypeAlias(_) | Type::Recursive(_) => recursion_guard.visit(
            &ty,
            || true,
            || is_instance_tuple_covers(db, env, tuple, ty.resolve_type_alias(db), recursion_guard),
        ),
        Type::Union(union) => union
            .elements(db)
            .iter()
            .all(|element| is_instance_tuple_covers(db, env, tuple, *element, recursion_guard)),
        Type::Intersection(intersection) => intersection
            .positive(db)
            .iter()
            .any(|element| is_instance_tuple_covers(db, env, tuple, *element, recursion_guard)),
        Type::TypeVar(typevar) => match typevar.require_bound_or_constraints(db, env) {
            TypeVarBoundOrConstraints::UpperBound(bound) => {
                is_instance_tuple_covers(db, env, tuple, bound, recursion_guard)
            }
            TypeVarBoundOrConstraints::Constraints(constraints) => {
                constraints.elements(db).iter().all(|constraint| {
                    is_instance_tuple_covers(db, env, tuple, *constraint, recursion_guard)
                })
            }
        },
        ty => tuple.fixed_elements().any(|element| {
            let Type::ClassLiteral(class) = element else {
                return false;
            };
            is_instance_truthiness(db, env, ty, *class).is_always_true()
        }),
    }
}

/// Returns `true` if the function body is stub-like, ignoring a leading docstring.
pub(crate) fn function_has_stub_body(node: &ast::StmtFunctionDef) -> bool {
    let suite = ast::helpers::body_without_leading_docstring(&node.body);

    suite.iter().all(|stmt| match stmt {
        ast::Stmt::Pass(_) => true,
        ast::Stmt::Expr(ast::StmtExpr { value, .. }) => value.is_ellipsis_literal_expr(),
        _ => false,
    })
}

/// Classify the body of this function:
/// - [`FunctionBodyKind::Stub`] if it is a stub function (i.e., only contains `pass` or `...`
/// - [`FunctionBodyKind::AlwaysRaisesNotImplementedError`] if it consists of a single
///   `raise NotImplementedError` statement
/// - [`FunctionBodyKind::Regular`] otherwise
///
/// In all cases, we allow a docstring as the first statement in the function body;
/// the analysis is only done on the remaining statements if the first is a docstring.
pub(super) fn function_body_kind<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    node: &ast::StmtFunctionDef,
    infer_type: impl Fn(&ast::Expr) -> Type<'db>,
) -> FunctionBodyKind {
    // Allow docstrings, but only as the first statement.
    let suite = ast::helpers::body_without_leading_docstring(&node.body);

    if function_has_stub_body(node) {
        return FunctionBodyKind::Stub;
    }

    if let [ast::Stmt::Raise(raise)] = suite
        && let ast::StmtRaise {
            exc: Some(exc),
            cause: None,
            node_index: _,
            range: _,
        } = raise
    {
        if infer_type(exc).is_subtype_of(
            db,
            env,
            UnionType::from_two_elements(
                db,
                env,
                KnownClass::NotImplementedError.to_class_literal(db, env),
                KnownClass::NotImplementedError.to_instance(db, env),
            ),
        ) {
            return FunctionBodyKind::AlwaysRaisesNotImplementedError;
        }
    }

    FunctionBodyKind::Regular
}

/// Classification of function body kinds.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum FunctionBodyKind {
    /// The function body only consists of `...`, `pass`, and/or a docstring.
    Stub,
    /// The function body consists of a single `raise NotImplementedError` statement,
    /// possibly preceded by a docstring.
    AlwaysRaisesNotImplementedError,
    /// Any function body that is not a stub and does not consist of a single
    /// `raise NotImplementedError` statement.
    Regular,
}

/// Non-exhaustive enumeration of known functions (e.g. `builtins.reveal_type`, ...) that might
/// have special behavior.
#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    strum_macros::EnumCount,
    strum_macros::EnumString,
    strum_macros::IntoStaticStr,
    get_size2::GetSize,
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
    /// `builtins.__import__`, which returns the top-level module.
    #[strum(serialize = "__import__")]
    DunderImport,
    /// `collections.namedtuple`
    #[strum(serialize = "namedtuple")]
    NamedTuple,
    /// `importlib.import_module`, which returns the submodule.
    ImportModule,
    /// `typing(_extensions).final`
    Final,
    /// `typing(_extensions).disjoint_base`
    DisjointBase,
    /// [`typing(_extensions).no_type_check`](https://typing.python.org/en/latest/spec/directives.html#no-type-check)
    NoTypeCheck,
    /// `typing(_extensions).type_check_only`
    TypeCheckOnly,

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
    /// `dataclasses.field`
    Field,

    /// `pydantic.fields.Field`
    #[strum(serialize = "Field")]
    PydanticField,
    /// `pydantic.functional_validators.field_validator`
    #[strum(serialize = "field_validator")]
    PydanticFieldValidator,

    /// `_pytest.fixtures.fixture`
    #[strum(serialize = "fixture")]
    PytestFixture,
    /// `_pytest.fixtures.yield_fixture`
    #[strum(serialize = "yield_fixture")]
    PytestYieldFixture,

    /// `functools.total_ordering`
    TotalOrdering,

    /// `inspect.getattr_static`
    GetattrStatic,

    /// `ty_extensions.static_assert`
    StaticAssert,
    /// `ty_extensions._internal.is_equivalent_to`
    IsEquivalentTo,
    /// `ty_extensions._internal.is_subtype_of`
    IsSubtypeOf,
    /// `ty_extensions._internal.is_assignable_to`
    IsAssignableTo,
    /// `ty_extensions._internal.is_constraint_set_assignable_to`
    IsConstraintSetAssignableTo,
    /// `ty_extensions._internal.is_disjoint_from`
    IsDisjointFrom,
    /// `ty_extensions._internal.is_singleton`
    IsSingleton,
    /// `ty_extensions._internal.generic_context`
    GenericContext,
    /// `ty_extensions._internal.into_callable`
    IntoCallable,
    /// `ty_extensions._internal.into_regular_callable`
    IntoRegularCallable,
    /// `ty_extensions._internal.dunder_all_names`
    DunderAllNames,
    /// `ty_extensions._internal.enum_members`
    EnumMembers,
    /// `ty_extensions._internal.all_members`
    AllMembers,
    /// `ty_extensions._internal.has_member`
    HasMember,
    /// `ty_extensions._internal.reveal_protocol_interface`
    RevealProtocolInterface,
    /// `ty_extensions._internal.reveal_mro`
    RevealMro,
    /// `struct.unpack`
    Unpack,
    /// `types.new_class`
    NewClass,
}

fn call_argument_node<'a>(
    call_expression: &'a ast::ExprCall,
    name: &str,
    position: usize,
) -> Option<ast::AnyNodeRef<'a>> {
    call_expression
        .arguments
        .find_argument(name, position)
        .map(|argument| match argument {
            ast::ArgOrKeyword::Arg(expr) => ast::AnyNodeRef::from(expr),
            ast::ArgOrKeyword::Keyword(keyword) => ast::AnyNodeRef::from(keyword),
        })
}

pub(in crate::types) trait KnownFunctionEffects<'db>:
    identity_sealed::Sealed
{
    type Error;

    async fn checkpoint(&self, name: &str) -> Result<(), Self::Error>;

    async fn known_module(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<Option<KnownModule>, Self::Error>;
}

impl<'db> KnownFunctionEffects<'db> for LegacyFunctionIdentityEffects {
    type Error = Infallible;

    async fn checkpoint(&self, _name: &str) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn known_module(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<Option<KnownModule>, Self::Error> {
        Ok(
            file_to_module(db, definition.program_file(db).resolver_file(db))
                .and_then(|module| module.known(db)),
        )
    }
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
        legacy_inline(Self::try_from_definition_and_name_with(
            db,
            definition,
            name,
            &LegacyFunctionIdentityEffects,
        ))
    }

    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn classification_work(name: &str) -> Option<usize> {
        // Each variant has one spelling. Include the compatibility spelling and module test.
        name.len()
            .checked_add(1)?
            .checked_mul(<Self as strum::EnumCount>::COUNT.checked_add(1)?)?
            .checked_add(1)
    }

    pub(in crate::types) async fn try_from_definition_and_name_with<
        'db,
        E: KnownFunctionEffects<'db>,
    >(
        db: &'db dyn Db,
        definition: Definition<'db>,
        name: &str,
        effects: &E,
    ) -> Result<Option<Self>, E::Error> {
        effects.checkpoint(name).await?;
        // Special case: `__dataclass_transform__` is recognized as `DataclassTransform`
        // regardless of module, for backwards compatibility with earlier versions of the
        // `dataclass_transform` specification. This matches pyright's behavior:
        // https://github.com/microsoft/pyright/blob/1.1.396/packages/pyright-internal/src/analyzer/dataClasses.ts#L1024-L1033
        if name == "__dataclass_transform__" {
            return Ok(Some(Self::DataclassTransform));
        }

        let Ok(candidate) = Self::from_str(name) else {
            return Ok(None);
        };
        let Some(module) = effects.known_module(db, definition).await? else {
            return Ok(None);
        };
        Ok(candidate.check_module(module).then_some(candidate))
    }

    /// Return `true` if `self` is defined in `module`
    const fn check_module(self, module: KnownModule) -> bool {
        match self {
            Self::IsInstance
            | Self::IsSubclass
            | Self::HasAttr
            | Self::Len
            | Self::Repr
            | Self::DunderImport => module.is_builtins(),
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
            | Self::DisjointBase
            | Self::NoTypeCheck => {
                matches!(module, KnownModule::Typing | KnownModule::TypingExtensions)
            }
            Self::AbstractMethod => {
                matches!(module, KnownModule::Abc)
            }
            Self::Dataclass | Self::Field => {
                matches!(module, KnownModule::Dataclasses)
            }
            Self::PydanticField => matches!(module, KnownModule::PydanticFields),
            Self::PydanticFieldValidator => {
                matches!(module, KnownModule::PydanticFunctionalValidators)
            }
            Self::PytestFixture | Self::PytestYieldFixture => {
                matches!(module, KnownModule::PytestFixtures)
            }
            Self::TotalOrdering => module.is_functools(),
            Self::GetattrStatic => module.is_inspect(),
            Self::StaticAssert => module.is_ty_extensions(),
            Self::IsAssignableTo
            | Self::IsConstraintSetAssignableTo
            | Self::IsDisjointFrom
            | Self::IsEquivalentTo
            | Self::IsSingleton
            | Self::IsSubtypeOf
            | Self::GenericContext
            | Self::IntoCallable
            | Self::IntoRegularCallable
            | Self::DunderAllNames
            | Self::EnumMembers
            | Self::HasMember
            | Self::RevealProtocolInterface
            | Self::RevealMro
            | Self::AllMembers => module.is_ty_extensions_internal(),
            Self::ImportModule => module.is_importlib(),
            Self::Unpack => {
                matches!(module, KnownModule::Struct)
            }
            Self::NewClass => {
                matches!(module, KnownModule::Types)
            }

            Self::TypeCheckOnly => matches!(module, KnownModule::Typing),
            Self::NamedTuple => matches!(module, KnownModule::Collections),
        }
    }

    /// Selects any function-specific processing to run after argument binding succeeds.
    pub(in crate::types) const fn call_check(self) -> Option<KnownFunctionCallCheck> {
        match self {
            Self::RevealType => Some(KnownFunctionCallCheck::RevealType),
            Self::HasMember => Some(KnownFunctionCallCheck::HasMember),
            Self::AssertType => Some(KnownFunctionCallCheck::AssertType),
            Self::AssertNever => Some(KnownFunctionCallCheck::AssertNever),
            Self::StaticAssert => Some(KnownFunctionCallCheck::StaticAssert),
            Self::Cast => Some(KnownFunctionCallCheck::Cast),
            Self::GetProtocolMembers => Some(KnownFunctionCallCheck::GetProtocolMembers),
            Self::RevealProtocolInterface => Some(KnownFunctionCallCheck::RevealProtocolInterface),
            Self::RevealMro => Some(KnownFunctionCallCheck::RevealMro),
            Self::IsInstance => Some(KnownFunctionCallCheck::IsInstance),
            Self::IsSubclass => Some(KnownFunctionCallCheck::IsSubclass),
            Self::DunderImport => Some(KnownFunctionCallCheck::DunderImport),
            Self::ImportModule => Some(KnownFunctionCallCheck::ImportModule),
            Self::TotalOrdering => Some(KnownFunctionCallCheck::TotalOrdering),
            Self::HasAttr
            | Self::Len
            | Self::Repr
            | Self::NamedTuple
            | Self::Final
            | Self::DisjointBase
            | Self::NoTypeCheck
            | Self::TypeCheckOnly
            | Self::Overload
            | Self::Override
            | Self::IsProtocol
            | Self::RuntimeCheckable
            | Self::DataclassTransform
            | Self::AbstractMethod
            | Self::Dataclass
            | Self::Field
            | Self::PydanticField
            | Self::PydanticFieldValidator
            | Self::PytestFixture
            | Self::PytestYieldFixture
            | Self::GetattrStatic
            | Self::IsEquivalentTo
            | Self::IsSubtypeOf
            | Self::IsAssignableTo
            | Self::IsConstraintSetAssignableTo
            | Self::IsDisjointFrom
            | Self::IsSingleton
            | Self::GenericContext
            | Self::IntoCallable
            | Self::IntoRegularCallable
            | Self::DunderAllNames
            | Self::EnumMembers
            | Self::AllMembers
            | Self::Unpack
            | Self::NewClass => None,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        self.into()
    }
}

/// Function-specific processing that can emit diagnostics or refine a bound call's return type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::types) enum KnownFunctionCallCheck {
    RevealType,
    HasMember,
    AssertType,
    AssertNever,
    StaticAssert,
    Cast,
    GetProtocolMembers,
    RevealProtocolInterface,
    RevealMro,
    IsInstance,
    IsSubclass,
    DunderImport,
    ImportModule,
    TotalOrdering,
}

impl KnownFunctionCallCheck {
    /// Emit any function-specific diagnostics and update the binding's return type as needed.
    pub(in crate::types) fn check_call<'db>(
        self,
        context: &InferContext<'db, '_>,
        overload: &mut Binding<'db>,
        call_arguments: &CallArguments<'_, 'db>,
        call_expression: &ast::ExprCall,
        caller_semantic_index: &SemanticIndex<'db>,
    ) {
        let db = context.db();
        let parameter_types = overload.parameter_types();

        match self {
            Self::RevealType => {
                let env = context.program_environment();
                let revealed_type = overload
                    .arguments_for_parameter(call_arguments, 0)
                    .fold(UnionBuilder::new(db, env), |builder, (_, ty)| {
                        builder.add(ty)
                    })
                    .build();
                report_revealed_type(
                    context,
                    revealed_type,
                    call_argument_node(call_expression, "obj", 0)
                        .unwrap_or_else(|| ast::AnyNodeRef::from(call_expression)),
                );
            }

            Self::HasMember => {
                let [Some(ty), Some(Type::LiteralValue(literal))] = parameter_types else {
                    return;
                };
                let Some(member) = literal.as_string() else {
                    return;
                };
                let env = context.program_environment();
                let ty_members = all_members(db, env, *ty);
                overload.set_return_type(Type::bool_literal(
                    ty_members.iter().any(|m| m.name == member.value(db)),
                ));
            }

            Self::AssertType => {
                let [Some(actual_ty), Some(asserted_ty)] = parameter_types else {
                    return;
                };
                let env = context.program_environment();
                let asserted_ty = asserted_ty.project_type_form(db, env);
                if actual_ty.is_equivalent_to(db, env, asserted_ty) {
                    return;
                }
                let diagnostic = if actual_ty.is_spellable(db)
                    || !actual_ty.is_subtype_of(db, env, asserted_ty)
                {
                    &TYPE_ASSERTION_FAILURE
                } else {
                    &ASSERT_TYPE_UNSPELLABLE_SUBTYPE
                };
                if let Some(builder) = context.report_lint(diagnostic, call_expression) {
                    let settings = DisplaySettings::from_possibly_ambiguous_types(
                        db,
                        env,
                        [*actual_ty, asserted_ty],
                    );
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Argument does not have asserted type `{}`",
                        asserted_ty.display_with(db, env, settings.clone()),
                    ));

                    diagnostic.annotate(
                        Annotation::secondary(
                            context.span(
                                call_argument_node(call_expression, "val", 0)
                                    .unwrap_or_else(|| ast::AnyNodeRef::from(call_expression)),
                            ),
                        )
                        .message(format_args!(
                            "Inferred type is `{}`",
                            actual_ty.display_with(db, env, settings.clone())
                        )),
                    );

                    if actual_ty.is_subtype_of(db, env, asserted_ty) {
                        diagnostic.info(format_args!(
                            "`{inferred_type}` is a subtype of `{asserted_type}`, but they are not equivalent",
                            asserted_type = asserted_ty.display_with(db, env, settings.clone()),
                            inferred_type = actual_ty.display_with(db, env, settings.clone()),
                        ));
                    } else {
                        diagnostic.info(format_args!(
                            "`{asserted_type}` and `{inferred_type}` are not equivalent types",
                            asserted_type = asserted_ty.display_with(db, env, settings.clone()),
                            inferred_type = actual_ty.display_with(db, env, settings.clone()),
                        ));
                    }

                    diagnostic.set_concise_message(format_args!(
                        "Type `{}` does not match asserted type `{}`",
                        actual_ty.display_with(db, env, settings.clone()),
                        asserted_ty.display_with(db, env, settings),
                    ));
                }
            }

            Self::AssertNever => {
                let [Some(actual_ty)] = parameter_types else {
                    return;
                };
                let env = context.program_environment();
                if actual_ty.is_equivalent_to(db, env, Type::Never) {
                    return;
                }
                if let Some(builder) = context.report_lint(&TYPE_ASSERTION_FAILURE, call_expression)
                {
                    let mut diagnostic =
                        builder.into_diagnostic("Argument does not have asserted type `Never`");
                    diagnostic.annotate(
                        Annotation::secondary(
                            context.span(
                                call_argument_node(call_expression, "arg", 0)
                                    .unwrap_or_else(|| ast::AnyNodeRef::from(call_expression)),
                            ),
                        )
                        .message(format_args!(
                            "Inferred type of argument is `{}`",
                            actual_ty.display(db, env)
                        )),
                    );
                    diagnostic.info(format_args!(
                        "`Never` and `{inferred_type}` are not equivalent types",
                        inferred_type = actual_ty.display(db, env),
                    ));

                    diagnostic.set_concise_message(format_args!(
                        "Type `{}` is not equivalent to `Never`",
                        actual_ty.display(db, env),
                    ));
                }
            }

            Self::StaticAssert => {
                let [Some(parameter_ty), message] = parameter_types else {
                    return;
                };
                let env = context.program_environment();
                let truthiness = match parameter_ty.try_bool(db, env) {
                    Ok(truthiness) => truthiness,
                    Err(err) => {
                        err.report_diagnostic(
                            context,
                            call_argument_node(call_expression, "condition", 0)
                                .unwrap_or_else(|| ast::AnyNodeRef::from(call_expression)),
                        );

                        return;
                    }
                };

                if let Some(builder) = context.report_lint(&STATIC_ASSERT_ERROR, call_expression) {
                    if truthiness.is_always_true() {
                        return;
                    }
                    let mut diagnostic = if let Some(message) = message
                        .and_then(Type::as_string_literal)
                        .map(|s| s.value(db))
                    {
                        builder.into_diagnostic(format_args!("Static assertion error: {message}"))
                    } else if *parameter_ty == Type::bool_literal(false) {
                        builder.into_diagnostic(
                            "Static assertion error: argument evaluates to `False`",
                        )
                    } else if truthiness.is_always_false() {
                        builder.into_diagnostic(format_args!(
                            "Static assertion error: argument of type `{parameter_ty}` \
                            is always falsy",
                            parameter_ty = parameter_ty.display(db, env)
                        ))
                    } else {
                        builder.into_diagnostic(format_args!(
                            "Static assertion error: argument of type `{parameter_ty}` \
                            has an ambiguous static truthiness",
                            parameter_ty = parameter_ty.display(db, env)
                        ))
                    };
                    if let Some(condition) = call_argument_node(call_expression, "condition", 0) {
                        diagnostic.annotate(
                            Annotation::secondary(context.span(condition)).message(format_args!(
                                "Inferred type of argument is `{}`",
                                parameter_ty.display(db, env)
                            )),
                        );
                    }
                }
            }

            Self::Cast => {
                let [Some(casted_type), Some(source_type)] = parameter_types else {
                    return;
                };
                let env = context.program_environment();
                let casted_type = casted_type.project_type_form(db, env);
                if source_type.is_equivalent_to(db, env, casted_type)
                    && non_any_dynamic_content(db, env, *source_type).is_absent()
                    && non_any_dynamic_content(db, env, casted_type).is_absent()
                {
                    if let Some(builder) = context.report_lint(&REDUNDANT_CAST, call_expression) {
                        let source_display = source_type.display(db, env).to_string();
                        let casted_display = casted_type.display(db, env).to_string();
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Value is already of type `{casted_display}`",
                        ));
                        if source_display != casted_display {
                            diagnostic.info(format_args!(
                                "`{casted_display}` is equivalent to `{source_display}`",
                            ));
                        }
                        if let Some(value) = call_expression.arguments.find_argument_value("val", 1)
                        {
                            let source = source_text(db, context.file());
                            let covering = covering_node(
                                context.module().syntax().into(),
                                call_expression.range(),
                            );
                            let replacement = unwrapped_call_argument(
                                call_expression,
                                value,
                                covering.parent(),
                                context.module().tokens(),
                                &source,
                            );
                            diagnostic.help("Remove the redundant `cast`");
                            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                                replacement,
                                call_expression.range(),
                            )));
                        }
                    }
                } else if context.is_lint_enabled(&DISJOINT_CAST)
                    && !context.file().is_stub(db)
                    && !caller_semantic_index.is_in_type_checking_block(
                        context.scope().file_scope_id(db),
                        call_expression.range(),
                    )
                    && source_type.is_disjoint_from(db, env, casted_type)
                    && !casted_type.is_equivalent_to(db, env, Type::Never)
                    && !source_type.is_equivalent_to(db, env, Type::Never)
                    && let Some(builder) = context.report_lint(&DISJOINT_CAST, call_expression)
                {
                    let types = [*source_type, casted_type];
                    let settings = DisplaySettings::from_possibly_ambiguous_types(db, env, types);
                    let source_display = source_type.display_with(db, env, settings.clone());
                    let casted_display = casted_type.display_with(db, env, settings.clone());
                    let mut diagnostic = builder.into_diagnostic("Cast to a disjoint type");
                    diagnostic.set_concise_message(format_args!(
                        "Cast from `{source_display}` to disjoint type `{casted_display}`",
                    ));
                    if let Some(arg) = call_expression.arguments.find_argument_value("typ", 0) {
                        diagnostic.annotate(
                            context
                                .secondary(arg)
                                .message("Disjoint from the inferred type"),
                        );
                    }
                    if let Some(arg) = call_expression.arguments.find_argument_value("val", 1) {
                        diagnostic.annotate(
                            context
                                .secondary(arg)
                                .message(format_args!("Inferred as `{source_display}`")),
                        );
                    }

                    // deduplicate definitions before attaching a subdiagnostic to each definition,
                    // or we'd have multiple subdiagnostics pointing to a single definition
                    // if the two types are specializations of the same generic class.
                    let definitions: FxIndexMap<Definition<'db>, String> = types
                        .into_iter()
                        .filter_map(|ty| ty.definition(db, env))
                        .filter_map(|definition| definition.definition())
                        .filter_map(|definition| Some((definition, definition.name(db)?)))
                        .collect();

                    for (definition, name) in definitions {
                        let file = definition.python_file(db);
                        let module = parsed_module(db, file).load(db);
                        let mut range = definition.focus_range(db, &module);
                        if let DefinitionKind::Class(class) = definition.kind(db) {
                            let definition_types = infer_definition_types(db, definition);
                            if let Some(decorator) =
                                class.node(&module).decorator_list.iter().find(|decorator| {
                                    definition_types
                                        .expression_type(&decorator.expression)
                                        .as_function_literal()
                                        .is_some_and(|func| func.is_known(db, KnownFunction::Final))
                                })
                            {
                                range = range.cover_range(decorator.range());
                            }
                        }
                        diagnostic.annotate(
                            Annotation::secondary(Span::from(range))
                                .message(format_args!("`{name}` defined here")),
                        );
                    }

                    if casted_type.is_protocol_instance() {
                        if source_type.is_protocol_instance() {
                            diagnostic.info(format_args!(
                                "protocol `{casted_display}` is disjoint \
                                from protocol `{source_display}`"
                            ));
                        } else {
                            diagnostic.info(format_args!(
                                "protocol `{casted_display}` is disjoint \
                                from `{source_display}`"
                            ));
                        }
                    } else if source_type.is_protocol_instance() {
                        diagnostic.info(format_args!(
                            "`{casted_display}` is disjoint \
                            from protocol `{source_display}`"
                        ));
                    } else {
                        diagnostic.info(format_args!(
                            "`{casted_display}` is disjoint from `{source_display}`"
                        ));
                    }

                    source_type
                        .disjointness_error_context(db, env, casted_type)
                        .attach_to(db, env, &mut diagnostic);
                }
            }

            Self::GetProtocolMembers => {
                let [Some(Type::ClassLiteral(class))] = parameter_types else {
                    return;
                };
                if class.is_protocol(context.db()) {
                    return;
                }
                report_bad_argument_to_get_protocol_members(context, call_expression, *class);
            }

            Self::RevealProtocolInterface => {
                let [Some(param_type)] = parameter_types else {
                    return;
                };
                let env = context.program_environment();
                let Some(protocol_class) = param_type
                    .to_class_type(db)
                    .and_then(|class| class.into_protocol_class(db))
                else {
                    report_bad_argument_to_protocol_interface(
                        context,
                        call_expression,
                        *param_type,
                    );
                    return;
                };
                if let Some(builder) =
                    context.report_diagnostic(DiagnosticId::RevealedType, Severity::Info)
                {
                    let mut diag = builder.into_diagnostic("Revealed protocol interface");
                    let span = context.span(
                        call_argument_node(call_expression, "protocol", 0)
                            .unwrap_or_else(|| ast::AnyNodeRef::from(call_expression)),
                    );
                    diag.annotate(Annotation::primary(span).message(format_args!(
                        "`{}`",
                        protocol_class.interface(db).display(db, env)
                    )));
                }
            }

            Self::RevealMro => {
                let [Some(param_type)] = parameter_types else {
                    return;
                };
                let mut good_argument = true;
                let classes = match param_type {
                    Type::ClassLiteral(class) => vec![ClassType::NonGeneric(*class)],
                    Type::GenericAlias(generic_alias) => vec![ClassType::Generic(*generic_alias)],
                    Type::Union(union) => {
                        let elements = union.elements(db);
                        let mut classes = Vec::with_capacity(elements.len());
                        for element in elements {
                            match element {
                                Type::ClassLiteral(class) => {
                                    classes.push(ClassType::NonGeneric(*class));
                                }
                                Type::GenericAlias(generic_alias) => {
                                    classes.push(ClassType::Generic(*generic_alias));
                                }
                                _ => {
                                    good_argument = false;
                                    break;
                                }
                            }
                        }
                        classes
                    }
                    _ => {
                        good_argument = false;
                        vec![]
                    }
                };
                if !good_argument {
                    let Some(builder) =
                        context.report_lint(&INVALID_ARGUMENT_TYPE, call_expression)
                    else {
                        return;
                    };
                    let mut diagnostic =
                        builder.into_diagnostic("Invalid argument to `reveal_mro`");
                    diagnostic.set_primary_annotation_message(format_args!(
                        "Can only pass a class object, generic alias or a union thereof"
                    ));
                    return;
                }
                if let Some(builder) =
                    context.report_diagnostic(DiagnosticId::RevealedType, Severity::Info)
                {
                    let env = context.program_environment();
                    let mut diag = builder.into_diagnostic("Revealed MRO");
                    let span = context.span(
                        call_argument_node(call_expression, "cls", 0)
                            .unwrap_or_else(|| ast::AnyNodeRef::from(call_expression)),
                    );
                    let mut message = String::new();
                    let display_settings = DisplaySettings::from_possibly_ambiguous_types(
                        db,
                        env,
                        classes
                            .iter()
                            .flat_map(|class| class.iter_mro(db))
                            .filter_map(ClassBase::into_class),
                    );
                    for (i, class) in classes.iter().enumerate() {
                        message.push('(');
                        for class in class.iter_mro(db) {
                            message.push_str(
                                &class
                                    .display_with(db, env, display_settings.clone())
                                    .to_string(),
                            );
                            // Omit the comma for the last element (which is always `object`)
                            if class
                                .into_class()
                                .is_none_or(|base| !base.is_object(context.db()))
                            {
                                message.push_str(", ");
                            }
                        }
                        // If the last element was also the first element
                        // (i.e., it's a length-1 tuple -- which can only happen if we're revealing
                        // the MRO for `object` itself), add a trailing comma so that it's still a
                        // valid tuple display.
                        if class.is_object(db) {
                            message.push(',');
                        }
                        message.push(')');
                        if i < classes.len() - 1 {
                            message.push_str(" | ");
                        }
                    }
                    diag.annotate(Annotation::primary(span).message(message));
                }
            }

            Self::IsInstance | Self::IsSubclass => {
                let [Some(first_arg), Some(second_argument)] = parameter_types else {
                    return;
                };

                check_classinfo_in_isinstance(
                    db,
                    context,
                    call_expression,
                    if self == Self::IsInstance {
                        KnownFunction::IsInstance
                    } else {
                        KnownFunction::IsSubclass
                    },
                    *second_argument,
                    call_expression.arguments.args.get(1),
                );

                if self == Self::IsInstance {
                    let env = context.program_environment();
                    let truthiness = match second_argument {
                        Type::ClassLiteral(class) => {
                            is_instance_truthiness(db, env, *first_arg, *class)
                        }
                        Type::SpecialForm(
                            SpecialFormType::TypingCallable
                            | SpecialFormType::CollectionsAbcCallable,
                        ) => {
                            let callable_top = Type::Callable(CallableType::top(db));
                            if first_arg.is_subtype_of(db, env, callable_top) {
                                Truthiness::AlwaysTrue
                            } else {
                                Truthiness::Ambiguous
                            }
                        }
                        _ if is_instance_tuple_exhaustive(
                            db,
                            env,
                            *first_arg,
                            *second_argument,
                        ) =>
                        {
                            Truthiness::AlwaysTrue
                        }
                        _ => Truthiness::Ambiguous,
                    };
                    overload.set_return_type(Type::from_truthiness(db, env, truthiness));
                }
            }

            known @ (Self::DunderImport | Self::ImportModule) => {
                let [Some(first), rest @ ..] = parameter_types else {
                    return;
                };
                let Some(full_module_name) = first.as_string_literal() else {
                    return;
                };

                if rest.iter().any(Option::is_some) {
                    return;
                }

                let module_name = full_module_name.value(db);

                if known == Self::DunderImport && module_name.contains('.') {
                    // `__import__("collections.abc")` returns the `collections` module.
                    // `importlib.import_module("collections.abc")` returns the `collections.abc` module.
                    // ty doesn't have a way to represent the return type of the former yet.
                    // https://github.com/astral-sh/ruff/pull/19008#discussion_r2173481311
                    return;
                }

                let Some(module_name) = ModuleName::new(module_name) else {
                    return;
                };
                let importing_file = ImportingFile::File(
                    context.file(),
                    context.program_environment().resolver_environment(db),
                );
                let Some(module) = resolve_module(db, importing_file, &module_name) else {
                    return;
                };

                overload.set_return_type(Type::module_literal(db, context.program_file(), module));
            }

            Self::TotalOrdering => {
                // When `total_ordering(cls)` is called as a function (not as a decorator),
                // check that the class defines at least one ordering method.
                let [Some(class_type)] = parameter_types else {
                    return;
                };

                let class = match class_type {
                    Type::ClassLiteral(class) => ClassType::NonGeneric(*class),
                    Type::GenericAlias(generic) => ClassType::Generic(*generic),
                    _ => return,
                };

                if !class.has_ordering_method_in_mro(db) {
                    report_invalid_total_ordering_call(
                        context,
                        class.class_literal(db),
                        call_expression,
                    );
                }
            }
        }
    }
}

/// Emit a `revealed-type` diagnostic for a `reveal_type(...)` call.
pub(super) fn report_revealed_type<'db>(
    context: &InferContext<'db, '_>,
    revealed_type: Type<'db>,
    argument_node: impl Ranged,
) {
    let db = context.db();
    if let Some(builder) = context.report_diagnostic(DiagnosticId::RevealedType, Severity::Info) {
        let env = context.program_environment();
        let mut diag = builder.into_diagnostic("Revealed type");
        diag.annotate(
            Annotation::primary(context.span(argument_node)).message(format_args!(
                "`{}`",
                revealed_type.display(db, env).preserve_long_unions()
            )),
        );
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use strum::IntoEnumIterator;

    use super::*;
    use crate::db::tests::setup_db;
    use crate::place::known_module_symbol;

    #[test]
    fn known_function_roundtrip_from_str() {
        let db = setup_db();

        for function in KnownFunction::iter() {
            let function_name = function.name();

            let module = match function {
                KnownFunction::Len
                | KnownFunction::Repr
                | KnownFunction::IsInstance
                | KnownFunction::HasAttr
                | KnownFunction::IsSubclass
                | KnownFunction::DunderImport => KnownModule::Builtins,

                KnownFunction::AbstractMethod => KnownModule::Abc,

                KnownFunction::Dataclass | KnownFunction::Field => KnownModule::Dataclasses,

                KnownFunction::PydanticField => KnownModule::PydanticFields,
                KnownFunction::PydanticFieldValidator => KnownModule::PydanticFunctionalValidators,
                KnownFunction::PytestFixture | KnownFunction::PytestYieldFixture => {
                    KnownModule::PytestFixtures
                }

                KnownFunction::GetattrStatic => KnownModule::Inspect,

                KnownFunction::Cast
                | KnownFunction::Final
                | KnownFunction::Overload
                | KnownFunction::Override
                | KnownFunction::RevealType
                | KnownFunction::AssertType
                | KnownFunction::AssertNever
                | KnownFunction::IsProtocol
                | KnownFunction::GetProtocolMembers
                | KnownFunction::RuntimeCheckable
                | KnownFunction::DataclassTransform
                | KnownFunction::DisjointBase
                | KnownFunction::NoTypeCheck => KnownModule::TypingExtensions,

                KnownFunction::TypeCheckOnly => KnownModule::Typing,

                KnownFunction::StaticAssert => KnownModule::TyExtensions,

                KnownFunction::IsSingleton
                | KnownFunction::IsSubtypeOf
                | KnownFunction::GenericContext
                | KnownFunction::IntoCallable
                | KnownFunction::IntoRegularCallable
                | KnownFunction::DunderAllNames
                | KnownFunction::EnumMembers
                | KnownFunction::IsDisjointFrom
                | KnownFunction::IsAssignableTo
                | KnownFunction::IsConstraintSetAssignableTo
                | KnownFunction::IsEquivalentTo
                | KnownFunction::HasMember
                | KnownFunction::RevealProtocolInterface
                | KnownFunction::RevealMro
                | KnownFunction::AllMembers => KnownModule::TyExtensionsInternal,

                KnownFunction::ImportModule => KnownModule::ImportLib,
                KnownFunction::NamedTuple => KnownModule::Collections,
                KnownFunction::TotalOrdering => KnownModule::Functools,
                KnownFunction::Unpack => KnownModule::Struct,
                KnownFunction::NewClass => KnownModule::Types,
            };

            if module.is_third_party() {
                continue;
            }

            let function_definition =
                known_module_symbol(&db, &db.program_environment(), module, function_name)
                    .place
                    .expect_type()
                    .expect_function_literal()
                    .definition(&db);

            assert_eq!(
                KnownFunction::try_from_definition_and_name(
                    &db,
                    function_definition,
                    function_name
                ),
                Some(function),
                "The strum `EnumString` implementation appears to be incorrect for `{function_name}`"
            );
        }
    }
}
