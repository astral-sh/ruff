use std::cell::{OnceCell, RefCell};
use std::collections::hash_map;
use std::convert::Infallible;
use std::rc::Rc;

use compact_str::CompactString;
use itertools::Itertools;
use ruff_db::diagnostic::{Annotation, Span, SubDiagnostic, SubDiagnosticSeverity};
use ruff_db::files::File;
use ruff_db::parsed::ParsedModuleRef;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::helpers::is_dotted_name;
use ruff_python_ast::name::Name;
use ruff_python_ast::{
    self as ast, AnyNodeRef, ArgOrKeyword, ArgumentsSourceOrder, ExprContext, HasNodeIndex,
    PythonVersion,
};
use ruff_python_stdlib::builtins::version_builtin_was_added;
use ruff_python_stdlib::typing::as_pep_585_generic;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use strum::IntoEnumIterator;
use ty_module_resolver::{ImportingFile, ModuleName, resolve_module};
use ty_python_core::ast_ids::HasScopedUseId;
use ty_python_core::statement::StatementInner;

use super::{
    CollectionUseConstraints, DeferredAndUndecorated, DefinitionInference,
    DefinitionInferenceExtra, DefinitionTypes, ExpressionInference, ExpressionInferenceExtra,
    FrozenMap, FrozenSet, FrozenValueMap, FunctionDecoratorInference, InferenceRegion,
    OtherDefinitionInferenceExtra, ScopeInference, ScopeInferenceExtra, infer_deferred_types,
    infer_definition_types, infer_expression_types, infer_same_file_expression_type,
    infer_unpack_types,
};
use crate::place::{
    DefinedPlace, Definedness, Place, PlaceAndQualifiers, TypeOrigin, loop_header_reachability,
    module_type_implicit_global_symbol, place_from_bindings_with_reachability_cache,
    typing_extensions_symbol,
};
use crate::place_load::PlaceLoadSource;
use crate::reachability::{ReachabilityEvaluationCache, analyze_condition_expression};
use crate::types::add_inferred_python_version_hint_to_diagnostic;
use crate::types::attribute_write::{AssignmentAttributeMembers, assignment_attribute_members};
use crate::types::call::bind::{ArgumentTypeContext, CallableDescription};
use crate::types::call::{Bindings, CallArguments, CallError, CallErrorKind};
use crate::types::callable::CallableTypeKind;
use crate::types::class::{
    ClassLiteral, CodeGeneratorKind, FrozenDataclassDispatch, MethodDecorator,
};
use crate::types::constraints::{CandidateSolutions, ConstraintSetBuilder, Solutions};
use crate::types::context::InferContext;
use crate::types::dedicated::pydantic;
use crate::types::diagnostic::{
    self, CALL_NON_CALLABLE, CYCLIC_TYPE_ALIAS_DEFINITION, GeneratorMismatchKind,
    INEFFECTIVE_FINAL, INVALID_ARGUMENT_TYPE, INVALID_ASSIGNMENT,
    INVALID_LEGACY_TYPE_VARIABLE, INVALID_NEWTYPE, INVALID_PARAMSPEC, INVALID_TYPE_ALIAS_TYPE,
    INVALID_TYPE_FORM, INVALID_TYPE_VARIABLE_DEFAULT, POSSIBLY_MISSING_IMPLICIT_CALL,
    TypeCheckDiagnostics, UNRESOLVED_GLOBAL, UNRESOLVED_REFERENCE,
    UNSOUND_YIELD, UNSUPPORTED_OPERATOR, YieldKind, autofix_with_notimplementederror,
    report_attempted_instantiation_of_abstract_class, report_attempted_protocol_instantiation,
    report_bad_dunder_delattr_call, report_bad_dunder_delete_call, report_call_to_abstract_method,
    report_cannot_pop_required_field_on_typed_dict,
    report_invalid_class_match_pattern, report_invalid_exception_caught,
    report_invalid_exception_cause, report_invalid_exception_raised,
    report_invalid_exception_tuple_caught, report_invalid_generator_yield_type,
    report_invalid_key_on_typed_dict, report_invalid_match_args_type,
    report_match_pattern_against_non_runtime_checkable_protocol,
    report_match_pattern_against_typed_dict, report_mismatched_type_name,
    report_too_many_positional_patterns_for_class_pattern, report_undefined_reveal,
    report_unsound_yield, report_unsupported_augmented_assignment,
};
use crate::types::function::{
    FunctionType, KnownFunction, OverloadLiteral, report_revealed_type,
    same_module_uncached_raw_signature,
};
use crate::types::generics::{
    GenericContext, Specialization, SpecializationBuilder, bind_typevar, enclosing_binding_contexts,
};
use crate::types::infer::builder::binary_expressions::BinaryInferenceState;
use crate::types::infer::builder::function::decorators::{
    FunctionDecoratorClassification, FunctionDecoratorFacts, OrdinaryFunctionDecoratorEffects,
    function_decorators_sync,
};
use crate::types::infer::builder::named_tuple::NamedTupleKind;
use crate::types::infer::builder::paramspec_validation::validate_paramspec_components;
use crate::types::infer::{
    StatementInference, StatementInferenceInner, StatementInferenceInnerExtra, TypeAndRange,
    TypeExpressionFlags, infer_statement_types, nearest_enclosing_class,
    nearest_enclosing_function, original_class_type,
};
use crate::types::match_pattern::{ClassPatternPositionalResult, class_pattern_positional_result};
use crate::types::narrow::NarrowingEvaluatorExtension;
use crate::types::narrow::pattern_success_types;
use crate::types::newtype::NewType;
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::signatures::{CallableSignature, ReturnCallableTypeVarScope};
use crate::types::special_form::TypeQualifier;
use crate::types::tuple::promotion::TupleSizePromotionConstraints;
use crate::types::tuple::{TupleSpecBuilder, TupleType};
use crate::types::type_alias::{ManualPEP695TypeAliasType, PEP695TypeAliasType};
use crate::types::typed_dict::{TypedDictAssignmentKind, TypedDictKeyAssignment};
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarInstance};
use crate::types::unpacker::{UnpackResult, fixed_sequence_elements};
use crate::types::{
    BindingContext, BoundTypeVarInstance, CallDunderError, CallableBinding, CallableType,
    ClassType, DynamicType, GeneratorTypeMode, InferenceFlags, InternedConstraintSet,
    IntersectionType, KnownBoundMethodType, KnownClass, KnownInstanceType, KnownUnion,
    LiteralValueType, LiteralValueTypeKind, MemberLookupPolicy, ParamSpecAttrKind, Parameter,
    Parameters, ProgramEnvironment, PropertyDeprecations, SentinelInstance, Signature,
    SpecialFormType, Type, TypeAliasType, TypeAndQualifiers, TypeContext, TypeQualifiers,
    TypeVarBoundOrConstraints, TypeVarVariance, TypingModule, UnionAccumulator, UnionBuilder,
    UnionType, any_over_type, binding_type, extract_fixed_length_iterable_element_types,
    infer_complete_scope_types, infer_scope_types,
};
use crate::{AnalysisSettings, Db, FxIndexSet, FxOrderSet, SemanticModel};
use ty_python_core::definition::{
    AnnotatedAssignmentDefinitionKind, ComprehensionDefinitionKind, Definition, DefinitionKind,
    DefinitionNodeKey, DefinitionState, ExceptHandlerDefinitionKind, ForStmtDefinitionKind,
    LambdaParameterDefinitionNodeKind, LoopHeaderDefinitionKind, NestedBindingExecution,
    NestedBindingsDefinitionKind, ParameterDefinitionNodeKind, TargetKind, WithItemDefinitionKind,
};
use ty_python_core::expression::{Expression, ExpressionKind};
use ty_python_core::narrowing_constraints::ConstraintKey;
use ty_python_core::node_key::NodeKey;
use ty_python_core::place::{PlaceExpr, PlaceExprRef};
use ty_python_core::predicate::PatternPredicate;
use ty_python_core::scope::{FileScopeId, NodeWithScopeKind, NodeWithScopeRef, ScopeId, ScopeKind};
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{
    EvaluationMode, ProgramFile, SemanticIndex, Truthiness, unpack::UnpackPosition,
};
use ty_python_core::{ExpressionNodeKey, Statement};

pub(in crate::types::infer) mod annotated_assignment;
#[cfg(feature = "experimental-analysis")]
pub use annotated_assignment::AnnotatedAssignmentOperation;
mod annotation_expression;
mod applicable_constraints;
pub(in crate::types::infer) mod assignment;
mod assignment_validation;
mod declaration_binding;
mod attribute;
#[cfg(feature = "experimental-analysis")]
pub use attribute::AttributeOperation;
mod attribute_assignment;
mod awaitable;
mod binary_expressions;
mod chained_comparison;
#[cfg(feature = "experimental-analysis")]
pub use chained_comparison::ChainedComparisonOperation;
#[cfg(test)]
pub(in crate::types::infer) use chained_comparison::{guarded_observations, guarded_type_with};
mod class;
mod deferred;
#[cfg(feature = "experimental-analysis")]
pub(in crate::types::infer) use class::source_effects::ClassIdentity;
mod dict;
mod dynamic_class;
mod enum_call;
pub(in crate::types::infer) mod expression_search;
mod final_attribute;
mod function;
#[cfg(feature = "experimental-analysis")]
pub use function::application::DecoratorApplicationOperation;
#[cfg(feature = "experimental-analysis")]
pub(in crate::types::infer) use function::source_effects::OverloadIdentity;
pub(in crate::types::infer) mod implicit_place;
mod imports;
mod local;
#[cfg(test)]
mod local_frame_probe;
mod named_tuple;
mod new_class;
mod number_literal;
mod paramspec_validation;
mod post_inference;
mod range;
mod redundant_conditions;
mod scope;
pub(in crate::types::infer) mod source_binding;
mod source_declaration;
pub(in crate::types::infer) mod source_definition;
pub(in crate::types::infer) mod source_expression;
pub(in crate::types::infer) mod source_function_body;
pub(in crate::types::infer) mod source_merge;
pub(in crate::types::infer) mod source_parameter;
pub(in crate::types::infer) mod source_return;
pub(in crate::types::infer) mod source_statement;
mod string_literal;
mod subscript;
mod tuple_expression;
mod type_call;
mod type_expression;
mod type_form;
mod typed_dict;
mod typeguard;
mod typevar;

/// A helper to track if we already know that declared and inferred types are the same.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DeclaredAndInferredType<'db> {
    /// We know that both the declared and inferred types are the same.
    AreTheSame(TypeAndQualifiers<'db>),
    /// Declared and inferred types might be different, we need to check assignability.
    MightBeDifferent {
        declared_ty: TypeAndQualifiers<'db>,
        inferred_ty: Type<'db>,
    },
}

impl<'db> DeclaredAndInferredType<'db> {
    fn are_the_same_type(ty: Type<'db>) -> Self {
        Self::AreTheSame(TypeAndQualifiers::new(
            ty,
            TypeOrigin::Inferred,
            TypeQualifiers::empty(),
        ))
    }
}

/// We currently store one dataclass field-specifiers inline, because that covers standard
/// dataclasses. attrs uses 2 specifiers, pydantic and strawberry use 3 specifiers. SQLAlchemy
/// uses 7 field specifiers. We could probably store more inline if this turns out to be a
/// performance problem. For now, we optimize for memory usage.
const NUM_FIELD_SPECIFIERS_INLINE: usize = 1;

/// Builder to infer all types in a region.
///
/// A builder is used by creating it with [`new()`](TypeInferenceBuilder::new), and then calling
/// [`finish_expression()`](TypeInferenceBuilder::finish_expression), [`finish_definition()`](TypeInferenceBuilder::finish_definition), or [`finish_scope()`](TypeInferenceBuilder::finish_scope) on it, which returns
/// type inference result.
///
/// There are a few different kinds of methods in the type inference builder, and the naming
/// distinctions are a bit subtle.
///
/// The `finish` methods infer types using [`infer_region`](TypeInferenceBuilder::infer_region),
/// or one of its region-specific delegates: [`infer_region_scope`](TypeInferenceBuilder::infer_region_scope),
/// [`infer_region_definition`](TypeInferenceBuilder::infer_region_definition),
/// [`infer_region_function_decorators`](TypeInferenceBuilder::infer_region_function_decorators),
/// [`infer_region_deferred`](TypeInferenceBuilder::infer_region_deferred), or
/// [`infer_region_expression`](TypeInferenceBuilder::infer_region_expression), depending which
/// kind of [`InferenceRegion`] we are inferring types for.
///
/// Scope inference starts with the scope body, walking all statements and expressions and
/// recording the types of each expression in the inference result. Most of the methods
/// here (with names like `infer_*_statement` or `infer_*_expression` or some other node kind) take
/// a single AST node and are called as part of this AST visit.
///
/// When the visit encounters a node which creates a [`Definition`], we look up the definition in
/// the semantic index and call the [`infer_definition_types()`] query on it, which creates another
/// [`TypeInferenceBuilder`] just for that definition, and we merge the returned inference result
/// into the one we are currently building for the entire scope. Using the query in this way
/// ensures that if we first infer types for some scattered definitions in a scope, and later for
/// the entire scope, we don't re-infer any types, we reuse the cached inference for those
/// definitions and their sub-expressions.
///
/// Functions with a name like `infer_*_definition` take both a node and a [`Definition`], and are
/// called by [`infer_region_definition`](TypeInferenceBuilder::infer_region_definition).
///
/// So for example we have both
/// [`infer_function_definition_statement`](TypeInferenceBuilder::infer_function_definition_statement),
/// which takes just the function AST node, and
/// [`infer_function_definition`](TypeInferenceBuilder::infer_function_definition), which takes
/// both the node and the [`Definition`] id. The former is called as part of walking the AST, and
/// it just looks up the [`Definition`] for that function in the semantic index and calls
/// [`infer_definition_types()`] on it, which will create a new [`TypeInferenceBuilder`] with
/// [`InferenceRegion::Definition`], and in that builder
/// [`infer_region_definition`](TypeInferenceBuilder::infer_region_definition) will call
/// [`infer_function_definition`](TypeInferenceBuilder::infer_function_definition) to actually
/// infer a type for the definition.
///
/// Similarly, when we encounter a standalone-inferable expression (right-hand side of an
/// assignment, type narrowing guard), we use the [`infer_expression_types()`] query to ensure we
/// don't infer its types more than once.
pub(super) struct TypeInferenceBuilder<'db, 'ast> {
    context: InferContext<'db, 'ast>,

    index: &'db SemanticIndex<'db>,
    region: InferenceRegion<'db>,

    /// The types of every expression in this region.
    expressions: FxHashMap<ExpressionNodeKey, Type<'db>>,

    /// Truthiness overrides for evaluating comparison chains directly as conditions.
    /// See [`ExpressionInferenceExtra::comparison_truthiness`] for why these are stored
    /// separately from expression types.
    comparison_truthiness: FxHashMap<ExpressionNodeKey, Truthiness>,

    /// Controlled merges retain a backing bound across removals of stale overrides. The current
    /// map capacity alone can undercount buckets retained after those removals.
    #[cfg(any(test, feature = "experimental-analysis"))]
    source_truthiness_backing: usize,

    /// An expression cache shared across builders during multi-inference.
    expression_cache: Option<Rc<RefCell<ExpressionCache<'db>>>>,

    /// Reachability evaluations reused while inferring this region.
    ///
    /// Most inference regions never evaluate reachability, so allocate the cache lazily. Speculative
    /// builders share an initialized cache with their parent so repeated place lookups performed
    /// during multi-inference can reuse predicate truthiness computed by the parent builder.
    reachability_cache: OnceCell<Rc<ReachabilityEvaluationCache<'db>>>,

    /// Type qualifiers (`Required`, `NotRequired`, etc.) for annotation expressions.
    /// Only populated for expressions that have non-empty qualifiers.
    qualifiers: FxHashMap<ExpressionNodeKey, TypeQualifiers>,

    /// Metadata for type expressions.
    /// Only populated for expressions that have non-empty flags.
    type_expression_flags: FxHashMap<ExpressionNodeKey, TypeExpressionFlags>,

    /// The constraints on any collection initializers that are accessed in this region.
    //
    // TODO: Store projected constraint sets directly here instead of specialized receiver types.
    // Bound-method calls on unconstrained collection initializers can introduce method-local typevars
    // (for example, `list.sort` constrains `T@list` using `SupportsRichComparisonT@sort`). A
    // principled representation would store an owned constraint set over the collection initializer's
    // generic context and existentially quantify away the method-local typevars, so combining
    // `xs.append("x")` with `xs.sort()` yields `str ≤ T ≤ SupportsRichComparison` instead of
    // leaking `SupportsRichComparisonT@sort` into the inferred list element type.
    collection_use_constraints: FxHashMap<Definition<'db>, FxIndexSet<Type<'db>>>,

    /// Expressions that are string annotations
    string_annotations: FxHashSet<ExpressionNodeKey>,

    /// Expected types for expression nodes tracked for IDE completion.
    expected_types: FxHashMap<ExpressionNodeKey, Type<'db>>,

    /// The scope this region is part of.
    scope: ScopeId<'db>,

    // bindings, declarations, and deferred can only exist in definition, or scope contexts.
    /// The types of every binding in this region.
    ///
    /// The list should only contain one entry per binding at most.
    bindings: VecMap<Definition<'db>, Type<'db>>,

    /// The types and type qualifiers of every valid declaration in this region.
    ///
    /// The list should only contain one entry per declaration at most.
    declarations: VecMap<Definition<'db>, TypeAndQualifiers<'db>>,

    /// The definitions with deferred sub-parts.
    ///
    /// The list should only contain one entry per definition.
    deferred: VecSet<Definition<'db>>,

    /// The returned types and their corresponding ranges of the region, if it is a function body.
    return_types_and_ranges: Vec<TypeAndRange<'db>>,

    /// A set of functions that have been defined **and** called in this region.
    ///
    /// This is a set because the same function could be called multiple times in the same region.
    /// This is mainly used in [`post_inference::overloaded_function::check_overloaded_function`] to
    /// check an overloaded function that is shadowed by a function with the same name in this
    /// scope but has been called before. For example:
    ///
    /// ```py
    /// from typing import overload
    ///
    /// @overload
    /// def foo() -> None: ...
    /// @overload
    /// def foo(x: int) -> int: ...
    /// def foo(x: int | None) -> int | None: return x
    ///
    /// foo()  # An overloaded function that was defined in this scope have been called
    ///
    /// def foo(x: int) -> int:
    ///     return x
    /// ```
    ///
    /// To keep the calculation deterministic, we use an `FxIndexSet` whose order is determined by the sequence of insertion calls.
    called_functions: FxIndexSet<FunctionType<'db>>,

    /// Aliases whose type-expression diagnostics are collected once during file checking.
    implicit_aliases: FxIndexSet<Definition<'db>>,

    /// Whether we are in a context that binds unbound typevars.
    typevar_binding_context: Option<Definition<'db>>,

    /// The deferred state of inferring types of certain expressions within the region.
    ///
    /// This is different from [`InferenceRegion::Deferred`] which works on the entire definition
    /// while this is relevant for specific expressions within the region itself and is updated
    /// during the inference process.
    ///
    /// For example, when inferring the types of an annotated assignment, the type of an annotation
    /// expression could be deferred if the file has `from __future__ import annotations` import or
    /// is a stub file but we're still in a non-deferred region.
    deferred_state: DeferredExpressionState,

    /// For decorated function or class definitions, the type before applying decorators.
    undecorated_type: Option<Type<'db>>,

