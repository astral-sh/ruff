use std::borrow::Cow;

use itertools::{Either, Itertools};
use ruff_db::diagnostic::{Annotation, DiagnosticId, Severity};
use ruff_db::files::File;
use ruff_db::parsed::ParsedModuleRef;
use ruff_db::source::source_text;
use ruff_python_ast::name::Name;
use ruff_python_ast::{
    self as ast, AnyNodeRef, ArgOrKeyword, ArgumentsSourceOrder, ExprContext, HasNodeIndex,
    NodeIndex, PythonVersion,
};
use ruff_python_stdlib::builtins::version_builtin_was_added;
use ruff_python_stdlib::typing::as_pep_585_generic;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use ty_module_resolver::{KnownModule, ModuleName, resolve_module};

use super::deferred;
use super::{
    DefinitionInference, DefinitionInferenceExtra, ExpressionInference, ExpressionInferenceExtra,
    InferenceRegion, ScopeInference, ScopeInferenceExtra, infer_deferred_types,
    infer_definition_types, infer_expression_types, infer_same_file_expression_type,
    infer_unpack_types,
};
use crate::diagnostic::format_enumeration;
use crate::node_key::NodeKey;
use crate::place::{
    ConsideredDefinitions, DefinedPlace, Definedness, LookupError, Place, PlaceAndQualifiers,
    TypeOrigin, builtins_module_scope, builtins_symbol, class_body_implicit_symbol,
    explicit_global_symbol, global_symbol, loop_header_reachability,
    module_type_implicit_global_declaration, module_type_implicit_global_symbol, place,
    place_from_bindings, place_from_declarations, typing_extensions_symbol,
};
use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::ast_ids::{HasScopedUseId, ScopedUseId};
use crate::semantic_index::definition::{
    AnnotatedAssignmentDefinitionKind, AssignmentDefinitionKind, ComprehensionDefinitionKind,
    Definition, DefinitionKind, DefinitionNodeKey, DefinitionState, ExceptHandlerDefinitionKind,
    ForStmtDefinitionKind, LoopHeaderDefinitionKind, TargetKind, WithItemDefinitionKind,
};
use crate::semantic_index::expression::{Expression, ExpressionKind};
use crate::semantic_index::narrowing_constraints::ConstraintKey;
use crate::semantic_index::place::{PlaceExpr, PlaceExprRef};
use crate::semantic_index::scope::{
    FileScopeId, NodeWithScopeKind, NodeWithScopeRef, ScopeId, ScopeKind,
};
use crate::semantic_index::symbol::{ScopedSymbolId, Symbol};
use crate::semantic_index::{
    ApplicableConstraints, EnclosingSnapshotResult, SemanticIndex, place_table,
};
use crate::types::CallableTypes;
use crate::types::call::bind::MatchingOverloadIndex;
use crate::types::call::{Binding, Bindings, CallArguments, CallError, CallErrorKind};
use crate::types::callable::CallableTypeKind;
use crate::types::class::{
    ClassLiteral, CodeGeneratorKind, DynamicClassAnchor, DynamicClassLiteral,
    DynamicMetaclassConflict, MethodDecorator,
};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::context::InferContext;
use crate::types::diagnostic::{
    self, CALL_NON_CALLABLE, CONFLICTING_DECLARATIONS, CYCLIC_CLASS_DEFINITION,
    CYCLIC_TYPE_ALIAS_DEFINITION, DUPLICATE_BASE, INCONSISTENT_MRO, INEFFECTIVE_FINAL,
    INVALID_ARGUMENT_TYPE, INVALID_ASSIGNMENT, INVALID_ATTRIBUTE_ACCESS, INVALID_BASE,
    INVALID_DECLARATION, INVALID_ENUM_MEMBER_ANNOTATION, INVALID_LEGACY_TYPE_VARIABLE,
    INVALID_NEWTYPE, INVALID_PARAMSPEC, INVALID_TYPE_ALIAS_TYPE, INVALID_TYPE_FORM,
    INVALID_TYPE_GUARD_CALL, INVALID_TYPE_VARIABLE_BOUND, INVALID_TYPE_VARIABLE_CONSTRAINTS,
    IncompatibleBases, NO_MATCHING_OVERLOAD, POSSIBLY_MISSING_IMPLICIT_CALL,
    POSSIBLY_MISSING_SUBMODULE, SUBCLASS_OF_FINAL_CLASS, UNDEFINED_REVEAL, UNRESOLVED_ATTRIBUTE,
    UNRESOLVED_GLOBAL, UNRESOLVED_REFERENCE, UNSUPPORTED_DYNAMIC_BASE, UNSUPPORTED_OPERATOR,
    UNUSED_AWAITABLE, hint_if_stdlib_attribute_exists_on_other_versions,
    report_attempted_protocol_instantiation, report_bad_dunder_set_call,
    report_call_to_abstract_method, report_cannot_pop_required_field_on_typed_dict,
    report_conflicting_metaclass_from_bases, report_instance_layout_conflict,
    report_invalid_assignment, report_invalid_attribute_assignment,
    report_invalid_class_match_pattern, report_invalid_exception_caught,
    report_invalid_exception_cause, report_invalid_exception_raised,
    report_invalid_exception_tuple_caught, report_invalid_key_on_typed_dict,
    report_invalid_type_checking_constant,
    report_match_pattern_against_non_runtime_checkable_protocol,
    report_match_pattern_against_typed_dict, report_possibly_missing_attribute,
    report_possibly_unresolved_reference, report_unsupported_augmented_assignment,
    report_unsupported_comparison,
};
use crate::types::enums::{enum_ignored_names, is_enum_class_by_inheritance};
use crate::types::function::{FunctionType, KnownFunction};
use crate::types::generics::{InferableTypeVars, SpecializationBuilder, bind_typevar};
use crate::types::infer::builder::named_tuple::NamedTupleKind;
use crate::types::infer::builder::paramspec_validation::validate_paramspec_components;
use crate::types::infer::{nearest_enclosing_class, nearest_enclosing_function};
use crate::types::mro::DynamicMroErrorKind;
use crate::types::newtype::NewType;
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::subclass_of::SubclassOfInner;
use crate::types::tuple::{Tuple, TupleLength, TupleSpecBuilder, TupleType};
use crate::types::type_alias::{ManualPEP695TypeAliasType, PEP695TypeAliasType};
use crate::types::typed_dict::{validate_typed_dict_constructor, validate_typed_dict_dict_literal};
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarConstraints, TypeVarIdentity};
use crate::types::{
    CallDunderError, CallableBinding, CallableType, ClassType, DynamicType, EvaluationMode,
    InferenceFlags, InternedConstraintSet, InternedType, IntersectionBuilder, IntersectionType,
    KnownClass, KnownInstanceType, KnownUnion, LiteralValueTypeKind, MemberLookupPolicy,
    ParamSpecAttrKind, Parameter, ParameterForm, Parameters, Signature, SpecialFormType,
    SubclassOfType, Truthiness, Type, TypeAliasType, TypeAndQualifiers, TypeContext,
    TypeQualifiers, TypeVarBoundOrConstraints, TypeVarKind, TypeVarVariance, TypedDictType,
    UnionBuilder, UnionType, binding_type, definition_expression_type, infer_complete_scope_types,
    infer_scope_types, todo_type,
};
use crate::types::{ClassBase, add_inferred_python_version_hint_to_diagnostic};
use crate::unpack::UnpackPosition;
use crate::{AnalysisSettings, Db, FxIndexSet, Program};

mod annotation_expression;
mod binary_expressions;
mod class;
mod function;
mod imports;
mod named_tuple;
mod paramspec_validation;
mod subscript;
mod type_expression;
mod typevar;