    /// Input types for failed decorator applications, keyed by decorator expression.
    ///
    /// Recheck these calls after inference so formatting diagnostics cannot pull deferred
    /// function defaults into definition inference cycles.
    deferred_decorator_calls: Vec<(ExpressionNodeKey, Type<'db>)>,

    /// The fallback type for missing expressions/bindings/declarations or recursive type inference.
    cycle_recovery: Option<Type<'db>>,

    /// If the inference region refers to a definition, whether synthesized dictionary-key
    /// assignments derived from its right-hand side should be discarded.
    discards_dict_key_assignments: bool,

    /// A list of `dataclass_transform` field specifiers that are "active" (when inferring
    /// the right hand side of an annotated assignment in a class that is a dataclass).
    dataclass_field_specifiers: SmallVec<[Type<'db>; NUM_FIELD_SPECIFIERS_INLINE]>,
}

fn transparent_callable_decorator_result<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    bindings: &Bindings<'db>,
    decorated_ty: Type<'db>,
) -> Option<Type<'db>> {
    let Ok(result) = function::application::transparent_callable_decorator_sync(
        bindings,
        decorated_ty,
        function::application::DecoratorApplicationFacts,
        &function::application::OrdinaryDecoratorApplicationEffects { db, env },
    );
    result
}

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
    /// How big a string do we build before bailing?
    ///
    /// This is a fairly arbitrary number. It should be *far* more than enough
    /// for most use cases, but we can reevaluate it later if useful.
    pub(super) const MAX_STRING_LITERAL_SIZE: usize = 4096;

    /// Creates a new builder for inferring types in a region.
    pub(super) fn new(
        db: &'db dyn Db,
        env: &'ast ProgramEnvironment<'db>,
        region: InferenceRegion<'db>,
        file: File,
        program_file: ProgramFile<'db>,
        index: &'db SemanticIndex<'db>,
        module: &'ast ParsedModuleRef,
    ) -> Self {
        let scope = region.scope(db);
        let context = InferContext::new(db, env, scope, file, program_file, module);
        Self::from_context(context, region, index)
    }

    fn from_context(
        context: InferContext<'db, 'ast>,
        region: InferenceRegion<'db>,
        index: &'db SemanticIndex<'db>,
    ) -> Self {
        let scope = context.scope();
        Self {
            context,
            index,
            region,
            scope,
            return_types_and_ranges: vec![],
            called_functions: FxIndexSet::default(),
            implicit_aliases: FxIndexSet::default(),
            deferred_state: DeferredExpressionState::None,
            expressions: FxHashMap::default(),
            comparison_truthiness: FxHashMap::default(),
            #[cfg(any(test, feature = "experimental-analysis"))]
            source_truthiness_backing: 0,
            expression_cache: None,
            reachability_cache: OnceCell::new(),
            qualifiers: FxHashMap::default(),
            type_expression_flags: FxHashMap::default(),
            collection_use_constraints: FxHashMap::default(),
            string_annotations: FxHashSet::default(),
            expected_types: FxHashMap::default(),
            bindings: VecMap::default(),
            declarations: VecMap::default(),
            typevar_binding_context: None,
            deferred: VecSet::default(),
            undecorated_type: None,
            deferred_decorator_calls: Vec::new(),
            cycle_recovery: None,
            discards_dict_key_assignments: false,
            dataclass_field_specifiers: SmallVec::new(),
        }
    }

    fn reachability_cache(&self) -> &ReachabilityEvaluationCache<'db> {
        if let Some(cache) = self.reachability_cache.get() {
            return cache.as_ref();
        }
        self.reachability_cache_for_scope(self.scope().file_scope_id(self.db()))
    }

    fn reachability_cache_for_scope(
        &self,
        file_scope: FileScopeId,
    ) -> &ReachabilityEvaluationCache<'db> {
        self.reachability_cache
            .get_or_init(|| {
                let scope = self.scope();
                let reachability_constraints = self
                    .index
                    .use_def_map(file_scope)
                    .reachability_constraints();
                Rc::new(ReachabilityEvaluationCache::new(
                    scope,
                    reachability_constraints,
                ))
            })
            .as_ref()
    }

    fn discard_dict_key_assignments_for(&mut self, definition: Definition<'db>) {
        if matches!(self.region, InferenceRegion::Definition(d) if d == definition) {
            self.discards_dict_key_assignments = true;
        }
    }

    fn fallback_type(&self) -> Option<Type<'db>> {
        self.cycle_recovery
    }

    fn recursive_type_expression_definition(&self) -> Option<Definition<'db>> {
        self.typevar_binding_context.or(match self.region {
            InferenceRegion::Definition(definition)
            | InferenceRegion::FunctionDefaults(definition)
            | InferenceRegion::Deferred(definition) => Some(definition),
            InferenceRegion::Statement(_)
            | InferenceRegion::Expression(_, _)
            | InferenceRegion::FunctionDecorators(_)
            | InferenceRegion::Scope(_, _) => None,
        })
    }

    fn extend_cycle_recovery(&mut self, other: Option<Type<'db>>) {
        let db = self.db();
        if let Some(other) = other {
            match self.cycle_recovery {
                Some(existing) => {
                    self.cycle_recovery = Some(UnionType::from_two_elements(
                        db,
                        self.program_environment(),
                        existing,
                        other,
                    ));
                }
                None => {
                    self.cycle_recovery = Some(other);
                }
            }
        }
    }

    fn extend_definition(
        &mut self,
        definition: Definition<'db>,
        inference: &DefinitionInference<'db>,
    ) {
        crate::types::signatures::effects::legacy_inline(self.extend_definition_with(
            definition,
            inference,
            &source_merge::LegacyExpressionMergeEffects,
        ));
    }

    fn extend_statement(&mut self, inference: &StatementInference<'db>) {
        let inference = match inference {
            StatementInference::Other(inference) => inference,
            StatementInference::Expression(inference) => return self.extend_expression(inference),
            StatementInference::Definition(definition, inference) => {
                return self.extend_definition(*definition, inference);
            }
        };

        #[cfg(debug_assertions)]
        assert_eq!(self.scope, inference.scope);

        self.extend_expression_types(inference.expressions.iter().copied());
        self.declarations.extend(inference.declarations());

        if !matches!(self.region, InferenceRegion::Scope(..)) {
            self.bindings.extend(inference.bindings());
        }

        if let Some(extra) = &inference.extra {
            self.implicit_aliases
                .extend(extra.implicit_aliases.iter().copied());
            self.comparison_truthiness
                .extend(extra.comparison_truthiness.iter().copied());
            self.called_functions
                .extend(extra.called_functions.iter().copied());
            self.return_types_and_ranges
                .extend(extra.return_types_and_ranges.iter().copied());
            self.extend_cycle_recovery(extra.cycle_recovery);
            self.context.extend(&extra.diagnostics);
            self.deferred.extend(extra.deferred.iter().copied());
            self.string_annotations
                .extend(extra.string_annotations.iter().copied());
            self.expected_types
                .extend(extra.expected_types.iter().copied());
            self.qualifiers.extend(extra.qualifiers.iter().copied());
            self.type_expression_flags
                .extend(extra.type_expression_flags.iter().copied());
        }
    }

    fn extend_expression(&mut self, inference: &ExpressionInference<'db>) {
        crate::types::signatures::effects::legacy_inline(
            self.extend_expression_with(inference, &source_merge::LegacyExpressionMergeEffects),
        );
    }

    fn extend_expression_unchecked(&mut self, inference: &ExpressionInference<'db>) {
        crate::types::signatures::effects::legacy_inline(self.extend_expression_unchecked_with(
            inference,
            true,
            &source_merge::LegacyExpressionMergeEffects,
        ));
    }

    /// Replacing an expression's type also replaces any truthiness override. A newly inferred
    /// comparison may no longer need an override, so extending the sparse map alone is not enough.
    fn extend_expression_types(
        &mut self,
        expressions: impl IntoIterator<Item = (ExpressionNodeKey, Type<'db>)>,
    ) {
        if self.comparison_truthiness.is_empty() {
            self.expressions.extend(expressions);
        } else {
            for (expression, ty) in expressions {
                self.expressions.insert(expression, ty);
                self.comparison_truthiness.remove(&expression);
            }
        }
    }

    /// Merges expression results without claiming bindings owned by their enclosing statement.
    fn extend_expression_without_bindings(&mut self, inference: &ExpressionInference<'db>) {
        crate::types::signatures::effects::legacy_inline(self.extend_expression_unchecked_with(
            inference,
            false,
            &source_merge::LegacyExpressionMergeEffects,
        ));
    }

    fn extend_expression_cache_entry(&mut self, inference: &FullExpressionCacheEntry<'db>) {
        #[cfg(debug_assertions)]
        assert_eq!(self.scope, inference.scope);

        self.extend_expression_types(inference.expressions.iter().map(|(key, ty)| (*key, *ty)));
        self.comparison_truthiness.extend(
            inference
                .comparison_truthiness
                .iter()
                .map(|(key, truthiness)| (*key, *truthiness)),
        );
        self.context.extend(&inference.diagnostics);
        self.extend_cycle_recovery(inference.cycle_recovery);
        self.called_functions
            .extend(inference.called_functions.iter().copied());
        self.implicit_aliases
            .extend(inference.implicit_aliases.iter().copied());
        self.string_annotations
            .extend(inference.string_annotations.iter().copied());
        self.expected_types
            .extend(inference.expected_types.iter().map(|(key, ty)| (*key, *ty)));
        self.type_expression_flags.extend(
            inference
                .type_expression_flags
                .iter()
                .map(|(key, flags)| (*key, *flags)),
        );

        #[expect(
            clippy::iter_over_hash_type,
            reason = "constraints for distinct collection definitions are merged independently"
        )]
        for (collection_def, constraints) in &inference.collection_use_constraints {
            self.collection_use_constraints
                .entry(*collection_def)
                .and_modify(|this| this.extend(constraints))
                .or_insert(constraints.clone());
        }

        if !matches!(self.region, InferenceRegion::Scope(..)) {
            self.bindings.extend(
                inference
                    .bindings
                    .iter()
                    .map(|(definition, ty)| (*definition, *ty)),
            );
        }
    }

    fn extend_scope(&mut self, inference: &ScopeInference<'db>) {
        self.extend_expression_types(inference.expressions.iter());

        if let Some(extra) = &inference.extra {
            self.implicit_aliases
                .extend(extra.implicit_aliases.iter().copied());
            self.context.extend(&extra.diagnostics);
            self.extend_cycle_recovery(extra.cycle_recovery);
            self.string_annotations
                .extend(extra.string_annotations.iter().copied());
            self.expected_types
                .extend(extra.expected_types.iter().copied());
            self.type_expression_flags
                .extend(extra.type_expression_flags.iter().copied());

            #[expect(
                clippy::iter_over_hash_type,
                reason = "constraints for distinct collection definitions are merged independently"
            )]
            for (collection_def, constraints) in &extra.collection_use_constraints {
                self.collection_use_constraints
                    .entry(*collection_def)
                    .and_modify(|this| this.extend(constraints))
                    .or_insert(constraints.clone());
            }
        }
    }

    fn file(&self) -> File {
        self.context.file()
    }

    fn program_file(&self) -> ProgramFile<'db> {
        self.context.program_file()
    }

    #[inline]
    fn program_environment(&self) -> &'ast ProgramEnvironment<'db> {
        self.context.program_environment()
    }

    fn module(&self) -> &'ast ParsedModuleRef {
        self.context.module()
    }

    fn db(&self) -> &'db dyn Db {
        self.context.db()
    }

    fn scope(&self) -> ScopeId<'db> {
        self.scope
    }

    /// Returns call bindings annotated with the call site's enclosing binding contexts.
    ///
    /// Call binding uses this as an optimization hint to avoid freshening generic callable
    /// signatures when the callable's generic context cannot collide with a containing scope.
    fn bindings_for_call(&self, callable_type: Type<'db>) -> Bindings<'db> {
        let db = self.db();
        callable_type
            .bindings(db, self.program_environment())
            .with_enclosing_binding_contexts(enclosing_binding_contexts(
                self.index,
                self.scope().file_scope_id(db),
            ))
    }

    fn settings(&self) -> &AnalysisSettings {
        self.db().analysis_settings(self.file())
    }

    fn is_in_type_checking_block(&self, scope: ScopeId<'db>, node: impl Ranged) -> bool {
        self.index
            .is_in_type_checking_block(scope.file_scope_id(self.db()), node.range())
    }

    /// Returns whether the current scope is the body of a dataclass or dataclass-transform class.
    ///
    /// Methods and nested functions have separate scopes and are not considered class bodies.
    fn is_in_dataclass_like_class_body(&self) -> bool {
        let db = self.db();
        let scope = self.scope();

        self.index.scope(scope.file_scope_id(db)).kind() == ScopeKind::Class
            && nearest_enclosing_class(db, self.index, scope)
                .and_then(|class| CodeGeneratorKind::from_class(db, class.into()))
                .is_some_and(|kind| {
                    matches!(
                        kind,
                        CodeGeneratorKind::DataclassLike(_) | CodeGeneratorKind::Pydantic(_)
                    )
                })
    }

    /// If the current scope is a class body scope of a dataclass-like class, populate
    /// `self.dataclass_field_specifiers` with the field specifiers from the class's
    /// `dataclass_params` or `dataclass_transform` parameters. This is needed so that
    /// calls to field-specifier functions are recognized during type inference of the
    /// right-hand side of annotated assignments.
    fn setup_dataclass_field_specifiers(&mut self) {
        if !self.has_dataclass_field_specifier_source(self.scope().file_scope_id(self.db())) {
            return;
        }
        fn field_specifiers<'db>(
            db: &'db dyn Db,
            index: &'db SemanticIndex<'db>,
            scope: ScopeId<'db>,
        ) -> Option<SmallVec<[Type<'db>; NUM_FIELD_SPECIFIERS_INLINE]>> {
            let enclosing_scope = index.scope(scope.file_scope_id(db));
            let class_node = enclosing_scope.node().as_class()?;
            let class_definition = index.expect_single_definition(class_node);
            let class_literal = original_class_type(db, class_definition)?.as_static()?;

            class_literal
                .dataclass_params(db)
                .map(|params| SmallVec::from(params.field_specifiers(db)))
                .or_else(|| {
                    Some(SmallVec::from(
                        CodeGeneratorKind::from_class(db, class_literal.into())?
                            .field_specifiers(db)?,
                    ))
                })
        }

        if let Some(specifiers) = field_specifiers(self.db(), self.index, self.scope()) {
            self.dataclass_field_specifiers = specifiers;
        }
    }

    fn has_dataclass_field_specifier_source(&self, file_scope: FileScopeId) -> bool {
        self.index.scope(file_scope).node().as_class().is_some()
    }

    /// Setup a shared expression cache for multi-inference.
    ///
    /// Returns `false` if the expression cache was already initialized.
    fn setup_expression_cache(&mut self) -> bool {
        if self.expression_cache.is_some() {
            false
        } else {
            self.expression_cache = Some(Rc::new(RefCell::new(ExpressionCache::default())));
            true
        }
    }

    fn teardown_expression_cache(&mut self) {
        self.expression_cache = None;
    }

    /// Are we currently inferring types in file with deferred types?
    /// This is true for stub files, for files with `__future__.annotations`, and
    /// by default for all source files in Python 3.14 and later.
    fn defer_annotations(&self) -> bool {
        let db = self.db();
        self.index.has_future_annotations()
            || self.in_stub()
            || self.program_environment().python_version(db) >= PythonVersion::PY314
    }

    /// Are we currently in a context where name resolution should be deferred
    /// (`__future__.annotations`, stub file, or stringified annotation)?
    fn is_deferred(&self) -> bool {
        self.deferred_state.is_deferred()
    }

    /// Return the node key of the given AST node, or the key of the outermost enclosing string
    /// literal, if the node originates from inside a stringified annotation.
    fn enclosing_node_key(&self, node: AnyNodeRef<'_>) -> NodeKey {
        match self.deferred_state {
            DeferredExpressionState::InStringAnnotation(enclosing_node_key) => enclosing_node_key,
            _ => NodeKey::from_node(node),
        }
    }

    fn in_stub(&self) -> bool {
        self.context.in_stub()
    }

    fn in_string_annotation(&self) -> bool {
        self.deferred_state.in_string_annotation()
    }

    /// Temporarily changes lookup behavior without discarding the current string annotation.
    ///
    /// Parsed string nodes do not belong to the module's semantic index, so their enclosing
    /// annotation must remain available even when nested expressions request another lookup mode.
    fn replace_deferred_state(
        &mut self,
        state: DeferredExpressionState,
    ) -> DeferredExpressionState {
        let previous = self.deferred_state;
        if !previous.in_string_annotation() {
            self.deferred_state = state;
        }
        previous
    }

    /// Get the already-inferred type of an expression node, or Unknown.
    fn expression_type(&self, expr: &ast::Expr) -> Type<'db> {
        self.try_expression_type(expr).unwrap_or_else(Type::unknown)
    }

    fn try_expression_type(&self, expr: &ast::Expr) -> Option<Type<'db>> {
        self.expressions
            .get(&expr.into())
            .copied()
            .or(self.fallback_type())
    }

    /// Return an already-inferred type for `expr`, or infer it with `tcx` if needed.
    ///
    /// This is used in places where an expression may already have been inferred earlier with a
    /// more specific type context, and re-inferring it would be redundant or would duplicate
    /// diagnostics.
    fn get_or_infer_expression(&mut self, expr: &ast::Expr, tcx: TypeContext<'db>) -> Type<'db> {
        self.try_expression_type(expr)
            .unwrap_or_else(|| self.infer_expression(expr, tcx))
    }

    /// Store qualifiers for an annotation expression.
    fn store_qualifiers(&mut self, expr: &ast::Expr, qualifiers: TypeQualifiers) {
        if !qualifiers.is_empty() {
            self.qualifiers.insert(expr.into(), qualifiers);
        }
    }

    /// Store metadata for a type expression.
    fn store_type_expression_flags(
        &mut self,
        expr: impl Into<ExpressionNodeKey>,
        flags: TypeExpressionFlags,
    ) {
        if flags.is_empty() {
            return;
        }

        self.type_expression_flags
            .entry(expr.into())
            .or_default()
            .insert(flags);
    }

    /// Get metadata for a type expression from the current inference result.
    fn type_expression_flags(&self, expr: impl Into<ExpressionNodeKey>) -> TypeExpressionFlags {
        self.type_expression_flags
            .get(&expr.into())
            .copied()
            .unwrap_or_default()
    }

    /// Get the type of an expression from any scope in the same file.
    ///
    /// If the expression is in the current scope, and we are inferring the entire scope, just look
    /// up the expression in our own results, otherwise call [`infer_scope_types()`] for the scope
    /// of the expression.
    ///
    /// ## Panics
    ///
    /// If the expression is in the current scope but we haven't yet inferred a type for it.
    ///
    /// Can cause query cycles if the expression is from a different scope and type inference is
    /// already in progress for that scope (further up the stack).
    fn file_expression_type(&self, expression: &ast::Expr) -> Type<'db> {
        let file_scope = self.index.expression_scope_id(expression);
        let expr_scope = file_scope.to_scope_id(self.db(), self.program_file());
        match self.region {
            InferenceRegion::Scope(scope, _) if scope == expr_scope => {
                self.expression_type(expression)
            }
            _ => infer_complete_scope_types(self.db(), expr_scope).expression_type(expression),
        }
    }

    /// Get metadata for a type expression from any scope in the same file.
    fn file_type_expression_flags(&self, expression: &ast::Expr) -> TypeExpressionFlags {
        let file_scope = self.index.expression_scope_id(expression);
        let expr_scope = file_scope.to_scope_id(self.db(), self.program_file());
        match self.region {
            InferenceRegion::Scope(scope, _) if scope == expr_scope => {
                self.type_expression_flags(expression)
            }
            _ => {
                infer_complete_scope_types(self.db(), expr_scope).type_expression_flags(expression)
            }
        }
    }

    /// Infers types in the given [`InferenceRegion`].
    fn infer_region(&mut self) {
        match self.region {
            InferenceRegion::Statement(statement) => self.infer_region_statement(statement),
            InferenceRegion::Scope(scope, tcx) => self.infer_region_scope(scope, tcx),
            InferenceRegion::Definition(definition) => self.infer_region_definition(definition),
            InferenceRegion::FunctionDecorators(definition) => {
                self.infer_region_function_decorators(definition);
            }
            InferenceRegion::FunctionDefaults(definition) => {
                if let DefinitionKind::Function(function) = definition.kind(self.db()) {
                    self.infer_function_defaults(definition, function.node(self.module()));
                }
            }
            InferenceRegion::Deferred(definition) => self.infer_region_deferred(definition),
            InferenceRegion::Expression(expression, tcx) => {
                self.infer_region_expression(expression, tcx);
            }
        }
    }

    fn infer_region_scope(&mut self, scope: ScopeId<'db>, tcx: TypeContext<'db>) {
        match scope::infer_scope_sync(
            self,
            scope,
            tcx,
            scope::ScopeFacts,
            &scope::OrdinaryScopeEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    fn infer_region_statement(&mut self, statement: StatementInner<'db>) {
        self.infer_statement(statement.node_ref(self.db()).node(self.module()));
    }

    fn infer_region_definition(&mut self, definition: Definition<'db>) {
        crate::types::signatures::effects::legacy_inline(
            self.infer_region_definition_with(
                &source_definition::LegacyDefinitionEffects,
                definition,
            ),
        );
    }

    async fn infer_region_definition_with<E: source_definition::DefinitionEffects<'db>>(
        &mut self,
        effects: &E,
        definition: Definition<'db>,
    ) -> Result<(), E::Error> {
        match effects.definition_kind(self.db(), definition).await? {
            DefinitionKind::Function(function) => {
                effects
                    .function(self, function.node(self.module()), definition)
                    .await?;
            }
            DefinitionKind::Class(class) => {
                effects
                    .class(self, class.node(self.module()), definition)
                    .await?;
            }
            DefinitionKind::TypeAlias(type_alias) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::TypeAlias,
                        self,
                        |builder| {
                            builder.infer_type_alias_definition(
                                type_alias.node(builder.module()),
                                definition,
                            );
                        },
                    )
                    .await?;
            }
            DefinitionKind::Import(import) => {
                effects
                    .import(self, import.alias(self.module()), definition)
                    .await?;
            }
            DefinitionKind::ImportFrom(import_from) => {
                effects
                    .import_from(
                        self,
                        import_from.import(self.module()),
                        import_from.alias(self.module()),
                        definition,
                    )
                    .await?;
            }
            DefinitionKind::ImportFromSubmodule(import_from) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::ImportFromSubmodule,
                        self,
                        |builder| {
                            builder.infer_import_from_submodule_definition(
                                import_from.import(builder.module()),
                                definition,
                            );
                        },
                    )
                    .await?;
            }
            DefinitionKind::StarImport(import) => {
                effects
                    .import_from(
                        self,
                        import.import(self.module()),
                        import.alias(self.module()),
                        definition,
                    )
                    .await?;
            }
            DefinitionKind::Assignment(assignment) => {
                effects.assignment(self, assignment, definition).await?;
            }
            DefinitionKind::AnnotatedAssignment(annotated_assignment) => {
                effects
                    .annotated_assignment(self, annotated_assignment, definition)
                    .await?;
            }
            DefinitionKind::AugmentedAssignment(augmented_assignment) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::AugmentedAssignment,
                        self,
                        |builder| {
                            builder.infer_augment_assignment_definition(
                                augmented_assignment.node(builder.module()),
                                definition,
                            );
                        },
                    )
                    .await?;
            }
            DefinitionKind::DictKeyAssignment(dict_key_assignment) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::DictKeyAssignment,
                        self,
                        |builder| {
                            builder.infer_dict_key_assignment_definition(
                                dict_key_assignment.key(builder.module()),
                                dict_key_assignment.value(builder.module()),
                                dict_key_assignment.assignment(),
                                definition,
                            );
                        },
                    )
                    .await?;
            }
            DefinitionKind::For(for_statement_definition) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::For,
                        self,
                        |builder| {
                            builder.infer_for_statement_definition(
                                for_statement_definition,
                                definition,
                            );
                        },
                    )
                    .await?;
            }
            DefinitionKind::NamedExpression(named_expression) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::NamedExpression,
                        self,
                        |builder| {
                            builder.infer_named_expression_definition(
                                named_expression.node(builder.module()),
                                definition,
                            );
                        },
                    )
                    .await?;
            }
            DefinitionKind::Comprehension(comprehension) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::Comprehension,
                        self,
                        |builder| {
                            builder.infer_comprehension_definition(comprehension, definition);
                        },
                    )
                    .await?;
            }
            DefinitionKind::Parameter(
                ParameterDefinitionNodeKind::VariadicPositionalParameter(parameter),
            ) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::Parameter,
                        self,
                        |builder| {
                            builder.infer_variadic_positional_parameter_definition(
                                parameter.node(builder.module()),
                                definition,
                            );
                        },
                    )
                    .await?;
            }
            DefinitionKind::Parameter(ParameterDefinitionNodeKind::VariadicKeywordParameter(
                parameter,
            )) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::Parameter,
                        self,
                        |builder| {
                            builder.infer_variadic_keyword_parameter_definition(
                                parameter.node(builder.module()),
                                definition,
                            );
                        },
                    )
                    .await?;
            }
            DefinitionKind::Parameter(ParameterDefinitionNodeKind::Parameter(
                parameter_with_default,
            )) => {
                effects
                    .parameter(self, parameter_with_default.node(self.module()), definition)
                    .await?;
            }
            DefinitionKind::LambdaParameter(LambdaParameterDefinitionNodeKind {
                index,
                lambda,
                parameter: ParameterDefinitionNodeKind::VariadicPositionalParameter(parameter),
            }) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::LambdaParameter,
                        self,
                        |builder| {
                            builder.infer_variadic_positional_lambda_parameter_definition(
                                *index,
                                parameter.node(builder.module()),
                                lambda.node(builder.module()),
                                definition,
                            );
                        },
                    )
                    .await?;
            }
            DefinitionKind::LambdaParameter(LambdaParameterDefinitionNodeKind {
                parameter: ParameterDefinitionNodeKind::VariadicKeywordParameter(parameter),
                ..
            }) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::LambdaParameter,
                        self,
                        |builder| {
                            builder.infer_variadic_keyword_lambda_parameter_definition(
                                parameter.node(builder.module()),
                                definition,
                            );
                        },
                    )
                    .await?;
            }
            DefinitionKind::LambdaParameter(LambdaParameterDefinitionNodeKind {
                index,
                lambda,
                parameter: ParameterDefinitionNodeKind::Parameter(parameter_with_default),
            }) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::LambdaParameter,
                        self,
                        |builder| {
                            builder.infer_lambda_parameter_definition(
                                *index,
                                parameter_with_default.node(builder.module()),
                                lambda.node(builder.module()),
                                definition,
                            );
                        },
                    )
                    .await?;
            }
            DefinitionKind::WithItem(with_item_definition) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::WithItem,
                        self,
                        |builder| {
                            builder.infer_with_item_definition(with_item_definition, definition);
                        },
                    )
                    .await?;
            }
            DefinitionKind::MatchPattern(match_pattern) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::MatchPattern,
                        self,
                        |builder| {
                            builder.infer_match_pattern_definition(
                                match_pattern.pattern(builder.module()),
                                match_pattern.predicate(),
                                definition,
                            );
                        },
                    )
                    .await?;
            }
            DefinitionKind::ExceptHandler(except_handler_definition) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::ExceptHandler,
                        self,
                        |builder| {
                            builder.infer_except_handler_definition(
                                except_handler_definition,
                                definition,
                            );
                        },
                    )
                    .await?;
            }
            DefinitionKind::TypeVar(node) => {
                effects
                    .type_parameter(
                        self,
                        typevar::pep695::TypeParameterDefinitionNode::TypeVar(node),
                        definition,
                    )
                    .await?;
            }
            DefinitionKind::ParamSpec(node) => {
                effects
                    .type_parameter(
                        self,
                        typevar::pep695::TypeParameterDefinitionNode::ParamSpec(node),
                        definition,
                    )
                    .await?;
            }
            DefinitionKind::TypeVarTuple(node) => {
                effects
                    .type_parameter(
                        self,
                        typevar::pep695::TypeParameterDefinitionNode::TypeVarTuple(node),
                        definition,
                    )
                    .await?;
            }
            DefinitionKind::LoopHeader(loop_header) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::LoopHeaderDefinition,
                        self,
                        |builder| {
                            builder.infer_loop_header_definition(loop_header, definition);
                        },
                    )
                    .await?;
            }
            DefinitionKind::NestedBindings(nested_bindings) => {
                effects
                    .legacy_operation(
                        source_definition::SourceDefinitionEffect::NestedBindings,
                        self,
                        |builder| {
                            builder.infer_nested_bindings_definition(nested_bindings, definition);
                        },
                    )
                    .await?;
            }
        }
        Ok(())
    }

    fn infer_region_function_decorators(
        &mut self,
        definition: Definition<'db>,
    ) -> FunctionDecoratorClassification {
        let function = match definition.kind(self.db()) {
            DefinitionKind::Function(function) => Some(function.node(self.module())),
            _ => None,
        };
        match function_decorators_sync(
            self,
            function,
            FunctionDecoratorFacts,
            &OrdinaryFunctionDecoratorEffects,
        ) {
            Ok(classification) => classification,
            Err(error) => match error {},
        }
    }

    fn infer_region_deferred(&mut self, definition: Definition<'db>) {
        crate::types::signatures::effects::legacy_inline(
            self.infer_region_deferred_with(&deferred::LegacyDeferredEffects, definition),
        );
    }

    fn infer_region_expression(&mut self, expression: Expression<'db>, tcx: TypeContext<'db>) {
        self.setup_dataclass_field_specifiers();
        self.setup_expression_region_flags(expression.kind(self.db()));

        match expression.kind(self.db()) {
            ExpressionKind::Callee => {
                self.infer_expression_impl(expression.node_ref(self.db()).node(self.module()), tcx);
            }
            ExpressionKind::Normal => {
                self.infer_expression_impl(expression.node_ref(self.db()).node(self.module()), tcx);
            }
            ExpressionKind::TypeExpression => {
                self.infer_type_expression(expression.node_ref(self.db()).node(self.module()));
            }
        }
    }

    fn setup_expression_region_flags(&mut self, kind: ExpressionKind) {
        if kind == ExpressionKind::Callee {
            self.context.inference_flags |= InferenceFlags::CHECK_UNBOUND_TYPEVARS;
        }
    }

    /// Add a binding for the given definition.
    ///
    /// Returns the result of the `infer_value_ty` closure, which is called with the declared type
    /// as type context.
    fn add_binding<'a>(
        &mut self,
        node: AnyNodeRef<'a>,
        binding: Definition<'db>,
    ) -> AddBinding<'db, 'a> {
        crate::types::signatures::effects::legacy_inline(self.add_binding_with(
            &source_binding::LegacySourceBindingEffects,
            node,
            binding,
        ))
    }

    /// For a member binding without a live place declaration, obtain its declared type from
    /// normal attribute or subscript lookup on its receiver.
    fn fallback_member_declared_type(&mut self, node: AnyNodeRef<'_>) -> Option<Type<'db>> {
        let db = self.db();
        if let AnyNodeRef::ExprAttribute(ast::ExprAttribute { value, attr, .. }) = node {
            let value_type = self.try_expression_type(value).unwrap_or_else(|| {
                self.infer_maybe_standalone_expression(value, TypeContext::default())
            });
            if let Place::Defined(DefinedPlace {
                ty,
                definedness: Definedness::AlwaysDefined,
                ..
            }) = value_type
                .member(db, self.program_environment(), attr)
                .place
            {
                // TODO: also consider qualifiers on the attribute
                Some(ty)
            } else {
                None
            }
        } else if let AnyNodeRef::ExprSubscript(
            subscript @ ast::ExprSubscript {
                value, slice, ctx, ..
            },
        ) = node
        {
            let value_ty = self.get_or_infer_expression(value, TypeContext::default());
            let slice_ty = self.get_or_infer_expression(slice, TypeContext::default());
            Some(
                self.infer_subscript_expression_types(subscript, value_ty, slice_ty, *ctx)
                    .unwrap_or_else(|recovery_ty| recovery_ty),
            )
        } else {
            None
        }
    }

    /// Returns the owner of an assignment redirected by `global` or `nonlocal`.
    ///
    /// `global` assignments target the module symbol, while `nonlocal` assignments target the
    /// closest owning function-like scope. Local assignments and forwarding declarations whose
    /// owner cannot be resolved return `None`.
    ///
    /// ```python
    /// x = 0
    ///
    /// def outer():
    ///     y = 0
    ///
    ///     def inner():
    ///         global x
    ///         nonlocal y
    ///         x = 1  # owned by the module scope
    ///         y = 1  # owned by `outer`
    /// ```
    fn forwarded_assignment_owner(
        &self,
        scope: FileScopeId,
        symbol: ScopedSymbolId,
    ) -> Option<(FileScopeId, ScopedSymbolId)> {
        let scoped_symbol = self.index.place_table(scope).symbol(symbol);

        if scope.is_global() || scoped_symbol.is_local() {
            return None;
        }

        if scoped_symbol.is_global() {
            let global_scope = FileScopeId::global();
            // If this variable appears in a `global` declaration but has no explicit binding in
            // the global scope, return `None` so the caller can fall back to the local scope.
            return self
                .index
                .place_table(global_scope)
                .symbol_id(scoped_symbol.name())
                .map(|symbol| (global_scope, symbol));
        }

        debug_assert!(scoped_symbol.is_nonlocal());

        // Walk up parent scopes looking for the enclosing scope that defines this name.
        // `ancestor_scopes` includes the current scope, so skip that one.
        for (enclosing_scope, enclosing) in self.index.ancestor_scopes(scope).skip(1) {
            // Ignore class scopes and the global scope.
            if !enclosing.kind().is_function_like() {
                continue;
            }
            let place_table = self.index.place_table(enclosing_scope);
            let Some(enclosing_symbol) = place_table.symbol_id(scoped_symbol.name()) else {
                // This ancestor scope doesn't have a binding. Keep going.
                continue;
            };
            let symbol = place_table.symbol(enclosing_symbol);
            if symbol.is_global() {
                // The variable is `global` in this ancestor scope. This breaks the `nonlocal`
                // chain, and it's a syntax error in `infer_nonlocal_statement`. Ignore that here
                // and bail out of this loop.
                break;
            }
            if !symbol.is_local() {
                // The variable is either explicitly `nonlocal` or just a free read in this
                // ancestor scope. Keep going.
                continue;
            }

            // We found the closest definition. Note that (as in `infer_place_load`) this does not
            // need to be a binding. It could be just a declaration, e.g. `x: int`.
            return Some((enclosing_scope, enclosing_symbol));
        }

        // If no ancestor owns the name, return `None` so the caller can fall back to the local
        // scope. This will also be reported as a syntax error in `infer_nonlocal_statement`.
        None
    }

    /// Returns `true` if `symbol_id` should be looked up in the global scope, skipping intervening
    /// local scopes.
    fn skip_non_global_scopes(
        &self,
        file_scope_id: FileScopeId,
        symbol_id: ScopedSymbolId,
    ) -> bool {
        !file_scope_id.is_global()
            && self
                .index
                .symbol_is_global_in_scope(symbol_id, file_scope_id)
    }

    fn add_declaration(
        &mut self,
        node: AnyNodeRef,
        declaration: Definition<'db>,
        ty: TypeAndQualifiers<'db>,
    ) {
        source_declaration::add_declaration_sync(self, node, declaration, ty);
    }

    fn add_declaration_with_binding(
        &mut self,
        node: AnyNodeRef,
        definition: Definition<'db>,
        declared_and_inferred_ty: &DeclaredAndInferredType<'db>,
    ) {
        debug_assert!(
            definition
                .kind(self.db())
                .category(self.context.in_stub(), self.module())
                .is_binding()
        );
        debug_assert!(
            definition
                .kind(self.db())
                .category(self.context.in_stub(), self.module())
                .is_declaration()
        );

        self.add_validated_declaration_with_binding(node, definition, declared_and_inferred_ty);
    }

    /// The caller has checked that the definition is both a declaration and a binding.
    fn add_validated_declaration_with_binding(
        &mut self,
        node: AnyNodeRef,
        definition: Definition<'db>,
        declared_and_inferred_ty: &DeclaredAndInferredType<'db>,
    ) {
        match declaration_binding::add_declaration_binding_sync(
            self,
            node,
            definition,
            declared_and_inferred_ty.clone(),
            declaration_binding::DeclarationBindingFacts,
            &declaration_binding::OrdinaryDeclarationBindingEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    /// Checks an assigned value against its target's declared type and reports any mismatch.
    ///
    /// Returns `true` when the value is assignable, even if the stricter `unsound-assignment`
    /// rule reports that it is not a subtype. Returns `false` when the value is not assignable,
    /// in which case `invalid-assignment` is reported instead.
    ///
    /// The `unsound-assignment` rule is deliberately limited to name bindings; assignments to
    /// attributes and subscripts are outside its scope.
    fn validate_assignment_type_legacy(
        &self,
        target_node: AnyNodeRef,
        definition: Definition<'db>,
        declaration: Option<Definition<'db>>,
        target_ty: Type<'db>,
        value_ty: Type<'db>,
    ) -> bool {
        match assignment_validation::validate_assignment_sync(
            self,
            assignment_validation::AssignmentValidationRequest {
                node: target_node,
                binding: definition,
                declaration,
                target: target_ty,
                value: value_ty,
            },
            assignment_validation::AssignmentValidationFacts,
            &assignment_validation::OrdinaryAssignmentValidationEffects,
        ) {
            Ok(valid) => valid,
            Err(never) => match never {},
        }
    }

    fn record_return_type(&mut self, ty: Type<'db>, range: TextRange) {
        self.return_types_and_ranges
            .push(TypeAndRange { ty, range });
    }

    fn infer_module(&mut self, module: &ast::ModModule) {
        match source_statement::infer_module_sync(
            self,
            module,
            source_statement::StatementFacts,
            &source_statement::OrdinaryStatementEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    fn infer_type_alias_type_params(&mut self, type_alias: &ast::StmtTypeAlias) {
        let type_params = type_alias
            .type_params
            .as_ref()
            .expect("type alias type params scope without type params");

        let binding_context = self.index.expect_single_definition(type_alias);
        let previous_typevar_binding_context =
            self.typevar_binding_context.replace(binding_context);
        self.infer_type_parameters(type_params);
        self.typevar_binding_context = previous_typevar_binding_context;
    }

    fn infer_type_alias(&mut self, type_alias: &ast::StmtTypeAlias) {
        let previous_check_unbound_typevars = self
            .context
            .inference_flags
            .replace(InferenceFlags::CHECK_UNBOUND_TYPEVARS, true);
        self.context.inference_flags |= InferenceFlags::IN_TYPE_ALIAS;
        self.infer_type_expression(&type_alias.value);
        self.context
            .inference_flags
            .remove(InferenceFlags::IN_TYPE_ALIAS);
        self.context.inference_flags.set(
            InferenceFlags::CHECK_UNBOUND_TYPEVARS,
            previous_check_unbound_typevars,
        );

        if let Some(name) = type_alias.name.as_name_expr() {
            self.check_type_alias_cycle(&name.id, &type_alias.value);
        }
    }

    /// Check both alias syntaxes, including union members that disappear during expansion.
    fn check_type_alias_cycle(&mut self, name: &str, value: &ast::Expr) {
        let db = self.db();
        let value_ty = self.expression_type(value);
        let expanded = value_ty.expand_eagerly(db, self.program_environment());
        if (expanded.is_divergent() || value_ty.has_unguarded_alias_cycle(db))
            && let Some(builder) = self
                .context
                .report_lint(&CYCLIC_TYPE_ALIAS_DEFINITION, value)
        {
            builder.into_diagnostic(format_args!(
                "Type alias `{name}` has a circular definition"
            ));
        }
        if expanded.is_divergent() {
            // Preserve the dynamic recovery type for aliases that cannot be expanded at all.
            // Union cycles retain their non-recursive members for recovery.
            self.expressions.insert(value.into(), expanded);
        }
    }

    /// If the current scope is a method inside an enclosing class,
    /// return `Some(class)` where `class` represents the enclosing class.
    ///
    /// If the current scope is not a method inside an enclosing class,
    /// return `None`.
    ///
    /// Note that this method will only return `Some` if the immediate parent scope
    /// is a class scope OR the immediate parent scope is an annotation scope
    /// and the grandparent scope is a class scope. This means it has different
    /// behaviour to the [`super::nearest_enclosing_class`] function.
    fn class_context_of_current_method(&self) -> Option<ClassType<'db>> {
        let current_scope_id = self.scope().file_scope_id(self.db());
        let class_definition = self.index.class_definition_of_method(current_scope_id)?;
        original_class_type(self.db(), class_definition)
            .map(|class_literal| class_literal.default_specialization(self.db()))
    }

    /// Report an undeclared protocol attribute written through a method receiver.
    ///
    /// The instance or class receiver may be referenced directly, from an eager nested scope, or
    /// through a capture in a nested function.
    fn report_undeclared_protocol_attribute(&self, target: &ast::ExprAttribute) {
        let db = self.db();
        let Some(receiver) = target.value.as_name_expr() else {
            return;
        };
        let Some(method_scope_id) = self.receiver_method_scope(receiver) else {
            return;
        };
        let Some(protocol) = self
            .index
            .class_definition_of_method(method_scope_id)
            .and_then(|definition| original_class_type(db, definition))
            .map(|class| class.default_specialization(db))
            .and_then(|class| class.into_protocol_class(db))
        else {
            return;
        };
        if protocol.interface(db).includes_member(db, target.attr.id())
            || protocol.has_member_declaration(db, target.attr.id())
        {
            return;
        }

        diagnostic::report_undeclared_protocol_attribute(&self.context, target, protocol);
    }

    /// If the current scope is a (non-lambda) function, return that function's AST node.
    ///
    /// If the current scope is not a function (or it is a lambda function), return `None`.
    fn current_function_definition(&self) -> Option<&ast::StmtFunctionDef> {
        let current_scope_id = self.scope().file_scope_id(self.db());
        let current_scope = self.index.scope(current_scope_id);
        if !current_scope.kind().is_non_lambda_function() {
            return None;
        }
        current_scope
            .node()
            .as_function()
            .map(|node_ref| node_ref.node(self.module()))
    }

    fn function_type(&self, function: &ast::StmtFunctionDef) -> Option<FunctionType<'db>> {
        let definition = self.index.expect_single_definition(function);
        infer_definition_types(self.db(), definition).function_type(definition)
    }

    fn current_function_type(&self) -> Option<FunctionType<'db>> {
        self.function_type(self.current_function_definition()?)
    }

    fn function_decorator_types<'a>(
        &'a self,
        function: &'a ast::StmtFunctionDef,
    ) -> impl Iterator<Item = Type<'db>> + 'a {
        let definition = self.index.expect_single_definition(function);

        let definition_types = infer_definition_types(self.db(), definition);

        function
            .decorator_list
            .iter()
            .map(move |decorator| definition_types.expression_type(&decorator.expression))
    }

    /// Returns `true` if the current scope is the function body scope of a function overload (that
    /// is, the stub declaration decorated with `@overload`, not the implementation), or an
    /// abstract method (decorated with `@abstractmethod`.)
    fn in_function_overload_or_abstractmethod(&self) -> bool {
        let Some(function) = self.current_function_definition() else {
            return false;
        };

        self.function_decorator_types(function)
            .any(|decorator_type| {
                match decorator_type {
                    Type::FunctionLiteral(function) => matches!(
                        function.known(self.db()),
                        Some(KnownFunction::Overload | KnownFunction::AbstractMethod)
                    ),
                    Type::Never => {
                        // In unreachable code, we infer `Never` for decorators like `typing.overload`.
                        // Return `true` here to avoid false positive `invalid-return-type` lints for
                        // `@overload`ed functions without a body in unreachable code.
                        true
                    }
                    Type::Divergent(_) => true,
                    _ => false,
                }
            })
    }

    fn infer_body(&mut self, suite: &[ast::Stmt]) {
        match source_statement::infer_body_sync(
            self,
            suite,
            &source_statement::OrdinaryStatementEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    fn infer_statement(&mut self, statement: &ast::Stmt) {
        match source_statement::infer_statement_sync(
            self,
            statement,
            &source_statement::OrdinaryStatementEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    fn infer_definition(&mut self, node: impl Into<DefinitionNodeKey> + std::fmt::Debug + Copy) {
        let definition = self.index.expect_single_definition(node);
        let result = infer_definition_types(self.db(), definition);
        self.extend_definition(definition, result);
    }

    fn infer_type_alias_definition(
        &mut self,
        type_alias: &ast::StmtTypeAlias,
        definition: Definition<'db>,
    ) {
        let alias_name = &type_alias.name.as_name_expr().unwrap().id;

        // Check that no type parameter with a default follows a TypeVarTuple
        // in the type alias's PEP 695 type parameter list.
        if let Some(type_params) = type_alias.type_params.as_deref() {
            post_inference::type_param_validation::check_single_typevar_tuple_pep695(
                &self.context,
                type_params,
                post_inference::type_param_validation::TypeParameterOwner::TypeAlias(alias_name),
            );
            post_inference::type_param_validation::check_no_default_after_typevar_tuple_pep695(
                &self.context,
                type_params,
            );
        }

        let rhs_scope = self
            .index
            .node_scope(NodeWithScopeRef::TypeAlias(type_alias))
            .to_scope_id(self.db(), self.program_file());

        let type_alias_ty =
            Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(
                PEP695TypeAliasType::new(self.db(), alias_name, rhs_scope, None, None),
            )));

        self.store_expression_type(&type_alias.name, type_alias_ty);

        self.add_declaration_with_binding(
            type_alias.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(type_alias_ty),
        );
    }

    fn infer_if_statement(&mut self, if_statement: &ast::StmtIf) {
        match source_statement::infer_if_sync(
            self,
            if_statement,
            &source_statement::OrdinaryStatementEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    fn infer_try_statement(&mut self, try_statement: &ast::StmtTry) {
        let ast::StmtTry {
            range: _,
            node_index: _,
            body,
            handlers,
            orelse,
            finalbody,
            is_star: _,
        } = try_statement;

        self.infer_body(body);

        for handler in handlers {
            let ast::ExceptHandler::ExceptHandler(handler) = handler;
            let ast::ExceptHandlerExceptHandler {
                type_: handled_exceptions,
                name: symbol_name,
                body,
                range: _,
                node_index: _,
            } = handler;

            // If `symbol_name` is `Some()` and `handled_exceptions` is `None`,
            // it's invalid syntax (something like `except as e:`).
            // However, it's obvious that the user *wanted* `e` to be bound here,
            // so we'll have created a definition in the semantic-index stage anyway.
            if symbol_name.is_some() {
                self.infer_definition(handler);
            } else {
                self.infer_exception(handled_exceptions.as_deref(), try_statement.is_star);
            }

            self.infer_body(body);
        }

        self.infer_body(orelse);
        self.infer_body(finalbody);
    }

    fn infer_with_statement(&mut self, with_statement: &ast::StmtWith) {
        let db = self.db();
        let ast::StmtWith {
            range: _,
            node_index: _,
            is_async,
            items,
            body,
        } = with_statement;
        for item in items {
            let target = item.optional_vars.as_deref();
            if let Some(target) = target {
                self.infer_target(target, &item.context_expr, &|builder, tcx| {
                    // TODO: `infer_with_statement_definition` reports a diagnostic if `ctx_manager_ty` isn't a context manager
                    //  but only if the target is a name. We should report a diagnostic here if the target isn't a name:
                    //  `with not_context_manager as a.x: ...
                    builder
                        .infer_standalone_expression(&item.context_expr, tcx)
                        .enter(db, builder.program_environment())
                });
            } else {
                // Call into the context expression inference to validate that it evaluates
                // to a valid context manager.
                let context_expression_ty = self
                    .infer_maybe_standalone_expression(&item.context_expr, TypeContext::default());
                self.infer_context_expression(&item.context_expr, context_expression_ty, *is_async);
                self.infer_optional_expression(target, TypeContext::default());
            }
        }

        self.infer_body(body);
    }

    fn infer_with_item_definition(
        &mut self,
        with_item: &WithItemDefinitionKind<'db>,
        definition: Definition<'db>,
    ) {
        let context_expr = with_item.context_expr(self.module());
        let target = with_item.target(self.module());

        let target_ty = match with_item.target_kind() {
            TargetKind::Sequence(unpack_position, unpack) => {
                let unpacked = infer_unpack_types(self.db(), unpack);
                if unpack_position == UnpackPosition::First {
                    self.context.extend(unpacked.diagnostics());
                }
                unpacked.expression_type(target)
            }
            TargetKind::Single => {
                let context_expr_ty =
                    self.infer_standalone_expression(context_expr, TypeContext::default());
                self.infer_context_expression(context_expr, context_expr_ty, with_item.is_async())
            }
        };

        self.store_expression_type(target, target_ty);
        self.add_binding(target.into(), definition)
            .insert(self, target_ty);
    }

    /// Infers the type of a context expression (`with expr`) and returns the target's type
    ///
    /// Returns [`Type::unknown`] if the context expression doesn't implement the context manager protocol.
    ///
    /// ## Terminology
    /// See [PEP343](https://peps.python.org/pep-0343/#standard-terminology).
    fn infer_context_expression(
        &mut self,
        context_expression: &ast::Expr,
        context_expression_type: Type<'db>,
        is_async: bool,
    ) -> Type<'db> {
        let db = self.db();
        let eval_mode = if is_async {
            EvaluationMode::Async
        } else {
            EvaluationMode::Sync
        };

        let env = self.program_environment();
        context_expression_type
            .try_enter_with_mode(db, env, eval_mode)
            .unwrap_or_else(|err| {
                err.report_diagnostic(
                    &self.context,
                    context_expression_type,
                    context_expression.into(),
                );
                err.fallback_enter_type(db, env)
            })
    }

    fn infer_exception(&mut self, node: Option<&ast::Expr>, is_star: bool) -> Type<'db> {
        let db = self.db();
        // If there is no handled exception, it's invalid syntax;
        // a diagnostic will have already been emitted
        let node_ty = node.map_or(Type::unknown(), |ty| {
            self.infer_expression(ty, TypeContext::default())
        });
        let env = self.program_environment();
        let type_base_exception = KnownClass::BaseException.to_subclass_of(db, env);

        // If it's an `except*` handler, this won't actually be the type of the bound symbol;
        // it will actually be the type of the generic parameters to `BaseExceptionGroup` or `ExceptionGroup`.
        let symbol_ty = if let Some(tuple_spec) = node_ty.tuple_instance_spec(db, env) {
            let mut builder = UnionBuilder::new(db, env);
            let mut invalid_elements = vec![];

            for (index, element) in tuple_spec.iter_element_types(self.db()).enumerate() {
                builder.add_in_place(if element.is_assignable_to(db, env, type_base_exception) {
                    element.to_instance_approximation(db, env).expect(
                        "`Type::to_instance()` should always return `Some()` \
                                if called on a type assignable to `type[BaseException]`",
                    )
                } else {
                    invalid_elements.push((index, element));
                    Type::unknown()
                });
            }

            if !invalid_elements.is_empty()
                && let Some(node) = node
            {
                if let ast::Expr::Tuple(tuple) = node
                    && let Some(tuple_length) = tuple_spec.len().into_fixed_length()
                    && let Some(elements) = fixed_sequence_elements(node, tuple_length)
                {
                    let invalid_elements = invalid_elements
                        .iter()
                        .map(|(index, ty)| (&elements[*index], *ty));

                    report_invalid_exception_tuple_caught(
                        &self.context,
                        tuple,
                        node_ty,
                        invalid_elements,
                    );
                } else {
                    report_invalid_exception_caught(&self.context, node, node_ty);
                }
            }

            builder.build()
        } else if let Some(symbol_ty) =
            self.exception_handler_symbol_ty_from_valid_ty(node_ty, type_base_exception)
        {
            symbol_ty
        } else if node_ty.is_assignable_to(
            db,
            env,
            UnionType::from_two_elements(
                db,
                env,
                type_base_exception,
                Type::homogeneous_tuple(db, env, type_base_exception),
            ),
        ) {
            // TODO: Handle valid handler expressions that are opaque to the structural helper
            // above, for example a type variable bounded by the full class-or-tuple union.
            KnownClass::BaseException.to_instance(db, env)
        } else {
            if let Some(node) = node {
                report_invalid_exception_caught(&self.context, node, node_ty);
            }
            Type::unknown()
        };

        if is_star {
            let class =
                if symbol_ty.is_subtype_of(db, env, KnownClass::Exception.to_instance(db, env)) {
                    KnownClass::ExceptionGroup
                } else {
                    KnownClass::BaseExceptionGroup
                };
            class.to_specialized_instance(db, env, &[symbol_ty])
        } else {
            symbol_ty
        }
    }

    fn exception_handler_symbol_ty_from_valid_ty(
        &self,
        ty: Type<'db>,
        type_base_exception: Type<'db>,
    ) -> Option<Type<'db>> {
        let db = self.db();
        let env = self.program_environment();

        if let Some(tuple_spec) = ty.tuple_instance_spec(db, env) {
            // `except (ValueError, TypeError) as e:`
            UnionType::try_from_elements(
                db,
                env,
                tuple_spec.iter_element_types(self.db()).map(|element| {
                    if element.is_assignable_to(db, env, type_base_exception) {
                        Some(element.to_instance_approximation(db, env).expect(
                            "`Type::to_instance()` should always return `Some()` \
                                if called on a type assignable to `type[BaseException]`",
                        ))
                    } else {
                        None
                    }
                }),
            )
        } else if ty.is_assignable_to(db, env, type_base_exception) {
            // `except ValueError as e:`
            Some(ty.to_instance_approximation(db, env).expect(
                "`Type::to_instance()` should always return `Some()` \
                    if called on a type assignable to `type[BaseException]`",
            ))
        } else if ty.is_assignable_to(
            db,
            env,
            Type::homogeneous_tuple(db, env, type_base_exception),
        ) {
            // `except exception_types as e:`, where
            // `exception_types: tuple[type[ValueError], ...]`
            Some(
                ty.tuple_instance_spec(db, env)
                    .and_then(|spec| {
                        let specialization = spec
                            .homogeneous_element_type(db, env)
                            .to_instance_approximation(db, env);

                        debug_assert!(specialization.is_some_and(|specialization_type| {
                            specialization_type.is_assignable_to(
                                db,
                                env,
                                KnownClass::BaseException.to_instance(db, env),
                            )
                        }));

                        specialization
                    })
                    .unwrap_or_else(|| KnownClass::BaseException.to_instance(db, env)),
            )
        } else if let Type::Union(union) = ty {
            // `except exception_types as e:`, where
            // `exception_types: type[ValueError] | tuple[type[ValueError], ...]`
            union.try_map(db, env, |element| {
                self.exception_handler_symbol_ty_from_valid_ty(*element, type_base_exception)
            })
        } else {
            None
        }
    }

    fn infer_except_handler_definition(
        &mut self,
        except_handler_definition: &ExceptHandlerDefinitionKind,
        definition: Definition<'db>,
    ) {
        let symbol_ty = self.infer_exception(
            except_handler_definition.handled_exceptions(self.module()),
            except_handler_definition.is_star(),
        );

        self.add_binding(
            except_handler_definition.node(self.module()).into(),
            definition,
        )
        .insert(self, symbol_ty);
    }

    /// Infer the type for a loop header definition.
    ///
    /// The loop header sees all the bindings that originate in the loop and are visible at a
    /// loop-back edge (either the end of the loop body or a `continue` statement). See `struct
    /// LoopHeader` in the semantic index for more on how all this fits together.
    fn infer_loop_header_definition(
        &mut self,
        loop_header_kind: &LoopHeaderDefinitionKind,
        definition: Definition<'db>,
    ) {
        // This cutoff was chosen by benchmarking real isort to keep loop analysis
        // overhead minimal while preserving diagnostics.
        const MAX_EXACT_LOOP_HEADER_REACHABILITY_NODES: usize = 4096;
        let db = self.db();

        let loop_header = loop_header_reachability(self.db(), definition);
        let use_def = self
            .index
            .use_def_map(self.scope().file_scope_id(self.db()));

        // Loop-header types are an approximation point for loop fixpoint analysis. Inferring the
        // exact union of every visible loop-back binding can recursively force inference of large
        // boolean expressions and explode on real-world loops.
        if use_def.reachability_constraints().used_interiors().len()
            > MAX_EXACT_LOOP_HEADER_REACHABILITY_NODES
        {
            self.bindings.insert(definition, Type::unknown());
            return;
        }

        let place = loop_header_kind.place();
        let env = self.program_environment();
        let mut union = UnionBuilder::new(db, env).or_recursively_defined(RecursivelyDefined::Yes);

        for reachable_binding in &loop_header.reachable_bindings {
            let binding_ty = binding_type(db, reachable_binding.definition);
            let narrowed_ty = use_def
                .narrowing_evaluator(reachable_binding.narrowing_constraint)
                .narrow(db, env, binding_ty, place);

            union.add_in_place(narrowed_ty);
        }

        self.bindings.insert(definition, union.build());
    }

    fn infer_nested_bindings_definition(
        &mut self,
        nested_bindings_kind: &NestedBindingsDefinitionKind,
        definition: Definition<'db>,
    ) {
        const MAX_EXACT_NESTED_BINDING_REACHABILITY_NODES: usize = 4096;

        let db = self.db();
        let scope_id = definition.file_scope(db);
        let mut binding_sources = nested_bindings_kind
            .visible_binding_sources(self.index, scope_id)
            .peekable();
        if binding_sources.peek().is_some()
            && self
                .index
                .use_def_map(scope_id)
                .reachability_constraints()
                .used_interiors()
                .len()
                > MAX_EXACT_NESTED_BINDING_REACHABILITY_NODES
        {
            // As with loop header definitions above, use a reachability cutoff to avoid excessive
            // perf costs in complicated projects like `isort`.
            self.bindings.insert(definition, Type::unknown());
            return;
        }

        let recursively_defined = match nested_bindings_kind.execution {
            NestedBindingExecution::Lazy => RecursivelyDefined::Yes,
            NestedBindingExecution::Eager => RecursivelyDefined::No,
        };
        let env = self.program_environment();
        let mut union = UnionBuilder::new(db, env).or_recursively_defined(recursively_defined);
        for bindings in binding_sources {
            if nested_bindings_kind.execution == NestedBindingExecution::Eager {
                // A comprehension can execute repeatedly, so a source that is unreachable in the
                // first modeled iteration may become reachable in a later one. Preserve each
                // source's narrowed type and let the proxy's outer use-def state track boundness.
                for binding in bindings {
                    let DefinitionState::Defined(source) = binding.binding else {
                        continue;
                    };
                    let ty = binding_type(db, source);
                    union.add_in_place(binding.narrowing_constraint.narrow(
                        db,
                        env,
                        ty,
                        source.place(db),
                    ));
                }
                continue;
            }

            let Some(ty) = place_from_bindings_with_reachability_cache(
                db,
                env,
                bindings,
                self.reachability_cache(),
            )
            .place
            .raw_type() else {
                continue;
            };
            union.add_in_place(ty);
        }
        let ty = union.build();
        let ty = match nested_bindings_kind.execution {
            NestedBindingExecution::Lazy => ty,
            NestedBindingExecution::Eager => ty.promote(db, env),
        };
        self.bindings.insert(definition, ty);
    }

    fn infer_match_statement(&mut self, match_statement: &ast::StmtMatch) {
        let db = self.db();
        let ast::StmtMatch {
            range: _,
            node_index: _,
            subject,
            cases,
        } = match_statement;

        self.infer_standalone_expression(subject, TypeContext::default());

        for case in cases {
            let ast::MatchCase {
                range: _,
                node_index: _,
                body,
                pattern,
                guard,
            } = case;
            self.infer_match_pattern(pattern);

            if let Some(guard) = guard.as_deref() {
                let guard_ty = self.infer_standalone_expression(guard, TypeContext::default());

                if let Err(err) = guard_ty.try_bool(db, self.program_environment()) {
                    err.report_diagnostic(&self.context, guard);
                }
            }

            self.infer_body(body);
        }
    }

    fn infer_match_pattern_definition(
        &mut self,
        pattern: &'ast ast::Pattern,
        predicate: PatternPredicate<'db>,
        definition: Definition<'db>,
    ) {
        let ty =
            pattern_success_types(self.db(), predicate).binding_type(definition.place(self.db()));
        self.add_binding(pattern.into(), definition)
            .insert(self, ty);
    }

    fn validate_class_pattern(&mut self, pattern: &ast::PatternMatchClass, cls_ty: Type<'db>) {
        let db = self.db();
        let env = self.program_environment();
        if let Type::SpecialForm(SpecialFormType::CollectionsAbcCallable) = cls_ty {
            if let Some(first_excess_pattern) = pattern.arguments.patterns.first() {
                report_too_many_positional_patterns_for_class_pattern(
                    &self.context,
                    first_excess_pattern,
                    0,
                    pattern.arguments.patterns.len(),
                    "collections.abc.Callable",
                );
            }
            return;
        }

        if let Type::ClassLiteral(class) = cls_ty {
            if class.is_typed_dict(self.db()) {
                report_match_pattern_against_typed_dict(&self.context, &*pattern.cls, class);
                return;
            }
            if let Some(protocol_class) = class.into_protocol_class(self.db())
                && !protocol_class.is_runtime_checkable(self.db())
            {
                report_match_pattern_against_non_runtime_checkable_protocol(
                    &self.context,
                    &*pattern.cls,
                    protocol_class,
                );
                return;
            }

            let positional_patterns = &pattern.arguments.patterns;
            if let [first_positional_pattern, ..] = positional_patterns.as_slice()
                && let Some(result) = class_pattern_positional_result(db, env, class)
            {
                match result {
                    ClassPatternPositionalResult::Limit(limit) => {
                        if let Some(first_excess_pattern) = positional_patterns.get(limit) {
                            report_too_many_positional_patterns_for_class_pattern(
                                &self.context,
                                first_excess_pattern,
                                limit,
                                positional_patterns.len(),
                                cls_ty.display(db, env),
                            );
                        }
                    }
                    ClassPatternPositionalResult::InvalidType(match_args_ty) => {
                        report_invalid_match_args_type(
                            &self.context,
                            first_positional_pattern,
                            match_args_ty,
                            cls_ty,
                        );
                    }
                }
            }
        } else if !cls_ty.is_assignable_to(db, env, KnownClass::Type.to_instance(db, env)) {
            report_invalid_class_match_pattern(&self.context, &*pattern.cls, cls_ty);
        }
    }

    fn infer_match_pattern(&mut self, pattern: &ast::Pattern) {
        // We need to create a standalone expression for each arm of a match statement, since they
        // can introduce constraints on the match subject. (Or more accurately, for the match arm's
        // pattern, since its the pattern that introduces any constraints, not the body.) Ideally,
        // that standalone expression would wrap the match arm's pattern as a whole. But a
        // standalone expression can currently only wrap an ast::Expr, which patterns are not. So,
        // we need to choose an Expr that can “stand in” for the pattern, which we can wrap in a
        // standalone expression.
        //
        // The structural pattern is stored separately on `PatternPredicate`, so analyses that need
        // the complete pattern can inspect its arguments without making them standalone
        // expressions.
        //
        // This function is only called for the top-level pattern of a match arm, and is
        // responsible for inferring the standalone expression for each supported pattern type. It
        // then hands off to `infer_nested_match_pattern` for any subexpressions and subpatterns,
        // where we do NOT have any additional standalone expressions to infer through.
        //
        match pattern {
            ast::Pattern::MatchValue(match_value) => {
                self.infer_standalone_expression(&match_value.value, TypeContext::default());
            }
            ast::Pattern::MatchClass(match_class) => {
                let ast::PatternMatchClass {
                    range: _,
                    node_index: _,
                    cls,
                    arguments,
                } = match_class;
                for pattern in &arguments.patterns {
                    self.infer_nested_match_pattern(pattern);
                }
                for keyword in &arguments.keywords {
                    self.infer_nested_match_pattern(&keyword.pattern);
                }
                let cls_ty = self.infer_standalone_expression(cls, TypeContext::default());
                self.validate_class_pattern(match_class, cls_ty);
            }
            ast::Pattern::MatchOr(match_or) => {
                for pattern in &match_or.patterns {
                    self.infer_match_pattern(pattern);
                }
            }
            _ => {
                self.infer_nested_match_pattern(pattern);
            }
        }
    }

    fn infer_nested_match_pattern(&mut self, pattern: &ast::Pattern) {
        match pattern {
            ast::Pattern::MatchValue(match_value) => {
                self.infer_maybe_standalone_expression(&match_value.value, TypeContext::default());
            }
            ast::Pattern::MatchSequence(match_sequence) => {
                for pattern in &match_sequence.patterns {
                    self.infer_nested_match_pattern(pattern);
                }
            }
            ast::Pattern::MatchMapping(match_mapping) => {
                let ast::PatternMatchMapping {
                    range: _,
                    node_index: _,
                    keys,
                    patterns,
                    rest,
                } = match_mapping;
                for key in keys {
                    self.infer_maybe_standalone_expression(key, TypeContext::default());
                }
                for pattern in patterns {
                    self.infer_nested_match_pattern(pattern);
                }
                if let Some(rest) = rest {
                    self.infer_definition(rest);
                }
            }
            ast::Pattern::MatchClass(match_class) => {
                let ast::PatternMatchClass {
                    range: _,
                    node_index: _,
                    cls,
                    arguments,
                } = match_class;
                for pattern in &arguments.patterns {
                    self.infer_nested_match_pattern(pattern);
                }
                for keyword in &arguments.keywords {
                    self.infer_nested_match_pattern(&keyword.pattern);
                }
                let cls_ty = self.infer_maybe_standalone_expression(cls, TypeContext::default());
                self.validate_class_pattern(match_class, cls_ty);
            }
            ast::Pattern::MatchAs(match_as) => {
                if let Some(pattern) = &match_as.pattern {
                    self.infer_nested_match_pattern(pattern);
                }
                if let Some(name) = &match_as.name {
                    self.infer_definition(name);
                }
            }
            ast::Pattern::MatchOr(match_or) => {
                for pattern in &match_or.patterns {
                    self.infer_nested_match_pattern(pattern);
                }
            }
            ast::Pattern::MatchStar(match_star) => {
                if let Some(name) = &match_star.name {
                    self.infer_definition(name);
                }
            }
            ast::Pattern::MatchSingleton(_) => {}
        }
    }

    fn infer_assignment_statement(&mut self, assignment: &ast::StmtAssign) {
        let Ok(()) = assignment::statement::infer_assignment_statement_sync(
            self,
            assignment,
            assignment::statement::AssignmentStatementFacts,
            &assignment::statement::OrdinaryAssignmentStatementEffects,
        );
    }

    fn infer_unpacked_assignment_target(
        &mut self,
        target: &ast::Expr,
        value: &ast::Expr,
        unpacked: &UnpackResult<'db>,
    ) {
        match target {
            ast::Expr::Starred(ast::ExprStarred { value: target, .. }) => {
                self.infer_unpacked_assignment_target(target, value, unpacked);
            }
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                for target in elts {
                    self.infer_unpacked_assignment_target(target, value, unpacked);
                }
            }
            _ => {
                let assigned_ty = unpacked.expression_type(target);
                self.infer_target_impl(target, value, Some(&|_, _| assigned_ty));
            }
        }
    }

    /// Infer the (definition) types involved in a `target` expression.
    ///
    /// This is used for assignment statements, for statements, etc. with a single or multiple
    /// targets (unpacking). If `target` is an attribute expression, we check that the assignment
    /// is valid. For 'target's that are definitions, this check happens elsewhere.
    ///
    /// The `infer_value_expr` function is used to infer the type of the `value` expression which
    /// are not `Name` expressions. The returned type is the one that is eventually assigned to the
    /// `target`.
    fn infer_target(
        &mut self,
        target: &ast::Expr,
        value: &ast::Expr,
        infer_value_expr: &dyn Fn(&mut Self, TypeContext<'db>) -> Type<'db>,
    ) {
        match target {
            ast::Expr::Name(_) => {
                self.infer_target_impl(target, value, None);
            }

            _ => self.infer_target_impl(target, value, Some(&infer_value_expr)),
        }
    }

    /// Returns `true` if `property_ty` is a property whose deleter returns `Never`/`NoReturn`
    /// when called for deletion on `object_ty`.
    fn property_deleter_returns_never(&self, property_ty: Type<'db>, object_ty: Type<'db>) -> bool {
        let env = self.program_environment();
        let db = self.db();
        property_ty.as_property_instance().is_some_and(|property| {
            property.deleter(db).is_some_and(|deleter| {
                match deleter.try_call(db, env, &CallArguments::positional([object_ty])) {
                    Ok(result) => result.return_type(db, env).is_never(),
                    Err(err) => err.return_type(db, env).is_never(),
                }
            })
        })
    }

    fn validate_attribute_deletion(
        &mut self,
        target: &ast::ExprAttribute,
        object_ty: Type<'db>,
        attribute: &str,
        emit_diagnostics: bool,
    ) -> bool {
        let env = self.program_environment();
        let db = self.db();

        match object_ty {
            Type::Recursive(recursive) => recursive.unfold(db, env).is_unchanged_or(|unfolded| {
                self.validate_attribute_deletion(target, unfolded, attribute, emit_diagnostics)
            }),
            Type::RecursiveVar(_) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            Type::Union(union) => {
                for element_ty in union.elements(db) {
                    if !self.validate_attribute_deletion(
                        target,
                        *element_ty,
                        attribute,
                        emit_diagnostics,
                    ) {
                        return false;
                    }
                }
                true
            }

            Type::Intersection(intersection) => {
                let positive = intersection.positive(db);
                if positive.iter().any(|element_ty| {
                    self.validate_attribute_deletion(target, *element_ty, attribute, false)
                }) {
                    true
                } else {
                    if emit_diagnostics && let Some(element_ty) = positive.first() {
                        self.validate_attribute_deletion(target, *element_ty, attribute, true);
                    }
                    false
                }
            }

            Type::EnumComplement(complement) => self.validate_attribute_deletion(
                target,
                complement.remaining_literal_union(db, env),
                attribute,
                emit_diagnostics,
            ),

            // Type aliases need their own arm so aliased unions and intersections reuse the
            // specialized handling above. `NewType` instances don't: dunder lookup and attribute
            // fallback already delegate through the concrete base type when needed.
            Type::TypeAlias(alias) => self.validate_attribute_deletion(
                target,
                alias.value_type(db),
                attribute,
                emit_diagnostics,
            ),

            Type::NominalInstance(..)
            | Type::ProtocolInstance(_)
            | Type::LiteralValue(..)
            | Type::SpecialForm(..)
            | Type::ClassLiteral(..)
            | Type::GenericAlias(..)
            | Type::SubclassOf(..)
            | Type::KnownInstance(..)
            | Type::PropertyInstance(..)
            | Type::SlotDescriptor(..)
            | Type::FunctionLiteral(..)
            | Type::Callable(..)
            | Type::BoundMethod(_)
            | Type::KnownBoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::TypeVar(..)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::TypedDict(_)
            | Type::NewTypeInstance(_) => {
                let frozen_dataclass_dispatch = object_ty
                    .nominal_class(db, env)
                    .and_then(|class| class.static_class_literal(db))
                    .and_then(|(class, specialization)| {
                        class.inherited_frozen_dataclass_dispatch(
                            db,
                            specialization,
                            "__delattr__",
                            attribute,
                        )
                    });

                let delattr_receiver = frozen_dataclass_dispatch
                    .map_or(object_ty, |dispatch| dispatch.receiver(db, env, object_ty));

                let mut delattr_arguments =
                    CallArguments::positional([Type::string_literal(db, attribute)]);
                let delattr_dunder_call_result = if matches!(delattr_receiver, Type::BoundSuper(_))
                {
                    match delattr_receiver
                        .member_lookup_with_policy(
                            db,
                            env,
                            "__delattr__",
                            MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                        )
                        .place
                    {
                        Place::Defined(DefinedPlace {
                            ty: delattr,
                            definedness,
                            provenance,
                            ..
                        }) => match delattr.try_call(db, env, &delattr_arguments) {
                            Ok(bindings) if definedness == Definedness::PossiblyUndefined => {
                                Err(CallDunderError::PossiblyUnbound {
                                    bindings: Box::new(bindings),
                                    unbound_on: None,
                                })
                            }
                            Ok(bindings) => Ok(bindings),
                            Err(CallError(kind, bindings)) => {
                                Err(CallDunderError::CallError(kind, bindings, provenance))
                            }
                        },
                        Place::Undefined => Err(CallDunderError::MethodNotAvailable),
                    }
                } else {
                    delattr_receiver.try_call_dunder_with_policy(
                        db,
                        env,
                        "__delattr__",
                        &mut delattr_arguments,
                        TypeContext::default(),
                        MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                    )
                };

                let returns_never = matches!(
                    frozen_dataclass_dispatch,
                    Some(FrozenDataclassDispatch::FrozenField)
                ) || match &delattr_dunder_call_result {
                    Ok(result) => result.return_type(db, env).is_never(),
                    Err(err) => err.return_type(db, env).is_some_and(|ty| ty.is_never()),
                };
                if returns_never {
                    if emit_diagnostics
                        && let Some(builder) = self.context.report_lint(&INVALID_ASSIGNMENT, target)
                    {
                        builder.into_diagnostic(format_args!(
                            "Cannot delete attribute `{attribute}` on type `{}` \
                             whose `__delattr__` method returns `Never`/`NoReturn`",
                            object_ty.display(db, env),
                        ));
                    }
                    return false;
                }

                match delattr_dunder_call_result {
                    Ok(_) | Err(CallDunderError::PossiblyUnbound { .. })
                        if !matches!(
                            frozen_dataclass_dispatch,
                            Some(FrozenDataclassDispatch::Delegate(_))
                        ) =>
                    {
                        if self.validate_final_attribute_deletion(
                            target,
                            object_ty,
                            attribute,
                            emit_diagnostics,
                        ) {
                            return false;
                        }
                        return true;
                    }
                    Ok(_) | Err(CallDunderError::PossiblyUnbound { .. }) => {}
                    Err(CallDunderError::CallError(kind, _bindings, _)) => {
                        if emit_diagnostics {
                            report_bad_dunder_delattr_call(
                                &self.context,
                                attribute,
                                object_ty,
                                target,
                                kind == CallErrorKind::BindingError,
                            );
                        }
                        return false;
                    }
                    Err(CallDunderError::MethodNotAvailable) => {}
                }

                if self.validate_final_attribute_deletion(
                    target,
                    object_ty,
                    attribute,
                    emit_diagnostics,
                ) {
                    return false;
                }

                if let Some(PlaceAndQualifiers {
                    place:
                        Place::Defined(DefinedPlace {
                            ty: attr_ty,
                            definedness: Definedness::AlwaysDefined,
                            ..
                        }),
                    ..
                }) = assignment_attribute_members(db, env, object_ty, attribute)
                    .and_then(AssignmentAttributeMembers::type_member)
                {
                    let attr_ty = attr_ty.bind_self_typevars(db, env, object_ty);
                    let delete_dunder_call_result = attr_ty.try_call_dunder(
                        db,
                        env,
                        "__delete__",
                        CallArguments::positional([object_ty]),
                        TypeContext::default(),
                    );

                    // `Never` supports arbitrary operations only because there can be no runtime
                    // value to mutate; it is not a concrete descriptor with a terminal deleter.
                    let deleter_returns_never = !attr_ty.is_never()
                        && match &delete_dunder_call_result {
                            Ok(bindings) => bindings.return_type(db, env).is_never(),
                            Err(error) => {
                                error.return_type(db, env).is_some_and(|ty| ty.is_never())
                            }
                        };
                    if deleter_returns_never
                        || self.property_deleter_returns_never(attr_ty, object_ty)
                    {
                        if emit_diagnostics
                            && let Some(builder) =
                                self.context.report_lint(&INVALID_ASSIGNMENT, target)
                        {
                            builder.into_diagnostic(format_args!(
                                "Cannot delete attribute `{attribute}` on type `{}` \
                                 whose `__delete__` method returns `Never`/`NoReturn`",
                                object_ty.display(db, env),
                            ));
                        }
                        return false;
                    }

                    match delete_dunder_call_result {
                        Ok(_) | Err(CallDunderError::PossiblyUnbound { .. }) => return true,
                        Err(CallDunderError::CallError(kind, bindings, _)) => {
                            if emit_diagnostics {
                                let failure = CallError(kind, bindings);
                                report_bad_dunder_delete_call(
                                    &self.context,
                                    &failure,
                                    attribute,
                                    object_ty,
                                    target,
                                );
                            }
                            return false;
                        }
                        Err(CallDunderError::MethodNotAvailable) => {}
                    }
                }

                true
            }

            Type::Dynamic(..)
            | Type::Divergent(_)
            | Type::Never
            | Type::ModuleLiteral(..)
            | Type::BoundSuper(..) => true,
        }
    }

    #[expect(clippy::type_complexity)]
    fn infer_target_impl(
        &mut self,
        target: &ast::Expr,
        value: &ast::Expr,
        infer_assigned_ty: Option<&dyn Fn(&mut Self, TypeContext<'db>) -> Type<'db>>,
    ) {
        let db = self.db();
        match target {
            ast::Expr::Name(name) => {
                if let Some(infer_assigned_ty) = infer_assigned_ty {
                    infer_assigned_ty(self, TypeContext::default());
                }

                self.infer_definition(name);
            }
            ast::Expr::Starred(ast::ExprStarred {
                value: starred_value,
                ..
            }) => {
                self.infer_target_impl(starred_value, value, infer_assigned_ty);
            }
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                let assigned_ty = infer_assigned_ty.map(|f| f(self, TypeContext::default()));

                if let Some(tuple_spec) = assigned_ty
                    .and_then(|ty| ty.tuple_instance_spec(db, self.program_environment()))
                {
                    let assigned_tys = tuple_spec.iter_element_types(self.db()).collect::<Vec<_>>();

                    for (i, element) in elts.iter().enumerate() {
                        match assigned_tys.get(i).copied() {
                            None => self.infer_target_impl(element, value, None),
                            Some(ty) => self.infer_target_impl(element, value, Some(&|_, _| ty)),
                        }
                    }
                } else {
                    for element in elts {
                        self.infer_target_impl(element, value, None);
                    }
                }
            }
            ast::Expr::Attribute(
                attr_expr @ ast::ExprAttribute {
                    value: object,
                    ctx: ExprContext::Store,
                    attr,
                    ..
                },
            ) => {
                let object_ty = self.infer_expression(object, TypeContext::default());
                self.report_undeclared_protocol_attribute(attr_expr);

                if let Some(infer_assigned_ty) = infer_assigned_ty {
                    let infer_assigned_ty = &mut |builder: &mut Self, tcx| {
                        let assigned_ty = infer_assigned_ty(builder, tcx);
                        builder.store_expression_type(target, assigned_ty);
                        assigned_ty
                    };

                    self.validate_attribute_assignment(
                        attr_expr,
                        value,
                        object_ty,
                        attr.id(),
                        infer_assigned_ty,
                        true,
                    );
                }
            }
            ast::Expr::Subscript(subscript_expr) => {
                if let Some(infer_assigned_ty) = infer_assigned_ty {
                    let object_ty =
                        self.infer_expression(&subscript_expr.value, TypeContext::default());
                    let mut infer_slice_ty = |builder: &mut Self, tcx| {
                        builder.infer_expression(&subscript_expr.slice, tcx)
                    };
                    let infer_assigned_ty = &mut |builder: &mut Self, tcx| {
                        let assigned_ty = infer_assigned_ty(builder, tcx);
                        builder.store_expression_type(target, assigned_ty);
                        assigned_ty
                    };

                    self.validate_subscript_assignment(
                        subscript_expr,
                        value,
                        object_ty,
                        &mut infer_slice_ty,
                        infer_assigned_ty,
                    );
                }
            }

            // TODO: Remove this once we handle all possible assignment targets.
            _ => {
                if let Some(infer_assigned_ty) = infer_assigned_ty {
                    infer_assigned_ty(self, TypeContext::default());
                }

                self.infer_expression(target, TypeContext::default());
            }
        }
    }

    fn stub_placeholder_binding_type(in_stub: bool, value: &ast::Expr) -> Option<Type<'db>> {
        if in_stub && value.is_ellipsis_literal_expr() {
            Some(Type::unknown())
        } else {
            None
        }
    }

    fn infer_newtype_expression(
        &mut self,
        target: &ast::Expr,
        call_expr: &ast::ExprCall,
        definition: Definition<'db>,
    ) -> Type<'db> {
        fn error<'db>(
            context: &InferContext<'db, '_>,
            message: impl std::fmt::Display,
            node: impl Ranged,
        ) -> Type<'db> {
            if let Some(builder) = context.report_lint(&INVALID_NEWTYPE, node) {
                builder.into_diagnostic(message);
            }
            Type::unknown()
        }

        let db = self.db();
        let arguments = &call_expr.arguments;

        if !arguments.keywords.is_empty() {
            return error(
                &self.context,
                "Keyword arguments are not supported in `NewType` creation",
                call_expr,
            );
        }

        if let Some(starred) = arguments.args.iter().find(|arg| arg.is_starred_expr()) {
            return error(
                &self.context,
                "Starred arguments are not supported in `NewType` creation",
                starred,
            );
        }

        if arguments.args.len() != 2 {
            return error(
                &self.context,
                format!(
                    "Wrong number of arguments in `NewType` creation: expected 2, found {}",
                    arguments.args.len()
                ),
                call_expr,
            );
        }

        let name_param_ty = self.infer_expression(&arguments.args[0], TypeContext::default());

        let Some(name) = name_param_ty.as_string_literal().map(|name| name.value(db)) else {
            return error(
                &self.context,
                "The first argument to `NewType` must be a string literal",
                call_expr,
            );
        };

        let ast::Expr::Name(ast::ExprName {
            id: target_name, ..
        }) = target
        else {
            return error(
                &self.context,
                "A `NewType` definition must be a simple variable assignment",
                target,
            );
        };

        if name != target_name {
            report_mismatched_type_name(
                &self.context,
                &arguments.args[0],
                "NewType",
                target_name,
                Some(name),
                name_param_ty,
            );
        }

        // Inference of `tp` must be deferred, to avoid cycles.
        self.deferred.insert(definition);

        Type::KnownInstance(KnownInstanceType::NewType(NewType::new(
            db, name, definition, None,
        )))
    }

    fn infer_sentinel_expression(
        &mut self,
        target: &ast::Expr,
        call_expr: &ast::ExprCall,
        definition: Definition<'db>,
    ) -> Option<Type<'db>> {
        if !self.sentinel_definition_scope_is_supported() {
            return None;
        }

        let ast::Expr::Name(ast::ExprName {
            id: target_name, ..
        }) = target
        else {
            return None;
        };

        let ast::Arguments {
            args,
            keywords,
            range: _,
            node_index: _,
        } = &call_expr.arguments;

        if args.iter().any(ast::Expr::is_starred_expr) {
            return None;
        }

        let (name_arg, mut repr_arg) = match &**args {
            [name_arg] => (name_arg, None),
            [name_arg, repr_arg] => (name_arg, Some(repr_arg)),
            _ => return None,
        };

        for keyword in keywords {
            let Some(keyword_name) = &keyword.arg else {
                return None;
            };

            if keyword_name.as_str() != "repr" || repr_arg.is_some() {
                return None;
            }

            repr_arg = Some(&keyword.value);
        }

        if !matches!(name_arg, ast::Expr::StringLiteral(_)) {
            return None;
        }

        let Some(repr_arg) = repr_arg else {
            return Some(Type::KnownInstance(KnownInstanceType::Sentinel(
                SentinelInstance::new(self.db(), target_name, definition),
            )));
        };

        if !matches!(repr_arg, ast::Expr::StringLiteral(_)) && !repr_arg.is_none_literal_expr() {
            return None;
        }

        Some(Type::KnownInstance(KnownInstanceType::Sentinel(
            SentinelInstance::new(self.db(), target_name, definition),
        )))
    }

    fn sentinel_definition_scope_is_supported(&self) -> bool {
        let db = self.db();
        let mut scope_id = self.scope.file_scope_id(db);

        loop {
            let scope = self.index.scope(scope_id);
            match scope.node().scope_kind() {
                ScopeKind::Module => return true,
                ScopeKind::Class => {}
                ScopeKind::Function
                | ScopeKind::Lambda
                | ScopeKind::Comprehension
                | ScopeKind::TypeAlias
                | ScopeKind::TypeParams => return false,
            }

            let Some(parent) = scope.parent() else {
                return false;
            };
            scope_id = parent;
        }
    }

    fn infer_assignment_deferred(&mut self, target: &ast::Expr, value: &'ast ast::Expr) {
        let Ok(()) = deferred::assignment::infer_assignment_deferred_sync(
            self,
            target,
            value,
            deferred::assignment::DeferredAssignmentFacts,
            &deferred::assignment::OrdinaryDeferredAssignmentEffects,
        );
    }

    // Infer the deferred base type of a NewType.
    fn infer_newtype_assignment_deferred(&mut self, arguments: &ast::Arguments) {
        let db = self.db();
        let env = self.program_environment();
        let inferred = self.infer_type_expression(&arguments.args[1]);

        if inferred.has_typevar_or_typevar_instance(db, env) {
            if let Some(builder) = self
                .context
                .report_lint(&INVALID_NEWTYPE, &arguments.args[1])
            {
                let mut diag = builder.into_diagnostic("invalid base for `typing.NewType`");
                diag.set_primary_annotation_message("A `NewType` base cannot be generic");
            }
            return;
        }

        match inferred {
            Type::NewTypeInstance(_) | Type::NominalInstance(_) => return,
            // There are exactly two union types allowed as bases for NewType: `int | float` and
            // `int | float | complex`. These are allowed because that's what `float` and `complex`
            // expand into in type position. We don't currently ask whether the union was implicit
            // or explicit, so the explicit version is also allowed.
            Type::Union(union_ty) => {
                if let Some(KnownUnion::Float | KnownUnion::Complex) = union_ty.known(self.db()) {
                    return;
                }
            }
            // `Unknown` is likely to be the result of an unresolved import or a typo, which will
            // already get a diagnostic, so don't pile on an extra diagnostic here.
            Type::Dynamic(DynamicType::Unknown) => return,
            _ => {}
        }
        if let Some(builder) = self
            .context
            .report_lint(&INVALID_NEWTYPE, &arguments.args[1])
        {
            let mut diag = builder.into_diagnostic("invalid base for `typing.NewType`");
            diag.set_primary_annotation_message(format!("type `{}`", inferred.display(db, env)));
            if matches!(inferred, Type::ProtocolInstance(_)) {
                diag.info("The base of a `NewType` is not allowed to be a protocol class.");
            } else if matches!(inferred, Type::TypedDict(_)) {
                diag.info("The base of a `NewType` is not allowed to be a `TypedDict`.");
            } else {
                diag.info("The base of a `NewType` must be a class type or another `NewType`.");
            }
        }
    }

    /// Infer a `TypeAliasType("Name", value)` call in a simple assignment context.
    ///
    /// Follows the same pattern as [`Self::infer_newtype_expression`]: validates the
    /// arguments, constructs a [`ManualPEP695TypeAliasType`], and defers inference of
    /// the value argument.
    fn infer_typealiastype_call(
        &mut self,
        target: &ast::Expr,
        call_expr: &ast::ExprCall,
        definition: Definition<'db>,
        typing_module: TypingModule,
    ) -> Type<'db> {
        fn error<'db>(
            context: &InferContext<'db, '_>,
            message: impl std::fmt::Display,
            node: impl Ranged,
        ) -> Type<'db> {
            if let Some(builder) = context.report_lint(&INVALID_TYPE_ALIAS_TYPE, node) {
                builder.into_diagnostic(message);
            }
            Type::unknown()
        }

        let db = self.db();
        let arguments = &call_expr.arguments;

        if let Some(starred) = arguments.args.iter().find(|arg| arg.is_starred_expr()) {
            return error(
                &self.context,
                "Starred arguments are not supported in `TypeAliasType` creation",
                starred,
            );
        }

        if arguments.args.len() != 2 {
            return error(
                &self.context,
                format_args!(
                    "Wrong number of arguments in `TypeAliasType` creation: expected 2, found {}",
                    arguments.args.len()
                ),
                call_expr,
            );
        }

        let name_param_ty = self.infer_expression(&arguments.args[0], TypeContext::default());

        let Some(name) = name_param_ty.as_string_literal().map(|name| name.value(db)) else {
            return error(
                &self.context,
                "The first argument to `TypeAliasType` must be a string literal",
                &arguments.args[0],
            );
        };

        let ast::Expr::Name(ast::ExprName {
            id: target_name, ..
        }) = target
        else {
            return error(
                &self.context,
                "A `TypeAliasType` definition must be a simple variable assignment",
                target,
            );
        };

        if name != target_name {
            report_mismatched_type_name(
                &self.context,
                &arguments.args[0],
                "TypeAliasType",
                target_name,
                Some(name),
                name_param_ty,
            );
        }

        // Inference of the value argument must be deferred, to avoid cycles.
        self.deferred.insert(definition);

        Type::KnownInstance(KnownInstanceType::TypeAliasType(
            TypeAliasType::ManualPEP695(ManualPEP695TypeAliasType::new(
                db,
                name,
                definition,
                typing_module,
                None,
                None,
            )),
        ))
    }

    /// Infer the deferred value type of a `TypeAliasType`.
    fn infer_typealiastype_assignment_deferred(
        &mut self,
        definition: Definition<'db>,
        target: &ast::Expr,
        arguments: &ast::Arguments,
    ) {
        let db = self.db();
        // Match the binding context used by eager assignment inference so legacy type variables
        // in the alias value are bound to the alias definition.
        let previous_context = self.typevar_binding_context.replace(definition);

        let value_ty = self.infer_type_expression(&arguments.args[1]);
        let mut type_params = FxHashSet::default();
        let mut valid_type_params = true;
        // Infer keyword arguments (e.g. `type_params`) so their types are stored.
        for keyword in &arguments.keywords {
            self.infer_expression(&keyword.value, TypeContext::default());

            if keyword.arg.as_deref() != Some("type_params") {
                continue;
            }

            let Some(tuple) = keyword.value.as_tuple_expr() else {
                valid_type_params = false;
                if let Some(builder) = self
                    .context
                    .report_lint(&INVALID_TYPE_ALIAS_TYPE, &keyword.value)
                {
                    builder.into_diagnostic(
                        "The `type_params` argument to `TypeAliasType` must be a tuple literal",
                    );
                }
                continue;
            };

            let db = self.db();
            let mut typevar_with_default = None;
            let mut typevar_tuple: Option<TypeVarInstance> = None;
            let mut reported_default_order_error = false;

            for element in &tuple.elts {
                let bound_typevar = match self.expression_type(element) {
                    Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) => bind_typevar(
                        self.db(),
                        self.index,
                        definition.file_scope(db),
                        Some(definition),
                        typevar,
                    ),
                    _ => None,
                };
                let Some(bound_typevar) = bound_typevar else {
                    valid_type_params = false;
                    if let Some(builder) =
                        self.context.report_lint(&INVALID_TYPE_ALIAS_TYPE, element)
                    {
                        builder.into_diagnostic(
                            "Each `type_params` entry for `TypeAliasType` must be a type variable",
                        );
                    }
                    continue;
                };
                let typevar = bound_typevar.typevar(db);

                if bound_typevar.binding_context(db) != BindingContext::Definition(definition) {
                    valid_type_params = false;
                    if let Some(builder) =
                        self.context.report_lint(&INVALID_TYPE_ALIAS_TYPE, element)
                    {
                        builder.into_diagnostic(format_args!(
                            "Type parameter `{}` is bound in an outer scope \
                            and cannot be used in `type_params`",
                            typevar.name(db),
                        ));
                    }
                    continue;
                }

                if !type_params.insert(bound_typevar.identity(db)) {
                    valid_type_params = false;
                    if let Some(builder) =
                        self.context.report_lint(&INVALID_TYPE_ALIAS_TYPE, element)
                    {
                        builder.into_diagnostic(format_args!(
                            "Type parameter `{}` is duplicated in `type_params`",
                            typevar.name(db),
                        ));
                    }
                }

                if typevar
                    .default_type(db, self.program_environment())
                    .is_some()
                {
                    if let Some(typevar_tuple) = typevar_tuple {
                        valid_type_params = false;
                        if let Some(builder) = self
                            .context
                            .report_lint(&INVALID_TYPE_VARIABLE_DEFAULT, element)
                        {
                            builder.into_diagnostic(format_args!(
                                "Type parameter `{}` with a default follows TypeVarTuple `{}`",
                                typevar.name(db),
                                typevar_tuple.name(db),
                            ));
                        }
                    }
                    typevar_with_default.get_or_insert(typevar);
                } else if let Some(typevar_with_default) = typevar_with_default {
                    valid_type_params = false;
                    if !reported_default_order_error
                        && let Some(builder) = self
                            .context
                            .report_lint(&INVALID_TYPE_VARIABLE_DEFAULT, element)
                    {
                        reported_default_order_error = true;
                        builder.into_diagnostic(format_args!(
                            "Type parameter `{}` without a default \
                            cannot follow earlier parameter `{}` with a default",
                            typevar.name(db),
                            typevar_with_default.name(db),
                        ));
                    }
                }

                if typevar.is_typevartuple(db) {
                    if typevar_tuple.is_some() {
                        valid_type_params = false;
                        if let Some(builder) =
                            self.context.report_lint(&INVALID_TYPE_ALIAS_TYPE, element)
                        {
                            builder.into_diagnostic(
                                "Only one `TypeVarTuple` parameter is allowed in `type_params`",
                            );
                        }
                    } else {
                        typevar_tuple = Some(typevar);
                    }
                }
            }
        }

        if valid_type_params {
            let mut value_typevars = FxOrderSet::default();
            value_ty.find_legacy_typevars(
                db,
                self.program_environment(),
                Some(definition),
                &mut value_typevars,
            );

            for typevar in value_typevars {
                if !type_params.contains(&typevar.identity(self.db()))
                    && let Some(builder) = self
                        .context
                        .report_lint(&INVALID_TYPE_ALIAS_TYPE, &arguments.args[1])
                {
                    builder.into_diagnostic(format_args!(
                        "Type parameter `{}` used in the alias value \
                        must be included in `type_params`",
                        typevar.name(self.db()),
                    ));
                }
            }
        }

        self.typevar_binding_context = previous_context;
        if let Some(name) = target.as_name_expr() {
            self.check_type_alias_cycle(&name.id, &arguments.args[1]);
        }
    }

    fn is_valid_receiver_annotation_target(&self, target: &ast::Expr) -> bool {
        target
            .as_attribute_expr()
            .is_some_and(|target| self.is_receiver_attribute_annotation_target(target))
    }

    fn infer_annotated_assignment_statement(&mut self, assignment: &ast::StmtAnnAssign) {
        let db = self.db();
        let env = self.program_environment();
        if assignment.target.is_name_expr() {
            self.infer_definition(assignment);
        } else {
            // Non-name assignment targets are inferred as ordinary expressions, not definitions.
            let ast::StmtAnnAssign {
                range: _,
                node_index: _,
                annotation,
                value,
                target,
                simple: _,
            } = assignment;
            let annotated = self.infer_annotation_expression(
                annotation,
                DeferredExpressionState::from(self.defer_annotations()),
            );

            if !annotated.qualifiers.is_empty() {
                for qualifier in TypeQualifier::iter() {
                    if !qualifier.is_valid_for_non_name_targets()
                        && annotated
                            .qualifiers
                            .contains(TypeQualifiers::from(qualifier))
                        && let Some(builder) = self
                            .context
                            .report_lint(&INVALID_TYPE_FORM, annotation.as_ref())
                    {
                        builder.into_diagnostic(format_args!(
                            "`{name}` annotations are not allowed for non-name targets",
                            name = qualifier.name()
                        ));
                    }
                }
            }

            // P.args and P.kwargs are only valid as annotations on *args and **kwargs.
            if let Type::TypeVar(typevar) = annotated.inner_type()
                && typevar.is_paramspec(self.db())
                && let Some(attr) = typevar.paramspec_attr(self.db())
            {
                let name = typevar.name(self.db());
                let (attr_name, variadic) = match attr {
                    ParamSpecAttrKind::Args => ("args", "*args"),
                    ParamSpecAttrKind::Kwargs => ("kwargs", "**kwargs"),
                };
                if let Some(builder) = self
                    .context
                    .report_lint(&INVALID_PARAMSPEC, annotation.as_ref())
                {
                    builder.into_diagnostic(format_args!(
                        "`{name}.{attr_name}` is only valid \
                        for annotating `{variadic}` function parameters",
                    ));
                }
            } else if let ast::Expr::Attribute(attr_expr) = annotation.as_ref()
                && matches!(attr_expr.attr.as_str(), "args" | "kwargs")
            {
                let value_ty = self.expression_type(&attr_expr.value);
                if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = value_ty
                    && typevar.is_paramspec(self.db())
                {
                    let name = typevar.name(self.db());
                    let attr_name = &attr_expr.attr;
                    let variadic = if attr_name == "args" {
                        "*args"
                    } else {
                        "**kwargs"
                    };
                    if let Some(builder) = self
                        .context
                        .report_lint(&INVALID_PARAMSPEC, annotation.as_ref())
                    {
                        builder.into_diagnostic(format_args!(
                            "`{name}.{attr_name}` is only valid \
                            for annotating `{variadic}` function parameters",
                        ));
                    }
                }
            }

            // Disallow annotations on non-name targets unless they are valid receivers (e.g.
            // `self.x: int` or `cls.x: int`).
            if !self.is_valid_receiver_annotation_target(target) {
                let message = match target.as_ref() {
                    ast::Expr::Attribute(_) => {
                        "Type annotations are not allowed on this attribute expression"
                    }
                    ast::Expr::Subscript(_) => {
                        "Type annotations are not allowed on subscripted expressions"
                    }
                    _ => {
                        // For parser-recovered invalid targets, the syntax diagnostic is
                        // sufficient.
                        if let Some(value) = value {
                            self.infer_maybe_standalone_expression(value, TypeContext::default());
                        }
                        self.infer_expression(target, TypeContext::default());
                        return;
                    }
                };

                // For syntactically valid non-name targets, reject the annotation and validate
                // any accompanying assignment.
                if let Some(builder) = self
                    .context
                    .report_lint(&INVALID_TYPE_FORM, annotation.as_ref())
                {
                    builder.into_diagnostic(message);
                }

                if let Some(value) = value {
                    self.infer_target(target, value, &|builder, tcx| {
                        builder.infer_maybe_standalone_expression(value, tcx)
                    });
                } else {
                    self.infer_expression(target, TypeContext::default());
                }
                return;
            }

            let value_ty = value.as_ref().map(|value| {
                self.infer_maybe_standalone_expression(
                    value,
                    TypeContext::new(Some(annotated.inner_type())),
                )
            });

            // If we have an annotated assignment like `self.attr: int = 1`, we still need to
            // do type inference on the `self.attr` target to get types for all sub-expressions.
            self.infer_expression(target, TypeContext::default());
            if let ast::Expr::Attribute(target) = target.as_ref() {
                self.report_undeclared_protocol_attribute(target);
            }

            // For annotated assignments like `self.x: Final[int] = 1`, the `Final` qualifier
            // comes from the annotation itself, so we can check it directly rather than
            // looking up qualifiers from the object type (as `validate_final_attribute_assignment`
            // does for augmented assignments).
            if value.is_some()
                && annotated.qualifiers.contains(TypeQualifiers::FINAL)
                && let ast::Expr::Attribute(attr_expr) = target.as_ref()
            {
                let object_ty = self.expression_type(&attr_expr.value);
                self.invalid_assignment_to_final_attribute(
                    object_ty,
                    attr_expr,
                    attr_expr.attr.id(),
                    annotated.qualifiers,
                );
            }

            // But here we explicitly overwrite the type for the overall `self.attr` node.
            // We do not use `store_expression_type` here, because it checks that no type
            // has been stored for the expression before. When there's a value, use the
            // inferred type (matching the name-target definition path); otherwise fall
            // back to the annotated type. If the value is not assignable to the declared
            // type, report an error and fall back to the annotated type.
            let target_ty = if let Some(value_ty) = value_ty {
                let declared_ty = annotated.inner_type();
                if value_ty.is_assignable_to(db, env, declared_ty) {
                    value_ty
                } else {
                    if let Some(builder) = self
                        .context
                        .report_lint(&INVALID_ASSIGNMENT, value.as_deref().unwrap())
                    {
                        let mut diag = builder.into_diagnostic(format_args!(
                            "Object of type `{}` is not assignable to `{}`",
                            value_ty.display(db, env),
                            declared_ty.display(db, env),
                        ));
                        diag.annotate(
                            self.context
                                .secondary(annotation.as_ref())
                                .message("Declared type"),
                        );
                        diag.set_primary_annotation_message(format_args!(
                            "Incompatible value of type `{}`",
                            value_ty.display(db, env),
                        ));
                    }
                    declared_ty
                }
            } else {
                annotated.inner_type()
            };
            self.expressions.insert((&**target).into(), target_ty);
        }
    }

    /// Infer an annotated assignment's annotation using the file's deferred-annotation semantics.
    fn infer_annotated_assignment_annotation(
        &mut self,
        assignment: &AnnotatedAssignmentDefinitionKind,
    ) -> TypeAndQualifiers<'db> {
        let result = annotated_assignment::infer_annotated_assignment_annotation_sync(
            self,
            assignment,
            &annotated_assignment::OrdinaryAnnotatedAssignmentEffects,
        );
        match result {
            Ok(declared) => declared,
            Err(error) => match error {},
        }
    }

    /// Initialize a declaration cycle without discarding its annotation diagnostics or metadata.
    pub(super) fn infer_annotated_assignment_cycle_initial(
        mut self,
        definition: Definition<'db>,
        assignment: &AnnotatedAssignmentDefinitionKind,
        cycle_recovery: Type<'db>,
    ) -> DefinitionInference<'db> {
        let declared = self.infer_annotated_assignment_annotation(assignment);
        self.declarations.insert(definition, declared);
        self.cycle_recovery = Some(cycle_recovery);
        self.finish_inferred_definition(definition)
    }

    /// Infer the types in an annotated assignment definition.
    fn infer_annotated_assignment_definition(
        &mut self,
        assignment: &'db AnnotatedAssignmentDefinitionKind,
        definition: Definition<'db>,
    ) {
        let result = annotated_assignment::infer_annotated_assignment_definition_sync(
            self,
            assignment,
            definition,
            &annotated_assignment::OrdinaryAnnotatedAssignmentEffects,
        );
        match result {
            Ok(()) => {}
            Err(error) => match error {},
        }
    }

    fn infer_augmented_assignment_statement(&mut self, assignment: &ast::StmtAugAssign) {
        if assignment.target.is_name_expr() {
            self.infer_definition(assignment);
        } else {
            // Non-name assignment targets are inferred as ordinary expressions, not definitions.
            if let Ok(result_ty) = self.infer_augment_assignment(assignment) {
                let target = assignment.target.as_ref();
                match target {
                    ast::Expr::Attribute(attribute) => {
                        let object_ty = self.expression_type(&attribute.value);
                        self.validate_attribute_assignment(
                            attribute,
                            target,
                            object_ty,
                            attribute.attr.id(),
                            &mut |_, _| result_ty,
                            true,
                        );
                    }
                    ast::Expr::Subscript(subscript) => {
                        let object_ty = self.expression_type(&subscript.value);
                        let slice_ty = self.expression_type(&subscript.slice);
                        self.validate_subscript_assignment(
                            subscript,
                            target,
                            object_ty,
                            &mut |_, _| slice_ty,
                            &mut |_, _| result_ty,
                        );
                    }
                    _ => {}
                }
            }

            if let ast::Expr::Attribute(attr_expr) = assignment.target.as_ref() {
                self.report_undeclared_protocol_attribute(attr_expr);
            }
        }
    }

    /// Infer an augmented operator, returning its recovery type if the operation fails.
    fn infer_augmented_op(
        &mut self,
        assignment: &ast::StmtAugAssign,
        target_type: Type<'db>,
        value_expr: &ast::Expr,
        infer_value_ty: &mut dyn FnMut(&mut Self, TypeContext<'db>) -> Type<'db>,
        state: &mut BinaryInferenceState<'db>,
    ) -> Result<Type<'db>, Type<'db>> {
        let db = self.db();
        let env = self.program_environment();
        // If the target defines, e.g., `__iadd__`, infer the augmented assignment as a call to that
        // dunder.
        let op = assignment.op;

        // Fall back to non-augmented binary operator inference.
        let binary_return_ty =
            |builder: &mut Self, value_ty, state: &mut BinaryInferenceState<'db>| {
                builder
                    .infer_binary_expression_type(
                        assignment.into(),
                        target_type,
                        value_ty,
                        op,
                        state,
                    )
                    .ok_or_else(|| {
                        report_unsupported_augmented_assignment(
                            &builder.context,
                            assignment,
                            target_type,
                            value_ty,
                        );
                        Type::unknown()
                    })
            };

        match target_type {
            Type::Union(union) => {
                let mut infer_value_ty = MultiInferenceGuard::new(infer_value_ty);

                // Perform loud inference without type context, as there may be multiple
                // equally applicable type contexts for each union member.
                infer_value_ty.infer_loud(self, TypeContext::default());

                let mut operation_failed = false;
                let Ok(result_ty) = state.try_map_union(db, env, union, |elem_type, state| {
                    let result_ty = match self.infer_augmented_op(
                        assignment,
                        elem_type,
                        value_expr,
                        &mut |builder, tcx| infer_value_ty.infer_silent(builder, tcx),
                        state,
                    ) {
                        Ok(ty) => ty,
                        Err(recovery_ty) => {
                            operation_failed = true;
                            recovery_ty
                        }
                    };
                    Ok::<_, Infallible>(result_ty)
                });

                if operation_failed {
                    Err(result_ty)
                } else {
                    Ok(result_ty)
                }
            }

            _ => {
                if let Some(typed_dict_update_ty) = self
                    .try_infer_typed_dict_pep_584_augmented_assignment(
                        assignment,
                        target_type,
                        value_expr,
                        infer_value_ty,
                    )
                {
                    return Ok(typed_dict_update_ty);
                }

                let ast_arguments = [ArgOrKeyword::Arg(value_expr)];
                let mut call_arguments = CallArguments::positional([Type::unknown()]);

                let call = self.infer_and_try_call_dunder(
                    target_type,
                    op.in_place_dunder(),
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                    ArgumentsIter::synthesized(&ast_arguments),
                    &mut call_arguments,
                    &mut |builder, (_, _, tcx)| infer_value_ty(builder, tcx),
                    TypeContext::default(),
                );
                match call {
                    Ok(outcome) => {
                        state.deprecated_functions.extend(
                            outcome
                                .deprecated_functions(db)
                                .map(|(_, function)| function),
                        );
                        Ok(outcome.return_type(db, env))
                    }
                    Err(CallDunderError::MethodNotAvailable) => {
                        let value_ty = infer_value_ty(self, TypeContext::default());
                        binary_return_ty(self, value_ty, state)
                    }
                    Err(CallDunderError::PossiblyUnbound {
                        bindings: outcome, ..
                    }) => {
                        state.deprecated_functions.extend(
                            outcome
                                .deprecated_functions(db)
                                .map(|(_, function)| function),
                        );
                        let value_ty = outcome.type_for_argument(&call_arguments, 0);
                        match binary_return_ty(self, value_ty, state) {
                            Ok(binary_ty) => Ok(UnionType::from_two_elements(
                                db,
                                env,
                                outcome.return_type(db, env),
                                binary_ty,
                            )),
                            Err(recovery_ty) => Err(UnionType::from_two_elements(
                                db,
                                env,
                                outcome.return_type(db, env),
                                recovery_ty,
                            )),
                        }
                    }
                    Err(CallDunderError::CallError(_, bindings, _)) => {
                        let value_ty = bindings.type_for_argument(&call_arguments, 0);
                        report_unsupported_augmented_assignment(
                            &self.context,
                            assignment,
                            target_type,
                            value_ty,
                        );
                        Err(bindings.return_type(db, env))
                    }
                }
            }
        }
    }

    fn infer_augment_assignment_definition(
        &mut self,
        assignment: &'ast ast::StmtAugAssign,
        definition: Definition<'db>,
    ) {
        let target_ty = self
            .infer_augment_assignment(assignment)
            .unwrap_or_else(|recovery_ty| recovery_ty);
        self.add_binding(assignment.target.as_ref().into(), definition)
            .insert(self, target_ty);
    }

    fn infer_augment_assignment(
        &mut self,
        assignment: &ast::StmtAugAssign,
    ) -> Result<Type<'db>, Type<'db>> {
        let ast::StmtAugAssign {
            range: _,
            node_index: _,
            target,
            op: _,
            value,
        } = assignment;

        // Resolve the target type, assuming a load context.
        let target_result = match &**target {
            ast::Expr::Name(name) => {
                let previous_value = self.infer_name_load(name);
                self.store_expression_type(target, previous_value);
                Ok(previous_value)
            }
            ast::Expr::Attribute(attr) => {
                let result = self
                    .infer_attribute_load(attr)
                    .map(|ty| ty.inner_type())
                    .map_err(|ty| ty.inner_type());
                let previous_value = result.unwrap_or_else(|recovery_ty| recovery_ty);
                self.store_expression_type(target, previous_value);
                result
            }
            ast::Expr::Subscript(subscript) => {
                let result = self.infer_subscript_load(subscript);
                let previous_value = result.unwrap_or_else(|recovery_ty| recovery_ty);
                self.store_expression_type(target, previous_value);
                result
            }
            _ => Ok(self.infer_expression(target, TypeContext::default())),
        };

        let target_type = target_result.unwrap_or_else(|recovery_ty| recovery_ty);
        let mut state = BinaryInferenceState::default();
        let operation_result = self.infer_augmented_op(
            assignment,
            target_type,
            value,
            &mut |builder, tcx| builder.infer_expression(value, tcx),
            &mut state,
        );
        self.report_deprecated_functions(assignment, state.deprecated_functions);

        match (target_result, operation_result) {
            (Ok(_), Ok(result_ty)) => Ok(result_ty),
            (_, Ok(recovery_ty) | Err(recovery_ty)) => Err(recovery_ty),
        }
    }

    fn infer_dict_key_assignment_definition(
        &mut self,
        key: &'ast ast::Expr,
        value: &'ast ast::Expr,
        assignment: Definition<'db>,
        definition: Definition<'db>,
    ) {
        let value_ty = infer_definition_types(self.db(), assignment).expression_type(value);
        self.add_binding(key.into(), definition)
            .insert(self, value_ty);
    }

    fn infer_type_alias_statement(&mut self, node: &ast::StmtTypeAlias) {
        self.infer_definition(node);
    }

    fn fixed_length_iterable_element_type(
        &self,
        iterable: &ast::Expr,
        expression_type: impl FnMut(&ast::Expr) -> Type<'db>,
    ) -> Option<Type<'db>> {
        let db = self.db();
        let env = self.program_environment();
        let element_types =
            extract_fixed_length_iterable_element_types(db, env, iterable, expression_type)?;

        if element_types.is_empty() {
            None
        } else {
            Some(UnionType::from_elements(
                db,
                env,
                element_types.iter().copied(),
            ))
        }
    }

    fn infer_for_statement(&mut self, for_statement: &ast::StmtFor) {
        let db = self.db();
        let ast::StmtFor {
            range: _,
            node_index: _,
            target,
            iter,
            body,
            orelse,
            is_async,
        } = for_statement;

        self.infer_target(target, iter, &|builder, tcx| {
            // TODO: `infer_for_statement_definition` reports a diagnostic if `iter_ty` isn't iterable
            //  but only if the target is a name. We should report a diagnostic here if the target isn't a name:
            //  `for a.x in not_iterable: ...
            let iterable_type = builder.infer_standalone_expression(iter, tcx);
            if !*is_async
                && let Some(element_type) = builder
                    .fixed_length_iterable_element_type(iter, |expr| builder.expression_type(expr))
            {
                element_type
            } else {
                let env = builder.program_environment();
                iterable_type
                    .iterate(db, env)
                    .homogeneous_element_type(db, env)
            }
        });

        self.infer_body(body);
        self.infer_body(orelse);
    }

    fn infer_for_statement_definition(
        &mut self,
        for_stmt: &ForStmtDefinitionKind<'db>,
        definition: Definition<'db>,
    ) {
        let db = self.db();
        let iterable = for_stmt.iterable(self.module());
        let target = for_stmt.target(self.module());

        let loop_var_value_type = match for_stmt.target_kind() {
            TargetKind::Sequence(unpack_position, unpack) => {
                let unpacked = infer_unpack_types(self.db(), unpack);
                if unpack_position == UnpackPosition::First {
                    self.context.extend(unpacked.diagnostics());
                }

                unpacked.expression_type(target)
            }
            TargetKind::Single => {
                let iterable_type =
                    self.infer_standalone_expression(iterable, TypeContext::default());

                if !for_stmt.is_async()
                    && let Some(element_type) = self
                        .fixed_length_iterable_element_type(iterable, |expr| {
                            self.expression_type(expr)
                        })
                {
                    element_type
                } else {
                    let env = self.program_environment();
                    iterable_type
                        .try_iterate_with_mode(
                            db,
                            env,
                            EvaluationMode::from_is_async(for_stmt.is_async()),
                        )
                        .map(|tuple| tuple.homogeneous_element_type(db, env))
                        .unwrap_or_else(|err| {
                            err.report_diagnostic(&self.context, iterable_type, iterable.into());
                            err.fallback_element_type(db, env)
                        })
                }
            }
        };

        self.store_expression_type(target, loop_var_value_type);
        self.add_binding(target.into(), definition)
            .insert(self, loop_var_value_type);
    }

    fn infer_while_statement(&mut self, while_statement: &ast::StmtWhile) {
        let db = self.db();
        let ast::StmtWhile {
            range: _,
            node_index: _,
            test,
            body,
            orelse,
        } = while_statement;

        let test_ty = self.infer_standalone_expression(test, TypeContext::default());

        if let Err(err) = test_ty.try_bool(db, self.program_environment()) {
            err.report_diagnostic(&self.context, &**test);
        }

        self.infer_body(body);
        self.infer_body(orelse);
    }

    fn infer_assert_statement(&mut self, assert: &ast::StmtAssert) {
        let db = self.db();
        let ast::StmtAssert {
            range: _,
            node_index: _,
            test,
            msg,
        } = assert;

        let test_ty = self.infer_standalone_expression(test, TypeContext::default());

        if let Err(err) = test_ty.try_bool(db, self.program_environment()) {
            err.report_diagnostic(&self.context, &**test);
        }

        self.infer_optional_expression(msg.as_deref(), TypeContext::default());
    }

    fn infer_raise_statement(&mut self, raise: &ast::StmtRaise) {
        let db = self.db();
        let ast::StmtRaise {
            range: _,
            node_index: _,
            exc,
            cause,
        } = raise;

        let env = self.program_environment();
        let base_exception_type = KnownClass::BaseException.to_subclass_of(db, env);
        let base_exception_instance = KnownClass::BaseException.to_instance(db, env);

        let can_be_raised =
            UnionType::from_two_elements(db, env, base_exception_type, base_exception_instance);
        let can_be_exception_cause =
            UnionType::from_two_elements(db, env, can_be_raised, Type::none(db, env));

        if let Some(raised) = exc {
            let raised_type = self.infer_expression(raised, TypeContext::default());

            if !raised_type.is_assignable_to(db, env, can_be_raised) {
                report_invalid_exception_raised(&self.context, raised, raised_type);
            }
        }

        if let Some(cause) = cause {
            let cause_type = self.infer_expression(cause, TypeContext::default());

            if !cause_type.is_assignable_to(db, env, can_be_exception_cause) {
                report_invalid_exception_cause(&self.context, cause, cause_type);
            }
        }
    }

    fn infer_return_statement(&mut self, ret: &ast::StmtReturn) {
        match source_return::infer_return_sync(
            self,
            ret,
            source_return::ReturnFacts,
            &source_return::OrdinaryReturnEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    fn infer_delete_statement(&mut self, delete: &ast::StmtDelete) {
        let ast::StmtDelete {
            range: _,
            node_index: _,
            targets,
        } = delete;
        for target in targets {
            self.infer_expression(target, TypeContext::default());
        }
    }

    fn infer_global_statement(&mut self, global: &ast::StmtGlobal) {
        // CPython allows examples like this, where a global variable is never explicitly defined
        // in the global scope:
        //
        // ```py
        // def f():
        //     global x
        //     x = 1
        // def g():
        //     print(x)
        // ```
        //
        // However, allowing this pattern would make it hard for us to guarantee
        // accurate analysis about the types and boundness of global-scope symbols,
        // so we require the variable to be explicitly defined (either bound or declared)
        // in the global scope.
        let ast::StmtGlobal {
            node_index: _,
            range: _,
            names,
        } = global;
        let global_place_table = self.index.place_table(FileScopeId::global());
        for name in names {
            if let Some(symbol_id) = global_place_table.symbol_id(name) {
                let symbol = global_place_table.symbol(symbol_id);
                if symbol.is_bound() || symbol.is_declared() {
                    // This name is explicitly defined in the global scope (not just in function
                    // bodies that mark it `global`).
                    continue;
                }
            }
            if !module_type_implicit_global_symbol(self.db(), self.program_file(), name)
                .place
                .is_undefined()
            {
                // This name is an implicit global like `__file__` (but not a built-in like `int`).
                continue;
            }
            // This variable isn't explicitly defined in the global scope, nor is it an
            // implicit global from `types.ModuleType`, so we consider this `global` statement invalid.
            let Some(builder) = self.context.report_lint(&UNRESOLVED_GLOBAL, name) else {
                return;
            };
            let mut diag =
                builder.into_diagnostic(format_args!("Invalid global declaration of `{name}`"));
            diag.set_primary_annotation_message(format_args!(
                "`{name}` has no declarations or bindings in the global scope"
            ));
            diag.info(
                "This limits ty's ability to make accurate inferences \
                about the boundness and types of global-scope symbols",
            );
            diag.info(format_args!(
                "Consider adding a declaration to the global scope, e.g. `{name}: int`"
            ));
        }
    }

    fn module_type_from_name(&self, module_name: &ModuleName) -> Option<Type<'db>> {
        let db = self.db();
        let importing_file = ImportingFile::File(
            self.file(),
            self.program_environment().resolver_environment(db),
        );
        resolve_module(db, importing_file, module_name)
            .map(|module| Type::module_literal(self.db(), self.program_file(), module))
    }

    fn infer_decorator(&mut self, decorator: &ast::Decorator) -> Type<'db> {
        let ast::Decorator {
            range: _,
            node_index: _,
            expression,
        } = decorator;

        self.infer_expression(expression, TypeContext::default())
    }

    /// Preserve the descriptor behavior of a transparent callable decorator when it is written
    /// as the equivalent assignment form in a class body.
    fn apply_desugared_decorator(
        &mut self,
        decorator_ty: Type<'db>,
        call_expression: &ast::ExprCall,
        return_ty: Type<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let arguments = &call_expression.arguments;
        let [decorated_expression] = &arguments.args[..] else {
            return return_ty;
        };
        if !arguments.keywords.is_empty() || decorated_expression.is_starred_expr() {
            return return_ty;
        }

        let decorated_ty =
            self.get_or_infer_expression(decorated_expression, TypeContext::default());
        let call_arguments = CallArguments::positional([decorated_ty]);
        let Ok(bindings) = decorator_ty.try_call(db, env, &call_arguments) else {
            return return_ty;
        };

        transparent_callable_decorator_result(db, env, &bindings, decorated_ty).unwrap_or(return_ty)
    }

    /// Apply a decorator to a function or class type and return the resulting type.
    ///
    /// Constructor semantics for class-like decorators are handled by `Type::bindings`, so we
    /// can always use `try_call` here.
    fn apply_decorator(
        &mut self,
        decorator_ty: Type<'db>,
        decorated_ty: Type<'db>,
        decorator_node: &ast::Decorator,
        decorated_function: Option<&ast::StmtFunctionDef>,
    ) -> Type<'db> {
        let effects = function::application::OrdinaryDecoratorApplicationEffects {
            db: self.db(),
            env: self.program_environment(),
        };
        let Ok(result) = function::application::apply_decorator_sync(
            self,
            decorator_ty,
            decorated_ty,
            decorator_node,
            decorated_function,
            function::application::DecoratorApplicationFacts,
            &effects,
        );
        result
    }

    fn defer_decorator_call(&mut self, decorator: &ast::Decorator, input_ty: Type<'db>) {
        let effects = function::application::OrdinaryDecoratorApplicationEffects {
            db: self.db(),
            env: self.program_environment(),
        };
        let Ok(()) =
            function::application::defer_decorator_call_sync(self, decorator, input_ty, &effects);
    }

    #[expect(clippy::too_many_arguments)]
    fn infer_and_try_call_dunder(
        &mut self,
        object: Type<'db>,
        name: &str,
        lookup_policy: MemberLookupPolicy,
        ast_arguments: ArgumentsIter<'_>,
        argument_types: &mut CallArguments<'_, 'db>,
        infer_argument_ty: &mut dyn FnMut(&mut Self, ArgExpr<'db, '_>) -> Type<'db>,
        call_expression_tcx: TypeContext<'db>,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        let db = self.db();
        let env = self.program_environment();
        match object
            .member_lookup_with_policy(db, env, name, lookup_policy)
            .place
        {
            Place::Defined(DefinedPlace {
                ty: dunder_callable,
                definedness: boundness,
                provenance,
                ..
            }) => {
                let mut bindings = self.bindings_for_call(dunder_callable).match_parameters(
                    db,
                    env,
                    argument_types,
                );

                if let Err(call_error) = self.infer_and_check_argument_types(
                    ast_arguments,
                    argument_types,
                    infer_argument_ty,
                    &mut bindings,
                    call_expression_tcx,
                ) {
                    return Err(CallDunderError::CallError(
                        call_error,
                        Box::new(bindings),
                        provenance,
                    ));
                }

                if boundness == Definedness::PossiblyUndefined {
                    return Err(CallDunderError::PossiblyUnbound {
                        bindings: Box::new(bindings),
                        unbound_on: None,
                    });
                }
                Ok(bindings)
            }
            Place::Undefined => Err(CallDunderError::MethodNotAvailable),
        }
    }

    fn infer_and_check_argument_types(
        &mut self,
        ast_arguments: ArgumentsIter<'_>,
        argument_types: &mut CallArguments<'_, 'db>,
        infer_argument_ty: &mut dyn FnMut(&mut Self, ArgExpr<'db, '_>) -> Type<'db>,
        bindings: &mut Bindings<'db>,
        call_expression_tcx: TypeContext<'db>,
    ) -> Result<(), CallErrorKind> {
        local::check_arguments(
            self,
            ast_arguments,
            argument_types,
            infer_argument_ty,
            bindings,
            call_expression_tcx,
        )
    }

    fn infer_standalone_statement_impl(&mut self, standalone_statement: Statement<'db>) {
        let types = infer_statement_types(self.db(), standalone_statement);
        self.extend_statement(&types);
    }

    fn infer_optional_expression(
        &mut self,
        expression: Option<&ast::Expr>,
        tcx: TypeContext<'db>,
    ) -> Option<Type<'db>> {
        expression.map(|expr| self.infer_expression(expr, tcx))
    }

    #[track_caller]
    fn infer_expression(&mut self, expression: &ast::Expr, tcx: TypeContext<'db>) -> Type<'db> {
        debug_assert!(
            !self.index.is_standalone_expression(expression),
            "Calling `self.infer_expression` on a standalone-expression \
            is not allowed because it can lead to double-inference. \
            Use `self.infer_standalone_expression` instead."
        );

        self.infer_expression_impl(expression, tcx)
    }

    fn infer_maybe_standalone_expression(
        &mut self,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        if let Some(standalone_expression) = self.index.try_expression(expression) {
            self.infer_standalone_expression_impl(expression, standalone_expression, tcx)
        } else {
            self.infer_expression(expression, tcx)
        }
    }

    fn infer_expression_with_collection_literal_peer_context(
        &mut self,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
        peer_ty: Option<Type<'db>>,
    ) -> Type<'db> {
        self.infer_with_collection_literal_peer_context(expression, tcx, peer_ty, |builder, tcx| {
            builder.infer_expression(expression, tcx)
        })
    }

    fn infer_maybe_standalone_expression_with_collection_literal_peer_context(
        &mut self,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
        peer_ty: Option<Type<'db>>,
    ) -> Type<'db> {
        self.infer_with_collection_literal_peer_context(expression, tcx, peer_ty, |builder, tcx| {
            builder.infer_maybe_standalone_expression(expression, tcx)
        })
    }

    fn infer_with_collection_literal_peer_context(
        &mut self,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
        peer_ty: Option<Type<'db>>,
        mut infer_expression: impl FnMut(&mut Self, TypeContext<'db>) -> Type<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let peer_tcx = if is_collection_literal(expression)
            && let Some(peer_ty) = peer_ty
            && prefer_collection_literal_peer_context(db, env, tcx)
        {
            TypeContext::new(Some(peer_ty))
        } else {
            return infer_expression(self, tcx);
        };

        let mut speculative_builder = self.speculate();
        let ty = infer_expression(&mut speculative_builder, peer_tcx);

        // Peer context is only an inference hint. If it introduces diagnostics, discard it and
        // infer normally so that only diagnostics intrinsic to the expression are reported.
        if speculative_builder.context.has_diagnostics() {
            infer_expression(self, tcx)
        } else {
            self.extend(speculative_builder);
            ty
        }
    }

    #[track_caller]
    fn infer_standalone_expression(
        &mut self,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let standalone_expression = self.index.expression(expression);
        self.infer_standalone_expression_impl(expression, standalone_expression, tcx)
    }

    fn infer_standalone_expression_impl(
        &mut self,
        expression: &ast::Expr,
        standalone_expression: Expression<'db>,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let types = infer_expression_types(self.db(), standalone_expression, tcx);
        self.extend_expression(types);

        // Instead of calling `self.expression_type(expr)` after extending here, we get
        // the result from `types` directly because we might be in cycle recovery where
        // `types.cycle_fallback_type` is `Some(fallback_ty)`, which we can retrieve by
        // using `expression_type` on `types`:
        types.expression_type(expression)
    }

    /// Infer the type of an expression.
    fn infer_expression_impl(
        &mut self,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        local::expression(self, expression, tcx, local::ExpressionMode::Cached)
    }

    /// Infer an expression without implicitly treating this root as a `TypeForm`.
    ///
    /// Child expressions still use their normal contextual inference, so the
    /// expression's existing bidirectional behavior is preserved.
    fn infer_value_expression_impl(
        &mut self,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        local::expression(self, expression, tcx, local::ExpressionMode::Value)
    }

    /// Apply context before recording an inferred expression type.
    fn finish_expression_type(
        &mut self,
        expression: &ast::Expr,
        ty: Type<'db>,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        crate::types::signatures::effects::legacy_inline(self.finish_expression_type_with(
            &source_expression::LegacySourceExpressionEffects,
            expression,
            ty,
            tcx,
        ))
    }

    /// Applies the provided type context to an already inferred type.
    fn apply_type_context(
        &mut self,
        expression: &ast::Expr,
        ty: Type<'db>,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        match source_expression::apply_type_context_sync(
            self,
            expression,
            ty,
            tcx,
            &source_expression::OrdinaryApplyTypeContextEffects,
        ) {
            Ok(ty) => ty,
            Err(never) => match never {},
        }
    }

    /// Specialize a bare generic class value based on type context.
    ///
    /// Currently supports only Callable type contexts.
    ///
    /// This lets `list` in a `Callable[[], list[str]]` context be treated as `list[str]`.
    fn specialize_generic_class_from_context(&self, ty: Type<'db>, target: Type<'db>) -> Type<'db> {
        match source_expression::specialize_class_context_sync(
            self,
            ty,
            target,
            &source_expression::OrdinaryClassContextEffects,
        ) {
            Ok(ty) => ty,
            Err(never) => match never {},
        }
    }

    fn specialize_class_literal_from_context(
        &self,
        class: ClassLiteral<'db>,
        target: Type<'db>,
    ) -> Type<'db> {
        let ty = Type::ClassLiteral(class);
        let env = self.program_environment();
        // TODO: The constraint-set assignability rules should already be
        // able to determine that `list` (coerced into a callable) is assignable
        // to `Callable[[], list[str]]` when `_T@list = str`. However, when
        // comparing callables, if either is generic, we existentially quantify
        // away its typevars, transforming `∃ _T@list . _T@list = str` into
        // `always`. If we _didn't_ perform that quantification, we would have
        // the information we need to choose an appropriate specialization of
        // `list` given the type context, and we wouldn't have to duplicate all
        // of the logic below.
        let db = self.db();
        let exactly_one_callable = |union: UnionType<'db>| {
            union
                .elements(db)
                .iter()
                .filter_map(|element| element.resolve_type_alias(db).as_callable())
                .exactly_one()
                .ok()
        };
        let Some(target_callable) = (match target.resolve_type_alias(db) {
            Type::Callable(callable) => Some(callable),
            Type::Union(union) => exactly_one_callable(union),
            _ => None,
        }) else {
            return ty;
        };
        // Callables made entirely of dynamic types provide no constraints for specializing the
        // class. The same is true for a parameterless context whose return type is an
        // unspecialized variable from an enclosing generic call.
        if target_callable.signatures(db).iter().all(|signature| {
            let parameters = signature.parameters().as_slice();
            (signature.return_ty.is_dynamic()
                && parameters
                    .iter()
                    .all(|parameter| parameter.annotated_type().is_dynamic()))
                || (parameters.is_empty()
                    && matches!(
                        signature.return_ty,
                        Type::Dynamic(DynamicType::UnspecializedTypeVar)
                    ))
        }) {
            return ty;
        }
        let Some(class_generic_context) = class.generic_context(db) else {
            return ty;
        };
        let Some(source_callable) = ty.try_upcast_to_callable(db, env) else {
            return ty;
        };
        // The callable relation existentially solves variables bound by each signature. Keep
        // method-local constructor variables scoped there, but expose class variables to this
        // outer contextual-specialization solve.
        let source_callable = source_callable.map(|callable| {
            let signatures = CallableSignature::from_overloads(
                callable.signatures(db).overloads.iter().map(|signature| {
                    let signature_generic_context = signature.generic_context.and_then(|context| {
                        let mut variables = context
                            .variables(db)
                            .filter(|typevar| {
                                !class_generic_context.contains(db, typevar.identity(db))
                            })
                            .peekable();
                        variables
                            .peek()
                            .is_some()
                            .then(|| GenericContext::from_typevar_instances(db, env, variables))
                    });
                    Signature::new_generic(
                        signature_generic_context,
                        signature.parameters().clone(),
                        signature.return_ty,
                    )
                    .with_definition(signature.definition())
                }),
            );
            callable.with_signatures(db, signatures)
        });
        let inferable = class_generic_context.inferable_typevars(db);
        let constraints = ConstraintSetBuilder::new();
        let path_bounds = source_callable
            .to_type(db, env)
            .assignable_solutions_with_inferable(
                db,
                env,
                Type::Callable(target_callable),
                inferable,
            );
        let Solutions::Constrained(solutions) = path_bounds.solve(db, env, &constraints, inferable)
        else {
            return ty;
        };

        let mut type_context_mappings: FxHashMap<BoundTypeVarIdentity<'db>, UnionAccumulator<'db>> =
            FxHashMap::default();
        for solution in solutions.into_vec() {
            for binding in solution.solved_typevars {
                let inferred_ty = binding
                    .solution
                    .filter_union(db, env, |ty| !ty.has_provisional_marker(db, env));
                if inferred_ty.has_provisional_marker(db, env) {
                    continue;
                }

                type_context_mappings
                    .entry(binding.bound_typevar.identity(db))
                    .and_modify(|existing| existing.add(db, env, inferred_ty))
                    .or_insert_with(|| UnionAccumulator::new(inferred_ty));
            }
        }

        if type_context_mappings.is_empty() {
            return ty;
        }

        let type_context_mappings: FxHashMap<BoundTypeVarIdentity<'db>, Type<'db>> =
            type_context_mappings
                .into_iter()
                .map(|(identity, accumulator)| (identity, accumulator.into_type(db, env)))
                .collect();
        let specialized = Type::from(class.apply_specialization(db, |generic_context| {
            generic_context.specialize_recursive(
                db,
                generic_context
                    .variables(db)
                    .map(|typevar| type_context_mappings.get(&typevar.identity(db)).copied()),
            )
        }));
        if specialized.is_assignable_to(db, env, Type::Callable(target_callable)) {
            specialized
        } else {
            ty
        }
    }

    #[track_caller]
    fn store_expression_type(&mut self, expression: &ast::Expr, ty: Type<'db>) {
        let previous = self.expressions.insert(expression.into(), ty);
        assert_eq!(previous, None);
    }

    /// Whether this region's inference should record expected types for string-literal
    /// completions. They're only ever read for files open in the editor.
    fn collects_expected_types(&self) -> bool {
        self.db().is_open_file(self.file())
    }

    fn store_maybe_expected_type(
        &mut self,
        expression: impl Into<ExpressionNodeKey>,
        ty: Type<'db>,
    ) {
        // Cheaper check first so most queries never depend on the open-file state
        if !self.has_string_literal_completion_candidates(ty) {
            return;
        }

        self.store_expected_type(expression, ty);
    }

    fn store_expected_type(&mut self, expression: impl Into<ExpressionNodeKey>, ty: Type<'db>) {
        if !self.collects_expected_types() {
            return;
        }

        self.expected_types.insert(expression.into(), ty);
    }

    fn has_string_literal_completion_candidates(&self, ty: Type<'db>) -> bool {
        match ty {
            Type::LiteralValue(literal) => literal.as_string().is_some(),
            Type::Union(union) => union
                .elements(self.db())
                .iter()
                .any(|ty| self.has_string_literal_completion_candidates(*ty)),
            Type::Intersection(intersection) => intersection
                .iter_positive(self.db())
                .any(|ty| self.has_string_literal_completion_candidates(ty)),
            Type::TypeAlias(_) | Type::Recursive(_) => true,
            _ => false,
        }
    }

    fn union_expected_types(&mut self, expected_types: &FxHashMap<ExpressionNodeKey, Type<'db>>) {
        let db = self.db();
        let env = self.program_environment();
        // Non-empty only if the producing inference collected, i.e. the file is open
        if expected_types.is_empty() {
            return;
        }

        #[expect(
            clippy::iter_over_hash_type,
            reason = "expected types for distinct expressions are unioned independently"
        )]
        for (expression, ty) in expected_types {
            self.expected_types
                .entry(*expression)
                .and_modify(|existing| {
                    *existing = UnionType::from_two_elements(db, env, *existing, *ty);
                })
                .or_insert(*ty);
        }
    }

    fn infer_number_literal_expression(&self, literal: &ast::ExprNumberLiteral) -> Type<'db> {
        match number_literal::infer_number_literal_sync(
            self,
            literal,
            number_literal::NumberLiteralFacts,
            &number_literal::OrdinaryNumberLiteralEffects,
        ) {
            Ok(ty) => ty,
            Err(never) => match never {},
        }
    }

    #[expect(clippy::unused_self)]
    fn infer_boolean_literal_expression(&self, literal: &ast::ExprBooleanLiteral) -> Type<'db> {
        let ast::ExprBooleanLiteral {
            range: _,
            node_index: _,
            value,
        } = literal;

        Type::bool_literal(*value)
    }

    fn infer_string_literal_expression(
        &mut self,
        literal: &ast::ExprStringLiteral,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let Ok(ty) = string_literal::infer_string_literal_sync(
            self,
            literal,
            tcx,
            string_literal::StringLiteralFacts,
            &string_literal::OrdinaryStringLiteralEffects,
        );
        ty
    }

    fn infer_bytes_literal_expression(&mut self, literal: &ast::ExprBytesLiteral) -> Type<'db> {
        // TODO: ignoring r/R prefixes for now, should normalize bytes values
        let bytes: Vec<u8> = literal.value.bytes().collect();
        Type::bytes_literal(self.db(), &bytes)
    }

    fn infer_fstring_expression(&mut self, fstring: &ast::ExprFString) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let ast::ExprFString {
            range: _,
            node_index: _,
            value,
        } = fstring;

        let mut collector = StringPartsCollector::new();
        for part in value {
            // Make sure we iter through every parts to infer all sub-expressions. The `collector`
            // struct ensures we don't allocate unnecessary strings.
            match part {
                ast::FStringPartRef::Literal(literal) => {
                    collector.push_str(&literal.value);
                }
                ast::FStringPartRef::FString(fstring) => {
                    for element in &fstring.elements {
                        match element {
                            ast::InterpolatedStringElement::Interpolation(expression) => {
                                let ast::InterpolatedElement {
                                    range: _,
                                    node_index: _,
                                    expression,
                                    debug_text,
                                    conversion,
                                    format_spec,
                                } = expression;
                                let ty = self.infer_expression(expression, TypeContext::default());

                                if let Some(format_spec) = format_spec {
                                    for element in format_spec.elements.interpolations() {
                                        self.infer_expression(
                                            &element.expression,
                                            TypeContext::default(),
                                        );
                                    }
                                }

                                // TODO: handle format specifiers by calling a method
                                // (`Type::format`?) that handles the `__format__` method.
                                // Conversion flags should be handled before calling `__format__`.
                                // https://docs.python.org/3/library/string.html#format-string-syntax
                                if debug_text.is_some()
                                    || !conversion.is_none()
                                    || format_spec.is_some()
                                {
                                    collector.add_non_literal_string_expression();
                                } else {
                                    let str_ty = ty.str(db, env);
                                    if let Some(literal) = str_ty.as_string_literal() {
                                        collector.push_str(literal.value(self.db()));
                                    } else if str_ty.is_subtype_of(db, env, Type::literal_string())
                                    {
                                        collector.add_literal_string_expression();
                                    } else {
                                        collector.add_non_literal_string_expression();
                                    }
                                }
                            }
                            ast::InterpolatedStringElement::Literal(literal) => {
                                collector.push_str(&literal.value);
                            }
                        }
                    }
                }
            }
        }
        collector.string_type(&self.context)
    }

    fn infer_tstring_expression(&mut self, tstring: &ast::ExprTString) -> Type<'db> {
        let db = self.db();
        let ast::ExprTString { value, .. } = tstring;
        for tstring in value {
            for element in &tstring.elements {
                match element {
                    ast::InterpolatedStringElement::Interpolation(
                        tstring_interpolation_element,
                    ) => {
                        let ast::InterpolatedElement {
                            expression,
                            format_spec,
                            ..
                        } = tstring_interpolation_element;
                        self.infer_expression(expression, TypeContext::default());
                        if let Some(format_spec) = format_spec {
                            for element in format_spec.elements.interpolations() {
                                self.infer_expression(&element.expression, TypeContext::default());
                            }
                        }
                    }
                    ast::InterpolatedStringElement::Literal(_) => {}
                }
            }
        }
        KnownClass::Template.to_instance(db, self.program_environment())
    }

    fn infer_ellipsis_literal_expression(
        &mut self,
        _literal: &ast::ExprEllipsisLiteral,
    ) -> Type<'db> {
        let db = self.db();
        KnownClass::EllipsisType.to_instance(db, self.program_environment())
    }

    fn infer_tuple_expression(
        &mut self,
        tuple: &ast::ExprTuple,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        match tuple_expression::infer_tuple_expression_sync(
            self,
            tuple,
            tcx,
            tuple_expression::TupleExpressionFacts,
            &tuple_expression::OrdinaryTupleExpressionEffects,
        ) {
            Ok(ty) => ty,
            Err(never) => match never {},
        }
    }

    fn infer_list_expression(&mut self, list: &ast::ExprList, tcx: TypeContext<'db>) -> Type<'db> {
        let db = self.db();
        let ast::ExprList {
            range: _,
            node_index: _,
            elts,
            ctx: _,
        } = list;

        let elts = elts.iter().map(|elt| [Some(elt)]).collect_vec();
        let mut infer_elt_ty =
            |builder: &mut Self, (_, elt, tcx)| builder.infer_expression(elt, tcx);

        self.infer_collection_literal(
            KnownClass::List,
            Some(list.into()),
            &elts,
            &mut infer_elt_ty,
            tcx,
        )
        .unwrap_or_else(|| {
            KnownClass::List.to_specialized_instance(
                db,
                self.program_environment(),
                &[Type::unknown()],
            )
        })
    }

    fn infer_set_expression(&mut self, set: &ast::ExprSet, tcx: TypeContext<'db>) -> Type<'db> {
        let db = self.db();
        let ast::ExprSet {
            range: _,
            node_index: _,
            elts,
        } = set;

        let elts = elts.iter().map(|elt| [Some(elt)]).collect_vec();
        let fallback_tcx = self.incomplete_typed_dict_key_context(set, tcx);
        let mut infer_elt_ty = |builder: &mut Self, arg: ArgExpr<'db, '_>| {
            let (_, elt, elt_tcx) = arg;
            builder.infer_set_element(elt, elt_tcx, fallback_tcx)
        };

        self.infer_collection_literal(
            KnownClass::Set,
            Some(set.into()),
            &elts,
            &mut infer_elt_ty,
            tcx,
        )
        .unwrap_or_else(|| {
            KnownClass::Set.to_specialized_instance(
                db,
                self.program_environment(),
                &[Type::unknown()],
            )
        })
    }

    /// Infers a set element, optionally with a fallback context for an incomplete `TypedDict` key.
    ///
    /// When normal set element context is available, semantic inference keeps that context. If a
    /// `TypedDict` key fallback is also available, it is only added to the stored expected type used
    /// by IDE string-literal completions.
    fn infer_set_element(
        &mut self,
        elt: &ast::Expr,
        elt_tcx: TypeContext<'db>,
        fallback_tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let inference_tcx = if elt_tcx.annotation.is_some() {
            elt_tcx
        } else {
            fallback_tcx
        };
        let inferred_ty = self.infer_expression(elt, inference_tcx);

        // `expected_types` is IDE completion metadata. If normal set inference already has a
        // string-literal context, preserve that semantic context for inference while also offering
        // the transient `TypedDict` key fallback as a completion candidate.
        if let (Some(elt_ty), Some(fallback_ty)) = (elt_tcx.annotation, fallback_tcx.annotation) {
            self.store_expected_type(
                elt,
                UnionType::from_two_elements(db, self.program_environment(), elt_ty, fallback_ty),
            );
        }

        inferred_ty
    }

    /// Returns a fallback type context for completing a `TypedDict` key while editing.
    ///
    /// While editing `{"key": value}` as a `TypedDict` literal, `{"key"}` parses as a set
    /// until the colon is typed. This preserves key completions in that transient state.
    fn incomplete_typed_dict_key_context(
        &self,
        set: &ast::ExprSet,
        tcx: TypeContext<'db>,
    ) -> TypeContext<'db> {
        let [elt] = set.elts.as_slice() else {
            return TypeContext::default();
        };

        if !elt.is_string_literal_expr() {
            return TypeContext::default();
        }

        TypeContext::new(
            tcx.annotation
                .and_then(|annotation| self.typed_dict_key_expected_type(annotation)),
        )
    }

    fn infer_dict_expression(&mut self, dict: &ast::ExprDict, tcx: TypeContext<'db>) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let ast::ExprDict {
            range: _,
            node_index: _,
            items,
        } = dict;

        let mut item_types = FxHashMap::default();

        // Validate `TypedDict` dictionary literal assignments.
        if let Some(annotation) =
            tcx.annotation
                .map(|annotation| match annotation.resolve_type_alias(db) {
                    Type::Union(union) if union.has_aliases(db) => union.expand_aliases(db, env),
                    annotation => annotation,
                })
        {
            if let Some(typed_dict) = annotation.as_typed_dict() {
                // If there is a single typed dict annotation, infer against it directly. Expanding
                // first means a union whose arms all alias the same `TypedDict` reaches this
                // branch rather than neither.
                if let Some(ty) =
                    self.infer_typed_dict_expression(dict, typed_dict, &mut item_types)
                {
                    return ty;
                }
            } else if let Type::Union(union) = annotation {
                let union_elements = union.elements(self.db());
                let mut typed_dicts = Vec::new();
                let mut has_dict_compatible_fallback = false;

                for element in union_elements {
                    let element = element.resolve_type_alias(db);

                    if let Some(typed_dict) = element.as_typed_dict() {
                        typed_dicts.push(typed_dict);
                    } else if !has_dict_compatible_fallback {
                        // Suppress `TypedDict` diagnostics only if this literal is assignable to
                        // the non-`TypedDict` arm of the union.
                        let mut speculative_builder = self.speculate_without_diagnostics();
                        has_dict_compatible_fallback = speculative_builder
                            .infer_dict_expression(dict, TypeContext::new(Some(element)))
                            .is_assignable_to(db, env, element);
                    }
                }

                if let [typed_dict] = typed_dicts.as_slice()
                    && !has_dict_compatible_fallback
                {
                    if let Some(ty) =
                        self.infer_typed_dict_expression(dict, *typed_dict, &mut item_types)
                    {
                        return ty;
                    }
                } else if !typed_dicts.is_empty() {
                    // Infer all expressions with diagnostics enabled before starting
                    // multi-inference. This preserves the general expression types even if we later
                    // fall back to a non-`TypedDict` arm of the union.
                    for item in items {
                        if let Some(key) = item.key.as_ref() {
                            let key_ty = self.infer_expression(key, TypeContext::default());
                            item_types.insert(key.node_index().load(), key_ty);
                        }

                        let value_ty = self.infer_expression(&item.value, TypeContext::default());
                        item_types.insert(item.value.node_index().load(), value_ty);
                    }

                    let mut narrowed_tys = Vec::new();
                    let mut item_types = FxHashMap::default();
                    // Reuse nested expressions that receive the same field context across candidates.
                    let teardown_expression_cache = self.setup_expression_cache();
                    for typed_dict in typed_dicts {
                        // Suppress diagnostics for discarded candidates. A mixed union like
                        // `TypedDict | dict[str, Any]` should remain quiet when the dict arm accepts
                        // the literal.
                        if let Some(inferred_ty) = self
                            .speculate_without_diagnostics()
                            .infer_typed_dict_expression(dict, typed_dict, &mut item_types)
                        {
                            narrowed_tys.push(inferred_ty);
                        }

                        item_types.clear();
                    }
                    if teardown_expression_cache {
                        self.teardown_expression_cache();
                    }

                    // Successfully narrowed to a subset of typed dicts.
                    if !narrowed_tys.is_empty() {
                        return UnionType::from_elements(db, env, narrowed_tys);
                    }
                }
            }
        }

        let items = items
            .iter()
            .map(|item| [item.key.as_ref(), Some(&item.value)])
            .collect_vec();

        // Avoid inferring the items multiple times if we already attempted to infer the
        // dictionary literal as a `TypedDict`. This also allows us to infer using the
        // type context of the expected `TypedDict` field.
        let mut infer_elt_ty = |builder: &mut Self, (_, elt, tcx): ArgExpr<'db, '_>| {
            item_types
                .get(&elt.node_index().load())
                .copied()
                .or_else(|| builder.try_expression_type(elt))
                .unwrap_or_else(|| builder.infer_expression(elt, tcx))
        };

        self.infer_collection_literal(
            KnownClass::Dict,
            Some(dict.into()),
            &items,
            &mut infer_elt_ty,
            tcx,
        )
        .unwrap_or_else(|| {
            KnownClass::Dict.to_specialized_instance(db, env, &[Type::unknown(), Type::unknown()])
        })
    }

    // Infer the type of a collection literal expression.
    fn infer_collection_literal<'expr, const N: usize>(
        &mut self,
        collection_class: KnownClass,
        collection_expr: Option<ast::ExprRef<'_>>,
        elts: &[[Option<&'expr ast::Expr>; N]],
        infer_elt_expression: &mut dyn FnMut(&mut Self, ArgExpr<'db, 'expr>) -> Type<'db>,
        tcx: TypeContext<'db>,
    ) -> Option<Type<'db>> {
        let db = self.db();
        let env = self.program_environment();
        let mut try_narrow = |narrowed_ty| {
            let mut speculative_builder = self.speculate();

            // Attempt to infer the collection literal using the narrowed type context.
            let inferred_ty = speculative_builder.infer_collection_literal_impl(
                collection_class,
                collection_expr,
                elts,
                infer_elt_expression,
                TypeContext::new(Some(narrowed_ty)),
            )?;

            // Ensure the inferred return type is assignable to the narrowed declared type.
            if !inferred_ty.is_assignable_to(db, env, narrowed_ty) {
                return None;
            }

            // Successfully narrowed to an element of the union.
            self.extend(speculative_builder);
            Some(inferred_ty)
        };

        // If the type context is a union, attempt to narrow to a specific element.
        for narrowed_ty in tcx
            .narrow_targets(db, env)
            .as_deref()
            .into_iter()
            .flatten()
            .filter(|ty| ty.class_specialization(db, env).is_some())
        {
            if let Some(result) = try_narrow(*narrowed_ty) {
                return Some(result);
            }
        }

        self.infer_collection_literal_impl(
            collection_class,
            collection_expr,
            elts,
            infer_elt_expression,
            tcx,
        )
    }

    // Infer the type of a collection literal expression.
    fn infer_collection_literal_impl<'expr, const N: usize>(
        &mut self,
        collection_class: KnownClass,
        collection_expr: Option<ast::ExprRef<'_>>,
        elts: &[[Option<&'expr ast::Expr>; N]],
        infer_elt_expression: &mut dyn FnMut(&mut Self, ArgExpr<'db, 'expr>) -> Type<'db>,
        tcx: TypeContext<'db>,
    ) -> Option<Type<'db>> {
        let db = self.db();
        let env = self.program_environment();

        // Extract the type variable `T` from `list[T]` in typeshed.
        let elt_tys = |collection_class: KnownClass| {
            let collection_alias = collection_class
                .try_to_class_literal(db, env)?
                .identity_specialization(db)
                .into_generic_alias()?;

            let generic_context = collection_alias
                .specialization(self.db())
                .generic_context(self.db());

            Some((
                collection_alias,
                generic_context,
                generic_context.variables(self.db()),
            ))
        };

        let Some((collection_alias, generic_context, elt_tys)) = elt_tys(collection_class) else {
            // Infer the element types without type context, and fallback to `Unknown` for
            // custom typesheds.
            for elts in elts {
                for (i, elt) in elts.iter().enumerate() {
                    let Some(elt) = elt else { continue };
                    infer_elt_expression(self, (i, elt, TypeContext::default()));
                }
            }

            return None;
        };

        let constraints = ConstraintSetBuilder::new();
        let inferable = generic_context.inferable_typevars(db);
        let identity_instance = Type::instance(db, env, ClassType::Generic(collection_alias));
        let mut builder = SpecializationBuilder::new(db, env, &constraints, generic_context);

        // Remove any union elements of that are unrelated to the collection type.
        //
        // For example, we only want the `list[int]` from `annotation: list[int] | None` if
        // `collection_ty` is `list`.
        let tcx = tcx.map(|annotation| {
            let collection_ty = collection_class.to_instance(db, env);
            annotation
                .discard_disjoint_union_elements(db, env, collection_ty, inferable)
                .or_never()
        });

        // Collect type constraints from the declared element types.
        //
        // We use a forward assignability check (`identity_instance ≤ tcx`) to infer what each
        // typevar maps to in the type context. For example, if the type context is `list[int]` and
        // `collection_instance` is `list[T]`, the check produces `T = int`.
        let (elt_tcx_constraints, elt_tcx_variance) = {
            let mut elt_tcx_constraints: FxHashMap<
                BoundTypeVarIdentity<'db>,
                UnionAccumulator<'db>,
            > = FxHashMap::default();
            let mut elt_tcx_variance: FxHashMap<BoundTypeVarIdentity<'_>, TypeVarVariance> =
                FxHashMap::default();

            if let Some(tcx) = tcx.annotation.map(|tcx| tcx.resolve_type_alias(db))
                && matches!(tcx, Type::NominalInstance(_))
                && let Some(specialization) = tcx.known_specialization(db, env, collection_class)
                && specialization.generic_context(self.db()) == generic_context
                && generic_context.variables(self.db()).all(|typevar| {
                    !typevar.is_paramspec(self.db())
                        && typevar
                            .typevar(self.db())
                            .bound_or_constraints(db, env)
                            .is_none()
                })
            {
                // For an instance of the collection class itself, the identity specialization
                // maps directly to the contextual specialization. Avoid constructing and solving
                // a general assignability constraint set for this common case.
                for (typevar, inferred_ty) in generic_context
                    .variables(self.db())
                    .zip(specialization.types(self.db()))
                {
                    let inferred_ty = inferred_ty
                        .filter_union(db, env, |ty| {
                            !ty.as_typevar()
                                .is_some_and(|tv| tv.is_inferable(self.db(), inferable))
                        })
                        .filter_union(db, env, |ty| !ty.has_unspecialized_type_var(db, env));
                    if inferred_ty.has_unspecialized_type_var(db, env) {
                        continue;
                    }

                    let identity = typevar.identity(self.db());
                    elt_tcx_constraints.insert(identity, UnionAccumulator::new(inferred_ty));
                    elt_tcx_variance.insert(identity, typevar.variance(db));
                }
            } else if let Some(tcx) = tcx.annotation
                && tcx.class_specialization(db, env).is_some()
            {
                let db = self.db();

                let path_bounds =
                    identity_instance.assignable_solutions_with_inferable(db, env, tcx, inferable);
                let solutions = path_bounds.solve_with(|variance, path_bound| {
                    let identity = path_bound.bound_typevar.identity(db);
                    elt_tcx_variance
                        .entry(identity)
                        .and_modify(|current| *current = current.join(variance))
                        .or_insert(variance);
                    CandidateSolutions::preliminary_solve(
                        db,
                        env,
                        &constraints,
                        inferable,
                        path_bound,
                    )
                });

                match solutions {
                    // If the type context is not compatible with the collection type (e.g., a
                    // `list` literal where a `tuple` is expected), the assignability check
                    // produces an unsatisfiable result. In that case, we simply proceed without
                    // type context constraints rather than aborting the entire collection literal
                    // inference.
                    Solutions::Unsatisfiable(_) | Solutions::Unconstrained => {}
                    Solutions::Constrained(solutions) => {
                        for solution in solutions.as_slice() {
                            for binding in &solution.solved_typevars {
                                // The SequentMap's transitivity reasoning can inject
                                // cross-typevar references into the solution bounds.
                                // For example, `_KT ≤ str ∧ str ≤ _VT` derives `_KT ≤ _VT`,
                                // which adds `_KT` to `_VT`'s lower bound. Remove inferable
                                // typevars from the same generic context, since they represent
                                // cross-typevar relationships that are resolved independently.
                                let inferred_ty = builder
                                    .remove_inferable_typevar_artifacts_from_solution(
                                        binding.bound_typevar,
                                        binding.solution,
                                    );

                                // Avoid inferring a preferred type based on partially specialized
                                // type context from an outer generic call. If the type context is
                                // a union, we try to keep any concrete elements.
                                let inferred_ty = inferred_ty.filter_union(db, env, |ty| {
                                    !ty.has_unspecialized_type_var(db, env)
                                });
                                if inferred_ty.has_unspecialized_type_var(db, env) {
                                    continue;
                                }

                                let identity = binding.bound_typevar.identity(db);
                                elt_tcx_constraints
                                    .entry(identity)
                                    .and_modify(|existing| {
                                        existing.add(db, env, inferred_ty);
                                    })
                                    .or_insert_with(|| UnionAccumulator::new(inferred_ty));
                            }
                        }

                        // Remove variance entries for typevars whose solutions were filtered out
                        // (e.g., due to unspecialized typevars). Variance should only be tracked
                        // for typevars with actual type context constraints.
                        elt_tcx_variance
                            .retain(|identity, _| elt_tcx_constraints.contains_key(identity));
                    }
                }
            }

            let elt_tcx_constraints: FxHashMap<BoundTypeVarIdentity<'db>, Type<'db>> =
                elt_tcx_constraints
                    .into_iter()
                    .map(|(identity, accumulator)| (identity, accumulator.into_type(db, env)))
                    .collect();

            (elt_tcx_constraints, elt_tcx_variance)
        };

        // Dictionary unpacking always contributes constraints on the inferred key and value types,
        // even when the unpacked mapping is assignable to the context. Keep it on the general path
        // so gradual types such as `Any` are preserved.
        let has_dict_unpack = collection_class == KnownClass::Dict
            && elts
                .iter()
                .any(|elts| matches!(elts.as_slice(), [None, Some(_)]));

        let mut pre_inferred_elt_tys = None;

        // Avoid projecting and solving a constraint set when contextual inference has already
        // provided the complete specialization and every literal element is compatible with it.
        if !has_dict_unpack
            && tcx.annotation.is_some()
            && let Some(specialization) = generic_context
                .variables(self.db())
                .map(|typevar| {
                    let identity = typevar.identity(self.db());
                    // Keep this parallel with the slow path below: a covariant context provides
                    // only an upper bound, which does not determine the specialization for an empty
                    // literal. A contravariant context provides a lower bound, for which inference
                    // selects the narrowest valid solution.
                    if elt_tcx_variance
                        .get(&identity)
                        .is_some_and(|variance| variance.is_covariant())
                    {
                        return None;
                    }
                    elt_tcx_constraints.get(&identity).copied()
                })
                .collect::<Option<Vec<_>>>()
        {
            // The slow path below adds the contextual specialization as an invariant mapping,
            // then discards every element constraint that is already assignable to its context.
            // Infer the elements once here and retain their types so that a failed fast-path check
            // does not recursively re-infer nested collection literals on the slow path.
            let mut inferred_elts = Vec::with_capacity(elts.len());
            let mut compatible = true;

            for elts in elts {
                let mut inferred_elt_tys = [None; N];
                for (i, elt, elt_tcx) in itertools::izip!(0.., elts, specialization.iter().copied())
                {
                    let Some(elt) = elt else { continue };
                    let elt_tcx = if elt.is_starred_expr() && collection_class != KnownClass::Dict {
                        Type::homogeneous_tuple(db, env, elt_tcx)
                    } else {
                        elt_tcx
                    };
                    let inferred_elt_ty =
                        infer_elt_expression(self, (i, elt, TypeContext::new(Some(elt_tcx))));
                    inferred_elt_tys[i] = Some(inferred_elt_ty);

                    if !inferred_elt_ty.is_assignable_to(db, env, elt_tcx) {
                        compatible = false;
                    }
                }
                inferred_elts.push(inferred_elt_tys);
            }

            if compatible {
                let class_type = collection_alias.origin(self.db()).apply_specialization(
                    db,
                    |generic_context| {
                        generic_context
                            .specialize_recursive(db, specialization.into_iter().map(Some))
                    },
                );
                return Type::from(class_type).to_instance_approximation(db, env);
            }

            pre_inferred_elt_tys = Some(inferred_elts);
        }

        // Create a set of constraints to infer a precise type for `T`.
        let mut tuple_size_promotion_constraints = TupleSizePromotionConstraints::default();

        for elt_ty in elt_tys.clone() {
            let elt_ty_identity = elt_ty.identity(self.db());
            let elt_tcx = elt_tcx_constraints
                // The annotated type acts as a constraint for `T`.
                //
                // Note that we infer the annotated type _before_ the elements, to more closely match
                // the order of any unions as written in the type annotation.
                .get(&elt_ty_identity)
                .copied();

            if elt_tcx.is_some_and(|elt_tcx| !elt_tcx.is_dynamic()) {
                // Record type annotations that provide concrete shape information in order to
                // disqualify this typevar from tuple size promotion.
                tuple_size_promotion_constraints.record_declared_type(elt_ty_identity);
            }

            // Avoid unnecessarily widening the return type based on a covariant
            // type parameter from the type context.
            //
            // Note that we also avoid unioning  the inferred type with `Unknown` in this
            // case, which is only necessary for invariant collections.
            if elt_tcx_variance
                .get(&elt_ty_identity)
                .is_some_and(|variance| variance.is_covariant())
            {
                continue;
            }

            // If there is no applicable context for this element type variable, we infer from the
            // literal elements directly. This violates the gradual guarantee (we don't know that
            // our inference is compatible with subsequent additions to the collection), but it
            // matches the behavior of other type checkers and is usually the desired behavior.
            if let Some(elt_tcx) = elt_tcx {
                builder.add_type_mapping(elt_ty, elt_tcx, TypeVarVariance::Invariant);
            }
        }

        if tcx.annotation.is_none()
            && let Some(collection_expr) = collection_expr
            && let InferenceRegion::Expression(current_expr, _) = self.region
            && current_expr.node_ref(self.db()).index() == *collection_expr.node_index()
            && let Some(assignment) = current_expr.assigned_to(self.db())
            && let Ok(collection_def) =
                DefinitionNodeKey::from_assignment(assignment.node(self.module())).exactly_one()
            && let Some(collection_def) = self.index.try_definition(collection_def)
        {
            // For unannotated collection literals, collect any constraints created by later uses
            // of this definition in the scope.
            for (statement, use_expression) in
                self.index.constraining_collection_uses(collection_def)
            {
                let statement_use_types = infer_statement_types(self.db(), statement);

                if let Some(divergent) = statement_use_types
                    .expression_type(use_expression)
                    .as_divergent()
                {
                    // Infer `collection[Divergent]` for the initial cycle result.
                    let divergent_instance = collection_alias
                        .origin(self.db())
                        .apply_specialization(db, |generic_context| {
                            generic_context
                                .repeat_specialization(self.db(), Type::Divergent(divergent))
                        });

                    builder
                        .infer(
                            identity_instance,
                            Type::instance(db, env, divergent_instance),
                        )
                        .ok()?;
                } else if let Some(constraints) =
                    statement_use_types.collection_use_constraints(collection_def)
                {
                    for constraint in constraints {
                        if constraint.has_unspecialized_type_var(db, env) {
                            continue;
                        }

                        builder.infer(identity_instance, *constraint).ok()?;
                    }
                }
            }
        }

        for (elts_index, elts) in elts.iter().enumerate() {
            // An unpacking expression for a dictionary.
            if let &[None, Some(value_expr)] = elts.as_slice() {
                let unpack_ty = infer_elt_expression(self, (1, value_expr, tcx));

                let Some((unpacked_key_ty, unpacked_value_ty)) =
                    unpack_ty.unpack_keys_and_items(db, env)
                else {
                    if let Some(builder) =
                        self.context.report_lint(&INVALID_ARGUMENT_TYPE, value_expr)
                    {
                        let mut diag = builder
                            .into_diagnostic("Argument expression after ** must be a mapping type");

                        diag.set_primary_annotation_message(format_args!(
                            "Found `{}`",
                            unpack_ty.display(db, env)
                        ));
                    }

                    continue;
                };

                let mut elt_tys = elt_tys.clone();
                if let Some((key_ty, value_ty)) = elt_tys.next_tuple() {
                    tuple_size_promotion_constraints.record_unpromotable_type(
                        db,
                        env,
                        key_ty.identity(self.db()),
                        unpacked_key_ty.promote(db, env),
                    );
                    tuple_size_promotion_constraints.record_unpromotable_type(
                        db,
                        env,
                        value_ty.identity(self.db()),
                        unpacked_value_ty.promote(db, env),
                    );

                    builder.infer(Type::TypeVar(key_ty), unpacked_key_ty).ok()?;

                    builder
                        .infer(Type::TypeVar(value_ty), unpacked_value_ty)
                        .ok()?;
                }

                continue;
            }

            // The inferred type of each element acts as an additional constraint on `T`.
            for (i, elt, elt_ty) in itertools::izip!(0.., elts, elt_tys.clone()) {
                let Some(elt) = elt else { continue };

                // Note that unlike when preferring the declared type, we use covariant type
                // assignments from the type context to potentially _narrow_ the inferred type,
                // by avoiding promotion.
                let elt_ty_identity = elt_ty.identity(self.db());

                // If the element is a starred expression, we want to apply the type context to each element
                // in the unpacked expression (which we will store as a tuple when inferring it). We
                // therefore wrap the type context in an `tuple[T, ...]` specialization.
                let elt_tcx = elt_tcx_constraints
                    .get(&elt_ty_identity)
                    .copied()
                    .map(|tcx| {
                        if elt.is_starred_expr() && collection_class != KnownClass::Dict {
                            Type::homogeneous_tuple(db, env, tcx)
                        } else {
                            tcx
                        }
                    });

                let inferred_elt_ty = pre_inferred_elt_tys
                    .as_ref()
                    .and_then(|inferred_elts| inferred_elts[elts_index][i])
                    .unwrap_or_else(|| {
                        infer_elt_expression(self, (i, elt, TypeContext::new(elt_tcx)))
                    });

                // Simplify the inference based on a non-covariant declared type.
                if let Some(elt_tcx) =
                    elt_tcx.filter(|_| !elt_tcx_variance[&elt_ty_identity].is_covariant())
                    && inferred_elt_ty.is_assignable_to(db, env, elt_tcx)
                {
                    continue;
                }

                // A covariant context is an upper bound, so promotion must not widen an otherwise
                // compatible element beyond that bound. In particular, promoting an exact float
                // introduces `int`, which is not assignable to an exact-float context.
                let promoted_elt_ty = inferred_elt_ty.promote(db, env);
                let inferred_elt_ty = if let Some(elt_tcx) = elt_tcx
                    && elt_tcx_variance[&elt_ty_identity].is_covariant()
                    && promoted_elt_ty != inferred_elt_ty
                    && !promoted_elt_ty.is_assignable_to(db, env, elt_tcx)
                    && inferred_elt_ty.is_assignable_to(db, env, elt_tcx)
                {
                    inferred_elt_ty
                } else {
                    promoted_elt_ty
                };

                let inferred_type_for_typevar = if elt.is_starred_expr() {
                    inferred_elt_ty
                        .iterate(db, env)
                        .homogeneous_element_type(db, env)
                } else {
                    inferred_elt_ty
                };

                tuple_size_promotion_constraints.record_inferred_expression_type(
                    db,
                    env,
                    elt_ty_identity,
                    elt,
                    inferred_type_for_typevar,
                );

                builder
                    .infer(Type::TypeVar(elt_ty), inferred_type_for_typevar)
                    .ok()?;
            }
        }

        let class_type = collection_alias
            .origin(self.db())
            .apply_specialization(db, |_| {
                builder.build_merged_with(|current_typevar, bounds| {
                    let lower = bounds?.evidence_lower()?;

                    let lower = lower.promote_collection_element_type(
                        db,
                        env,
                        tuple_size_promotion_constraints.allow(current_typevar.identity(self.db())),
                        is_empty_collection_type_context(tcx),
                    );

                    Some(lower)
                })
            });

        Type::from(class_type).to_instance_approximation(db, env)
    }

    /// Infer the type of the `iter` expression of the first comprehension.
    fn infer_first_comprehension_iter(&mut self, comprehensions: &[ast::Comprehension]) {
        let mut comprehensions_iter = comprehensions.iter();
        let Some(first_comprehension) = comprehensions_iter.next() else {
            unreachable!("Comprehension must contain at least one generator");
        };
        self.infer_maybe_standalone_expression(&first_comprehension.iter, TypeContext::default());
    }

    /// Derive the type context for a generator expression's yielded element from the expected type
    /// of the generator expression itself.
    ///
    /// We model the generator expression as a synthetic `GeneratorType[T, None, None]` or
    /// `AsyncGeneratorType[T, None]`, then ask constraint-set assignability to solve for `T` against
    /// the expected annotation. The solved `T` becomes the type context for the expression being
    /// yielded, so normal assignability handles protocols and unions like `Iterable[int] | None`
    /// without adding target-specific special cases here.
    fn generator_yield_type_context(
        &self,
        tcx: TypeContext<'db>,
        evaluation_mode: EvaluationMode,
    ) -> TypeContext<'db> {
        let db = self.db();
        let env = self.program_environment();
        let Some(annotation) = tcx.annotation else {
            return TypeContext::default();
        };

        let yield_typevar = BoundTypeVarInstance::synthetic(
            db,
            env,
            Name::new_static("_GeneratorYieldT"),
            TypeVarVariance::Covariant,
        );
        let yield_ty = Type::TypeVar(yield_typevar);
        let none = Type::none(db, env);
        let generator_ty = if evaluation_mode.is_async() {
            KnownClass::AsyncGeneratorType.to_specialized_instance(db, env, &[yield_ty, none])
        } else {
            KnownClass::GeneratorType.to_specialized_instance(db, env, &[yield_ty, none, none])
        };

        let generic_context = GenericContext::from_typevar_instances(db, env, [yield_typevar]);
        let inferable = generic_context.inferable_typevars(db);
        let path_bounds =
            generator_ty.assignable_solutions_with_inferable(db, env, annotation, inferable);
        let constraints = ConstraintSetBuilder::new();
        let Solutions::Constrained(solutions) = path_bounds.solve(db, env, &constraints, inferable)
        else {
            return TypeContext::default();
        };

        let mut yield_tcx: Option<UnionAccumulator<'db>> = None;
        for solution in solutions.into_vec() {
            for binding in solution.solved_typevars {
                if binding.bound_typevar != yield_typevar {
                    continue;
                }
                match &mut yield_tcx {
                    Some(accumulator) => {
                        accumulator.add(db, env, binding.solution);
                    }
                    None => yield_tcx = Some(UnionAccumulator::new(binding.solution)),
                }
            }
        }

        TypeContext::new(yield_tcx.map(|accumulator| accumulator.into_type(db, env)))
    }

    fn infer_generator_expression(
        &mut self,
        generator: &ast::ExprGenerator,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let ast::ExprGenerator {
            range: _,
            node_index: _,
            elt,
            generators,
            parenthesized: _,
        } = generator;

        self.infer_first_comprehension_iter(generators);

        let Some(scope_id) = self
            .index
            .try_node_scope(NodeWithScopeRef::GeneratorExpression(generator))
        else {
            return Type::unknown();
        };
        let evaluation_mode =
            EvaluationMode::from_is_async(scope_id.is_async_comprehension(self.index));
        let yield_tcx = self.generator_yield_type_context(tcx, evaluation_mode);
        let scope = scope_id.to_scope_id(self.db(), self.program_file());
        let inference = infer_scope_types(self.db(), scope, yield_tcx);
        self.extend_scope(inference);
        let yield_type = self.comprehension_element_type(elt, inference);

        if evaluation_mode.is_async() {
            KnownClass::AsyncGeneratorType.to_specialized_instance(
                db,
                env,
                &[yield_type, Type::none(db, env)],
            )
        } else {
            KnownClass::GeneratorType.to_specialized_instance(
                db,
                env,
                &[yield_type, Type::none(db, env), Type::none(db, env)],
            )
        }
    }

    fn comprehension_element_type(
        &self,
        element: &ast::Expr,
        inference: &ScopeInference<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let element_type = inference.expression_type(element);
        if element.is_starred_expr() {
            element_type
                .iterate(db, env)
                .homogeneous_element_type(db, env)
        } else {
            element_type
        }
    }

    /// Return a specialization of the collection class (list, dict, set) based on the type context and the inferred
    /// element / key-value types from the comprehension expression.
    fn infer_comprehension_specialization<const N: usize>(
        &mut self,
        collection_class: KnownClass,
        collection_expr: ast::ExprRef<'_>,
        elements: [Option<&ast::Expr>; N],
        inference: &ScopeInference<'db>,
        tcx: TypeContext<'db>,
    ) -> Option<Type<'db>> {
        let mut infer_element_ty =
            |_builder: &mut Self, (_, elt, _)| inference.expression_type(elt);

        self.infer_collection_literal(
            collection_class,
            Some(collection_expr),
            &[elements],
            &mut infer_element_ty,
            tcx,
        )
    }

    fn infer_list_comprehension_expression(
        &mut self,
        listcomp: &ast::ExprListComp,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let ast::ExprListComp {
            range: _,
            node_index: _,
            elt,
            generators,
        } = listcomp;

        self.infer_first_comprehension_iter(generators);

        let Some(scope_id) = self
            .index
            .try_node_scope(NodeWithScopeRef::ListComprehension(listcomp))
        else {
            return Type::unknown();
        };
        let scope = scope_id.to_scope_id(self.db(), self.program_file());
        let inference = infer_scope_types(self.db(), scope, tcx);
        self.extend_scope(inference);

        self.infer_comprehension_specialization(
            KnownClass::List,
            listcomp.into(),
            [Some(elt)],
            inference,
            tcx,
        )
        .unwrap_or_else(|| {
            KnownClass::List.to_specialized_instance(
                db,
                self.program_environment(),
                &[Type::unknown()],
            )
        })
    }

    fn infer_set_comprehension_expression(
        &mut self,
        setcomp: &ast::ExprSetComp,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let ast::ExprSetComp {
            range: _,
            node_index: _,
            elt,
            generators,
        } = setcomp;

        self.infer_first_comprehension_iter(generators);

        let Some(scope_id) = self
            .index
            .try_node_scope(NodeWithScopeRef::SetComprehension(setcomp))
        else {
            return Type::unknown();
        };
        let scope = scope_id.to_scope_id(self.db(), self.program_file());
        let inference = infer_scope_types(self.db(), scope, tcx);
        self.extend_scope(inference);

        self.infer_comprehension_specialization(
            KnownClass::Set,
            setcomp.into(),
            [Some(elt)],
            inference,
            tcx,
        )
        .unwrap_or_else(|| {
            KnownClass::Set.to_specialized_instance(
                db,
                self.program_environment(),
                &[Type::unknown()],
            )
        })
    }

    fn infer_dict_comprehension_expression(
        &mut self,
        dictcomp: &ast::ExprDictComp,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let ast::ExprDictComp {
            range: _,
            node_index: _,
            key,
            value,
            generators,
        } = dictcomp;

        self.infer_first_comprehension_iter(generators);

        let Some(scope_id) = self
            .index
            .try_node_scope(NodeWithScopeRef::DictComprehension(dictcomp))
        else {
            return Type::unknown();
        };
        let scope = scope_id.to_scope_id(self.db(), self.program_file());
        let inference = infer_scope_types(self.db(), scope, tcx);
        self.extend_scope(inference);

        self.infer_comprehension_specialization(
            KnownClass::Dict,
            dictcomp.into(),
            [key.as_deref(), Some(value)],
            inference,
            tcx,
        )
        .unwrap_or_else(|| {
            KnownClass::Dict.to_specialized_instance(
                db,
                self.program_environment(),
                &[Type::unknown(), Type::unknown()],
            )
        })
    }

    fn infer_generator_expression_scope(
        &mut self,
        generator: &ast::ExprGenerator,
        tcx: TypeContext<'db>,
    ) {
        let db = self.db();
        let ast::ExprGenerator {
            range: _,
            node_index: _,
            elt,
            generators,
            parenthesized: _,
        } = generator;

        let elt_tcx = if elt.is_starred_expr() {
            tcx.map(|yield_ty| {
                KnownClass::Iterable.to_specialized_instance(
                    db,
                    self.program_environment(),
                    &[yield_ty],
                )
            })
        } else {
            tcx
        };
        self.infer_expression(elt, elt_tcx);
        self.infer_comprehensions(generators);
    }

    fn infer_list_comprehension_expression_scope(
        &mut self,
        listcomp: &ast::ExprListComp,
        tcx: TypeContext<'db>,
    ) {
        let ast::ExprListComp {
            range: _,
            node_index: _,
            elt,
            generators,
        } = listcomp;

        // Infer the element type using the outer type context.
        let elts = [[Some(elt.as_ref())]];
        let mut infer_elt_ty =
            |builder: &mut Self, (_, elt, tcx)| builder.infer_expression(elt, tcx);

        self.infer_collection_literal(
            KnownClass::List,
            Some(listcomp.into()),
            &elts,
            &mut infer_elt_ty,
            tcx,
        );

        self.infer_comprehensions(generators);
    }

    fn infer_set_comprehension_expression_scope(
        &mut self,
        setcomp: &ast::ExprSetComp,
        tcx: TypeContext<'db>,
    ) {
        let ast::ExprSetComp {
            range: _,
            node_index: _,
            elt,
            generators,
        } = setcomp;

        // Infer the element type using the outer type context.
        let elts = [[Some(elt.as_ref())]];
        let mut infer_elt_ty =
            |builder: &mut Self, (_, elt, tcx)| builder.infer_expression(elt, tcx);

        self.infer_collection_literal(
            KnownClass::Set,
            Some(setcomp.into()),
            &elts,
            &mut infer_elt_ty,
            tcx,
        );

        self.infer_comprehensions(generators);
    }

    fn infer_dict_comprehension_expression_scope(
        &mut self,
        dictcomp: &ast::ExprDictComp,
        tcx: TypeContext<'db>,
    ) {
        let ast::ExprDictComp {
            range: _,
            node_index: _,
            key,
            value,
            generators,
        } = dictcomp;

        if key.is_some() {
            // Infer the key and value types using the outer type context.
            let elts = [[key.as_deref(), Some(value.as_ref())]];
            let mut infer_elt_ty =
                |builder: &mut Self, (_, elt, tcx)| builder.infer_expression(elt, tcx);

            self.infer_collection_literal(
                KnownClass::Dict,
                Some(dictcomp.into()),
                &elts,
                &mut infer_elt_ty,
                tcx,
            );
        } else {
            // Dict-unpack comprehensions are typed by the outer expression inference. Inferring
            // them through the collection-literal helper here would report the same invalid
            // mapping diagnostic twice.
            self.infer_expression(value, TypeContext::default());
        }

        self.infer_comprehensions(generators);
    }

    fn infer_comprehensions(&mut self, comprehensions: &[ast::Comprehension]) {
        let mut comprehensions_iter = comprehensions.iter();
        let Some(first_comprehension) = comprehensions_iter.next() else {
            unreachable!("Comprehension must contain at least one generator");
        };
        self.infer_comprehension(first_comprehension, true);
        for comprehension in comprehensions_iter {
            self.infer_comprehension(comprehension, false);
        }
    }

    fn infer_comprehension(&mut self, comprehension: &ast::Comprehension, is_first: bool) {
        let db = self.db();
        let env = self.program_environment();
        let ast::Comprehension {
            range: _,
            node_index: _,
            target,
            iter,
            ifs,
            is_async: _,
        } = comprehension;

        self.infer_target(target, iter, &|builder, tcx| {
            // TODO: `infer_comprehension_definition` reports a diagnostic if `iter_ty` isn't iterable
            //  but only if the target is a name. We should report a diagnostic here if the target isn't a name:
            //  `[... for a.x in not_iterable]
            if is_first {
                infer_same_file_expression_type(builder.db(), builder.index.expression(iter), tcx)
            } else {
                builder.infer_maybe_standalone_expression(iter, tcx)
            }
            .iterate(db, env)
            .homogeneous_element_type(db, env)
        });

        for expr in ifs {
            let test_ty = self.infer_maybe_standalone_expression(expr, TypeContext::default());

            if let Err(err) = test_ty.try_bool(db, env) {
                err.report_diagnostic(&self.context, expr);
            }

            self.check_condition_redundancy(expr, test_ty);
        }
    }

    fn infer_comprehension_definition(
        &mut self,
        comprehension: &ComprehensionDefinitionKind<'db>,
        definition: Definition<'db>,
    ) {
        let db = self.db();
        let iterable = comprehension.iterable(self.module());
        let target = comprehension.target(self.module());

        let mut infer_iterable_type = || {
            let expression = self.index.expression(iterable);
            let result = infer_expression_types(self.db(), expression, TypeContext::default());
            let iterable_type = result.expression_type(iterable);
            let element_type = if comprehension.is_async() {
                None
            } else {
                self.fixed_length_iterable_element_type(iterable, |expr| {
                    result.expression_type(expr)
                })
            };

            // Two things are different if it's the first comprehension:
            // (1) We must lookup the `ScopedExpressionId` of the iterable expression in the outer scope,
            //     because that's the scope we visit it in in the semantic index builder
            // (2) We must *not* call `self.extend()` on the result of the type inference,
            //     because `ScopedExpressionId`s are only meaningful within their own scope, so
            //     we'd add types for random wrong expressions in the current scope
            if !(comprehension.is_first() && target.is_name_expr()) {
                self.extend_expression_unchecked(result);
            }

            (iterable_type, element_type)
        };

        let target_type = match comprehension.target_kind() {
            TargetKind::Sequence(unpack_position, unpack) => {
                let unpacked = infer_unpack_types(self.db(), unpack);
                if unpack_position == UnpackPosition::First {
                    self.context.extend(unpacked.diagnostics());
                }

                unpacked.expression_type(target)
            }
            TargetKind::Single => {
                let (iterable_type, element_type) = infer_iterable_type();

                if let Some(element_type) = element_type {
                    element_type
                } else {
                    let env = self.program_environment();
                    iterable_type
                        .try_iterate_with_mode(
                            db,
                            env,
                            EvaluationMode::from_is_async(comprehension.is_async()),
                        )
                        .map(|tuple| tuple.homogeneous_element_type(db, env))
                        .unwrap_or_else(|err| {
                            err.report_diagnostic(&self.context, iterable_type, iterable.into());
                            err.fallback_element_type(db, env)
                        })
                }
            }
        };

        self.expressions.insert(target.into(), target_type);
        self.add_binding(target.into(), definition)
            .insert(self, target_type);
    }

    fn infer_named_expression(&mut self, named: &ast::ExprNamed) -> Type<'db> {
        // See https://peps.python.org/pep-0572/#differences-between-assignment-expressions-and-assignment-statements
        if named.target.is_name_expr() && !self.in_string_annotation() {
            let definition = self.index.expect_single_definition(named);
            let result = infer_definition_types(self.db(), definition);
            self.extend_definition(definition, result);
            result.binding_type(definition)
        } else {
            // String annotations have no indexed definitions, and syntactically invalid targets
            // cannot define a name. Both sides still need inference to preserve their diagnostics.
            self.infer_expression(&named.target, TypeContext::default());
            self.infer_expression(&named.value, TypeContext::default());
            Type::unknown()
        }
    }

    fn infer_named_expression_definition(
        &mut self,
        named: &'ast ast::ExprNamed,
        definition: Definition<'db>,
    ) -> Type<'db> {
        let ast::ExprNamed {
            range: _,
            node_index: _,
            target,
            value,
        } = named;

        let add = self.add_binding(named.target.as_ref().into(), definition);

        let ty = self.infer_expression(value, add.type_context());
        self.store_expression_type(target, ty);
        add.insert(self, ty)
    }

    fn infer_if_expression(
        &mut self,
        if_expression: &ast::ExprIf,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let ast::ExprIf {
            range: _,
            node_index: _,
            test,
            body,
            orelse,
        } = if_expression;

        let test_ty = self.infer_maybe_standalone_expression(test, TypeContext::default());
        let (body_ty, orelse_ty) = if is_collection_literal(body)
            && prefer_collection_literal_peer_context(db, env, tcx)
        {
            // Infer the peer branch first so the body can use its type as context.
            let orelse_ty = self.infer_expression(orelse, tcx);
            let body_ty = self.infer_expression_with_collection_literal_peer_context(
                body,
                tcx,
                Some(orelse_ty),
            );
            (body_ty, orelse_ty)
        } else {
            let body_ty = self.infer_expression(body, tcx);
            let orelse_ty = self.infer_expression_with_collection_literal_peer_context(
                orelse,
                tcx,
                Some(body_ty),
            );
            (body_ty, orelse_ty)
        };

        let test_truthiness = match test_ty.try_bool(db, env) {
            Ok(_) => analyze_condition_expression(test, &|node| {
                self.comparison_truthiness
                    .get(&node.into())
                    .copied()
                    .or_else(|| self.expression_type(node).bool_if_inhabited(db, env))
            })
            .unwrap_or(Truthiness::Ambiguous),
            Err(err) => {
                err.report_diagnostic(&self.context, &**test);
                err.fallback_truthiness()
            }
        };

        self.check_condition_redundancy(test, test_ty);

        match test_truthiness {
            Truthiness::AlwaysTrue => body_ty,
            Truthiness::AlwaysFalse => orelse_ty,
            Truthiness::Ambiguous => UnionType::from_two_elements(db, env, body_ty, orelse_ty),
        }
    }

    fn infer_lambda_body(&mut self, lambda_expression: &ast::ExprLambda, tcx: TypeContext<'db>) {
        self.infer_expression(&lambda_expression.body, tcx);
    }

    fn infer_lambda_expression(
        &mut self,
        lambda_expression: &ast::ExprLambda,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let ast::ExprLambda {
            range: _,
            node_index: _,
            parameters,
            body: _,
        } = lambda_expression;

        // In stub files, default values may reference names that are defined later in the file.
        let previous_deferred_state = self.replace_deferred_state(self.in_stub().into());

        // TODO: We could perform multi-inference here if there are multiple `Callable` annotations
        // in the union/intersection.
        let callable_tcx = if let Some(tcx) = tcx.annotation
            && let Some(callable) = tcx
                .filter_union(db, env, Type::is_callable_type)
                .resolve_type_alias(db)
                .as_callable()
        {
            match callable.signatures(self.db()).overloads.as_slice() {
                [signature] => Some(signature),
                // TODO: We could similarly perform multi-inference here if there are multiple overloads.
                _ => None,
            }
        } else {
            None
        };

        // Extract the annotated parameter types.
        //
        // Note that `Callable` annotations are only valid for positional parameters.
        let mut parameter_types = match callable_tcx {
            None => [].iter(),
            Some(signature) => signature.parameters().into_iter(),
        }
        .map(Parameter::annotated_type);

        let parameters = if let Some(parameters) = parameters {
            let positional_only = parameters
                .posonlyargs
                .iter()
                .map(|param| {
                    let parameter = Parameter::positional_only(Some(param.name().id.clone()))
                        .with_inferred_type(Type::Dynamic(DynamicType::UnknownLambdaParameter))
                        .with_optional_default_type(param.default().map(|default_expr| {
                            self.infer_expression(default_expr, TypeContext::default())
                                .replace_parameter_defaults(db, env)
                        }));

                    if let Some(annotated_type) = parameter_types.next() {
                        parameter.with_annotated_type(annotated_type)
                    } else {
                        parameter
                    }
                })
                .collect::<Vec<_>>();
            let positional_or_keyword = parameters
                .args
                .iter()
                .map(|param| {
                    let parameter = Parameter::positional_or_keyword(param.name().id.clone())
                        .with_inferred_type(Type::Dynamic(DynamicType::UnknownLambdaParameter))
                        .with_optional_default_type(param.default().map(|default_expr| {
                            self.infer_expression(default_expr, TypeContext::default())
                                .replace_parameter_defaults(db, env)
                        }));

                    if let Some(annotated_type) = parameter_types.next() {
                        parameter.with_annotated_type(annotated_type)
                    } else {
                        parameter
                    }
                })
                .collect::<Vec<_>>();
            let variadic = parameters.vararg.as_ref().map(|param| {
                Parameter::variadic(param.name().id.clone())
                    .with_inferred_type(Type::Dynamic(DynamicType::UnknownLambdaParameter))
            });
            let keyword_only = parameters
                .kwonlyargs
                .iter()
                .map(|param| {
                    Parameter::keyword_only(param.name().id.clone())
                        .with_inferred_type(Type::Dynamic(DynamicType::UnknownLambdaParameter))
                        .with_optional_default_type(param.default().map(|default_expr| {
                            self.infer_expression(default_expr, TypeContext::default())
                                .replace_parameter_defaults(db, env)
                        }))
                })
                .collect::<Vec<_>>();
            let keyword_variadic = parameters.kwarg.as_ref().map(|param| {
                Parameter::keyword_variadic(param.name().id.clone())
                    .with_inferred_type(Type::Dynamic(DynamicType::UnknownLambdaParameter))
            });

            let parameters = positional_only
                .into_iter()
                .chain(positional_or_keyword)
                .chain(variadic)
                .chain(keyword_only)
                .chain(keyword_variadic);

            Parameters::from_annotation(db, parameters)
        } else {
            Parameters::empty()
        };

        self.deferred_state = previous_deferred_state;

        let Some(scope_id) = self
            .index
            .try_node_scope(NodeWithScopeRef::Lambda(lambda_expression))
        else {
            return Type::unknown();
        };

        let scope = scope_id.to_scope_id(self.db(), self.program_file());

        // If we have a direct `Callable` type context, we can infer the body with the annotated
        // return type as type context.
        let return_tcx = if let Some(signature) = callable_tcx {
            match signature.return_ty {
                Type::Dynamic(DynamicType::Unknown) => TypeContext::new(None),
                _ => TypeContext::new(Some(signature.return_ty)),
            }
        } else {
            // TODO: Useful inference of a lambda's return type will require a different approach,
            // which does the inference of the body expression based on arguments at each call site,
            // rather than eagerly computing a return type without knowing the argument types.
            TypeContext::new(None)
        };

        let inference = infer_scope_types(self.db(), scope, return_tcx);
        self.extend_scope(inference);

        let return_ty = inference.expression_type(lambda_expression.body.as_ref());
        Type::Callable(CallableType::new(
            self.db(),
            CallableSignature::single(Signature::new(parameters, return_ty)),
            CallableTypeKind::FunctionLike,
        ))
    }

    /// Attempt to narrow a splatted dictionary argument based on the narrowed types of individual
    /// keys, if any.
    ///
    /// Returns the intersection between the dictionary type and a synthesized typed dict of any narrowed
    /// keys, or `None` otherwise.
    fn try_narrow_dict_kwargs(
        &self,
        argument_type: Type<'db>,
        argument: &'ast ast::ArgOrKeyword,
    ) -> Option<Type<'db>> {
        // Parsed string annotations are not indexed, so their keyword arguments have no
        // use-definition information from which to narrow dictionary keys.
        if self.in_string_annotation() {
            return None;
        }

        let env = self.program_environment();
        let db = self.db();
        let file_scope_id = self.scope().file_scope_id(db);
        let use_def = self.index.use_def_map(file_scope_id);

        let keyword = argument.as_variadic()?;

        if !argument_type
            .as_nominal_instance()?
            .has_known_class(db, KnownClass::Dict)
        {
            return None;
        }

        let definition_key = |definition: Definition<'_>| {
            let key = match definition.kind(db) {
                DefinitionKind::DictKeyAssignment(assignment) => assignment.key(self.module()),
                DefinitionKind::Assignment(assignment) => {
                    &assignment.target(self.module()).as_subscript_expr()?.slice
                }
                DefinitionKind::AnnotatedAssignment(assignment) => {
                    &assignment.target(self.module()).as_subscript_expr()?.slice
                }
                _ => return None,
            };

            Some(key.as_string_literal_expr()?.value.to_str())
        };

        // Collect the types of each distinct key.
        let mut elements: Vec<(&str, Type<'db>)> = Vec::new();
        for bindings in
            use_def.multi_bindings_at_use(keyword.scoped_use_id(db, self.program_file()))
        {
            let place = place_from_bindings_with_reachability_cache(
                db,
                env,
                bindings.clone(),
                self.reachability_cache(),
            );
            let Some(key) = place.first_definition.and_then(definition_key) else {
                continue;
            };

            if let Place::Defined(DefinedPlace {
                ty: field_ty,
                definedness: Definedness::AlwaysDefined,
                ..
            }) = place.place
            {
                elements.push((key, field_ty));
            }
        }

        if elements.is_empty() {
            return None;
        }

        // Synthesize overloads for `__getitem__` based on known dictionary elements.
        let getitem_overloads = elements.into_iter().map(|(name, ty)| {
            Signature::new(
                Parameters::standard([
                    Parameter::positional_only(Some(Name::new_static("self"))),
                    Parameter::positional_or_keyword(Name::new_static("key"))
                        .with_annotated_type(Type::string_literal(db, name)),
                ]),
                ty,
            )
        });

        let getitem_protocol = Type::protocol_with_methods(
            db,
            env,
            [(
                "__getitem__",
                CallableType::new(
                    db,
                    CallableSignature::from_overloads(getitem_overloads),
                    CallableTypeKind::FunctionLike,
                ),
            )],
        );

        // Note that we return an intersection to preserve the original dictionary type,
        // as it may contain keys that were not explicitly assigned to.
        Some(IntersectionType::from_elements(
            db,
            env,
            [argument_type, getitem_protocol],
        ))
    }

    /// Infer the variadic argument types needed for call binding and emit the shared diagnostics
    /// for invalid `*args` and `**kwargs` inputs.
    fn prepare_call_arguments<'a>(
        &mut self,
        arguments: &'a ast::Arguments,
    ) -> CallArguments<'a, 'db> {
        local::prepare_arguments(self, arguments)
    }

    // TODO: This should not be needed once we use constraint sets to track the usages of each
    // container literal across a scope.
    // https://github.com/astral-sh/ty/issues/3507
    fn collection_use_constraint_from_specialization(
        &self,
        identity_instance: Type<'db>,
        receiver_generic_context: Option<GenericContext<'db>>,
        call_specialization: Specialization<'db>,
    ) -> Option<Type<'db>> {
        let db = self.db();
        let env = self.program_environment();
        let constraint = identity_instance.apply_specialization(db, call_specialization);
        let Some(receiver_generic_context) = receiver_generic_context else {
            return Some(constraint);
        };

        // Method-local typevars describe requirements imposed by the method, not concrete element
        // types learned for the collection. Until collection-use constraints are represented as
        // projected constraint sets, avoid leaking those method-local typevars into the inferred
        // collection literal type.
        if any_over_type(db, env, constraint, false, |ty| {
            ty.as_typevar().is_some_and(|typevar| {
                !receiver_generic_context.contains(self.db(), typevar.identity(self.db()))
            })
        }) {
            return None;
        }

        Some(constraint)
    }

    fn infer_call_expression(
        &mut self,
        call_expression: &ast::ExprCall,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        local::call(self, call_expression, None, tcx)
    }

    /// Infer a callable expression without introducing new type variable bindings.
    ///
    /// An assignment such as `items = list[T]()` creates an instance, not a generic alias.
    /// Unlike `Items = list[T]`, it requires `T` to be bound in an enclosing generic scope.
    fn infer_callee(&mut self, expression: &ast::Expr) -> Type<'db> {
        local::callee(self, expression)
    }

    fn infer_empty_list_or_set_constructor(
        &mut self,
        collection_class: KnownClass,
        call_expression: &ast::ExprCall,
        tcx: TypeContext<'db>,
    ) -> Option<Type<'db>> {
        let elements: [[Option<&ast::Expr>; 1]; 0] = [];
        let mut infer_element_ty = |_: &mut Self, _| Type::unknown();

        self.infer_collection_literal(
            collection_class,
            Some(call_expression.into()),
            &elements,
            &mut infer_element_ty,
            tcx,
        )
    }

    /// Infers a truthiness-refined `range` instance for literal built-in `range(...)` calls.
    ///
    /// The refinement only records whether the constructed range is statically non-empty. Dynamic
    /// arguments, keyword arguments, starred arguments, shadowed `range` callables, and invalid
    /// literal forms fall back to the ordinary `range` instance.
    ///
    /// This uses the argument types inferred by normal call binding; it does not re-infer
    /// arguments just to compute the refinement.
    ///
    /// ```python
    /// range(3)        # known non-empty
    /// range(3, 0, -1) # known non-empty
    /// range(n)        # ordinary range
    /// ```
    fn infer_builtin_range_instance_type(
        &self,
        callable_type: Type<'db>,
        arguments: &ast::Arguments,
        call_arguments: &CallArguments<'_, 'db>,
    ) -> Option<Type<'db>> {
        crate::types::signatures::effects::legacy_inline(
            range::infer_builtin_range_instance_type_with(
                self,
                callable_type,
                arguments,
                call_arguments,
                &range::OrdinaryRangeInferenceEffects,
            ),
        )
    }

    fn infer_builtin_range_instance_type_positive(
        &self,
        arguments: &ast::Arguments,
        call_arguments: &CallArguments<'_, 'db>,
    ) -> Option<Type<'db>> {
        if !arguments.keywords.is_empty() || arguments.args.iter().any(ast::Expr::is_starred_expr) {
            return None;
        }

        let int_literal = |argument_index: usize| {
            call_arguments
                .argument_types(argument_index)?
                .get_default()?
                .as_int_literal()
        };

        let is_non_empty = match arguments.args.len() {
            1 => int_literal(0)? > 0,
            2 => int_literal(0)? < int_literal(1)?,
            3 => {
                let start = int_literal(0)?;
                let stop = int_literal(1)?;
                let step = int_literal(2)?;

                match step.cmp(&0) {
                    std::cmp::Ordering::Greater => start < stop,
                    std::cmp::Ordering::Less => start > stop,
                    std::cmp::Ordering::Equal => return None,
                }
            }
            _ => return None,
        };

        Some(Type::KnownInstance(KnownInstanceType::Range {
            is_non_empty,
        }))
    }

    fn infer_call_expression_impl(
        &mut self,
        call_expression: &ast::ExprCall,
        callable_type: Type<'db>,
        call_expression_tcx: TypeContext<'db>,
    ) -> Type<'db> {
        local::call(
            self,
            call_expression,
            Some(callable_type),
            call_expression_tcx,
        )
    }

    fn infer_starred_expression(
        &mut self,
        starred: &ast::ExprStarred,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let env = self.program_environment();
        let ast::ExprStarred {
            range: _,
            node_index: _,
            value,
            ctx: _,
        } = starred;

        let db = self.db();
        let iterable_type = self.infer_expression(value, tcx);
        let typevartuple = match iterable_type {
            Type::KnownInstance(KnownInstanceType::TypeVar(typevar))
                if typevar.is_typevartuple(db) =>
            {
                bind_typevar(
                    self.db(),
                    self.index,
                    self.scope().file_scope_id(db),
                    self.typevar_binding_context,
                    typevar,
                )
            }
            Type::TypeVar(typevar) if typevar.is_typevartuple(db) => Some(typevar),
            _ => None,
        };
        if let Some(typevartuple) = typevartuple {
            return Type::tuple(TupleType::new(
                db,
                env,
                &TupleSpecBuilder::with_capacity(0)
                    .concat_variadic_typevar(db, env, typevartuple)
                    .build(),
            ));
        }
        iterable_type
            .try_iterate(db, env)
            .map(|spec| Type::tuple(TupleType::new(db, env, &spec)))
            .unwrap_or_else(|err| {
                err.report_diagnostic(&self.context, iterable_type, value.as_ref().into());
                Type::homogeneous_tuple(db, env, err.fallback_element_type(db, env))
            })
    }

    fn infer_yield_expression(&mut self, yield_expression: &ast::ExprYield) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let ast::ExprYield {
            range: _,
            node_index: _,
            value,
        } = yield_expression;
        let Some(enclosing_function) = nearest_enclosing_function(db, self.index, self.scope())
        else {
            let _ = self.infer_optional_expression(value.as_deref(), TypeContext::default());
            return Type::unknown();
        };
        let declared_return_ty = same_module_uncached_raw_signature(
            db,
            enclosing_function,
            ReturnCallableTypeVarScope::Public,
        )
        .return_ty;
        let return_type_span = enclosing_function.spans(self.db()).return_type;

        let Some(generator_type_params) =
            declared_return_ty.generator_types(db, env, GeneratorTypeMode::IteratorDefaults)
        else {
            let _ = self.infer_optional_expression(value.as_deref(), TypeContext::default());
            return Type::unknown();
        };

        let expected_yield_ty = generator_type_params.yield_ty;
        let tcx = TypeContext::new(expected_yield_ty);
        let yielded_ty = self
            .infer_optional_expression(value.as_deref(), tcx)
            .unwrap_or_else(|| Type::none(db, env));
        let diagnostic_node: AnyNodeRef = value
            .as_deref()
            .map_or_else(|| yield_expression.into(), AnyNodeRef::from);

        if let Some(expected_yield_ty) = expected_yield_ty {
            self.validate_generator_yield_type(
                diagnostic_node,
                YieldKind::Yield,
                return_type_span,
                expected_yield_ty,
                yielded_ty,
            );
        }

        generator_type_params.send_ty.unwrap_or_else(Type::unknown)
    }

    fn infer_yield_from_expression(&mut self, yield_from: &ast::ExprYieldFrom) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let ast::ExprYieldFrom {
            range: _,
            node_index: _,
            value,
        } = yield_from;

        let Some(enclosing_function) = nearest_enclosing_function(db, self.index, self.scope())
        else {
            let _ = self.infer_expression(value, TypeContext::default());
            return Type::unknown();
        };
        let annotated_return_ty = same_module_uncached_raw_signature(
            db,
            enclosing_function,
            ReturnCallableTypeVarScope::Public,
        )
        .return_ty;

        let Some(outer_expected) =
            annotated_return_ty.generator_types(db, env, GeneratorTypeMode::IteratorDefaults)
        else {
            let _ = self.infer_expression(value, TypeContext::default());
            return Type::unknown();
        };
        let return_type_span = enclosing_function.spans(self.db()).return_type;

        let tcx = TypeContext::new(outer_expected.yield_ty.map(|yielded_ty| {
            KnownClass::Iterable.to_specialized_instance(db, env, &[yielded_ty])
        }));
        let iterable_type = self.infer_expression(value, tcx);

        let known_inner_yield_type = match iterable_type.try_iterate(db, env) {
            Ok(tuple) => Some(tuple.homogeneous_element_type(db, env)),
            Err(err) => {
                err.report_diagnostic(&self.context, iterable_type, AnyNodeRef::from(&**value));
                err.element_type(db, env)
            }
        };

        if let Some(outer_yield_ty) = outer_expected.yield_ty
            && let Some(known_inner_yield_type) = known_inner_yield_type
        {
            self.validate_generator_yield_type(
                &**value,
                YieldKind::YieldFrom,
                return_type_span.clone(),
                outer_yield_ty,
                known_inner_yield_type,
            );
        }

        // `yield from x` delegates to `iter(x)`, so the send and return types of the
        // expression are those of the *iterator*. If `x` is itself a generator, that's `x`.
        // Otherwise, e.g. for an instance of a class whose `__iter__` method is a
        // generator function, we look at the return type of `x.__iter__()`.
        let inner_generator = iterable_type
            .generator_types(db, env, GeneratorTypeMode::GeneratorOnly)
            .map(|types| (iterable_type, types))
            .or_else(|| {
                let iterator_type = match iterable_type.try_call_dunder(
                    db,
                    env,
                    "__iter__",
                    CallArguments::none(),
                    TypeContext::default(),
                ) {
                    Ok(bindings) => Some(bindings.return_type(db, env)),
                    Err(CallDunderError::PossiblyUnbound { .. }) => {
                        // Iteration can fall back to `__getitem__` where `__iter__` is absent.
                        // The available `__iter__` bindings do not describe those alternatives.
                        None
                    }
                    Err(err) => err.return_type(db, env),
                }?;
                // `Iterator` has no type parameter for `StopIteration.value`.
                iterator_type
                    .generator_types(db, env, GeneratorTypeMode::GeneratorOnly)
                    .map(|types| (iterator_type, types))
            });

        // `Iterator` annotations constrain yielded values but do not expose a send method.
        if let Some(outer_send_ty) = annotated_return_ty.generator_annotation_send_type(db, env) {
            let incompatible_send_ty = match inner_generator {
                Some((iterator_type, _)) => {
                    iterator_type.incompatible_yield_from_send_type(db, env, outer_send_ty)
                }
                None => {
                    let none = Type::none(db, env);
                    (!outer_send_ty.is_assignable_to(db, env, none)).then_some(none)
                }
            };
            if let Some(inner_send_ty) = incompatible_send_ty {
                report_invalid_generator_yield_type(
                    &self.context,
                    value.as_ref(),
                    return_type_span,
                    outer_send_ty,
                    inner_send_ty,
                    GeneratorMismatchKind::SendType,
                );
            }
        }

        inner_generator
            .and_then(|(_, generator_types)| generator_types.return_ty)
            .unwrap_or_else(Type::unknown)
    }

    fn validate_generator_yield_type(
        &self,
        yielded_value: impl Ranged,
        yield_kind: YieldKind,
        return_type_span: Option<Span>,
        expected_yield_ty: Type<'db>,
        yielded_ty: Type<'db>,
    ) {
        let db = self.db();
        let env = self.program_environment();

        if !yielded_ty.is_assignable_to(db, env, expected_yield_ty) {
            report_invalid_generator_yield_type(
                &self.context,
                yielded_value,
                return_type_span,
                expected_yield_ty,
                yielded_ty,
                GeneratorMismatchKind::YieldType,
            );
        } else if self.context.is_lint_enabled(&UNSOUND_YIELD)
            && expected_yield_ty.is_fully_static(db, env)
            && !yielded_ty.is_pure_redundant_with(db, env, expected_yield_ty)
        {
            // N.B. the implementation here is the ~same as for `UNSOUND_RETURN_STATEMENT` and `UNSOUND_ASSIGNMENT`;
            // update those too if updating this!
            report_unsound_yield(
                &self.context,
                yielded_value,
                yield_kind,
                return_type_span,
                expected_yield_ty,
                yielded_ty,
            );
        }
    }

    // Perform narrowing with applicable constraints between the current scope and the enclosing scope.
    fn narrow_place_with_applicable_constraints(
        &self,
        expr: PlaceExprRef,
        ty: Type<'db>,
        constraint_keys: &[(FileScopeId, ConstraintKey)],
    ) -> Type<'db> {
        match applicable_constraints::narrow_place_with_applicable_constraints_sync(
            self,
            expr,
            ty,
            constraint_keys,
            applicable_constraints::ApplicableConstraintsFacts,
            &applicable_constraints::OrdinaryApplicableConstraintsEffects,
        ) {
            Ok(ty) => ty,
            Err(never) => match never {},
        }
    }

    /// Compute the type for reads such as `box.value` or `items[0]` in loop iterations after
    /// `box` or `items` has been assigned a new object.
    ///
    /// A check on `box.value` before `box = Box()` describes the old box, not the new one.
    /// We start again from the type given by attribute lookup, then apply any checks made
    /// after the assignment. For example:
    ///
    /// ```py
    /// class Box:
    ///     value: int | None
    ///
    /// def f(box: Box):
    ///     assert box.value is not None
    ///     for _ in range(2):
    ///         reveal_type(box.value)  # revealed: int
    ///         box = Box()
    ///         assert box.value is not None
    /// ```
    ///
    /// The first assertion gives `int` for the first iteration. After `box = Box()`, the
    /// new box's value could be `None`; the second assertion narrows it to `int` for the
    /// next iteration. This helper computes that contribution from the previous iteration.
    ///
    /// `fallback_ty` is the starting type before applying those later checks: `int | None`
    /// here. The caller obtains it when inferring the attribute or item read being checked,
    /// including any narrowing already applied from enclosing scopes. This helper reuses
    /// that type; it does not look it up at the assignment or at the end of the loop.
    /// Recursive calls preserve checks made after the assignment within nested loops as well.
    fn loop_header_fallback_type(
        &self,
        definition: Definition<'db>,
        fallback_ty: Type<'db>,
        cache: &mut FxHashMap<Definition<'db>, Type<'db>>,
    ) -> Type<'db> {
        // Inner headers can be reached through multiple containing headers. The fallback type
        // is fixed for this traversal, so each definition's contribution only needs computing once.
        if let Some(ty) = cache.get(&definition) {
            return *ty;
        }

        let db = self.db();
        let env = self.program_environment();
        let header = loop_header_reachability(db, definition);
        let use_def = self.index.use_def_map(definition.file_scope(db));
        let place = definition.place(db);
        let mut union = UnionBuilder::new(db, env);

        for constraint in &header.deleted_narrowing_constraints {
            union.add_in_place(use_def.narrowing_evaluator(*constraint).narrow(
                db,
                env,
                fallback_ty,
                place,
            ));
        }
        for binding in &header.reachable_bindings {
            if binding.definition.kind(db).is_loop_header() {
                let ty = self.loop_header_fallback_type(binding.definition, fallback_ty, cache);
                union.add_in_place(
                    use_def
                        .narrowing_evaluator(binding.narrowing_constraint)
                        .narrow(db, env, ty, place),
                );
            }
        }

        let ty = union.build();
        cache.insert(definition, ty);
        ty
    }

    /// Check if the given ty is `@deprecated` or not
    fn check_deprecated<T: Ranged>(&self, ranged: T, ty: Type<'db>) {
        crate::types::signatures::effects::legacy_inline(self.check_deprecated_with(
            &source_expression::LegacySourceExpressionEffects,
            ranged,
            ty,
        ));
    }

    /// Report the distinct deprecated targets of one operation in a single diagnostic.
    /// Deduplicate by source function or overload. Keep a shared deprecation message in the
    /// primary annotation so it appears in concise output. Put differing messages in separate
    /// subdiagnostics with their declarations so each message's prose and line breaks remain
    /// readable. The summary names the possible deprecated targets in both output formats.
    fn report_deprecated_functions(
        &self,
        ranged: impl Ranged,
        functions: impl IntoIterator<Item = OverloadLiteral<'db>>,
    ) {
        let db = self.db();
        let functions: SmallVec<[_; 1]> = functions.into_iter().unique().collect();
        let Some(first) = functions.first() else {
            return;
        };
        let Some(builder) = self.context.report_lint(&diagnostic::DEPRECATED, ranged) else {
            return;
        };
        let shared_message = functions
            .iter()
            .filter_map(|function| function.deprecated(db)?.message)
            .map(|message| message.value(db))
            .filter(|message| !message.is_empty())
            .all_equal_value()
            .ok();
        let mut diagnostic = if functions.len() == 1 {
            let kind = if first.is_overload(db) {
                "overload of"
            } else {
                "function"
            };
            builder.into_diagnostic(format_args!(
                "The {kind} `{}` is deprecated",
                first.name(db)
            ))
        } else {
            let mut names = FxOrderSet::default();
            let mut all_methods = true;
            for function in &functions {
                let description = CallableDescription::from_overload(db, *function);
                names.insert(description.name());
                all_methods &= description.kind() == Some("method");
            }
            let kind = if all_methods { "method" } else { "function" };
            let plural = if names.len() == 1 { "" } else { "s" };
            let names = names
                .iter()
                .format_with(", ", |name, f| f(&format_args!("`{name}`")));
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Possible use of deprecated {kind}{plural}: {names}"
            ));
            for function in &functions {
                if shared_message.is_some() {
                    diagnostic.annotate(Annotation::secondary(function.spans(db).name));
                    continue;
                }
                let message = function
                    .deprecated(db)
                    .and_then(|deprecated| deprecated.message)
                    .map(|message| message.value(db))
                    .filter(|message| !message.is_empty())
                    .unwrap_or("Deprecated function defined here");
                let mut sub = SubDiagnostic::new(SubDiagnosticSeverity::Info, message);
                sub.annotate(Annotation::primary(function.spans(db).name));
                diagnostic.sub(sub);
            }
            diagnostic
        };
        if let Some(message) = shared_message {
            diagnostic.set_primary_annotation_message(message);
        }
        diagnostic.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Deprecated);
    }

    /// Report a deprecated callable only when its union alternative has no non-deprecated
    /// intersection member that could provide the implementation instead.
    fn check_deprecated_bindings<T: Ranged>(&self, ranged: &T, bindings: &Bindings<'db>) {
        self.report_deprecated_functions(
            ranged,
            bindings
                .deprecated_functions(self.db())
                .map(|(_, function)| function),
        );
    }

    /// Check the accessor invoked by an attribute operation, using the deprecations
    /// retained by member lookup or assignment validation. `access` describes the operation,
    /// which may differ from the AST context: an augmented assignment also reads its target.
    ///
    /// ```python
    /// from typing_extensions import deprecated
    ///
    /// class C:
    ///     @property
    ///     @deprecated("old getter")
    ///     def value(self) -> int:
    ///         return 0
    ///
    ///     @value.setter
    ///     def value(self, new: int) -> None: ...
    ///
    /// c = C()
    /// c.value = 1   # Only invokes the non-deprecated setter.
    /// c.value += 1  # Also invokes the deprecated getter.
    /// ```
    fn check_deprecated_property(
        &self,
        attribute: &ast::ExprAttribute,
        properties: PropertyDeprecations<'db>,
        access: ExprContext,
    ) {
        self.report_deprecated_functions(
            &attribute.attr,
            properties.functions(self.db(), access).iter().copied(),
        );
    }

    fn infer_name_load(&mut self, name_node: &ast::ExprName) -> Type<'db> {
        self.infer_name_load_with_definition(name_node).0
    }

    /// Resolve a name once, retaining its definition for type-alias interpretation.
    fn infer_name_load_with_definition(
        &mut self,
        name_node: &ast::ExprName,
    ) -> (Type<'db>, Option<Definition<'db>>) {
        crate::types::signatures::effects::legacy_inline(self.infer_name_load_with_definition_with(
            &source_expression::LegacySourceExpressionEffects,
            name_node,
        ))
    }

    /// Infer the type of a place expression from its ordered load sources.
    ///
    /// This also returns the [`ConstraintKey`]s used by expression-level narrowing.
    fn infer_place_load(
        &self,
        place_expr: PlaceExpr,
        expr_ref: ast::ExprRef,
    ) -> (PlaceAndQualifiers<'db>, Vec<(FileScopeId, ConstraintKey)>) {
        crate::types::signatures::effects::legacy_inline(self.infer_place_load_with(
            &source_expression::LegacySourceExpressionEffects,
            place_expr,
            expr_ref,
        ))
    }

    fn infer_place_load_source(
        &self,
        place_expr: PlaceExprRef,
        source: PlaceLoadSource<'db>,
        narrowing_constraints: &[(FileScopeId, ConstraintKey)],
    ) -> PlaceAndQualifiers<'db> {
        crate::types::signatures::effects::legacy_inline(self.infer_place_load_source_with(
            &source_expression::LegacySourceExpressionEffects,
            place_expr,
            source,
            narrowing_constraints,
        ))
    }

    /// Applies ty's convenience fallback for an unimported `reveal_type`.
    fn infer_unimported_reveal_type_fallback(
        &self,
        name: &ast::ExprName,
    ) -> PlaceAndQualifiers<'db> {
        if !self.in_stub() && !self.is_in_type_checking_block(self.scope(), name) {
            report_undefined_reveal(&self.context, name);
        }

        typing_extensions_symbol(self.db(), self.program_environment(), "reveal_type")
    }

    fn report_unresolved_reference(&self, expr_name_node: &ast::ExprName) {
        let db = self.db();
        let env = self.program_environment();
        let Some(builder) = self
            .context
            .report_lint(&UNRESOLVED_REFERENCE, expr_name_node)
        else {
            return;
        };

        let ast::ExprName { id, .. } = expr_name_node;
        let mut diagnostic =
            builder.into_diagnostic(format_args!("Name `{id}` used when not defined"));

        // ===
        // Subdiagnostic (1): check to see if it was added as a builtin in a later version of Python.
        // ===
        if let Some(version_added_to_builtins) = version_builtin_was_added(id) {
            diagnostic.info(format_args!(
                "`{id}` was added as a builtin in Python 3.{version_added_to_builtins}"
            ));
            add_inferred_python_version_hint_to_diagnostic(
                db,
                self.file(),
                &mut diagnostic,
                "resolving types",
            );
        }

        // ===
        // Subdiagnostic (2): check to see if it's a capitalized older type hint that is available as lowercase in this version of Python.
        // ===
        // We don't need to check for typing_extensions.Type,
        // because it's already caught by typing.Type.
        if self.program_environment().python_version(db) >= PythonVersion::PY39 {
            if let Some(("", builtin_name)) = as_pep_585_generic("typing", id) {
                diagnostic
                    .set_primary_annotation_message(format_args!("Did you mean `{builtin_name}`?"));
                if SemanticModel::new(db, self.program_file())
                    .definitely_has_builtin_binding(builtin_name, expr_name_node.into())
                {
                    diagnostic.help(format_args!("Replace with `{builtin_name}`"));
                    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                        builtin_name.to_string(),
                        expr_name_node.range(),
                    )));
                }
            }
        }

        // ===
        // Subdiagnostic (3):
        // - If it's an instance method, check to see if it's available as an attribute on `self`;
        // - If it's a classmethod, check to see if it's available as an attribute on `cls`
        // ===
        let Some(current_function) = self.current_function_definition() else {
            return;
        };

        let function_parameters = &*current_function.parameters;

        // `self`/`cls` can't be a keyword-only parameter.
        if function_parameters.posonlyargs.is_empty() && function_parameters.args.is_empty() {
            return;
        }

        let Some(first_parameter) = function_parameters.iter_non_variadic_params().next() else {
            return;
        };

        let Some(class) = self.class_context_of_current_method() else {
            return;
        };

        let first_parameter_name = first_parameter.name();

        let Some(function_type) = self.current_function_type() else {
            return;
        };

        let attribute_exists = match MethodDecorator::try_from_fn_type(self.db(), function_type) {
            Some(MethodDecorator::ClassMethod) => !Type::instance(db, env, class)
                .class_member(db, env, id)
                .place
                .is_undefined(),
            Some(MethodDecorator::None) => !Type::instance(db, env, class)
                .member(db, env, id)
                .place
                .is_undefined(),
            Some(MethodDecorator::StaticMethod) | None => false,
        };

        if attribute_exists {
            diagnostic.info(format_args!(
                "An attribute `{id}` is available: consider using `{first_parameter_name}.{id}`"
            ));
        }
    }

    fn narrow_expr_with_applicable_constraints<'r>(
        &mut self,
        target: impl Into<ast::ExprRef<'r>>,
        target_ty: Type<'db>,
        constraint_keys: &[(FileScopeId, ConstraintKey)],
    ) -> Type<'db> {
        match applicable_constraints::narrow_expr_with_applicable_constraints_sync(
            self,
            target.into(),
            target_ty,
            constraint_keys,
            applicable_constraints::ApplicableConstraintsFacts,
            &applicable_constraints::OrdinaryApplicableConstraintsEffects,
        ) {
            Ok(ty) => ty,
            Err(never) => match never {},
        }
    }

    /// Infer an attribute load, returning its recovery type if lookup fails.
    fn infer_attribute_load(
        &mut self,
        attribute: &ast::ExprAttribute,
    ) -> Result<TypeAndQualifiers<'db>, TypeAndQualifiers<'db>> {
        match attribute::infer_attribute_load_sync(
            self,
            attribute,
            attribute::AttributeFacts,
            &attribute::OrdinaryAttributeEffects,
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Reject access to a generic instance attribute through a class while retaining the normal
    /// attribute type for recovery. Reads and writes use the same restriction.
    fn validate_generic_class_attribute_access(
        &self,
        attribute: &ast::ExprAttribute,
        object_ty: Type<'db>,
        emit_diagnostics: bool,
    ) -> bool {
        match attribute::validate_generic_class_attribute_access_sync(
            self,
            attribute,
            object_ty,
            emit_diagnostics,
            &attribute::OrdinaryAttributeEffects,
        ) {
            Ok(valid) => valid,
            Err(never) => match never {},
        }
    }

    /// Infer an attribute load on a known receiver, returning its recovery type if lookup fails.
    fn infer_attribute_load_impl(
        &mut self,
        attribute: &ast::ExprAttribute,
        value_type: Type<'db>,
    ) -> Result<TypeAndQualifiers<'db>, TypeAndQualifiers<'db>> {
        match attribute::infer_attribute_load_impl_sync(
            self,
            attribute,
            value_type,
            attribute::AttributeFacts,
            &attribute::OrdinaryAttributeEffects,
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    fn infer_attribute_expression(&mut self, attribute: &ast::ExprAttribute) -> Type<'db> {
        match attribute::infer_attribute_expression_sync(
            self,
            attribute,
            attribute::AttributeFacts,
            &attribute::OrdinaryAttributeEffects,
        ) {
            Ok(ty) => ty,
            Err(never) => match never {},
        }
    }

    fn report_unsupported_unary_operator(
        &self,
        unary: &ast::ExprUnaryOp,
        op: ast::UnaryOp,
        operand_type: Type<'db>,
        unary_dunder_method: &str,
        error: Option<&CallDunderError<'db>>,
    ) {
        let db = self.db();
        let env = self.program_environment();
        let Some(builder) = self.context.report_lint(&UNSUPPORTED_OPERATOR, unary) else {
            return;
        };

        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Unary operator `{op}` is not supported for object of type `{}`",
            operand_type.display(db, env),
        ));

        if let Some(CallDunderError::PossiblyUnbound {
            unbound_on: Some(unbound_on),
            ..
        }) = error
        {
            for ty in unbound_on.iter().copied() {
                diagnostic.info(format_args!(
                    "`{}` does not implement `{unary_dunder_method}`",
                    ty.display(db, env)
                ));
            }
        }
    }

    fn infer_unary_expression(&mut self, unary: &ast::ExprUnaryOp) -> Type<'db> {
        let ast::ExprUnaryOp {
            range: _,
            node_index: _,
            op,
            operand,
        } = unary;

        let operand_type = self.infer_expression(operand, TypeContext::default());

        self.infer_unary_expression_type(*op, operand_type, unary)
    }

    fn infer_unary_expression_type(
        &mut self,
        op: ast::UnaryOp,
        operand_type: Type<'db>,
        unary: &ast::ExprUnaryOp,
    ) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let fallback_unary_expression_type = || {
            let unary_dunder_method = match op {
                ast::UnaryOp::Invert => "__invert__",
                ast::UnaryOp::UAdd => "__pos__",
                ast::UnaryOp::USub => "__neg__",
                ast::UnaryOp::Not => {
                    unreachable!("Not operator is handled in its own case");
                }
            };

            match operand_type.try_call_dunder(
                db,
                env,
                unary_dunder_method,
                CallArguments::none(),
                TypeContext::default(),
            ) {
                Ok(outcome) => {
                    self.check_deprecated_bindings(unary, &outcome);
                    outcome.return_type(db, env)
                }
                Err(e) => {
                    let bindings = match &e {
                        CallDunderError::PossiblyUnbound { bindings, .. } => Some(bindings),
                        CallDunderError::CallError(_, bindings, _) => Some(bindings),
                        CallDunderError::MethodNotAvailable => None,
                    };
                    if let Some(bindings) = bindings {
                        self.check_deprecated_bindings(unary, bindings);
                    }
                    self.report_unsupported_unary_operator(
                        unary,
                        op,
                        operand_type,
                        unary_dunder_method,
                        Some(&e),
                    );
                    e.fallback_return_type(db, env)
                }
            }
        };

        match (op, operand_type) {
            (_, Type::RecursiveVar(_)) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            (ast::UnaryOp::Invert | ast::UnaryOp::UAdd | ast::UnaryOp::USub, Type::Dynamic(_))
            | (_, Type::Divergent(_)) => operand_type,
            (_, Type::Never) => Type::Never,

            (_, Type::TypeAlias(alias)) => {
                self.infer_unary_expression_type(op, alias.value_type(db), unary)
            }

            (ast::UnaryOp::UAdd, Type::LiteralValue(literal)) => match literal.kind() {
                LiteralValueTypeKind::Int(value) => Type::int_literal(value.as_i64()),
                LiteralValueTypeKind::Bool(value) => Type::int_literal(i64::from(value)),
                _ => fallback_unary_expression_type(),
            },

            (ast::UnaryOp::USub, Type::LiteralValue(literal)) => match literal.kind() {
                LiteralValueTypeKind::Int(value) => value
                    .as_i64()
                    .checked_neg()
                    .map(Type::int_literal)
                    .unwrap_or_else(|| KnownClass::Int.to_instance(db, env)),
                LiteralValueTypeKind::Bool(value) => Type::int_literal(-i64::from(value)),
                _ => fallback_unary_expression_type(),
            },

            (ast::UnaryOp::Invert, Type::LiteralValue(literal)) => match literal.kind() {
                LiteralValueTypeKind::Int(value) => Type::int_literal(!value.as_i64()),
                LiteralValueTypeKind::Bool(value) => {
                    // `~bool` is currently deprecated in typeshed. Technically we should
                    // similarly check for deprecation of dunder methods on all our literal
                    // type fast paths, but we choose not to pay that extra cost, since it is
                    // implausible that e.g. `int.__neg__` would ever be deprecated.
                    if let Some(dunder) = literal
                        .fallback_instance(db, env)
                        .member_lookup_with_policy(
                            db,
                            env,
                            "__invert__",
                            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                        )
                        .place
                        .ignore_possibly_undefined()
                    {
                        self.check_deprecated(unary, dunder);
                    }
                    Type::int_literal(!i64::from(value))
                }
                _ => fallback_unary_expression_type(),
            },

            (ast::UnaryOp::Invert, Type::KnownInstance(KnownInstanceType::ConstraintSet(set))) => {
                let constraints = ConstraintSetBuilder::new();
                let result = constraints.into_owned(|constraints| {
                    let set = constraints.load(db, env, set.constraints(self.db()));
                    set.negate(self.db(), constraints)
                });
                Type::KnownInstance(KnownInstanceType::ConstraintSet(
                    InternedConstraintSet::new(self.db(), result),
                ))
            }

            (ast::UnaryOp::Not, ty) => {
                let original_truthiness = ty.try_bool(db, env).unwrap_or_else(|err| {
                    err.report_diagnostic(&self.context, unary);
                    err.fallback_truthiness()
                });

                self.check_negation_redundancy(unary, ty, original_truthiness);

                Type::from_truthiness(db, env, original_truthiness.negate())
            }
            (_, Type::Recursive(_)) => fallback_unary_expression_type(),
            // Handle constrained TypeVars specially: check each constraint individually.
            //
            // TODO: We expect to replace this with more general support once we migrate to the new
            // solver.
            (
                op @ (ast::UnaryOp::UAdd | ast::UnaryOp::USub | ast::UnaryOp::Invert),
                Type::TypeVar(tvar),
            ) => {
                let unary_dunder_method = match op {
                    ast::UnaryOp::Invert => "__invert__",
                    ast::UnaryOp::UAdd => "__pos__",
                    ast::UnaryOp::USub => "__neg__",
                    ast::UnaryOp::Not => unreachable!(),
                };

                match tvar.typevar(self.db()).bound_or_constraints(db, env) {
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        // Call the dunder method for every constraint up front so deprecation
                        // reporting doesn't depend on whether any constraint fails.
                        let outcomes: Vec<_> = constraints
                            .elements(db)
                            .iter()
                            .map(|constraint| {
                                constraint.try_call_dunder(
                                    db,
                                    env,
                                    unary_dunder_method,
                                    CallArguments::none(),
                                    TypeContext::default(),
                                )
                            })
                            .collect();
                        self.report_deprecated_functions(
                            unary,
                            outcomes
                                .iter()
                                .filter_map(|outcome| match outcome {
                                    Ok(bindings) => Some(bindings),
                                    // A method can be deprecated even if it is missing from some
                                    // union members or its signature rejects the implicit call.
                                    // Preserve those bindings so the deprecation is reported
                                    // alongside the unsupported-operator diagnostic.
                                    Err(
                                        CallDunderError::PossiblyUnbound { bindings, .. }
                                        | CallDunderError::CallError(_, bindings, _),
                                    ) => Some(bindings.as_ref()),
                                    // A completely missing method has no bindings to inspect.
                                    Err(CallDunderError::MethodNotAvailable) => None,
                                })
                                .flat_map(|bindings| bindings.deprecated_functions(db))
                                .map(|(_, function)| function),
                        );

                        let mut outcomes = outcomes.into_iter();
                        let result = Self::map_constrained_typevar_constraints(
                            db,
                            env,
                            operand_type,
                            constraints,
                            |_constraint| {
                                let outcome = outcomes.next()?.ok()?;
                                Some(outcome.return_type(db, env))
                            },
                        );
                        match result {
                            Some(ty) => ty,
                            None => {
                                // At least one constraint failed; report error.
                                self.report_unsupported_unary_operator(
                                    unary,
                                    op,
                                    operand_type,
                                    unary_dunder_method,
                                    None,
                                );
                                operand_type
                                    .try_call_dunder(
                                        db,
                                        env,
                                        unary_dunder_method,
                                        CallArguments::none(),
                                        TypeContext::default(),
                                    )
                                    .map_or_else(
                                        |e| e.fallback_return_type(db, env),
                                        |b| b.return_type(db, env),
                                    )
                            }
                        }
                    }
                    // For bounded TypeVars with union bounds (like `bound=float` which becomes
                    // `int | float`), we need to delegate to the bound type.
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        self.infer_unary_expression_type(op, bound, unary)
                    }
                    // For unconstrained TypeVars, fall through to default handling.
                    None => {
                        match operand_type.try_call_dunder(
                            db,
                            env,
                            unary_dunder_method,
                            CallArguments::none(),
                            TypeContext::default(),
                        ) {
                            Ok(outcome) => outcome.return_type(db, env),
                            Err(e) => {
                                self.report_unsupported_unary_operator(
                                    unary,
                                    op,
                                    operand_type,
                                    unary_dunder_method,
                                    Some(&e),
                                );
                                e.fallback_return_type(db, env)
                            }
                        }
                    }
                }
            }

            (
                ast::UnaryOp::UAdd | ast::UnaryOp::USub | ast::UnaryOp::Invert,
                Type::FunctionLiteral(_)
                | Type::Callable(..)
                | Type::WrapperDescriptor(_)
                | Type::KnownBoundMethod(_)
                | Type::DataclassDecorator(_)
                | Type::DataclassTransformer(_)
                | Type::BoundMethod(_)
                | Type::ModuleLiteral(_)
                | Type::ClassLiteral(_)
                | Type::GenericAlias(_)
                | Type::SubclassOf(_)
                | Type::NominalInstance(_)
                | Type::ProtocolInstance(_)
                | Type::SpecialForm(_)
                | Type::KnownInstance(_)
                | Type::PropertyInstance(_)
                | Type::SlotDescriptor(_)
                | Type::Union(_)
                | Type::Intersection(_)
                | Type::EnumComplement(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::BoundSuper(_)
                | Type::TypeIs(_)
                | Type::TypeGuard(_)
                | Type::TypeForm(_)
                | Type::TypedDict(_)
                | Type::NewTypeInstance(_),
            ) => fallback_unary_expression_type(),
        }
    }

    fn infer_boolean_expression(
        &mut self,
        bool_op: &ast::ExprBoolOp,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        match chained_comparison::infer_chain_sync(
            self,
            chained_comparison::ChainInput::Boolean {
                expression: bool_op,
                context: tcx,
            },
            chained_comparison::ChainFacts,
            &chained_comparison::OrdinaryChainedComparisonEffects,
        ) {
            Ok(ty) => ty,
            Err(never) => match never {},
        }
    }

    fn infer_compare_expression(&mut self, compare: &ast::ExprCompare) -> Type<'db> {
        match chained_comparison::infer_chain_sync(
            self,
            chained_comparison::ChainInput::Comparison(compare),
            chained_comparison::ChainFacts,
            &chained_comparison::OrdinaryChainedComparisonEffects,
        ) {
            Ok(ty) => ty,
            Err(never) => match never {},
        }
    }

    /// Infers explicit type-parameter declarations and merges their canonical results and deferred owners.
    fn infer_type_parameters(&mut self, type_parameters: &ast::TypeParams) {
        let Ok(()) = function::annotations::type_parameters_sync(
            self,
            type_parameters,
            function::annotations::AnnotationFacts,
            &function::annotations::OrdinaryAnnotationEffects,
        );
    }

    pub(super) fn finish_expression(mut self) -> ExpressionInference<'db> {
        self.infer_region();
        self.into_expression_inference()
    }

    /// Consume the results already collected by this builder without inferring its region.
    fn into_expression_inference(self) -> ExpressionInference<'db> {
        let region = self.region;
        self.into_expression_cache_entry()
            .into_expression_inference(region)
    }

    /// Consume the results already collected by this builder without compacting them.
    fn into_expression_cache_entry(self) -> FullExpressionCacheEntry<'db> {
        let Self {
            implicit_aliases,
            context,
            expressions,
            comparison_truthiness,
            qualifiers: _,
            #[cfg(any(test, feature = "experimental-analysis"))]
                source_truthiness_backing: _,
            type_expression_flags,
            collection_use_constraints,
            string_annotations,
            expected_types,
            scope,
            bindings,
            declarations,
            deferred,
            cycle_recovery,
            dataclass_field_specifiers: _,

            // Ignored; only relevant to definition regions
            undecorated_type: _,
            deferred_decorator_calls: _,
            discards_dict_key_assignments: _,

            // builder only state
            expression_cache: _,
            reachability_cache: _,
            typevar_binding_context: _,
            deferred_state: _,
            called_functions,
            index: _,
            region: _,
            return_types_and_ranges: _,
        } = self;

        let diagnostics = context.finish_uncompacted();
        let _ = scope;

        assert!(
            declarations.is_empty(),
            "Expression region can't have declarations"
        );
        assert!(
            deferred.is_empty(),
            "Expression region can't have deferred definitions"
        );

        FullExpressionCacheEntry {
            implicit_aliases,
            expressions,
            comparison_truthiness,
            type_expression_flags,
            collection_use_constraints,
            string_annotations,
            expected_types,
            bindings,
            diagnostics,
            called_functions,
            cycle_recovery,
            #[cfg(debug_assertions)]
            scope,
        }
    }

    pub(super) fn finish_statement(mut self) -> StatementInferenceInner<'db> {
        self.infer_region();

        let Self {
            implicit_aliases,
            context,
            expressions,
            comparison_truthiness,
            qualifiers,
            #[cfg(any(test, feature = "experimental-analysis"))]
                source_truthiness_backing: _,
            type_expression_flags,
            mut collection_use_constraints,
            string_annotations,
            expected_types,
            scope,
            bindings,
            declarations,
            deferred,
            cycle_recovery,
            called_functions,
            mut return_types_and_ranges,

            // Ignored; only relevant to definition regions
            undecorated_type: _,
            deferred_decorator_calls: _,
            discards_dict_key_assignments: _,

            // builder only state
            expression_cache: _,
            reachability_cache: _,
            dataclass_field_specifiers: _,
            typevar_binding_context: _,
            deferred_state: _,
            index: _,
            region: _,
        } = self;

        let _ = scope;
        let diagnostics = context.finish();

        let extra = (!implicit_aliases.is_empty()
            || !diagnostics.is_empty()
            || !comparison_truthiness.is_empty()
            || !string_annotations.is_empty()
            || cycle_recovery.is_some()
            || !expected_types.is_empty()
            || !deferred.is_empty()
            || !called_functions.is_empty()
            || !return_types_and_ranges.is_empty()
            || !qualifiers.is_empty()
            || !type_expression_flags.is_empty()
            || !collection_use_constraints.is_empty())
        .then(|| {
            collection_use_constraints.shrink_to_fit();
            return_types_and_ranges.shrink_to_fit();
            Box::new(StatementInferenceInnerExtra {
                implicit_aliases: implicit_aliases.into_iter().collect(),
                comparison_truthiness: FrozenMap::from(comparison_truthiness),
                string_annotations: FrozenSet::from(string_annotations),
                expected_types: FrozenMap::from(expected_types),
                called_functions: called_functions
                    .into_iter()
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
                return_types_and_ranges: return_types_and_ranges.into_boxed_slice(),
                type_expression_flags: FrozenMap::from(type_expression_flags),
                collection_use_constraints,
                cycle_recovery,
                deferred: deferred.into_boxed_slice(),
                diagnostics,
                qualifiers: FrozenMap::from(qualifiers),
            })
        });

        if bindings.len() > 20 {
            tracing::debug!(
                "Inferred statement region `{:?}` contains {} bindings. \
                Lookups by linear scan might be slow.",
                self.region,
                bindings.len(),
            );
        }

        if declarations.len() > 20 {
            tracing::debug!(
                "Inferred statement region `{:?}` contains {} declarations. \
                Lookups by linear scan might be slow.",
                self.region,
                declarations.len(),
            );
        }

        StatementInferenceInner {
            expressions: FrozenMap::from(expressions),
            #[cfg(debug_assertions)]
            scope,
            bindings: bindings.into_boxed_slice(),
            declarations: declarations.into_boxed_slice(),
            extra,
        }
    }

    pub(super) fn finish_function_decorator_inference(mut self) -> FunctionDecoratorInference<'db> {
        let classification = match self.region {
            InferenceRegion::FunctionDecorators(definition) => {
                self.infer_region_function_decorators(definition)
            }
            _ => {
                self.infer_region();
                FunctionDecoratorClassification::default()
            }
        };
        self.finish_inferred_function_decorators(classification)
    }

    fn finish_inferred_function_decorators(
        self,
        classification: FunctionDecoratorClassification,
    ) -> FunctionDecoratorInference<'db> {
        let Self {
            implicit_aliases,
            context,
            expressions,
            comparison_truthiness: _,
            bindings,
            #[cfg(any(test, feature = "experimental-analysis"))]
                source_truthiness_backing: _,
            called_functions,
            expression_cache: _,
            reachability_cache: _,
            declarations: _,
            deferred: _,
            scope: _,
            string_annotations: _,
            expected_types: _,
            return_types_and_ranges: _,
            collection_use_constraints: _,
            dataclass_field_specifiers: _,
            undecorated_type: _,
            deferred_decorator_calls: _,
            discards_dict_key_assignments: _,
            typevar_binding_context: _,
            deferred_state: _,
            index: _,
            region: _,
            cycle_recovery: _,
            qualifiers: _,
            type_expression_flags: _,
        } = self;
        let diagnostics = context.finish();

        FunctionDecoratorInference {
            implicit_aliases: implicit_aliases.into_iter().collect(),
            expression_types: FrozenMap::from(expressions),
            bindings: bindings.into_boxed_slice(),
            called_functions: called_functions
                .into_iter()
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            known_decorators: classification.known_decorators,
            has_unknown_decorators: classification.has_unknown_decorators,
            diagnostics,
        }
    }

    pub(super) fn finish_definition(
        mut self,
        definition: Definition<'db>,
    ) -> DefinitionInference<'db> {
        self.infer_region();
        self.finish_inferred_definition(definition)
    }

    fn finish_inferred_definition(self, definition: Definition<'db>) -> DefinitionInference<'db> {
        let Self {
            implicit_aliases,
            context,
            expressions,
            comparison_truthiness,
            qualifiers,
            #[cfg(any(test, feature = "experimental-analysis"))]
                source_truthiness_backing: _,
            type_expression_flags,
            mut collection_use_constraints,
            string_annotations,
            expected_types,
            scope,
            bindings,
            declarations,
            deferred,
            cycle_recovery,
            undecorated_type,
            deferred_decorator_calls,
            discards_dict_key_assignments,
            called_functions,

            // builder only state
            expression_cache: _,
            reachability_cache: _,
            dataclass_field_specifiers: _,
            typevar_binding_context: _,
            deferred_state: _,
            index: _,
            region: _,
            return_types_and_ranges: _,
        } = self;

        let _ = scope;
        let diagnostics = context.finish();

        let non_undecorated_extra_field_count = usize::from(!string_annotations.is_empty())
            + usize::from(!implicit_aliases.is_empty())
            + usize::from(!comparison_truthiness.is_empty())
            + usize::from(!expected_types.is_empty())
            + usize::from(!collection_use_constraints.is_empty())
            + usize::from(!called_functions.is_empty())
            + usize::from(!type_expression_flags.is_empty())
            + usize::from(cycle_recovery.is_some())
            + usize::from(!deferred.is_empty())
            + usize::from(!deferred_decorator_calls.is_empty())
            + usize::from(!diagnostics.is_empty())
            + usize::from(discards_dict_key_assignments)
            + usize::from(!qualifiers.is_empty());

        let extra = match (non_undecorated_extra_field_count, undecorated_type) {
            (0, None) => None,
            (1, None) if !qualifiers.is_empty() => Some(Box::new(
                DefinitionInferenceExtra::Qualifiers(FrozenMap::from(qualifiers)),
            )),
            (1, None) if !deferred.is_empty() => Some(Box::new(
                DefinitionInferenceExtra::Deferred(deferred.into_boxed_slice()),
            )),
            (1, None) if !diagnostics.is_empty() => Some(Box::new(
                DefinitionInferenceExtra::Diagnostics(Box::new(diagnostics)),
            )),
            (1, None) if !called_functions.is_empty() => {
                Some(Box::new(DefinitionInferenceExtra::CalledFunctions(
                    called_functions
                        .into_iter()
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                )))
            }
            (1, None) if !expected_types.is_empty() => Some(Box::new(
                DefinitionInferenceExtra::ExpectedTypes(FrozenMap::from(expected_types)),
            )),
            (1, None) if !string_annotations.is_empty() => Some(Box::new(
                DefinitionInferenceExtra::StringAnnotations(FrozenSet::from(string_annotations)),
            )),
            (1, None) if discards_dict_key_assignments => Some(Box::new(
                DefinitionInferenceExtra::DiscardsDictKeyAssignments,
            )),
            (0, Some(undecorated_type)) => Some(Box::new(DefinitionInferenceExtra::Undecorated(
                Box::new(undecorated_type),
            ))),
            (1, Some(undecorated_type)) if !deferred.is_empty() => {
                Some(Box::new(DefinitionInferenceExtra::DeferredAndUndecorated(
                    Box::new(DeferredAndUndecorated {
                        deferred: deferred.into_boxed_slice(),
                        undecorated_type,
                    }),
                )))
            }
            (_, undecorated_type) => {
                collection_use_constraints.shrink_to_fit();
                let extra = OtherDefinitionInferenceExtra {
                    implicit_aliases: implicit_aliases.into_iter().collect(),
                    comparison_truthiness: FrozenMap::from(comparison_truthiness),
                    string_annotations: FrozenSet::from(string_annotations),
                    expected_types: FrozenMap::from(expected_types),
                    collection_use_constraints,
                    called_functions: called_functions
                        .into_iter()
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                    type_expression_flags: FrozenMap::from(type_expression_flags),
                    cycle_recovery,
                    deferred: deferred.into_boxed_slice(),
                    diagnostics,
                    undecorated_type,
                    deferred_decorator_calls: deferred_decorator_calls.into_iter().collect(),
                    discards_dict_key_assignments,
                    qualifiers: FrozenMap::from(qualifiers),
                };
                Some(Box::new(DefinitionInferenceExtra::Other(Box::new(extra))))
            }
        };

        if bindings.len() > 20 {
            tracing::debug!(
                "Inferred definition region `{:?}` contains {} bindings. \
                Lookups by linear scan might be slow.",
                self.region,
                bindings.len(),
            );
        }

        if declarations.len() > 20 {
            tracing::debug!(
                "Inferred declaration region `{:?}` contains {} declarations. \
                Lookups by linear scan might be slow.",
                self.region,
                declarations.len(),
            );
        }

        DefinitionInference {
            expressions: FrozenMap::from(expressions),
            #[cfg(debug_assertions)]
            scope,
            types: DefinitionTypes::from_parts(
                definition,
                bindings.into_vec(),
                declarations.into_vec(),
            ),
            extra,
        }
    }

    pub(super) fn finish_scope(self) -> ScopeInference<'db> {
        scope::finish_scope(self)
    }

    const fn inference_flags(&self) -> InferenceFlags {
        self.context.inference_flags
    }

    /// Returns a fresh [`TypeInferenceBuilder`] for the current scope that can be used
    /// to speculatively infer expressions during multi-inference.
    ///
    /// The inference results can be merged into the current inference region using
    /// [`TypeInferenceBuilder::extend`].
    fn speculate(&self) -> Self {
        let db = self.db();
        let Self {
            region,
            index,
            cycle_recovery,
            deferred_state,
            typevar_binding_context,
            ref expression_cache,
            ref reachability_cache,
            ref return_types_and_ranges,
            ref dataclass_field_specifiers,

            // These fields are type inference results, but do not affect the inference of a given
            // expression.
            implicit_aliases: _,
            context: _,
            collection_use_constraints: _,
            expressions: _,
            comparison_truthiness: _,
            #[cfg(any(test, feature = "experimental-analysis"))]
                source_truthiness_backing: _,
            string_annotations: _,
            expected_types: _,
            scope: _,
            bindings: _,
            declarations: _,
            deferred: _,
            called_functions: _,
            undecorated_type: _,
            deferred_decorator_calls: _,
            discards_dict_key_assignments: _,
            qualifiers: _,
            type_expression_flags: _,
        } = *self;

        let mut builder = TypeInferenceBuilder::new(
            db,
            self.program_environment(),
            region,
            self.file(),
            self.program_file(),
            index,
            self.module(),
        );

        // Speculated builders are often discarded immediately.
        builder.context.defuse();

        // Ensure the speculative builder has the same inference context as the current one.
        builder.cycle_recovery = cycle_recovery;
        builder.deferred_state = deferred_state;
        builder.typevar_binding_context = typevar_binding_context;
        builder.context.inference_flags = self.inference_flags();
        builder.expression_cache.clone_from(expression_cache);
        builder.reachability_cache.clone_from(reachability_cache);
        builder
            .return_types_and_ranges
            .clone_from(return_types_and_ranges);
        builder
            .dataclass_field_specifiers
            .clone_from(dataclass_field_specifiers);

        builder
    }

    /// Returns a speculative builder that does not construct diagnostics.
    ///
    /// Note that this method may lead to lost diagnostics if the expression cache
    /// is enabled, as future multi-inference attempts may reuse inference results
    /// in which diagnostics were suppressed.
    fn speculate_without_diagnostics(&self) -> Self {
        let mut builder = self.speculate();
        builder.context.suppress_diagnostics();
        builder
    }

    /// Extend the current region with the results of a speculative [`TypeInferenceBuilder`].
    fn extend(&mut self, other: Self) {
        let Self {
            implicit_aliases,
            context,
            expressions,
            comparison_truthiness,
            type_expression_flags,
            #[cfg(any(test, feature = "experimental-analysis"))]
                source_truthiness_backing: _,
            collection_use_constraints,
            string_annotations,
            expected_types,
            scope,
            bindings,
            declarations,
            deferred,
            cycle_recovery,
            dataclass_field_specifiers: _,

            // Ignored; only relevant to definition regions
            undecorated_type: _,
            deferred_decorator_calls: _,
            discards_dict_key_assignments: _,

            // builder only state
            expression_cache: _,
            reachability_cache: _,
            typevar_binding_context: _,
            deferred_state: _,
            called_functions,
            index: _,
            region: _,
            return_types_and_ranges: _,
            qualifiers: _,
        } = other;

        let diagnostics = context.finish();
        let _ = scope;

        assert!(
            declarations.is_empty(),
            "speculative `TypeInferenceBuilder` should only be used for expression inference"
        );
        assert!(
            deferred.is_empty(),
            "speculative `TypeInferenceBuilder` should only be used for expression inference"
        );

        self.extend_expression_types(expressions);
        self.comparison_truthiness.extend(comparison_truthiness);
        self.context.extend(&diagnostics);
        self.extend_cycle_recovery(cycle_recovery);
        self.string_annotations
            .extend(string_annotations.iter().copied());
        self.expected_types.extend(expected_types.iter());
        self.type_expression_flags
            .extend(type_expression_flags.iter());
        self.called_functions.extend(called_functions);
        self.implicit_aliases.extend(implicit_aliases);

        if !matches!(self.region, InferenceRegion::Scope(..)) {
            self.bindings
                .extend(bindings.iter().map(|(def, ty)| (*def, *ty)));
        }

        #[expect(
            clippy::iter_over_hash_type,
            reason = "constraints for distinct collection definitions are merged independently"
        )]
        for (collection_def, constraints) in &collection_use_constraints {
            self.collection_use_constraints
                .entry(*collection_def)
                .and_modify(|this| this.extend(constraints))
                .or_insert(constraints.clone());
        }
    }
}

/// An expression cache shared across builders during multi-inference.
///
/// This provides a cheap way of reusing inference results without the overhead
/// of Salsa standalone expressions.
#[derive(Default)]
struct ExpressionCache<'db> {
    entries: FxHashMap<ExpressionNodeKey, ExpressionCacheEntries<'db>>,
}

impl<'db> ExpressionCache<'db> {
    fn get(
        &self,
        expression: ExpressionNodeKey,
        tcx: TypeContext<'db>,
    ) -> Option<&ExpressionCacheEntry<'db>> {
        self.entries.get(&expression)?.get(tcx)
    }

    fn insert(
        &mut self,
        expression: ExpressionNodeKey,
        tcx: TypeContext<'db>,
        value: ExpressionCacheEntry<'db>,
    ) {
        match self.entries.entry(expression) {
            hash_map::Entry::Occupied(mut entry) => {
                entry.get_mut().insert(tcx, value);
            }
            hash_map::Entry::Vacant(entry) => {
                entry.insert(ExpressionCacheEntries::Single(tcx, value));
            }
        }
    }
}

/// The inferred types of a given expression, keyed by type context.
enum ExpressionCacheEntries<'db> {
    Single(TypeContext<'db>, ExpressionCacheEntry<'db>),
    Many(FxHashMap<TypeContext<'db>, ExpressionCacheEntry<'db>>),
}