use super::comparisons::{self, BinaryComparisonVisitor};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct TypeAndRange<'db> {
    ty: Type<'db>,
    range: TextRange,
}

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
/// type inference result..
///
/// There are a few different kinds of methods in the type inference builder, and the naming
/// distinctions are a bit subtle.
///
/// The `finish` methods call [`infer_region`](TypeInferenceBuilder::infer_region), which delegates
/// to one of [`infer_region_scope`](TypeInferenceBuilder::infer_region_scope),
/// [`infer_region_definition`](TypeInferenceBuilder::infer_region_definition), or
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

    /// Expressions that are string annotations
    string_annotations: FxHashSet<ExpressionNodeKey>,

    /// The scope this region is part of.
    scope: ScopeId<'db>,

    // bindings, declarations, and deferred can only exist in definition, or scope contexts.
    /// The types of every binding in this region.
    ///
    /// The list should only contain one entry per binding at most.
    bindings: VecMap<Definition<'db>, Type<'db>>,

    /// The types and type qualifiers of every declaration in this region.
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
    /// This is mainly used in [`deferred::overloaded_function::check_overloaded_function`] to
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

    /// Whether we are in a context that binds unbound typevars.
    typevar_binding_context: Option<Definition<'db>>,

    /// Type-inference is context-dependent, especially in type expressions.
    /// This field tracks various flags that control how type inference should behave in the current context.
    inference_flags: InferenceFlags,

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

    multi_inference_state: MultiInferenceState,

    /// If you cannot avoid the possibility of calling `infer(_type)_expression` multiple times for a given expression,
    /// set this to `Get` after the expression has been inferred for the first time.
    /// While this is `Get`, any expressions will be considered to have already been inferred.
    inner_expression_inference_state: InnerExpressionInferenceState,

    inferring_vararg_annotation: bool,

    /// For function definitions, the undecorated type of the function.
    undecorated_type: Option<Type<'db>>,

    /// The fallback type for missing expressions/bindings/declarations or recursive type inference.
    cycle_recovery: Option<Type<'db>>,

    /// `true` if all places in this expression are definitely bound
    all_definitely_bound: bool,

    /// A list of `dataclass_transform` field specifiers that are "active" (when inferring
    /// the right hand side of an annotated assignment in a class that is a dataclass).
    dataclass_field_specifiers: SmallVec<[Type<'db>; NUM_FIELD_SPECIFIERS_INLINE]>,
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
        region: InferenceRegion<'db>,
        index: &'db SemanticIndex<'db>,
        module: &'ast ParsedModuleRef,
    ) -> Self {
        let scope = region.scope(db);

        Self {
            context: InferContext::new(db, scope, module),
            index,
            region,
            scope,
            return_types_and_ranges: vec![],
            called_functions: FxIndexSet::default(),
            deferred_state: DeferredExpressionState::None,
            inferring_vararg_annotation: false,
            multi_inference_state: MultiInferenceState::Panic,
            inner_expression_inference_state: InnerExpressionInferenceState::Infer,
            expressions: FxHashMap::default(),
            string_annotations: FxHashSet::default(),
            bindings: VecMap::default(),
            declarations: VecMap::default(),
            typevar_binding_context: None,
            inference_flags: InferenceFlags::empty(),
            deferred: VecSet::default(),
            undecorated_type: None,
            cycle_recovery: None,
            all_definitely_bound: true,
            dataclass_field_specifiers: SmallVec::new(),
        }
    }

    fn fallback_type(&self) -> Option<Type<'db>> {
        self.cycle_recovery
    }

    fn extend_cycle_recovery(&mut self, other: Option<Type<'db>>) {
        if let Some(other) = other {
            match self.cycle_recovery {
                Some(existing) => {
                    self.cycle_recovery =
                        Some(UnionType::from_two_elements(self.db(), existing, other));
                }
                None => {
                    self.cycle_recovery = Some(other);
                }
            }
        }
    }

    fn extend_definition(&mut self, inference: &DefinitionInference<'db>) {
        #[cfg(debug_assertions)]
        assert_eq!(self.scope, inference.scope);

        self.expressions.extend(inference.expressions.iter());
        self.declarations
            .extend(inference.declarations(), self.multi_inference_state);

        if !matches!(self.region, InferenceRegion::Scope(..)) {
            self.bindings
                .extend(inference.bindings(), self.multi_inference_state);
        }

        if let Some(extra) = &inference.extra {
            self.called_functions
                .extend(extra.called_functions.iter().copied());
            self.extend_cycle_recovery(extra.cycle_recovery);
            self.context.extend(&extra.diagnostics);
            self.deferred
                .extend(extra.deferred.iter().copied(), self.multi_inference_state);
            self.string_annotations
                .extend(extra.string_annotations.iter().copied());
        }
    }

    fn extend_expression(&mut self, inference: &ExpressionInference<'db>) {
        #[cfg(debug_assertions)]
        assert_eq!(self.scope, inference.scope);

        self.extend_expression_unchecked(inference);
    }

    fn extend_expression_unchecked(&mut self, inference: &ExpressionInference<'db>) {
        self.expressions.extend(inference.expressions.iter());

        if let Some(extra) = &inference.extra {
            self.context.extend(&extra.diagnostics);
            self.extend_cycle_recovery(extra.cycle_recovery);
            self.string_annotations
                .extend(extra.string_annotations.iter().copied());

            if !matches!(self.region, InferenceRegion::Scope(..)) {
                self.bindings
                    .extend(extra.bindings.iter().copied(), self.multi_inference_state);
            }
        }
    }

    fn extend_scope(&mut self, inference: &ScopeInference<'db>) {
        self.expressions.extend(inference.expressions.iter());

        if let Some(extra) = &inference.extra {
            self.context.extend(&extra.diagnostics);
            self.extend_cycle_recovery(extra.cycle_recovery);
            self.string_annotations
                .extend(extra.string_annotations.iter().copied());
        }
    }

    fn file(&self) -> File {
        self.context.file()
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

    fn settings(&self) -> &AnalysisSettings {
        self.db().analysis_settings(self.file())
    }

    /// If the current scope is a class body scope of a dataclass-like class, populate
    /// `self.dataclass_field_specifiers` with the field specifiers from the class's
    /// `dataclass_params` or `dataclass_transform` parameters. This is needed so that
    /// calls to field-specifier functions are recognized during type inference of the
    /// right-hand side of annotated assignments.
    fn setup_dataclass_field_specifiers(&mut self) {
        fn field_specifiers<'db>(
            db: &'db dyn Db,
            index: &'db SemanticIndex<'db>,
            scope: ScopeId<'db>,
        ) -> Option<SmallVec<[Type<'db>; NUM_FIELD_SPECIFIERS_INLINE]>> {
            let enclosing_scope = index.scope(scope.file_scope_id(db));
            let class_node = enclosing_scope.node().as_class()?;
            let class_definition = index.expect_single_definition(class_node);
            let class_literal = infer_definition_types(db, class_definition)
                .declaration_type(class_definition)
                .inner_type()
                .as_class_literal()?
                .as_static()?;

            class_literal
                .dataclass_params(db)
                .map(|params| SmallVec::from(params.field_specifiers(db)))
                .or_else(|| {
                    Some(SmallVec::from(
                        CodeGeneratorKind::from_class(db, class_literal.into(), None)?
                            .dataclass_transformer_params()?
                            .field_specifiers(db),
                    ))
                })
        }

        if let Some(specifiers) = field_specifiers(self.db(), self.index, self.scope()) {
            self.dataclass_field_specifiers = specifiers;
        }
    }

    /// Set the multi-inference state, returning the previous value.
    fn set_multi_inference_state(&mut self, state: MultiInferenceState) -> MultiInferenceState {
        std::mem::replace(&mut self.multi_inference_state, state)
    }

    /// Are we currently inferring types in file with deferred types?
    /// This is true for stub files, for files with `__future__.annotations`, and
    /// by default for all source files in Python 3.14 and later.
    fn defer_annotations(&self) -> bool {
        self.index.has_future_annotations()
            || self.in_stub()
            || Program::get(self.db()).python_version(self.db()) >= PythonVersion::PY314
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

    /// Check if a given AST node is reachable.
    ///
    /// Note that this only works if reachability is explicitly tracked for this specific
    /// type of node (see `node_reachability` in the use-def map).
    fn is_reachable<'a, N>(&self, node: N) -> bool
    where
        N: Into<AnyNodeRef<'a>>,
    {
        let file_scope_id = self.scope().file_scope_id(self.db());
        self.index.is_node_reachable(
            self.db(),
            file_scope_id,
            self.enclosing_node_key(node.into()),
        )
    }

    fn in_stub(&self) -> bool {
        self.context.in_stub()
    }

    /// Returns `true` if `expr` is a call to a known diagnostic function
    /// (e.g., `reveal_type` or `assert_type`) whose return value should not
    /// trigger the `unused-awaitable` lint.
    fn is_known_function_call(&self, expr: &ast::Expr) -> bool {
        let ast::Expr::Call(call) = expr else {
            return false;
        };
        matches!(
            self.expression_type(&call.func),
            Type::FunctionLiteral(f)
                if matches!(
                    f.known(self.db()),
                    Some(KnownFunction::RevealType | KnownFunction::AssertType)
                )
        )
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
        let expr_scope = file_scope.to_scope_id(self.db(), self.file());
        match self.region {
            InferenceRegion::Scope(scope, _) if scope == expr_scope => {
                self.expression_type(expression)
            }
            _ => infer_complete_scope_types(self.db(), expr_scope).expression_type(expression),
        }
    }

    /// Infers types in the given [`InferenceRegion`].
    fn infer_region(&mut self) {
        match self.region {
            InferenceRegion::Scope(scope, tcx) => self.infer_region_scope(scope, tcx),
            InferenceRegion::Definition(definition) => self.infer_region_definition(definition),
            InferenceRegion::Deferred(definition) => self.infer_region_deferred(definition),
            InferenceRegion::Expression(expression, tcx) => {
                self.infer_region_expression(expression, tcx);
            }
        }
    }

    fn infer_region_scope(&mut self, scope: ScopeId<'db>, tcx: TypeContext<'db>) {
        let node = scope.node(self.db());
        match node {
            NodeWithScopeKind::Module => {
                self.infer_module(self.module().syntax());
            }
            NodeWithScopeKind::Function(function) => {
                self.infer_function_body(function.node(self.module()));
            }
            NodeWithScopeKind::Lambda(lambda) => self.infer_lambda_body(lambda.node(self.module())),
            NodeWithScopeKind::Class(class) => self.infer_class_body(class.node(self.module())),
            NodeWithScopeKind::ClassTypeParameters(class) => {
                self.infer_class_type_params(class.node(self.module()));
            }
            NodeWithScopeKind::FunctionTypeParameters(function) => {
                self.infer_function_type_params(function.node(self.module()));
            }
            NodeWithScopeKind::TypeAliasTypeParameters(type_alias) => {
                self.infer_type_alias_type_params(type_alias.node(self.module()));
            }
            NodeWithScopeKind::TypeAlias(type_alias) => {
                self.infer_type_alias(type_alias.node(self.module()));
            }
            NodeWithScopeKind::ListComprehension(comprehension) => {
                self.infer_list_comprehension_expression_scope(
                    comprehension.node(self.module()),
                    tcx,
                );
            }
            NodeWithScopeKind::SetComprehension(comprehension) => {
                self.infer_set_comprehension_expression_scope(
                    comprehension.node(self.module()),
                    tcx,
                );
            }
            NodeWithScopeKind::DictComprehension(comprehension) => {
                self.infer_dict_comprehension_expression_scope(
                    comprehension.node(self.module()),
                    tcx,
                );
            }
            NodeWithScopeKind::GeneratorExpression(generator) => {
                self.infer_generator_expression_scope(generator.node(self.module()));
            }
        }

        // Infer deferred types for all definitions.
        let deferred_definitions: Vec<_> = std::mem::take(&mut self.deferred).into_iter().collect();
        for definition in &deferred_definitions {
            self.extend_definition(infer_deferred_types(self.db(), *definition));
        }

        assert!(
            self.deferred.is_empty(),
            "Inferring deferred types should not add more deferred definitions"
        );

        if self.db().should_check_file(self.file()) {
            let mut seen_overloaded_places = FxHashSet::default();
            let mut seen_public_functions = FxHashSet::default();

            for (definition, ty_and_quals) in &self.declarations {
                let ty = ty_and_quals.inner_type();
                match definition.kind(self.db()) {
                    DefinitionKind::Function(function) => {
                        deferred::function::check_function_definition(
                            &self.context,
                            *definition,
                            &|expr| self.file_expression_type(expr),
                        );
                        deferred::overloaded_function::check_overloaded_function(
                            &self.context,
                            ty,
                            *definition,
                            self.scope.scope(self.db()).node(),
                            self.index,
                            &mut seen_overloaded_places,
                            &mut seen_public_functions,
                        );
                        deferred::typeguard::check_type_guard_definition(
                            &self.context,
                            ty,
                            function.node(self.module()),
                            self.index,
                        );
                    }
                    DefinitionKind::Class(class_node) => {
                        deferred::static_class::check_static_class_definitions(
                            &self.context,
                            ty,
                            class_node.node(self.module()),
                            self.index,
                            &|expr| self.file_expression_type(expr),
                        );
                    }
                    _ => {}
                }
            }

            for definition in &deferred_definitions {
                deferred::dynamic_class::check_dynamic_class_definition(&self.context, *definition);
            }

            for function in &self.called_functions {
                deferred::overloaded_function::check_overloaded_function(
                    &self.context,
                    Type::FunctionLiteral(*function),
                    function.definition(self.db()),
                    self.scope.scope(self.db()).node(),
                    self.index,
                    &mut seen_overloaded_places,
                    &mut seen_public_functions,
                );
            }

            deferred::final_variable::check_final_without_value(&self.context, self.index);
        }
    }

    fn infer_region_definition(&mut self, definition: Definition<'db>) {
        match definition.kind(self.db()) {
            DefinitionKind::Function(function) => {
                self.infer_function_definition(function.node(self.module()), definition);
            }
            DefinitionKind::Class(class) => {
                self.infer_class_definition(class.node(self.module()), definition);
            }
            DefinitionKind::TypeAlias(type_alias) => {
                self.infer_type_alias_definition(type_alias.node(self.module()), definition);
            }
            DefinitionKind::Import(import) => {
                self.infer_import_definition(
                    import.import(self.module()),
                    import.alias(self.module()),
                    definition,
                );
            }
            DefinitionKind::ImportFrom(import_from) => {
                self.infer_import_from_definition(
                    import_from.import(self.module()),
                    import_from.alias(self.module()),
                    definition,
                );
            }
            DefinitionKind::ImportFromSubmodule(import_from) => {
                self.infer_import_from_submodule_definition(
                    import_from.import(self.module()),
                    definition,
                );
            }
            DefinitionKind::StarImport(import) => {
                self.infer_import_from_definition(
                    import.import(self.module()),
                    import.alias(self.module()),
                    definition,
                );
            }
            DefinitionKind::Assignment(assignment) => {
                self.infer_assignment_definition(assignment, definition);
            }
            DefinitionKind::AnnotatedAssignment(annotated_assignment) => {
                self.infer_annotated_assignment_definition(annotated_assignment, definition);
            }
            DefinitionKind::AugmentedAssignment(augmented_assignment) => {
                self.infer_augment_assignment_definition(
                    augmented_assignment.node(self.module()),
                    definition,
                );
            }
            DefinitionKind::DictKeyAssignment(dict_key_assignment) => {
                self.infer_dict_key_assignment_definition(
                    dict_key_assignment.key(self.module()),
                    dict_key_assignment.value(self.module()),
                    dict_key_assignment.assignment,
                    definition,
                );
            }
            DefinitionKind::For(for_statement_definition) => {
                self.infer_for_statement_definition(for_statement_definition, definition);
            }
            DefinitionKind::NamedExpression(named_expression) => {
                self.infer_named_expression_definition(
                    named_expression.node(self.module()),
                    definition,
                );
            }
            DefinitionKind::Comprehension(comprehension) => {
                self.infer_comprehension_definition(comprehension, definition);
            }
            DefinitionKind::VariadicPositionalParameter(parameter) => {
                self.infer_variadic_positional_parameter_definition(
                    parameter.node(self.module()),
                    definition,
                );
            }
            DefinitionKind::VariadicKeywordParameter(parameter) => {
                self.infer_variadic_keyword_parameter_definition(
                    parameter.node(self.module()),
                    definition,
                );
            }
            DefinitionKind::Parameter(parameter_with_default) => {
                self.infer_parameter_definition(
                    parameter_with_default.node(self.module()),
                    definition,
                );
            }
            DefinitionKind::WithItem(with_item_definition) => {
                self.infer_with_item_definition(with_item_definition, definition);
            }
            DefinitionKind::MatchPattern(match_pattern) => {
                self.infer_match_pattern_definition(
                    match_pattern.pattern(self.module()),
                    match_pattern.index(),
                    definition,
                );
            }
            DefinitionKind::ExceptHandler(except_handler_definition) => {
                self.infer_except_handler_definition(except_handler_definition, definition);
            }
            DefinitionKind::TypeVar(node) => {
                self.infer_typevar_definition(node.node(self.module()), definition);
            }
            DefinitionKind::ParamSpec(node) => {
                self.infer_paramspec_definition(node.node(self.module()), definition);
            }
            DefinitionKind::TypeVarTuple(node) => {
                self.infer_typevartuple_definition(node.node(self.module()), definition);
            }
            DefinitionKind::LoopHeader(loop_header) => {
                self.infer_loop_header_definition(loop_header, definition);
            }
        }
    }

    fn infer_region_deferred(&mut self, definition: Definition<'db>) {
        // N.B. We don't defer the types for an annotated assignment here because it is done in
        // the same definition query. It utilizes the deferred expression state instead.
        //
        // This is because for partially stringified annotations like `a: tuple[int, "ForwardRef"]`,
        // we need to defer the types of non-stringified expressions like `tuple` and `int` in the
        // definition query while the stringified expression `"ForwardRef"` would need to deferred
        // to use end-of-scope semantics. This would require custom and possibly a complex
        // implementation to allow this "split" to happen.

        match definition.kind(self.db()) {
            DefinitionKind::Function(function) => {
                self.infer_function_deferred(definition, function.node(self.module()));
            }
            DefinitionKind::Class(class) => {
                self.infer_class_deferred(definition, class.node(self.module()));
            }
            DefinitionKind::TypeVar(typevar) => {
                self.infer_typevar_deferred(typevar.node(self.module()));
            }
            DefinitionKind::ParamSpec(paramspec) => {
                self.infer_paramspec_deferred(paramspec.node(self.module()));
            }
            DefinitionKind::Assignment(assignment) => {
                self.infer_assignment_deferred(
                    assignment.target(self.module()),
                    assignment.value(self.module()),
                );
            }
            _ => {}
        }
    }

    fn infer_region_expression(&mut self, expression: Expression<'db>, tcx: TypeContext<'db>) {
        self.setup_dataclass_field_specifiers();

        match expression.kind(self.db()) {
            ExpressionKind::Normal => {
                self.infer_expression_impl(expression.node_ref(self.db(), self.module()), tcx);
            }
            ExpressionKind::TypeExpression => {
                self.infer_type_expression(expression.node_ref(self.db(), self.module()));
            }
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
        let db = self.db();
        debug_assert!(
            binding
                .kind(db)
                .category(self.context.in_stub(), self.module())
                .is_binding()
        );

        let db = self.db();
        let file_scope_id = binding.file_scope(db);
        let place_table = self.index.place_table(file_scope_id);
        let use_def = self.index.use_def_map(file_scope_id);

        let global_use_def_map = self.index.use_def_map(FileScopeId::global());
        let place_id = binding.place(self.db());
        let place = place_table.place(place_id);

        let (declarations, is_local) = if let Some(symbol) = place.as_symbol() {
            let symbol_id = place_id.expect_symbol();
            let skip_non_global_scopes = self.skip_non_global_scopes(file_scope_id, symbol_id);

            if skip_non_global_scopes {
                match self
                    .index
                    .place_table(FileScopeId::global())
                    .symbol_id(symbol.name())
                {
                    Some(id) => (
                        global_use_def_map.end_of_scope_symbol_declarations(id),
                        false,
                    ),
                    // This variable shows up in `global` declarations but doesn't have an explicit
                    // binding in the global scope.
                    None => (use_def.declarations_at_binding(binding), true),
                }
            } else if self
                .index
                .symbol_is_nonlocal_in_scope(symbol_id, file_scope_id)
            {
                // If we run out of ancestor scopes without finding a definition, we'll fall back to
                // the local scope. This will also be a syntax error in `infer_nonlocal_statement` (no
                // binding for `nonlocal` found), but ignore that here.
                let mut declarations = use_def.declarations_at_binding(binding);
                let mut is_local = true;
                // Walk up parent scopes looking for the enclosing scope that has definition of this
                // name. `ancestor_scopes` includes the current scope, so skip that one.
                for (enclosing_scope_file_id, enclosing_scope) in
                    self.index.ancestor_scopes(file_scope_id).skip(1)
                {
                    // Ignore class scopes and the global scope.
                    if !enclosing_scope.kind().is_function_like() {
                        continue;
                    }
                    let enclosing_place_table = self.index.place_table(enclosing_scope_file_id);
                    let Some(enclosing_symbol_id) = enclosing_place_table.symbol_id(symbol.name())
                    else {
                        // This ancestor scope doesn't have a binding. Keep going.
                        continue;
                    };

                    let enclosing_symbol = enclosing_place_table.symbol(enclosing_symbol_id);
                    if enclosing_symbol.is_nonlocal() {
                        // The variable is `nonlocal` in this ancestor scope. Keep going.
                        continue;
                    }
                    if enclosing_symbol.is_global() {
                        // The variable is `global` in this ancestor scope. This breaks the `nonlocal`
                        // chain, and it's a syntax error in `infer_nonlocal_statement`. Ignore that
                        // here and just bail out of this loop.
                        break;
                    }
                    // We found the closest definition. Note that (as in `infer_place_load`) this does
                    // *not* need to be a binding. It could be just a declaration, e.g. `x: int`.
                    declarations = self
                        .index
                        .use_def_map(enclosing_scope_file_id)
                        .end_of_scope_symbol_declarations(enclosing_symbol_id);
                    is_local = false;
                    break;
                }
                (declarations, is_local)
            } else {
                (use_def.declarations_at_binding(binding), true)
            }
        } else {
            (use_def.declarations_at_binding(binding), true)
        };

        let (mut place_and_quals, conflicting) = place_from_declarations(self.db(), declarations)
            .into_place_and_conflicting_declarations();

        if let Some(conflicting) = conflicting {
            // TODO point out the conflicting declarations in the diagnostic?
            let place = place_table.place(binding.place(db));
            if let Some(builder) = self.context.report_lint(&CONFLICTING_DECLARATIONS, node) {
                builder.into_diagnostic(format_args!(
                    "Conflicting declared types for `{place}`: {}",
                    format_enumeration(conflicting.iter().map(|ty| ty.display(db)))
                ));
            }
        }

        // Fall back to implicit module globals for (possibly) unbound names
        if !place_and_quals.place.is_definitely_bound()
            && let PlaceExprRef::Symbol(symbol) = place
        {
            let symbol_id = place_id.expect_symbol();

            if self.skip_non_global_scopes(file_scope_id, symbol_id)
                || self.scope.file_scope_id(self.db()).is_global()
            {
                place_and_quals = place_and_quals.or_fall_back_to(self.db(), || {
                    module_type_implicit_global_declaration(self.db(), symbol.name())
                });
            }
        }

        let PlaceAndQualifiers {
            place: resolved_place,
            qualifiers,
        } = place_and_quals;

        // If the place is unbound and its an attribute or subscript place, fall back to normal
        // attribute/subscript inference on the root type.
        let declared_ty =
            if resolved_place.is_undefined() && !place_table.place(place_id).is_symbol() {
                if let AnyNodeRef::ExprAttribute(ast::ExprAttribute { value, attr, .. }) = node {
                    let value_type =
                        self.infer_maybe_standalone_expression(value, TypeContext::default());
                    if let Place::Defined(DefinedPlace {
                        ty,
                        definedness: Definedness::AlwaysDefined,
                        ..
                    }) = value_type.member(db, attr).place
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
                    let value_ty = self.infer_expression(value, TypeContext::default());
                    let slice_ty = self.infer_expression(slice, TypeContext::default());
                    Some(self.infer_subscript_expression_types(subscript, value_ty, slice_ty, *ctx))
                } else {
                    None
                }
            } else {
                None
            }
            .or_else(|| resolved_place.ignore_possibly_undefined());

        AddBinding {
            declared_ty,
            binding,
            node,
            qualifiers,
            is_local,
        }
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
        debug_assert!(
            declaration
                .kind(self.db())
                .category(self.context.in_stub(), self.module())
                .is_declaration()
        );
        let use_def = self.index.use_def_map(declaration.file_scope(self.db()));
        let prior_bindings = use_def.bindings_at_definition(declaration);
        // unbound_ty is Never because for this check we don't care about unbound
        let inferred_ty = place_from_bindings(self.db(), prior_bindings)
            .place
            .with_qualifiers(TypeQualifiers::empty())
            .or_fall_back_to(self.db(), || {
                // Fallback to bindings declared on `types.ModuleType` if it's a global symbol
                let scope = self.scope().file_scope_id(self.db());
                let place = self
                    .index
                    .place_table(scope)
                    .place(declaration.place(self.db()));

                if let PlaceExprRef::Symbol(symbol) = &place
                    && scope.is_global()
                {
                    module_type_implicit_global_symbol(self.db(), symbol.name())
                } else {
                    Place::Undefined.into()
                }
            })
            .place
            .ignore_possibly_undefined()
            .unwrap_or(Type::Never);
        let ty = if inferred_ty.is_assignable_to(self.db(), ty.inner_type()) {
            ty
        } else {
            if let Some(builder) = self.context.report_lint(&INVALID_DECLARATION, node) {
                builder.into_diagnostic(format_args!(
                    "Cannot declare type `{}` for inferred type `{}`",
                    ty.inner_type().display(self.db()),
                    inferred_ty.display(self.db())
                ));
            }
            TypeAndQualifiers::declared(Type::unknown())
        };
        self.declarations
            .insert(declaration, ty, self.multi_inference_state);
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

        let (declared_ty, inferred_ty) = match *declared_and_inferred_ty {
            DeclaredAndInferredType::AreTheSame(type_and_qualifiers) => {
                (type_and_qualifiers, type_and_qualifiers.inner_type())
            }
            DeclaredAndInferredType::MightBeDifferent {
                declared_ty,
                inferred_ty,
            } => {
                let file_scope_id = self.scope().file_scope_id(self.db());
                if file_scope_id.is_global() {
                    let place_table = self.index.place_table(file_scope_id);
                    let place = place_table.place(definition.place(self.db()));
                    if let Some(module_type_implicit_declaration) = place
                        .as_symbol()
                        .map(|symbol| module_type_implicit_global_symbol(self.db(), symbol.name()))
                        .and_then(|place| place.place.ignore_possibly_undefined())
                    {
                        let declared_type = declared_ty.inner_type();
                        if !declared_type
                            .is_assignable_to(self.db(), module_type_implicit_declaration)
                        {
                            if let Some(builder) =
                                self.context.report_lint(&INVALID_DECLARATION, node)
                            {
                                let mut diagnostic = builder.into_diagnostic(format_args!(
                                    "Cannot shadow implicit global attribute `{place}` with declaration of type `{}`",
                                    declared_type.display(self.db())
                                ));
                                diagnostic.info(format_args!("The global symbol `{}` must always have a type assignable to `{}`",
                                    place,
                                    module_type_implicit_declaration.display(self.db())
                                ));
                            }
                        }
                    }
                }
                if inferred_ty.is_assignable_to(self.db(), declared_ty.inner_type()) {
                    (declared_ty, inferred_ty)
                } else {
                    report_invalid_assignment(
                        &self.context,
                        node,
                        definition,
                        declared_ty.inner_type(),
                        inferred_ty,
                    );

                    // if the assignment is invalid, fall back to assuming the annotation is correct
                    (declared_ty, declared_ty.inner_type())
                }
            }
        };

        self.declarations
            .insert(definition, declared_ty, self.multi_inference_state);
        self.bindings
            .insert(definition, inferred_ty, self.multi_inference_state);
    }

    fn add_unknown_declaration_with_binding(
        &mut self,
        node: AnyNodeRef,
        definition: Definition<'db>,
    ) {
        self.add_declaration_with_binding(
            node,
            definition,
            &DeclaredAndInferredType::are_the_same_type(Type::unknown()),
        );
    }

    fn record_return_type(&mut self, ty: Type<'db>, range: TextRange) {
        self.return_types_and_ranges
            .push(TypeAndRange { ty, range });
    }

    fn infer_module(&mut self, module: &ast::ModModule) {
        self.infer_body(&module.body);
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
        let value_ty =
            self.infer_annotation_expression(&type_alias.value, DeferredExpressionState::None);

        // A type alias where a value type points to itself, i.e. the expanded type is `Divergent` is meaningless
        // (but a type alias that expands to something like `list[Divergent]` may be a valid recursive type alias)
        // and would lead to infinite recursion. Therefore, such type aliases should not be exposed.
        // ```python
        // type Itself = Itself  # error: "Cyclic definition of `Itself`"
        // type A = B  # error: "Cyclic definition of `A`"
        // type B = A  # error: "Cyclic definition of `B`"
        // type G[T] = G[T]  # error: "Cyclic definition of `G`"
        // type RecursiveList[T] = list[T | RecursiveList[T]]  # OK
        // type RecursiveList2[T] = list[RecursiveList2[T]]  # It's not possible to create an element of this, but it's not an error for now
        // type IntOr = int | IntOr  # It's redundant, but OK for now
        // type IntOrStr = int | StrOrInt  # It's redundant, but OK
        // type StrOrInt = str | IntOrStr  # It's redundant, but OK
        // ```
        let expanded = value_ty.inner_type().expand_eagerly(self.db());
        if expanded.is_divergent() {
            if let Some(builder) = self
                .context
                .report_lint(&CYCLIC_TYPE_ALIAS_DEFINITION, type_alias)
            {
                builder.into_diagnostic(format_args!(
                    "Cyclic definition of `{}`",
                    &type_alias.name.as_name_expr().unwrap().id,
                ));
            }
            // Replace with `Divergent`.
            self.expressions
                .insert(type_alias.value.as_ref().into(), expanded);
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
        binding_type(self.db(), class_definition).to_class_type(self.db())
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
                    Type::Dynamic(DynamicType::Divergent(_)) => true,
                    _ => false,
                }
            })
    }

    fn infer_body(&mut self, suite: &[ast::Stmt]) {
        for statement in suite {
            self.infer_statement(statement);
        }
    }

    fn infer_statement(&mut self, statement: &ast::Stmt) {
        match statement {
            ast::Stmt::FunctionDef(function) => self.infer_function_definition_statement(function),
            ast::Stmt::ClassDef(class) => self.infer_class_definition_statement(class),
            ast::Stmt::Expr(ast::StmtExpr {
                range: _,
                node_index: _,
                value,
            }) => {
                // If this is a call expression, we would have added a `ReturnsNever` constraint,
                // meaning this will be a standalone expression.
                let ty = self.infer_maybe_standalone_expression(value, TypeContext::default());

                if ty.is_awaitable(self.db()) && !self.is_known_function_call(value) {
                    if let Some(builder) =
                        self.context.report_lint(&UNUSED_AWAITABLE, value.as_ref())
                    {
                        builder.into_diagnostic(format_args!(
                            "Object of type `{}` is not awaited",
                            ty.display(self.db()),
                        ));
                    }
                }
            }
            ast::Stmt::If(if_statement) => self.infer_if_statement(if_statement),
            ast::Stmt::Try(try_statement) => self.infer_try_statement(try_statement),
            ast::Stmt::With(with_statement) => self.infer_with_statement(with_statement),
            ast::Stmt::Match(match_statement) => self.infer_match_statement(match_statement),
            ast::Stmt::Assign(assign) => self.infer_assignment_statement(assign),
            ast::Stmt::AnnAssign(assign) => self.infer_annotated_assignment_statement(assign),
            ast::Stmt::AugAssign(aug_assign) => {
                self.infer_augmented_assignment_statement(aug_assign);
            }
            ast::Stmt::TypeAlias(type_statement) => self.infer_type_alias_statement(type_statement),
            ast::Stmt::For(for_statement) => self.infer_for_statement(for_statement),
            ast::Stmt::While(while_statement) => self.infer_while_statement(while_statement),
            ast::Stmt::Import(import) => self.infer_import_statement(import),
            ast::Stmt::ImportFrom(import) => self.infer_import_from_statement(import),
            ast::Stmt::Assert(assert_statement) => self.infer_assert_statement(assert_statement),
            ast::Stmt::Raise(raise) => self.infer_raise_statement(raise),
            ast::Stmt::Return(ret) => self.infer_return_statement(ret),
            ast::Stmt::Delete(delete) => self.infer_delete_statement(delete),
            ast::Stmt::Nonlocal(nonlocal) => self.infer_nonlocal_statement(nonlocal),
            ast::Stmt::Global(global) => self.infer_global_statement(global),
            ast::Stmt::Break(_)
            | ast::Stmt::Continue(_)
            | ast::Stmt::Pass(_)
            | ast::Stmt::IpyEscapeCommand(_) => {
                // No-op
            }
        }
    }

    fn infer_definition(&mut self, node: impl Into<DefinitionNodeKey> + std::fmt::Debug + Copy) {
        let definition = self.index.expect_single_definition(node);
        let result = infer_definition_types(self.db(), definition);
        self.extend_definition(result);
    }

    fn infer_type_alias_definition(
        &mut self,
        type_alias: &ast::StmtTypeAlias,
        definition: Definition<'db>,
    ) {
        self.infer_expression(&type_alias.name, TypeContext::default());

        // Check that no type parameter with a default follows a TypeVarTuple
        // in the type alias's PEP 695 type parameter list.
        if let Some(type_params) = type_alias.type_params.as_deref() {
            deferred::type_param_validation::check_no_default_after_typevar_tuple_pep695(
                &self.context,
                type_params,
            );
        }

        let rhs_scope = self
            .index
            .node_scope(NodeWithScopeRef::TypeAlias(type_alias))
            .to_scope_id(self.db(), self.file());

        let type_alias_ty = Type::KnownInstance(KnownInstanceType::TypeAliasType(
            TypeAliasType::PEP695(PEP695TypeAliasType::new(
                self.db(),
                &type_alias.name.as_name_expr().unwrap().id,
                rhs_scope,
                None,
            )),
        ));

        self.add_declaration_with_binding(
            type_alias.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(type_alias_ty),
        );
    }

    fn infer_if_statement(&mut self, if_statement: &ast::StmtIf) {
        let ast::StmtIf {
            range: _,
            node_index: _,
            test,
            body,
            elif_else_clauses,
        } = if_statement;

        let test_ty = self.infer_standalone_expression(test, TypeContext::default());

        if let Err(err) = test_ty.try_bool(self.db()) {
            err.report_diagnostic(&self.context, &**test);
        }

        self.infer_body(body);

        for clause in elif_else_clauses {
            let ast::ElifElseClause {
                range: _,
                node_index: _,
                test,
                body,
            } = clause;

            if let Some(test) = &test {
                let test_ty = self.infer_standalone_expression(test, TypeContext::default());

                if let Err(err) = test_ty.try_bool(self.db()) {
                    err.report_diagnostic(&self.context, test);
                }
            }

            self.infer_body(body);
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
                        .enter(builder.db())
                });
            } else {
                // Call into the context expression inference to validate that it evaluates
                // to a valid context manager.
                let context_expression_ty =
                    self.infer_expression(&item.context_expr, TypeContext::default());
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
        let eval_mode = if is_async {
            EvaluationMode::Async
        } else {
            EvaluationMode::Sync
        };

        context_expression_type
            .try_enter_with_mode(self.db(), eval_mode)
            .unwrap_or_else(|err| {
                err.report_diagnostic(
                    &self.context,
                    context_expression_type,
                    context_expression.into(),
                );
                err.fallback_enter_type(self.db())
            })
    }

    fn infer_exception(&mut self, node: Option<&ast::Expr>, is_star: bool) -> Type<'db> {
        // If there is no handled exception, it's invalid syntax;
        // a diagnostic will have already been emitted
        let node_ty = node.map_or(Type::unknown(), |ty| {
            self.infer_expression(ty, TypeContext::default())
        });
        let type_base_exception = KnownClass::BaseException.to_subclass_of(self.db());

        // If it's an `except*` handler, this won't actually be the type of the bound symbol;
        // it will actually be the type of the generic parameters to `BaseExceptionGroup` or `ExceptionGroup`.
        let symbol_ty = if let Some(tuple_spec) = node_ty.tuple_instance_spec(self.db()) {
            let mut builder = UnionBuilder::new(self.db());
            let mut invalid_elements = vec![];

            for (index, element) in tuple_spec.all_elements().iter().enumerate() {
                builder = builder.add(
                    if element.is_assignable_to(self.db(), type_base_exception) {
                        element.to_instance(self.db()).expect(
                            "`Type::to_instance()` should always return `Some()` \
                                if called on a type assignable to `type[BaseException]`",
                        )
                    } else {
                        invalid_elements.push((index, element));
                        Type::unknown()
                    },
                );
            }

            if !invalid_elements.is_empty()
                && let Some(node) = node
            {
                if let ast::Expr::Tuple(tuple) = node
                    && !tuple.iter().any(ast::Expr::is_starred_expr)
                    && Some(tuple.len()) == tuple_spec.len().into_fixed_length()
                {
                    let invalid_elements = invalid_elements
                        .iter()
                        .map(|(index, ty)| (&tuple.elts[*index], **ty));

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
        } else if node_ty.is_assignable_to(self.db(), type_base_exception) {
            node_ty.to_instance(self.db()).expect(
                "`Type::to_instance()` should always return `Some()` \
                    if called on a type assignable to `type[BaseException]`",
            )
        } else if node_ty.is_assignable_to(
            self.db(),
            Type::homogeneous_tuple(self.db(), type_base_exception),
        ) {
            node_ty
                .tuple_instance_spec(self.db())
                .and_then(|spec| {
                    let specialization = spec
                        .homogeneous_element_type(self.db())
                        .to_instance(self.db());

                    debug_assert!(specialization.is_some_and(|specialization_type| {
                        specialization_type.is_assignable_to(
                            self.db(),
                            KnownClass::BaseException.to_instance(self.db()),
                        )
                    }));

                    specialization
                })
                .unwrap_or_else(|| KnownClass::BaseException.to_instance(self.db()))
        } else if node_ty.is_assignable_to(
            self.db(),
            UnionType::from_two_elements(
                self.db(),
                type_base_exception,
                Type::homogeneous_tuple(self.db(), type_base_exception),
            ),
        ) {
            KnownClass::BaseException.to_instance(self.db())
        } else {
            if let Some(node) = node {
                report_invalid_exception_caught(&self.context, node, node_ty);
            }
            Type::unknown()
        };

        if is_star {
            let class = if symbol_ty
                .is_subtype_of(self.db(), KnownClass::Exception.to_instance(self.db()))
            {
                KnownClass::ExceptionGroup
            } else {
                KnownClass::BaseExceptionGroup
            };
            class.to_specialized_instance(self.db(), &[symbol_ty])
        } else {
            symbol_ty
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
        loop_header_kind: &LoopHeaderDefinitionKind<'db>,
        definition: Definition<'db>,
    ) {
        let db = self.db();
        let place = loop_header_kind.place();
        let use_def = self
            .index
            .use_def_map(self.scope().file_scope_id(self.db()));
        let loop_header = loop_header_reachability(db, definition);

        let mut union = UnionBuilder::new(db).recursively_defined(RecursivelyDefined::Yes);

        for reachable_binding in &loop_header.reachable_bindings {
            let binding_ty = binding_type(db, reachable_binding.definition);
            let narrowed_ty = use_def
                .narrowing_evaluator(reachable_binding.narrowing_constraint)
                .narrow(db, binding_ty, place);

            union.add_in_place(narrowed_ty);
        }

        self.bindings
            .insert(definition, union.build(), self.multi_inference_state);
    }

    fn infer_match_statement(&mut self, match_statement: &ast::StmtMatch) {
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

                if let Err(err) = guard_ty.try_bool(self.db()) {
                    err.report_diagnostic(&self.context, guard);
                }
            }

            self.infer_body(body);
        }
    }

    fn infer_match_pattern_definition(
        &mut self,
        pattern: &'ast ast::Pattern,
        _index: u32,
        definition: Definition<'db>,
    ) {
        // TODO(dhruvmanila): The correct way to infer types here is to perform structural matching
        // against the subject expression type (which we can query via `infer_expression_types`)
        // and extract the type at the `index` position if the pattern matches. This will be
        // similar to the logic in `self.infer_assignment_definition`.
        self.add_binding(pattern.into(), definition)
            .insert(self, todo_type!("`match` pattern definition types"));
    }

    fn validate_class_pattern(&mut self, pattern: &ast::PatternMatchClass, cls_ty: Type<'db>) {
        if let Type::ClassLiteral(class) = cls_ty {
            if class.is_typed_dict(self.db()) {
                report_match_pattern_against_typed_dict(&self.context, &*pattern.cls, class);
            } else if let Some(protocol_class) = class.into_protocol_class(self.db())
                && !protocol_class.is_runtime_checkable(self.db())
            {
                report_match_pattern_against_non_runtime_checkable_protocol(
                    &self.context,
                    &*pattern.cls,
                    protocol_class,
                );
            }
        } else if !cls_ty.is_assignable_to(self.db(), KnownClass::Type.to_instance(self.db())) {
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
        // That said, when inferring the type of a standalone expression, we don't have access to
        // its parent or sibling nodes.  That means, for instance, that in a class pattern, where
        // we are currently using the class name as the standalone expression, we do not have
        // access to the class pattern's arguments in the standalone expression inference scope.
        // At the moment, we aren't trying to do anything with those arguments when creating a
        // narrowing constraint for the pattern.  But in the future, if we do, we will have to
        // either wrap those arguments in their own standalone expressions, or update Expression to
        // be able to wrap other AST node types besides just ast::Expr.
        //
        // This function is only called for the top-level pattern of a match arm, and is
        // responsible for inferring the standalone expression for each supported pattern type. It
        // then hands off to `infer_nested_match_pattern` for any subexpressions and subpatterns,
        // where we do NOT have any additional standalone expressions to infer through.
        //
        // TODO(dhruvmanila): Add a Salsa query for inferring pattern types and matching against
        // the subject expression: https://github.com/astral-sh/ruff/pull/13147#discussion_r1739424510
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
                    rest: _,
                } = match_mapping;
                for key in keys {
                    self.infer_expression(key, TypeContext::default());
                }
                for pattern in patterns {
                    self.infer_nested_match_pattern(pattern);
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
            }
            ast::Pattern::MatchOr(match_or) => {
                for pattern in &match_or.patterns {
                    self.infer_nested_match_pattern(pattern);
                }
            }
            ast::Pattern::MatchStar(_) | ast::Pattern::MatchSingleton(_) => {}
        }
    }

    fn infer_assignment_statement(&mut self, assignment: &ast::StmtAssign) {
        let ast::StmtAssign {
            range: _,
            node_index: _,
            targets,
            value,
        } = assignment;

        for target in targets {
            self.infer_target(target, value, &|builder, tcx| {
                builder.infer_standalone_expression(value, tcx)
            });
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

    /// Make sure that the attribute assignment `obj.attribute = value` is valid.
    ///
    /// `target` is the node for the left-hand side, `object_ty` is the type of `obj`, `attribute` is
    /// the name of the attribute being assigned, and `value_ty` is the type of the right-hand side of
    /// the assignment. If the assignment is invalid, emit diagnostics.
    fn validate_attribute_assignment(
        &mut self,
        target: &ast::ExprAttribute,
        object_ty: Type<'db>,
        attribute: &str,
        infer_value_ty: &mut dyn FnMut(&mut Self, TypeContext<'db>) -> Type<'db>,
        emit_diagnostics: bool,
    ) -> bool {
        let db = self.db();

        // This closure should only be called if `value_ty` was inferred with `attr_ty` as type context.
        let ensure_assignable_to =
            |builder: &Self, value_ty: Type<'db>, attr_ty: Type<'db>| -> bool {
                let assignable = value_ty.is_assignable_to(db, attr_ty);
                if !assignable && emit_diagnostics {
                    report_invalid_attribute_assignment(
                        &builder.context,
                        target.into(),
                        attr_ty,
                        value_ty,
                        attribute,
                    );
                }
                assignable
            };

        let emit_invalid_final = |builder: &Self| {
            if emit_diagnostics
                && let Some(builder) = builder.context.report_lint(&INVALID_ASSIGNMENT, target)
            {
                builder.into_diagnostic(format_args!(
                    "Cannot assign to final attribute `{attribute}` on type `{}`",
                    object_ty.display(db)
                ));
            }
        };

        // Return true (and emit a diagnostic) if this is an invalid assignment to a `Final` attribute.
        // Per PEP 591 and the typing conformance suite, Final instance attributes can be assigned
        // in __init__ methods. Multiple assignments within __init__ are allowed (matching mypy
        // and pyright behavior), as long as the attribute doesn't have a class-level value.
        let invalid_assignment_to_final = |builder: &Self, qualifiers: TypeQualifiers| -> bool {
            // Check if it's a Final attribute
            if !qualifiers.contains(TypeQualifiers::FINAL) {
                return false;
            }

            // Check if we're in an __init__ method (where Final attributes can be initialized).
            let is_in_init = builder
                .current_function_definition()
                .is_some_and(|func| func.name.id == "__init__");

            // Not in __init__ - always disallow
            if !is_in_init {
                emit_invalid_final(builder);
                return true;
            }

            // We're in __init__ - verify we're in a method of the class being mutated
            let Some(class_ty) = builder.class_context_of_current_method() else {
                // Not a method (standalone function named __init__)
                emit_invalid_final(builder);
                return true;
            };

            // Check that object_ty is an instance of the class we're in
            if !object_ty.is_subtype_of(builder.db(), Type::instance(builder.db(), class_ty)) {
                // Assigning to a different class's Final attribute
                emit_invalid_final(builder);
                return true;
            }

            // Check if class-level attribute already has a value
            if let Some((class_literal, _)) = class_ty.static_class_literal(db) {
                let class_scope_id = class_literal.body_scope(db).file_scope_id(db);
                let place_table = builder.index.place_table(class_scope_id);

                if let Some(symbol) = place_table.symbol_by_name(attribute)
                    && symbol.is_bound()
                {
                    if emit_diagnostics
                        && let Some(diag_builder) =
                            builder.context.report_lint(&INVALID_ASSIGNMENT, target)
                    {
                        diag_builder.into_diagnostic(format_args!(
                            "Cannot assign to final attribute `{attribute}` in `__init__` \
                            because it already has a value at class level"
                        ));
                    }

                    return true;
                }
            }

            // In __init__ and no class-level value - allow
            false
        };

        match object_ty {
            Type::Union(union) => {
                let mut infer_value_ty = MultiInferenceGuard::new(infer_value_ty);

                // Perform loud inference without type context, as there may be multiple
                // equally applicable type contexts for each union member.
                let value_ty = infer_value_ty.infer_loud(self, TypeContext::default());

                if union.elements(self.db()).iter().all(|elem| {
                    self.validate_attribute_assignment(
                        target,
                        *elem,
                        attribute,
                        &mut |builder, tcx| infer_value_ty.infer_silent(builder, tcx),
                        false,
                    )
                }) {
                    true
                } else {
                    // TODO: This is not a very helpful error message, as it does not include the underlying reason
                    // why the assignment is invalid. This would be a good use case for sub-diagnostics.
                    if emit_diagnostics
                        && let Some(builder) = self.context.report_lint(&INVALID_ASSIGNMENT, target)
                    {
                        builder.into_diagnostic(format_args!(
                            "Object of type `{}` is not assignable \
                                 to attribute `{attribute}` on type `{}`",
                            value_ty.display(self.db()),
                            object_ty.display(self.db()),
                        ));
                    }

                    false
                }
            }

            Type::Intersection(intersection) => {
                let mut infer_value_ty = MultiInferenceGuard::new(infer_value_ty);

                // TODO: Handle negative intersection elements
                if intersection.positive(db).iter().any(|elem| {
                    self.validate_attribute_assignment(
                        target,
                        *elem,
                        attribute,
                        &mut |builder, tcx| infer_value_ty.infer_silent(builder, tcx),
                        false,
                    )
                }) {
                    // Perform loud inference using the narrowed type context.
                    infer_value_ty.infer_loud(self, infer_value_ty.last_tcx());
                    true
                } else {
                    // Otherwise, perform loud inference without type context, as we failed to
                    // narrow to any given intersection element.
                    let value_ty = infer_value_ty.infer_loud(self, TypeContext::default());

                    if emit_diagnostics
                        && let Some(builder) = self.context.report_lint(&INVALID_ASSIGNMENT, target)
                    {
                        // TODO: same here, see above
                        builder.into_diagnostic(format_args!(
                            "Object of type `{}` is not assignable \
                                 to attribute `{attribute}` on type `{}`",
                            value_ty.display(self.db()),
                            object_ty.display(self.db()),
                        ));
                    }

                    false
                }
            }

            Type::TypeAlias(alias) => self.validate_attribute_assignment(
                target,
                alias.value_type(self.db()),
                attribute,
                infer_value_ty,
                emit_diagnostics,
            ),

            // Super instances do not allow attribute assignment
            Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Super) => {
                infer_value_ty(self, TypeContext::default());

                if emit_diagnostics
                    && let Some(builder) = self.context.report_lint(&INVALID_ASSIGNMENT, target)
                {
                    builder.into_diagnostic(format_args!(
                        "Cannot assign to attribute `{attribute}` on type `{}`",
                        object_ty.display(self.db()),
                    ));
                }

                false
            }
            Type::BoundSuper(_) => {
                infer_value_ty(self, TypeContext::default());

                if emit_diagnostics
                    && let Some(builder) = self.context.report_lint(&INVALID_ASSIGNMENT, target)
                {
                    builder.into_diagnostic(format_args!(
                        "Cannot assign to attribute `{attribute}` on type `{}`",
                        object_ty.display(self.db()),
                    ));
                }
                false
            }

            Type::Dynamic(..) | Type::Never => {
                infer_value_ty(self, TypeContext::default());
                true
            }

            Type::NominalInstance(..)
            | Type::ProtocolInstance(_)
            | Type::LiteralValue(..)
            | Type::SpecialForm(..)
            | Type::KnownInstance(..)
            | Type::PropertyInstance(..)
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
            | Type::TypedDict(_)
            | Type::NewTypeInstance(_) => {
                // We may infer the value type multiple times with distinct type context during
                // attribute resolution.
                let mut infer_value_ty = MultiInferenceGuard::new(infer_value_ty);

                // Perform loud inference without type context, as we may encounter multiple equally
                // applicable type contexts during attribute resolution.
                let value_ty = infer_value_ty.infer_loud(self, TypeContext::default());

                // Infer `__setattr__` once upfront. We use this result for:
                // 1. Checking if it returns `Never` (indicating an immutable class)
                // 2. As a fallback when no explicit attribute is found
                //
                // TODO: We could re-infer `value_ty` with type context here.
                let setattr_dunder_call_result = object_ty.try_call_dunder_with_policy(
                    db,
                    "__setattr__",
                    &mut CallArguments::positional([Type::string_literal(db, attribute), value_ty]),
                    TypeContext::default(),
                    MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                );

                // Check if `__setattr__` returns `Never` (indicating an immutable class).
                // If so, block all attribute assignments regardless of explicit attributes.
                let setattr_returns_never = match &setattr_dunder_call_result {
                    Ok(result) => result.return_type(db).is_never(),
                    Err(err) => err.return_type(db).is_some_and(|ty| ty.is_never()),
                };

                if setattr_returns_never {
                    if emit_diagnostics {
                        if let Some(builder) = self.context.report_lint(&INVALID_ASSIGNMENT, target)
                        {
                            let is_setattr_synthesized = match object_ty.class_member_with_policy(
                                db,
                                "__setattr__".into(),
                                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                            ) {
                                PlaceAndQualifiers {
                                    place: Place::Defined(DefinedPlace { ty: attr_ty, .. }),
                                    qualifiers: _,
                                } => attr_ty.is_callable_type(),
                                _ => false,
                            };

                            let member_exists =
                                !object_ty.member(db, attribute).place.is_undefined();

                            let msg = if !member_exists {
                                format!(
                                    "Cannot assign to unresolved attribute `{attribute}` on type `{}`",
                                    object_ty.display(db)
                                )
                            } else if is_setattr_synthesized {
                                format!(
                                    "Property `{attribute}` defined in `{}` is read-only",
                                    object_ty.display(db)
                                )
                            } else {
                                format!(
                                    "Cannot assign to attribute `{attribute}` on type `{}` \
                                     whose `__setattr__` method returns `Never`/`NoReturn`",
                                    object_ty.display(db)
                                )
                            };

                            builder.into_diagnostic(msg);
                        }
                    }
                    return false;
                }

                // Now check for explicit attributes (class member or instance member).
                // If an explicit attribute exists, validate against its type.
                // Only fall back to `__setattr__` when no explicit attribute is found.
                match object_ty.class_member(db, attribute.into()) {
                    meta_attr @ PlaceAndQualifiers { .. } if meta_attr.is_class_var() => {
                        if emit_diagnostics
                            && let Some(builder) =
                                self.context.report_lint(&INVALID_ATTRIBUTE_ACCESS, target)
                        {
                            builder.into_diagnostic(format_args!(
                                "Cannot assign to ClassVar `{attribute}` \
                                from an instance of type `{ty}`",
                                ty = object_ty.display(self.db()),
                            ));
                        }
                        false
                    }
                    PlaceAndQualifiers {
                        place:
                            Place::Defined(DefinedPlace {
                                ty: meta_attr_ty,
                                definedness: meta_attr_boundness,
                                ..
                            }),
                        qualifiers,
                    } => {
                        // Resolve `Self` type variables to the concrete instance type.
                        let meta_attr_ty = meta_attr_ty.bind_self_typevars(db, object_ty);

                        if invalid_assignment_to_final(self, qualifiers) {
                            return false;
                        }

                        let assignable_to_meta_attr = if let Place::Defined(DefinedPlace {
                            ty: meta_dunder_set,
                            ..
                        }) =
                            meta_attr_ty.class_member(db, "__set__".into()).place
                        {
                            // TODO: We could use the annotated parameter type of `__set__` as
                            // type context here.
                            let dunder_set_result = meta_dunder_set.try_call(
                                db,
                                &CallArguments::positional([meta_attr_ty, object_ty, value_ty]),
                            );

                            if emit_diagnostics
                                && let Err(dunder_set_failure) = dunder_set_result.as_ref()
                            {
                                report_bad_dunder_set_call(
                                    &self.context,
                                    dunder_set_failure,
                                    attribute,
                                    object_ty,
                                    target,
                                );
                            }

                            dunder_set_result.is_ok()
                        } else {
                            let value_ty = infer_value_ty
                                .infer_silent(self, TypeContext::new(Some(meta_attr_ty)));

                            ensure_assignable_to(self, value_ty, meta_attr_ty)
                        };

                        let assignable_to_instance_attribute = if meta_attr_boundness
                            == Definedness::PossiblyUndefined
                        {
                            let (assignable, boundness) = if let PlaceAndQualifiers {
                                place:
                                    Place::Defined(DefinedPlace {
                                        ty: instance_attr_ty,
                                        definedness: instance_attr_boundness,
                                        ..
                                    }),
                                qualifiers,
                            } =
                                object_ty.instance_member(db, attribute)
                            {
                                // Bind `Self` via MRO matching.
                                let instance_attr_ty =
                                    instance_attr_ty.bind_self_typevars(db, object_ty);
                                let value_ty = infer_value_ty
                                    .infer_silent(self, TypeContext::new(Some(instance_attr_ty)));
                                if invalid_assignment_to_final(self, qualifiers) {
                                    return false;
                                }

                                (
                                    ensure_assignable_to(self, value_ty, instance_attr_ty),
                                    instance_attr_boundness,
                                )
                            } else {
                                (true, Definedness::PossiblyUndefined)
                            };

                            if boundness == Definedness::PossiblyUndefined {
                                report_possibly_missing_attribute(
                                    &self.context,
                                    target,
                                    attribute,
                                    object_ty,
                                );
                            }

                            assignable
                        } else {
                            true
                        };

                        assignable_to_meta_attr && assignable_to_instance_attribute
                    }

                    PlaceAndQualifiers {
                        place: Place::Undefined,
                        ..
                    } => {
                        if let PlaceAndQualifiers {
                            place:
                                Place::Defined(DefinedPlace {
                                    ty: instance_attr_ty,
                                    definedness: instance_attr_boundness,
                                    ..
                                }),
                            qualifiers,
                        } = object_ty.instance_member(db, attribute)
                        {
                            // Bind `Self` via MRO matching.
                            let instance_attr_ty =
                                instance_attr_ty.bind_self_typevars(db, object_ty);
                            let value_ty = infer_value_ty
                                .infer_silent(self, TypeContext::new(Some(instance_attr_ty)));
                            if invalid_assignment_to_final(self, qualifiers) {
                                return false;
                            }

                            if instance_attr_boundness == Definedness::PossiblyUndefined {
                                report_possibly_missing_attribute(
                                    &self.context,
                                    target,
                                    attribute,
                                    object_ty,
                                );
                            }

                            ensure_assignable_to(self, value_ty, instance_attr_ty)
                        } else {
                            // No explicit attribute found. Use `__setattr__` (already inferred
                            // above) as a fallback for dynamic attribute assignment.
                            match setattr_dunder_call_result {
                                // If __setattr__ succeeded, allow the assignment.
                                Ok(_) | Err(CallDunderError::PossiblyUnbound(_)) => true,
                                Err(CallDunderError::CallError(..)) => {
                                    if emit_diagnostics
                                        && let Some(builder) =
                                            self.context.report_lint(&UNRESOLVED_ATTRIBUTE, target)
                                    {
                                        builder.into_diagnostic(format_args!(
                                            "Cannot assign object of type `{}` to attribute \
                                            `{attribute}` on type `{}` with \
                                            custom `__setattr__` method.",
                                            value_ty.display(db),
                                            object_ty.display(db)
                                        ));
                                    }
                                    false
                                }
                                Err(CallDunderError::MethodNotAvailable) => {
                                    if emit_diagnostics
                                        && let Some(builder) =
                                            self.context.report_lint(&UNRESOLVED_ATTRIBUTE, target)
                                    {
                                        builder.into_diagnostic(format_args!(
                                            "Unresolved attribute `{}` on type `{}`",
                                            attribute,
                                            object_ty.display(db)
                                        ));
                                    }
                                    false
                                }
                            }
                        }
                    }
                }
            }

            Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..) => {
                match object_ty.class_member(db, attribute.into()) {
                    PlaceAndQualifiers {
                        place:
                            Place::Defined(DefinedPlace {
                                ty: meta_attr_ty,
                                definedness: meta_attr_boundness,
                                ..
                            }),
                        qualifiers,
                    } => {
                        if invalid_assignment_to_final(self, qualifiers) {
                            infer_value_ty(self, TypeContext::default());
                            return false;
                        }

                        // We may infer the value type multiple times with distinct type context during
                        // attribute resolution.
                        let mut infer_value_ty = MultiInferenceGuard::new(infer_value_ty);

                        // Perform loud inference without type context, as we may encounter multiple equally
                        // applicable type contexts during attribute resolution.
                        let value_ty = infer_value_ty.infer_loud(self, TypeContext::default());

                        let assignable_to_meta_attr = if let Place::Defined(DefinedPlace {
                            ty: meta_dunder_set,
                            ..
                        }) =
                            meta_attr_ty.class_member(db, "__set__".into()).place
                        {
                            // TODO: We could use the annotated parameter type of `__set__` as
                            // type context here.
                            let dunder_set_result = meta_dunder_set.try_call(
                                db,
                                &CallArguments::positional([meta_attr_ty, object_ty, value_ty]),
                            );

                            if emit_diagnostics
                                && let Err(dunder_set_failure) = dunder_set_result.as_ref()
                            {
                                report_bad_dunder_set_call(
                                    &self.context,
                                    dunder_set_failure,
                                    attribute,
                                    object_ty,
                                    target,
                                );
                            }

                            dunder_set_result.is_ok()
                        } else {
                            let value_ty = infer_value_ty
                                .infer_silent(self, TypeContext::new(Some(meta_attr_ty)));
                            ensure_assignable_to(self, value_ty, meta_attr_ty)
                        };

                        let assignable_to_class_attr = if meta_attr_boundness
                            == Definedness::PossiblyUndefined
                        {
                            let (assignable, boundness) = if let Place::Defined(DefinedPlace {
                                ty: class_attr_ty,
                                definedness: class_attr_boundness,
                                ..
                            }) = object_ty
                                .find_name_in_mro(db, attribute)
                                .expect("called on Type::ClassLiteral or Type::SubclassOf")
                                .place
                            {
                                let value_ty = infer_value_ty
                                    .infer_silent(self, TypeContext::new(Some(class_attr_ty)));
                                (
                                    ensure_assignable_to(self, value_ty, class_attr_ty),
                                    class_attr_boundness,
                                )
                            } else {
                                (true, Definedness::PossiblyUndefined)
                            };

                            if boundness == Definedness::PossiblyUndefined {
                                report_possibly_missing_attribute(
                                    &self.context,
                                    target,
                                    attribute,
                                    object_ty,
                                );
                            }

                            assignable
                        } else {
                            true
                        };

                        assignable_to_meta_attr && assignable_to_class_attr
                    }
                    PlaceAndQualifiers {
                        place: Place::Undefined,
                        ..
                    } => {
                        if let PlaceAndQualifiers {
                            place:
                                Place::Defined(DefinedPlace {
                                    ty: class_attr_ty,
                                    definedness: class_attr_boundness,
                                    ..
                                }),
                            qualifiers,
                        } = object_ty
                            .find_name_in_mro(db, attribute)
                            .expect("called on Type::ClassLiteral or Type::SubclassOf")
                        {
                            let value_ty =
                                infer_value_ty(self, TypeContext::new(Some(class_attr_ty)));
                            if invalid_assignment_to_final(self, qualifiers) {
                                return false;
                            }

                            if class_attr_boundness == Definedness::PossiblyUndefined {
                                report_possibly_missing_attribute(
                                    &self.context,
                                    target,
                                    attribute,
                                    object_ty,
                                );
                            }

                            ensure_assignable_to(self, value_ty, class_attr_ty)
                        } else {
                            infer_value_ty(self, TypeContext::default());

                            let attribute_is_bound_on_instance =
                                object_ty.to_instance(self.db()).is_some_and(|instance| {
                                    !instance
                                        .instance_member(self.db(), attribute)
                                        .place
                                        .is_undefined()
                                });

                            // Attribute is declared or bound on instance. Forbid access from the class object
                            if emit_diagnostics {
                                if attribute_is_bound_on_instance {
                                    if let Some(builder) =
                                        self.context.report_lint(&INVALID_ATTRIBUTE_ACCESS, target)
                                    {
                                        builder.into_diagnostic(format_args!(
                                            "Cannot assign to instance attribute \
                                             `{attribute}` from the class object `{ty}`",
                                            ty = object_ty.display(self.db()),
                                        ));
                                    }
                                } else {
                                    if let Some(builder) =
                                        self.context.report_lint(&UNRESOLVED_ATTRIBUTE, target)
                                    {
                                        builder.into_diagnostic(format_args!(
                                            "Unresolved attribute `{}` on type `{}`.",
                                            attribute,
                                            object_ty.display(db)
                                        ));
                                    }
                                }
                            }

                            false
                        }
                    }
                }
            }

            Type::ModuleLiteral(module) => {
                let sym = if module
                    .module(db)
                    .known(db)
                    .is_some_and(KnownModule::is_builtins)
                {
                    builtins_symbol(db, attribute)
                } else {
                    module.static_member(db, attribute)
                };
                if let Place::Defined(DefinedPlace { ty: attr_ty, .. }) = sym.place {
                    let value_ty = infer_value_ty(self, TypeContext::new(Some(attr_ty)));

                    let assignable = value_ty.is_assignable_to(db, attr_ty);
                    if assignable {
                        true
                    } else {
                        if emit_diagnostics {
                            report_invalid_attribute_assignment(
                                &self.context,
                                target.into(),
                                attr_ty,
                                value_ty,
                                attribute,
                            );
                        }
                        false
                    }
                } else {
                    infer_value_ty(self, TypeContext::default());

                    if emit_diagnostics
                        && let Some(builder) =
                            self.context.report_lint(&UNRESOLVED_ATTRIBUTE, target)
                    {
                        builder.into_diagnostic(format_args!(
                            "Unresolved attribute `{}` on type `{}`.",
                            attribute,
                            object_ty.display(db)
                        ));
                    }

                    false
                }
            }
        }
    }

    #[expect(clippy::type_complexity)]
    fn infer_target_impl(
        &mut self,
        target: &ast::Expr,
        value: &ast::Expr,
        infer_assigned_ty: Option<&dyn Fn(&mut Self, TypeContext<'db>) -> Type<'db>>,
    ) {
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

                if let Some(tuple_spec) =
                    assigned_ty.and_then(|ty| ty.tuple_instance_spec(self.db()))
                {
                    let assigned_tys = tuple_spec.all_elements().to_vec();

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

                if let Some(infer_assigned_ty) = infer_assigned_ty {
                    let infer_assigned_ty = &mut |builder: &mut Self, tcx| {
                        let assigned_ty = infer_assigned_ty(builder, tcx);
                        builder.store_expression_type(target, assigned_ty);
                        assigned_ty
                    };

                    self.validate_attribute_assignment(
                        attr_expr,
                        object_ty,
                        attr.id(),
                        infer_assigned_ty,
                        true,
                    );
                }
            }
            ast::Expr::Subscript(subscript_expr) => {
                if let Some(infer_assigned_ty) = infer_assigned_ty {
                    let infer_assigned_ty = &mut |builder: &mut Self, tcx| {
                        let assigned_ty = infer_assigned_ty(builder, tcx);
                        builder.store_expression_type(target, assigned_ty);
                        assigned_ty
                    };

                    self.validate_subscript_assignment(subscript_expr, value, infer_assigned_ty);
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

    fn infer_assignment_definition(
        &mut self,
        assignment: &AssignmentDefinitionKind<'db>,
        definition: Definition<'db>,
    ) {
        let target = assignment.target(self.module());

        let add = self.add_binding(target.into(), definition);
        let target_ty =
            self.infer_assignment_definition_impl(assignment, definition, add.type_context());
        self.store_expression_type(target, target_ty);
        add.insert(self, target_ty);
    }

    fn infer_assignment_definition_impl(
        &mut self,
        assignment: &AssignmentDefinitionKind<'db>,
        definition: Definition<'db>,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let value = assignment.value(self.module());
        let target = assignment.target(self.module());

        let mut target_ty = match assignment.target_kind() {
            TargetKind::Sequence(unpack_position, unpack) => {
                let unpacked = infer_unpack_types(self.db(), unpack);
                // Only copy the diagnostics if this is the first assignment to avoid duplicating the
                // unpack assignments.
                if unpack_position == UnpackPosition::First {
                    self.context.extend(unpacked.diagnostics());
                }

                unpacked.expression_type(target)
            }
            TargetKind::Single => {
                // This could be an implicit type alias (OptionalList = list[T] | None). Use the definition
                // of `OptionalList` as the binding context while inferring the RHS (`list[T] | None`), in
                // order to bind `T` to `OptionalList`.
                let previous_typevar_binding_context =
                    self.typevar_binding_context.replace(definition);

                let value_ty = if let Some(standalone_expression) = self.index.try_expression(value)
                {
                    self.infer_standalone_expression_impl(value, standalone_expression, tcx)
                } else if let ast::Expr::Call(call_expr) = value {
                    // If the RHS is not a standalone expression, this is a simple assignment
                    // (single target, no unpackings). That means it's a valid syntactic form
                    // for a legacy TypeVar creation; check for that.
                    let callable_type = self.infer_maybe_standalone_expression(
                        call_expr.func.as_ref(),
                        TypeContext::default(),
                    );

                    let ty = if let Some(namedtuple_kind) =
                        NamedTupleKind::from_type(self.db(), callable_type)
                    {
                        self.infer_namedtuple_call_expression(
                            call_expr,
                            Some(definition),
                            namedtuple_kind,
                        )
                    } else {
                        match callable_type
                            .as_class_literal()
                            .and_then(|cls| cls.known(self.db()))
                        {
                            Some(
                                typevar_class @ (KnownClass::TypeVar
                                | KnownClass::ExtensionsTypeVar),
                            ) => self.infer_legacy_typevar(
                                target,
                                call_expr,
                                definition,
                                typevar_class,
                            ),
                            Some(
                                paramspec_class @ (KnownClass::ParamSpec
                                | KnownClass::ExtensionsParamSpec),
                            ) => self.infer_legacy_paramspec(
                                target,
                                call_expr,
                                definition,
                                paramspec_class,
                            ),
                            Some(KnownClass::NewType) => {
                                self.infer_newtype_expression(target, call_expr, definition)
                            }
                            Some(KnownClass::Type) => {
                                // Try to extract the dynamic class with definition.
                                // This returns `None` if it's not a three-arg call to `type()`,
                                // signalling that we must fall back to normal call inference.
                                self.infer_builtins_type_call(call_expr, Some(definition))
                            }
                            Some(KnownClass::TypeAliasType) => {
                                self.infer_typealiastype_call(target, call_expr, definition)
                            }
                            Some(_) | None => {
                                self.infer_call_expression_impl(call_expr, callable_type, tcx)
                            }
                        }
                    };

                    self.store_expression_type(value, ty);
                    ty
                } else {
                    self.infer_expression(value, tcx)
                };

                self.typevar_binding_context = previous_typevar_binding_context;

                // `TYPE_CHECKING` is a special variable that should only be assigned `False`
                // at runtime, but is always considered `True` in type checking.
                // See mdtest/known_constants.md#user-defined-type_checking for details.
                if target.as_name_expr().map(|name| name.id.as_str()) == Some("TYPE_CHECKING") {
                    if !matches!(
                        value.as_boolean_literal_expr(),
                        Some(ast::ExprBooleanLiteral { value: false, .. })
                    ) {
                        report_invalid_type_checking_constant(&self.context, target.into());
                    }
                    Type::bool_literal(true)
                } else if self.in_stub() && value.is_ellipsis_literal_expr() {
                    Type::unknown()
                } else {
                    value_ty
                }
            }
        };

        if let Some(special_form) = target.as_name_expr().and_then(|name| {
            SpecialFormType::try_from_file_and_name(self.db(), self.file(), &name.id)
        }) {
            target_ty = Type::SpecialForm(special_form);
        }

        target_ty
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
            return error(
                &self.context,
                format_args!(
                    "The name of a `NewType` (`{name}`) must match \
                    the name of the variable it is assigned to (`{target_name}`)"
                ),
                target,
            );
        }

        // Inference of `tp` must be deferred, to avoid cycles.
        self.deferred.insert(definition, self.multi_inference_state);

        Type::KnownInstance(KnownInstanceType::NewType(NewType::new(
            db,
            ast::name::Name::from(name),
            definition,
            None,
        )))
    }

    fn infer_assignment_deferred(&mut self, target: &ast::Expr, value: &'ast ast::Expr) {
        // Infer deferred bounds/constraints/defaults of a legacy TypeVar / ParamSpec / NewType.
        let ast::Expr::Call(ast::ExprCall {
            func, arguments, ..
        }) = value
        else {
            return;
        };
        let func_ty = self
            .try_expression_type(func)
            .unwrap_or_else(|| self.infer_expression(func, TypeContext::default()));
        if func_ty == Type::SpecialForm(SpecialFormType::NamedTuple) {
            // Only the `fields` argument is deferred for `NamedTuple`;
            // other arguments are inferred eagerly.
            self.infer_typing_namedtuple_fields(&arguments.args[1]);
            return;
        }
        let known_class = func_ty
            .as_class_literal()
            .and_then(|cls| cls.known(self.db()));
        match (known_class, self.region) {
            (Some(KnownClass::NewType), _) => {
                self.infer_newtype_assignment_deferred(arguments);
                return;
            }
            (Some(KnownClass::TypeAliasType), InferenceRegion::Deferred(definition)) => {
                self.infer_typealiastype_assignment_deferred(definition, arguments);
                return;
            }
            (Some(KnownClass::Type), InferenceRegion::Deferred(definition)) => {
                self.infer_builtins_type_deferred(definition, value);
                return;
            }
            _ => {}
        }
        let mut constraint_tys = Vec::new();
        for arg in arguments.args.iter().skip(1) {
            let constraint = self.infer_type_expression(arg);
            constraint_tys.push(constraint);

            if constraint.has_typevar_or_typevar_instance(self.db())
                && let Some(builder) = self
                    .context
                    .report_lint(&INVALID_TYPE_VARIABLE_CONSTRAINTS, arg)
            {
                builder.into_diagnostic("TypeVar constraint cannot be generic");
            }
        }
        let mut bound_or_constraints = if !constraint_tys.is_empty() {
            Some(TypeVarBoundOrConstraints::Constraints(
                TypeVarConstraints::new(self.db(), constraint_tys.into_boxed_slice()),
            ))
        } else {
            None
        };
        if let Some(bound) = arguments.find_keyword("bound") {
            let bound_type = self.infer_type_expression(&bound.value);
            bound_or_constraints = Some(TypeVarBoundOrConstraints::UpperBound(bound_type));

            if bound_type.has_typevar_or_typevar_instance(self.db())
                && let Some(builder) = self
                    .context
                    .report_lint(&INVALID_TYPE_VARIABLE_BOUND, bound)
            {
                builder.into_diagnostic("TypeVar upper bound cannot be generic");
            }
        }
        if let Some(default) = arguments.find_keyword("default") {
            if matches!(
                known_class,
                Some(KnownClass::ParamSpec | KnownClass::ExtensionsParamSpec)
            ) {
                // Pass `None` for the name: the outer-scope typevar check inside
                // `infer_paramspec_default` is only relevant for PEP 695 type parameter
                // scopes. Legacy ParamSpec definitions live at module/class-body scope,
                // so the check would be a no-op here. Out-of-scope defaults for legacy
                // typevars are instead validated by `check_legacy_typevar_defaults`
                // (for functions) and `report_invalid_typevar_default_reference`
                // (for classes).
                self.infer_paramspec_default(&default.value, None);
            } else {
                let default_ty = self.infer_type_expression(&default.value);
                let bound_or_constraints_node = arguments
                    .find_keyword("bound")
                    .map(|kw| BoundOrConstraintsNodes::Bound(&kw.value))
                    .or_else(|| {
                        if arguments.args.len() < 3 {
                            return None;
                        }
                        Some(BoundOrConstraintsNodes::Constraints(&arguments.args[1..]))
                    });
                self.validate_typevar_default(
                    target.as_name_expr().map(|name| &*name.id),
                    bound_or_constraints,
                    default_ty,
                    &default.value,
                    bound_or_constraints_node,
                );
            }
        }
    }

    // Infer the deferred base type of a NewType.
    fn infer_newtype_assignment_deferred(&mut self, arguments: &ast::Arguments) {
        let inferred = self.infer_type_expression(&arguments.args[1]);

        if inferred.has_typevar_or_typevar_instance(self.db()) {
            if let Some(builder) = self
                .context
                .report_lint(&INVALID_NEWTYPE, &arguments.args[1])
            {
                let mut diag = builder.into_diagnostic("invalid base for `typing.NewType`");
                diag.set_primary_message("A `NewType` base cannot be generic");
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
            diag.set_primary_message(format!("type `{}`", inferred.display(self.db())));
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
            return error(
                &self.context,
                format_args!(
                    "The name of a `TypeAliasType` (`{name}`) must match \
                    the name of the variable it is assigned to (`{target_name}`)"
                ),
                target,
            );
        }

        // Inference of the value argument must be deferred, to avoid cycles.
        self.deferred.insert(definition, self.multi_inference_state);

        Type::KnownInstance(KnownInstanceType::TypeAliasType(
            TypeAliasType::ManualPEP695(ManualPEP695TypeAliasType::new(
                db,
                ast::name::Name::new(name),
                definition,
            )),
        ))
    }

    /// Infer the deferred value type of a `TypeAliasType`.
    fn infer_typealiastype_assignment_deferred(
        &mut self,
        definition: Definition<'db>,
        arguments: &ast::Arguments,
    ) {
        // Match the binding context used by eager assignment inference so legacy type variables
        // in the alias value are bound to the alias definition.
        let previous_context = self.typevar_binding_context.replace(definition);

        self.infer_type_expression(&arguments.args[1]);
        // Infer keyword arguments (e.g. `type_params`) so their types are stored.
        for keyword in &arguments.keywords {
            self.infer_expression(&keyword.value, TypeContext::default());
        }

        self.typevar_binding_context = previous_context;
    }

    /// Deferred inference for assigned `type()` calls.
    ///
    /// Infers the bases argument that was skipped during initial inference to handle
    /// forward references and recursive definitions.
    fn infer_builtins_type_deferred(&mut self, definition: Definition<'db>, call_expr: &ast::Expr) {
        let db = self.db();

        let ast::Expr::Call(call) = call_expr else {
            return;
        };

        // Get the already-inferred class type from the initial pass.
        let inferred_type = definition_expression_type(db, definition, call_expr);
        let Type::ClassLiteral(ClassLiteral::Dynamic(dynamic_class)) = inferred_type else {
            return;
        };

        let [_name_arg, bases_arg, _namespace_arg] = &*call.arguments.args else {
            return;
        };

        // Set the typevar binding context to allow legacy typevar binding in expressions
        // like `Generic[T]`. This matches the context used during initial inference.
        let previous_context = self.typevar_binding_context.replace(definition);

        // Infer the bases argument (this was skipped during initial inference).
        let bases_type = self.infer_expression(bases_arg, TypeContext::default());

        // Restore the previous context.
        self.typevar_binding_context = previous_context;

        // Extract and validate bases.
        let Some(bases) = self.extract_explicit_bases(bases_arg, bases_type) else {
            return;
        };

        // Validate individual bases for special types that aren't allowed in dynamic classes.
        let name = dynamic_class.name(db);
        self.validate_dynamic_type_bases(bases_arg, &bases, name);
    }

    /// Infer a call to `builtins.type()`.
    ///
    /// `builtins.type` has two overloads: a single-argument overload (e.g. `type("foo")`,
    /// and a 3-argument `type(name, bases, dict)` overload. Both are handled here.
    /// The `definition` parameter should be `Some()` if this call to `builtins.type()`
    /// occurs on the right-hand side of an assignment statement that has a [`Definition`]
    /// associated with it in the semantic index.
    ///
    /// If it's unclear which overload we should pick, we return `type[Unknown]`,
    /// to avoid cascading errors later on.
    fn infer_builtins_type_call(
        &mut self,
        call_expr: &ast::ExprCall,
        definition: Option<Definition<'db>>,
    ) -> Type<'db> {
        let db = self.db();

        let ast::Arguments {
            args,
            keywords,
            range: _,
            node_index: _,
        } = &call_expr.arguments;

        for keyword in keywords {
            self.infer_expression(&keyword.value, TypeContext::default());
        }

        let [name_arg, bases_arg, namespace_arg] = match &**args {
            [single] => {
                let arg_type = self.infer_expression(single, TypeContext::default());

                return if keywords.is_empty() {
                    arg_type.dunder_class(db)
                } else {
                    if keywords.iter().any(|keyword| keyword.arg.is_some())
                        && let Some(builder) =
                            self.context.report_lint(&NO_MATCHING_OVERLOAD, call_expr)
                    {
                        let mut diagnostic = builder
                            .into_diagnostic("No overload of class `type` matches arguments");
                        diagnostic.help(format_args!(
                            "`builtins.type()` expects no keyword arguments",
                        ));
                    }
                    SubclassOfType::subclass_of_unknown()
                };
            }

            [first, second] if second.is_starred_expr() => {
                self.infer_expression(first, TypeContext::default());
                self.infer_expression(second, TypeContext::default());

                match &**keywords {
                    [single] if single.arg.is_none() => {
                        return SubclassOfType::subclass_of_unknown();
                    }
                    _ => {
                        if let Some(builder) =
                            self.context.report_lint(&NO_MATCHING_OVERLOAD, call_expr)
                        {
                            let mut diagnostic = builder
                                .into_diagnostic("No overload of class `type` matches arguments");
                            diagnostic.help(format_args!(
                                "`builtins.type()` expects no keyword arguments",
                            ));
                        }

                        return SubclassOfType::subclass_of_unknown();
                    }
                }
            }

            [name, bases, namespace] => [name, bases, namespace],

            _ => {
                for arg in args {
                    self.infer_expression(arg, TypeContext::default());
                }

                if let Some(builder) = self.context.report_lint(&NO_MATCHING_OVERLOAD, call_expr) {
                    let mut diagnostic =
                        builder.into_diagnostic("No overload of class `type` matches arguments");
                    diagnostic.help(format_args!(
                        "`builtins.type()` can either be called with one or three \
                        positional arguments (got {})",
                        args.len()
                    ));
                }

                return SubclassOfType::subclass_of_unknown();
            }
        };

        let name_type = self.infer_expression(name_arg, TypeContext::default());

        let namespace_type = self.infer_expression(namespace_arg, TypeContext::default());

        // TODO: validate other keywords against `__init_subclass__` methods of superclasses
        if keywords
            .iter()
            .filter_map(|keyword| keyword.arg.as_deref())
            .contains("metaclass")
        {
            if let Some(builder) = self.context.report_lint(&NO_MATCHING_OVERLOAD, call_expr) {
                let mut diagnostic =
                    builder.into_diagnostic("No overload of class `type` matches arguments");
                diagnostic
                    .help("The `metaclass` keyword argument is not supported in `type()` calls");
            }
        }

        // If any argument is a starred expression, we can't know how many positional arguments
        // we're receiving, so fall back to `type[Unknown]` to avoid false-positive errors.
        if args.iter().any(ast::Expr::is_starred_expr) {
            return SubclassOfType::subclass_of_unknown();
        }

        // Extract members from the namespace dict (third argument).
        let (members, has_dynamic_namespace): (Box<[(ast::name::Name, Type<'db>)]>, bool) =
            if let ast::Expr::Dict(dict) = namespace_arg {
                // Check if all keys are string literal types. If any key is not a string literal
                // type or is missing (spread), the namespace is considered dynamic.
                let all_keys_are_string_literals = dict.items.iter().all(|item| {
                    item.key
                        .as_ref()
                        .is_some_and(|k| self.expression_type(k).is_string_literal())
                });
                let members = dict
                    .items
                    .iter()
                    .filter_map(|item| {
                        // Only extract items with string literal keys.
                        let key_expr = item.key.as_ref()?;
                        let key_name = self.expression_type(key_expr).as_string_literal()?;
                        let key_name = ast::name::Name::new(key_name.value(db));
                        // Get the already-inferred type from when we inferred the dict above.
                        let value_ty = self.expression_type(&item.value);
                        Some((key_name, value_ty))
                    })
                    .collect();
                (members, !all_keys_are_string_literals)
            } else if let Type::TypedDict(typed_dict) = namespace_type {
                // `namespace` is a TypedDict instance. Extract known keys as members.
                // TypedDicts are "open" (can have additional string keys), so this
                // is still a dynamic namespace for unknown attributes.
                let members: Box<[(ast::name::Name, Type<'db>)]> = typed_dict
                    .items(db)
                    .iter()
                    .map(|(name, field)| (name.clone(), field.declared_ty))
                    .collect();
                (members, true)
            } else {
                // `namespace` is not a dict literal, so it's dynamic.
                (Box::new([]), true)
            };

        if !matches!(namespace_type, Type::TypedDict(_))
            && !namespace_type.is_assignable_to(
                db,
                KnownClass::Dict
                    .to_specialized_instance(db, &[KnownClass::Str.to_instance(db), Type::any()]),
            )
            && let Some(builder) = self
                .context
                .report_lint(&INVALID_ARGUMENT_TYPE, namespace_arg)
        {
            let mut diagnostic = builder
                .into_diagnostic("Invalid argument to parameter 3 (`namespace`) of `type()`");
            diagnostic.set_primary_message(format_args!(
                "Expected `dict[str, Any]`, found `{}`",
                namespace_type.display(db)
            ));
        }

        // Extract name and base classes.
        let name = if let Some(literal) = name_type.as_string_literal() {
            Name::new(literal.value(db))
        } else {
            if !name_type.is_assignable_to(db, KnownClass::Str.to_instance(db))
                && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, name_arg)
            {
                let mut diagnostic =
                    builder.into_diagnostic("Invalid argument to parameter 1 (`name`) of `type()`");
                diagnostic.set_primary_message(format_args!(
                    "Expected `str`, found `{}`",
                    name_type.display(db)
                ));
            }
            Name::new_static("<unknown>")
        };

        let scope = self.scope();

        // For assigned `type()` calls, bases inference is deferred to handle forward references
        // and recursive references (e.g., `X = type("X", (tuple["X | None"],), {})`).
        // This avoids expensive Salsa fixpoint iteration by deferring inference until the
        // class type is already bound. For dangling calls, infer and extract bases eagerly
        // (they'll be stored in the anchor and used for validation).
        let explicit_bases = if definition.is_none() {
            let bases_type = self.infer_expression(bases_arg, TypeContext::default());
            self.extract_explicit_bases(bases_arg, bases_type)
        } else {
            None
        };

        // Create the anchor for identifying this dynamic class.
        // - For assigned `type()` calls, the Definition uniquely identifies the class,
        //   and bases inference is deferred.
        // - For dangling calls, compute a relative offset from the scope's node index,
        //   and store the explicit bases directly (since they were inferred eagerly).
        let anchor = if let Some(def) = definition {
            // Register for deferred inference to infer bases and validate later.
            self.deferred.insert(def, self.multi_inference_state);
            DynamicClassAnchor::Definition(def)
        } else {
            let call_node_index = call_expr.node_index().load();
            let scope_anchor = scope.node(db).node_index().unwrap_or(NodeIndex::from(0));
            let anchor_u32 = scope_anchor
                .as_u32()
                .expect("scope anchor should not be NodeIndex::NONE");
            let call_u32 = call_node_index
                .as_u32()
                .expect("call node should not be NodeIndex::NONE");

            // Use [Unknown] as fallback if bases extraction failed (e.g., not a tuple).
            let anchor_bases = explicit_bases
                .clone()
                .unwrap_or_else(|| Box::from([Type::unknown()]));

            DynamicClassAnchor::ScopeOffset {
                scope,
                offset: call_u32 - anchor_u32,
                explicit_bases: anchor_bases,
            }
        };

        let dynamic_class = DynamicClassLiteral::new(
            db,
            name.clone(),
            anchor,
            members,
            has_dynamic_namespace,
            None,
        );

        // For dangling calls, validate bases eagerly. For assigned calls, validation is
        // deferred along with bases inference.
        if let Some(explicit_bases) = &explicit_bases {
            // Validate bases and collect disjoint bases for diagnostics.
            let mut disjoint_bases =
                self.validate_dynamic_type_bases(bases_arg, explicit_bases, &name);

            // Check for MRO errors.
            if report_dynamic_mro_errors(&self.context, dynamic_class, call_expr, bases_arg) {
                // MRO succeeded, check for instance-layout-conflict.
                disjoint_bases.remove_redundant_entries(db);
                if disjoint_bases.len() > 1 {
                    report_instance_layout_conflict(
                        &self.context,
                        dynamic_class.header_range(db),
                        bases_arg.as_tuple_expr().map(|tuple| tuple.elts.as_slice()),
                        &disjoint_bases,
                    );
                }
            }

            // Check for metaclass conflicts.
            if let Err(DynamicMetaclassConflict {
                metaclass1,
                base1,
                metaclass2,
                base2,
            }) = dynamic_class.try_metaclass(db)
            {
                report_conflicting_metaclass_from_bases(
                    &self.context,
                    call_expr.into(),
                    dynamic_class.name(db),
                    metaclass1,
                    base1.display(db),
                    metaclass2,
                    base2.display(db),
                );
            }
        }

        Type::ClassLiteral(ClassLiteral::Dynamic(dynamic_class))
    }

    /// Extract explicit base types from a bases tuple type.
    ///
    /// Emits a diagnostic if `bases_type` is not a valid tuple type.
    ///
    /// Returns `None` if the bases cannot be extracted.
    fn extract_explicit_bases(
        &mut self,
        bases_node: &ast::Expr,
        bases_type: Type<'db>,
    ) -> Option<Box<[Type<'db>]>> {
        let db = self.db();
        // Check if bases_type is a tuple; emit diagnostic if not.
        if bases_type.tuple_instance_spec(db).is_none()
            && !bases_type.is_assignable_to(
                db,
                Type::homogeneous_tuple(db, KnownClass::Type.to_instance(db)),
            )
            && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, bases_node)
        {
            let mut diagnostic =
                builder.into_diagnostic("Invalid argument to parameter 2 (`bases`) of `type()`");
            diagnostic.set_primary_message(format_args!(
                "Expected `tuple[type, ...]`, found `{}`",
                bases_type.display(db)
            ));
        }
        bases_type
            .fixed_tuple_elements(db)
            .map(Cow::into_owned)
            .map(Into::into)
    }

    /// Validate base classes from the second argument of a `type()` call.
    ///
    /// This validates bases that are valid `ClassBase` variants but aren't allowed
    /// for dynamic classes created via `type()`. Invalid bases that can't be converted
    /// to `ClassBase` at all are handled by `DynamicMroErrorKind::InvalidBases`.
    ///
    /// Returns disjoint bases found (for instance-layout-conflict checking).
    fn validate_dynamic_type_bases(
        &mut self,
        bases_node: &ast::Expr,
        bases: &[Type<'db>],
        name: &Name,
    ) -> IncompatibleBases<'db> {
        let db = self.db();

        // Get AST nodes for base expressions (for diagnostics).
        let bases_tuple_elts = bases_node.as_tuple_expr().map(|t| t.elts.as_slice());

        let mut disjoint_bases = IncompatibleBases::default();

        // Check each base for special cases that are not allowed for dynamic classes.
        for (idx, base) in bases.iter().enumerate() {
            let diagnostic_node = bases_tuple_elts
                .and_then(|elts| elts.get(idx))
                .unwrap_or(bases_node);

            // Try to convert to ClassBase to check for special cases.
            let Some(class_base) = ClassBase::try_from_type(db, *base, None) else {
                // Can't convert; will be handled by `InvalidBases` error from `try_mro`.
                continue;
            };

            // Check for special bases that are not allowed for dynamic classes.
            // Dynamic classes can't be generic, protocols, TypedDicts, or enums.
            // (`NamedTuple` is rejected earlier: `try_from_type` returns `None`
            // without a concrete subclass, so it's reported as an `InvalidBases` MRO error.)
            match class_base {
                ClassBase::Generic | ClassBase::TypedDict => {
                    if let Some(builder) = self.context.report_lint(&INVALID_BASE, diagnostic_node)
                    {
                        let mut diagnostic =
                            builder.into_diagnostic("Invalid base for class created via `type()`");
                        diagnostic
                            .set_primary_message(format_args!("Has type `{}`", base.display(db)));
                        match class_base {
                            ClassBase::Generic => {
                                diagnostic.info("Classes created via `type()` cannot be generic");
                                diagnostic.info(format_args!(
                                    "Consider using `class {name}(Generic[...]): ...` instead"
                                ));
                            }
                            ClassBase::TypedDict => {
                                diagnostic
                                    .info("Classes created via `type()` cannot be TypedDicts");
                                diagnostic.info(format_args!(
                                    "Consider using `TypedDict(\"{name}\", {{}})` instead"
                                ));
                            }
                            _ => unreachable!(),
                        }
                    }
                }
                ClassBase::Protocol => {
                    if let Some(builder) = self
                        .context
                        .report_lint(&UNSUPPORTED_DYNAMIC_BASE, diagnostic_node)
                    {
                        let mut diagnostic = builder
                            .into_diagnostic("Unsupported base for class created via `type()`");
                        diagnostic
                            .set_primary_message(format_args!("Has type `{}`", base.display(db)));
                        diagnostic.info("Classes created via `type()` cannot be protocols");
                        diagnostic.info(format_args!(
                            "Consider using `class {name}(Protocol): ...` instead"
                        ));
                    }
                }
                ClassBase::Class(class_type) => {
                    // Check if base is @final (includes enums with members).
                    // If it's @final, we emit a diagnostic and skip other checks
                    // to avoid duplicate errors (e.g., enums with members are both
                    // @final and would trigger the enum-specific diagnostic).
                    if class_type.is_final(db) {
                        if let Some(builder) = self
                            .context
                            .report_lint(&SUBCLASS_OF_FINAL_CLASS, diagnostic_node)
                        {
                            builder.into_diagnostic(format_args!(
                                "Class `{name}` cannot inherit from final class `{}`",
                                class_type.name(db)
                            ));
                        }
                        // Still collect disjoint bases even for invalid bases.
                        if let Some(disjoint_base) = class_type.nearest_disjoint_base(db) {
                            disjoint_bases.insert(disjoint_base, idx, class_type.class_literal(db));
                        }
                        continue;
                    }

                    // Enum subclasses require the EnumMeta metaclass, which
                    // expects special dict attributes that `type()` doesn't provide.
                    if let Some((static_class, _)) = class_type.static_class_literal(db) {
                        if is_enum_class_by_inheritance(db, static_class) {
                            if let Some(builder) =
                                self.context.report_lint(&INVALID_BASE, diagnostic_node)
                            {
                                let mut diagnostic = builder
                                    .into_diagnostic("Invalid base for class created via `type()`");
                                diagnostic.set_primary_message(format_args!(
                                    "Has type `{}`",
                                    base.display(db)
                                ));
                                diagnostic
                                    .info("Creating an enum class via `type()` is not supported");
                                diagnostic.info(format_args!(
                                    "Consider using `Enum(\"{name}\", [])` instead"
                                ));
                            }
                            // Still collect disjoint bases even for invalid bases.
                            if let Some(disjoint_base) = class_type.nearest_disjoint_base(db) {
                                disjoint_bases.insert(
                                    disjoint_base,
                                    idx,
                                    class_type.class_literal(db),
                                );
                            }
                            continue;
                        }
                    }

                    // Collect disjoint bases for instance-layout-conflict checking.
                    if let Some(disjoint_base) = class_type.nearest_disjoint_base(db) {
                        disjoint_bases.insert(disjoint_base, idx, class_type.class_literal(db));
                    }
                }
                ClassBase::Dynamic(_) => {
                    // Dynamic bases are allowed.
                }
            }
        }

        disjoint_bases
    }

    fn infer_annotated_assignment_statement(&mut self, assignment: &ast::StmtAnnAssign) {
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
                for qualifier in [TypeQualifiers::CLASS_VAR, TypeQualifiers::INIT_VAR] {
                    if annotated.qualifiers.contains(qualifier)
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
                        "`{name}.{attr_name}` is only valid for annotating `{variadic}` function parameters",
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
                            "`{name}.{attr_name}` is only valid for annotating `{variadic}` function parameters",
                        ));
                    }
                }
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

            // But here we explicitly overwrite the type for the overall `self.attr` node.
            // We do not use `store_expression_type` here, because it checks that no type
            // has been stored for the expression before. When there's a value, use the
            // inferred type (matching the name-target definition path); otherwise fall
            // back to the annotated type. If the value is not assignable to the declared
            // type, report an error and fall back to the annotated type.
            let target_ty = if let Some(value_ty) = value_ty {
                let declared_ty = annotated.inner_type();
                if value_ty.is_assignable_to(self.db(), declared_ty) {
                    value_ty
                } else {
                    if let Some(builder) = self
                        .context
                        .report_lint(&INVALID_ASSIGNMENT, value.as_deref().unwrap())
                    {
                        let mut diag = builder.into_diagnostic(format_args!(
                            "Object of type `{}` is not assignable to `{}`",
                            value_ty.display(self.db()),
                            declared_ty.display(self.db()),
                        ));
                        diag.annotate(
                            self.context
                                .secondary(annotation.as_ref())
                                .message("Declared type"),
                        );
                        diag.set_primary_message(format_args!(
                            "Incompatible value of type `{}`",
                            value_ty.display(self.db()),
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

    /// Infer the types in an annotated assignment definition.
    fn infer_annotated_assignment_definition(
        &mut self,
        assignment: &'db AnnotatedAssignmentDefinitionKind,
        definition: Definition<'db>,
    ) {
        /// Simple syntactic validation for the right-hand sides of PEP-613 type aliases.
        ///
        /// TODO: this is far from exhaustive and should be improved.
        const fn alias_syntax_validation(expr: &ast::Expr) -> bool {
            const fn inner(expr: &ast::Expr, allow_context_dependent: bool) -> bool {
                match expr {
                    ast::Expr::Name(_)
                    | ast::Expr::StringLiteral(_)
                    | ast::Expr::NoneLiteral(_) => true,
                    ast::Expr::Attribute(ast::ExprAttribute {
                        value,
                        attr: _,
                        node_index: _,
                        range: _,
                        ctx: _,
                    }) => inner(value, allow_context_dependent),
                    ast::Expr::Subscript(ast::ExprSubscript {
                        value,
                        slice,
                        node_index: _,
                        range: _,
                        ctx: _,
                    }) => {
                        if !inner(value, allow_context_dependent) {
                            return false;
                        }
                        match &**slice {
                            ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                                match elts.as_slice() {
                                    [first, ..] => inner(first, true),
                                    _ => true,
                                }
                            }
                            _ => inner(slice, true),
                        }
                    }
                    ast::Expr::BinOp(ast::ExprBinOp {
                        left,
                        op,
                        right,
                        range: _,
                        node_index: _,
                    }) => {
                        op.is_bit_or()
                            && inner(left, allow_context_dependent)
                            && inner(right, allow_context_dependent)
                    }
                    ast::Expr::UnaryOp(ast::ExprUnaryOp {
                        op,
                        operand,
                        range: _,
                        node_index: _,
                    }) => {
                        allow_context_dependent
                            && matches!(op, ast::UnaryOp::UAdd | ast::UnaryOp::USub)
                            && matches!(
                                &**operand,
                                ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                                    value: ast::Number::Int(_),
                                    ..
                                })
                            )
                    }
                    ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                        value,
                        node_index: _,
                        range: _,
                    }) => allow_context_dependent && value.is_int(),
                    ast::Expr::EllipsisLiteral(_)
                    | ast::Expr::BytesLiteral(_)
                    | ast::Expr::BooleanLiteral(_)
                    | ast::Expr::Starred(_)
                    | ast::Expr::List(_) => allow_context_dependent,
                    _ => false,
                }
            }
            inner(expr, false)
        }

        let annotation = assignment.annotation(self.module());
        let target = assignment.target(self.module());
        let value = assignment.value(self.module());

        let mut declared = self.infer_annotation_expression_allow_pep_613(
            annotation,
            DeferredExpressionState::from(self.defer_annotations()),
        );

        // P.args and P.kwargs are only valid as annotations on *args and **kwargs,
        // not as variable annotations. Check both resolved type and AST form.
        if let Type::TypeVar(typevar) = declared.inner_type()
            && typevar.is_paramspec(self.db())
            && let Some(attr) = typevar.paramspec_attr(self.db())
        {
            let name = typevar.name(self.db());
            let (attr_name, variadic) = match attr {
                ParamSpecAttrKind::Args => ("args", "*args"),
                ParamSpecAttrKind::Kwargs => ("kwargs", "**kwargs"),
            };
            if let Some(builder) = self.context.report_lint(&INVALID_PARAMSPEC, annotation) {
                builder.into_diagnostic(format_args!(
                    "`{name}.{attr_name}` is only valid for annotating `{variadic}` function parameters",
                ));
            }
        } else if let ast::Expr::Attribute(attr_expr) = annotation
            && matches!(attr_expr.attr.as_str(), "args" | "kwargs")
        {
            // Also check the AST form for cases where P isn't bound (e.g., class body
            // annotations). In this case, the type might not resolve to a TypeVar.
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
                if let Some(builder) = self.context.report_lint(&INVALID_PARAMSPEC, annotation) {
                    builder.into_diagnostic(format_args!(
                        "`{name}.{attr_name}` is only valid for annotating `{variadic}` function parameters",
                    ));
                }
            }
        }

        let is_pep_613_type_alias = declared.inner_type().is_typealias_special_form();

        if is_pep_613_type_alias
            && let Some(value) = value
            && !alias_syntax_validation(value)
            && let Some(builder) = self.context.report_lint(
                &INVALID_TYPE_FORM,
                definition.full_range(self.db(), self.module()),
            )
        {
            // TODO: better error message; full type-expression validation; etc.
            let mut diagnostic = builder
                .into_diagnostic("Invalid right-hand side for `typing.TypeAlias` assignment");
            diagnostic.help(
                "See https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions",
            );
        }

        if !declared.qualifiers.is_empty() {
            let current_scope_id = self.scope().file_scope_id(self.db());
            let current_scope = self.index.scope(current_scope_id);
            if current_scope.kind() != ScopeKind::Class {
                for qualifier in [TypeQualifiers::CLASS_VAR, TypeQualifiers::INIT_VAR] {
                    if declared.qualifiers.contains(qualifier)
                        && let Some(builder) =
                            self.context.report_lint(&INVALID_TYPE_FORM, annotation)
                    {
                        builder.into_diagnostic(format_args!(
                            "`{name}` annotations are only allowed in class-body scopes",
                            name = qualifier.name()
                        ));
                    }
                }
            }

            // `Required`, `NotRequired`, and `ReadOnly` are only valid inside TypedDict classes.
            if declared.qualifiers.intersects(
                TypeQualifiers::REQUIRED | TypeQualifiers::NOT_REQUIRED | TypeQualifiers::READ_ONLY,
            ) {
                let in_typed_dict = current_scope.kind() == ScopeKind::Class
                    && nearest_enclosing_class(self.db(), self.index, self.scope()).is_some_and(
                        |class| {
                            class.iter_mro(self.db(), None).any(|base| {
                                matches!(
                                    base,
                                    ClassBase::TypedDict
                                        | ClassBase::Dynamic(DynamicType::TodoFunctionalTypedDict)
                                )
                            })
                        },
                    );
                if !in_typed_dict {
                    for qualifier in [
                        TypeQualifiers::REQUIRED,
                        TypeQualifiers::NOT_REQUIRED,
                        TypeQualifiers::READ_ONLY,
                    ] {
                        if declared.qualifiers.contains(qualifier)
                            && let Some(builder) =
                                self.context.report_lint(&INVALID_TYPE_FORM, annotation)
                        {
                            builder.into_diagnostic(format_args!(
                                "`{name}` is only allowed in TypedDict fields",
                                name = qualifier.name()
                            ));
                        }
                    }
                }
            }
        }

        if target
            .as_name_expr()
            .is_some_and(|name| &name.id == "TYPE_CHECKING")
        {
            if !KnownClass::Bool
                .to_instance(self.db())
                .is_assignable_to(self.db(), declared.inner_type())
            {
                // annotation not assignable from `bool` is an error
                report_invalid_type_checking_constant(&self.context, target.into());
            } else if self.in_stub()
                && value
                    .as_ref()
                    .is_none_or(|value| value.is_ellipsis_literal_expr())
            {
                // stub file assigning nothing or `...` is fine
            } else if !matches!(
                value
                    .as_ref()
                    .and_then(|value| value.as_boolean_literal_expr()),
                Some(ast::ExprBooleanLiteral { value: false, .. })
            ) {
                // otherwise, assigning something other than `False` is an error
                report_invalid_type_checking_constant(&self.context, target.into());
            }
            declared.inner = Type::bool_literal(true);
        }

        // Handle various singletons.
        if let Some(name_expr) = target.as_name_expr()
            && let Some(special_form) =
                SpecialFormType::try_from_file_and_name(self.db(), self.file(), &name_expr.id)
        {
            declared.inner = Type::SpecialForm(special_form);
        }

        // If the target of an assignment is not one of the place expressions we support,
        // then they are not definitions, so we can only be here if the target is in a form supported as a place expression.
        // In this case, we can simply store types in `target` below, instead of calling `infer_expression` (which would return `Never`).
        debug_assert!(PlaceExpr::try_from_expr(target).is_some());

        if let Some(value) = value {
            self.setup_dataclass_field_specifiers();

            // We defer the r.h.s. of PEP-613 `TypeAlias` assignments in stub files.
            let previous_deferred_state = self.deferred_state;

            if is_pep_613_type_alias && self.in_stub() {
                self.deferred_state = DeferredExpressionState::Deferred;
            }

            // This might be a PEP-613 type alias (`OptionalList: TypeAlias = list[T] | None`). Use
            // the definition of `OptionalList` as the binding context while inferring the
            // RHS (`list[T] | None`), in order to bind `T` to `OptionalList`.
            let previous_typevar_binding_context = self.typevar_binding_context.replace(definition);

            let inferred_ty = self.infer_maybe_standalone_expression(
                value,
                TypeContext::new(Some(declared.inner_type())),
            );

            self.typevar_binding_context = previous_typevar_binding_context;

            self.deferred_state = previous_deferred_state;

            self.dataclass_field_specifiers.clear();

            let inferred_ty = if target
                .as_name_expr()
                .is_some_and(|name| &name.id == "TYPE_CHECKING")
            {
                Type::bool_literal(true)
            } else if self.in_stub() && value.is_ellipsis_literal_expr() {
                declared.inner_type()
            } else {
                inferred_ty
            };

            if is_pep_613_type_alias {
                let is_valid_special_form = |ty: Type<'db>| match ty {
                    Type::SpecialForm(SpecialFormType::TypeQualifier(_)) => false,
                    Type::ClassLiteral(literal) => {
                        !literal.is_known(self.db(), KnownClass::InitVar)
                    }
                    _ => true,
                };

                let is_invalid = match value {
                    ast::Expr::Subscript(sub) => {
                        !is_valid_special_form(self.expression_type(&sub.value))
                    }
                    _ => !is_valid_special_form(self.expression_type(value)),
                };

                if is_invalid
                    && let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, value)
                {
                    builder.into_diagnostic(
                        "Type qualifiers are not allowed in type alias definitions",
                    );
                }

                let inferred_ty =
                    if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = inferred_ty {
                        let identity = TypeVarIdentity::new(
                            self.db(),
                            typevar.identity(self.db()).name(self.db()),
                            typevar.identity(self.db()).definition(self.db()),
                            TypeVarKind::Pep613Alias,
                        );
                        Type::KnownInstance(KnownInstanceType::TypeVar(
                            typevar.with_identity(self.db(), identity),
                        ))
                    } else {
                        inferred_ty
                    };
                self.add_declaration_with_binding(
                    target.into(),
                    definition,
                    &DeclaredAndInferredType::AreTheSame(TypeAndQualifiers::declared(inferred_ty)),
                );
            } else {
                // Check for annotated enum members. The typing spec states that enum
                // members should not have explicit type annotations.
                if let Some(name_expr) = target.as_name_expr()
                    && !name_expr.id.starts_with("__")
                    && !matches!(name_expr.id.as_str(), "_ignore_" | "_value_" | "_name_")
                    // Not bare Final (bare Final is allowed on enum members)
                    && !(declared.qualifiers.contains(TypeQualifiers::FINAL)
                        && matches!(declared.inner_type(), Type::Dynamic(DynamicType::Unknown)))
                    // Value type would be an enum member at runtime (exclude callables,
                    // which are never members)
                    && !inferred_ty.is_subtype_of(
                        self.db(),
                        Type::Callable(CallableType::unknown(self.db()))
                            .top_materialization(self.db()),
                    )
                {
                    let current_scope_id = self.scope().file_scope_id(self.db());
                    let current_scope = self.index.scope(current_scope_id);
                    if current_scope.kind() == ScopeKind::Class
                        && let Some(class) =
                            nearest_enclosing_class(self.db(), self.index, self.scope())
                        && is_enum_class_by_inheritance(self.db(), class)
                        && !enum_ignored_names(self.db(), self.scope()).contains(&name_expr.id)
                        && let Some(builder) = self
                            .context
                            .report_lint(&INVALID_ENUM_MEMBER_ANNOTATION, annotation)
                    {
                        let mut diag = builder.into_diagnostic(format_args!(
                            "Type annotation on enum member `{}` is not allowed",
                            &name_expr.id
                        ));
                        diag.info(
                            "See: https://typing.python.org/en/latest/spec/enums.html#enum-members",
                        );
                    }
                }

                self.add_declaration_with_binding(
                    target.into(),
                    definition,
                    &DeclaredAndInferredType::MightBeDifferent {
                        declared_ty: declared,
                        inferred_ty,
                    },
                );
            }

            self.store_expression_type(target, inferred_ty);
        } else {
            if is_pep_613_type_alias {
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, annotation) {
                    builder.into_diagnostic(
                        "`TypeAlias` must be assigned a value in annotated assignments",
                    );
                }
                declared.inner = Type::unknown();
            }
            if self.in_stub() {
                self.add_declaration_with_binding(
                    target.into(),
                    definition,
                    &DeclaredAndInferredType::AreTheSame(declared),
                );
            } else {
                self.add_declaration(target.into(), definition, declared);
            }

            self.store_expression_type(target, declared.inner_type());
        }
    }

    fn infer_augmented_assignment_statement(&mut self, assignment: &ast::StmtAugAssign) {
        if assignment.target.is_name_expr() {
            self.infer_definition(assignment);
        } else {
            // Non-name assignment targets are inferred as ordinary expressions, not definitions.
            self.infer_augment_assignment(assignment);
        }
    }

    fn infer_augmented_op(
        &mut self,
        assignment: &ast::StmtAugAssign,
        target_type: Type<'db>,
        value_expr: &ast::Expr,
        infer_value_ty: &mut dyn FnMut(&mut Self, TypeContext<'db>) -> Type<'db>,
    ) -> Type<'db> {
        // If the target defines, e.g., `__iadd__`, infer the augmented assignment as a call to that
        // dunder.
        let op = assignment.op;
        let db = self.db();

        // Fall back to non-augmented binary operator inference.
        let binary_return_ty = |builder: &mut Self, value_ty| {
            builder
                .infer_binary_expression_type(assignment.into(), false, target_type, value_ty, op)
                .unwrap_or_else(|| {
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

                union.map(db, |&elem_type| {
                    self.infer_augmented_op(
                        assignment,
                        elem_type,
                        value_expr,
                        &mut |builder, tcx| infer_value_ty.infer_silent(builder, tcx),
                    )
                })
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
                    return typed_dict_update_ty;
                }

                let ast_arguments = [ArgOrKeyword::Arg(value_expr)];
                let mut call_arguments = CallArguments::positional([Type::unknown()]);

                let call = self.infer_and_try_call_dunder(
                    db,
                    target_type,
                    op.in_place_dunder(),
                    ArgumentsIter::synthesized(&ast_arguments),
                    &mut call_arguments,
                    &mut |builder, (_, _, tcx)| infer_value_ty(builder, tcx),
                    TypeContext::default(),
                );

                let [Some(value_ty)] = call_arguments.types() else {
                    unreachable!();
                };

                match call {
                    Ok(outcome) => outcome.return_type(db),
                    Err(CallDunderError::MethodNotAvailable) => {
                        let value_ty = infer_value_ty(self, TypeContext::default());
                        binary_return_ty(self, value_ty)
                    }
                    Err(CallDunderError::PossiblyUnbound(outcome)) => UnionType::from_two_elements(
                        db,
                        outcome.return_type(db),
                        binary_return_ty(self, *value_ty),
                    ),
                    Err(CallDunderError::CallError(_, bindings)) => {
                        report_unsupported_augmented_assignment(
                            &self.context,
                            assignment,
                            target_type,
                            *value_ty,
                        );
                        bindings.return_type(db)
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
        let target_ty = self.infer_augment_assignment(assignment);
        self.add_binding(assignment.into(), definition)
            .insert(self, target_ty);
    }

    fn infer_augment_assignment(&mut self, assignment: &ast::StmtAugAssign) -> Type<'db> {
        let ast::StmtAugAssign {
            range: _,
            node_index: _,
            target,
            op: _,
            value,
        } = assignment;

        // Resolve the target type, assuming a load context.
        let target_type = match &**target {
            ast::Expr::Name(name) => {
                let previous_value = self.infer_name_load(name);
                self.store_expression_type(target, previous_value);
                previous_value
            }
            ast::Expr::Attribute(attr) => {
                let previous_value = self.infer_attribute_load(attr);
                self.store_expression_type(target, previous_value);
                previous_value
            }
            ast::Expr::Subscript(subscript) => {
                let previous_value = self.infer_subscript_load(subscript);
                self.store_expression_type(target, previous_value);
                previous_value
            }
            _ => self.infer_expression(target, TypeContext::default()),
        };

        self.infer_augmented_op(assignment, target_type, value, &mut |builder, tcx| {
            builder.infer_expression(value, tcx)
        })
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

    fn infer_for_statement(&mut self, for_statement: &ast::StmtFor) {
        let ast::StmtFor {
            range: _,
            node_index: _,
            target,
            iter,
            body,
            orelse,
            is_async: _,
        } = for_statement;

        self.infer_target(target, iter, &|builder, tcx| {
            // TODO: `infer_for_statement_definition` reports a diagnostic if `iter_ty` isn't iterable
            //  but only if the target is a name. We should report a diagnostic here if the target isn't a name:
            //  `for a.x in not_iterable: ...
            builder
                .infer_standalone_expression(iter, tcx)
                .iterate(builder.db())
                .homogeneous_element_type(builder.db())
        });

        self.infer_body(body);
        self.infer_body(orelse);
    }

    fn infer_for_statement_definition(
        &mut self,
        for_stmt: &ForStmtDefinitionKind<'db>,
        definition: Definition<'db>,
    ) {
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

                iterable_type
                    .try_iterate_with_mode(
                        self.db(),
                        EvaluationMode::from_is_async(for_stmt.is_async()),
                    )
                    .map(|tuple| tuple.homogeneous_element_type(self.db()))
                    .unwrap_or_else(|err| {
                        err.report_diagnostic(&self.context, iterable_type, iterable.into());
                        err.fallback_element_type(self.db())
                    })
            }
        };

        self.store_expression_type(target, loop_var_value_type);
        self.add_binding(target.into(), definition)
            .insert(self, loop_var_value_type);
    }

    fn infer_while_statement(&mut self, while_statement: &ast::StmtWhile) {
        let ast::StmtWhile {
            range: _,
            node_index: _,
            test,
            body,
            orelse,
        } = while_statement;

        let test_ty = self.infer_standalone_expression(test, TypeContext::default());

        if let Err(err) = test_ty.try_bool(self.db()) {
            err.report_diagnostic(&self.context, &**test);
        }

        self.infer_body(body);
        self.infer_body(orelse);
    }

    fn infer_assert_statement(&mut self, assert: &ast::StmtAssert) {
        let ast::StmtAssert {
            range: _,
            node_index: _,
            test,
            msg,
        } = assert;

        let test_ty = self.infer_standalone_expression(test, TypeContext::default());

        if let Err(err) = test_ty.try_bool(self.db()) {
            err.report_diagnostic(&self.context, &**test);
        }

        self.infer_optional_expression(msg.as_deref(), TypeContext::default());
    }

    fn infer_raise_statement(&mut self, raise: &ast::StmtRaise) {
        let ast::StmtRaise {
            range: _,
            node_index: _,
            exc,
            cause,
        } = raise;

        let base_exception_type = KnownClass::BaseException.to_subclass_of(self.db());
        let base_exception_instance = KnownClass::BaseException.to_instance(self.db());

        let can_be_raised =
            UnionType::from_two_elements(self.db(), base_exception_type, base_exception_instance);
        let can_be_exception_cause =
            UnionType::from_two_elements(self.db(), can_be_raised, Type::none(self.db()));

        if let Some(raised) = exc {
            let raised_type = self.infer_expression(raised, TypeContext::default());

            if !raised_type.is_assignable_to(self.db(), can_be_raised) {
                report_invalid_exception_raised(&self.context, raised, raised_type);
            }
        }

        if let Some(cause) = cause {
            let cause_type = self.infer_expression(cause, TypeContext::default());

            if !cause_type.is_assignable_to(self.db(), can_be_exception_cause) {
                report_invalid_exception_cause(&self.context, cause, cause_type);
            }
        }
    }

    fn infer_return_statement(&mut self, ret: &ast::StmtReturn) {
        let tcx = if ret.value.is_some() {
            nearest_enclosing_function(self.db(), self.index, self.scope())
                .map(|func| {
                    // When inferring expressions within a function body,
                    // the expected type passed should be the "raw" type,
                    // i.e. type variables in the return type are non-inferable,
                    // and the return types of async functions are not wrapped in `CoroutineType[...]`.
                    TypeContext::new(Some(
                        func.last_definition_raw_signature(self.db()).return_ty,
                    ))
                })
                .unwrap_or_default()
        } else {
            TypeContext::default()
        };
        if let Some(ty) = self.infer_optional_expression(ret.value.as_deref(), tcx) {
            let range = ret
                .value
                .as_ref()
                .map_or(ret.range(), |value| value.range());
            self.record_return_type(ty, range);
        } else {
            self.record_return_type(Type::none(self.db()), ret.range());
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
            if !module_type_implicit_global_symbol(self.db(), name)
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
            diag.set_primary_message(format_args!(
                "`{name}` has no declarations or bindings in the global scope"
            ));
            diag.info("This limits ty's ability to make accurate inferences about the boundness and types of global-scope symbols");
            diag.info(format_args!(
                "Consider adding a declaration to the global scope, e.g. `{name}: int`"
            ));
        }
    }

    fn infer_nonlocal_statement(&mut self, nonlocal: &ast::StmtNonlocal) {
        let ast::StmtNonlocal {
            node_index: _,
            range,
            names,
        } = nonlocal;
        let db = self.db();
        let scope = self.scope();
        let file_scope_id = scope.file_scope_id(db);

        'names: for name in names {
            // Walk up parent scopes looking for a possible enclosing scope that may have a
            // definition of this name visible to us. Note that we skip the scope containing the
            // use that we are resolving, since we already looked for the place there up above.
            for (enclosing_scope_file_id, _) in self.index.ancestor_scopes(file_scope_id).skip(1) {
                // Class scopes are not visible to nested scopes, and `nonlocal` cannot refer to
                // globals, so check only function-like scopes.
                let enclosing_scope = self.index.scope(enclosing_scope_file_id);
                if !enclosing_scope.kind().is_function_like() {
                    continue;
                }
                let enclosing_place_table = self.index.place_table(enclosing_scope_file_id);
                let Some(enclosing_symbol_id) = enclosing_place_table.symbol_id(name) else {
                    // This scope doesn't define this name. Keep going.
                    continue;
                };
                let enclosing_symbol = enclosing_place_table.symbol(enclosing_symbol_id);
                // We've found a definition for this name in an enclosing function-like scope.
                // Either this definition is the valid place this name refers to, or else we'll
                // emit a syntax error. Either way, we won't walk any more enclosing scopes. Note
                // that there are differences here compared to `infer_place_load`: A regular load
                // (e.g. `print(x)`) is allowed to refer to a global variable (e.g. `x = 1` in the
                // global scope), and similarly it's allowed to refer to a local variable in an
                // enclosing function that's declared `global` (e.g. `global x`). However, the
                // `nonlocal` keyword can't refer to global variables (that's a `SyntaxError`), and
                // it also can't refer to local variables in enclosing functions that are declared
                // `global` (also a `SyntaxError`).
                if enclosing_symbol.is_global() {
                    // A "chain" of `nonlocal` statements is "broken" by a `global` statement. Stop
                    // looping and report that this `nonlocal` statement is invalid.
                    break;
                }
                if !enclosing_symbol.is_bound()
                    && !enclosing_symbol.is_declared()
                    && !enclosing_symbol.is_nonlocal()
                {
                    debug_assert!(enclosing_symbol.is_used());
                    // The name is only referenced here, not defined. Keep going.
                    continue;
                }
                // We found a definition. We've checked that the name isn't `global` in this scope,
                // but it's ok if it's `nonlocal`. If a "chain" of `nonlocal` statements fails to
                // lead to a valid binding, the outermost one will be an error; we don't need to
                // walk the whole chain for each one.
                continue 'names;
            }
            // There's no matching binding in an enclosing scope. This `nonlocal` statement is
            // invalid.
            if let Some(builder) = self
                .context
                .report_diagnostic(DiagnosticId::InvalidSyntax, Severity::Error)
            {
                builder
                    .into_diagnostic(format_args!("no binding for nonlocal `{name}` found"))
                    .annotate(Annotation::primary(self.context.span(*range)));
            }
        }
    }

    fn module_type_from_name(&self, module_name: &ModuleName) -> Option<Type<'db>> {
        resolve_module(self.db(), self.file(), module_name)
            .map(|module| Type::module_literal(self.db(), self.file(), module))
    }

    fn infer_decorator(&mut self, decorator: &ast::Decorator) -> Type<'db> {
        let ast::Decorator {
            range: _,
            node_index: _,
            expression,
        } = decorator;

        self.infer_expression(expression, TypeContext::default())
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
    ) -> Type<'db> {
        fn propagate_callable_kind<'d>(
            db: &'d dyn Db,
            ty: Type<'d>,
            kind: CallableTypeKind,
        ) -> Option<Type<'d>> {
            match ty {
                Type::Callable(callable) => Some(Type::Callable(CallableType::new(
                    db,
                    callable.signatures(db),
                    kind,
                ))),
                Type::Union(union) => {
                    union.try_map(db, |element| propagate_callable_kind(db, *element, kind))
                }
                Type::TypeAlias(alias) => propagate_callable_kind(db, alias.value_type(db), kind),
                // Intersections are currently not handled here because that would require
                // the decorator to be explicitly annotated as returning an intersection.
                Type::Intersection(_) => None,
                // All other types cannot have a callable kind propagated to them.
                Type::Dynamic(_)
                | Type::Never
                | Type::FunctionLiteral(_)
                | Type::BoundMethod(_)
                | Type::KnownBoundMethod(_)
                | Type::WrapperDescriptor(_)
                | Type::DataclassDecorator(_)
                | Type::DataclassTransformer(_)
                | Type::ModuleLiteral(_)
                | Type::ClassLiteral(_)
                | Type::GenericAlias(_)
                | Type::SubclassOf(_)
                | Type::NominalInstance(_)
                | Type::ProtocolInstance(_)
                | Type::SpecialForm(_)
                | Type::KnownInstance(_)
                | Type::PropertyInstance(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::LiteralValue(_)
                | Type::TypeVar(_)
                | Type::BoundSuper(_)
                | Type::TypeIs(_)
                | Type::TypeGuard(_)
                | Type::TypedDict(_)
                | Type::NewTypeInstance(_) => None,
            }
        }

        // For FunctionLiteral, get the kind directly without computing the full signature.
        // This avoids a query cycle when the function has default parameter values, since
        // computing the signature requires evaluating those defaults which may trigger
        // deferred inference.
        let propagatable_kind = match decorated_ty {
            Type::FunctionLiteral(func) => {
                let db = self.db();
                if func.is_classmethod(db) {
                    Some(CallableTypeKind::ClassMethodLike)
                } else if func.is_staticmethod(db) {
                    Some(CallableTypeKind::StaticMethodLike)
                } else {
                    Some(CallableTypeKind::FunctionLike)
                }
            }
            _ => decorated_ty
                .try_upcast_to_callable(self.db())
                .and_then(CallableTypes::exactly_one)
                .and_then(|callable| match callable.kind(self.db()) {
                    kind @ (CallableTypeKind::FunctionLike
                    | CallableTypeKind::StaticMethodLike
                    | CallableTypeKind::ClassMethodLike) => Some(kind),
                    _ => None,
                }),
        };

        let call_arguments = CallArguments::positional([decorated_ty]);
        let return_ty = decorator_ty
            .try_call(self.db(), &call_arguments)
            .map(|bindings| bindings.return_type(self.db()))
            .unwrap_or_else(|CallError(_, bindings)| {
                bindings.report_diagnostics(&self.context, decorator_node.into());
                bindings.return_type(self.db())
            });

        // When a method on a class is decorated with a function that returns a
        // `Callable`, assume that the returned callable is also function-like (or
        // classmethod-like or staticmethod-like). See "Decorating a method with
        // a `Callable`-typed decorator" in `callables_as_descriptors.md` for the
        // extended explanation.
        propagatable_kind
            .and_then(|kind| propagate_callable_kind(self.db(), return_ty, kind))
            .unwrap_or(return_ty)
    }

    /// Infer the argument types for a single binding.
    fn infer_argument_types<'a>(
        &mut self,
        ast_arguments: &ast::Arguments,
        arguments: &mut CallArguments<'a, 'db>,
        argument_forms: &[Option<ParameterForm>],
    ) {
        debug_assert!(
            ast_arguments.len() == arguments.len() && arguments.len() == argument_forms.len()
        );

        let iter = itertools::izip!(
            arguments.iter_mut(),
            argument_forms.iter().copied(),
            ast_arguments.arguments_source_order()
        );

        for ((_, argument_type), argument_form, ast_argument) in iter {
            let argument = match ast_argument {
                // Splatted arguments are inferred before parameter matching to
                // determine their length.
                ast::ArgOrKeyword::Arg(ast::Expr::Starred(_))
                | ast::ArgOrKeyword::Keyword(ast::Keyword { arg: None, .. }) => continue,

                ast::ArgOrKeyword::Arg(arg) => arg,
                ast::ArgOrKeyword::Keyword(ast::Keyword { value, .. }) => value,
            };

            let ty = self.infer_argument_type(argument, argument_form, TypeContext::default());
            *argument_type = Some(ty);
        }
    }

    #[expect(clippy::too_many_arguments)]
    fn infer_and_try_call_dunder(
        &mut self,
        db: &'db dyn Db,
        object: Type<'db>,
        name: &str,
        ast_arguments: ArgumentsIter<'_>,
        argument_types: &mut CallArguments<'_, 'db>,
        infer_argument_ty: &mut dyn FnMut(&mut Self, ArgExpr<'db, '_>) -> Type<'db>,
        call_expression_tcx: TypeContext<'db>,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        match object
            .member_lookup_with_policy(db, name.into(), MemberLookupPolicy::NO_INSTANCE_FALLBACK)
            .place
        {
            Place::Defined(DefinedPlace {
                ty: dunder_callable,
                definedness: boundness,
                ..
            }) => {
                let mut bindings = dunder_callable
                    .bindings(db)
                    .match_parameters(db, argument_types);

                if let Err(call_error) = self.infer_and_check_argument_types(
                    ast_arguments,
                    argument_types,
                    infer_argument_ty,
                    &mut bindings,
                    call_expression_tcx,
                ) {
                    return Err(CallDunderError::CallError(call_error, Box::new(bindings)));
                }

                if boundness == Definedness::PossiblyUndefined {
                    return Err(CallDunderError::PossiblyUnbound(Box::new(bindings)));
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
        let db = self.db();
        let constraints = ConstraintSetBuilder::new();

        let has_generic_context = bindings
            .iter_flat()
            .flat_map(CallableBinding::overloads)
            .any(|overload| overload.signature.generic_context.is_some());

        // If the type context is a union, attempt to narrow to a specific element.
        let narrow_targets: &[_] = match call_expression_tcx.annotation {
            // TODO: We could theoretically attempt to narrow to every element of
            // the power set of this union. However, this leads to an exponential
            // explosion of inference attempts, and is rarely needed in practice.
            //
            // We only need to attempt narrowing on generic calls, otherwise the type
            // context has no effect.
            Some(Type::Union(union)) if has_generic_context => union.elements(db),
            _ => &[],
        };

        // We silence diagnostics until we successfully narrow to a specific type.
        let was_in_multi_inference = self.context.set_multi_inference(true);

        let mut try_narrow = |narrowed_ty| {
            let mut speculated_bindings = bindings.clone();
            let narrowed_tcx = TypeContext::new(Some(narrowed_ty));

            // Attempt to infer the argument types using the narrowed type context.
            self.infer_all_argument_types(
                ast_arguments.clone(),
                argument_types,
                infer_argument_ty,
                bindings,
                narrowed_tcx,
                MultiInferenceState::Ignore,
            );

            // Ensure the argument types match their annotated types.
            if speculated_bindings
                .check_types_impl(
                    db,
                    &constraints,
                    argument_types,
                    narrowed_tcx,
                    &self.dataclass_field_specifiers,
                )
                .is_err()
            {
                return None;
            }

            // Ensure the inferred return type is assignable to the (narrowed) declared type.
            //
            // TODO: Checking assignability against the full declared type could help avoid
            // cases where the constraint solver is not smart enough to solve complex unions.
            // We should see revisit this after the new constraint solver is implemented.
            if !speculated_bindings
                .return_type(db)
                .is_assignable_to(db, narrowed_ty)
            {
                return None;
            }

            // Successfully narrowed to an element of the union.
            //
            // If necessary, infer the argument types again with diagnostics enabled.
            if !was_in_multi_inference {
                self.context.set_multi_inference(was_in_multi_inference);

                self.infer_all_argument_types(
                    ast_arguments.clone(),
                    argument_types,
                    infer_argument_ty,
                    bindings,
                    narrowed_tcx,
                    MultiInferenceState::Intersect,
                );
            }

            Some(bindings.check_types_impl(
                db,
                &constraints,
                argument_types,
                narrowed_tcx,
                &self.dataclass_field_specifiers,
            ))
        };

        // Prefer the declared type of generic classes.
        for narrowed_ty in narrow_targets
            .iter()
            .filter(|ty| ty.class_specialization(db).is_some())
        {
            if let Some(result) = try_narrow(*narrowed_ty) {
                return result;
            }
        }

        // Try the remaining elements of the union.
        //
        // TODO: We could also attempt an inference without type context, but this
        // leads to similar performance issues.
        for narrowed_ty in narrow_targets
            .iter()
            .filter(|ty| ty.class_specialization(db).is_none())
        {
            if let Some(result) = try_narrow(*narrowed_ty) {
                return result;
            }
        }

        // Re-enable diagnostics, and infer against the entire union as a fallback.
        self.context.set_multi_inference(was_in_multi_inference);

        self.infer_all_argument_types(
            ast_arguments,
            argument_types,
            infer_argument_ty,
            bindings,
            call_expression_tcx,
            MultiInferenceState::Intersect,
        );

        bindings.check_types_impl(
            db,
            &constraints,
            argument_types,
            call_expression_tcx,
            &self.dataclass_field_specifiers,
        )
    }

    /// Infer the argument types for all bindings.
    ///
    /// Note that this method may infer the type of a given argument expression multiple times with
    /// distinct type context. The provided `MultiInferenceState` can be used to dictate multi-inference
    /// behavior.
    fn infer_all_argument_types(
        &mut self,
        ast_arguments: ArgumentsIter<'_>,
        arguments_types: &mut CallArguments<'_, 'db>,
        infer_argument_ty: &mut dyn FnMut(&mut Self, ArgExpr<'db, '_>) -> Type<'db>,
        bindings: &Bindings<'db>,
        call_expression_tcx: TypeContext<'db>,
        multi_inference_state: MultiInferenceState,
    ) {
        debug_assert_eq!(arguments_types.len(), bindings.argument_forms().len());

        let db = self.db();
        let constraints = ConstraintSetBuilder::new();
        let iter = itertools::izip!(
            0..,
            arguments_types.iter_mut(),
            bindings.argument_forms().iter().copied(),
            ast_arguments
        );

        let overloads_with_binding = bindings
            .iter_flat()
            .filter_map(|binding| {
                match binding.matching_overload_index() {
                    MatchingOverloadIndex::Single(_) | MatchingOverloadIndex::Multiple(_) => {
                        let overloads = binding
                            .matching_overloads()
                            .map(move |(_, overload)| (overload, binding));

                        Some(Either::Right(overloads))
                    }

                    // If there is a single overload that does not match, we still infer the argument
                    // types for better diagnostics.
                    MatchingOverloadIndex::None => match binding.overloads() {
                        [overload] => Some(Either::Left(std::iter::once((overload, binding)))),
                        _ => None,
                    },
                }
            })
            .flatten()
            .collect::<Vec<_>>();

        let old_multi_inference_state = self.set_multi_inference_state(multi_inference_state);

        for (argument_index, (_, argument_type), argument_form, ast_argument) in iter {
            let ast_argument = match ast_argument {
                // Splatted arguments are inferred before parameter matching to
                // determine their length.
                //
                // TODO: Re-infer splatted arguments with their type context.
                ast::ArgOrKeyword::Arg(ast::Expr::Starred(_))
                | ast::ArgOrKeyword::Keyword(ast::Keyword { arg: None, .. }) => continue,

                ast::ArgOrKeyword::Arg(arg) => arg,
                ast::ArgOrKeyword::Keyword(ast::Keyword { value, .. }) => value,
            };

            // Type-form arguments are inferred without type context, so we can infer the argument type directly.
            if let Some(ParameterForm::Type) = argument_form {
                *argument_type = Some(self.infer_type_expression(ast_argument));
                continue;
            }

            // Retrieve the parameter type for the current argument in a given overload and its binding.
            let parameter_type = |overload: &Binding<'db>, binding: &CallableBinding<'db>| {
                let argument_index = if binding.bound_type.is_some() {
                    argument_index + 1
                } else {
                    argument_index
                };

                let argument_matches = &overload.argument_matches()[argument_index];
                let [parameter_index] = argument_matches.parameters.as_slice() else {
                    return None;
                };

                let mut parameter_type =
                    overload.signature.parameters()[*parameter_index].annotated_type();

                // If the parameter is a single type variable with an upper bound, e.g., `typing.Self`,
                // use the upper bound as type context.
                if let Type::TypeVar(typevar) = parameter_type
                    && let Some(TypeVarBoundOrConstraints::UpperBound(bound)) =
                        typevar.typevar(db).bound_or_constraints(db)
                {
                    return Some(bound);
                }

                // If this is a generic call, attempt to specialize the parameter type using the
                // declared type context, if provided.
                if let Some(generic_context) = overload.signature.generic_context {
                    let mut builder =
                        SpecializationBuilder::new(db, generic_context.inferable_typevars(db));

                    if let Some(declared_return_ty) = call_expression_tcx.annotation {
                        let _ = builder.infer_reverse(
                            &constraints,
                            declared_return_ty,
                            overload
                                .constructor_instance_type
                                .unwrap_or(overload.signature.return_ty),
                        );
                    }

                    let specialization = builder
                        // Default specialize any type variables to a marker type, which will be ignored
                        // during argument inference, allowing the concrete subset of the parameter
                        // type to still affect argument inference.
                        //
                        // TODO: Eventually, we want to "tie together" the typevars of the two calls
                        // so that we can infer their specializations at the same time — or at least, for
                        // the specialization of one to influence the specialization of the other. It's
                        // not yet clear how we're going to do that. (We might have to start inferring
                        // constraint sets for each expression, instead of simple types?)
                        .with_default(generic_context, |_| {
                            Type::Dynamic(DynamicType::UnspecializedTypeVar)
                        })
                        .build(generic_context);

                    parameter_type = parameter_type.apply_specialization(db, specialization);
                }

                Some(parameter_type)
            };

            // If there is only a single binding and overload, we can infer the argument directly with
            // the unique parameter type annotation.
            if let Ok((overload, binding)) = overloads_with_binding.iter().exactly_one() {
                let tcx = TypeContext::new(parameter_type(overload, binding));
                *argument_type = Some(infer_argument_ty(self, (argument_index, ast_argument, tcx)));
            } else {
                // We perform inference once without any type context, emitting any diagnostics that are unrelated
                // to bidirectional type inference.
                *argument_type = Some(infer_argument_ty(
                    self,
                    (argument_index, ast_argument, TypeContext::default()),
                ));

                // We then silence any diagnostics emitted during multi-inference, as the type context is only
                // used as a hint to infer a more assignable argument type, and should not lead to diagnostics
                // for non-matching overloads.
                let was_in_multi_inference = self.context.set_multi_inference(true);

                // Infer the type of each argument once with each distinct parameter type as type context.
                let parameter_types = overloads_with_binding
                    .iter()
                    .filter_map(|(overload, binding)| parameter_type(overload, binding));

                let mut seen = FxHashSet::default();

                for parameter_type in parameter_types {
                    if !seen.insert(parameter_type) {
                        continue;
                    }

                    let tcx = TypeContext::new(Some(parameter_type));
                    let inferred_ty = infer_argument_ty(self, (argument_index, ast_argument, tcx));

                    // Ensure the inferred type is assignable to the declared type.
                    //
                    // If not, we want to avoid storing the "failed" inference attempt.
                    if !inferred_ty.is_assignable_to(db, parameter_type) {
                        continue;
                    }

                    // Each type is a valid independent inference of the given argument, and we may require different
                    // permutations of argument types to correctly perform argument expansion during overload evaluation,
                    // so we take the intersection of all the types we inferred for each argument.
                    //
                    // TODO: intersecting the inferred argument types is correct for unions of
                    // callables, since the argument must satisfy each callable, but it's not clear
                    // that it's correct for an intersection of callables, or for a case where
                    // different overloads provide different type context; unioning may be more
                    // correct in those cases.
                    *argument_type = argument_type
                        .map(|current| {
                            IntersectionType::from_two_elements(db, inferred_ty, current)
                        })
                        .or(Some(inferred_ty));
                }

                // Re-enable diagnostics.
                self.context.set_multi_inference(was_in_multi_inference);
            }
        }

        self.set_multi_inference_state(old_multi_inference_state);
    }

    fn infer_argument_type(
        &mut self,
        ast_argument: &ast::Expr,
        form: Option<ParameterForm>,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        match form {
            None | Some(ParameterForm::Value) => self.infer_expression(ast_argument, tcx),
            Some(ParameterForm::Type) => self.infer_type_expression(ast_argument),
        }
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
            "Calling `self.infer_expression` on a standalone-expression is not allowed because it can lead to double-inference. Use `self.infer_standalone_expression` instead."
        );

        self.infer_expression_impl(expression, tcx)
    }

    fn infer_expression_with_state(
        &mut self,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
        state: DeferredExpressionState,
    ) -> Type<'db> {
        let previous_deferred_state = std::mem::replace(&mut self.deferred_state, state);
        let ty = self.infer_expression(expression, tcx);
        self.deferred_state = previous_deferred_state;
        ty
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
        if self.inner_expression_inference_state.is_get() {
            return self.expression_type(expression);
        }
        let mut ty = match expression {
            ast::Expr::NoneLiteral(ast::ExprNoneLiteral {
                range: _,
                node_index: _,
            }) => Type::none(self.db()),
            ast::Expr::NumberLiteral(literal) => self.infer_number_literal_expression(literal),
            ast::Expr::BooleanLiteral(literal) => self.infer_boolean_literal_expression(literal),
            ast::Expr::StringLiteral(literal) => self.infer_string_literal_expression(literal, tcx),
            ast::Expr::BytesLiteral(bytes_literal) => {
                self.infer_bytes_literal_expression(bytes_literal)
            }
            ast::Expr::FString(fstring) => self.infer_fstring_expression(fstring),
            ast::Expr::TString(tstring) => self.infer_tstring_expression(tstring),
            ast::Expr::EllipsisLiteral(literal) => self.infer_ellipsis_literal_expression(literal),
            ast::Expr::Tuple(tuple) => self.infer_tuple_expression(tuple, tcx),
            ast::Expr::List(list) => self.infer_list_expression(list, tcx),
            ast::Expr::Set(set) => self.infer_set_expression(set, tcx),
            ast::Expr::Dict(dict) => self.infer_dict_expression(dict, tcx),
            ast::Expr::Generator(generator) => self.infer_generator_expression(generator),
            ast::Expr::ListComp(listcomp) => {
                self.infer_list_comprehension_expression(listcomp, tcx)
            }
            ast::Expr::DictComp(dictcomp) => {
                self.infer_dict_comprehension_expression(dictcomp, tcx)
            }
            ast::Expr::SetComp(setcomp) => self.infer_set_comprehension_expression(setcomp, tcx),
            ast::Expr::Name(name) => self.infer_name_expression(name),
            ast::Expr::Attribute(attribute) => self.infer_attribute_expression(attribute),
            ast::Expr::UnaryOp(unary_op) => self.infer_unary_expression(unary_op),
            ast::Expr::BinOp(binary) => self.infer_binary_expression(binary, tcx),
            ast::Expr::BoolOp(bool_op) => self.infer_boolean_expression(bool_op),
            ast::Expr::Compare(compare) => self.infer_compare_expression(compare),
            ast::Expr::Subscript(subscript) => self.infer_subscript_expression(subscript),
            ast::Expr::Slice(slice) => self.infer_slice_expression(slice),
            ast::Expr::If(if_expression) => self.infer_if_expression(if_expression, tcx),
            ast::Expr::Lambda(lambda_expression) => self.infer_lambda_expression(lambda_expression),
            ast::Expr::Call(call_expression) => self.infer_call_expression(call_expression, tcx),
            ast::Expr::Starred(starred) => self.infer_starred_expression(starred, tcx),
            ast::Expr::Yield(yield_expression) => self.infer_yield_expression(yield_expression),
            ast::Expr::YieldFrom(yield_from) => self.infer_yield_from_expression(yield_from),
            ast::Expr::Await(await_expression) => self.infer_await_expression(await_expression),
            ast::Expr::Named(named) => {
                // Definitions must be unique, so we bypass multi-inference for named expressions.
                if !self.multi_inference_state.is_panic()
                    && let Some(ty) = self.expressions.get(&expression.into())
                {
                    return *ty;
                }

                self.infer_named_expression(named)
            }
            ast::Expr::IpyEscapeCommand(_) => {
                todo_type!("Ipy escape command support")
            }
        };

        // Avoid promoting explicitly annotated literal values.
        if let Type::LiteralValue(literal) = ty
            && let Some(tcx) = tcx.annotation
            && let Type::Union(_) | Type::LiteralValue(_) = tcx
                .resolve_type_alias(self.db())
                .filter_union(self.db(), |ty| ty.as_literal_value().is_some())
            && ty.is_assignable_to(self.db(), tcx)
        {
            ty = Type::LiteralValue(literal.to_unpromotable());
        }

        self.store_expression_type_impl(expression, ty, tcx);

        ty
    }

    #[track_caller]
    fn store_expression_type(&mut self, expression: &ast::Expr, ty: Type<'db>) {
        self.store_expression_type_impl(expression, ty, TypeContext::default());
    }

    #[track_caller]
    fn store_expression_type_impl(
        &mut self,
        expression: &ast::Expr,
        ty: Type<'db>,
        tcx: TypeContext<'db>,
    ) {
        if self.inner_expression_inference_state.is_get() {
            // If `inner_expression_inference_state` is `Get`, the expression type has already been stored.
            return;
        }

        let db = self.db();

        match self.multi_inference_state {
            MultiInferenceState::Ignore => {}

            MultiInferenceState::Panic => {
                let previous = self.expressions.insert(expression.into(), ty);
                assert_eq!(previous, None);
            }

            MultiInferenceState::Intersect => {
                self.expressions
                    .entry(expression.into())
                    .and_modify(|current| {
                        // Avoid storing "failed" multi-inference attempts, which can lead to
                        // unnecessary union simplification overhead.
                        if tcx
                            .annotation
                            .is_none_or(|tcx| ty.is_assignable_to(db, tcx))
                        {
                            *current = IntersectionType::from_two_elements(db, *current, ty);
                        }
                    })
                    .or_insert(ty);
            }
        }
    }

    fn infer_number_literal_expression(&self, literal: &ast::ExprNumberLiteral) -> Type<'db> {
        let ast::ExprNumberLiteral {
            range: _,
            node_index: _,
            value,
        } = literal;
        let db = self.db();

        match value {
            ast::Number::Int(n) => n
                .as_i64()
                .map(Type::int_literal)
                .unwrap_or_else(|| KnownClass::Int.to_instance(db)),
            ast::Number::Float(_) => KnownClass::Float.to_instance(db),
            ast::Number::Complex { .. } => KnownClass::Complex.to_instance(db),
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
        if tcx.is_typealias() {
            let aliased_type = self.infer_string_type_expression(literal);
            return Type::KnownInstance(KnownInstanceType::LiteralStringAlias(InternedType::new(
                self.db(),
                aliased_type,
            )));
        }
        if literal.value.len() <= Self::MAX_STRING_LITERAL_SIZE {
            Type::string_literal(self.db(), literal.value.to_str())
        } else {
            Type::literal_string()
        }
    }

    fn infer_bytes_literal_expression(&mut self, literal: &ast::ExprBytesLiteral) -> Type<'db> {
        // TODO: ignoring r/R prefixes for now, should normalize bytes values
        let bytes: Vec<u8> = literal.value.bytes().collect();
        Type::bytes_literal(self.db(), &bytes)
    }

    fn infer_fstring_expression(&mut self, fstring: &ast::ExprFString) -> Type<'db> {
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
                ast::FStringPart::Literal(literal) => {
                    collector.push_str(&literal.value);
                }
                ast::FStringPart::FString(fstring) => {
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
                                    let str_ty = ty.str(self.db());
                                    if let Some(literal) = str_ty.as_string_literal() {
                                        collector.push_str(literal.value(self.db()));
                                    } else if str_ty
                                        .is_subtype_of(self.db(), Type::literal_string())
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
        collector.string_type(self.db())
    }

    fn infer_tstring_expression(&mut self, tstring: &ast::ExprTString) -> Type<'db> {
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
        KnownClass::Template.to_instance(self.db())
    }

    fn infer_ellipsis_literal_expression(
        &mut self,
        _literal: &ast::ExprEllipsisLiteral,
    ) -> Type<'db> {
        KnownClass::EllipsisType.to_instance(self.db())
    }

    fn infer_tuple_expression(
        &mut self,
        tuple: &ast::ExprTuple,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        /// If a tuple literal has more elements than this constant,
        /// we promote `Literal` types when inferring the elements of the tuple.
        /// This provides a huge speedup on files that have very large unannotated tuple literals.
        const MAX_TUPLE_LENGTH_FOR_UNANNOTATED_LITERAL_INFERENCE: usize = 64;

        let ast::ExprTuple {
            range: _,
            node_index: _,
            elts,
            ctx: _,
            parenthesized: _,
        } = tuple;

        // Remove any union elements of the annotation that are unrelated to the tuple type.
        let tcx = tcx.map(|annotation| {
            let inferable = KnownClass::Tuple
                .try_to_class_literal(self.db())
                .and_then(|class| class.generic_context(self.db()))
                .map(|generic_context| generic_context.inferable_typevars(self.db()))
                .unwrap_or(InferableTypeVars::None);
            annotation.filter_disjoint_elements(
                self.db(),
                Type::homogeneous_tuple(self.db(), Type::unknown()),
                inferable,
            )
        });

        let mut is_homogeneous_tuple_annotation = false;

        let annotated_tuple = tcx
            .known_specialization(self.db(), KnownClass::Tuple)
            .and_then(|specialization| {
                let spec = specialization
                    .tuple(self.db())
                    .expect("the specialization of `KnownClass::Tuple` must have a tuple spec");

                if let Tuple::Variable(tuple) = spec
                    && tuple.prefix_elements().is_empty()
                    && tuple.suffix_elements().is_empty()
                {
                    is_homogeneous_tuple_annotation = true;
                }

                spec.resize(self.db(), TupleLength::Fixed(elts.len())).ok()
            });

        // TODO: this is a simplification for now.
        //
        // It might be possible to use the type context where the annotation is not a pure-homogeneous
        // tuple and the actual tuple has starred elements in it. It seems complex to reason about,
        // though, and unlikely to come up much.
        let can_use_type_context =
            is_homogeneous_tuple_annotation || elts.iter().all(|elt| !elt.is_starred_expr());

        let mut annotated_elt_tys = annotated_tuple
            .as_ref()
            .map(Tuple::all_elements)
            .unwrap_or_default()
            .iter()
            .copied();

        let db = self.db();

        let mut infer_element = |elt: &ast::Expr| {
            let annotated_elt_ty = annotated_elt_tys.by_ref().next();
            let ctx = if can_use_type_context {
                let expected = if elt.is_starred_expr() {
                    let expected_element = annotated_elt_ty.unwrap_or_else(Type::object);
                    Some(KnownClass::Iterable.to_specialized_instance(db, &[expected_element]))
                } else {
                    annotated_elt_ty
                };
                TypeContext::new(expected)
            } else {
                TypeContext::default()
            };
            if tuple.len() > MAX_TUPLE_LENGTH_FOR_UNANNOTATED_LITERAL_INFERENCE {
                // Promote literals for very large unannotated tuples,
                // to avoid pathological performance issues
                self.infer_expression(elt, ctx).promote(db)
            } else {
                self.infer_expression(elt, ctx)
            }
        };

        let mut builder = TupleSpecBuilder::with_capacity(elts.len());

        for element in elts {
            if let ast::Expr::Starred(starred) = element {
                let element_type = infer_element(element);
                // Fine to use `iterate` rather than `try_iterate` here:
                // errors from iterating over something not iterable will have been
                // emitted in the `infer_element` call above.
                let mut spec = element_type.iterate(db).into_owned();

                let known_length = match &*starred.value {
                    ast::Expr::List(ast::ExprList { elts, .. })
                    | ast::Expr::Set(ast::ExprSet { elts, .. }) => elts
                        .iter()
                        .all(|elt| !elt.is_starred_expr())
                        .then_some(elts.len()),
                    ast::Expr::Dict(ast::ExprDict { items, .. }) => items
                        .iter()
                        .all(|item| item.key.is_some())
                        .then_some(items.len()),
                    _ => None,
                };

                if let Some(known_length) = known_length {
                    spec = spec
                        .resize(db, TupleLength::Fixed(known_length))
                        .unwrap_or(spec);
                }

                builder = builder.concat(db, &spec);
            } else {
                builder.push(infer_element(element));
            }
        }

        Type::tuple(TupleType::new(db, &builder.build()))
    }

    fn infer_list_expression(&mut self, list: &ast::ExprList, tcx: TypeContext<'db>) -> Type<'db> {
        let ast::ExprList {
            range: _,
            node_index: _,
            elts,
            ctx: _,
        } = list;

        let mut elts = elts.iter().map(|elt| [Some(elt)]);
        let mut infer_elt_ty =
            |builder: &mut Self, (_, elt, tcx)| builder.infer_expression(elt, tcx);

        self.infer_collection_literal(KnownClass::List, &mut elts, &mut infer_elt_ty, tcx)
            .unwrap_or_else(|| {
                KnownClass::List.to_specialized_instance(self.db(), &[Type::unknown()])
            })
    }

    fn infer_set_expression(&mut self, set: &ast::ExprSet, tcx: TypeContext<'db>) -> Type<'db> {
        let ast::ExprSet {
            range: _,
            node_index: _,
            elts,
        } = set;

        let mut elts = elts.iter().map(|elt| [Some(elt)]);
        let mut infer_elt_ty =
            |builder: &mut Self, (_, elt, tcx)| builder.infer_expression(elt, tcx);

        self.infer_collection_literal(KnownClass::Set, &mut elts, &mut infer_elt_ty, tcx)
            .unwrap_or_else(|| {
                KnownClass::Set.to_specialized_instance(self.db(), &[Type::unknown()])
            })
    }

    fn infer_dict_expression(&mut self, dict: &ast::ExprDict, tcx: TypeContext<'db>) -> Type<'db> {
        let ast::ExprDict {
            range: _,
            node_index: _,
            items,
        } = dict;

        let mut item_types = FxHashMap::default();

        // Validate `TypedDict` dictionary literal assignments.
        if let Some(tcx) = tcx.annotation {
            let tcx = tcx.filter_union(self.db(), Type::is_typed_dict);

            if let Some(typed_dict) = tcx.as_typed_dict() {
                // If there is a single typed dict annotation, infer against it directly.
                if let Some(ty) =
                    self.infer_typed_dict_expression(dict, typed_dict, &mut item_types)
                {
                    return ty;
                }
            } else if let Type::Union(tcx) = tcx {
                // Otherwise, disable diagnostics as we attempt to narrow to specific elements of the union.
                let old_multi_inference = self.context.set_multi_inference(true);
                let old_multi_inference_state =
                    self.set_multi_inference_state(MultiInferenceState::Ignore);

                let mut narrowed_typed_dicts = Vec::new();
                for element in tcx.elements(self.db()) {
                    let typed_dict = element
                        .as_typed_dict()
                        .expect("filtered out non-typed-dict types above");

                    if self
                        .infer_typed_dict_expression(dict, typed_dict, &mut item_types)
                        .is_some()
                    {
                        narrowed_typed_dicts.push(typed_dict);
                    }

                    item_types.clear();
                }

                if !narrowed_typed_dicts.is_empty() {
                    // Now that we know which typed dict annotations are valid, re-infer with diagnostics enabled,
                    self.context.set_multi_inference(old_multi_inference);

                    // We may have to infer the same expression multiple times with distinct type context,
                    // so we take the intersection of all valid inferences for a given expression.
                    self.set_multi_inference_state(MultiInferenceState::Intersect);

                    let mut narrowed_tys = Vec::new();
                    for typed_dict in narrowed_typed_dicts {
                        let mut item_types = FxHashMap::default();

                        let ty = self
                            .infer_typed_dict_expression(dict, typed_dict, &mut item_types)
                            .expect("ensured the typed dict is valid above");

                        narrowed_tys.push(ty);
                    }

                    self.set_multi_inference_state(old_multi_inference_state);
                    return UnionType::from_elements(self.db(), narrowed_tys);
                }

                self.context.set_multi_inference(old_multi_inference);
                self.set_multi_inference_state(old_multi_inference_state);
            }
        }

        // Avoid false positives for the functional `TypedDict` form, which is currently
        // unsupported.
        if let Some(Type::Dynamic(DynamicType::TodoFunctionalTypedDict)) = tcx.annotation {
            return KnownClass::Dict
                .to_specialized_instance(self.db(), &[Type::unknown(), Type::unknown()]);
        }

        let mut items = items
            .iter()
            .map(|item| [item.key.as_ref(), Some(&item.value)]);

        // Avoid inferring the items multiple times if we already attempted to infer the
        // dictionary literal as a `TypedDict`. This also allows us to infer using the
        // type context of the expected `TypedDict` field.
        let mut infer_elt_ty = |builder: &mut Self, (_, elt, tcx): ArgExpr<'db, '_>| {
            item_types
                .get(&elt.node_index().load())
                .copied()
                .unwrap_or_else(|| builder.infer_expression(elt, tcx))
        };

        self.infer_collection_literal(KnownClass::Dict, &mut items, &mut infer_elt_ty, tcx)
            .unwrap_or_else(|| {
                KnownClass::Dict
                    .to_specialized_instance(self.db(), &[Type::unknown(), Type::unknown()])
            })
    }

    fn infer_typed_dict_expression(
        &mut self,
        dict: &ast::ExprDict,
        typed_dict: TypedDictType<'db>,
        item_types: &mut FxHashMap<NodeIndex, Type<'db>>,
    ) -> Option<Type<'db>> {
        let ast::ExprDict {
            range: _,
            node_index: _,
            items,
        } = dict;

        let typed_dict_items = typed_dict.items(self.db());

        for item in items {
            let key_ty = self.infer_optional_expression(item.key.as_ref(), TypeContext::default());
            if let Some((key, key_ty)) = item.key.as_ref().zip(key_ty) {
                item_types.insert(key.node_index().load(), key_ty);
            }

            let value_ty = if let Some(key_ty) = key_ty
                && let Some(key) = key_ty.as_string_literal()
                && let Some(field) = typed_dict_items.get(key.value(self.db()))
            {
                self.infer_expression(&item.value, TypeContext::new(Some(field.declared_ty)))
            } else {
                self.infer_expression(&item.value, TypeContext::default())
            };

            item_types.insert(item.value.node_index().load(), value_ty);
        }

        validate_typed_dict_dict_literal(&self.context, typed_dict, dict, dict.into(), |expr| {
            item_types
                .get(&expr.node_index().load())
                .copied()
                .unwrap_or(Type::unknown())
        })
        .ok()
        .map(|_| Type::TypedDict(typed_dict))
    }

    // Infer the type of a collection literal expression.
    fn infer_collection_literal<'expr, const N: usize>(
        &mut self,
        collection_class: KnownClass,
        elts: &mut dyn Iterator<Item = [Option<&'expr ast::Expr>; N]>,
        infer_elt_expression: &mut dyn FnMut(&mut Self, ArgExpr<'db, 'expr>) -> Type<'db>,
        tcx: TypeContext<'db>,
    ) -> Option<Type<'db>> {
        // Extract the type variable `T` from `list[T]` in typeshed.
        let elt_tys = |collection_class: KnownClass| {
            let collection_alias = collection_class
                .try_to_class_literal(self.db())?
                .identity_specialization(self.db())
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
            for (i, elt) in elts.flatten().flatten().enumerate() {
                infer_elt_expression(self, (i, elt, TypeContext::default()));
            }

            return None;
        };

        let constraints = ConstraintSetBuilder::new();
        let inferable = generic_context.inferable_typevars(self.db());

        // Remove any union elements of that are unrelated to the collection type.
        //
        // For example, we only want the `list[int]` from `annotation: list[int] | None` if
        // `collection_ty` is `list`.
        let tcx = tcx.map(|annotation| {
            let collection_ty = collection_class.to_instance(self.db());
            annotation.filter_disjoint_elements(self.db(), collection_ty, inferable)
        });

        // Collect type constraints from the declared element types.
        let (elt_tcx_constraints, elt_tcx_variance) = {
            let mut builder = SpecializationBuilder::new(self.db(), inferable);

            // For a given type variable, we keep track of the variance of any assignments to
            // that type variable in the type context.
            let mut elt_tcx_variance: FxHashMap<BoundTypeVarIdentity<'_>, TypeVarVariance> =
                FxHashMap::default();

            if let Some(tcx) = tcx.annotation
                // If there are multiple potential type contexts, we fallback to `Unknown`.
                // TODO: We could perform multi-inference here.
                && tcx
                    .filter_union(self.db(), |ty| ty.class_specialization(self.db()).is_some())
                    .class_specialization(self.db())
                    .is_some()
            {
                let collection_instance =
                    Type::instance(self.db(), ClassType::Generic(collection_alias));

                builder
                    .infer_reverse_map(
                        &constraints,
                        tcx,
                        collection_instance,
                        |(typevar, variance, inferred_ty)| {
                            // Avoid inferring a preferred type based on partially specialized type context
                            // from an outer generic call. If the type context is a union, we try to keep
                            // any concrete elements.
                            let inferred_ty = inferred_ty.filter_union(self.db(), |ty| {
                                !ty.has_unspecialized_type_var(self.db())
                            });
                            if inferred_ty.has_unspecialized_type_var(self.db()) {
                                return None;
                            }

                            elt_tcx_variance
                                .entry(typevar)
                                .and_modify(|current| *current = current.join(variance))
                                .or_insert(variance);

                            Some(inferred_ty)
                        },
                    )
                    .ok()?;
            }

            (builder.into_type_mappings(), elt_tcx_variance)
        };

        // Create a set of constraints to infer a precise type for `T`.
        let mut builder = SpecializationBuilder::new(self.db(), inferable);

        for elt_ty in elt_tys.clone() {
            let elt_ty_identity = elt_ty.identity(self.db());
            let elt_tcx = elt_tcx_constraints
                // The annotated type acts as a constraint for `T`.
                //
                // Note that we infer the annotated type _before_ the elements, to more closely match
                // the order of any unions as written in the type annotation.
                .get(&elt_ty_identity)
                .copied();

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
                builder
                    .infer(&constraints, Type::TypeVar(elt_ty), elt_tcx)
                    .ok()?;
            }
        }

        for elts in elts {
            // An unpacking expression for a dictionary.
            if let &[None, Some(value_expr)] = elts.as_slice() {
                let unpack_ty = infer_elt_expression(self, (1, value_expr, tcx));

                let Some((unpacked_key_ty, unpacked_value_ty)) =
                    unpack_ty.unpack_keys_and_items(self.db())
                else {
                    if let Some(builder) =
                        self.context.report_lint(&INVALID_ARGUMENT_TYPE, value_expr)
                    {
                        let mut diag = builder
                            .into_diagnostic("Argument expression after ** must be a mapping type");

                        diag.set_primary_message(format_args!(
                            "Found `{}`",
                            unpack_ty.display(self.db())
                        ));
                    }

                    continue;
                };

                let mut elt_tys = elt_tys.clone();
                if let Some((key_ty, value_ty)) = elt_tys.next_tuple() {
                    builder
                        .infer(&constraints, Type::TypeVar(key_ty), unpacked_key_ty)
                        .ok()?;

                    builder
                        .infer(&constraints, Type::TypeVar(value_ty), unpacked_value_ty)
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
                            Type::homogeneous_tuple(self.db(), tcx)
                        } else {
                            tcx
                        }
                    });

                let inferred_elt_ty =
                    infer_elt_expression(self, (i, elt, TypeContext::new(elt_tcx)));

                // Simplify the inference based on a non-covariant declared type.
                if let Some(elt_tcx) =
                    elt_tcx.filter(|_| !elt_tcx_variance[&elt_ty_identity].is_covariant())
                    && inferred_elt_ty.is_assignable_to(self.db(), elt_tcx)
                {
                    continue;
                }

                // Promote types to avoid excessively large unions for large nested list literals,
                // which the constraint solver struggles with.
                let inferred_elt_ty = inferred_elt_ty.promote(self.db());

                builder
                    .infer(
                        &constraints,
                        Type::TypeVar(elt_ty),
                        if elt.is_starred_expr() {
                            inferred_elt_ty
                                .iterate(self.db())
                                .homogeneous_element_type(self.db())
                        } else {
                            inferred_elt_ty
                        },
                    )
                    .ok()?;
            }
        }

        // Promote singleton types to `T | Unknown` in inferred type parameters,
        // so that e.g. `[None]` is inferred as `list[None | Unknown]`.
        if elt_tcx_constraints.is_empty() {
            builder.map_types(|ty| ty.promote_singletons(self.db()));
        }

        let class_type = collection_alias
            .origin(self.db())
            .apply_specialization(self.db(), |_| builder.build(generic_context));

        Type::from(class_type).to_instance(self.db())
    }

    /// Infer the type of the `iter` expression of the first comprehension.
    /// Returns the evaluation mode (async or sync) of the comprehension.
    fn infer_first_comprehension_iter(
        &mut self,
        comprehensions: &[ast::Comprehension],
    ) -> EvaluationMode {
        let mut comprehensions_iter = comprehensions.iter();
        let Some(first_comprehension) = comprehensions_iter.next() else {
            unreachable!("Comprehension must contain at least one generator");
        };
        self.infer_maybe_standalone_expression(&first_comprehension.iter, TypeContext::default());

        if first_comprehension.is_async {
            EvaluationMode::Async
        } else {
            EvaluationMode::Sync
        }
    }

    fn infer_generator_expression(&mut self, generator: &ast::ExprGenerator) -> Type<'db> {
        let ast::ExprGenerator {
            range: _,
            node_index: _,
            elt,
            generators,
            parenthesized: _,
        } = generator;

        let evaluation_mode = self.infer_first_comprehension_iter(generators);

        let Some(scope_id) = self
            .index
            .try_node_scope(NodeWithScopeRef::GeneratorExpression(generator))
        else {
            return Type::unknown();
        };
        let scope = scope_id.to_scope_id(self.db(), self.file());
        let inference = infer_scope_types(self.db(), scope, TypeContext::default());
        let yield_type = inference.expression_type(elt.as_ref());

        if evaluation_mode.is_async() {
            KnownClass::AsyncGeneratorType
                .to_specialized_instance(self.db(), &[yield_type, Type::none(self.db())])
        } else {
            KnownClass::GeneratorType.to_specialized_instance(
                self.db(),
                &[yield_type, Type::none(self.db()), Type::none(self.db())],
            )
        }
    }

    /// Return a specialization of the collection class (list, dict, set) based on the type context and the inferred
    /// element / key-value types from the comprehension expression.
    fn infer_comprehension_specialization<const N: usize>(
        &mut self,
        collection_class: KnownClass,
        elements: &[Option<&ast::Expr>; N],
        inference: &ScopeInference<'db>,
        tcx: TypeContext<'db>,
    ) -> Option<Type<'db>> {
        let mut elements = [elements].into_iter().copied();
        let mut infer_element_ty =
            |_builder: &mut Self, (_, elt, _)| inference.expression_type(elt);

        self.infer_collection_literal(collection_class, &mut elements, &mut infer_element_ty, tcx)
    }

    fn infer_list_comprehension_expression(
        &mut self,
        listcomp: &ast::ExprListComp,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
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
        let scope = scope_id.to_scope_id(self.db(), self.file());
        let inference = infer_scope_types(self.db(), scope, tcx);
        self.extend_scope(inference);

        self.infer_comprehension_specialization(KnownClass::List, &[Some(elt)], inference, tcx)
            .unwrap_or_else(|| {
                KnownClass::List.to_specialized_instance(self.db(), &[Type::unknown()])
            })
    }

    fn infer_set_comprehension_expression(
        &mut self,
        setcomp: &ast::ExprSetComp,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
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
        let scope = scope_id.to_scope_id(self.db(), self.file());
        let inference = infer_scope_types(self.db(), scope, tcx);
        self.extend_scope(inference);

        self.infer_comprehension_specialization(KnownClass::Set, &[Some(elt)], inference, tcx)
            .unwrap_or_else(|| {
                KnownClass::Set.to_specialized_instance(self.db(), &[Type::unknown()])
            })
    }

    fn infer_dict_comprehension_expression(
        &mut self,
        dictcomp: &ast::ExprDictComp,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
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
        let scope = scope_id.to_scope_id(self.db(), self.file());
        let inference = infer_scope_types(self.db(), scope, tcx);
        self.extend_scope(inference);

        self.infer_comprehension_specialization(
            KnownClass::Dict,
            &[Some(key), Some(value)],
            inference,
            tcx,
        )
        .unwrap_or_else(|| {
            KnownClass::Dict.to_specialized_instance(self.db(), &[Type::unknown(), Type::unknown()])
        })
    }

    fn infer_generator_expression_scope(&mut self, generator: &ast::ExprGenerator) {
        let ast::ExprGenerator {
            range: _,
            node_index: _,
            elt,
            generators,
            parenthesized: _,
        } = generator;

        self.infer_expression(elt, TypeContext::default());
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
        let mut elts = [[Some(elt.as_ref())]].into_iter();
        let mut infer_elt_ty =
            |builder: &mut Self, (_, elt, tcx)| builder.infer_expression(elt, tcx);
        self.infer_collection_literal(KnownClass::List, &mut elts, &mut infer_elt_ty, tcx);

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
        let mut elts = [[Some(elt.as_ref())]].into_iter();
        let mut infer_elt_ty =
            |builder: &mut Self, (_, elt, tcx)| builder.infer_expression(elt, tcx);
        self.infer_collection_literal(KnownClass::Set, &mut elts, &mut infer_elt_ty, tcx);

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

        // Infer the key and value types using the outer type context.
        let mut elts = [[Some(key.as_ref()), Some(value.as_ref())]].into_iter();
        let mut infer_elt_ty =
            |builder: &mut Self, (_, elt, tcx)| builder.infer_expression(elt, tcx);
        self.infer_collection_literal(KnownClass::Dict, &mut elts, &mut infer_elt_ty, tcx);

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
                infer_same_file_expression_type(
                    builder.db(),
                    builder.index.expression(iter),
                    tcx,
                    builder.module(),
                )
            } else {
                builder.infer_maybe_standalone_expression(iter, tcx)
            }
            .iterate(builder.db())
            .homogeneous_element_type(builder.db())
        });

        for expr in ifs {
            self.infer_maybe_standalone_expression(expr, TypeContext::default());
        }
    }

    fn infer_comprehension_definition(
        &mut self,
        comprehension: &ComprehensionDefinitionKind<'db>,
        definition: Definition<'db>,
    ) {
        let iterable = comprehension.iterable(self.module());
        let target = comprehension.target(self.module());

        let mut infer_iterable_type = || {
            let expression = self.index.expression(iterable);
            let result = infer_expression_types(self.db(), expression, TypeContext::default());

            // Two things are different if it's the first comprehension:
            // (1) We must lookup the `ScopedExpressionId` of the iterable expression in the outer scope,
            //     because that's the scope we visit it in in the semantic index builder
            // (2) We must *not* call `self.extend()` on the result of the type inference,
            //     because `ScopedExpressionId`s are only meaningful within their own scope, so
            //     we'd add types for random wrong expressions in the current scope
            if comprehension.is_first() && target.is_name_expr() {
                result.expression_type(iterable)
            } else {
                self.extend_expression_unchecked(result);
                result.expression_type(iterable)
            }
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
                let iterable_type = infer_iterable_type();

                iterable_type
                    .try_iterate_with_mode(
                        self.db(),
                        EvaluationMode::from_is_async(comprehension.is_async()),
                    )
                    .map(|tuple| tuple.homogeneous_element_type(self.db()))
                    .unwrap_or_else(|err| {
                        err.report_diagnostic(&self.context, iterable_type, iterable.into());
                        err.fallback_element_type(self.db())
                    })
            }
        };

        self.expressions.insert(target.into(), target_type);
        self.add_binding(target.into(), definition)
            .insert(self, target_type);
    }

    fn infer_named_expression(&mut self, named: &ast::ExprNamed) -> Type<'db> {
        // See https://peps.python.org/pep-0572/#differences-between-assignment-expressions-and-assignment-statements
        if named.target.is_name_expr() {
            let definition = self.index.expect_single_definition(named);
            let result = infer_definition_types(self.db(), definition);
            self.extend_definition(result);
            result.binding_type(definition)
        } else {
            // For syntactically invalid targets, we still need to run type inference:
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
        let ast::ExprIf {
            range: _,
            node_index: _,
            test,
            body,
            orelse,
        } = if_expression;

        let test_ty = self.infer_maybe_standalone_expression(test, TypeContext::default());
        let body_ty = self.infer_expression(body, tcx);
        let orelse_ty = self.infer_expression(orelse, tcx);

        match test_ty.try_bool(self.db()).unwrap_or_else(|err| {
            err.report_diagnostic(&self.context, &**test);
            err.fallback_truthiness()
        }) {
            Truthiness::AlwaysTrue => body_ty,
            Truthiness::AlwaysFalse => orelse_ty,
            Truthiness::Ambiguous => UnionType::from_two_elements(self.db(), body_ty, orelse_ty),
        }
    }

    fn infer_lambda_body(&mut self, lambda_expression: &ast::ExprLambda) {
        self.infer_expression(&lambda_expression.body, TypeContext::default());
    }

    fn infer_lambda_expression(&mut self, lambda_expression: &ast::ExprLambda) -> Type<'db> {
        let ast::ExprLambda {
            range: _,
            node_index: _,
            parameters,
            body: _,
        } = lambda_expression;

        // In stub files, default values may reference names that are defined later in the file.
        let in_stub = self.in_stub();
        let previous_deferred_state = std::mem::replace(&mut self.deferred_state, in_stub.into());

        let parameters = if let Some(parameters) = parameters {
            let positional_only = parameters
                .posonlyargs
                .iter()
                .map(|param| {
                    Parameter::positional_only(Some(param.name().id.clone()))
                        .with_optional_default_type(param.default().map(|default_expr| {
                            self.infer_expression(default_expr, TypeContext::default())
                                .replace_parameter_defaults(self.db())
                        }))
                })
                .collect::<Vec<_>>();
            let positional_or_keyword = parameters
                .args
                .iter()
                .map(|param| {
                    Parameter::positional_or_keyword(param.name().id.clone())
                        .with_optional_default_type(param.default().map(|default_expr| {
                            self.infer_expression(default_expr, TypeContext::default())
                                .replace_parameter_defaults(self.db())
                        }))
                })
                .collect::<Vec<_>>();
            let variadic = parameters
                .vararg
                .as_ref()
                .map(|param| Parameter::variadic(param.name().id.clone()));
            let keyword_only = parameters
                .kwonlyargs
                .iter()
                .map(|param| {
                    Parameter::keyword_only(param.name().id.clone()).with_optional_default_type(
                        param.default().map(|default_expr| {
                            self.infer_expression(default_expr, TypeContext::default())
                                .replace_parameter_defaults(self.db())
                        }),
                    )
                })
                .collect::<Vec<_>>();
            let keyword_variadic = parameters
                .kwarg
                .as_ref()
                .map(|param| Parameter::keyword_variadic(param.name().id.clone()));

            Parameters::new(
                self.db(),
                positional_only
                    .into_iter()
                    .chain(positional_or_keyword)
                    .chain(variadic)
                    .chain(keyword_only)
                    .chain(keyword_variadic),
            )
        } else {
            Parameters::empty()
        };

        self.deferred_state = previous_deferred_state;

        // TODO: Useful inference of a lambda's return type will require a different approach,
        // which does the inference of the body expression based on arguments at each call site,
        // rather than eagerly computing a return type without knowing the argument types.
        Type::function_like_callable(self.db(), Signature::new(parameters, Type::unknown()))
    }

    fn infer_call_expression(
        &mut self,
        call_expression: &ast::ExprCall,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let callable_type =
            self.infer_maybe_standalone_expression(&call_expression.func, TypeContext::default());

        self.infer_call_expression_impl(call_expression, callable_type, tcx)
    }

    fn infer_call_expression_impl(
        &mut self,
        call_expression: &ast::ExprCall,
        callable_type: Type<'db>,
        call_expression_tcx: TypeContext<'db>,
    ) -> Type<'db> {
        fn report_missing_implicit_constructor_call<'db>(
            context: &InferContext<'db, '_>,
            db: &'db dyn Db,
            callable_type: Type<'db>,
            call_expression: &ast::ExprCall,
            bindings: &Bindings<'db>,
        ) {
            if bindings.has_implicit_dunder_new_is_possibly_unbound() {
                if let Some(builder) =
                    context.report_lint(&POSSIBLY_MISSING_IMPLICIT_CALL, call_expression)
                {
                    builder.into_diagnostic(format_args!(
                        "Method `__new__` on type `{}` may be missing.",
                        callable_type.display(db),
                    ));
                }
            }

            if bindings.has_implicit_dunder_init_is_possibly_unbound() {
                if let Some(builder) =
                    context.report_lint(&POSSIBLY_MISSING_IMPLICIT_CALL, call_expression)
                {
                    builder.into_diagnostic(format_args!(
                        "Method `__init__` on type `{}` may be missing.",
                        callable_type.display(db),
                    ));
                }
            }
        }

        let ast::ExprCall {
            range: _,
            node_index: _,
            func,
            arguments,
        } = call_expression;

        // Fast-path dict(...) in TypedDict context: infer keyword values against fields,
        // then validate and return the TypedDict type.
        if let Some(tcx) = call_expression_tcx.annotation
            && let Some(typed_dict) = tcx
                .filter_union(self.db(), Type::is_typed_dict)
                .as_typed_dict()
            && callable_type
                .as_class_literal()
                .is_some_and(|class_literal| class_literal.is_known(self.db(), KnownClass::Dict))
            && arguments.args.is_empty()
            && arguments
                .keywords
                .iter()
                .all(|keyword| keyword.arg.is_some())
        {
            let items = typed_dict.items(self.db());
            for keyword in &arguments.keywords {
                if let Some(arg_name) = &keyword.arg {
                    let value_tcx = items
                        .get(arg_name.id.as_str())
                        .map(|field| TypeContext::new(Some(field.declared_ty)))
                        .unwrap_or_default();
                    self.infer_expression(&keyword.value, value_tcx);
                }
            }

            validate_typed_dict_constructor(
                &self.context,
                typed_dict,
                arguments,
                func.as_ref().into(),
                |expr| self.expression_type(expr),
            );

            return Type::TypedDict(typed_dict);
        }

        // Handle 3-argument `type(name, bases, dict)`.
        if let Type::ClassLiteral(class) = callable_type
            && class.is_known(self.db(), KnownClass::Type)
        {
            return self.infer_builtins_type_call(call_expression, None);
        }

        // Handle `typing.NamedTuple(typename, fields)` and `collections.namedtuple(typename, field_names)`.
        if let Some(namedtuple_kind) = NamedTupleKind::from_type(self.db(), callable_type) {
            return self.infer_namedtuple_call_expression(call_expression, None, namedtuple_kind);
        }

        // We don't call `Type::try_call`, because we want to perform type inference on the
        // arguments after matching them to parameters, but before checking that the argument types
        // are assignable to any parameter annotations.
        let mut call_arguments =
            CallArguments::from_arguments(arguments, |argument, splatted_value| {
                let ty = self.infer_expression(splatted_value, TypeContext::default());
                if let Some(argument) = argument {
                    self.store_expression_type(argument, ty);
                }
                ty
            });

        // Validate that starred arguments are iterable.
        for arg in &arguments.args {
            if let ast::Expr::Starred(ast::ExprStarred { value, .. }) = arg {
                let iterable_type = self.expression_type(value);
                if let Err(err) = iterable_type.try_iterate(self.db()) {
                    err.report_diagnostic(&self.context, iterable_type, value.as_ref().into());
                }
            }
        }

        // Validate that double-starred keyword arguments are mappings.
        for keyword in arguments.keywords.iter().filter(|k| k.arg.is_none()) {
            let mapping_type = self.expression_type(&keyword.value);

            if mapping_type.as_paramspec_typevar(self.db()).is_some()
                || mapping_type.unpack_keys_and_items(self.db()).is_some()
            {
                continue;
            }

            let Some(builder) = self
                .context
                .report_lint(&INVALID_ARGUMENT_TYPE, &keyword.value)
            else {
                continue;
            };

            builder
                .into_diagnostic("Argument expression after ** must be a mapping type")
                .set_primary_message(format_args!("Found `{}`", mapping_type.display(self.db())));
        }

        if callable_type.is_notimplemented(self.db()) {
            if let Some(builder) = self
                .context
                .report_lint(&CALL_NON_CALLABLE, call_expression)
            {
                let mut diagnostic = builder.into_diagnostic("`NotImplemented` is not callable");
                diagnostic.annotate(
                    self.context
                        .secondary(&**func)
                        .message("Did you mean `NotImplementedError`?"),
                );
                diagnostic.set_concise_message(
                    "`NotImplemented` is not callable - did you mean `NotImplementedError`?",
                );
            }
            return Type::unknown();
        }

        // Special handling for `TypedDict` method calls
        if let ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() {
            let value_type = self.expression_type(value);

            if let Type::TypedDict(typed_dict_ty) = value_type
                && matches!(attr.id.as_str(), "pop" | "setdefault")
                && !arguments.args.is_empty()

                // Validate the key argument for `TypedDict` methods
                && let Some(first_arg) = arguments.args.first()
                    && let ast::Expr::StringLiteral(ast::ExprStringLiteral {
                        value: key_literal,
                        ..
                    }) = first_arg
            {
                let key = key_literal.to_str();
                let items = typed_dict_ty.items(self.db());

                // Check if key exists
                if let Some((_, field)) = items
                    .iter()
                    .find(|(field_name, _)| field_name.as_str() == key)
                {
                    // Key exists - check if it's a `pop()` on a required field
                    if attr.id.as_str() == "pop" && field.is_required() {
                        report_cannot_pop_required_field_on_typed_dict(
                            &self.context,
                            first_arg.into(),
                            Type::TypedDict(typed_dict_ty),
                            key,
                        );
                        return Type::unknown();
                    }
                } else {
                    // Key not found, report error with suggestion and return early
                    let key_ty = Type::string_literal(self.db(), key);
                    report_invalid_key_on_typed_dict(
                        &self.context,
                        first_arg.into(),
                        first_arg.into(),
                        Type::TypedDict(typed_dict_ty),
                        None,
                        key_ty,
                        items,
                    );
                    // Return `Unknown` to prevent the overload system from generating its own error
                    return Type::unknown();
                }
            }
        }

        if let Type::FunctionLiteral(function) = callable_type {
            // Make sure that the `function.definition` is only called when the function is defined
            // in the same file as the one we're currently inferring the types for. This is because
            // the `definition` method accesses the semantic index, which could create a
            // cross-module AST dependency.
            if function.file(self.db()) == self.file()
                && function.definition(self.db()).scope(self.db()) == self.scope()
            {
                self.called_functions.insert(function);
            }

            // Warn when `final()` is called as a function (not a decorator).
            // Type checkers cannot interpret this usage and will not prevent subclassing.
            if function.is_known(self.db(), KnownFunction::Final) {
                if let Some(builder) = self
                    .context
                    .report_lint(&INEFFECTIVE_FINAL, call_expression)
                {
                    let mut diagnostic = builder.into_diagnostic(
                        "Type checkers will not prevent subclassing when `final()` is called as a function",
                    );
                    diagnostic.info("Use `@final` as a decorator on a class or method instead");
                }
            }
        }

        // Check for unsound calls to abstract classmethods/staticmethods on class objects
        match callable_type {
            Type::BoundMethod(bound_method) => {
                let function = bound_method.function(self.db());
                if let Some(class) = bound_method
                    .self_instance(self.db())
                    .to_class_type(self.db())
                {
                    if function.is_classmethod(self.db())
                        && function.as_abstract_method(self.db(), class).is_some()
                        && function.has_trivial_body(self.db())
                    {
                        report_call_to_abstract_method(
                            &self.context,
                            call_expression,
                            function,
                            "classmethod",
                        );
                    }
                }
            }
            Type::FunctionLiteral(function) if function.is_staticmethod(self.db()) => {
                if let ast::Expr::Attribute(ast::ExprAttribute { value, .. }) = func.as_ref() {
                    let value_type = self.expression_type(value);
                    if let Some(class) = value_type.to_class_type(self.db()) {
                        if function.as_abstract_method(self.db(), class).is_some()
                            && function.has_trivial_body(self.db())
                        {
                            report_call_to_abstract_method(
                                &self.context,
                                call_expression,
                                function,
                                "staticmethod",
                            );
                        }
                    }
                }
            }
            _ => {}
        }

        let class = match callable_type {
            Type::ClassLiteral(class) => Some(ClassType::NonGeneric(class)),
            Type::GenericAlias(generic) => Some(ClassType::Generic(generic)),
            Type::SubclassOf(subclass) => subclass.subclass_of().into_class(self.db()),
            _ => None,
        };

        if let Some(class) = class {
            // It might look odd here that we emit an error for class-literals and generic aliases but not
            // `type[]` types. But it's deliberate! The typing spec explicitly mandates that `type[]` types
            // can be called even though class-literals cannot. This is because even though a protocol class
            // `SomeProtocol` is always an abstract class, `type[SomeProtocol]` can be a concrete subclass of
            // that protocol -- and indeed, according to the spec, type checkers must disallow abstract
            // subclasses of the protocol to be passed to parameters that accept `type[SomeProtocol]`.
            // <https://typing.python.org/en/latest/spec/protocol.html#type-and-class-objects-vs-protocols>.
            if !callable_type.is_subclass_of()
                && let Some(protocol) = class.into_protocol_class(self.db())
            {
                report_attempted_protocol_instantiation(&self.context, call_expression, protocol);
            }

            // Inference of correctly-placed `TypeVar`, `ParamSpec`, `NewType`, and
            // `TypeAliasType` definitions is done in `infer_legacy_typevar`,
            // `infer_paramspec`, `infer_newtype_expression`, and
            // `infer_typealiastype_call`, and doesn't use the full call-binding
            // machinery. If we reach here, it means that someone is trying to
            // instantiate one of these in an invalid context.
            match class.known(self.db()) {
                Some(KnownClass::TypeVar | KnownClass::ExtensionsTypeVar) => {
                    if let Some(builder) = self
                        .context
                        .report_lint(&INVALID_LEGACY_TYPE_VARIABLE, call_expression)
                    {
                        builder.into_diagnostic(
                            "A `TypeVar` definition must be a simple variable assignment",
                        );
                    }
                }
                Some(KnownClass::ParamSpec | KnownClass::ExtensionsParamSpec) => {
                    if let Some(builder) = self
                        .context
                        .report_lint(&INVALID_PARAMSPEC, call_expression)
                    {
                        builder.into_diagnostic(
                            "A `ParamSpec` definition must be a simple variable assignment",
                        );
                    }
                }
                Some(KnownClass::NewType) => {
                    if let Some(builder) =
                        self.context.report_lint(&INVALID_NEWTYPE, call_expression)
                    {
                        builder.into_diagnostic(
                            "A `NewType` definition must be a simple variable assignment",
                        );
                    }
                }
                Some(KnownClass::TypeAliasType) => {
                    if let Some(builder) = self
                        .context
                        .report_lint(&INVALID_TYPE_ALIAS_TYPE, call_expression)
                    {
                        builder.into_diagnostic(
                            "A `TypeAliasType` definition must be a simple variable assignment",
                        );
                    }
                }
                _ => {}
            }
        }

        let mut bindings = callable_type
            .bindings(self.db())
            .match_parameters(self.db(), &call_arguments);

        report_missing_implicit_constructor_call(
            &self.context,
            self.db(),
            callable_type,
            call_expression,
            &bindings,
        );

        let bindings_result = self.infer_and_check_argument_types(
            ArgumentsIter::from_ast(arguments),
            &mut call_arguments,
            &mut |builder, (_, expr, tcx)| builder.infer_expression(expr, tcx),
            &mut bindings,
            call_expression_tcx,
        );

        // Validate `TypedDict` constructor calls after argument type inference.
        if let Some(class) = class
            && class.class_literal(self.db()).is_typed_dict(self.db())
        {
            validate_typed_dict_constructor(
                &self.context,
                TypedDictType::new(class),
                arguments,
                func.as_ref().into(),
                |expr| self.expression_type(expr),
            );
        }

        let mut bindings = match bindings_result {
            Ok(()) => bindings,
            Err(_) => {
                bindings.report_diagnostics(&self.context, call_expression.into());
                return bindings.return_type(self.db());
            }
        };

        for binding in bindings.iter_flat_mut() {
            let binding_type = binding.callable_type;
            for (_, overload) in binding.matching_overloads_mut() {
                match binding_type {
                    Type::FunctionLiteral(function_literal) => {
                        if let Some(known_function) = function_literal.known(self.db()) {
                            known_function.check_call(
                                &self.context,
                                overload,
                                &call_arguments,
                                call_expression,
                                self.file(),
                            );
                        }
                    }
                    Type::ClassLiteral(class) => {
                        if let Some(known_class) = class.known(self.db()) {
                            known_class.check_call(
                                &self.context,
                                self.index,
                                overload,
                                call_expression,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        let db = self.db();
        let scope = self.scope();
        let return_ty = bindings.return_type(db);

        let find_narrowed_place = || match arguments.args.first() {
            None => {
                // This branch looks extraneous, especially in the face of `missing-arguments`.
                // However, that lint won't be able to catch this:
                //
                // ```python
                // def f(v: object = object()) -> TypeIs[int]: ...
                //
                // if f(): ...
                // ```
                //
                // TODO: Will this report things that is actually fine?
                if let Some(builder) = self
                    .context
                    .report_lint(&INVALID_TYPE_GUARD_CALL, arguments)
                {
                    builder.into_diagnostic("Type guard call does not have a target");
                }
                None
            }
            Some(expr) => match PlaceExpr::try_from_expr(expr) {
                Some(place_expr) => place_table(db, scope).place_id(&place_expr),
                None => None,
            },
        };

        match return_ty {
            Type::TypeIs(type_is) => match find_narrowed_place() {
                Some(place) => type_is.bind(db, scope, place),
                None => return_ty,
            },
            Type::TypeGuard(type_guard) => match find_narrowed_place() {
                Some(place) => type_guard.bind(db, scope, place),
                None => return_ty,
            },
            _ => return_ty,
        }
    }

    fn infer_starred_expression(
        &mut self,
        starred: &ast::ExprStarred,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let ast::ExprStarred {
            range: _,
            node_index: _,
            value,
            ctx: _,
        } = starred;

        let db = self.db();
        let iterable_type = self.infer_expression(value, tcx);
        iterable_type
            .try_iterate(db)
            .map(|spec| Type::tuple(TupleType::new(db, &spec)))
            .unwrap_or_else(|err| {
                err.report_diagnostic(&self.context, iterable_type, value.as_ref().into());
                Type::homogeneous_tuple(db, err.fallback_element_type(db))
            })
    }

    fn infer_yield_expression(&mut self, yield_expression: &ast::ExprYield) -> Type<'db> {
        let ast::ExprYield {
            range: _,
            node_index: _,
            value,
        } = yield_expression;
        self.infer_optional_expression(value.as_deref(), TypeContext::default());
        todo_type!("yield expressions")
    }

    fn infer_yield_from_expression(&mut self, yield_from: &ast::ExprYieldFrom) -> Type<'db> {
        let ast::ExprYieldFrom {
            range: _,
            node_index: _,
            value,
        } = yield_from;

        let iterable_type = self.infer_expression(value, TypeContext::default());
        iterable_type
            .try_iterate(self.db())
            .map(|tuple| tuple.homogeneous_element_type(self.db()))
            .unwrap_or_else(|err| {
                err.report_diagnostic(&self.context, iterable_type, value.as_ref().into());
                err.fallback_element_type(self.db())
            });

        iterable_type
            .generator_return_type(self.db())
            .unwrap_or_else(Type::unknown)
    }

    fn infer_await_expression(&mut self, await_expression: &ast::ExprAwait) -> Type<'db> {
        let ast::ExprAwait {
            range: _,
            node_index: _,
            value,
        } = await_expression;
        let expr_type = self.infer_expression(value, TypeContext::default());
        expr_type.try_await(self.db()).unwrap_or_else(|err| {
            err.report_diagnostic(&self.context, expr_type, value.as_ref().into());
            Type::unknown()
        })
    }

    // Perform narrowing with applicable constraints between the current scope and the enclosing scope.
    fn narrow_place_with_applicable_constraints(
        &self,
        expr: PlaceExprRef,
        mut ty: Type<'db>,
        constraint_keys: &[(FileScopeId, ConstraintKey)],
    ) -> Type<'db> {
        let db = self.db();
        for (enclosing_scope_file_id, constraint_key) in constraint_keys {
            let use_def = self.index.use_def_map(*enclosing_scope_file_id);
            let place_table = self.index.place_table(*enclosing_scope_file_id);
            let place = place_table.place_id(expr).unwrap();

            match use_def.applicable_constraints(
                *constraint_key,
                *enclosing_scope_file_id,
                expr,
                self.index,
            ) {
                ApplicableConstraints::UnboundBinding(constraint) => {
                    ty = constraint.narrow(db, ty, place);
                }
                // Performs narrowing based on constrained bindings.
                // This handling must be performed even if narrowing is attempted and failed using `infer_place_load`.
                // The result of `infer_place_load` can be applied as is only when its boundness is `Bound`.
                // For example, this handling is required in the following case:
                // ```python
                // class C:
                //     x: int | None = None
                // c = C()
                // # c.x: int | None = <unbound>
                // if c.x is None:
                //     c.x = 1
                // # else: c.x: int = <unbound>
                // # `c.x` is not definitely bound here
                // reveal_type(c.x)  # revealed: int
                // ```
                ApplicableConstraints::ConstrainedBindings(bindings) => {
                    let reachability_constraints = bindings.reachability_constraints;
                    let predicates = bindings.predicates;
                    let mut union = UnionBuilder::new(db);
                    for binding in bindings {
                        let static_reachability = reachability_constraints.evaluate(
                            db,
                            predicates,
                            binding.reachability_constraint,
                        );
                        if static_reachability.is_always_false() {
                            continue;
                        }
                        match binding.binding {
                            DefinitionState::Defined(definition) => {
                                let binding_ty = binding_type(db, definition);
                                union = union.add(
                                    binding.narrowing_constraint.narrow(db, binding_ty, place),
                                );
                            }
                            DefinitionState::Undefined | DefinitionState::Deleted => {
                                union =
                                    union.add(binding.narrowing_constraint.narrow(db, ty, place));
                            }
                        }
                    }
                    // If there are no visible bindings, the union becomes `Never`.
                    // Since an unbound binding is recorded even for an undefined place,
                    // this can only happen if the code is unreachable
                    // and therefore it is correct to set the result to `Never`.
                    ty = union.build();
                }
            }
        }
        ty
    }

    /// Check if the given ty is `@deprecated` or not
    fn check_deprecated<T: Ranged>(&self, ranged: T, ty: Type) {
        // First handle classes
        if let Type::ClassLiteral(class_literal) = ty {
            let Some(deprecated) = class_literal.deprecated(self.db()) else {
                return;
            };

            let Some(builder) = self.context.report_lint(&diagnostic::DEPRECATED, ranged) else {
                return;
            };

            let class_name = class_literal.name(self.db());
            let mut diag =
                builder.into_diagnostic(format_args!(r#"The class `{class_name}` is deprecated"#));
            if let Some(message) = deprecated.message(self.db()) {
                diag.set_primary_message(message.value(self.db()));
            }
            diag.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Deprecated);
            return;
        }

        // Next handle functions
        let function = match ty {
            Type::FunctionLiteral(function) => function,
            Type::BoundMethod(bound) => bound.function(self.db()),
            _ => return,
        };

        // Currently we only check the final implementation for deprecation, as
        // that check can be done on any reference to the function. Analysis of
        // deprecated overloads needs to be done in places where we resolve the
        // actual overloads being used.
        let Some(deprecated) = function.implementation_deprecated(self.db()) else {
            return;
        };

        let Some(builder) = self
            .context
            .report_lint(&crate::types::diagnostic::DEPRECATED, ranged)
        else {
            return;
        };

        let func_name = function.name(self.db());
        let mut diag =
            builder.into_diagnostic(format_args!(r#"The function `{func_name}` is deprecated"#));
        if let Some(message) = deprecated.message(self.db()) {
            diag.set_primary_message(message.value(self.db()));
        }
        diag.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Deprecated);
    }

    fn infer_name_load(&mut self, name_node: &ast::ExprName) -> Type<'db> {
        let ast::ExprName {
            range: _,
            node_index: _,
            id: symbol_name,
            ctx: _,
        } = name_node;
        let expr = PlaceExpr::from_expr_name(name_node);
        let db = self.db();

        let (resolved, constraint_keys) =
            self.infer_place_load(PlaceExprRef::from(&expr), ast::ExprRef::Name(name_node));

        let resolved_after_fallback = resolved
            // Not found in the module's explicitly declared global symbols?
            // Check the "implicit globals" such as `__doc__`, `__file__`, `__name__`, etc.
            // These are looked up as attributes on `types.ModuleType`.
            .or_fall_back_to(db, || {
                module_type_implicit_global_symbol(db, symbol_name).map_type(|ty| {
                    self.narrow_place_with_applicable_constraints(
                        PlaceExprRef::from(&expr),
                        ty,
                        &constraint_keys,
                    )
                })
            })
            // Not found in globals? Fallback to builtins
            // (without infinite recursion if we're already in builtins.)
            .or_fall_back_to(db, || {
                if Some(self.scope()) == builtins_module_scope(db) {
                    Place::Undefined.into()
                } else {
                    builtins_symbol(db, symbol_name)
                }
            })
            // Still not found? It might be `reveal_type`...
            .or_fall_back_to(db, || {
                if symbol_name == "reveal_type" {
                    if let Some(builder) = self.context.report_lint(&UNDEFINED_REVEAL, name_node) {
                        let mut diag =
                            builder.into_diagnostic("`reveal_type` used without importing it");
                        diag.info(
                            "This is allowed for debugging convenience but will fail at runtime",
                        );
                    }
                    typing_extensions_symbol(db, symbol_name)
                } else {
                    Place::Undefined.into()
                }
            });

        if !resolved_after_fallback.place.is_definitely_bound() {
            self.all_definitely_bound = false;
        }

        let ty =
            resolved_after_fallback.unwrap_with_diagnostic(db, |lookup_error| match lookup_error {
                LookupError::Undefined(qualifiers) => {
                    self.report_unresolved_reference(name_node);
                    TypeAndQualifiers::new(Type::unknown(), TypeOrigin::Inferred, qualifiers)
                }
                LookupError::PossiblyUndefined(type_when_bound) => {
                    if self.is_reachable(name_node) {
                        report_possibly_unresolved_reference(&self.context, name_node);
                    }
                    type_when_bound
                }
            });

        ty.inner_type()
    }

    fn infer_local_place_load(
        &self,
        expr: PlaceExprRef,
        expr_ref: ast::ExprRef,
    ) -> (Place<'db>, Option<ScopedUseId>) {
        let db = self.db();
        let scope = self.scope();
        let file_scope_id = scope.file_scope_id(db);
        let place_table = self.index.place_table(file_scope_id);
        let use_def = self.index.use_def_map(file_scope_id);

        // If we're inferring types of deferred expressions, look them up from end-of-scope.
        if self.is_deferred() {
            let place = if let Some(place_id) = place_table.place_id(expr) {
                place_from_bindings(db, use_def.reachable_bindings(place_id)).place
            } else {
                assert!(
                    self.deferred_state.in_string_annotation(),
                    "Expected the place table to create a place for every valid PlaceExpr node"
                );
                Place::Undefined
            };
            (place, None)
        } else {
            if expr_ref
                .as_name_expr()
                .is_some_and(|name| name.is_invalid())
            {
                return (Place::Undefined, None);
            }

            let use_id = expr_ref.scoped_use_id(db, scope);
            let place = place_from_bindings(db, use_def.bindings_at_use(use_id)).place;

            (place, Some(use_id))
        }
    }

    /// Infer the type of a place expression from definitions, assuming a load context.
    /// This method also returns the [`ConstraintKey`]s for each scope associated with `expr`,
    /// which is used to narrow by condition rather than by assignment.
    fn infer_place_load(
        &self,
        place_expr: PlaceExprRef,
        expr_ref: ast::ExprRef,
    ) -> (PlaceAndQualifiers<'db>, Vec<(FileScopeId, ConstraintKey)>) {
        let db = self.db();
        let scope = self.scope();
        let file_scope_id = scope.file_scope_id(db);
        let place_table = self.index.place_table(file_scope_id);

        let mut constraint_keys = vec![];
        let (local_scope_place, use_id) = self.infer_local_place_load(place_expr, expr_ref);
        if let Some(use_id) = use_id {
            constraint_keys.push((file_scope_id, ConstraintKey::UseId(use_id)));
        }

        let place = PlaceAndQualifiers::from(local_scope_place).or_fall_back_to(db, || {
            let mut symbol_resolves_locally = false;
            if let Some(symbol) = place_expr.as_symbol()
                && let Some(symbol_id) = place_table.symbol_id(symbol.name())
            {
                // Footgun: `place_expr` and `symbol` were probably constructed with all-zero
                // flags. We need to read the place table to get correct flags.
                symbol_resolves_locally = place_table.symbol(symbol_id).is_local();
                // If we try to access a variable in a class before it has been defined, the
                // lookup will fall back to global. See the comment on `Symbol::is_local`.
                let fallback_to_global =
                    scope.node(db).scope_kind().is_class() && symbol_resolves_locally;
                if self.skip_non_global_scopes(file_scope_id, symbol_id) || fallback_to_global {
                    return global_symbol(self.db(), self.file(), symbol.name()).map_type(|ty| {
                        self.narrow_place_with_applicable_constraints(
                            place_expr,
                            ty,
                            &constraint_keys,
                        )
                    });
                }
            }

            // Symbols that are bound or declared in the local scope, and not marked `nonlocal` or
            // `global`, never refer to an enclosing scope. (If you reference such a symbol before
            // it's bound, you get an `UnboundLocalError`.) Short-circuit instead of walking
            // enclosing scopes in this case. The one exception to this rule is the global fallback
            // in class bodies, which we already handled above.
            if symbol_resolves_locally {
                return Place::Undefined.into();
            }

            for parent_id in place_table.parents(place_expr) {
                let parent_expr = place_table.place(parent_id);
                let mut expr_ref = expr_ref;
                for _ in 0..(place_expr.num_member_segments() - parent_expr.num_member_segments()) {
                    match expr_ref {
                        ast::ExprRef::Attribute(attribute) => {
                            expr_ref = ast::ExprRef::from(&attribute.value);
                        }
                        ast::ExprRef::Subscript(subscript) => {
                            expr_ref = ast::ExprRef::from(&subscript.value);
                        }
                        _ => unreachable!(),
                    }
                }
                let (parent_place, _use_id) = self.infer_local_place_load(parent_expr, expr_ref);
                if let Place::Defined(_) = parent_place {
                    return Place::Undefined.into();
                }
            }

            // Walk up parent scopes looking for a possible enclosing scope that may have a
            // definition of this name visible to us (would be `LOAD_DEREF` at runtime.)
            // Note that we skip the scope containing the use that we are resolving, since we
            // already looked for the place there up above.
            let mut nonlocal_union_builder = UnionBuilder::new(db);
            let mut found_some_definition = false;
            for (enclosing_scope_file_id, _) in self.index.ancestor_scopes(file_scope_id).skip(1) {
                // If the current enclosing scope is global, no place lookup is performed here,
                // instead falling back to the module's explicit global lookup below.
                if enclosing_scope_file_id.is_global() {
                    break;
                }

                // Class scopes are not visible to nested scopes, and we need to handle global
                // scope differently (because an unbound name there falls back to builtins), so
                // check only function-like scopes.
                // There is one exception to this rule: annotation scopes can see
                // names defined in an immediately-enclosing class scope.
                let enclosing_scope = self.index.scope(enclosing_scope_file_id);

                let is_immediately_enclosing_scope = scope.is_annotation(db)
                    && scope
                        .scope(db)
                        .parent()
                        .is_some_and(|parent| parent == enclosing_scope_file_id);

                let has_root_place_been_reassigned = || {
                    let enclosing_place_table = self.index.place_table(enclosing_scope_file_id);
                    enclosing_place_table
                        .parents(place_expr)
                        .any(|enclosing_root_place_id| {
                            enclosing_place_table
                                .place(enclosing_root_place_id)
                                .is_bound()
                        })
                };

                // If the reference is in a nested eager scope, we need to look for the place at
                // the point where the previous enclosing scope was defined, instead of at the end
                // of the scope. (Note that the semantic index builder takes care of only
                // registering eager bindings for nested scopes that are actually eager, and for
                // enclosing scopes that actually contain bindings that we should use when
                // resolving the reference.)
                let mut eagerly_resolved_place = None;
                if !self.is_deferred() {
                    match self.index.enclosing_snapshot(
                        enclosing_scope_file_id,
                        place_expr,
                        file_scope_id,
                    ) {
                        EnclosingSnapshotResult::FoundConstraint(constraint) => {
                            constraint_keys.push((
                                enclosing_scope_file_id,
                                ConstraintKey::NarrowingConstraint(constraint),
                            ));
                            // If the current scope is eager, it is certain that the place is undefined in the current scope.
                            // Do not call the `place` query below as a fallback.
                            if scope.scope(db).is_eager() {
                                eagerly_resolved_place = Some(Place::Undefined.into());
                            }
                        }
                        EnclosingSnapshotResult::FoundBindings(bindings) => {
                            let place = place_from_bindings(db, bindings).place.map_type(|ty| {
                                self.narrow_place_with_applicable_constraints(
                                    place_expr,
                                    ty,
                                    &constraint_keys,
                                )
                            });
                            constraint_keys.push((
                                enclosing_scope_file_id,
                                ConstraintKey::NestedScope(file_scope_id),
                            ));
                            return place.into();
                        }
                        // There are no visible bindings / constraint here.
                        // Don't fall back to non-eager place resolution.
                        EnclosingSnapshotResult::NotFound => {
                            if has_root_place_been_reassigned() {
                                return Place::Undefined.into();
                            }
                            continue;
                        }
                        EnclosingSnapshotResult::NoLongerInEagerContext => {
                            if has_root_place_been_reassigned() {
                                return Place::Undefined.into();
                            }
                        }
                    }
                }

                if !enclosing_scope.kind().is_function_like() && !is_immediately_enclosing_scope {
                    continue;
                }

                let enclosing_place_table = self.index.place_table(enclosing_scope_file_id);
                let Some(enclosing_place_id) = enclosing_place_table.place_id(place_expr) else {
                    continue;
                };

                let enclosing_place = enclosing_place_table.place(enclosing_place_id);

                // Reads of "free" variables terminate at any enclosing scope that marks the
                // variable `global`, whether or not that scope actually binds the variable. If we
                // see a `global` declaration, stop walking scopes and proceed to the global
                // handling below. (If we're walking from a prior/inner scope where this variable
                // is `nonlocal`, then this is a semantic syntax error, but we don't enforce that
                // here. See `infer_nonlocal_statement`.)
                if enclosing_place.as_symbol().is_some_and(Symbol::is_global) {
                    break;
                }

                let enclosing_scope_id = enclosing_scope_file_id.to_scope_id(db, self.file());

                // If the name is declared or bound in this scope, figure out its type. This might
                // resolve the name and end the walk. But if the name is declared `nonlocal` in
                // this scope, we'll keep walking enclosing scopes and union this type with the
                // other types we find. (It's a semantic syntax error to declare a type for a
                // `nonlocal` variable, but we don't enforce that here. See the
                // `ast::Stmt::AnnAssign` handling in `SemanticIndexBuilder::visit_stmt`.)
                if enclosing_place.is_bound() || enclosing_place.is_declared() {
                    let local_place_and_qualifiers = eagerly_resolved_place.unwrap_or_else(|| {
                        place(
                            db,
                            enclosing_scope_id,
                            place_expr,
                            ConsideredDefinitions::AllReachable,
                        )
                        .map_type(|ty| {
                            self.narrow_place_with_applicable_constraints(
                                place_expr,
                                ty,
                                &constraint_keys,
                            )
                        })
                    });
                    // We could have `Place::Undefined` here, despite the checks above, for example if
                    // this scope contains a `del` statement but no binding or declaration.
                    if let Place::Defined(DefinedPlace {
                        ty: type_,
                        definedness: boundness,
                        ..
                    }) = local_place_and_qualifiers.place
                    {
                        nonlocal_union_builder.add_in_place(type_);
                        // `ConsideredDefinitions::AllReachable` never returns PossiblyUnbound
                        debug_assert_eq!(boundness, Definedness::AlwaysDefined);
                        found_some_definition = true;
                    }

                    if !enclosing_place.as_symbol().is_some_and(Symbol::is_nonlocal) {
                        // We've reached a function-like scope that marks this name bound or
                        // declared but doesn't mark it `nonlocal`. The name is therefore resolved,
                        // and we won't consider any scopes outside of this one.
                        return if found_some_definition {
                            Place::bound(nonlocal_union_builder.build()).into()
                        } else {
                            Place::Undefined.into()
                        };
                    }
                }
            }

            PlaceAndQualifiers::default()
                // If we're in a class body, check for implicit class body symbols first.
                // These take precedence over globals.
                .or_fall_back_to(db, || {
                    if scope.node(db).scope_kind().is_class()
                        && let Some(symbol) = place_expr.as_symbol()
                    {
                        let implicit = class_body_implicit_symbol(db, symbol.name());
                        if implicit.place.is_definitely_bound() {
                            return implicit.map_type(|ty| {
                                self.narrow_place_with_applicable_constraints(
                                    place_expr,
                                    ty,
                                    &constraint_keys,
                                )
                            });
                        }
                    }
                    Place::Undefined.into()
                })
                // No nonlocal binding? Check the module's explicit globals.
                // Avoid infinite recursion if `self.scope` already is the module's global scope.
                .or_fall_back_to(db, || {
                    if file_scope_id.is_global() {
                        return Place::Undefined.into();
                    }

                    if !self.is_deferred() {
                        match self.index.enclosing_snapshot(
                            FileScopeId::global(),
                            place_expr,
                            file_scope_id,
                        ) {
                            EnclosingSnapshotResult::FoundConstraint(constraint) => {
                                constraint_keys.push((
                                    FileScopeId::global(),
                                    ConstraintKey::NarrowingConstraint(constraint),
                                ));
                                // Reaching here means that no bindings are found in any scope.
                                // Since `explicit_global_symbol` may return a cycle initial value, we return `Place::Undefined` here.
                                return Place::Undefined.into();
                            }
                            EnclosingSnapshotResult::FoundBindings(bindings) => {
                                let place =
                                    place_from_bindings(db, bindings).place.map_type(|ty| {
                                        self.narrow_place_with_applicable_constraints(
                                            place_expr,
                                            ty,
                                            &constraint_keys,
                                        )
                                    });
                                constraint_keys.push((
                                    FileScopeId::global(),
                                    ConstraintKey::NestedScope(file_scope_id),
                                ));
                                return place.into();
                            }
                            // There are no visible bindings / constraint here.
                            EnclosingSnapshotResult::NotFound => {
                                return Place::Undefined.into();
                            }
                            EnclosingSnapshotResult::NoLongerInEagerContext => {}
                        }
                    }

                    let Some(symbol) = place_expr.as_symbol() else {
                        return Place::Undefined.into();
                    };

                    explicit_global_symbol(db, self.file(), symbol.name()).map_type(|ty| {
                        self.narrow_place_with_applicable_constraints(
                            place_expr,
                            ty,
                            &constraint_keys,
                        )
                    })
                })
        });

        if let Some(ty) = place.place.ignore_possibly_undefined() {
            self.check_deprecated(expr_ref, ty);
        }

        (place, constraint_keys)
    }

    pub(super) fn report_unresolved_reference(&self, expr_name_node: &ast::ExprName) {
        if !self.is_reachable(expr_name_node) {
            return;
        }

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
                self.db(),
                &mut diagnostic,
                "resolving types",
            );
        }

        // ===
        // Subdiagnostic (2): check to see if it's a capitalized older type hint that is available as lowercase in this version of Python.
        // ===
        // We don't need to check for typing_extensions.Type,
        // because it's already caught by typing.Type.
        if Program::get(self.db()).python_version(self.db()) >= PythonVersion::PY39 {
            if let Some(("", builtin_name)) = as_pep_585_generic("typing", id) {
                diagnostic.set_primary_message(format_args!("Did you mean `{builtin_name}`?"));
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

        let function_definition = self.index.expect_single_definition(current_function);
        let Type::FunctionLiteral(function_type) = binding_type(self.db(), function_definition)
        else {
            return;
        };

        let attribute_exists = match MethodDecorator::try_from_fn_type(self.db(), function_type) {
            Ok(MethodDecorator::ClassMethod) => !Type::instance(self.db(), class)
                .class_member(self.db(), id.clone())
                .place
                .is_undefined(),
            Ok(MethodDecorator::None) => !Type::instance(self.db(), class)
                .member(self.db(), id)
                .place
                .is_undefined(),
            Ok(MethodDecorator::StaticMethod) | Err(()) => false,
        };

        if attribute_exists {
            diagnostic.info(format_args!(
                "An attribute `{id}` is available: consider using `{first_parameter_name}.{id}`"
            ));
        }
    }

    fn infer_name_expression(&mut self, name: &ast::ExprName) -> Type<'db> {
        match name.ctx {
            ExprContext::Load => self.infer_name_load(name),
            ExprContext::Store => Type::Never,
            ExprContext::Del => {
                self.infer_name_load(name);
                Type::Never
            }
            ExprContext::Invalid => Type::unknown(),
        }
    }

    fn narrow_expr_with_applicable_constraints<'r>(
        &mut self,
        target: impl Into<ast::ExprRef<'r>>,
        target_ty: Type<'db>,
        constraint_keys: &[(FileScopeId, ConstraintKey)],
    ) -> Type<'db> {
        let target = target.into();

        if let Some(place_expr) = PlaceExpr::try_from_expr(target) {
            self.narrow_place_with_applicable_constraints(
                PlaceExprRef::from(&place_expr),
                target_ty,
                constraint_keys,
            )
        } else {
            target_ty
        }
    }

    /// Infer the type of a [`ast::ExprAttribute`] expression, assuming a load context.
    fn infer_attribute_load(&mut self, attribute: &ast::ExprAttribute) -> Type<'db> {
        fn is_dotted_name(attribute: &ast::Expr) -> bool {
            match attribute {
                ast::Expr::Name(_) => true,
                ast::Expr::Attribute(ast::ExprAttribute { value, .. }) => is_dotted_name(value),
                _ => false,
            }
        }

        let ast::ExprAttribute { value, attr, .. } = attribute;

        let mut value_type = self.infer_maybe_standalone_expression(value, TypeContext::default());
        let db = self.db();
        let mut constraint_keys = vec![];

        if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = value_type
            && typevar.is_paramspec(db)
            && let Some(bound_typevar) = bind_typevar(
                db,
                self.index,
                self.scope().file_scope_id(db),
                self.typevar_binding_context,
                typevar,
            )
        {
            value_type = Type::TypeVar(bound_typevar);
        }

        let mut assigned_type = None;
        if let Some(place_expr) = PlaceExpr::try_from_expr(attribute) {
            let (resolved, keys) = self.infer_place_load(
                PlaceExprRef::from(&place_expr),
                ast::ExprRef::Attribute(attribute),
            );
            constraint_keys.extend(keys);
            if let Place::Defined(DefinedPlace {
                ty,
                definedness: Definedness::AlwaysDefined,
                ..
            }) = resolved.place
            {
                assigned_type = Some(ty);
            }
        }
        let mut fallback_place = value_type.member(db, &attr.id);
        // Exclude non-definitely-bound places for purposes of reachability
        // analysis. We currently do not perform boundness analysis for implicit
        // instance attributes, so we exclude them here as well.
        if !fallback_place.place.is_definitely_bound()
            || fallback_place
                .qualifiers
                .contains(TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE)
        {
            self.all_definitely_bound = false;
        }

        fallback_place = fallback_place.map_type(|ty| {
            self.narrow_expr_with_applicable_constraints(attribute, ty, &constraint_keys)
        });

        let attr_name = &attr.id;
        let resolved_type =
            fallback_place.unwrap_with_diagnostic(db, |lookup_err| match lookup_err {
                LookupError::Undefined(_) => {
                    let fallback = || {
                        TypeAndQualifiers::new(
                            Type::unknown(),
                            TypeOrigin::Inferred,
                            TypeQualifiers::empty(),
                        )
                    };

                    if !self.is_reachable(attribute) {
                        return fallback();
                    }

                    let bound_on_instance = match value_type {
                        Type::ClassLiteral(class) => {
                            !class.instance_member(db, None, attr).is_undefined()
                        }
                        Type::SubclassOf(subclass_of @ SubclassOfType { .. }) => {
                            match subclass_of.subclass_of() {
                                SubclassOfInner::Class(class) => {
                                    !class.instance_member(db, attr).is_undefined()
                                }
                                SubclassOfInner::Dynamic(_) => unreachable!(
                                    "Attribute lookup on a dynamic `SubclassOf` type \
                                    should always return a bound symbol"
                                ),
                                SubclassOfInner::TypeVar(_) => false,
                            }
                        }
                        _ => false,
                    };

                    if let Type::ModuleLiteral(module) = value_type {
                        let module = module.module(db);
                        let module_name = module.name(db);
                        if module.kind(db).is_package()
                            && let Some(relative_submodule) = ModuleName::new(attr_name)
                        {
                            let mut maybe_submodule_name = module_name.clone();
                            maybe_submodule_name.extend(&relative_submodule);
                            if resolve_module(db, self.file(), &maybe_submodule_name).is_some() {
                                if let Some(builder) = self
                                    .context
                                    .report_lint(&POSSIBLY_MISSING_SUBMODULE, attribute)
                                {
                                    let mut diag = builder.into_diagnostic(format_args!(
                                        "Submodule `{attr_name}` might not have been imported"
                                    ));
                                    diag.help(format_args!(
                                        "Consider explicitly importing `{maybe_submodule_name}`"
                                    ));
                                }
                                return fallback();
                            }
                        }
                    }

                    if let Type::SpecialForm(special_form) = value_type {
                        if let Some(builder) =
                            self.context.report_lint(&UNRESOLVED_ATTRIBUTE, attribute)
                        {
                            let mut diag = builder.into_diagnostic(format_args!(
                                "Special form `{special_form}` has no attribute `{attr_name}`",
                            ));
                            if let Ok(defined_type) = value_type.in_type_expression(
                                db,
                                self.scope(),
                                self.typevar_binding_context,
                                self.inference_flags
                            ) && !defined_type.member(db, attr_name).place.is_undefined()
                            {
                                diag.help(format_args!(
                                    "Objects with type `{ty}` have a{maybe_n} `{attr_name}` \
                                    attribute, but the symbol `{special_form}` \
                                    does not itself inhabit the type `{ty}`",
                                    maybe_n = if attr_name.starts_with(['a', 'e', 'i', 'o', 'u']) {
                                        "n"
                                    } else {
                                        ""
                                    },
                                    ty = defined_type.display(self.db())
                                ));
                                if is_dotted_name(value) {
                                    let source =
                                        &source_text(self.db(), self.file())[value.range()];
                                    diag.help(format_args!(
                                        "This error may indicate that `{source}` was defined as \
                                        `{source} = {special_form}` when \
                                        `{source}: {special_form}` was intended"
                                    ));
                                }
                            }
                        }
                        return fallback();
                    }

                    let Some(builder) = self.context.report_lint(&UNRESOLVED_ATTRIBUTE, attribute)
                    else {
                        return fallback();
                    };

                    if bound_on_instance {
                        builder.into_diagnostic(format_args!(
                            "Attribute `{attr_name}` can only be accessed on instances, \
                            not on the class object `{}` itself.",
                            value_type.display(db)
                        ));
                        return fallback();
                    }

                    let mut diagnostic = match value_type {
                        Type::ModuleLiteral(module) => builder.into_diagnostic(format_args!(
                            "Module `{module_name}` has no member `{attr_name}`",
                            module_name = module.module(db).name(db),
                        )),
                        Type::ClassLiteral(class) => builder.into_diagnostic(format_args!(
                            "Class `{}` has no attribute `{attr_name}`",
                            class.name(db),
                        )),
                        Type::GenericAlias(alias) => builder.into_diagnostic(format_args!(
                            "Class `{}` has no attribute `{attr_name}`",
                            alias.display(db),
                        )),
                        Type::FunctionLiteral(function) => builder.into_diagnostic(format_args!(
                            "Function `{}` has no attribute `{attr_name}`",
                            function.name(db),
                        )),
                        _ => builder.into_diagnostic(format_args!(
                            "Object of type `{}` has no attribute `{attr_name}`",
                            value_type.display(db),
                        )),
                    };

                    if value_type.is_callable_type()
                        && KnownClass::FunctionType
                            .to_instance(db)
                            .member(db, attr_name)
                            .place
                            .is_definitely_bound()
                    {
                        diagnostic.help(format_args!(
                            "Function objects have a{maybe_n} `{attr_name}` attribute, \
                            but not all callable objects are functions",
                            maybe_n = if attr_name
                                .trim_start_matches('_')
                                .starts_with(['a', 'e', 'i', 'o', 'u'])
                            {
                                "n"
                            } else {
                                ""
                            },
                        ));

                        // without the <> around the URL, if you double click on the URL in the terminal it tries to load
                        // https://docs.astral.sh/ty/reference/typing-faq/#why-does-ty-say-callable-has-no-attribute-__name
                        // (without the __ suffix at the end of the URL). That doesn't exist, so the page loaded in the
                        // browser opens at the top of the FAQs page instead of taking you directly to the relevant FAQ.
                        diagnostic.help(
                            "See this FAQ for more information: \
                            <https://docs.astral.sh/ty/reference/typing-faq/\
                            #why-does-ty-say-callable-has-no-attribute-__name__>",
                        );
                    } else {
                        hint_if_stdlib_attribute_exists_on_other_versions(
                            db,
                            diagnostic,
                            value_type,
                            attr_name,
                            &format!("resolving the `{attr_name}` attribute"),
                        );
                    }

                    fallback()
                }
                LookupError::PossiblyUndefined(type_when_bound) => {
                    // `PossiblyUndefined` is ambiguous here. It could be because an attribute is
                    // conditionally defined, for example:
                    // ```
                    // class Foo:
                    //     if flag:
                    //         x = 42
                    // ```
                    // That is indeed a "possibly missing attribute", and it's a warning by default, because
                    // there's a high false positive rate.
                    //
                    // On the other hand, we could be looking at a union where some elements have
                    // the attribute but others definitely don't. That's a very different case, and
                    // we want it to be an error. Use `as_union_like` here to handle type aliases
                    // of unions and `NewType`s of float/complex in addition to explicit unions.
                    if let Some(union) = value_type.as_union_like(db) {
                        let elements_missing_the_attribute: Vec<_> = union
                            .elements(db)
                            .iter()
                            .filter(|element| element.member(db, attr_name).place.is_undefined())
                            .collect();
                        if !elements_missing_the_attribute.is_empty() {
                            if let Some(builder) =
                                self.context.report_lint(&UNRESOLVED_ATTRIBUTE, attribute)
                            {
                                let missing_types = elements_missing_the_attribute
                                    .iter()
                                    .map(|ty| format!("`{}`", ty.display(db)))
                                    .collect::<Vec<_>>()
                                    .join(", ");

                                builder.into_diagnostic(format_args!(
                                    "Attribute `{attr_name}` is not defined on {} in union `{value_type}`",
                                    missing_types,
                                    value_type = value_type.display(db),
                                ));
                            }
                            return type_when_bound;
                        }
                    }

                    report_possibly_missing_attribute(
                        &self.context,
                        attribute,
                        &attr.id,
                        value_type,
                    );

                    type_when_bound
                }
            });

        let resolved_type = resolved_type.inner_type();

        self.check_deprecated(attr, resolved_type);

        // Even if we can obtain the attribute type based on the assignments, we still perform default type inference
        // (to report errors).
        assigned_type.unwrap_or(resolved_type)
    }

    fn infer_attribute_expression(&mut self, attribute: &ast::ExprAttribute) -> Type<'db> {
        let ast::ExprAttribute {
            value,
            attr: _,
            range: _,
            node_index: _,
            ctx,
        } = attribute;

        match ctx {
            ExprContext::Load => self.infer_attribute_load(attribute),
            ExprContext::Store => {
                self.infer_expression(value, TypeContext::default());
                Type::Never
            }
            ExprContext::Del => {
                self.infer_attribute_load(attribute);
                Type::Never
            }
            ExprContext::Invalid => {
                self.infer_expression(value, TypeContext::default());
                Type::unknown()
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
                self.db(),
                unary_dunder_method,
                CallArguments::none(),
                TypeContext::default(),
            ) {
                Ok(outcome) => outcome.return_type(self.db()),
                Err(e) => {
                    if let Some(builder) = self.context.report_lint(&UNSUPPORTED_OPERATOR, unary) {
                        builder.into_diagnostic(format_args!(
                            "Unary operator `{op}` is not supported for object of type `{}`",
                            operand_type.display(self.db()),
                        ));
                    }
                    e.fallback_return_type(self.db())
                }
            }
        };

        match (op, operand_type) {
            (_, Type::Dynamic(_)) => operand_type,
            (_, Type::Never) => Type::Never,

            (_, Type::TypeAlias(alias)) => {
                self.infer_unary_expression_type(op, alias.value_type(self.db()), unary)
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
                    .unwrap_or_else(|| KnownClass::Int.to_instance(self.db())),
                LiteralValueTypeKind::Bool(value) => Type::int_literal(-i64::from(value)),
                _ => fallback_unary_expression_type(),
            },

            (ast::UnaryOp::Invert, Type::LiteralValue(literal)) => match literal.kind() {
                LiteralValueTypeKind::Int(value) => Type::int_literal(!value.as_i64()),
                LiteralValueTypeKind::Bool(value) => Type::int_literal(!i64::from(value)),
                _ => fallback_unary_expression_type(),
            },

            (ast::UnaryOp::Invert, Type::KnownInstance(KnownInstanceType::ConstraintSet(set))) => {
                let constraints = ConstraintSetBuilder::new();
                let result = constraints.into_owned(|constraints| {
                    let set = constraints.load(self.db(), set.constraints(self.db()));
                    set.negate(self.db(), constraints)
                });
                Type::KnownInstance(KnownInstanceType::ConstraintSet(
                    InternedConstraintSet::new(self.db(), result),
                ))
            }

            (ast::UnaryOp::Not, ty) => ty
                .try_bool(self.db())
                .unwrap_or_else(|err| {
                    err.report_diagnostic(&self.context, unary);
                    err.fallback_truthiness()
                })
                .negate()
                .into_type(self.db()),
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

                match tvar.typevar(self.db()).bound_or_constraints(self.db()) {
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        let db = self.db();
                        match Self::map_constrained_typevar_constraints(
                            db,
                            operand_type,
                            constraints,
                            |constraint| {
                                constraint
                                    .try_call_dunder(
                                        db,
                                        unary_dunder_method,
                                        CallArguments::none(),
                                        TypeContext::default(),
                                    )
                                    .map(|outcome| outcome.return_type(db))
                                    .ok()
                            },
                        ) {
                            Some(ty) => ty,
                            None => {
                                // At least one constraint failed; report error.
                                if let Some(builder) =
                                    self.context.report_lint(&UNSUPPORTED_OPERATOR, unary)
                                {
                                    builder.into_diagnostic(format_args!(
                                        "Unary operator `{op}` is not supported for object of type `{}`",
                                        operand_type.display(db),
                                    ));
                                }
                                operand_type
                                    .try_call_dunder(
                                        db,
                                        unary_dunder_method,
                                        CallArguments::none(),
                                        TypeContext::default(),
                                    )
                                    .map_or_else(
                                        |e| e.fallback_return_type(db),
                                        |b| b.return_type(db),
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
                    None => match operand_type.try_call_dunder(
                        self.db(),
                        unary_dunder_method,
                        CallArguments::none(),
                        TypeContext::default(),
                    ) {
                        Ok(outcome) => outcome.return_type(self.db()),
                        Err(e) => {
                            if let Some(builder) =
                                self.context.report_lint(&UNSUPPORTED_OPERATOR, unary)
                            {
                                builder.into_diagnostic(format_args!(
                                    "Unary operator `{op}` is not supported for object of type `{}`",
                                    operand_type.display(self.db()),
                                ));
                            }
                            e.fallback_return_type(self.db())
                        }
                    },
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
                | Type::Union(_)
                | Type::Intersection(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::BoundSuper(_)
                | Type::TypeIs(_)
                | Type::TypeGuard(_)
                | Type::TypedDict(_)
                | Type::NewTypeInstance(_),
            ) => fallback_unary_expression_type(),
        }
    }

    fn infer_boolean_expression(&mut self, bool_op: &ast::ExprBoolOp) -> Type<'db> {
        let ast::ExprBoolOp {
            range: _,
            node_index: _,
            op,
            values,
        } = bool_op;
        self.infer_chained_boolean_types(
            *op,
            values.iter().enumerate(),
            |builder, (index, value)| {
                let ty = if index == values.len() - 1 {
                    builder.infer_expression(value, TypeContext::default())
                } else {
                    builder.infer_maybe_standalone_expression(value, TypeContext::default())
                };

                (ty, value.range())
            },
        )
    }

    /// Computes the output of a chain of (one) boolean operation, consuming as input an iterator
    /// of operations and calling the `infer_ty` for each to infer their types.
    /// The iterator is consumed even if the boolean evaluation can be short-circuited,
    /// in order to ensure the invariant that all expressions are evaluated when inferring types.
    fn infer_chained_boolean_types<Iterator, Item, F>(
        &mut self,
        op: ast::BoolOp,
        operations: Iterator,
        infer_ty: F,
    ) -> Type<'db>
    where
        Iterator: IntoIterator<Item = Item>,
        F: Fn(&mut Self, Item) -> (Type<'db>, TextRange),
    {
        let mut done = false;
        let db = self.db();

        let elements = operations
            .into_iter()
            .with_position()
            .map(|(position, item)| {
                let (ty, range) = infer_ty(self, item);

                let is_last = matches!(
                    position,
                    itertools::Position::Last | itertools::Position::Only
                );

                if is_last {
                    if done { Type::Never } else { ty }
                } else {
                    let truthiness = ty.try_bool(self.db()).unwrap_or_else(|err| {
                        err.report_diagnostic(&self.context, range);
                        err.fallback_truthiness()
                    });

                    if done {
                        return Type::Never;
                    }

                    match (truthiness, op) {
                        (Truthiness::AlwaysTrue, ast::BoolOp::And) => Type::Never,
                        (Truthiness::AlwaysFalse, ast::BoolOp::Or) => Type::Never,

                        (Truthiness::AlwaysFalse, ast::BoolOp::And)
                        | (Truthiness::AlwaysTrue, ast::BoolOp::Or) => {
                            done = true;
                            ty
                        }

                        (Truthiness::Ambiguous, _) => IntersectionBuilder::new(db)
                            .add_positive(ty)
                            .add_negative(match op {
                                ast::BoolOp::And => Type::AlwaysTruthy,
                                ast::BoolOp::Or => Type::AlwaysFalsy,
                            })
                            .build(),
                    }
                }
            });

        UnionType::from_elements(db, elements)
    }

    fn infer_compare_expression(&mut self, compare: &ast::ExprCompare) -> Type<'db> {
        let ast::ExprCompare {
            range: _,
            node_index: _,
            left,
            ops,
            comparators,
        } = compare;

        self.infer_expression(left, TypeContext::default());

        // https://docs.python.org/3/reference/expressions.html#comparisons
        // > Formally, if `a, b, c, …, y, z` are expressions and `op1, op2, …, opN` are comparison
        // > operators, then `a op1 b op2 c ... y opN z` is equivalent to `a op1 b and b op2 c and
        // ... > y opN z`, except that each expression is evaluated at most once.
        //
        // As some operators (==, !=, <, <=, >, >=) *can* return an arbitrary type, the logic below
        // is shared with the one in `infer_binary_type_comparison`.
        self.infer_chained_boolean_types(
            ast::BoolOp::And,
            std::iter::once(&**left)
                .chain(comparators)
                .tuple_windows::<(_, _)>()
                .zip(ops),
            |builder, ((left, right), op)| {
                let left_ty = builder.expression_type(left);
                let right_ty = builder.infer_expression(right, TypeContext::default());

                let range = TextRange::new(left.start(), right.end());

                let ty = comparisons::infer_binary_type_comparison(
                    &builder.context,
                    left_ty,
                    *op,
                    right_ty,
                    range,
                    &BinaryComparisonVisitor::new(Ok(Type::bool_literal(true))),
                )
                .unwrap_or_else(|error| {
                    report_unsupported_comparison(
                        &builder.context,
                        &error,
                        range,
                        left,
                        right,
                        left_ty,
                        right_ty,
                    );

                    match op {
                        // `in, not in, is, is not` always return bool instances
                        ast::CmpOp::In | ast::CmpOp::NotIn | ast::CmpOp::Is | ast::CmpOp::IsNot => {
                            KnownClass::Bool.to_instance(builder.db())
                        }
                        // Other operators can return arbitrary types
                        _ => Type::unknown(),
                    }
                });

                (ty, range)
            },
        )
    }

    fn infer_type_parameters(&mut self, type_parameters: &ast::TypeParams) {
        let ast::TypeParams {
            range: _,
            node_index: _,
            type_params,
        } = type_parameters;
        for type_param in type_params {
            match type_param {
                ast::TypeParam::TypeVar(node) => self.infer_definition(node),
                ast::TypeParam::ParamSpec(node) => self.infer_definition(node),
                ast::TypeParam::TypeVarTuple(node) => self.infer_definition(node),
            }
        }
    }

    pub(super) fn finish_expression(mut self) -> ExpressionInference<'db> {
        self.infer_region();

        let Self {
            context,
            mut expressions,
            string_annotations,
            scope,
            bindings,
            declarations,
            deferred,
            cycle_recovery,
            all_definitely_bound,
            dataclass_field_specifiers: _,

            // Ignored; only relevant to definition regions
            undecorated_type: _,

            // builder only state
            typevar_binding_context: _,
            inference_flags: _,
            deferred_state: _,
            multi_inference_state: _,
            inner_expression_inference_state: _,
            inferring_vararg_annotation: _,
            called_functions: _,
            index: _,
            region: _,
            return_types_and_ranges: _,
        } = self;

        let diagnostics = context.finish();
        let _ = scope;

        assert!(
            declarations.is_empty(),
            "Expression region can't have declarations"
        );
        assert!(
            deferred.is_empty(),
            "Expression region can't have deferred definitions"
        );

        let extra =
            (!string_annotations.is_empty() || cycle_recovery.is_some() || !bindings.is_empty() || !diagnostics.is_empty() || !all_definitely_bound).then(|| {
                if bindings.len() > 20 {
                    tracing::debug!(
                        "Inferred expression region `{:?}` contains {} bindings. Lookups by linear scan might be slow.",
                        self.region,
                        bindings.len()
                    );
                }

                Box::new(ExpressionInferenceExtra {
                    string_annotations,
                    bindings: bindings.into_boxed_slice(),
                    diagnostics,
                    cycle_recovery,
                    all_definitely_bound,
                })
            });

        expressions.shrink_to_fit();

        ExpressionInference {
            expressions,
            extra,
            #[cfg(debug_assertions)]
            scope,
        }
    }

    pub(super) fn finish_definition(mut self) -> DefinitionInference<'db> {
        self.infer_region();

        let Self {
            context,
            mut expressions,
            string_annotations,
            scope,
            bindings,
            declarations,
            deferred,
            cycle_recovery,
            undecorated_type,
            called_functions,
            // builder only state
            dataclass_field_specifiers: _,
            all_definitely_bound: _,
            typevar_binding_context: _,
            inference_flags: _,
            deferred_state: _,
            inferring_vararg_annotation: _,
            multi_inference_state: _,
            inner_expression_inference_state: _,
            index: _,
            region: _,
            return_types_and_ranges: _,
        } = self;

        let _ = scope;
        let diagnostics = context.finish();

        let extra = (!diagnostics.is_empty()
            || !string_annotations.is_empty()
            || cycle_recovery.is_some()
            || undecorated_type.is_some()
            || !deferred.is_empty()
            || !called_functions.is_empty())
        .then(|| {
            Box::new(DefinitionInferenceExtra {
                string_annotations,
                called_functions: called_functions
                    .into_iter()
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
                cycle_recovery,
                deferred: deferred.into_boxed_slice(),
                diagnostics,
                undecorated_type,
            })
        });

        if bindings.len() > 20 {
            tracing::debug!(
                "Inferred definition region `{:?}` contains {} bindings. Lookups by linear scan might be slow.",
                self.region,
                bindings.len(),
            );
        }

        if declarations.len() > 20 {
            tracing::debug!(
                "Inferred declaration region `{:?}` contains {} declarations. Lookups by linear scan might be slow.",
                self.region,
                declarations.len(),
            );
        }

        expressions.shrink_to_fit();

        DefinitionInference {
            expressions,
            #[cfg(debug_assertions)]
            scope,
            bindings: bindings.into_boxed_slice(),
            declarations: declarations.into_boxed_slice(),
            extra,
        }
    }

    pub(super) fn finish_scope(mut self) -> ScopeInference<'db> {
        self.infer_region();

        let Self {
            context,
            string_annotations,
            mut expressions,
            scope,
            cycle_recovery,

            // Ignored, because scope types are never extended into other scopes.
            deferred: _,
            bindings: _,
            declarations: _,

            // Ignored; only relevant to definition regions
            undecorated_type: _,

            // Builder only state
            dataclass_field_specifiers: _,
            all_definitely_bound: _,
            typevar_binding_context: _,
            inference_flags: _,
            deferred_state: _,
            multi_inference_state: _,
            inner_expression_inference_state: _,
            inferring_vararg_annotation: _,
            called_functions: _,
            index: _,
            region: _,
            return_types_and_ranges: _,
        } = self;

        let _ = scope;
        let diagnostics = context.finish();

        let extra =
            (!string_annotations.is_empty() || !diagnostics.is_empty() || cycle_recovery.is_some())
                .then(|| {
                    Box::new(ScopeInferenceExtra {
                        string_annotations,
                        cycle_recovery,
                        diagnostics,
                    })
                });

        expressions.shrink_to_fit();

        ScopeInference { expressions, extra }
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
        let prev_multi_inference_state =
            builder.set_multi_inference_state(MultiInferenceState::Ignore);
        let was_in_multi_inference = builder.context.set_multi_inference(true);

        let value_ty = (self.infer_expr)(builder, tcx);
        self.last_tcx = Some(tcx);

        // Reset the multi-inference state.
        builder.set_multi_inference_state(prev_multi_inference_state);
        builder.context.set_multi_inference(was_in_multi_inference);

        value_ty
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