impl<'db> ExpressionCacheEntries<'db> {
    fn get(&self, tcx: TypeContext<'db>) -> Option<&ExpressionCacheEntry<'db>> {
        match self {
            Self::Single(cached_tcx, value) if *cached_tcx == tcx => Some(value),
            Self::Single(_, _) => None,
            Self::Many(values) => values.get(&tcx),
        }
    }

    fn insert(&mut self, tcx: TypeContext<'db>, value: ExpressionCacheEntry<'db>) {
        if let Self::Single(cached_tcx, cached_value) = self
            && *cached_tcx == tcx
        {
            *cached_value = value;
            return;
        }

        let previous = std::mem::replace(self, Self::Many(FxHashMap::default()));
        *self = match previous {
            Self::Single(cached_tcx, cached_value) => Self::Many(FxHashMap::from_iter([
                (cached_tcx, cached_value),
                (tcx, value),
            ])),
            Self::Many(mut values) => {
                values.insert(tcx, value);
                Self::Many(values)
            }
        };
    }
}

/// The inferred types for an expression region under a given type context.
#[derive(Clone)]
enum ExpressionCacheEntry<'db> {
    Small(Type<'db>),
    Full(Rc<FullExpressionCacheEntry<'db>>),
}

/// The full inference results for an expression region.
///
/// Unlike [`ExpressionInference`], this type is short-lived, and avoids the cost of compaction
/// that is otherwise performed for Salsa results.
struct FullExpressionCacheEntry<'db> {
    implicit_aliases: FxIndexSet<Definition<'db>>,
    expressions: FxHashMap<ExpressionNodeKey, Type<'db>>,
    comparison_truthiness: FxHashMap<ExpressionNodeKey, Truthiness>,
    type_expression_flags: FxHashMap<ExpressionNodeKey, TypeExpressionFlags>,
    collection_use_constraints: CollectionUseConstraints<'db>,
    string_annotations: FxHashSet<ExpressionNodeKey>,
    expected_types: FxHashMap<ExpressionNodeKey, Type<'db>>,
    bindings: VecMap<Definition<'db>, Type<'db>>,
    diagnostics: TypeCheckDiagnostics,
    called_functions: FxIndexSet<FunctionType<'db>>,
    cycle_recovery: Option<Type<'db>>,
    #[cfg(debug_assertions)]
    scope: ScopeId<'db>,
}

impl<'db> FullExpressionCacheEntry<'db> {
    fn expression_type(&self, expression: ExpressionNodeKey) -> Type<'db> {
        self.expressions
            .get(&expression)
            .copied()
            .or(self.cycle_recovery)
            .unwrap_or_else(Type::unknown)
    }

    fn is_single_expression(&self, expression: ExpressionNodeKey, ty: Type<'db>) -> bool {
        self.implicit_aliases.is_empty()
            && self.expressions.len() == 1
            && self.expressions.get(&expression) == Some(&ty)
            && self.comparison_truthiness.is_empty()
            && self.type_expression_flags.is_empty()
            && self.collection_use_constraints.is_empty()
            && self.string_annotations.is_empty()
            && self.expected_types.is_empty()
            && self.bindings.is_empty()
            && self.diagnostics.is_empty()
            && self.called_functions.is_empty()
            && self.cycle_recovery.is_none()
    }

    fn into_expression_inference(
        mut self,
        region: InferenceRegion<'db>,
    ) -> ExpressionInference<'db> {
        let extra = (!self.implicit_aliases.is_empty()
            || !self.string_annotations.is_empty()
            || !self.comparison_truthiness.is_empty()
            || !self.type_expression_flags.is_empty()
            || !self.collection_use_constraints.is_empty()
            || !self.expected_types.is_empty()
            || self.cycle_recovery.is_some()
            || !self.bindings.is_empty()
            || !self.called_functions.is_empty()
            || !self.diagnostics.is_empty())
        .then(|| {
            if self.bindings.len() > 20 {
                tracing::debug!(
                    "Inferred expression region `{:?}` contains {} bindings. \
                    Lookups by linear scan might be slow.",
                    region,
                    self.bindings.len()
                );
            }

            self.collection_use_constraints.shrink_to_fit();
            self.diagnostics.shrink_to_fit();
            Box::new(ExpressionInferenceExtra {
                implicit_aliases: self.implicit_aliases.into_iter().collect(),
                string_annotations: FrozenSet::from(self.string_annotations),
                comparison_truthiness: FrozenMap::from(self.comparison_truthiness),
                expected_types: FrozenMap::from(self.expected_types),
                type_expression_flags: FrozenMap::from(self.type_expression_flags),
                bindings: self.bindings.into_boxed_slice(),
                diagnostics: self.diagnostics,
                called_functions: self.called_functions.into_iter().collect(),
                cycle_recovery: self.cycle_recovery,
                collection_use_constraints: self.collection_use_constraints,
            })
        });