/// An iterator over arguments to a functional call.
#[derive(Clone)]
enum ArgumentsIter<'a> {
    FromAst(ArgumentsSourceOrder<'a>),
    Synthesized(std::slice::Iter<'a, ArgOrKeyword<'a>>),
}

impl<'a> ArgumentsIter<'a> {
    fn from_ast(arguments: &'a ast::Arguments) -> Self {
        Self::FromAst(arguments.arguments_source_order())
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

/// Dictates the behavior when an expression is inferred multiple times.
#[derive(Default, Debug, Clone, Copy)]
enum MultiInferenceState {
    /// Panic if the expression has already been inferred.
    #[default]
    Panic,

    /// Ignore the newly inferred value.
    Ignore,

    /// Store the intersection of all types inferred for the expression.
    Intersect,
}

impl MultiInferenceState {
    const fn is_panic(self) -> bool {
        matches!(self, MultiInferenceState::Panic)
    }
}

#[derive(Default, Debug, Clone, Copy)]
enum InnerExpressionInferenceState {
    #[default]
    Infer,
    Get,
}

impl InnerExpressionInferenceState {
    const fn is_get(self) -> bool {
        matches!(self, InnerExpressionInferenceState::Get)
    }
}

/// The deferred state of a specific expression in an inference region.
#[derive(Default, Debug, Clone, Copy)]
enum DeferredExpressionState {
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
    concatenated: Option<String>,
    contains_non_literal_str: bool,
}

impl StringPartsCollector {
    fn new() -> Self {
        Self {
            concatenated: Some(String::new()),
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

    fn string_type(self, db: &dyn Db) -> Type<'_> {
        if self.contains_non_literal_str {
            KnownClass::Str.to_instance(db)
        } else if let Some(concatenated) = self.concatenated {
            Type::string_literal(db, &concatenated)
        } else {
            Type::literal_string()
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
}

impl<K, V> VecMap<K, V>
where
    K: Eq,
    K: std::fmt::Debug,
    V: std::fmt::Debug,
{
    fn insert(&mut self, key: K, value: V, multi_inference_state: MultiInferenceState) {
        if matches!(multi_inference_state, MultiInferenceState::Ignore) {
            return;
        }

        debug_assert!(
            !self.0.iter().any(|(existing, _)| existing == &key),
            "An existing entry already exists for key {key:?}",
        );

        self.0.push((key, value));
    }

    #[inline]
    fn extend<T: IntoIterator<Item = (K, V)>>(
        &mut self,
        iter: T,
        multi_inference_state: MultiInferenceState,
    ) {
        if cfg!(debug_assertions) {
            for (key, value) in iter {
                self.insert(key, value, multi_inference_state);
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
    fn insert(&mut self, value: V, multi_inference_state: MultiInferenceState) {
        if matches!(multi_inference_state, MultiInferenceState::Ignore) {
            return;
        }

        debug_assert!(
            !self.0.iter().any(|existing| existing == &value),
            "An existing entry already exists for {value:?}",
        );

        self.0.push(value);
    }
}

impl<V> VecSet<V>
where
    V: Eq,
    V: std::fmt::Debug,
{
    #[inline]
    fn extend<T: IntoIterator<Item = V>>(
        &mut self,
        iter: T,
        multi_inference_state: MultiInferenceState,
    ) {
        if cfg!(debug_assertions) {
            for value in iter {
                self.insert(value, multi_inference_state);
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
struct AddBinding<'db, 'ast> {
    declared_ty: Option<Type<'db>>,
    binding: Definition<'db>,
    node: AnyNodeRef<'ast>,
    qualifiers: TypeQualifiers,
    is_local: bool,
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
        let declared_ty = self.declared_ty.unwrap_or(Type::unknown());

        let db = builder.db();
        let file_scope_id = self.binding.file_scope(db);
        let use_def = builder.index.use_def_map(file_scope_id);
        let place_table = builder.index.place_table(file_scope_id);

        let mut bound_ty = inferred_ty;

        if self.qualifiers.contains(TypeQualifiers::FINAL) {
            let mut previous_bindings = use_def.bindings_at_definition(self.binding);

            // An assignment to a local `Final`-qualified symbol is only an error if there are prior bindings

            let previous_definition = previous_bindings.find_map(|r| r.binding.definition());

            if !self.is_local || previous_definition.is_some() {
                let place = place_table.place(self.binding.place(db));
                if let Some(diag_builder) = builder.context.report_lint(
                    &INVALID_ASSIGNMENT,
                    self.binding.full_range(builder.db(), builder.module()),
                ) {
                    let mut diagnostic = diag_builder.into_diagnostic(format_args!(
                        "Reassignment of `Final` symbol `{place}` is not allowed"
                    ));

                    diagnostic.set_primary_message("Reassignment of `Final` symbol");

                    if let Some(previous_definition) = previous_definition {
                        // It is not very helpful to show the previous definition if it results from
                        // an import. Ideally, we would show the original definition in the external
                        // module, but that information is currently not threaded through attribute
                        // lookup.
                        if !previous_definition.kind(db).is_import() {
                            if let DefinitionKind::AnnotatedAssignment(assignment) =
                                previous_definition.kind(db)
                            {
                                let range = assignment.annotation(builder.module()).range();
                                diagnostic.annotate(
                                    builder
                                        .context
                                        .secondary(range)
                                        .message("Symbol declared as `Final` here"),
                                );
                            } else {
                                let range = previous_definition.full_range(db, builder.module());
                                diagnostic.annotate(
                                    builder
                                        .context
                                        .secondary(range)
                                        .message("Symbol declared as `Final` here"),
                                );
                            }
                            diagnostic.set_primary_message("Symbol later reassigned here");
                        }
                    }
                }
            }
        }

        if !bound_ty.is_assignable_to(db, declared_ty) {
            report_invalid_assignment(
                &builder.context,
                self.node,
                self.binding,
                declared_ty,
                bound_ty,
            );

            // Allow declarations to override inference in case of invalid assignment.
            bound_ty = declared_ty;
        }
        // In the following cases, the bound type may not be the same as the RHS value type.
        if let AnyNodeRef::ExprAttribute(ast::ExprAttribute { value, attr, .. }) = self.node {
            let value_ty = builder.try_expression_type(value).unwrap_or_else(|| {
                builder.infer_maybe_standalone_expression(value, TypeContext::default())
            });
            // If the member is a data descriptor, the RHS value may differ from the value actually assigned.
            if value_ty
                .class_member(db, attr.id.clone())
                .place
                .ignore_possibly_undefined()
                .is_some_and(|ty| ty.may_be_data_descriptor(db))
            {
                bound_ty = declared_ty;
            }
        } else if let AnyNodeRef::ExprSubscript(ast::ExprSubscript { value, .. }) = self.node {
            let value_ty = builder
                .try_expression_type(value)
                .unwrap_or_else(|| builder.infer_expression(value, TypeContext::default()));

            if !value_ty.is_typed_dict() && !Self::is_safe_mutable_class(db, value_ty) {
                bound_ty = declared_ty;
            }
        }

        builder
            .bindings
            .insert(self.binding, bound_ty, builder.multi_inference_state);

        inferred_ty
    }

    /// Arbitrary `__getitem__`/`__setitem__` methods on a class do not
    /// necessarily guarantee that the passed-in value for `__setitem__` is stored and
    /// can be retrieved unmodified via `__getitem__`. Therefore, we currently only
    /// perform assignment-based narrowing on a few built-in classes (`list`, `dict`,
    /// `bytesarray`, `TypedDict` and `collections` types) where we are confident that
    /// this kind of narrowing can be performed soundly. This is the same approach as
    /// pyright. TODO: Other standard library classes may also be considered safe. Also,
    /// subclasses of these safe classes that do not override `__getitem__/__setitem__`
    /// may be considered safe.
    fn is_safe_mutable_class(db: &'db dyn Db, ty: Type<'db>) -> bool {
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
            .map(|class| class.to_instance(db))
            .any(|safe_mutable_class| {
                ty.is_equivalent_to(db, safe_mutable_class)
                    || ty
                        .generic_origin(db)
                        .zip(safe_mutable_class.generic_origin(db))
                        .is_some_and(|(l, r)| l == r)
            })
    }
}

#[derive(Copy, Clone, Debug)]
enum BoundOrConstraintsNodes<'ast> {
    Bound(&'ast ast::Expr),
    Constraints(&'ast [ast::Expr]),
}

/// Report MRO errors for a dynamic class.
///
/// Returns `true` if the MRO is valid, `false` if there were errors.
pub(super) fn report_dynamic_mro_errors<'db>(
    context: &InferContext<'db, '_>,
    dynamic_class: DynamicClassLiteral<'db>,
    call_expr: &ast::ExprCall,
    bases: &ast::Expr,
) -> bool {
    let db = context.db();
    let Err(error) = dynamic_class.try_mro(db) else {
        return true;
    };

    let bases_tuple_elts = bases.as_tuple_expr().map(|tuple| tuple.elts.as_slice());

    match error.reason() {
        DynamicMroErrorKind::InvalidBases(invalid_bases) => {
            for (idx, base_type) in invalid_bases {
                // Check if the type is "type-like" (e.g., `type[Base]`).
                let instance_of_type = KnownClass::Type.to_instance(db);

                // Determine the diagnostic node; prefer specific base expr, fall back to bases.
                let specific_base = bases_tuple_elts.and_then(|elts| elts.get(*idx));
                let diagnostic_range = specific_base
                    .map(ast::Expr::range)
                    .unwrap_or_else(|| bases.range());

                if base_type.is_assignable_to(db, instance_of_type) {
                    if let Some(builder) =
                        context.report_lint(&UNSUPPORTED_DYNAMIC_BASE, diagnostic_range)
                    {
                        let mut diagnostic = builder.into_diagnostic("Unsupported class base");
                        diagnostic.set_primary_message(format_args!(
                            "Has type `{}`",
                            base_type.display(db)
                        ));
                        diagnostic.info(format_args!(
                            "ty cannot determine a MRO for class `{}` due to this base",
                            dynamic_class.name(db)
                        ));
                        diagnostic.info("Only class objects or `Any` are supported as class bases");
                    }
                } else if let Some(builder) = context.report_lint(&INVALID_BASE, diagnostic_range) {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Invalid class base with type `{}`",
                        base_type.display(db)
                    ));
                    if specific_base.is_none() {
                        diagnostic
                            .info(format_args!("Element {} of the tuple is invalid", idx + 1));
                    }
                }
            }
        }
        DynamicMroErrorKind::InheritanceCycle => {
            if let Some(builder) = context.report_lint(&CYCLIC_CLASS_DEFINITION, call_expr) {
                builder.into_diagnostic(format_args!(
                    "Cyclic definition of `{}`",
                    dynamic_class.name(db)
                ));
            }
        }
        DynamicMroErrorKind::DuplicateBases(duplicates) => {
            if let Some(builder) = context.report_lint(&DUPLICATE_BASE, call_expr) {
                builder.into_diagnostic(format_args!(
                    "Duplicate base class{maybe_s} {dupes} in class `{class}`",
                    maybe_s = if duplicates.len() == 1 { "" } else { "es" },
                    dupes = duplicates
                        .iter()
                        .map(|base: &ClassBase<'_>| base.display(db))
                        .join(", "),
                    class = dynamic_class.name(db),
                ));
            }
        }
        DynamicMroErrorKind::UnresolvableMro => {
            if let Some(builder) = context.report_lint(&INCONSISTENT_MRO, call_expr) {
                builder.into_diagnostic(format_args!(
                    "Cannot create a consistent method resolution order (MRO) \
                        for class `{}` with bases `[{}]`",
                    dynamic_class.name(db),
                    dynamic_class
                        .explicit_bases(db)
                        .iter()
                        .map(|base| base.display(db))
                        .join(", ")
                ));
            }
        }
    }

    false
}