        ExpressionInference {
            expressions: FrozenMap::from(self.expressions),
            extra,
            #[cfg(debug_assertions)]
            scope: self.scope,
        }
    }
}

/// Manages the inference of a given expression.
struct MultiInferenceGuard<'db, 'ast, 'infer> {
    infer_expr:
        &'infer mut dyn FnMut(&mut TypeInferenceBuilder<'db, 'ast>, TypeContext<'db>) -> Type<'db>,
    last_tcx: Option<TypeContext<'db>>,
    finalized: bool,
}

impl<'db, 'ast, 'infer> MultiInferenceGuard<'db, 'ast, 'infer> {
    /// Creates a [`MultiInferenceGuard`] for the given expression.
    fn new(
        infer_expr: &'infer mut dyn FnMut(
            &mut TypeInferenceBuilder<'db, 'ast>,
            TypeContext<'db>,
        ) -> Type<'db>,
    ) -> Self {
        Self {
            infer_expr,
            last_tcx: None,
            finalized: false,
        }
    }

    /// Infer the expression with diagnostics enabled.
    ///
    /// This method must be called exactly once in the lifetime of the [`MultiInferenceGuard`].
    fn infer_loud(
        &mut self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        debug_assert!(
            !self.finalized,
            "called `infer_loud` multiple times on a `MultiInferenceGuard`"
        );

        self.finalized = true;
        (self.infer_expr)(builder, tcx)
    }

    /// Infer the expression silently, with diagnostics disabled.
    ///
    /// This method may be called an unlimited number of times.
    fn infer_silent(
        &mut self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        self.last_tcx = Some(tcx);
        (self.infer_expr)(&mut builder.speculate_without_diagnostics(), tcx)
    }

    fn last_tcx(&self) -> TypeContext<'db> {
        self.last_tcx.unwrap_or_default()
    }
}

impl Drop for MultiInferenceGuard<'_, '_, '_> {
    fn drop(&mut self) {
        debug_assert!(
            self.finalized,
            "dropped `MultiInferenceGuard` without calling `infer_loud`"
        );
    }
}

/// An expression representing the function argument at the given index, along with its type
/// context.
type ArgExpr<'db, 'ast> = (usize, &'ast ast::Expr, TypeContext<'db>);

#[derive(Clone, Copy)]
enum CallArgumentInferenceMode {
    /// Infer against every candidate type context entirely speculatively.
    Speculate,

    /// Commit a default inference without type context, if there are multiple
    /// applicable type contexts.
    Commit,
}

impl CallArgumentInferenceMode {
    fn requires_default_inference(self) -> bool {
        matches!(self, Self::Commit)
    }
}

/// The set of type contexts to use when inferring a call-site argument, across all matching overloads.
#[derive(Debug, PartialEq, Eq)]
enum MatchingArgumentTypeContext<'db> {
    Unique(Option<ArgumentTypeContext<'db>>),
    Many(Vec<Option<ArgumentTypeContext<'db>>>),
}

fn is_collection_literal(expression: &ast::Expr) -> bool {
    matches!(
        expression,
        ast::Expr::List(_) | ast::Expr::Set(_) | ast::Expr::Dict(_)
    )
}

/// Returns `true` if `tcx` cannot provide useful type context for a collection literal.
///
/// During generic call argument inference, type variables that cannot yet be specialized are
/// replaced by `UnspecializedTypeVar`. This marker intentionally carries neither type-variable
/// identity nor a concrete expected type, and collection literal inference ignores it rather than
/// using it as a constraint.
///
/// A bare generic parameter, such as the parameter to `reveal_type`, therefore provides an exact
/// `UnspecializedTypeVar` context that should not prevent a peer expression from providing context
/// instead.
///
/// This deliberately matches only the bare marker: a partially specialized context such as
/// `list[UnspecializedTypeVar | int]` still carries useful collection structure and concrete type
/// information.
fn is_empty_collection_type_context(tcx: TypeContext<'_>) -> bool {
    tcx.annotation
        .is_none_or(|annotation| annotation == Type::Dynamic(DynamicType::UnspecializedTypeVar))
}

fn prefer_collection_literal_peer_context<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    tcx: TypeContext<'db>,
) -> bool {
    is_empty_collection_type_context(tcx)
        // Peer type context should be preferred over `list[UnspecializedTypeVar]`, which
        // provides no useful element type.
        || tcx
            .annotation
            .and_then(|annotation| annotation.class_specialization(db, env))
            .is_some_and(|(_, specialization)| {
                specialization
                    .types(db)
                    .iter()
                    .all(|ty| *ty == Type::Dynamic(DynamicType::UnspecializedTypeVar))
            })
}

/// An iterator over arguments to a functional call.
#[derive(Clone)]
enum ArgumentsIter<'a> {
    FromAst(ArgumentsSourceOrder<'a>),
    Synthesized(std::slice::Iter<'a, ArgOrKeyword<'a>>),
}

impl<'a> ArgumentsIter<'a> {
    fn from_ast(arguments: &'a ast::Arguments) -> Self {
        Self::FromAst(arguments.iter_source_order())
    }

    fn synthesized(arguments: &'a [ArgOrKeyword<'a>]) -> Self {
        Self::Synthesized(arguments.iter())
    }
}

impl<'a> Iterator for ArgumentsIter<'a> {
    type Item = ArgOrKeyword<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            ArgumentsIter::FromAst(args) => args.next(),
            ArgumentsIter::Synthesized(args) => args.next().copied(),
        }
    }
}

/// The deferred state of a specific expression in an inference region.
#[derive(Default, Debug, Clone, Copy)]
pub(in crate::types::infer) enum DeferredExpressionState {
    /// The expression is not deferred.
    #[default]
    None,

    /// The expression is deferred.
    ///
    /// In the following example,
    /// ```py
    /// from __future__ import annotation
    ///
    /// a: tuple[int, "ForwardRef"] = ...
    /// ```
    ///
    /// The expression `tuple` and `int` are deferred but `ForwardRef` (after parsing) is both
    /// deferred and in a string annotation context.
    Deferred,

    /// The expression is in a string annotation context.
    ///
    /// This is required to differentiate between a deferred annotation and a string annotation.
    /// The former can occur when there's a `from __future__ import annotations` statement or we're
    /// in a stub file.
    ///
    /// In the following example,
    /// ```py
    /// a: "List[int]" = ...
    /// b: tuple[int, "ForwardRef"] = ...
    /// ```
    ///
    /// The annotation of `a` is completely inside a string while for `b`, it's only partially
    /// stringified.
    ///
    /// This variant wraps a [`NodeKey`] that allows us to retrieve the original
    /// [`ast::ExprStringLiteral`] node which created the string annotation.
    InStringAnnotation(NodeKey),
}

impl DeferredExpressionState {
    const fn is_deferred(self) -> bool {
        matches!(
            self,
            DeferredExpressionState::Deferred | DeferredExpressionState::InStringAnnotation(_)
        )
    }

    const fn in_string_annotation(self) -> bool {
        matches!(self, DeferredExpressionState::InStringAnnotation(_))
    }
}

impl From<bool> for DeferredExpressionState {
    fn from(value: bool) -> Self {
        if value {
            DeferredExpressionState::Deferred
        } else {
            DeferredExpressionState::None
        }
    }
}

/// Struct collecting string parts when inferring a formatted string. Infers a string literal if the
/// concatenated string is small enough, otherwise infers a literal string.
///
/// If the formatted string contains an expression (with a representation unknown at compile time),
/// infers an instance of `builtins.str`.
#[derive(Debug)]
struct StringPartsCollector {
    concatenated: Option<CompactString>,
    contains_non_literal_str: bool,
}

impl StringPartsCollector {
    fn new() -> Self {
        Self {
            concatenated: Some(CompactString::new("")),
            contains_non_literal_str: false,
        }
    }

    fn push_str(&mut self, literal: &str) {
        if let Some(mut concatenated) = self.concatenated.take() {
            if concatenated.len().saturating_add(literal.len())
                <= TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE
            {
                concatenated.push_str(literal);
                self.concatenated = Some(concatenated);
            } else {
                self.concatenated = None;
            }
        }
    }

    /// Add an expression whose `__str__` return type is `LiteralString`.
    /// The exact value is unknown, so we can't track the concatenated string,
    /// but the result is still `LiteralString`.
    fn add_literal_string_expression(&mut self) {
        self.concatenated = None;
    }

    /// Add an expression whose `__str__` return type is not `LiteralString`.
    /// The result will degrade to `str`.
    fn add_non_literal_string_expression(&mut self) {
        self.concatenated = None;
        self.contains_non_literal_str = true;
    }

    fn string_type<'db>(self, context: &InferContext<'db, '_>) -> Type<'db> {
        let db = context.db();
        if self.contains_non_literal_str {
            KnownClass::Str.to_instance(db, context.program_environment())
        } else if let Some(concatenated) = self.concatenated {
            Type::string_literal(db, &concatenated)
        } else {
            Type::LiteralValue(LiteralValueType::promotable(
                LiteralValueTypeKind::LiteralString,
            ))
        }
    }
}

/// Map based on a `Vec`. It doesn't enforce
/// uniqueness on insertion. Instead, it relies on the caller
/// that elements are unique. For example, the way we visit definitions
/// in the `TypeInference` builder already implicitly guarantees that each definition
/// is only visited once.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct VecMap<K, V>(Vec<(K, V)>);

impl<K, V> VecMap<K, V> {
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn iter(&self) -> VecMapIterator<'_, K, V> {
        VecMapIterator {
            inner: self.0.iter(),
        }
    }

    fn into_boxed_slice(self) -> Box<[(K, V)]> {
        self.0.into_boxed_slice()
    }

    fn into_vec(self) -> Vec<(K, V)> {
        self.0
    }
}

impl<K, V> VecMap<K, V>
where
    K: Eq,
    K: std::fmt::Debug,
    V: std::fmt::Debug,
{
    fn insert(&mut self, key: K, value: V) {
        debug_assert!(
            !self.0.iter().any(|(existing, _)| existing == &key),
            "An existing entry already exists for key {key:?}",
        );

        self.0.push((key, value));
    }

    #[inline]
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        if cfg!(debug_assertions) {
            for (key, value) in iter {
                self.insert(key, value);
            }
        } else {
            self.0.extend(iter);
        }
    }
}

impl<K, V> Default for VecMap<K, V> {
    fn default() -> Self {
        Self(Vec::default())
    }
}

impl<'a, K, V> IntoIterator for &'a VecMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = VecMapIterator<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

struct VecMapIterator<'a, K, V> {
    inner: std::slice::Iter<'a, (K, V)>,
}

impl<'a, K, V> Iterator for VecMapIterator<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, v)| (k, v))
    }
}

impl<K, V> std::iter::FusedIterator for VecMapIterator<'_, K, V> {}

impl<K, V> ExactSizeIterator for VecMapIterator<'_, K, V> {
    fn len(&self) -> usize {
        self.inner.len()
    }
}

/// Set based on a `Vec`. It doesn't enforce
/// uniqueness on insertion. Instead, it relies on the caller
/// that elements are unique. For example, the way we visit definitions
/// in the `TypeInference` builder make already implicitly guarantees that each definition
/// is only visited once.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct VecSet<V>(Vec<V>);

impl<V> VecSet<V> {
    #[inline]
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn into_boxed_slice(self) -> Box<[V]> {
        self.0.into_boxed_slice()
    }
}

impl<V> VecSet<V>
where
    V: Eq,
    V: std::fmt::Debug,
{
    fn insert(&mut self, value: V) {
        debug_assert!(
            !self.0.iter().any(|existing| existing == &value),
            "An existing entry already exists for {value:?}",
        );

        self.0.push(value);
    }

    #[inline]
    fn extend<T: IntoIterator<Item = V>>(&mut self, iter: T) {
        if cfg!(debug_assertions) {
            for value in iter {
                self.insert(value);
            }
        } else {
            self.0.extend(iter);
        }
    }
}

impl<V> Default for VecSet<V> {
    fn default() -> Self {
        Self(Vec::default())
    }
}

impl<V> IntoIterator for VecSet<V> {
    type Item = V;
    type IntoIter = std::vec::IntoIter<V>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[must_use]
pub(in crate::types::infer) struct AddBinding<'db, 'ast> {
    declared_ty: Option<Type<'db>>,
    declaration: Option<Definition<'db>>,
    binding: Definition<'db>,
    node: AnyNodeRef<'ast>,
    qualifiers: TypeQualifiers,
    is_local: bool,
    /// Whether `Final` comes from an actual declaration, rather than an import.
    has_final_declaration: bool,
}

impl<'db, 'ast> AddBinding<'db, 'ast> {
    fn type_context(&self) -> TypeContext<'db> {
        TypeContext::new(self.declared_ty)
    }

    fn insert(
        self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        inferred_ty: Type<'db>,
    ) -> Type<'db> {
        crate::types::signatures::effects::legacy_inline(self.insert_with(
            builder,
            &source_binding::LegacySourceBindingEffects,
            inferred_ty,
        ))
    }

    /// Arbitrary `__getitem__`/`__setitem__` methods on a class do not
    /// necessarily guarantee that the passed-in value for `__setitem__` is stored and
    /// can be retrieved unmodified via `__getitem__`. Therefore, we currently only
    /// perform assignment-based narrowing on a few built-in classes (`list`, `dict`,
    /// `bytesarray`, `TypedDict`, and `collections` types) where we are confident that
    /// this kind of narrowing can be performed soundly. This is the same approach as
    /// pyright. TODO: Other standard library classes may also be considered safe. Also,
    /// subclasses of these safe classes that do not override `__getitem__/__setitem__`
    /// may be considered safe.
    fn is_safe_mutable_class(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> bool {
        const SAFE_MUTABLE_CLASSES: &[KnownClass] = &[
            KnownClass::List,
            KnownClass::Dict,
            KnownClass::Bytearray,
            KnownClass::DefaultDict,
            KnownClass::ChainMap,
            KnownClass::Counter,
            KnownClass::Deque,
            KnownClass::OrderedDict,
        ];

        SAFE_MUTABLE_CLASSES
            .iter()
            .map(|class| class.to_instance(db, env))
            .any(|safe_mutable_class| {
                ty.is_equivalent_to(db, env, safe_mutable_class)
                    || ty
                        .generic_origin(db, env)
                        .zip(safe_mutable_class.generic_origin(db, env))
                        .is_some_and(|(l, r)| l == r)
            })
    }
}

#[derive(Copy, Clone, Debug)]
pub(in crate::types::infer) enum BoundOrConstraintsNodes<'ast> {
    Bound(&'ast ast::Expr),
    Constraints(&'ast [ast::Expr]),
}
