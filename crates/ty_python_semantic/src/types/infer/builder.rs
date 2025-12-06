use std::iter;

use itertools::{Either, EitherOrBoth, Itertools};
use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span};
use ruff_db::files::File;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_db::source::source_text;
use ruff_python_ast::visitor::{Visitor, walk_expr};
use ruff_python_ast::{
    self as ast, AnyNodeRef, ExprContext, HasNodeIndex, NodeIndex, PythonVersion,
};
use ruff_python_stdlib::builtins::version_builtin_was_added;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use super::{
    DefinitionInference, DefinitionInferenceExtra, ExpressionInference, ExpressionInferenceExtra,
    InferenceRegion, ScopeInference, ScopeInferenceExtra, infer_deferred_types,
    infer_definition_types, infer_expression_types, infer_same_file_expression_type,
    infer_unpack_types,
};
use crate::diagnostic::format_enumeration;
use crate::module_name::{ModuleName, ModuleNameResolutionError};
use crate::module_resolver::{
    KnownModule, ModuleResolveMode, file_to_module, resolve_module, search_paths,
};
use crate::node_key::NodeKey;
use crate::place::{
    ConsideredDefinitions, Definedness, LookupError, Place, PlaceAndQualifiers, TypeOrigin,
    builtins_module_scope, builtins_symbol, explicit_global_symbol, global_symbol,
    module_type_implicit_global_declaration, module_type_implicit_global_symbol, place,
    place_from_bindings, place_from_declarations, typing_extensions_symbol,
};
use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::ast_ids::{HasScopedUseId, ScopedUseId};
use crate::semantic_index::definition::{
    AnnotatedAssignmentDefinitionKind, AssignmentDefinitionKind, ComprehensionDefinitionKind,
    Definition, DefinitionKind, DefinitionNodeKey, DefinitionState, ExceptHandlerDefinitionKind,
    ForStmtDefinitionKind, TargetKind, WithItemDefinitionKind,
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
use crate::subscript::{PyIndex, PySlice};
use crate::types::call::bind::{CallableDescription, MatchingOverloadIndex};
use crate::types::call::{Binding, Bindings, CallArguments, CallError, CallErrorKind};
use crate::types::class::{CodeGeneratorKind, FieldKind, MetaclassErrorKind, MethodDecorator};
use crate::types::context::{InNoTypeCheck, InferContext};
use crate::types::cyclic::CycleDetector;
use crate::types::diagnostic::{
    self, CALL_NON_CALLABLE, CONFLICTING_DECLARATIONS, CONFLICTING_METACLASS,
    CYCLIC_CLASS_DEFINITION, CYCLIC_TYPE_ALIAS_DEFINITION, DIVISION_BY_ZERO, DUPLICATE_KW_ONLY,
    INCONSISTENT_MRO, INVALID_ARGUMENT_TYPE, INVALID_ASSIGNMENT, INVALID_ATTRIBUTE_ACCESS,
    INVALID_BASE, INVALID_DECLARATION, INVALID_GENERIC_CLASS, INVALID_KEY,
    INVALID_LEGACY_TYPE_VARIABLE, INVALID_METACLASS, INVALID_NAMED_TUPLE, INVALID_NEWTYPE,
    INVALID_OVERLOAD, INVALID_PARAMETER_DEFAULT, INVALID_PARAMSPEC, INVALID_PROTOCOL,
    INVALID_TYPE_ARGUMENTS, INVALID_TYPE_FORM, INVALID_TYPE_GUARD_CALL,
    INVALID_TYPE_VARIABLE_CONSTRAINTS, IncompatibleBases, NON_SUBSCRIPTABLE,
    POSSIBLY_MISSING_ATTRIBUTE, POSSIBLY_MISSING_IMPLICIT_CALL, POSSIBLY_MISSING_IMPORT,
    SUBCLASS_OF_FINAL_CLASS, UNDEFINED_REVEAL, UNRESOLVED_ATTRIBUTE, UNRESOLVED_GLOBAL,
    UNRESOLVED_IMPORT, UNRESOLVED_REFERENCE, UNSUPPORTED_OPERATOR, USELESS_OVERLOAD_BODY,
    hint_if_stdlib_attribute_exists_on_other_versions,
    hint_if_stdlib_submodule_exists_on_other_versions, report_attempted_protocol_instantiation,
    report_bad_dunder_set_call, report_cannot_pop_required_field_on_typed_dict,
    report_duplicate_bases, report_implicit_return_type, report_index_out_of_bounds,
    report_instance_layout_conflict, report_invalid_arguments_to_annotated,
    report_invalid_assignment, report_invalid_attribute_assignment,
    report_invalid_exception_caught, report_invalid_exception_cause,
    report_invalid_exception_raised, report_invalid_exception_tuple_caught,
    report_invalid_generator_function_return_type, report_invalid_key_on_typed_dict,
    report_invalid_or_unsupported_base, report_invalid_return_type,
    report_invalid_type_checking_constant, report_named_tuple_field_with_leading_underscore,
    report_namedtuple_field_without_default_after_field_with_default, report_non_subscriptable,
    report_possibly_missing_attribute, report_possibly_unresolved_reference,
    report_rebound_typevar, report_slice_step_size_zero, report_unsupported_comparison,
};
use crate::types::function::{
    FunctionDecorators, FunctionLiteral, FunctionType, KnownFunction, OverloadLiteral,
    is_implicit_classmethod, is_implicit_staticmethod,
};
use crate::types::generics::{
    GenericContext, InferableTypeVars, LegacyGenericBase, SpecializationBuilder, bind_typevar,
    enclosing_generic_contexts, typing_self,
};
use crate::types::infer::nearest_enclosing_function;
use crate::types::instance::SliceLiteral;
use crate::types::mro::MroErrorKind;
use crate::types::newtype::NewType;
use crate::types::subclass_of::SubclassOfInner;
use crate::types::tuple::{Tuple, TupleLength, TupleSpec, TupleType};
use crate::types::typed_dict::{
    TypedDictAssignmentKind, validate_typed_dict_constructor, validate_typed_dict_dict_literal,
    validate_typed_dict_key_assignment,
};
use crate::types::visitor::any_over_type;
use crate::types::{
    BoundTypeVarInstance, CallDunderError, CallableBinding, CallableType, CallableTypeKind,
    ClassLiteral, ClassType, DataclassParams, DynamicType, InternedType, IntersectionBuilder,
    IntersectionType, KnownClass, KnownInstanceType, LintDiagnosticGuard, MemberLookupPolicy,
    MetaclassCandidate, PEP695TypeAliasType, ParamSpecAttrKind, Parameter, ParameterForm,
    Parameters, Signature, SpecialFormType, SubclassOfType, TrackedConstraintSet, Truthiness, Type,
    TypeAliasType, TypeAndQualifiers, TypeContext, TypeQualifiers, TypeVarBoundOrConstraints,
    TypeVarBoundOrConstraintsEvaluation, TypeVarDefaultEvaluation, TypeVarIdentity,
    TypeVarInstance, TypeVarKind, TypeVarVariance, TypedDictType, UnionBuilder, UnionType,
    UnionTypeInstance, binding_type, infer_scope_types, todo_type,
};
use crate::types::{CallableTypes, overrides};
use crate::types::{ClassBase, add_inferred_python_version_hint_to_diagnostic};
use crate::unpack::{EvaluationMode, UnpackPosition};
use crate::{Db, FxIndexSet, FxOrderSet, Program};

mod annotation_expression;
mod type_expression;

/// Whether the intersection type is on the left or right side of the comparison.
#[derive(Debug, Clone, Copy)]
enum IntersectionOn {
    Left,
    Right,
}

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

/// A [`CycleDetector`] that is used in `infer_binary_type_comparison`.
type BinaryComparisonVisitor<'db> = CycleDetector<
    ast::CmpOp,
    (Type<'db>, ast::CmpOp, Type<'db>),
    Result<Type<'db>, UnsupportedComparisonError<'db>>,
>;

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
    /// This is mainly used in [`check_overloaded_functions`] to check an overloaded function that
    /// is shadowed by a function with the same name in this scope but has been called before. For
    /// example:
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
    ///
    /// [`check_overloaded_functions`]: TypeInferenceBuilder::check_overloaded_functions
    called_functions: FxIndexSet<FunctionType<'db>>,

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

    multi_inference_state: MultiInferenceState,

    /// If you cannot avoid the possibility of calling `infer(_type)_expression` multiple times for a given expression,
    /// set this to `Get` after the expression has been inferred for the first time.
    /// While this is `Get`, any expressions will be considered to have already been inferred.
    inner_expression_inference_state: InnerExpressionInferenceState,

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
            multi_inference_state: MultiInferenceState::Panic,
            inner_expression_inference_state: InnerExpressionInferenceState::Infer,
            expressions: FxHashMap::default(),
            string_annotations: FxHashSet::default(),
            bindings: VecMap::default(),
            declarations: VecMap::default(),
            typevar_binding_context: None,
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
                        Some(UnionType::from_elements(self.db(), [existing, other]));
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
            InferenceRegion::Scope(scope) if scope == expr_scope => {
                self.expression_type(expression)
            }
            _ => infer_scope_types(self.db(), expr_scope).expression_type(expression),
        }
    }

    /// Infers types in the given [`InferenceRegion`].
    fn infer_region(&mut self) {
        match self.region {
            InferenceRegion::Scope(scope) => self.infer_region_scope(scope),
            InferenceRegion::Definition(definition) => self.infer_region_definition(definition),
            InferenceRegion::Deferred(definition) => self.infer_region_deferred(definition),
            InferenceRegion::Expression(expression, tcx) => {
                self.infer_region_expression(expression, tcx);
            }
        }
    }

    fn infer_region_scope(&mut self, scope: ScopeId<'db>) {
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
                self.infer_list_comprehension_expression_scope(comprehension.node(self.module()));
            }
            NodeWithScopeKind::SetComprehension(comprehension) => {
                self.infer_set_comprehension_expression_scope(comprehension.node(self.module()));
            }
            NodeWithScopeKind::DictComprehension(comprehension) => {
                self.infer_dict_comprehension_expression_scope(comprehension.node(self.module()));
            }
            NodeWithScopeKind::GeneratorExpression(generator) => {
                self.infer_generator_expression_scope(generator.node(self.module()));
            }
        }

        // Infer deferred types for all definitions.
        for definition in std::mem::take(&mut self.deferred) {
            self.extend_definition(infer_deferred_types(self.db(), definition));
        }

        assert!(
            self.deferred.is_empty(),
            "Inferring deferred types should not add more deferred definitions"
        );

        if self.db().should_check_file(self.file()) {
            self.check_class_definitions();
            self.check_overloaded_functions(node);
        }
    }

    /// Iterate over all class definitions to check that the definition will not cause an exception
    /// to be raised at runtime. This needs to be done after most other types in the scope have been
    /// inferred, due to the fact that base classes can be deferred. If it looks like a class
    /// definition is invalid in some way, issue a diagnostic.
    ///
    /// Among the things we check for in this method are whether Python will be able to determine a
    /// consistent "[method resolution order]" and [metaclass] for each class.
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    /// [metaclass]: https://docs.python.org/3/reference/datamodel.html#metaclasses
    fn check_class_definitions(&mut self) {
        let class_definitions = self.declarations.iter().filter_map(|(definition, ty)| {
            // Filter out class literals that result from imports
            if let DefinitionKind::Class(class) = definition.kind(self.db()) {
                ty.inner_type()
                    .as_class_literal()
                    .map(|class_literal| (class_literal, class.node(self.module())))
            } else {
                None
            }
        });

        // Iterate through all class definitions in this scope.
        for (class, class_node) in class_definitions {
            // (1) Check that the class does not have a cyclic definition
            if let Some(inheritance_cycle) = class.inheritance_cycle(self.db()) {
                if inheritance_cycle.is_participant() {
                    if let Some(builder) = self
                        .context
                        .report_lint(&CYCLIC_CLASS_DEFINITION, class_node)
                    {
                        builder.into_diagnostic(format_args!(
                            "Cyclic definition of `{}` (class cannot inherit from itself)",
                            class.name(self.db())
                        ));
                    }
                }
                // If a class is cyclically defined, that's a sufficient error to report; the
                // following checks (which are all inheritance-based) aren't even relevant.
                continue;
            }

            let is_named_tuple = CodeGeneratorKind::NamedTuple.matches(self.db(), class, None);

            // (2) If it's a `NamedTuple` class, check that no field without a default value
            // appears after a field with a default value.
            if is_named_tuple {
                let mut field_with_default_encountered = None;

                for (field_name, field) in
                    class.own_fields(self.db(), None, CodeGeneratorKind::NamedTuple)
                {
                    if field_name.starts_with('_') {
                        report_named_tuple_field_with_leading_underscore(
                            &self.context,
                            class,
                            &field_name,
                            field.first_declaration,
                        );
                    }

                    if matches!(
                        field.kind,
                        FieldKind::NamedTuple {
                            default_ty: Some(_)
                        }
                    ) {
                        field_with_default_encountered =
                            Some((field_name, field.first_declaration));
                    } else if let Some(field_with_default) = field_with_default_encountered.as_ref()
                    {
                        report_namedtuple_field_without_default_after_field_with_default(
                            &self.context,
                            class,
                            (&field_name, field.first_declaration),
                            field_with_default,
                        );
                    }
                }
            }

            let is_protocol = class.is_protocol(self.db());

            let mut disjoint_bases = IncompatibleBases::default();

            // (3) Iterate through the class's explicit bases to check for various possible errors:
            //     - Check for inheritance from plain `Generic`,
            //     - Check for inheritance from a `@final` classes
            //     - If the class is a protocol class: check for inheritance from a non-protocol class
            //     - If the class is a NamedTuple class: check for multiple inheritance that isn't `Generic[]`
            for (i, base_class) in class.explicit_bases(self.db()).iter().enumerate() {
                if is_named_tuple
                    && !matches!(
                        base_class,
                        Type::SpecialForm(SpecialFormType::NamedTuple)
                            | Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(_))
                    )
                {
                    if let Some(builder) = self
                        .context
                        .report_lint(&INVALID_NAMED_TUPLE, &class_node.bases()[i])
                    {
                        builder.into_diagnostic(format_args!(
                            "NamedTuple class `{}` cannot use multiple inheritance except with `Generic[]`",
                            class.name(self.db()),
                        ));
                    }
                }

                let base_class = match base_class {
                    Type::SpecialForm(SpecialFormType::Generic) => {
                        if let Some(builder) = self
                            .context
                            .report_lint(&INVALID_BASE, &class_node.bases()[i])
                        {
                            // Unsubscripted `Generic` can appear in the MRO of many classes,
                            // but it is never valid as an explicit base class in user code.
                            builder.into_diagnostic("Cannot inherit from plain `Generic`");
                        }
                        continue;
                    }
                    // Note that unlike several of the other errors caught in this function,
                    // this does not lead to the class creation failing at runtime,
                    // but it is semantically invalid.
                    Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(_)) => {
                        if class_node.type_params.is_none() {
                            continue;
                        }
                        let Some(builder) = self
                            .context
                            .report_lint(&INVALID_GENERIC_CLASS, &class_node.bases()[i])
                        else {
                            continue;
                        };
                        builder.into_diagnostic(
                            "Cannot both inherit from subscripted `Protocol` \
                            and use PEP 695 type variables",
                        );
                        continue;
                    }
                    Type::ClassLiteral(class) => ClassType::NonGeneric(*class),
                    Type::GenericAlias(class) => ClassType::Generic(*class),
                    _ => continue,
                };

                if let Some(disjoint_base) = base_class.nearest_disjoint_base(self.db()) {
                    disjoint_bases.insert(disjoint_base, i, base_class.class_literal(self.db()).0);
                }

                if is_protocol
                    && !(base_class.is_protocol(self.db()) || base_class.is_object(self.db()))
                {
                    if let Some(builder) = self
                        .context
                        .report_lint(&INVALID_PROTOCOL, &class_node.bases()[i])
                    {
                        builder.into_diagnostic(format_args!(
                            "Protocol class `{}` cannot inherit from non-protocol class `{}`",
                            class.name(self.db()),
                            base_class.name(self.db()),
                        ));
                    }
                }

                if base_class.is_final(self.db()) {
                    if let Some(builder) = self
                        .context
                        .report_lint(&SUBCLASS_OF_FINAL_CLASS, &class_node.bases()[i])
                    {
                        builder.into_diagnostic(format_args!(
                            "Class `{}` cannot inherit from final class `{}`",
                            class.name(self.db()),
                            base_class.name(self.db()),
                        ));
                    }
                }
            }

            // (4) Check that the class's MRO is resolvable
            match class.try_mro(self.db(), None) {
                Err(mro_error) => match mro_error.reason() {
                    MroErrorKind::DuplicateBases(duplicates) => {
                        let base_nodes = class_node.bases();
                        for duplicate in duplicates {
                            report_duplicate_bases(&self.context, class, duplicate, base_nodes);
                        }
                    }
                    MroErrorKind::InvalidBases(bases) => {
                        let base_nodes = class_node.bases();
                        for (index, base_ty) in bases {
                            report_invalid_or_unsupported_base(
                                &self.context,
                                &base_nodes[*index],
                                *base_ty,
                                class,
                            );
                        }
                    }
                    MroErrorKind::UnresolvableMro { bases_list } => {
                        if let Some(builder) =
                            self.context.report_lint(&INCONSISTENT_MRO, class_node)
                        {
                            builder.into_diagnostic(format_args!(
                                "Cannot create a consistent method resolution order (MRO) \
                                    for class `{}` with bases list `[{}]`",
                                class.name(self.db()),
                                bases_list
                                    .iter()
                                    .map(|base| base.display(self.db()))
                                    .join(", ")
                            ));
                        }
                    }
                    MroErrorKind::Pep695ClassWithGenericInheritance => {
                        if let Some(builder) =
                            self.context.report_lint(&INVALID_GENERIC_CLASS, class_node)
                        {
                            builder.into_diagnostic(
                                "Cannot both inherit from `typing.Generic` \
                                and use PEP 695 type variables",
                            );
                        }
                    }
                    MroErrorKind::InheritanceCycle => {
                        if let Some(builder) = self
                            .context
                            .report_lint(&CYCLIC_CLASS_DEFINITION, class_node)
                        {
                            builder.into_diagnostic(format_args!(
                                "Cyclic definition of `{}` (class cannot inherit from itself)",
                                class.name(self.db())
                            ));
                        }
                    }
                },
                Ok(_) => {
                    disjoint_bases.remove_redundant_entries(self.db());

                    if disjoint_bases.len() > 1 {
                        report_instance_layout_conflict(
                            &self.context,
                            class,
                            class_node,
                            &disjoint_bases,
                        );
                    }
                }
            }

            // (5) Check that the class's metaclass can be determined without error.
            if let Err(metaclass_error) = class.try_metaclass(self.db()) {
                match metaclass_error.reason() {
                    MetaclassErrorKind::Cycle => {
                        if let Some(builder) = self
                            .context
                            .report_lint(&CYCLIC_CLASS_DEFINITION, class_node)
                        {
                            builder.into_diagnostic(format_args!(
                                "Cyclic definition of `{}`",
                                class.name(self.db())
                            ));
                        }
                    }
                    MetaclassErrorKind::NotCallable(ty) => {
                        if let Some(builder) =
                            self.context.report_lint(&INVALID_METACLASS, class_node)
                        {
                            builder.into_diagnostic(format_args!(
                                "Metaclass type `{}` is not callable",
                                ty.display(self.db())
                            ));
                        }
                    }
                    MetaclassErrorKind::PartlyNotCallable(ty) => {
                        if let Some(builder) =
                            self.context.report_lint(&INVALID_METACLASS, class_node)
                        {
                            builder.into_diagnostic(format_args!(
                                "Metaclass type `{}` is partly not callable",
                                ty.display(self.db())
                            ));
                        }
                    }
                    MetaclassErrorKind::Conflict {
                        candidate1:
                            MetaclassCandidate {
                                metaclass: metaclass1,
                                explicit_metaclass_of: class1,
                            },
                        candidate2:
                            MetaclassCandidate {
                                metaclass: metaclass2,
                                explicit_metaclass_of: class2,
                            },
                        candidate1_is_base_class,
                    } => {
                        if let Some(builder) =
                            self.context.report_lint(&CONFLICTING_METACLASS, class_node)
                        {
                            if *candidate1_is_base_class {
                                builder.into_diagnostic(format_args!(
                                    "The metaclass of a derived class (`{class}`) \
                                     must be a subclass of the metaclasses of all its bases, \
                                     but `{metaclass1}` (metaclass of base class `{base1}`) \
                                     and `{metaclass2}` (metaclass of base class `{base2}`) \
                                     have no subclass relationship",
                                    class = class.name(self.db()),
                                    metaclass1 = metaclass1.name(self.db()),
                                    base1 = class1.name(self.db()),
                                    metaclass2 = metaclass2.name(self.db()),
                                    base2 = class2.name(self.db()),
                                ));
                            } else {
                                builder.into_diagnostic(format_args!(
                                    "The metaclass of a derived class (`{class}`) \
                                     must be a subclass of the metaclasses of all its bases, \
                                     but `{metaclass_of_class}` (metaclass of `{class}`) \
                                     and `{metaclass_of_base}` (metaclass of base class `{base}`) \
                                     have no subclass relationship",
                                    class = class.name(self.db()),
                                    metaclass_of_class = metaclass1.name(self.db()),
                                    metaclass_of_base = metaclass2.name(self.db()),
                                    base = class2.name(self.db()),
                                ));
                            }
                        }
                    }
                }
            }

            // (6) If the class is generic, verify that its generic context does not violate any of
            // the typevar scoping rules.
            if let (Some(legacy), Some(inherited)) = (
                class.legacy_generic_context(self.db()),
                class.inherited_legacy_generic_context(self.db()),
            ) {
                if !inherited.is_subset_of(self.db(), legacy) {
                    if let Some(builder) =
                        self.context.report_lint(&INVALID_GENERIC_CLASS, class_node)
                    {
                        builder.into_diagnostic(
                            "`Generic` base class must include all type \
                            variables used in other base classes",
                        );
                    }
                }
            }

            let scope = class.body_scope(self.db()).scope(self.db());
            if self.context.is_lint_enabled(&INVALID_GENERIC_CLASS)
                && let Some(parent) = scope.parent()
            {
                for self_typevar in class.typevars_referenced_in_definition(self.db()) {
                    let self_typevar_name = self_typevar.typevar(self.db()).name(self.db());
                    for enclosing in enclosing_generic_contexts(self.db(), self.index, parent) {
                        if let Some(other_typevar) =
                            enclosing.binds_named_typevar(self.db(), self_typevar_name)
                        {
                            report_rebound_typevar(
                                &self.context,
                                self_typevar_name,
                                class,
                                class_node,
                                other_typevar,
                            );
                        }
                    }
                }
            }

            // (7) Check that a dataclass does not have more than one `KW_ONLY`.
            if let Some(field_policy @ CodeGeneratorKind::DataclassLike(_)) =
                CodeGeneratorKind::from_class(self.db(), class, None)
            {
                let specialization = None;

                let kw_only_sentinel_fields: Vec<_> = class
                    .fields(self.db(), specialization, field_policy)
                    .iter()
                    .filter_map(|(name, field)| {
                        field.is_kw_only_sentinel(self.db()).then_some(name)
                    })
                    .collect();

                if kw_only_sentinel_fields.len() > 1 {
                    // TODO: The fields should be displayed in a subdiagnostic.
                    if let Some(builder) = self
                        .context
                        .report_lint(&DUPLICATE_KW_ONLY, &class_node.name)
                    {
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Dataclass has more than one field annotated with `KW_ONLY`"
                        ));

                        diagnostic.info(format_args!(
                            "`KW_ONLY` fields: {}",
                            kw_only_sentinel_fields
                                .iter()
                                .map(|name| format!("`{name}`"))
                                .join(", ")
                        ));
                    }
                }
            }

            // (8) Check for violations of the Liskov Substitution Principle,
            // and for violations of other rules relating to invalid overrides of some sort.
            overrides::check_class(&self.context, class);

            if let Some(protocol) = class.into_protocol_class(self.db()) {
                protocol.validate_members(&self.context);
            }
        }
    }

    /// Check the overloaded functions in this scope.
    ///
    /// This only checks the overloaded functions that are:
    /// 1. Visible publicly at the end of this scope
    /// 2. Or, defined and called in this scope
    ///
    /// For (1), this has the consequence of not checking an overloaded function that is being
    /// shadowed by another function with the same name in this scope.
    fn check_overloaded_functions(&mut self, scope: &NodeWithScopeKind) {
        // Collect all the unique overloaded function places in this scope. This requires a set
        // because an overloaded function uses the same place for each of the overloads and the
        // implementation.
        let overloaded_function_places: FxIndexSet<_> = self
            .declarations
            .iter()
            .filter_map(|(definition, ty)| {
                // Filter out function literals that result from anything other than a function
                // definition e.g., imports which would create a cross-module AST dependency.
                if !matches!(definition.kind(self.db()), DefinitionKind::Function(_)) {
                    return None;
                }
                let function = ty.inner_type().as_function_literal()?;
                if function.has_known_decorator(self.db(), FunctionDecorators::OVERLOAD) {
                    Some(definition.place(self.db()))
                } else {
                    None
                }
            })
            .collect();

        let use_def = self
            .index
            .use_def_map(self.scope().file_scope_id(self.db()));

        let mut public_functions = FxIndexSet::default();

        for place in overloaded_function_places {
            if let Place::Defined(Type::FunctionLiteral(function), _, Definedness::AlwaysDefined) =
                place_from_bindings(
                    self.db(),
                    use_def.end_of_scope_symbol_bindings(place.as_symbol().unwrap()),
                )
                .place
            {
                if function.file(self.db()) != self.file() {
                    // If the function is not in this file, we don't need to check it.
                    // https://github.com/astral-sh/ruff/pull/17609#issuecomment-2839445740
                    continue;
                }

                // Extend the functions that we need to check with the publicly visible overloaded
                // function. This is always going to be either the implementation or the last
                // overload if the implementation doesn't exists.
                public_functions.insert(function);
            }
        }

        for function in self.called_functions.union(&public_functions) {
            let (overloads, implementation) = function.overloads_and_implementation(self.db());
            if overloads.is_empty() {
                continue;
            }

            // Check that the overloaded function has at least two overloads
            if let [single_overload] = overloads {
                let function_node = function.node(self.db(), self.file(), self.module());
                if let Some(builder) = self
                    .context
                    .report_lint(&INVALID_OVERLOAD, &function_node.name)
                {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Overloaded function `{}` requires at least two overloads",
                        &function_node.name
                    ));
                    diagnostic.annotate(
                        self.context
                            .secondary(single_overload.focus_range(self.db(), self.module()))
                            .message(format_args!("Only one overload defined here")),
                    );
                }
            }

            // Check that the overloaded function has an implementation. Overload definitions
            // within stub files, protocols, and on abstract methods within abstract base classes
            // are exempt from this check.
            if implementation.is_none() && !self.in_stub() {
                let mut implementation_required = true;

                if let NodeWithScopeKind::Class(class_node_ref) = scope {
                    let class = binding_type(
                        self.db(),
                        self.index
                            .expect_single_definition(class_node_ref.node(self.module())),
                    )
                    .expect_class_literal();

                    if class.is_protocol(self.db())
                        || (class.is_abstract(self.db())
                            && overloads.iter().all(|overload| {
                                overload.has_known_decorator(
                                    self.db(),
                                    FunctionDecorators::ABSTRACT_METHOD,
                                )
                            }))
                    {
                        implementation_required = false;
                    }
                }

                if implementation_required {
                    let function_node = function.node(self.db(), self.file(), self.module());
                    if let Some(builder) = self
                        .context
                        .report_lint(&INVALID_OVERLOAD, &function_node.name)
                    {
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Overloads for function `{}` must be followed by a non-`@overload`-decorated implementation function",
                            &function_node.name
                        ));
                        diagnostic.info(format_args!(
                            "Attempting to call `{}` will raise `TypeError` at runtime",
                            &function_node.name
                        ));
                        diagnostic.info(
                            "Overloaded functions without implementations are only permitted \
                            in stub files, on protocols, or for abstract methods",
                        );
                        diagnostic.info(
                            "See https://docs.python.org/3/library/typing.html#typing.overload \
                            for more details",
                        );
                    }
                }
            }

            for (decorator, name) in [
                (FunctionDecorators::CLASSMETHOD, "classmethod"),
                (FunctionDecorators::STATICMETHOD, "staticmethod"),
            ] {
                let mut decorator_present = false;
                let mut decorator_missing = vec![];

                for function in overloads.iter().chain(implementation.as_ref()) {
                    if function.has_known_decorator(self.db(), decorator) {
                        decorator_present = true;
                    } else {
                        decorator_missing.push(function);
                    }
                }

                if !decorator_present {
                    // Both overloads and implementation does not have the decorator
                    continue;
                }
                if decorator_missing.is_empty() {
                    // All overloads and implementation have the decorator
                    continue;
                }

                let function_node = function.node(self.db(), self.file(), self.module());
                if let Some(builder) = self
                    .context
                    .report_lint(&INVALID_OVERLOAD, &function_node.name)
                {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Overloaded function `{}` does not use the `@{name}` decorator \
                         consistently",
                        &function_node.name
                    ));
                    for function in decorator_missing {
                        diagnostic.annotate(
                            self.context
                                .secondary(function.focus_range(self.db(), self.module()))
                                .message(format_args!("Missing here")),
                        );
                    }
                }
            }

            for (decorator, name) in [
                (FunctionDecorators::FINAL, "final"),
                (FunctionDecorators::OVERRIDE, "override"),
            ] {
                if let Some(implementation) = implementation {
                    for overload in overloads {
                        if !overload.has_known_decorator(self.db(), decorator) {
                            continue;
                        }
                        let function_node = function.node(self.db(), self.file(), self.module());
                        let Some(builder) = self
                            .context
                            .report_lint(&INVALID_OVERLOAD, &function_node.name)
                        else {
                            continue;
                        };
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "`@{name}` decorator should be applied only to the \
                                overload implementation"
                        ));
                        diagnostic.annotate(
                            self.context
                                .secondary(implementation.focus_range(self.db(), self.module()))
                                .message(format_args!("Implementation defined here")),
                        );
                    }
                } else {
                    let mut overloads = overloads.iter();
                    let Some(first_overload) = overloads.next() else {
                        continue;
                    };
                    for overload in overloads {
                        if !overload.has_known_decorator(self.db(), decorator) {
                            continue;
                        }
                        let function_node = function.node(self.db(), self.file(), self.module());
                        let Some(builder) = self
                            .context
                            .report_lint(&INVALID_OVERLOAD, &function_node.name)
                        else {
                            continue;
                        };
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "`@{name}` decorator should be applied only to the \
                                first overload"
                        ));
                        diagnostic.annotate(
                            self.context
                                .secondary(first_overload.focus_range(self.db(), self.module()))
                                .message(format_args!("First overload defined here")),
                        );
                    }
                }
            }
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
                self.infer_assignment_deferred(assignment.value(self.module()));
            }
            _ => {}
        }
    }

    fn infer_region_expression(&mut self, expression: Expression<'db>, tcx: TypeContext<'db>) {
        match expression.kind(self.db()) {
            ExpressionKind::Normal => {
                self.infer_expression_impl(expression.node_ref(self.db(), self.module()), tcx);
            }
            ExpressionKind::TypeExpression => {
                self.infer_type_expression(expression.node_ref(self.db(), self.module()));
            }
        }
    }

    /// Raise a diagnostic if the given type cannot be divided by zero.
    ///
    /// Expects the resolved type of the left side of the binary expression.
    fn check_division_by_zero(
        &mut self,
        node: AnyNodeRef<'_>,
        op: ast::Operator,
        left: Type<'db>,
    ) -> bool {
        match left {
            Type::BooleanLiteral(_) | Type::IntLiteral(_) => {}
            Type::NominalInstance(instance)
                if matches!(
                    instance.known_class(self.db()),
                    Some(KnownClass::Float | KnownClass::Int | KnownClass::Bool)
                ) => {}
            _ => return false,
        }

        let (op, by_zero) = match op {
            ast::Operator::Div => ("divide", "by zero"),
            ast::Operator::FloorDiv => ("floor divide", "by zero"),
            ast::Operator::Mod => ("reduce", "modulo zero"),
            _ => return false,
        };

        if let Some(builder) = self.context.report_lint(&DIVISION_BY_ZERO, node) {
            builder.into_diagnostic(format_args!(
                "Cannot {op} object of type `{}` {by_zero}",
                left.display(self.db())
            ));
        }

        true
    }

    /// Add a binding for the given definition.
    ///
    /// Returns the result of the `infer_value_ty` closure, which is called with the declared type
    /// as type context.
    fn add_binding(
        &mut self,
        node: AnyNodeRef,
        binding: Definition<'db>,
        infer_value_ty: impl FnOnce(&mut Self, TypeContext<'db>) -> Type<'db>,
    ) -> Type<'db> {
        /// Arbitrary `__getitem__`/`__setitem__` methods on a class do not
        /// necessarily guarantee that the passed-in value for `__setitem__` is stored and
        /// can be retrieved unmodified via `__getitem__`. Therefore, we currently only
        /// perform assignment-based narrowing on a few built-in classes (`list`, `dict`,
        /// `bytesarray`, `TypedDict` and `collections` types) where we are confident that
        /// this kind of narrowing can be performed soundly. This is the same approach as
        /// pyright. TODO: Other standard library classes may also be considered safe. Also,
        /// subclasses of these safe classes that do not override `__getitem__/__setitem__`
        /// may be considered safe.
        fn is_safe_mutable_class<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
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

        debug_assert!(
            binding
                .kind(self.db())
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
        if !place_and_quals.place.is_definitely_bound() {
            if let PlaceExprRef::Symbol(symbol) = place {
                let symbol_id = place_id.expect_symbol();

                if self.skip_non_global_scopes(file_scope_id, symbol_id)
                    || self.scope.file_scope_id(self.db()).is_global()
                {
                    place_and_quals = place_and_quals.or_fall_back_to(self.db(), || {
                        module_type_implicit_global_declaration(self.db(), symbol.name())
                    });
                }
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
                    if let Place::Defined(ty, _, Definedness::AlwaysDefined) =
                        value_type.member(db, attr).place
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

        let inferred_ty = infer_value_ty(self, TypeContext::new(declared_ty));

        let declared_ty = declared_ty.unwrap_or(Type::unknown());
        let mut bound_ty = inferred_ty;

        if qualifiers.contains(TypeQualifiers::FINAL) {
            let mut previous_bindings = use_def.bindings_at_definition(binding);

            // An assignment to a local `Final`-qualified symbol is only an error if there are prior bindings

            let previous_definition = previous_bindings
                .next()
                .and_then(|r| r.binding.definition());

            if !is_local || previous_definition.is_some() {
                let place = place_table.place(binding.place(db));
                if let Some(builder) = self.context.report_lint(
                    &INVALID_ASSIGNMENT,
                    binding.full_range(self.db(), self.module()),
                ) {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
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
                                let range = assignment.annotation(self.module()).range();
                                diagnostic.annotate(
                                    self.context
                                        .secondary(range)
                                        .message("Symbol declared as `Final` here"),
                                );
                            } else {
                                let range =
                                    previous_definition.full_range(self.db(), self.module());
                                diagnostic.annotate(
                                    self.context
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
            report_invalid_assignment(&self.context, node, binding, declared_ty, bound_ty);

            // Allow declarations to override inference in case of invalid assignment.
            bound_ty = declared_ty;
        }
        // In the following cases, the bound type may not be the same as the RHS value type.
        if let AnyNodeRef::ExprAttribute(ast::ExprAttribute { value, attr, .. }) = node {
            let value_ty = self.try_expression_type(value).unwrap_or_else(|| {
                self.infer_maybe_standalone_expression(value, TypeContext::default())
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
        } else if let AnyNodeRef::ExprSubscript(ast::ExprSubscript { value, .. }) = node {
            let value_ty = self
                .try_expression_type(value)
                .unwrap_or_else(|| self.infer_expression(value, TypeContext::default()));

            if !value_ty.is_typed_dict() && !is_safe_mutable_class(db, value_ty) {
                bound_ty = declared_ty;
            }
        }

        self.bindings
            .insert(binding, bound_ty, self.multi_inference_state);

        inferred_ty
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
                if let PlaceExprRef::Symbol(symbol) = &place {
                    if scope.is_global() {
                        module_type_implicit_global_symbol(self.db(), symbol.name())
                    } else {
                        Place::Undefined.into()
                    }
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

    fn infer_class_type_params(&mut self, class: &ast::StmtClassDef) {
        let type_params = class
            .type_params
            .as_deref()
            .expect("class type params scope without type params");

        let binding_context = self.index.expect_single_definition(class);
        let previous_typevar_binding_context =
            self.typevar_binding_context.replace(binding_context);

        self.infer_type_parameters(type_params);

        if let Some(arguments) = class.arguments.as_deref() {
            let in_stub = self.in_stub();
            let previous_deferred_state =
                std::mem::replace(&mut self.deferred_state, in_stub.into());
            let mut call_arguments =
                CallArguments::from_arguments(arguments, |argument, splatted_value| {
                    let ty = self.infer_expression(splatted_value, TypeContext::default());
                    if let Some(argument) = argument {
                        self.store_expression_type(argument, ty);
                    }
                    ty
                });
            let argument_forms = vec![Some(ParameterForm::Value); call_arguments.len()];
            self.infer_argument_types(arguments, &mut call_arguments, &argument_forms);
            self.deferred_state = previous_deferred_state;
        }

        self.typevar_binding_context = previous_typevar_binding_context;
    }

    fn infer_class_body(&mut self, class: &ast::StmtClassDef) {
        self.infer_body(&class.body);
    }

    fn infer_function_type_params(&mut self, function: &ast::StmtFunctionDef) {
        let type_params = function
            .type_params
            .as_deref()
            .expect("function type params scope without type params");

        let binding_context = self.index.expect_single_definition(function);
        let previous_typevar_binding_context =
            self.typevar_binding_context.replace(binding_context);
        self.infer_return_type_annotation(
            function.returns.as_deref(),
            self.defer_annotations().into(),
        );
        self.infer_type_parameters(type_params);
        self.infer_parameters(&function.parameters);
        self.typevar_binding_context = previous_typevar_binding_context;
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

    fn infer_function_body(&mut self, function: &ast::StmtFunctionDef) {
        // Parameters are odd: they are Definitions in the function body scope, but have no
        // constituent nodes that are part of the function body. In order to get diagnostics
        // merged/emitted for them, we need to explicitly infer their definitions here.
        for parameter in &function.parameters {
            self.infer_definition(parameter);
        }
        self.infer_body(&function.body);

        if let Some(returns) = function.returns.as_deref() {
            fn is_stub_suite(suite: &[ast::Stmt]) -> bool {
                match suite {
                    [
                        ast::Stmt::Expr(ast::StmtExpr { value: first, .. }),
                        ast::Stmt::Expr(ast::StmtExpr { value: second, .. }),
                        ..,
                    ] => first.is_string_literal_expr() && second.is_ellipsis_literal_expr(),
                    [
                        ast::Stmt::Expr(ast::StmtExpr { value, .. }),
                        ast::Stmt::Pass(_),
                        ..,
                    ] => value.is_string_literal_expr(),
                    [ast::Stmt::Expr(ast::StmtExpr { value, .. }), ..] => {
                        value.is_ellipsis_literal_expr() || value.is_string_literal_expr()
                    }
                    [ast::Stmt::Pass(_)] => true,
                    _ => false,
                }
            }

            let has_empty_body =
                self.return_types_and_ranges.is_empty() && is_stub_suite(&function.body);

            let mut enclosing_class_context = None;

            if has_empty_body {
                if self.in_stub() {
                    return;
                }
                if self.in_function_overload_or_abstractmethod() {
                    return;
                }
                if self.scope().scope(self.db()).in_type_checking_block() {
                    return;
                }
                if let Some(class) = self.class_context_of_current_method() {
                    enclosing_class_context = Some(class);
                    if class.is_protocol(self.db()) {
                        return;
                    }
                }
            }

            let declared_ty = self.file_expression_type(returns);
            let expected_ty = match declared_ty {
                Type::TypeIs(_) => KnownClass::Bool.to_instance(self.db()),
                ty => ty,
            };

            let scope_id = self.index.node_scope(NodeWithScopeRef::Function(function));
            if scope_id.is_generator_function(self.index) {
                // TODO: `AsyncGeneratorType` and `GeneratorType` are both generic classes.
                //
                // If type arguments are supplied to `(Async)Iterable`, `(Async)Iterator`,
                // `(Async)Generator` or `(Async)GeneratorType` in the return annotation,
                // we should iterate over the `yield` expressions and `return` statements
                // in the function to check that they are consistent with the type arguments
                // provided. Once we do this, the `.to_instance_unknown` call below should
                // be replaced with `.to_specialized_instance`.
                let inferred_return = if function.is_async {
                    KnownClass::AsyncGeneratorType
                } else {
                    KnownClass::GeneratorType
                };

                if !inferred_return
                    .to_instance_unknown(self.db())
                    .is_assignable_to(self.db(), expected_ty)
                {
                    report_invalid_generator_function_return_type(
                        &self.context,
                        returns.range(),
                        inferred_return,
                        declared_ty,
                    );
                }
                return;
            }

            for invalid in self
                .return_types_and_ranges
                .iter()
                .copied()
                .filter_map(|ty_range| match ty_range.ty {
                    // We skip `is_assignable_to` checks for `NotImplemented`,
                    // so we remove it beforehand.
                    Type::Union(union) => Some(TypeAndRange {
                        ty: union.filter(self.db(), |ty| !ty.is_notimplemented(self.db())),
                        range: ty_range.range,
                    }),
                    ty if ty.is_notimplemented(self.db()) => None,
                    _ => Some(ty_range),
                })
                .filter(|ty_range| !ty_range.ty.is_assignable_to(self.db(), expected_ty))
            {
                report_invalid_return_type(
                    &self.context,
                    invalid.range,
                    returns.range(),
                    declared_ty,
                    invalid.ty,
                );
            }
            if self
                .index
                .use_def_map(scope_id)
                .can_implicitly_return_none(self.db())
                && !Type::none(self.db()).is_assignable_to(self.db(), expected_ty)
            {
                let no_return = self.return_types_and_ranges.is_empty();
                report_implicit_return_type(
                    &self.context,
                    returns.range(),
                    declared_ty,
                    has_empty_body,
                    enclosing_class_context,
                    no_return,
                );
            }
        }
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
                self.infer_maybe_standalone_expression(value, TypeContext::default());
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

    fn infer_function_definition_statement(&mut self, function: &ast::StmtFunctionDef) {
        self.infer_definition(function);
    }

    fn infer_function_definition(
        &mut self,
        function: &ast::StmtFunctionDef,
        definition: Definition<'db>,
    ) {
        let ast::StmtFunctionDef {
            range: _,
            node_index: _,
            is_async: _,
            name,
            type_params,
            parameters,
            returns,
            body: _,
            decorator_list,
        } = function;

        let mut decorator_types_and_nodes = Vec::with_capacity(decorator_list.len());
        let mut function_decorators = FunctionDecorators::empty();
        let mut deprecated = None;
        let mut dataclass_transformer_params = None;

        for decorator in decorator_list {
            let decorator_type = self.infer_decorator(decorator);
            let decorator_function_decorator =
                FunctionDecorators::from_decorator_type(self.db(), decorator_type);
            function_decorators |= decorator_function_decorator;

            match decorator_type {
                Type::FunctionLiteral(function) => {
                    if let Some(KnownFunction::NoTypeCheck) = function.known(self.db()) {
                        // If the function is decorated with the `no_type_check` decorator,
                        // we need to suppress any errors that come after the decorators.
                        self.context.set_in_no_type_check(InNoTypeCheck::Yes);
                        continue;
                    }
                }
                Type::KnownInstance(KnownInstanceType::Deprecated(deprecated_inst)) => {
                    deprecated = Some(deprecated_inst);
                }
                Type::DataclassTransformer(params) => {
                    dataclass_transformer_params = Some(params);
                }
                _ => {}
            }
            if !decorator_function_decorator.is_empty() {
                continue;
            }

            decorator_types_and_nodes.push((decorator_type, decorator));
        }

        for default in parameters
            .iter_non_variadic_params()
            .filter_map(|param| param.default.as_deref())
        {
            self.infer_expression(default, TypeContext::default());
        }

        // If there are type params, parameters and returns are evaluated in that scope, that is, in
        // `infer_function_type_params`, rather than here.
        if type_params.is_none() {
            if self.defer_annotations() {
                self.deferred.insert(definition, self.multi_inference_state);
            } else {
                let previous_typevar_binding_context =
                    self.typevar_binding_context.replace(definition);
                self.infer_return_type_annotation(
                    returns.as_deref(),
                    DeferredExpressionState::None,
                );
                self.infer_parameters(parameters);
                self.typevar_binding_context = previous_typevar_binding_context;
            }
        }

        let known_function =
            KnownFunction::try_from_definition_and_name(self.db(), definition, name);

        // `type_check_only` is itself not available at runtime
        if known_function == Some(KnownFunction::TypeCheckOnly) {
            function_decorators |= FunctionDecorators::TYPE_CHECK_ONLY;
        }

        let body_scope = self
            .index
            .node_scope(NodeWithScopeRef::Function(function))
            .to_scope_id(self.db(), self.file());

        let overload_literal = OverloadLiteral::new(
            self.db(),
            &name.id,
            known_function,
            body_scope,
            function_decorators,
            deprecated,
            dataclass_transformer_params,
        );
        let function_literal = FunctionLiteral::new(self.db(), overload_literal);

        let mut inferred_ty =
            Type::FunctionLiteral(FunctionType::new(self.db(), function_literal, None, None));
        self.undecorated_type = Some(inferred_ty);

        for (decorator_ty, decorator_node) in decorator_types_and_nodes.iter().rev() {
            inferred_ty = match decorator_ty
                .try_call(self.db(), &CallArguments::positional([inferred_ty]))
                .map(|bindings| bindings.return_type(self.db()))
            {
                Ok(return_ty) => {
                    fn into_function_like_callable<'d>(
                        db: &'d dyn Db,
                        ty: Type<'d>,
                    ) -> Option<Type<'d>> {
                        match ty {
                            Type::Callable(callable) => Some(Type::Callable(CallableType::new(
                                db,
                                callable.signatures(db),
                                CallableTypeKind::FunctionLike,
                            ))),
                            Type::Union(union) => union
                                .try_map(db, |element| into_function_like_callable(db, *element)),
                            // Intersections are currently not handled here because that would require
                            // the decorator to be explicitly annotated as returning an intersection.
                            _ => None,
                        }
                    }

                    let is_input_function_like = inferred_ty
                        .try_upcast_to_callable(self.db())
                        .and_then(CallableTypes::exactly_one)
                        .is_some_and(|callable| callable.is_function_like(self.db()));

                    if is_input_function_like
                        && let Some(return_ty_function_like) =
                            into_function_like_callable(self.db(), return_ty)
                    {
                        // When a method on a class is decorated with a function that returns a `Callable`, assume that
                        // the returned callable is also function-like. See "Decorating a method with a `Callable`-typed
                        // decorator" in `callables_as_descriptors.md` for the extended explanation.
                        return_ty_function_like
                    } else {
                        return_ty
                    }
                }
                Err(CallError(_, bindings)) => {
                    bindings.report_diagnostics(&self.context, (*decorator_node).into());
                    bindings.return_type(self.db())
                }
            };
        }

        self.add_declaration_with_binding(
            function.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(inferred_ty),
        );

        if function_decorators.contains(FunctionDecorators::OVERLOAD) {
            for stmt in &function.body {
                match stmt {
                    ast::Stmt::Pass(_) => continue,
                    ast::Stmt::Expr(ast::StmtExpr { value, .. }) => {
                        if matches!(
                            &**value,
                            ast::Expr::StringLiteral(_) | ast::Expr::EllipsisLiteral(_)
                        ) {
                            continue;
                        }
                    }
                    _ => {}
                }
                let Some(builder) = self.context.report_lint(&USELESS_OVERLOAD_BODY, stmt) else {
                    continue;
                };
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Useless body for `@overload`-decorated function `{}`",
                    &function.name
                ));
                diagnostic.set_primary_message("This statement will never be executed");
                diagnostic.info(
                    "`@overload`-decorated functions are solely for type checkers \
                    and must be overwritten at runtime by a non-`@overload`-decorated implementation",
                );
                diagnostic.help("Consider replacing this function body with `...` or `pass`");
                break;
            }
        }
    }

    fn infer_return_type_annotation(
        &mut self,
        returns: Option<&ast::Expr>,
        deferred_expression_state: DeferredExpressionState,
    ) {
        if let Some(returns) = returns {
            let annotated = self.infer_annotation_expression(returns, deferred_expression_state);

            if !annotated.qualifiers.is_empty() {
                for qualifier in [
                    TypeQualifiers::FINAL,
                    TypeQualifiers::CLASS_VAR,
                    TypeQualifiers::INIT_VAR,
                ] {
                    if annotated.qualifiers.contains(qualifier) {
                        if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, returns)
                        {
                            builder.into_diagnostic(format!(
                                "`{name}` is not allowed in function return type annotations",
                                name = qualifier.name()
                            ));
                        }
                    }
                }
            }
        }
    }

    fn infer_parameters(&mut self, parameters: &ast::Parameters) {
        let ast::Parameters {
            range: _,
            node_index: _,
            posonlyargs: _,
            args: _,
            vararg,
            kwonlyargs: _,
            kwarg,
        } = parameters;

        for param_with_default in parameters.iter_non_variadic_params() {
            self.infer_parameter_with_default(param_with_default);
        }
        if let Some(vararg) = vararg {
            self.infer_parameter(vararg);
        }
        if let Some(kwarg) = kwarg {
            self.infer_parameter(kwarg);
        }
    }

    fn infer_parameter_with_default(&mut self, parameter_with_default: &ast::ParameterWithDefault) {
        let ast::ParameterWithDefault {
            range: _,
            node_index: _,
            parameter,
            default: _,
        } = parameter_with_default;

        let annotated = self.infer_optional_annotation_expression(
            parameter.annotation.as_deref(),
            self.defer_annotations().into(),
        );

        if let Some(qualifiers) = annotated.map(|annotated| annotated.qualifiers) {
            if !qualifiers.is_empty() {
                for qualifier in [
                    TypeQualifiers::FINAL,
                    TypeQualifiers::CLASS_VAR,
                    TypeQualifiers::INIT_VAR,
                ] {
                    if qualifiers.contains(qualifier) {
                        if let Some(builder) =
                            self.context.report_lint(&INVALID_TYPE_FORM, parameter)
                        {
                            builder.into_diagnostic(format!(
                                "`{name}` is not allowed in function parameter annotations",
                                name = qualifier.name()
                            ));
                        }
                    }
                }
            }
        }
    }

    fn infer_parameter(&mut self, parameter: &ast::Parameter) {
        let ast::Parameter {
            range: _,
            node_index: _,
            name: _,
            annotation,
        } = parameter;

        self.infer_optional_annotation_expression(
            annotation.as_deref(),
            self.defer_annotations().into(),
        );
    }

    /// Set initial declared type (if annotated) and inferred type for a function-parameter symbol,
    /// in the function body scope.
    ///
    /// The declared type is the annotated type, if any, or `Unknown`.
    ///
    /// The inferred type is the annotated type, if any. If there is no annotation, it is the union
    /// of `Unknown` and the type of the default value, if any.
    ///
    /// Parameter definitions are odd in that they define a symbol in the function-body scope, so
    /// the Definition belongs to the function body scope, but the expressions (annotation and
    /// default value) both belong to outer scopes. (The default value always belongs to the outer
    /// scope in which the function is defined, the annotation belongs either to the outer scope,
    /// or maybe to an intervening type-params scope, if it's a generic function.) So we don't use
    /// `self.infer_expression` or store any expression types here, we just use `expression_ty` to
    /// get the types of the expressions from their respective scopes.
    ///
    /// It is safe (non-cycle-causing) to use `expression_ty` here, because an outer scope can't
    /// depend on a definition from an inner scope, so we shouldn't be in-process of inferring the
    /// outer scope here.
    fn infer_parameter_definition(
        &mut self,
        parameter_with_default: &ast::ParameterWithDefault,
        definition: Definition<'db>,
    ) {
        let ast::ParameterWithDefault {
            parameter,
            default,
            range: _,
            node_index: _,
        } = parameter_with_default;
        let default_ty = default
            .as_ref()
            .map(|default| self.file_expression_type(default));
        if let Some(annotation) = parameter.annotation.as_ref() {
            let declared_ty = self.file_expression_type(annotation);
            if let Some(default_ty) = default_ty {
                if !default_ty.is_assignable_to(self.db(), declared_ty)
                    && !((self.in_stub()
                        || self.in_function_overload_or_abstractmethod()
                        || self
                            .class_context_of_current_method()
                            .is_some_and(|class| class.is_protocol(self.db())))
                        && default
                            .as_ref()
                            .is_some_and(|d| d.is_ellipsis_literal_expr()))
                {
                    if let Some(builder) = self
                        .context
                        .report_lint(&INVALID_PARAMETER_DEFAULT, parameter_with_default)
                    {
                        builder.into_diagnostic(format_args!(
                            "Default value of type `{}` is not assignable \
                             to annotated parameter type `{}`",
                            default_ty.display(self.db()),
                            declared_ty.display(self.db())
                        ));
                    }
                }
            }
            self.add_declaration_with_binding(
                parameter.into(),
                definition,
                &DeclaredAndInferredType::are_the_same_type(declared_ty),
            );
        } else {
            let ty = if let Some(default_ty) = default_ty {
                UnionType::from_elements(self.db(), [Type::unknown(), default_ty])
            } else if let Some(ty) = self.special_first_method_parameter_type(parameter) {
                ty
            } else {
                Type::unknown()
            };

            self.add_binding(parameter.into(), definition, |_, _| ty);
        }
    }

    /// Set initial declared/inferred types for a `*args` variadic positional parameter.
    ///
    /// The annotated type is implicitly wrapped in a homogeneous tuple.
    ///
    /// See [`infer_parameter_definition`] doc comment for some relevant observations about scopes.
    ///
    /// [`infer_parameter_definition`]: Self::infer_parameter_definition
    fn infer_variadic_positional_parameter_definition(
        &mut self,
        parameter: &ast::Parameter,
        definition: Definition<'db>,
    ) {
        if let Some(annotation) = parameter.annotation() {
            let ty = if annotation.is_starred_expr() {
                todo_type!("PEP 646")
            } else {
                let annotated_type = self.file_expression_type(annotation);
                if let Type::TypeVar(typevar) = annotated_type
                    && typevar.is_paramspec(self.db())
                {
                    match typevar.paramspec_attr(self.db()) {
                        // `*args: P.args`
                        Some(ParamSpecAttrKind::Args) => annotated_type,

                        // `*args: P.kwargs`
                        Some(ParamSpecAttrKind::Kwargs) => {
                            // TODO: Should this diagnostic be raised as part of
                            // `ArgumentTypeChecker`?
                            if let Some(builder) =
                                self.context.report_lint(&INVALID_TYPE_FORM, annotation)
                            {
                                let name = typevar.name(self.db());
                                let mut diag = builder.into_diagnostic(format_args!(
                                    "`{name}.kwargs` is valid only in `**kwargs` annotation",
                                ));
                                diag.set_primary_message(format_args!(
                                    "Did you mean `{name}.args`?"
                                ));
                                diagnostic::add_type_expression_reference_link(diag);
                            }
                            Type::homogeneous_tuple(self.db(), Type::unknown())
                        }

                        // `*args: P`
                        None => {
                            // The diagnostic for this case is handled in `in_type_expression`.
                            Type::homogeneous_tuple(self.db(), Type::unknown())
                        }
                    }
                } else {
                    Type::homogeneous_tuple(self.db(), annotated_type)
                }
            };

            self.add_declaration_with_binding(
                parameter.into(),
                definition,
                &DeclaredAndInferredType::are_the_same_type(ty),
            );
        } else {
            self.add_binding(parameter.into(), definition, |builder, _| {
                Type::homogeneous_tuple(builder.db(), Type::unknown())
            });
        }
    }

    /// Special case for unannotated `cls` and `self` arguments to class methods and instance methods.
    fn special_first_method_parameter_type(
        &mut self,
        parameter: &ast::Parameter,
    ) -> Option<Type<'db>> {
        let db = self.db();
        let file = self.file();

        let function_scope_id = self.scope();
        let function_scope = function_scope_id.scope(db);
        let function = function_scope.node().as_function()?;

        let parent_file_scope_id = function_scope.parent()?;
        let mut parent_scope_id = parent_file_scope_id.to_scope_id(db, file);

        // Skip type parameter scopes, if the method itself is generic.
        if parent_scope_id.is_annotation(db) {
            let parent_scope = parent_scope_id.scope(db);
            parent_scope_id = parent_scope.parent()?.to_scope_id(db, file);
        }

        // Return early if this is not a method inside a class.
        let class = parent_scope_id.scope(db).node().as_class()?;

        let method_definition = self.index.expect_single_definition(function);
        let DefinitionKind::Function(function_definition) = method_definition.kind(db) else {
            return None;
        };

        if function_definition
            .node(self.module())
            .parameters
            .index(parameter.name())
            .is_none_or(|index| index != 0)
        {
            return None;
        }

        let function_node = function_definition.node(self.module());
        let function_name = &function_node.name;

        // TODO: handle implicit type of `cls` for classmethods
        if is_implicit_classmethod(function_name) || is_implicit_staticmethod(function_name) {
            return None;
        }

        let inference = infer_definition_types(db, method_definition);
        for decorator in &function_node.decorator_list {
            let decorator_ty = inference.expression_type(&decorator.expression);
            if decorator_ty.as_class_literal().is_some_and(|class| {
                matches!(
                    class.known(db),
                    Some(KnownClass::Classmethod | KnownClass::Staticmethod)
                )
            }) {
                return None;
            }
        }

        let class_definition = self.index.expect_single_definition(class);
        let class_literal = infer_definition_types(db, class_definition)
            .declaration_type(class_definition)
            .inner_type()
            .as_class_literal()?;

        typing_self(db, self.scope(), Some(method_definition), class_literal)
    }

    /// Set initial declared/inferred types for a `**kwargs` keyword-variadic parameter.
    ///
    /// The annotated type is implicitly wrapped in a string-keyed dictionary.
    ///
    /// See [`infer_parameter_definition`] doc comment for some relevant observations about scopes.
    ///
    /// [`infer_parameter_definition`]: Self::infer_parameter_definition
    fn infer_variadic_keyword_parameter_definition(
        &mut self,
        parameter: &ast::Parameter,
        definition: Definition<'db>,
    ) {
        if let Some(annotation) = parameter.annotation() {
            let annotated_type = self.file_expression_type(annotation);
            let ty = if let Type::TypeVar(typevar) = annotated_type
                && typevar.is_paramspec(self.db())
            {
                match typevar.paramspec_attr(self.db()) {
                    // `**kwargs: P.args`
                    Some(ParamSpecAttrKind::Args) => {
                        // TODO: Should this diagnostic be raised as part of `ArgumentTypeChecker`?
                        if let Some(builder) =
                            self.context.report_lint(&INVALID_TYPE_FORM, annotation)
                        {
                            let name = typevar.name(self.db());
                            let mut diag = builder.into_diagnostic(format_args!(
                                "`{name}.args` is valid only in `*args` annotation",
                            ));
                            diag.set_primary_message(format_args!("Did you mean `{name}.kwargs`?"));
                            diagnostic::add_type_expression_reference_link(diag);
                        }
                        KnownClass::Dict.to_specialized_instance(
                            self.db(),
                            [KnownClass::Str.to_instance(self.db()), Type::unknown()],
                        )
                    }

                    // `**kwargs: P.kwargs`
                    Some(ParamSpecAttrKind::Kwargs) => annotated_type,

                    // `**kwargs: P`
                    None => {
                        // The diagnostic for this case is handled in `in_type_expression`.
                        KnownClass::Dict.to_specialized_instance(
                            self.db(),
                            [KnownClass::Str.to_instance(self.db()), Type::unknown()],
                        )
                    }
                }
            } else {
                KnownClass::Dict.to_specialized_instance(
                    self.db(),
                    [KnownClass::Str.to_instance(self.db()), annotated_type],
                )
            };
            self.add_declaration_with_binding(
                parameter.into(),
                definition,
                &DeclaredAndInferredType::are_the_same_type(ty),
            );
        } else {
            self.add_binding(parameter.into(), definition, |builder, _| {
                KnownClass::Dict.to_specialized_instance(
                    builder.db(),
                    [KnownClass::Str.to_instance(builder.db()), Type::unknown()],
                )
            });
        }
    }

    fn infer_class_definition_statement(&mut self, class: &ast::StmtClassDef) {
        self.infer_definition(class);
    }

    fn infer_class_definition(
        &mut self,
        class_node: &ast::StmtClassDef,
        definition: Definition<'db>,
    ) {
        let ast::StmtClassDef {
            range: _,
            node_index: _,
            name,
            type_params,
            decorator_list,
            arguments: _,
            body: _,
        } = class_node;

        let mut deprecated = None;
        let mut type_check_only = false;
        let mut dataclass_params = None;
        let mut dataclass_transformer_params = None;
        for decorator in decorator_list {
            let decorator_ty = self.infer_decorator(decorator);
            if decorator_ty
                .as_function_literal()
                .is_some_and(|function| function.is_known(self.db(), KnownFunction::Dataclass))
            {
                dataclass_params = Some(DataclassParams::default_params(self.db()));
                continue;
            }

            if let Type::DataclassDecorator(params) = decorator_ty {
                dataclass_params = Some(params);
                continue;
            }

            if let Type::KnownInstance(KnownInstanceType::Deprecated(deprecated_inst)) =
                decorator_ty
            {
                deprecated = Some(deprecated_inst);
                continue;
            }

            if decorator_ty
                .as_function_literal()
                .is_some_and(|function| function.is_known(self.db(), KnownFunction::TypeCheckOnly))
            {
                type_check_only = true;
                continue;
            }

            if let Type::FunctionLiteral(f) = decorator_ty {
                // We do not yet detect or flag `@dataclass_transform` applied to more than one
                // overload, or an overload and the implementation both. Nevertheless, this is not
                // allowed. We do not try to treat the offenders intelligently -- just use the
                // params of the last seen usage of `@dataclass_transform`
                let transformer_params = f
                    .iter_overloads_and_implementation(self.db())
                    .find_map(|overload| overload.dataclass_transformer_params(self.db()));
                if let Some(transformer_params) = transformer_params {
                    dataclass_params = Some(DataclassParams::from_transformer_params(
                        self.db(),
                        transformer_params,
                    ));
                    continue;
                }
            }

            if let Type::DataclassTransformer(params) = decorator_ty {
                dataclass_transformer_params = Some(params);
                continue;
            }
        }

        let body_scope = self
            .index
            .node_scope(NodeWithScopeRef::Class(class_node))
            .to_scope_id(self.db(), self.file());

        let maybe_known_class = KnownClass::try_from_file_and_name(self.db(), self.file(), name);

        let in_typing_module = || {
            matches!(
                file_to_module(self.db(), self.file()).and_then(|module| module.known(self.db())),
                Some(KnownModule::Typing | KnownModule::TypingExtensions)
            )
        };

        let ty = match (maybe_known_class, &*name.id) {
            (None, "NamedTuple") if in_typing_module() => {
                Type::SpecialForm(SpecialFormType::NamedTuple)
            }
            (None, "Any") if in_typing_module() => Type::SpecialForm(SpecialFormType::Any),
            _ => Type::from(ClassLiteral::new(
                self.db(),
                name.id.clone(),
                body_scope,
                maybe_known_class,
                deprecated,
                type_check_only,
                dataclass_params,
                dataclass_transformer_params,
            )),
        };

        self.add_declaration_with_binding(
            class_node.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(ty),
        );

        // if there are type parameters, then the keywords and bases are within that scope
        // and we don't need to run inference here
        if type_params.is_none() {
            for keyword in class_node.keywords() {
                self.infer_expression(&keyword.value, TypeContext::default());
            }

            // Inference of bases deferred in stubs, or if any are string literals.
            if self.in_stub() || class_node.bases().iter().any(contains_string_literal) {
                self.deferred.insert(definition, self.multi_inference_state);
            } else {
                let previous_typevar_binding_context =
                    self.typevar_binding_context.replace(definition);
                for base in class_node.bases() {
                    self.infer_expression(base, TypeContext::default());
                }
                self.typevar_binding_context = previous_typevar_binding_context;
            }
        }
    }

    fn infer_function_deferred(
        &mut self,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) {
        let previous_typevar_binding_context = self.typevar_binding_context.replace(definition);
        self.infer_return_type_annotation(
            function.returns.as_deref(),
            DeferredExpressionState::Deferred,
        );
        self.infer_parameters(function.parameters.as_ref());
        self.typevar_binding_context = previous_typevar_binding_context;
    }

    fn infer_class_deferred(&mut self, definition: Definition<'db>, class: &ast::StmtClassDef) {
        let previous_typevar_binding_context = self.typevar_binding_context.replace(definition);
        for base in class.bases() {
            if self.in_stub() {
                self.infer_expression_with_state(
                    base,
                    TypeContext::default(),
                    DeferredExpressionState::Deferred,
                );
            } else {
                self.infer_expression(base, TypeContext::default());
            }
        }
        self.typevar_binding_context = previous_typevar_binding_context;
    }

    fn infer_type_alias_definition(
        &mut self,
        type_alias: &ast::StmtTypeAlias,
        definition: Definition<'db>,
    ) {
        self.infer_expression(&type_alias.name, TypeContext::default());

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
                self.infer_target(target, &item.context_expr, |builder, tcx| {
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
        self.add_binding(target.into(), definition, |_, _| target_ty);
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

            for (index, element) in tuple_spec.all_elements().enumerate() {
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
            UnionType::from_elements(
                self.db(),
                [
                    type_base_exception,
                    Type::homogeneous_tuple(self.db(), type_base_exception),
                ],
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
            class.to_specialized_instance(self.db(), [symbol_ty])
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
            |_, _| symbol_ty,
        );
    }

    fn infer_typevar_definition(
        &mut self,
        node: &ast::TypeParamTypeVar,
        definition: Definition<'db>,
    ) {
        let ast::TypeParamTypeVar {
            range: _,
            node_index: _,
            name,
            bound,
            default,
        } = node;

        let bound_or_constraint = match bound.as_deref() {
            Some(expr @ ast::Expr::Tuple(ast::ExprTuple { elts, .. })) => {
                if elts.len() < 2 {
                    if let Some(builder) = self
                        .context
                        .report_lint(&INVALID_TYPE_VARIABLE_CONSTRAINTS, expr)
                    {
                        builder.into_diagnostic("TypeVar must have at least two constrained types");
                    }
                    None
                } else {
                    Some(TypeVarBoundOrConstraintsEvaluation::LazyConstraints)
                }
            }
            Some(_) => Some(TypeVarBoundOrConstraintsEvaluation::LazyUpperBound),
            None => None,
        };
        if bound_or_constraint.is_some() || default.is_some() {
            self.deferred.insert(definition, self.multi_inference_state);
        }
        let identity =
            TypeVarIdentity::new(self.db(), &name.id, Some(definition), TypeVarKind::Pep695);
        let ty = Type::KnownInstance(KnownInstanceType::TypeVar(TypeVarInstance::new(
            self.db(),
            identity,
            bound_or_constraint,
            None, // explicit_variance
            default.as_deref().map(|_| TypeVarDefaultEvaluation::Lazy),
        )));
        self.add_declaration_with_binding(
            node.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(ty),
        );
    }

    fn infer_typevar_deferred(&mut self, node: &ast::TypeParamTypeVar) {
        let ast::TypeParamTypeVar {
            range: _,
            node_index: _,
            name: _,
            bound,
            default,
        } = node;
        let previous_deferred_state =
            std::mem::replace(&mut self.deferred_state, DeferredExpressionState::Deferred);
        match bound.as_deref() {
            Some(expr @ ast::Expr::Tuple(ast::ExprTuple { elts, .. })) => {
                // Here, we interpret `bound` as a heterogeneous tuple and convert it to `TypeVarConstraints` in `TypeVarInstance::lazy_constraints`.
                let tuple_ty = Type::heterogeneous_tuple(
                    self.db(),
                    elts.iter()
                        .map(|expr| self.infer_type_expression(expr))
                        .collect::<Box<[_]>>(),
                );
                self.store_expression_type(expr, tuple_ty);
            }
            Some(expr) => {
                self.infer_type_expression(expr);
            }
            None => {}
        }
        self.infer_optional_type_expression(default.as_deref());
        self.deferred_state = previous_deferred_state;
    }

    fn infer_paramspec_definition(
        &mut self,
        node: &ast::TypeParamParamSpec,
        definition: Definition<'db>,
    ) {
        let ast::TypeParamParamSpec {
            range: _,
            node_index: _,
            name,
            default,
        } = node;
        if default.is_some() {
            self.deferred.insert(definition, self.multi_inference_state);
        }
        let identity = TypeVarIdentity::new(
            self.db(),
            &name.id,
            Some(definition),
            TypeVarKind::Pep695ParamSpec,
        );
        let ty = Type::KnownInstance(KnownInstanceType::TypeVar(TypeVarInstance::new(
            self.db(),
            identity,
            None, // ParamSpec, when declared using PEP 695 syntax, has no bounds or constraints
            None, // explicit_variance
            default.as_deref().map(|_| TypeVarDefaultEvaluation::Lazy),
        )));
        self.add_declaration_with_binding(
            node.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(ty),
        );
    }

    fn infer_paramspec_deferred(&mut self, node: &ast::TypeParamParamSpec) {
        let ast::TypeParamParamSpec {
            range: _,
            node_index: _,
            name: _,
            default: Some(default),
        } = node
        else {
            return;
        };
        let previous_deferred_state =
            std::mem::replace(&mut self.deferred_state, DeferredExpressionState::Deferred);
        self.infer_paramspec_default(default);
        self.deferred_state = previous_deferred_state;
    }

    fn infer_paramspec_default(&mut self, default_expr: &ast::Expr) {
        match default_expr {
            ast::Expr::EllipsisLiteral(ellipsis) => {
                let ty = self.infer_ellipsis_literal_expression(ellipsis);
                self.store_expression_type(default_expr, ty);
                return;
            }
            ast::Expr::List(ast::ExprList { elts, .. }) => {
                let types = elts
                    .iter()
                    .map(|elt| self.infer_type_expression(elt))
                    .collect::<Vec<_>>();
                // N.B. We cannot represent a heterogeneous list of types in our type system, so we
                // use a heterogeneous tuple type to represent the list of types instead.
                self.store_expression_type(
                    default_expr,
                    Type::heterogeneous_tuple(self.db(), types),
                );
                return;
            }
            ast::Expr::Name(_) => {
                let ty = self.infer_type_expression(default_expr);
                let is_paramspec = match ty {
                    Type::TypeVar(typevar) => typevar.is_paramspec(self.db()),
                    Type::KnownInstance(known_instance) => {
                        known_instance.class(self.db()) == KnownClass::ParamSpec
                    }
                    _ => false,
                };
                if is_paramspec {
                    return;
                }
            }
            _ => {}
        }
        if let Some(builder) = self.context.report_lint(&INVALID_PARAMSPEC, default_expr) {
            builder.into_diagnostic(
                "The default value to `ParamSpec` must be either \
                    a list of types, `ParamSpec`, or `...`",
            );
        }
    }

    /// Infer the type of the expression that represents an explicit specialization of a
    /// `ParamSpec` type variable.
    fn infer_paramspec_explicit_specialization_value(
        &mut self,
        expr: &ast::Expr,
        exactly_one_paramspec: bool,
    ) -> Result<Type<'db>, ()> {
        let db = self.db();

        match expr {
            ast::Expr::EllipsisLiteral(_) => {
                return Ok(Type::paramspec_value_callable(
                    db,
                    Parameters::gradual_form(),
                ));
            }

            ast::Expr::Tuple(ast::ExprTuple { elts, .. })
            | ast::Expr::List(ast::ExprList { elts, .. }) => {
                // This should be taken care of by the caller.
                if expr.is_tuple_expr() {
                    assert!(
                        exactly_one_paramspec,
                        "Inferring ParamSpec value during explicit specialization for a \
                    tuple expression should only happen when it contains exactly one ParamSpec"
                    );
                }

                let mut parameter_types = Vec::with_capacity(elts.len());

                // Whether to infer `Todo` for the parameters
                let mut return_todo = false;

                for param in elts {
                    let param_type = self.infer_type_expression(param);
                    // This is similar to what we currently do for inferring tuple type expression.
                    // We currently infer `Todo` for the parameters to avoid invalid diagnostics
                    // when trying to check for assignability or any other relation. For example,
                    // `*tuple[int, str]`, `Unpack[]`, etc. are not yet supported.
                    return_todo |= param_type.is_todo()
                        && matches!(param, ast::Expr::Starred(_) | ast::Expr::Subscript(_));
                    parameter_types.push(param_type);
                }

                let parameters = if return_todo {
                    // TODO: `Unpack`
                    Parameters::todo()
                } else {
                    Parameters::new(
                        self.db(),
                        parameter_types.iter().map(|param_type| {
                            Parameter::positional_only(None).with_annotated_type(*param_type)
                        }),
                    )
                };

                return Ok(Type::paramspec_value_callable(db, parameters));
            }

            ast::Expr::Subscript(_) => {
                // TODO: Support `Concatenate[...]`
                return Ok(Type::paramspec_value_callable(db, Parameters::todo()));
            }

            ast::Expr::Name(_) => {
                let param_type = self.infer_type_expression(expr);

                match param_type {
                    Type::TypeVar(typevar) if typevar.is_paramspec(db) => {
                        return Ok(param_type);
                    }

                    Type::KnownInstance(known_instance)
                        if known_instance.class(self.db()) == KnownClass::ParamSpec =>
                    {
                        // TODO: Emit diagnostic: "ParamSpec "P" is unbound"
                        return Err(());
                    }

                    // This is to handle the following case:
                    //
                    // ```python
                    // from typing import ParamSpec
                    //
                    // class Foo[**P]: ...
                    //
                    // Foo[ParamSpec]  # P: (ParamSpec, /)
                    // ```
                    Type::NominalInstance(nominal)
                        if nominal.has_known_class(self.db(), KnownClass::ParamSpec) =>
                    {
                        return Ok(Type::paramspec_value_callable(
                            db,
                            Parameters::new(
                                self.db(),
                                [
                                    Parameter::positional_only(None)
                                        .with_annotated_type(param_type),
                                ],
                            ),
                        ));
                    }

                    _ if exactly_one_paramspec => {
                        // Square brackets are optional when `ParamSpec` is the only type variable
                        // being specialized. This means that a single name expression represents a
                        // parameter list with a single parameter. For example,
                        //
                        // ```python
                        // class OnlyParamSpec[**P]: ...
                        //
                        // OnlyParamSpec[int]  # P: (int, /)
                        // ```
                        let parameters =
                            if param_type.is_todo() {
                                Parameters::todo()
                            } else {
                                Parameters::new(
                                    self.db(),
                                    [Parameter::positional_only(None)
                                        .with_annotated_type(param_type)],
                                )
                            };
                        return Ok(Type::paramspec_value_callable(db, parameters));
                    }

                    // This is specifically to handle a case where there are more than one type
                    // variables and at least one of them is a `ParamSpec` which is specialized
                    // using `typing.Any`. This isn't explicitly allowed in the spec, but both mypy
                    // and Pyright allows this and the ecosystem report suggested there are usages
                    // of this in the wild e.g., `staticmethod[Any, Any]`. For example,
                    //
                    // ```python
                    // class Foo[**P, T]: ...
                    //
                    // Foo[Any, int]  # P: (Any, /), T: int
                    // ```
                    Type::Dynamic(DynamicType::Any) => {
                        return Ok(Type::paramspec_value_callable(
                            db,
                            Parameters::gradual_form(),
                        ));
                    }

                    _ => {}
                }
            }

            _ => {}
        }

        if let Some(builder) = self.context.report_lint(&INVALID_TYPE_ARGUMENTS, expr) {
            builder.into_diagnostic(
                "Type argument for `ParamSpec` must be either \
                    a list of types, `ParamSpec`, `Concatenate`, or `...`",
            );
        }

        Err(())
    }

    fn infer_typevartuple_definition(
        &mut self,
        node: &ast::TypeParamTypeVarTuple,
        definition: Definition<'db>,
    ) {
        let ast::TypeParamTypeVarTuple {
            range: _,
            node_index: _,
            name: _,
            default,
        } = node;
        self.infer_optional_expression(default.as_deref(), TypeContext::default());
        let pep_695_todo = todo_type!("PEP-695 TypeVarTuple definition types");
        self.add_declaration_with_binding(
            node.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(pep_695_todo),
        );
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
        pattern: &ast::Pattern,
        _index: u32,
        definition: Definition<'db>,
    ) {
        // TODO(dhruvmanila): The correct way to infer types here is to perform structural matching
        // against the subject expression type (which we can query via `infer_expression_types`)
        // and extract the type at the `index` position if the pattern matches. This will be
        // similar to the logic in `self.infer_assignment_definition`.
        self.add_binding(pattern.into(), definition, |_, _| {
            todo_type!("`match` pattern definition types")
        });
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
                self.infer_standalone_expression(cls, TypeContext::default());
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
                self.infer_maybe_standalone_expression(cls, TypeContext::default());
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
            self.infer_target(target, value, |builder, tcx| {
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
    fn infer_target<F>(&mut self, target: &ast::Expr, value: &ast::Expr, infer_value_expr: F)
    where
        F: Fn(&mut Self, TypeContext<'db>) -> Type<'db>,
    {
        match target {
            ast::Expr::Name(_) => {
                self.infer_target_impl(target, value, None);
            }

            _ => self.infer_target_impl(
                target,
                value,
                Some(&|builder, tcx| infer_value_expr(builder, tcx)),
            ),
        }
    }

    /// Validate a subscript assignment of the form `object[key] = rhs_value`.
    fn validate_subscript_assignment(
        &mut self,
        target: &ast::ExprSubscript,
        rhs_value: &ast::Expr,
        rhs_value_ty: Type<'db>,
    ) -> bool {
        let ast::ExprSubscript {
            range: _,
            node_index: _,
            value: object,
            slice,
            ctx: _,
        } = target;

        let object_ty = self.infer_expression(object, TypeContext::default());
        let slice_ty = self.infer_expression(slice, TypeContext::default());

        self.validate_subscript_assignment_impl(
            target,
            None,
            object_ty,
            slice_ty,
            rhs_value,
            rhs_value_ty,
            true,
        )
    }

    #[expect(clippy::too_many_arguments)]
    fn validate_subscript_assignment_impl(
        &self,
        target: &'ast ast::ExprSubscript,
        full_object_ty: Option<Type<'db>>,
        object_ty: Type<'db>,
        slice_ty: Type<'db>,
        rhs_value_node: &'ast ast::Expr,
        rhs_value_ty: Type<'db>,
        emit_diagnostic: bool,
    ) -> bool {
        /// Given a string literal or a union of string literals, return an iterator over the contained
        /// strings, or `None`, if the type is neither.
        fn key_literals<'db>(
            db: &'db dyn Db,
            slice_ty: Type<'db>,
        ) -> Option<impl Iterator<Item = &'db str> + 'db> {
            if let Some(literal) = slice_ty.as_string_literal() {
                Some(Either::Left(std::iter::once(literal.value(db))))
            } else {
                slice_ty.as_union().map(|union| {
                    Either::Right(
                        union
                            .elements(db)
                            .iter()
                            .filter_map(|ty| ty.as_string_literal().map(|lit| lit.value(db))),
                    )
                })
            }
        }

        let db = self.db();

        let attach_original_type_info = |diagnostic: &mut LintDiagnosticGuard| {
            if let Some(full_object_ty) = full_object_ty {
                diagnostic.info(format_args!(
                    "The full type of the subscripted object is `{}`",
                    full_object_ty.display(db)
                ));
            }
        };

        match object_ty {
            Type::Union(union) => {
                // Note that we use a loop here instead of .all(…) to avoid short-circuiting.
                // We need to keep iterating to emit all diagnostics.
                let mut valid = true;
                for element_ty in union.elements(db) {
                    valid &= self.validate_subscript_assignment_impl(
                        target,
                        full_object_ty.or(Some(object_ty)),
                        *element_ty,
                        slice_ty,
                        rhs_value_node,
                        rhs_value_ty,
                        emit_diagnostic,
                    );
                }
                valid
            }

            Type::Intersection(intersection) => {
                let check_positive_elements = |emit_diagnostic_and_short_circuit| {
                    let mut valid = false;
                    for element_ty in intersection.positive(db) {
                        valid |= self.validate_subscript_assignment_impl(
                            target,
                            full_object_ty.or(Some(object_ty)),
                            *element_ty,
                            slice_ty,
                            rhs_value_node,
                            rhs_value_ty,
                            emit_diagnostic_and_short_circuit,
                        );

                        if !valid && emit_diagnostic_and_short_circuit {
                            break;
                        }
                    }

                    valid
                };

                // Perform an initial check of all elements. If the assignment is valid
                // for at least one element, we do not emit any diagnostics. Otherwise,
                // we re-run the check and emit a diagnostic on the first failing element.
                let valid = check_positive_elements(false);

                if !valid {
                    check_positive_elements(true);
                }

                valid
            }

            Type::TypedDict(typed_dict) => {
                // As an optimization, prevent calling `__setitem__` on (unions of) large `TypedDict`s, and
                // validate the assignment ourselves. This also allows us to emit better diagnostics.

                let mut valid = true;
                let Some(keys) = key_literals(db, slice_ty) else {
                    // Check if the key has a valid type. We only allow string literals, a union of string literals,
                    // or a dynamic type like `Any`. We can do this by checking assignability to `LiteralString`,
                    // but we need to exclude `LiteralString` itself. This check would technically allow weird key
                    // types like `LiteralString & Any` to pass, but it does not need to be perfect. We would just
                    // fail to provide the "can only be subscripted with a string literal key" hint in that case.

                    if slice_ty.is_dynamic() {
                        return true;
                    }

                    let assigned_d = rhs_value_ty.display(db);
                    let value_d = object_ty.display(db);

                    if slice_ty.is_assignable_to(db, Type::LiteralString)
                        && !slice_ty.is_equivalent_to(db, Type::LiteralString)
                    {
                        if let Some(builder) = self
                            .context
                            .report_lint(&INVALID_ASSIGNMENT, target.slice.as_ref())
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Cannot assign value of type `{assigned_d}` to key of type `{}` on TypedDict `{value_d}`",
                                slice_ty.display(db)
                            ));
                            attach_original_type_info(&mut diagnostic);
                        }
                    } else {
                        if let Some(builder) = self
                            .context
                            .report_lint(&INVALID_KEY, target.slice.as_ref())
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "TypedDict `{value_d}` can only be subscripted with a string literal key, got key of type `{}`.",
                                slice_ty.display(db)
                            ));
                            attach_original_type_info(&mut diagnostic);
                        }
                    }

                    return false;
                };

                for key in keys {
                    valid &= validate_typed_dict_key_assignment(
                        &self.context,
                        typed_dict,
                        full_object_ty,
                        key,
                        rhs_value_ty,
                        target.value.as_ref(),
                        target.slice.as_ref(),
                        rhs_value_node,
                        TypedDictAssignmentKind::Subscript,
                        emit_diagnostic,
                    );
                }

                valid
            }

            _ => {
                match object_ty.try_call_dunder(
                    db,
                    "__setitem__",
                    CallArguments::positional([slice_ty, rhs_value_ty]),
                    TypeContext::default(),
                ) {
                    Ok(_) => true,
                    Err(err) => match err {
                        CallDunderError::PossiblyUnbound { .. } => {
                            if emit_diagnostic
                                && let Some(builder) = self
                                    .context
                                    .report_lint(&POSSIBLY_MISSING_IMPLICIT_CALL, target)
                            {
                                let mut diagnostic = builder.into_diagnostic(format_args!(
                                    "Method `__setitem__` of type `{}` may be missing",
                                    object_ty.display(db),
                                ));
                                attach_original_type_info(&mut diagnostic);
                            }
                            false
                        }
                        CallDunderError::CallError(call_error_kind, bindings) => {
                            match call_error_kind {
                                CallErrorKind::NotCallable => {
                                    if emit_diagnostic
                                        && let Some(builder) =
                                            self.context.report_lint(&CALL_NON_CALLABLE, target)
                                    {
                                        let mut diagnostic = builder.into_diagnostic(format_args!(
                                            "Method `__setitem__` of type `{}` is not callable \
                                             on object of type `{}`",
                                            bindings.callable_type().display(db),
                                            object_ty.display(db),
                                        ));
                                        attach_original_type_info(&mut diagnostic);
                                    }
                                }
                                CallErrorKind::BindingError => {
                                    if let Some(typed_dict) = object_ty.as_typed_dict() {
                                        if let Some(key) = slice_ty.as_string_literal() {
                                            let key = key.value(db);
                                            validate_typed_dict_key_assignment(
                                                &self.context,
                                                typed_dict,
                                                full_object_ty,
                                                key,
                                                rhs_value_ty,
                                                target.value.as_ref(),
                                                target.slice.as_ref(),
                                                rhs_value_node,
                                                TypedDictAssignmentKind::Subscript,
                                                true,
                                            );
                                        }
                                    } else {
                                        if emit_diagnostic
                                            && let Some(builder) = self.context.report_lint(
                                                &INVALID_ASSIGNMENT,
                                                target.range.cover(rhs_value_node.range()),
                                            )
                                        {
                                            let assigned_d = rhs_value_ty.display(db);
                                            let object_d = object_ty.display(db);

                                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                                    "Invalid subscript assignment with key of type `{}` and value of \
                                                     type `{assigned_d}` on object of type `{object_d}`",
                                                    slice_ty.display(db),
                                                ));

                                            // Special diagnostic for dictionaries
                                            if let Some([expected_key_ty, expected_value_ty]) =
                                                object_ty
                                                    .known_specialization(db, KnownClass::Dict)
                                                    .map(|s| s.types(db))
                                            {
                                                if !slice_ty.is_assignable_to(db, *expected_key_ty)
                                                {
                                                    diagnostic.annotate(
                                                        self.context
                                                            .secondary(target.slice.as_ref())
                                                            .message(format_args!(
                                                                "Expected key of type `{}`, got `{}`",
                                                                expected_key_ty.display(db),
                                                                slice_ty.display(db),
                                                            )),
                                                    );
                                                }

                                                if !rhs_value_ty
                                                    .is_assignable_to(db, *expected_value_ty)
                                                {
                                                    diagnostic.annotate(
                                                        self.context
                                                            .secondary(rhs_value_node)
                                                            .message(format_args!(
                                                                "Expected value of type `{}`, got `{}`",
                                                                expected_value_ty.display(db),
                                                                rhs_value_ty.display(db),
                                                            )),
                                                    );
                                                }
                                            }

                                            attach_original_type_info(&mut diagnostic);
                                        }
                                    }
                                }
                                CallErrorKind::PossiblyNotCallable => {
                                    if emit_diagnostic
                                        && let Some(builder) =
                                            self.context.report_lint(&CALL_NON_CALLABLE, target)
                                    {
                                        let mut diagnostic = builder.into_diagnostic(format_args!(
                                            "Method `__setitem__` of type `{}` may not be callable on object of type `{}`",
                                            bindings.callable_type().display(db),
                                            object_ty.display(db),
                                        ));
                                        attach_original_type_info(&mut diagnostic);
                                    }
                                }
                            }
                            false
                        }
                        CallDunderError::MethodNotAvailable => {
                            if emit_diagnostic
                                && let Some(builder) =
                                    self.context.report_lint(&INVALID_ASSIGNMENT, target)
                            {
                                let mut diagnostic = builder.into_diagnostic(format_args!(
                                    "Cannot assign to a subscript on an object of type `{}`",
                                    object_ty.display(db),
                                ));
                                attach_original_type_info(&mut diagnostic);

                                // If it's a user-defined class, suggest adding a `__setitem__` method.
                                if object_ty
                                    .as_nominal_instance()
                                    .and_then(|instance| {
                                        file_to_module(
                                            db,
                                            instance.class(db).class_literal(db).0.file(db),
                                        )
                                    })
                                    .and_then(|module| module.search_path(db))
                                    .is_some_and(crate::SearchPath::is_first_party)
                                {
                                    diagnostic.help(format_args!(
                                        "Consider adding a `__setitem__` method to `{}`.",
                                        object_ty.display(db),
                                    ));
                                } else {
                                    diagnostic.info(format_args!(
                                        "`{}` does not have a `__setitem__` method.",
                                        object_ty.display(db),
                                    ));
                                }
                            }
                            false
                        }
                    },
                }
            }
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

        let mut first_tcx = None;

        // A wrapper over `infer_value_ty` that allows inferring the value type multiple times
        // during attribute resolution.
        let pure_infer_value_ty = infer_value_ty;
        let mut infer_value_ty = |builder: &mut Self, tcx: TypeContext<'db>| -> Type<'db> {
            // Overwrite the previously inferred value, preferring later inferences, which are
            // likely more precise. Note that we still ensure each inference is assignable to
            // its declared type, so this mainly affects the IDE hover type.
            let prev_multi_inference_state =
                builder.set_multi_inference_state(MultiInferenceState::Overwrite);

            // If we are inferring the argument multiple times, silence diagnostics to avoid duplicated warnings.
            let was_in_multi_inference = if let Some(first_tcx) = first_tcx {
                // The first time we infer an argument during multi-inference must be without type context,
                // to avoid leaking diagnostics for bidirectional inference attempts.
                debug_assert_eq!(first_tcx, TypeContext::default());

                builder.context.set_multi_inference(true)
            } else {
                builder.context.is_in_multi_inference()
            };

            let value_ty = pure_infer_value_ty(builder, tcx);

            // Reset the multi-inference state.
            first_tcx.get_or_insert(tcx);
            builder.multi_inference_state = prev_multi_inference_state;
            builder.context.set_multi_inference(was_in_multi_inference);

            value_ty
        };

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
            if emit_diagnostics {
                if let Some(builder) = builder.context.report_lint(&INVALID_ASSIGNMENT, target) {
                    builder.into_diagnostic(format_args!(
                        "Cannot assign to final attribute `{attribute}` on type `{}`",
                        object_ty.display(db)
                    ));
                }
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
            {
                let class_definition = class_ty.class_literal(db).0;
                let class_scope_id = class_definition.body_scope(db).file_scope_id(db);
                let place_table = builder.index.place_table(class_scope_id);

                if let Some(symbol) = place_table.symbol_by_name(attribute) {
                    if symbol.is_bound() {
                        if emit_diagnostics {
                            if let Some(diag_builder) =
                                builder.context.report_lint(&INVALID_ASSIGNMENT, target)
                            {
                                diag_builder.into_diagnostic(format_args!(
                                    "Cannot assign to final attribute `{attribute}` in `__init__` \
                                                     because it already has a value at class level"
                                ));
                            }
                        }
                        return true;
                    }
                }
            }

            // In __init__ and no class-level value - allow
            false
        };

        match object_ty {
            Type::Union(union) => {
                // First infer the value without type context, and then again for each union element.
                let value_ty = infer_value_ty(self, TypeContext::default());

                if union.elements(self.db()).iter().all(|elem| {
                    self.validate_attribute_assignment(
                        target,
                        *elem,
                        attribute,
                        // Note that `infer_value_ty` silences diagnostics after the first inference.
                        &mut infer_value_ty,
                        false,
                    )
                }) {
                    true
                } else {
                    // TODO: This is not a very helpful error message, as it does not include the underlying reason
                    // why the assignment is invalid. This would be a good use case for sub-diagnostics.
                    if emit_diagnostics {
                        if let Some(builder) = self.context.report_lint(&INVALID_ASSIGNMENT, target)
                        {
                            builder.into_diagnostic(format_args!(
                                "Object of type `{}` is not assignable \
                                 to attribute `{attribute}` on type `{}`",
                                value_ty.display(self.db()),
                                object_ty.display(self.db()),
                            ));
                        }
                    }

                    false
                }
            }

            Type::Intersection(intersection) => {
                // First infer the value without type context, and then again for each union element.
                let value_ty = infer_value_ty(self, TypeContext::default());

                // TODO: Handle negative intersection elements
                if intersection.positive(db).iter().any(|elem| {
                    self.validate_attribute_assignment(
                        target,
                        *elem,
                        attribute,
                        // Note that `infer_value_ty` silences diagnostics after the first inference.
                        &mut infer_value_ty,
                        false,
                    )
                }) {
                    true
                } else {
                    if emit_diagnostics {
                        if let Some(builder) = self.context.report_lint(&INVALID_ASSIGNMENT, target)
                        {
                            // TODO: same here, see above
                            builder.into_diagnostic(format_args!(
                                "Object of type `{}` is not assignable \
                                 to attribute `{attribute}` on type `{}`",
                                value_ty.display(self.db()),
                                object_ty.display(self.db()),
                            ));
                        }
                    }
                    false
                }
            }

            Type::TypeAlias(alias) => self.validate_attribute_assignment(
                target,
                alias.value_type(self.db()),
                attribute,
                pure_infer_value_ty,
                emit_diagnostics,
            ),

            // Super instances do not allow attribute assignment
            Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Super) => {
                infer_value_ty(self, TypeContext::default());

                if emit_diagnostics {
                    if let Some(builder) = self.context.report_lint(&INVALID_ASSIGNMENT, target) {
                        builder.into_diagnostic(format_args!(
                            "Cannot assign to attribute `{attribute}` on type `{}`",
                            object_ty.display(self.db()),
                        ));
                    }
                }
                false
            }
            Type::BoundSuper(_) => {
                infer_value_ty(self, TypeContext::default());

                if emit_diagnostics {
                    if let Some(builder) = self.context.report_lint(&INVALID_ASSIGNMENT, target) {
                        builder.into_diagnostic(format_args!(
                            "Cannot assign to attribute `{attribute}` on type `{}`",
                            object_ty.display(self.db()),
                        ));
                    }
                }
                false
            }

            Type::Dynamic(..) | Type::Never => {
                infer_value_ty(self, TypeContext::default());
                true
            }

            Type::NominalInstance(..)
            | Type::ProtocolInstance(_)
            | Type::BooleanLiteral(..)
            | Type::IntLiteral(..)
            | Type::StringLiteral(..)
            | Type::BytesLiteral(..)
            | Type::EnumLiteral(..)
            | Type::LiteralString
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
            | Type::TypedDict(_)
            | Type::NewTypeInstance(_) => {
                // TODO: We could use the annotated parameter type of `__setattr__` as type context here.
                // However, we would still have to perform the first inference without type context.
                let value_ty = infer_value_ty(self, TypeContext::default());

                // First, try to call the `__setattr__` dunder method. If this is present/defined, overrides
                // assigning the attributed by the normal mechanism.
                let setattr_dunder_call_result = object_ty.try_call_dunder_with_policy(
                    db,
                    "__setattr__",
                    &mut CallArguments::positional([Type::string_literal(db, attribute), value_ty]),
                    TypeContext::default(),
                    MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                );

                let check_setattr_return_type = |result: Bindings<'db>| -> bool {
                    match result.return_type(db) {
                        Type::Never => {
                            if emit_diagnostics {
                                if let Some(builder) =
                                    self.context.report_lint(&INVALID_ASSIGNMENT, target)
                                {
                                    let is_setattr_synthesized = match object_ty
                                        .class_member_with_policy(
                                            db,
                                            "__setattr__".into(),
                                            MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                                        ) {
                                        PlaceAndQualifiers {
                                            place: Place::Defined(attr_ty, _, _),
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
                            false
                        }
                        _ => true,
                    }
                };

                match setattr_dunder_call_result {
                    Ok(result) => check_setattr_return_type(result),
                    Err(CallDunderError::PossiblyUnbound(result)) => {
                        check_setattr_return_type(*result)
                    }
                    Err(CallDunderError::CallError(..)) => {
                        if emit_diagnostics {
                            if let Some(builder) =
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
                        }
                        false
                    }
                    Err(CallDunderError::MethodNotAvailable) => {
                        match object_ty.class_member(db, attribute.into()) {
                            meta_attr @ PlaceAndQualifiers { .. } if meta_attr.is_class_var() => {
                                if emit_diagnostics {
                                    if let Some(builder) =
                                        self.context.report_lint(&INVALID_ATTRIBUTE_ACCESS, target)
                                    {
                                        builder.into_diagnostic(format_args!(
                                            "Cannot assign to ClassVar `{attribute}` \
                                     from an instance of type `{ty}`",
                                            ty = object_ty.display(self.db()),
                                        ));
                                    }
                                }
                                false
                            }
                            PlaceAndQualifiers {
                                place: Place::Defined(meta_attr_ty, _, meta_attr_boundness),
                                qualifiers,
                            } => {
                                if invalid_assignment_to_final(self, qualifiers) {
                                    return false;
                                }

                                let assignable_to_meta_attr =
                                    if let Place::Defined(meta_dunder_set, _, _) =
                                        meta_attr_ty.class_member(db, "__set__".into()).place
                                    {
                                        // TODO: We could use the annotated parameter type of `__set__` as
                                        // type context here.
                                        let dunder_set_result = meta_dunder_set.try_call(
                                            db,
                                            &CallArguments::positional([
                                                meta_attr_ty,
                                                object_ty,
                                                value_ty,
                                            ]),
                                        );

                                        if emit_diagnostics {
                                            if let Err(dunder_set_failure) =
                                                dunder_set_result.as_ref()
                                            {
                                                report_bad_dunder_set_call(
                                                    &self.context,
                                                    dunder_set_failure,
                                                    attribute,
                                                    object_ty,
                                                    target,
                                                );
                                            }
                                        }

                                        dunder_set_result.is_ok()
                                    } else {
                                        let value_ty = infer_value_ty(
                                            self,
                                            TypeContext::new(Some(meta_attr_ty)),
                                        );

                                        ensure_assignable_to(self, value_ty, meta_attr_ty)
                                    };

                                let assignable_to_instance_attribute = if meta_attr_boundness
                                    == Definedness::PossiblyUndefined
                                {
                                    let (assignable, boundness) = if let PlaceAndQualifiers {
                                        place:
                                            Place::Defined(instance_attr_ty, _, instance_attr_boundness),
                                        qualifiers,
                                    } =
                                        object_ty.instance_member(db, attribute)
                                    {
                                        let value_ty = infer_value_ty(
                                            self,
                                            TypeContext::new(Some(instance_attr_ty)),
                                        );
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
                                        Place::Defined(instance_attr_ty, _, instance_attr_boundness),
                                    qualifiers,
                                } = object_ty.instance_member(db, attribute)
                                {
                                    let value_ty = infer_value_ty(
                                        self,
                                        TypeContext::new(Some(instance_attr_ty)),
                                    );
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
                                    if emit_diagnostics {
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
                        place: Place::Defined(meta_attr_ty, _, meta_attr_boundness),
                        qualifiers,
                    } => {
                        // We may have to perform multi-inference if the meta attribute is possibly unbound.
                        // However, we are required to perform the first inference without type context.
                        let value_ty = infer_value_ty(self, TypeContext::default());

                        if invalid_assignment_to_final(self, qualifiers) {
                            return false;
                        }

                        let assignable_to_meta_attr = if let Place::Defined(meta_dunder_set, _, _) =
                            meta_attr_ty.class_member(db, "__set__".into()).place
                        {
                            // TODO: We could use the annotated parameter type of `__set__` as
                            // type context here.
                            let dunder_set_result = meta_dunder_set.try_call(
                                db,
                                &CallArguments::positional([meta_attr_ty, object_ty, value_ty]),
                            );

                            if emit_diagnostics {
                                if let Err(dunder_set_failure) = dunder_set_result.as_ref() {
                                    report_bad_dunder_set_call(
                                        &self.context,
                                        dunder_set_failure,
                                        attribute,
                                        object_ty,
                                        target,
                                    );
                                }
                            }

                            dunder_set_result.is_ok()
                        } else {
                            let value_ty =
                                infer_value_ty(self, TypeContext::new(Some(meta_attr_ty)));
                            ensure_assignable_to(self, value_ty, meta_attr_ty)
                        };

                        let assignable_to_class_attr = if meta_attr_boundness
                            == Definedness::PossiblyUndefined
                        {
                            let (assignable, boundness) =
                                if let Place::Defined(class_attr_ty, _, class_attr_boundness) =
                                    object_ty
                                        .find_name_in_mro(db, attribute)
                                        .expect("called on Type::ClassLiteral or Type::SubclassOf")
                                        .place
                                {
                                    let value_ty =
                                        infer_value_ty(self, TypeContext::new(Some(class_attr_ty)));
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
                            place: Place::Defined(class_attr_ty, _, class_attr_boundness),
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
                if let Place::Defined(attr_ty, _, _) = module.static_member(db, attribute).place {
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

                    if emit_diagnostics {
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
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                let assigned_ty = infer_assigned_ty.map(|f| f(self, TypeContext::default()));

                if let Some(tuple_spec) =
                    assigned_ty.and_then(|ty| ty.tuple_instance_spec(self.db()))
                {
                    let assigned_tys = tuple_spec.all_elements().copied().collect::<Vec<_>>();

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
                let assigned_ty = infer_assigned_ty.map(|f| f(self, TypeContext::default()));
                self.store_expression_type(target, assigned_ty.unwrap_or(Type::unknown()));

                if let Some(assigned_ty) = assigned_ty {
                    self.validate_subscript_assignment(subscript_expr, value, assigned_ty);
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

        self.add_binding(target.into(), definition, |builder, tcx| {
            let target_ty = builder.infer_assignment_definition_impl(assignment, definition, tcx);
            builder.store_expression_type(target, target_ty);
            target_ty
        });
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

                    let ty = match callable_type
                        .as_class_literal()
                        .and_then(|cls| cls.known(self.db()))
                    {
                        Some(
                            typevar_class @ (KnownClass::TypeVar | KnownClass::ExtensionsTypeVar),
                        ) => {
                            self.infer_legacy_typevar(target, call_expr, definition, typevar_class)
                        }
                        Some(KnownClass::ParamSpec) => {
                            self.infer_paramspec(target, call_expr, definition)
                        }
                        Some(KnownClass::NewType) => {
                            self.infer_newtype_expression(target, call_expr, definition)
                        }
                        Some(_) | None => {
                            self.infer_call_expression_impl(call_expr, callable_type, tcx)
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
                    Type::BooleanLiteral(true)
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

    fn infer_paramspec(
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
            if let Some(builder) = context.report_lint(&INVALID_PARAMSPEC, node) {
                builder.into_diagnostic(message);
            }
            // If the call doesn't create a valid paramspec, we'll emit diagnostics and fall back to
            // just creating a regular instance of `typing.ParamSpec`.
            KnownClass::ParamSpec.to_instance(context.db())
        }

        let db = self.db();
        let arguments = &call_expr.arguments;
        let assume_all_features = self.in_stub();
        let python_version = Program::get(db).python_version(db);
        let have_features_from =
            |version: PythonVersion| assume_all_features || python_version >= version;

        let mut default = None;
        let mut name_param_ty = None;

        if arguments.args.len() > 1 {
            return error(
                &self.context,
                "`ParamSpec` can only have one positional argument",
                call_expr,
            );
        }

        if let Some(starred) = arguments.args.iter().find(|arg| arg.is_starred_expr()) {
            return error(
                &self.context,
                "Starred arguments are not supported in `ParamSpec` creation",
                starred,
            );
        }

        for kwarg in &arguments.keywords {
            let Some(identifier) = kwarg.arg.as_ref() else {
                return error(
                    &self.context,
                    "Starred arguments are not supported in `ParamSpec` creation",
                    kwarg,
                );
            };
            match identifier.id().as_str() {
                "name" => {
                    // Duplicate keyword argument is a syntax error, so we don't have to check if
                    // `name_param_ty.is_some()` here.
                    if !arguments.args.is_empty() {
                        return error(
                            &self.context,
                            "The `name` parameter of `ParamSpec` can only be provided once",
                            kwarg,
                        );
                    }
                    name_param_ty =
                        Some(self.infer_expression(&kwarg.value, TypeContext::default()));
                }
                "bound" | "covariant" | "contravariant" | "infer_variance" => {
                    return error(
                        &self.context,
                        "The variance and bound arguments for `ParamSpec` do not have defined semantics yet",
                        call_expr,
                    );
                }
                "default" => {
                    if !have_features_from(PythonVersion::PY313) {
                        // We don't return here; this error is informational since this will error
                        // at runtime, but the user's intent is plain, we may as well respect it.
                        error(
                            &self.context,
                            "The `default` parameter of `typing.ParamSpec` was added in Python 3.13",
                            kwarg,
                        );
                    }
                    default = Some(TypeVarDefaultEvaluation::Lazy);
                }
                name => {
                    // We don't return here; this error is informational since this will error
                    // at runtime, but it will likely cause fewer cascading errors if we just
                    // ignore the unknown keyword and still understand as much of the typevar as we
                    // can.
                    error(
                        &self.context,
                        format_args!("Unknown keyword argument `{name}` in `ParamSpec` creation"),
                        kwarg,
                    );
                    self.infer_expression(&kwarg.value, TypeContext::default());
                }
            }
        }

        let Some(name_param_ty) = name_param_ty.or_else(|| {
            arguments
                .find_positional(0)
                .map(|arg| self.infer_expression(arg, TypeContext::default()))
        }) else {
            return error(
                &self.context,
                "The `name` parameter of `ParamSpec` is required.",
                call_expr,
            );
        };

        let Some(name_param) = name_param_ty.as_string_literal().map(|name| name.value(db)) else {
            return error(
                &self.context,
                "The first argument to `ParamSpec` must be a string literal",
                call_expr,
            );
        };

        let ast::Expr::Name(ast::ExprName {
            id: target_name, ..
        }) = target
        else {
            return error(
                &self.context,
                "A `ParamSpec` definition must be a simple variable assignment",
                target,
            );
        };

        if name_param != target_name {
            return error(
                &self.context,
                format_args!(
                    "The name of a `ParamSpec` (`{name_param}`) must match \
                    the name of the variable it is assigned to (`{target_name}`)"
                ),
                target,
            );
        }

        if default.is_some() {
            self.deferred.insert(definition, self.multi_inference_state);
        }

        let identity =
            TypeVarIdentity::new(db, target_name, Some(definition), TypeVarKind::ParamSpec);
        Type::KnownInstance(KnownInstanceType::TypeVar(TypeVarInstance::new(
            db, identity, None, None, default,
        )))
    }

    fn infer_legacy_typevar(
        &mut self,
        target: &ast::Expr,
        call_expr: &ast::ExprCall,
        definition: Definition<'db>,
        known_class: KnownClass,
    ) -> Type<'db> {
        fn error<'db>(
            context: &InferContext<'db, '_>,
            message: impl std::fmt::Display,
            node: impl Ranged,
        ) -> Type<'db> {
            if let Some(builder) = context.report_lint(&INVALID_LEGACY_TYPE_VARIABLE, node) {
                builder.into_diagnostic(message);
            }
            // If the call doesn't create a valid typevar, we'll emit diagnostics and fall back to
            // just creating a regular instance of `typing.TypeVar`.
            KnownClass::TypeVar.to_instance(context.db())
        }

        let db = self.db();
        let arguments = &call_expr.arguments;
        let is_typing_extensions = known_class == KnownClass::ExtensionsTypeVar;
        let assume_all_features = self.in_stub() || is_typing_extensions;
        let python_version = Program::get(db).python_version(db);
        let have_features_from =
            |version: PythonVersion| assume_all_features || python_version >= version;

        let mut has_bound = false;
        let mut default = None;
        let mut covariant = false;
        let mut contravariant = false;
        let mut name_param_ty = None;

        if let Some(starred) = arguments.args.iter().find(|arg| arg.is_starred_expr()) {
            return error(
                &self.context,
                "Starred arguments are not supported in `TypeVar` creation",
                starred,
            );
        }

        for kwarg in &arguments.keywords {
            let Some(identifier) = kwarg.arg.as_ref() else {
                return error(
                    &self.context,
                    "Starred arguments are not supported in `TypeVar` creation",
                    kwarg,
                );
            };
            match identifier.id().as_str() {
                "name" => {
                    // Duplicate keyword argument is a syntax error, so we don't have to check if
                    // `name_param_ty.is_some()` here.
                    if !arguments.args.is_empty() {
                        return error(
                            &self.context,
                            "The `name` parameter of `TypeVar` can only be provided once.",
                            kwarg,
                        );
                    }
                    name_param_ty =
                        Some(self.infer_expression(&kwarg.value, TypeContext::default()));
                }
                "bound" => has_bound = true,
                "covariant" => {
                    match self
                        .infer_expression(&kwarg.value, TypeContext::default())
                        .bool(db)
                    {
                        Truthiness::AlwaysTrue => covariant = true,
                        Truthiness::AlwaysFalse => {}
                        Truthiness::Ambiguous => {
                            return error(
                                &self.context,
                                "The `covariant` parameter of `TypeVar` \
                                cannot have an ambiguous truthiness",
                                &kwarg.value,
                            );
                        }
                    }
                }
                "contravariant" => {
                    match self
                        .infer_expression(&kwarg.value, TypeContext::default())
                        .bool(db)
                    {
                        Truthiness::AlwaysTrue => contravariant = true,
                        Truthiness::AlwaysFalse => {}
                        Truthiness::Ambiguous => {
                            return error(
                                &self.context,
                                "The `contravariant` parameter of `TypeVar` \
                                cannot have an ambiguous truthiness",
                                &kwarg.value,
                            );
                        }
                    }
                }
                "default" => {
                    if !have_features_from(PythonVersion::PY313) {
                        // We don't return here; this error is informational since this will error
                        // at runtime, but the user's intent is plain, we may as well respect it.
                        error(
                            &self.context,
                            "The `default` parameter of `typing.TypeVar` was added in Python 3.13",
                            kwarg,
                        );
                    }

                    default = Some(TypeVarDefaultEvaluation::Lazy);
                }
                "infer_variance" => {
                    if !have_features_from(PythonVersion::PY312) {
                        // We don't return here; this error is informational since this will error
                        // at runtime, but the user's intent is plain, we may as well respect it.
                        error(
                            &self.context,
                            "The `infer_variance` parameter of `typing.TypeVar` was added in Python 3.12",
                            kwarg,
                        );
                    }
                    // TODO support `infer_variance` in legacy TypeVars
                    if self
                        .infer_expression(&kwarg.value, TypeContext::default())
                        .bool(db)
                        .is_ambiguous()
                    {
                        return error(
                            &self.context,
                            "The `infer_variance` parameter of `TypeVar` \
                            cannot have an ambiguous truthiness",
                            &kwarg.value,
                        );
                    }
                }
                name => {
                    // We don't return here; this error is informational since this will error
                    // at runtime, but it will likely cause fewer cascading errors if we just
                    // ignore the unknown keyword and still understand as much of the typevar as we
                    // can.
                    error(
                        &self.context,
                        format_args!("Unknown keyword argument `{name}` in `TypeVar` creation",),
                        kwarg,
                    );
                    self.infer_expression(&kwarg.value, TypeContext::default());
                }
            }
        }

        let variance = match (covariant, contravariant) {
            (true, true) => {
                return error(
                    &self.context,
                    "A `TypeVar` cannot be both covariant and contravariant",
                    call_expr,
                );
            }
            (true, false) => TypeVarVariance::Covariant,
            (false, true) => TypeVarVariance::Contravariant,
            (false, false) => TypeVarVariance::Invariant,
        };

        let Some(name_param_ty) = name_param_ty.or_else(|| {
            arguments
                .find_positional(0)
                .map(|arg| self.infer_expression(arg, TypeContext::default()))
        }) else {
            return error(
                &self.context,
                "The `name` parameter of `TypeVar` is required.",
                call_expr,
            );
        };

        let Some(name_param) = name_param_ty.as_string_literal().map(|name| name.value(db)) else {
            return error(
                &self.context,
                "The first argument to `TypeVar` must be a string literal.",
                call_expr,
            );
        };

        let ast::Expr::Name(ast::ExprName {
            id: target_name, ..
        }) = target
        else {
            return error(
                &self.context,
                "A `TypeVar` definition must be a simple variable assignment",
                target,
            );
        };

        if name_param != target_name {
            return error(
                &self.context,
                format_args!(
                    "The name of a `TypeVar` (`{name_param}`) must match \
                    the name of the variable it is assigned to (`{target_name}`)"
                ),
                target,
            );
        }

        // Inference of bounds, constraints, and defaults must be deferred, to avoid cycles. So we
        // only check presence/absence/number here.

        let num_constraints = arguments.args.len().saturating_sub(1);

        let bound_or_constraints = match (has_bound, num_constraints) {
            (false, 0) => None,
            (true, 0) => Some(TypeVarBoundOrConstraintsEvaluation::LazyUpperBound),
            (true, _) => {
                return error(
                    &self.context,
                    "A `TypeVar` cannot have both a bound and constraints",
                    call_expr,
                );
            }
            (_, 1) => {
                return error(
                    &self.context,
                    "A `TypeVar` cannot have exactly one constraint",
                    &arguments.args[1],
                );
            }
            (false, _) => Some(TypeVarBoundOrConstraintsEvaluation::LazyConstraints),
        };

        if bound_or_constraints.is_some() || default.is_some() {
            self.deferred.insert(definition, self.multi_inference_state);
        }

        let identity = TypeVarIdentity::new(db, target_name, Some(definition), TypeVarKind::Legacy);
        Type::KnownInstance(KnownInstanceType::TypeVar(TypeVarInstance::new(
            db,
            identity,
            bound_or_constraints,
            Some(variance),
            default,
        )))
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
                    "Wrong number of arguments in `NewType` creation, expected 2, found {}",
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

    fn infer_assignment_deferred(&mut self, value: &ast::Expr) {
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
        let known_class = func_ty
            .as_class_literal()
            .and_then(|cls| cls.known(self.db()));
        if let Some(KnownClass::NewType) = known_class {
            self.infer_newtype_assignment_deferred(arguments);
            return;
        }
        for arg in arguments.args.iter().skip(1) {
            self.infer_type_expression(arg);
        }
        if let Some(bound) = arguments.find_keyword("bound") {
            self.infer_type_expression(&bound.value);
        }
        if let Some(default) = arguments.find_keyword("default") {
            if let Some(KnownClass::ParamSpec) = known_class {
                self.infer_paramspec_default(&default.value);
            } else {
                self.infer_type_expression(&default.value);
            }
        }
    }

    // Infer the deferred base type of a NewType.
    fn infer_newtype_assignment_deferred(&mut self, arguments: &ast::Arguments) {
        match self.infer_type_expression(&arguments.args[1]) {
            Type::NominalInstance(_) | Type::NewTypeInstance(_) => {}
            // `Unknown` is likely to be the result of an unresolved import or a typo, which will
            // already get a diagnostic, so don't pile on an extra diagnostic here.
            Type::Dynamic(DynamicType::Unknown) => {}
            other_type => {
                if let Some(builder) = self
                    .context
                    .report_lint(&INVALID_NEWTYPE, &arguments.args[1])
                {
                    let mut diag = builder.into_diagnostic("invalid base for `typing.NewType`");
                    diag.set_primary_message(format!("type `{}`", other_type.display(self.db())));
                    if matches!(other_type, Type::ProtocolInstance(_)) {
                        diag.info("The base of a `NewType` is not allowed to be a protocol class.");
                    } else if matches!(other_type, Type::TypedDict(_)) {
                        diag.info("The base of a `NewType` is not allowed to be a `TypedDict`.");
                    } else {
                        diag.info(
                            "The base of a `NewType` must be a class type or another `NewType`.",
                        );
                    }
                }
            }
        }
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
            let annotated =
                self.infer_annotation_expression(annotation, DeferredExpressionState::None);

            if !annotated.qualifiers.is_empty() {
                for qualifier in [TypeQualifiers::CLASS_VAR, TypeQualifiers::INIT_VAR] {
                    if annotated.qualifiers.contains(qualifier) {
                        if let Some(builder) = self
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
            }

            if let Some(value) = value {
                self.infer_maybe_standalone_expression(
                    value,
                    TypeContext::new(Some(annotated.inner_type())),
                );
            }

            // If we have an annotated assignment like `self.attr: int = 1`, we still need to
            // do type inference on the `self.attr` target to get types for all sub-expressions.
            self.infer_expression(target, TypeContext::default());

            // But here we explicitly overwrite the type for the overall `self.attr` node with
            // the annotated type. We do no use `store_expression_type` here, because it checks
            // that no type has been stored for the expression before.
            self.expressions
                .insert((&**target).into(), annotated.inner_type());
        }
    }

    /// Infer the types in an annotated assignment definition.
    fn infer_annotated_assignment_definition(
        &mut self,
        assignment: &'db AnnotatedAssignmentDefinitionKind,
        definition: Definition<'db>,
    ) {
        let annotation = assignment.annotation(self.module());
        let target = assignment.target(self.module());
        let value = assignment.value(self.module());

        let mut declared = self.infer_annotation_expression_allow_pep_613(
            annotation,
            DeferredExpressionState::from(self.defer_annotations()),
        );

        if !declared.qualifiers.is_empty() {
            let current_scope_id = self.scope().file_scope_id(self.db());
            let current_scope = self.index.scope(current_scope_id);
            if current_scope.kind() != ScopeKind::Class {
                for qualifier in [TypeQualifiers::CLASS_VAR, TypeQualifiers::INIT_VAR] {
                    if declared.qualifiers.contains(qualifier) {
                        if let Some(builder) =
                            self.context.report_lint(&INVALID_TYPE_FORM, annotation)
                        {
                            builder.into_diagnostic(format_args!(
                                "`{name}` annotations are only allowed in class-body scopes",
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
            declared.inner = Type::BooleanLiteral(true);
        }

        // Check if this is a PEP 613 `TypeAlias`. (This must come below the SpecialForm handling
        // immediately below, since that can overwrite the type to be `TypeAlias`.)
        let is_pep_613_type_alias = declared.inner_type().is_typealias_special_form();

        // Handle various singletons.
        if let Some(name_expr) = target.as_name_expr() {
            if let Some(special_form) =
                SpecialFormType::try_from_file_and_name(self.db(), self.file(), &name_expr.id)
            {
                declared.inner = Type::SpecialForm(special_form);
            }
        }

        // If the target of an assignment is not one of the place expressions we support,
        // then they are not definitions, so we can only be here if the target is in a form supported as a place expression.
        // In this case, we can simply store types in `target` below, instead of calling `infer_expression` (which would return `Never`).
        debug_assert!(PlaceExpr::try_from_expr(target).is_some());

        if let Some(value) = value {
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
                    .as_class_literal()?;

                class_literal
                    .dataclass_params(db)
                    .map(|params| SmallVec::from(params.field_specifiers(db)))
                    .or_else(|| {
                        Some(SmallVec::from(
                            CodeGeneratorKind::from_class(db, class_literal, None)?
                                .dataclass_transformer_params()?
                                .field_specifiers(db),
                        ))
                    })
            }

            if let Some(specifiers) = field_specifiers(self.db(), self.index, self.scope()) {
                self.dataclass_field_specifiers = specifiers;
            }

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
                Type::BooleanLiteral(true)
            } else if self.in_stub() && value.is_ellipsis_literal_expr() {
                declared.inner_type()
            } else {
                inferred_ty
            };

            if is_pep_613_type_alias {
                self.add_declaration_with_binding(
                    target.into(),
                    definition,
                    &DeclaredAndInferredType::AreTheSame(TypeAndQualifiers::declared(inferred_ty)),
                );
            } else {
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
        value_type: Type<'db>,
    ) -> Type<'db> {
        // If the target defines, e.g., `__iadd__`, infer the augmented assignment as a call to that
        // dunder.
        let op = assignment.op;
        let db = self.db();

        let report_unsupported_augmented_op = |ctx: &mut InferContext| {
            let Some(builder) = ctx.report_lint(&UNSUPPORTED_OPERATOR, assignment) else {
                return;
            };
            builder.into_diagnostic(format_args!(
                "Operator `{op}=` is unsupported between objects of type `{}` and `{}`",
                target_type.display(db),
                value_type.display(db)
            ));
        };

        // Fall back to non-augmented binary operator inference.
        let mut binary_return_ty = || {
            self.infer_binary_expression_type(assignment.into(), false, target_type, value_type, op)
                .unwrap_or_else(|| {
                    report_unsupported_augmented_op(&mut self.context);
                    Type::unknown()
                })
        };

        match target_type {
            Type::Union(union) => union.map(db, |&elem_type| {
                self.infer_augmented_op(assignment, elem_type, value_type)
            }),
            _ => {
                let call = target_type.try_call_dunder(
                    db,
                    op.in_place_dunder(),
                    CallArguments::positional([value_type]),
                    TypeContext::default(),
                );

                match call {
                    Ok(outcome) => outcome.return_type(db),
                    Err(CallDunderError::MethodNotAvailable) => binary_return_ty(),
                    Err(CallDunderError::PossiblyUnbound(outcome)) => {
                        UnionType::from_elements(db, [outcome.return_type(db), binary_return_ty()])
                    }
                    Err(CallDunderError::CallError(_, bindings)) => {
                        report_unsupported_augmented_op(&mut self.context);
                        bindings.return_type(db)
                    }
                }
            }
        }
    }

    fn infer_augment_assignment_definition(
        &mut self,
        assignment: &ast::StmtAugAssign,
        definition: Definition<'db>,
    ) {
        let target_ty = self.infer_augment_assignment(assignment);
        self.add_binding(assignment.into(), definition, |_, _| target_ty);
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
        let value_type = self.infer_expression(value, TypeContext::default());

        self.infer_augmented_op(assignment, target_type, value_type)
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

        self.infer_target(target, iter, |builder, tcx| {
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
        self.add_binding(target.into(), definition, |_, _| loop_var_value_type);
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

    fn infer_import_statement(&mut self, import: &ast::StmtImport) {
        let ast::StmtImport {
            range: _,
            node_index: _,
            names,
        } = import;

        for alias in names {
            self.infer_definition(alias);
        }
    }

    fn report_unresolved_import(
        &self,
        import_node: AnyNodeRef<'_>,
        range: TextRange,
        level: u32,
        module: Option<&str>,
    ) {
        let is_import_reachable = self.is_reachable(import_node);

        if !is_import_reachable {
            return;
        }

        let Some(builder) = self.context.report_lint(&UNRESOLVED_IMPORT, range) else {
            return;
        };
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Cannot resolve imported module `{}`",
            format_import_from_module(level, module)
        ));

        if level == 0 {
            if let Some(module_name) = module.and_then(ModuleName::new) {
                let program = Program::get(self.db());
                let typeshed_versions = program.search_paths(self.db()).typeshed_versions();

                // Loop over ancestors in case we have info on the parent module but not submodule
                for module_name in module_name.ancestors() {
                    if let Some(version_range) = typeshed_versions.exact(&module_name) {
                        // We know it is a stdlib module on *some* Python versions...
                        let python_version = program.python_version(self.db());
                        if !version_range.contains(python_version) {
                            // ...But not on *this* Python version.
                            diagnostic.info(format_args!(
                                "The stdlib module `{module_name}` is only available on Python {version_range}",
                                version_range = version_range.diagnostic_display(),
                            ));
                            add_inferred_python_version_hint_to_diagnostic(
                                self.db(),
                                &mut diagnostic,
                                "resolving modules",
                            );
                            return;
                        }
                        // We found the most precise answer we could, stop searching
                        break;
                    }
                }
            }
        } else {
            if let Some(better_level) = (0..level).rev().find(|reduced_level| {
                let Ok(module_name) = ModuleName::from_identifier_parts(
                    self.db(),
                    self.file(),
                    module,
                    *reduced_level,
                ) else {
                    return false;
                };
                resolve_module(self.db(), self.file(), &module_name).is_some()
            }) {
                diagnostic
                    .help("The module can be resolved if the number of leading dots is reduced");
                diagnostic.help(format_args!(
                    "Did you mean `{}`?",
                    format_import_from_module(better_level, module)
                ));
                diagnostic.set_concise_message(format_args!(
                    "Cannot resolve imported module `{}` - did you mean `{}`?",
                    format_import_from_module(level, module),
                    format_import_from_module(better_level, module)
                ));
            }
        }

        // Add search paths information to the diagnostic
        // Use the same search paths function that is used in actual module resolution
        let verbose = self.db().verbose();
        let search_paths = search_paths(self.db(), ModuleResolveMode::StubsAllowed);

        diagnostic.info(format_args!(
            "Searched in the following paths during module resolution:"
        ));

        let mut search_paths = search_paths.enumerate();

        while let Some((index, path)) = search_paths.next() {
            if index > 4 && !verbose {
                let more = search_paths.count() + 1;
                diagnostic.info(format_args!(
                    "  ... and {more} more paths. Run with `-v` to see all paths."
                ));
                break;
            }
            diagnostic.info(format_args!(
                "  {}. {} ({})",
                index + 1,
                path,
                path.describe_kind()
            ));
        }

        diagnostic.info(
            "make sure your Python environment is properly configured: \
                https://docs.astral.sh/ty/modules/#python-environment",
        );
    }

    fn infer_import_definition(
        &mut self,
        node: &ast::StmtImport,
        alias: &ast::Alias,
        definition: Definition<'db>,
    ) {
        let ast::Alias {
            range: _,
            node_index: _,
            name,
            asname,
        } = alias;

        // The name of the module being imported
        let Some(full_module_name) = ModuleName::new(name) else {
            tracing::debug!("Failed to resolve import due to invalid syntax");
            self.add_unknown_declaration_with_binding(alias.into(), definition);
            return;
        };

        // Resolve the module being imported.
        let Some(full_module_ty) = self.module_type_from_name(&full_module_name) else {
            self.report_unresolved_import(node.into(), alias.range(), 0, Some(name));
            self.add_unknown_declaration_with_binding(alias.into(), definition);
            return;
        };

        let binding_ty = if asname.is_some() {
            // If we are renaming the imported module via an `as` clause, then we bind the resolved
            // module's type to that name, even if that module is nested.
            full_module_ty
        } else if full_module_name.contains('.') {
            // If there's no `as` clause and the imported module is nested, we're not going to bind
            // the resolved module itself into the current scope; we're going to bind the top-most
            // parent package of that module.
            let topmost_parent_name =
                ModuleName::new(full_module_name.components().next().unwrap()).unwrap();
            let Some(topmost_parent_ty) = self.module_type_from_name(&topmost_parent_name) else {
                self.add_unknown_declaration_with_binding(alias.into(), definition);
                return;
            };
            topmost_parent_ty
        } else {
            // If there's no `as` clause and the imported module isn't nested, then the imported
            // module _is_ what we bind into the current scope.
            full_module_ty
        };

        self.add_declaration_with_binding(
            alias.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(binding_ty),
        );
    }

    fn infer_import_from_statement(&mut self, import: &ast::StmtImportFrom) {
        let ast::StmtImportFrom {
            range: _,
            node_index: _,
            module: _,
            names,
            level: _,
        } = import;

        self.check_import_from_module_is_resolvable(import);

        for alias in names {
            for definition in self.index.definitions(alias) {
                let inferred = infer_definition_types(self.db(), *definition);
                // Check non-star imports for deprecations
                if definition.kind(self.db()).as_star_import().is_none() {
                    // In the initial cycle, `declaration_types()` is empty, so no deprecation check is performed.
                    for ty in inferred.declaration_types() {
                        self.check_deprecated(alias, ty.inner);
                    }
                }
                self.extend_definition(inferred);
            }
        }
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
            UnionType::from_elements(self.db(), [base_exception_type, base_exception_instance]);
        let can_be_exception_cause =
            UnionType::from_elements(self.db(), [can_be_raised, Type::none(self.db())]);

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

    /// Resolve the [`ModuleName`], and the type of the module, being referred to by an
    /// [`ast::StmtImportFrom`] node. Emit a diagnostic if the module cannot be resolved.
    fn check_import_from_module_is_resolvable(&mut self, import_from: &ast::StmtImportFrom) {
        let ast::StmtImportFrom { module, level, .. } = import_from;

        // For diagnostics, we want to highlight the unresolvable
        // module and not the entire `from ... import ...` statement.
        let module_ref = module
            .as_ref()
            .map(AnyNodeRef::from)
            .unwrap_or_else(|| AnyNodeRef::from(import_from));
        let module = module.as_deref();

        tracing::trace!(
            "Resolving import statement from module `{}` into file `{}`",
            format_import_from_module(*level, module),
            self.file().path(self.db()),
        );
        let module_name = ModuleName::from_import_statement(self.db(), self.file(), import_from);

        let module_name = match module_name {
            Ok(module_name) => module_name,
            Err(ModuleNameResolutionError::InvalidSyntax) => {
                tracing::debug!("Failed to resolve import due to invalid syntax");
                // Invalid syntax diagnostics are emitted elsewhere.
                return;
            }
            Err(ModuleNameResolutionError::TooManyDots) => {
                tracing::debug!(
                    "Relative module resolution `{}` failed: too many leading dots",
                    format_import_from_module(*level, module),
                );
                self.report_unresolved_import(
                    import_from.into(),
                    module_ref.range(),
                    *level,
                    module,
                );
                return;
            }
            Err(ModuleNameResolutionError::UnknownCurrentModule) => {
                tracing::debug!(
                    "Relative module resolution `{}` failed: could not resolve file `{}` to a module \
                    (try adjusting configured search paths?)",
                    format_import_from_module(*level, module),
                    self.file().path(self.db())
                );
                self.report_unresolved_import(
                    import_from.into(),
                    module_ref.range(),
                    *level,
                    module,
                );
                return;
            }
        };

        if resolve_module(self.db(), self.file(), &module_name).is_none() {
            self.report_unresolved_import(import_from.into(), module_ref.range(), *level, module);
        }
    }

    fn infer_import_from_definition(
        &mut self,
        import_from: &ast::StmtImportFrom,
        alias: &ast::Alias,
        definition: Definition<'db>,
    ) {
        let Ok(module_name) =
            ModuleName::from_import_statement(self.db(), self.file(), import_from)
        else {
            self.add_unknown_declaration_with_binding(alias.into(), definition);
            return;
        };

        let Some(module) = resolve_module(self.db(), self.file(), &module_name) else {
            self.add_unknown_declaration_with_binding(alias.into(), definition);
            return;
        };

        let module_ty = Type::module_literal(self.db(), self.file(), module);

        let name = if let Some(star_import) = definition.kind(self.db()).as_star_import() {
            self.index
                .place_table(self.scope().file_scope_id(self.db()))
                .symbol(star_import.symbol_id())
                .name()
        } else {
            &alias.name.id
        };

        // Avoid looking up attributes on a module if a module imports from itself
        // (e.g. `from parent import submodule` inside the `parent` module).
        let import_is_self_referential = module_ty
            .as_module_literal()
            .is_some_and(|module| Some(self.file()) == module.module(self.db()).file(self.db()));

        // Although it isn't the runtime semantics, we go to some trouble to prioritize a submodule
        // over module `__getattr__`, because that's what other type checkers do.
        let mut from_module_getattr = None;

        // First try loading the requested attribute from the module.
        if !import_is_self_referential {
            if let PlaceAndQualifiers {
                place: Place::Defined(ty, _, boundness),
                qualifiers,
            } = module_ty.member(self.db(), name)
            {
                if &alias.name != "*" && boundness == Definedness::PossiblyUndefined {
                    // TODO: Consider loading _both_ the attribute and any submodule and unioning them
                    // together if the attribute exists but is possibly-unbound.
                    if let Some(builder) = self
                        .context
                        .report_lint(&POSSIBLY_MISSING_IMPORT, AnyNodeRef::Alias(alias))
                    {
                        builder.into_diagnostic(format_args!(
                            "Member `{name}` of module `{module_name}` may be missing",
                        ));
                    }
                }
                if qualifiers.contains(TypeQualifiers::FROM_MODULE_GETATTR) {
                    from_module_getattr = Some((ty, qualifiers));
                } else {
                    self.add_declaration_with_binding(
                        alias.into(),
                        definition,
                        &DeclaredAndInferredType::MightBeDifferent {
                            declared_ty: TypeAndQualifiers {
                                inner: ty,
                                origin: TypeOrigin::Declared,
                                qualifiers,
                            },
                            inferred_ty: ty,
                        },
                    );
                    return;
                }
            }
        }

        // Evaluate whether `X.Y` would constitute a valid submodule name,
        // given a `from X import Y` statement. If it is valid, this will be `Some()`;
        // else, it will be `None`.
        let full_submodule_name = ModuleName::new(name).map(|final_part| {
            let mut ret = module_name.clone();
            ret.extend(&final_part);
            ret
        });

        // If the module doesn't bind the symbol, check if it's a submodule.  This won't get
        // handled by the `Type::member` call because it relies on the semantic index's
        // `imported_modules` set.  The semantic index does not include information about
        // `from...import` statements because there are two things it cannot determine while only
        // inspecting the content of the current file:
        //
        //   - whether the imported symbol is an attribute or submodule
        //   - whether the containing file is in a module or a package (needed to correctly resolve
        //     relative imports)
        //
        // The first would be solvable by making it a _potentially_ imported modules set.  The
        // second is not.
        //
        // Regardless, for now, we sidestep all of that by repeating the submodule-or-attribute
        // check here when inferring types for a `from...import` statement.
        if let Some(submodule_type) = full_submodule_name
            .as_ref()
            .and_then(|submodule_name| self.module_type_from_name(submodule_name))
        {
            self.add_declaration_with_binding(
                alias.into(),
                definition,
                &DeclaredAndInferredType::are_the_same_type(submodule_type),
            );
            return;
        }

        // We've checked for a submodule, so now we can go ahead and use a type from module
        // `__getattr__`.
        if let Some((ty, qualifiers)) = from_module_getattr {
            self.add_declaration_with_binding(
                alias.into(),
                definition,
                &DeclaredAndInferredType::MightBeDifferent {
                    declared_ty: TypeAndQualifiers {
                        inner: ty,
                        origin: TypeOrigin::Declared,
                        qualifiers,
                    },
                    inferred_ty: ty,
                },
            );
            return;
        }

        self.add_unknown_declaration_with_binding(alias.into(), definition);

        if &alias.name == "*" {
            return;
        }

        if !self.is_reachable(import_from) {
            return;
        }

        let Some(builder) = self
            .context
            .report_lint(&UNRESOLVED_IMPORT, AnyNodeRef::Alias(alias))
        else {
            return;
        };

        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Module `{module_name}` has no member `{name}`"
        ));

        let mut submodule_hint_added = false;

        if let Some(full_submodule_name) = full_submodule_name {
            submodule_hint_added = hint_if_stdlib_submodule_exists_on_other_versions(
                self.db(),
                &mut diagnostic,
                &full_submodule_name,
                module,
            );
        }

        if !submodule_hint_added {
            hint_if_stdlib_attribute_exists_on_other_versions(
                self.db(),
                diagnostic,
                module_ty,
                name,
                "resolving imports",
            );
        }
    }

    /// Infer the implicit local definition `x = <module 'whatever.thispackage.x'>` that
    /// `from .x.y import z` or `from whatever.thispackage.x.y` can introduce in `__init__.py(i)`.
    ///
    /// For the definition `z`, see [`TypeInferenceBuilder::infer_import_from_definition`].
    ///
    /// The runtime semantic of this kind of statement is to introduce a variable in the global
    /// scope of this module *the first time it's imported in the entire program*. This
    /// implementation just blindly introduces a local variable wherever the `from..import` is
    /// (if the imports actually resolve).
    ///
    /// That gap between the semantics and implementation are currently the responsibility of the
    /// code that actually creates these kinds of Definitions (so blindly introducing a local
    /// is all we need to be doing here).
    fn infer_import_from_submodule_definition(
        &mut self,
        import_from: &ast::StmtImportFrom,
        definition: Definition<'db>,
    ) {
        // Get this package's absolute module name by resolving `.`, and make sure it exists
        let Ok(thispackage_name) = ModuleName::package_for_file(self.db(), self.file()) else {
            self.add_binding(import_from.into(), definition, |_, _| Type::unknown());
            return;
        };
        let Some(module) = resolve_module(self.db(), self.file(), &thispackage_name) else {
            self.add_binding(import_from.into(), definition, |_, _| Type::unknown());
            return;
        };

        // We have `from whatever.thispackage.x.y ...` or `from .x.y ...`
        // and we want to extract `x` (to ultimately construct `whatever.thispackage.x`):

        // First we normalize to `whatever.thispackage.x.y`
        let Some(final_part) = ModuleName::from_identifier_parts(
            self.db(),
            self.file(),
            import_from.module.as_deref(),
            import_from.level,
        )
        .ok()
        // `whatever.thispackage.x.y` => `x.y`
        .and_then(|submodule_name| submodule_name.relative_to(&thispackage_name))
        // `x.y` => `x`
        .and_then(|relative_submodule_name| {
            relative_submodule_name
                .components()
                .next()
                .and_then(ModuleName::new)
        }) else {
            self.add_binding(import_from.into(), definition, |_, _| Type::unknown());
            return;
        };

        // `x` => `whatever.thispackage.x`
        let mut full_submodule_name = thispackage_name.clone();
        full_submodule_name.extend(&final_part);

        // Try to actually resolve the import `whatever.thispackage.x`
        if let Some(submodule_type) = self.module_type_from_name(&full_submodule_name) {
            // Success, introduce a binding!
            //
            // We explicitly don't introduce a *declaration* because it's actual ok
            // (and fairly common) to overwrite this import with a function or class
            // and we don't want it to be a type error to do so.
            self.add_binding(import_from.into(), definition, |_, _| submodule_type);
            return;
        }

        // That didn't work, try to produce diagnostics
        self.add_binding(import_from.into(), definition, |_, _| Type::unknown());

        if !self.is_reachable(import_from) {
            return;
        }

        let Some(builder) = self
            .context
            .report_lint(&UNRESOLVED_IMPORT, AnyNodeRef::StmtImportFrom(import_from))
        else {
            return;
        };

        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Module `{thispackage_name}` has no submodule `{final_part}`"
        ));

        hint_if_stdlib_submodule_exists_on_other_versions(
            self.db(),
            &mut diagnostic,
            &full_submodule_name,
            module,
        );
    }

    fn infer_return_statement(&mut self, ret: &ast::StmtReturn) {
        let tcx = if ret.value.is_some() {
            nearest_enclosing_function(self.db(), self.index, self.scope())
                .map(|func| {
                    // When inferring expressions within a function body,
                    // the expected type passed should be the "raw" type,
                    // i.e. type variables in the return type are non-inferable,
                    // and the return types of async functions are not wrapped in `CoroutineType[...]`.
                    TypeContext::new(func.last_definition_raw_signature(self.db()).return_ty)
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

    fn infer_and_check_argument_types(
        &mut self,
        ast_arguments: &ast::Arguments,
        argument_types: &mut CallArguments<'_, 'db>,
        bindings: &mut Bindings<'db>,
        call_expression_tcx: TypeContext<'db>,
    ) -> Result<(), CallErrorKind> {
        let db = self.db();

        // If the type context is a union, attempt to narrow to a specific element.
        let narrow_targets: &[_] = match call_expression_tcx.annotation {
            // TODO: We could theoretically attempt to narrow to every element of
            // the power set of this union. However, this leads to an exponential
            // explosion of inference attempts, and is rarely needed in practice.
            Some(Type::Union(union)) => union.elements(db),
            _ => &[],
        };

        // We silence diagnostics until we successfully narrow to a specific type.
        let mut speculated_bindings = bindings.clone();
        let was_in_multi_inference = self.context.set_multi_inference(true);

        let mut try_narrow = |narrowed_ty| {
            let narrowed_tcx = TypeContext::new(Some(narrowed_ty));

            // Attempt to infer the argument types using the narrowed type context.
            self.infer_all_argument_types(
                ast_arguments,
                argument_types,
                bindings,
                narrowed_tcx,
                MultiInferenceState::Ignore,
            );

            // Ensure the argument types match their annotated types.
            if speculated_bindings
                .check_types_impl(
                    db,
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
            if speculated_bindings.constructor_instance_type().is_none()
                && !speculated_bindings
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
                    ast_arguments,
                    argument_types,
                    bindings,
                    narrowed_tcx,
                    MultiInferenceState::Intersect,
                );
            }

            Some(bindings.check_types_impl(
                db,
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
            bindings,
            call_expression_tcx,
            MultiInferenceState::Intersect,
        );

        bindings.check_types_impl(
            db,
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
        ast_arguments: &ast::Arguments,
        arguments_types: &mut CallArguments<'_, 'db>,
        bindings: &Bindings<'db>,
        call_expression_tcx: TypeContext<'db>,
        multi_inference_state: MultiInferenceState,
    ) {
        debug_assert_eq!(ast_arguments.len(), arguments_types.len());
        debug_assert_eq!(arguments_types.len(), bindings.argument_forms().len());

        let db = self.db();
        let iter = itertools::izip!(
            0..,
            arguments_types.iter_mut(),
            bindings.argument_forms().iter().copied(),
            ast_arguments.arguments_source_order()
        );

        let overloads_with_binding = bindings
            .iter()
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
                    overload.signature.parameters()[*parameter_index].annotated_type()?;

                // If this is a generic call, attempt to specialize the parameter type using the
                // declared type context, if provided.
                if let Some(generic_context) = overload.signature.generic_context
                    && let Some(return_ty) = overload.signature.return_ty
                    && let Some(declared_return_ty) = call_expression_tcx.annotation
                {
                    let mut builder =
                        SpecializationBuilder::new(db, generic_context.inferable_typevars(db));

                    let _ = builder.infer(return_ty, declared_return_ty);
                    let specialization = builder.build(generic_context);

                    parameter_type = parameter_type.apply_specialization(db, specialization);
                }

                // TODO: For now, skip any parameter annotations that still mention any typevars. There
                // are two issues:
                //
                // First, if we include those typevars in the type context that we use to infer the
                // corresponding argument type, the typevars might end up appearing in the inferred
                // argument type as well. As part of analyzing this call, we're going to (try to)
                // infer a specialization of those typevars, and would need to substitute those
                // typevars in the inferred argument type. We can't do that easily at the moment,
                // since specialization inference occurs _after_ we've inferred argument types, and
                // we can't _update_ an expression's inferred type after the fact.
                //
                // Second, certain kinds of arguments themselves have typevars that we need to
                // infer specializations for. (For instance, passing the result of _another_  call
                // to the argument of _this_ call, where both are calls to generic functions.) In
                // that case, we want to "tie together" the typevars of the two calls so that we
                // can infer their specializations at the same time — or at least, for the
                // specialization of one to influence the specialization of the other. It's not yet
                // clear how we're going to do that. (We might have to start inferring constraint
                // sets for each expression, instead of simple types?)
                //
                // Regardless, for now, the expedient "solution" is to not perform bidi type
                // checking for these kinds of parameters.
                if parameter_type.has_typevar(db) {
                    return None;
                }

                Some(parameter_type)
            };

            // If there is only a single binding and overload, we can infer the argument directly with
            // the unique parameter type annotation.
            if let Ok((overload, binding)) = overloads_with_binding.iter().exactly_one() {
                *argument_type = Some(self.infer_expression(
                    ast_argument,
                    TypeContext::new(parameter_type(overload, binding)),
                ));
            } else {
                // We perform inference once without any type context, emitting any diagnostics that are unrelated
                // to bidirectional type inference.
                *argument_type = Some(self.infer_expression(ast_argument, TypeContext::default()));

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
                    let inferred_ty =
                        self.infer_expression(ast_argument, TypeContext::new(Some(parameter_type)));

                    // Each type is a valid independent inference of the given argument, and we may require different
                    // permutations of argument types to correctly perform argument expansion during overload evaluation,
                    // so we take the intersection of all the types we inferred for each argument.
                    *argument_type = argument_type
                        .map(|current| IntersectionType::from_elements(db, [inferred_ty, current]))
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
        let ty = match expression {
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
            ast::Expr::Starred(starred) => self.infer_starred_expression(starred),
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

        self.store_expression_type(expression, ty);

        ty
    }

    #[track_caller]
    fn store_expression_type(&mut self, expression: &ast::Expr, ty: Type<'db>) {
        if self.deferred_state.in_string_annotation()
            || self.inner_expression_inference_state.is_get()
        {
            // Avoid storing the type of expressions that are part of a string annotation because
            // the expression ids don't exists in the semantic index. Instead, we'll store the type
            // on the string expression itself that represents the annotation.
            // Also, if `inner_expression_inference_state` is `Get`, the expression type has already been stored.
            return;
        }

        let db = self.db();

        match self.multi_inference_state {
            MultiInferenceState::Ignore => {}

            MultiInferenceState::Panic => {
                let previous = self.expressions.insert(expression.into(), ty);
                assert_eq!(previous, None);
            }

            MultiInferenceState::Overwrite => {
                self.expressions.insert(expression.into(), ty);
            }

            MultiInferenceState::Intersect => {
                self.expressions
                    .entry(expression.into())
                    .and_modify(|current| {
                        *current = IntersectionType::from_elements(db, [*current, ty]);
                    })
                    .or_insert(ty);
            }
        }
    }

    fn infer_number_literal_expression(&mut self, literal: &ast::ExprNumberLiteral) -> Type<'db> {
        let ast::ExprNumberLiteral {
            range: _,
            node_index: _,
            value,
        } = literal;
        let db = self.db();

        match value {
            ast::Number::Int(n) => n
                .as_i64()
                .map(Type::IntLiteral)
                .unwrap_or_else(|| KnownClass::Int.to_instance(db)),
            ast::Number::Float(_) => KnownClass::Float.to_instance(db),
            ast::Number::Complex { .. } => KnownClass::Complex.to_instance(db),
        }
    }

    #[expect(clippy::unused_self)]
    fn infer_boolean_literal_expression(&mut self, literal: &ast::ExprBooleanLiteral) -> Type<'db> {
        let ast::ExprBooleanLiteral {
            range: _,
            node_index: _,
            value,
        } = literal;

        Type::BooleanLiteral(*value)
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
            Type::LiteralString
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
                                    debug_text: _,
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
                                if !conversion.is_none() || format_spec.is_some() {
                                    collector.add_expression();
                                } else {
                                    if let Type::StringLiteral(literal) = ty.str(self.db()) {
                                        collector.push_str(literal.value(self.db()));
                                    } else {
                                        collector.add_expression();
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
        let ast::ExprTuple {
            range: _,
            node_index: _,
            elts,
            ctx: _,
            parenthesized: _,
        } = tuple;

        // Remove any union elements of that are unrelated to the tuple type.
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

        let annotated_tuple = tcx
            .known_specialization(self.db(), KnownClass::Tuple)
            .and_then(|specialization| {
                specialization
                    .tuple(self.db())
                    .expect("the specialization of `KnownClass::Tuple` must have a tuple spec")
                    .resize(self.db(), TupleLength::Fixed(elts.len()))
                    .ok()
            });

        let mut annotated_elt_tys = annotated_tuple.as_ref().map(Tuple::all_elements);

        let db = self.db();
        let element_types = elts.iter().map(|element| {
            let annotated_elt_ty = annotated_elt_tys.as_mut().and_then(Iterator::next).copied();
            self.infer_expression(element, TypeContext::new(annotated_elt_ty))
        });

        Type::heterogeneous_tuple(db, element_types)
    }

    fn infer_list_expression(&mut self, list: &ast::ExprList, tcx: TypeContext<'db>) -> Type<'db> {
        let ast::ExprList {
            range: _,
            node_index: _,
            elts,
            ctx: _,
        } = list;

        let elts = elts.iter().map(|elt| [Some(elt)]);
        let infer_elt_ty = |builder: &mut Self, elt, tcx| builder.infer_expression(elt, tcx);
        self.infer_collection_literal(elts, tcx, infer_elt_ty, KnownClass::List)
            .unwrap_or_else(|| {
                KnownClass::List.to_specialized_instance(self.db(), [Type::unknown()])
            })
    }

    fn infer_set_expression(&mut self, set: &ast::ExprSet, tcx: TypeContext<'db>) -> Type<'db> {
        let ast::ExprSet {
            range: _,
            node_index: _,
            elts,
        } = set;

        let elts = elts.iter().map(|elt| [Some(elt)]);
        let infer_elt_ty = |builder: &mut Self, elt, tcx| builder.infer_expression(elt, tcx);
        self.infer_collection_literal(elts, tcx, infer_elt_ty, KnownClass::Set)
            .unwrap_or_else(|| {
                KnownClass::Set.to_specialized_instance(self.db(), [Type::unknown()])
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
        if let Some(tcx) = tcx.annotation
            && let Some(typed_dict) = tcx
                .filter_union(self.db(), Type::is_typed_dict)
                .as_typed_dict()
            && let Some(ty) = self.infer_typed_dict_expression(dict, typed_dict, &mut item_types)
        {
            return ty;
        }

        // Avoid false positives for the functional `TypedDict` form, which is currently
        // unsupported.
        if let Some(Type::Dynamic(DynamicType::Todo(_))) = tcx.annotation {
            return KnownClass::Dict
                .to_specialized_instance(self.db(), [Type::unknown(), Type::unknown()]);
        }

        let items = items
            .iter()
            .map(|item| [item.key.as_ref(), Some(&item.value)]);

        // Avoid inferring the items multiple times if we already attempted to infer the
        // dictionary literal as a `TypedDict`. This also allows us to infer using the
        // type context of the expected `TypedDict` field.
        let infer_elt_ty = |builder: &mut Self, elt: &ast::Expr, tcx| {
            item_types
                .get(&elt.node_index().load())
                .copied()
                .unwrap_or_else(|| builder.infer_expression(elt, tcx))
        };

        self.infer_collection_literal(items, tcx, infer_elt_ty, KnownClass::Dict)
            .unwrap_or_else(|| {
                KnownClass::Dict
                    .to_specialized_instance(self.db(), [Type::unknown(), Type::unknown()])
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

            let value_ty = if let Some(Type::StringLiteral(key)) = key_ty
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
    fn infer_collection_literal<'expr, const N: usize, F, I>(
        &mut self,
        elts: I,
        tcx: TypeContext<'db>,
        mut infer_elt_expression: F,
        collection_class: KnownClass,
    ) -> Option<Type<'db>>
    where
        I: Iterator<Item = [Option<&'expr ast::Expr>; N]>,
        F: FnMut(&mut Self, &'expr ast::Expr, TypeContext<'db>) -> Type<'db>,
    {
        // Extract the type variable `T` from `list[T]` in typeshed.
        let elt_tys = |collection_class: KnownClass| {
            let class_literal = collection_class.try_to_class_literal(self.db())?;
            let generic_context = class_literal.generic_context(self.db())?;
            Some((
                class_literal,
                generic_context,
                generic_context.variables(self.db()),
            ))
        };

        let Some((class_literal, generic_context, elt_tys)) = elt_tys(collection_class) else {
            // Infer the element types without type context, and fallback to unknown for
            // custom typesheds.
            for elt in elts.flatten().flatten() {
                infer_elt_expression(self, elt, TypeContext::default());
            }

            return None;
        };

        let inferable = generic_context.inferable_typevars(self.db());

        // Remove any union elements of that are unrelated to the collection type.
        //
        // For example, we only want the `list[int]` from `annotation: list[int] | None` if
        // `collection_ty` is `list`.
        let tcx = tcx.map(|annotation| {
            let collection_ty = collection_class.to_instance(self.db());
            annotation.filter_disjoint_elements(self.db(), collection_ty, inferable)
        });

        // Extract the annotated type of `T`, if provided.
        let annotated_elt_tys = tcx
            .known_specialization(self.db(), collection_class)
            .map(|specialization| specialization.types(self.db()));

        // Create a set of constraints to infer a precise type for `T`.
        let mut builder = SpecializationBuilder::new(self.db(), inferable);

        match annotated_elt_tys {
            // The annotated type acts as a constraint for `T`.
            //
            // Note that we infer the annotated type _before_ the elements, to more closely match the
            // order of any unions as written in the type annotation.
            Some(annotated_elt_tys) => {
                for (elt_ty, annotated_elt_ty) in iter::zip(elt_tys.clone(), annotated_elt_tys) {
                    builder
                        .infer(Type::TypeVar(elt_ty), *annotated_elt_ty)
                        .ok()?;
                }
            }

            // If a valid type annotation was not provided, avoid restricting the type of the collection
            // by unioning the inferred type with `Unknown`.
            None => {
                for elt_ty in elt_tys.clone() {
                    builder.infer(Type::TypeVar(elt_ty), Type::unknown()).ok()?;
                }
            }
        }

        let elt_tcxs = match annotated_elt_tys {
            None => Either::Left(iter::repeat(TypeContext::default())),
            Some(tys) => Either::Right(tys.iter().map(|ty| TypeContext::new(Some(*ty)))),
        };

        for elts in elts {
            // An unpacking expression for a dictionary.
            if let &[None, Some(value)] = elts.as_slice() {
                let inferred_value_ty = infer_elt_expression(self, value, TypeContext::default());

                // Merge the inferred type of the nested dictionary.
                if let Some(specialization) =
                    inferred_value_ty.known_specialization(self.db(), KnownClass::Dict)
                {
                    for (elt_ty, inferred_elt_ty) in
                        iter::zip(elt_tys.clone(), specialization.types(self.db()))
                    {
                        builder
                            .infer(Type::TypeVar(elt_ty), *inferred_elt_ty)
                            .ok()?;
                    }
                }

                continue;
            }

            // The inferred type of each element acts as an additional constraint on `T`.
            for (elt, elt_ty, elt_tcx) in itertools::izip!(elts, elt_tys.clone(), elt_tcxs.clone())
            {
                let Some(elt) = elt else { continue };

                let inferred_elt_ty = infer_elt_expression(self, elt, elt_tcx);

                // Simplify the inference based on the declared type of the element.
                if let Some(elt_tcx) = elt_tcx.annotation {
                    if inferred_elt_ty.is_assignable_to(self.db(), elt_tcx) {
                        continue;
                    }
                }

                // Convert any element literals to their promoted type form to avoid excessively large
                // unions for large nested list literals, which the constraint solver struggles with.
                let inferred_elt_ty = inferred_elt_ty.promote_literals(self.db(), elt_tcx);

                builder.infer(Type::TypeVar(elt_ty), inferred_elt_ty).ok()?;
            }
        }

        let class_type =
            class_literal.apply_specialization(self.db(), |_| builder.build(generic_context));

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
        self.infer_standalone_expression(&first_comprehension.iter, TypeContext::default());

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

        let scope_id = self
            .index
            .node_scope(NodeWithScopeRef::GeneratorExpression(generator));
        let scope = scope_id.to_scope_id(self.db(), self.file());
        let inference = infer_scope_types(self.db(), scope);
        let yield_type = inference.expression_type(elt.as_ref());

        if evaluation_mode.is_async() {
            KnownClass::AsyncGeneratorType
                .to_specialized_instance(self.db(), [yield_type, Type::none(self.db())])
        } else {
            KnownClass::GeneratorType.to_specialized_instance(
                self.db(),
                [yield_type, Type::none(self.db()), Type::none(self.db())],
            )
        }
    }

    /// Return a specialization of the collection class (list, dict, set) based on the type context and the inferred
    /// element / key-value types from the comprehension expression.
    fn infer_comprehension_specialization(
        &self,
        collection_class: KnownClass,
        inferred_element_types: &[Type<'db>],
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        // Remove any union elements of that are unrelated to the collection type.
        let tcx = tcx.map(|annotation| {
            annotation.filter_disjoint_elements(
                self.db(),
                collection_class.to_instance(self.db()),
                InferableTypeVars::None,
            )
        });

        if let Some(annotated_element_types) = tcx
            .known_specialization(self.db(), collection_class)
            .map(|specialization| specialization.types(self.db()))
            && annotated_element_types
                .iter()
                .zip(inferred_element_types.iter())
                .all(|(annotated, inferred)| inferred.is_assignable_to(self.db(), *annotated))
        {
            collection_class
                .to_specialized_instance(self.db(), annotated_element_types.iter().copied())
        } else {
            collection_class.to_specialized_instance(
                self.db(),
                inferred_element_types.iter().map(|ty| {
                    UnionType::from_elements(
                        self.db(),
                        [
                            ty.promote_literals(self.db(), TypeContext::default()),
                            Type::unknown(),
                        ],
                    )
                }),
            )
        }
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

        let scope_id = self
            .index
            .node_scope(NodeWithScopeRef::ListComprehension(listcomp));
        let scope = scope_id.to_scope_id(self.db(), self.file());
        let inference = infer_scope_types(self.db(), scope);
        let element_type = inference.expression_type(elt.as_ref());

        self.infer_comprehension_specialization(KnownClass::List, &[element_type], tcx)
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

        let scope_id = self
            .index
            .node_scope(NodeWithScopeRef::DictComprehension(dictcomp));
        let scope = scope_id.to_scope_id(self.db(), self.file());
        let inference = infer_scope_types(self.db(), scope);
        let key_type = inference.expression_type(key.as_ref());
        let value_type = inference.expression_type(value.as_ref());

        self.infer_comprehension_specialization(KnownClass::Dict, &[key_type, value_type], tcx)
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

        let scope_id = self
            .index
            .node_scope(NodeWithScopeRef::SetComprehension(setcomp));
        let scope = scope_id.to_scope_id(self.db(), self.file());
        let inference = infer_scope_types(self.db(), scope);
        let element_type = inference.expression_type(elt.as_ref());

        self.infer_comprehension_specialization(KnownClass::Set, &[element_type], tcx)
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

    fn infer_list_comprehension_expression_scope(&mut self, listcomp: &ast::ExprListComp) {
        let ast::ExprListComp {
            range: _,
            node_index: _,
            elt,
            generators,
        } = listcomp;

        self.infer_expression(elt, TypeContext::default());
        self.infer_comprehensions(generators);
    }

    fn infer_dict_comprehension_expression_scope(&mut self, dictcomp: &ast::ExprDictComp) {
        let ast::ExprDictComp {
            range: _,
            node_index: _,
            key,
            value,
            generators,
        } = dictcomp;

        self.infer_expression(key, TypeContext::default());
        self.infer_expression(value, TypeContext::default());
        self.infer_comprehensions(generators);
    }

    fn infer_set_comprehension_expression_scope(&mut self, setcomp: &ast::ExprSetComp) {
        let ast::ExprSetComp {
            range: _,
            node_index: _,
            elt,
            generators,
        } = setcomp;

        self.infer_expression(elt, TypeContext::default());
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

        self.infer_target(target, iter, |builder, tcx| {
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
                builder.infer_standalone_expression(iter, tcx)
            }
            .iterate(builder.db())
            .homogeneous_element_type(builder.db())
        });

        for expr in ifs {
            self.infer_standalone_expression(expr, TypeContext::default());
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
        self.add_binding(target.into(), definition, |_, _| target_type);
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
        named: &ast::ExprNamed,
        definition: Definition<'db>,
    ) -> Type<'db> {
        let ast::ExprNamed {
            range: _,
            node_index: _,
            target,
            value,
        } = named;

        self.infer_expression(target, TypeContext::default());

        self.add_binding(named.target.as_ref().into(), definition, |builder, tcx| {
            builder.infer_expression(value, tcx)
        })
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

        let test_ty = self.infer_standalone_expression(test, TypeContext::default());
        let body_ty = self.infer_expression(body, tcx);
        let orelse_ty = self.infer_expression(orelse, tcx);

        match test_ty.try_bool(self.db()).unwrap_or_else(|err| {
            err.report_diagnostic(&self.context, &**test);
            err.fallback_truthiness()
        }) {
            Truthiness::AlwaysTrue => body_ty,
            Truthiness::AlwaysFalse => orelse_ty,
            Truthiness::Ambiguous => UnionType::from_elements(self.db(), [body_ty, orelse_ty]),
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

        let parameters = if let Some(parameters) = parameters {
            let positional_only = parameters
                .posonlyargs
                .iter()
                .map(|param| {
                    let mut parameter = Parameter::positional_only(Some(param.name().id.clone()));
                    if let Some(default) = param.default() {
                        parameter = parameter.with_default_type(
                            self.infer_expression(default, TypeContext::default())
                                .replace_parameter_defaults(self.db()),
                        );
                    }
                    parameter
                })
                .collect::<Vec<_>>();
            let positional_or_keyword = parameters
                .args
                .iter()
                .map(|param| {
                    let mut parameter = Parameter::positional_or_keyword(param.name().id.clone());
                    if let Some(default) = param.default() {
                        parameter = parameter.with_default_type(
                            self.infer_expression(default, TypeContext::default())
                                .replace_parameter_defaults(self.db()),
                        );
                    }
                    parameter
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
                    let mut parameter = Parameter::keyword_only(param.name().id.clone());
                    if let Some(default) = param.default() {
                        parameter = parameter.with_default_type(
                            self.infer_expression(default, TypeContext::default())
                                .replace_parameter_defaults(self.db()),
                        );
                    }
                    parameter
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

        // TODO: Useful inference of a lambda's return type will require a different approach,
        // which does the inference of the body expression based on arguments at each call site,
        // rather than eagerly computing a return type without knowing the argument types.
        Type::function_like_callable(self.db(), Signature::new(parameters, Some(Type::unknown())))
    }

    fn infer_call_expression(
        &mut self,
        call_expression: &ast::ExprCall,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        // TODO: Use the type context for more precise inference.
        let callable_type =
            self.infer_maybe_standalone_expression(&call_expression.func, TypeContext::default());

        self.infer_call_expression_impl(call_expression, callable_type, tcx)
    }

    fn infer_call_expression_impl(
        &mut self,
        call_expression: &ast::ExprCall,
        callable_type: Type<'db>,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let ast::ExprCall {
            range: _,
            node_index: _,
            func,
            arguments,
        } = call_expression;

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
            if let Type::TypedDict(typed_dict_ty) = value_type {
                if matches!(attr.id.as_str(), "pop" | "setdefault") && !arguments.args.is_empty() {
                    // Validate the key argument for `TypedDict` methods
                    if let Some(first_arg) = arguments.args.first() {
                        if let ast::Expr::StringLiteral(ast::ExprStringLiteral {
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
                                let key_ty = Type::StringLiteral(
                                    crate::types::StringLiteralType::new(self.db(), key),
                                );
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
            if !callable_type.is_subclass_of() {
                if let Some(protocol) = class.into_protocol_class(self.db()) {
                    report_attempted_protocol_instantiation(
                        &self.context,
                        call_expression,
                        protocol,
                    );
                }
            }

            // For class literals we model the entire class instantiation logic, so it is handled
            // in a separate function. For some known classes we have manual signatures defined and use
            // the `try_call` path below.
            // TODO: it should be possible to move these special cases into the `try_call_constructor`
            // path instead, or even remove some entirely once we support overloads fully.
            let has_special_cased_constructor = matches!(
                class.known(self.db()),
                Some(
                    KnownClass::Bool
                        | KnownClass::Str
                        | KnownClass::Type
                        | KnownClass::Object
                        | KnownClass::Property
                        | KnownClass::Super
                        | KnownClass::TypeAliasType
                        | KnownClass::Deprecated
                )
            ) || (
                // Constructor calls to `tuple` and subclasses of `tuple` are handled in `Type::Bindings`,
                // but constructor calls to `tuple[int]`, `tuple[int, ...]`, `tuple[int, *tuple[str, ...]]` (etc.)
                // are handled by the default constructor-call logic (we synthesize a `__new__` method for them
                // in `ClassType::own_class_member()`).
                class.is_known(self.db(), KnownClass::Tuple) && !class.is_generic()
            ) || CodeGeneratorKind::TypedDict.matches(
                self.db(),
                class.class_literal(self.db()).0,
                class.class_literal(self.db()).1,
            );

            // temporary special-casing for all subclasses of `enum.Enum`
            // until we support the functional syntax for creating enum classes
            if !has_special_cased_constructor
                && KnownClass::Enum
                    .to_class_literal(self.db())
                    .to_class_type(self.db())
                    .is_none_or(|enum_class| !class.is_subclass_of(self.db(), enum_class))
            {
                // Inference of correctly-placed `TypeVar`, `ParamSpec`, and `NewType` definitions
                // is done in `infer_legacy_typevar`, `infer_paramspec`, and
                // `infer_newtype_expression`, and doesn't use the full call-binding machinery. If
                // we reach here, it means that someone is trying to instantiate one of these in an
                // invalid context.
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
                    Some(KnownClass::ParamSpec) => {
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
                    _ => {}
                }

                let db = self.db();
                let infer_call_arguments = |bindings: Option<Bindings<'db>>| {
                    if let Some(bindings) = bindings {
                        let bindings = bindings.match_parameters(self.db(), &call_arguments);
                        self.infer_all_argument_types(
                            arguments,
                            &mut call_arguments,
                            &bindings,
                            tcx,
                            MultiInferenceState::Intersect,
                        );
                    } else {
                        let argument_forms = vec![Some(ParameterForm::Value); call_arguments.len()];
                        self.infer_argument_types(arguments, &mut call_arguments, &argument_forms);
                    }

                    call_arguments
                };

                return callable_type
                    .try_call_constructor(db, infer_call_arguments, tcx)
                    .unwrap_or_else(|err| {
                        err.report_diagnostic(&self.context, callable_type, call_expression.into());
                        err.return_type()
                    });
            }
        }

        let mut bindings = callable_type
            .bindings(self.db())
            .match_parameters(self.db(), &call_arguments);

        let bindings_result =
            self.infer_and_check_argument_types(arguments, &mut call_arguments, &mut bindings, tcx);

        // Validate `TypedDict` constructor calls after argument type inference
        if let Some(class_literal) = callable_type.as_class_literal() {
            if class_literal.is_typed_dict(self.db()) {
                let typed_dict_type = Type::typed_dict(ClassType::NonGeneric(class_literal));
                if let Some(typed_dict) = typed_dict_type.as_typed_dict() {
                    validate_typed_dict_constructor(
                        &self.context,
                        typed_dict,
                        arguments,
                        func.as_ref().into(),
                        |expr| self.expression_type(expr),
                    );
                }
            }
        }

        let mut bindings = match bindings_result {
            Ok(()) => bindings,
            Err(_) => {
                bindings.report_diagnostics(&self.context, call_expression.into());
                return bindings.return_type(self.db());
            }
        };

        for binding in &mut bindings {
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
            // TODO: TypeGuard
            Type::TypeIs(type_is) => match find_narrowed_place() {
                Some(place) => type_is.bind(db, scope, place),
                None => return_ty,
            },
            _ => return_ty,
        }
    }

    fn infer_starred_expression(&mut self, starred: &ast::ExprStarred) -> Type<'db> {
        let ast::ExprStarred {
            range: _,
            node_index: _,
            value,
            ctx: _,
        } = starred;

        let iterable_type = self.infer_expression(value, TypeContext::default());
        iterable_type
            .try_iterate(self.db())
            .map(|tuple| tuple.homogeneous_element_type(self.db()))
            .unwrap_or_else(|err| {
                err.report_diagnostic(&self.context, iterable_type, value.as_ref().into());
                err.fallback_element_type(self.db())
            });

        // TODO
        Type::Dynamic(DynamicType::TodoStarredExpression)
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
                    let union = union.build();
                    if union.is_assignable_to(db, ty) {
                        ty = union;
                    }
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

            let Some(builder) = self
                .context
                .report_lint(&crate::types::diagnostic::DEPRECATED, ranged)
            else {
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
            resolved_after_fallback.unwrap_with_diagnostic(|lookup_error| match lookup_error {
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
                place_from_bindings(db, use_def.all_reachable_bindings(place_id)).place
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
            if let Some(symbol) = place_expr.as_symbol() {
                if let Some(symbol_id) = place_table.symbol_id(symbol.name()) {
                    // Footgun: `place_expr` and `symbol` were probably constructed with all-zero
                    // flags. We need to read the place table to get correct flags.
                    symbol_resolves_locally = place_table.symbol(symbol_id).is_local();
                    // If we try to access a variable in a class before it has been defined, the
                    // lookup will fall back to global. See the comment on `Symbol::is_local`.
                    let fallback_to_global =
                        scope.node(db).scope_kind().is_class() && symbol_resolves_locally;
                    if self.skip_non_global_scopes(file_scope_id, symbol_id) || fallback_to_global {
                        return global_symbol(self.db(), self.file(), symbol.name()).map_type(
                            |ty| {
                                self.narrow_place_with_applicable_constraints(
                                    place_expr,
                                    ty,
                                    &constraint_keys,
                                )
                            },
                        );
                    }
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
                if let Place::Defined(_, _, _) = parent_place {
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
                    if let Place::Defined(type_, _, boundness) = local_place_and_qualifiers.place {
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

            PlaceAndQualifiers::from(Place::Undefined)
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
        // Subdiagnostic (2):
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
            if let Place::Defined(ty, _, Definedness::AlwaysDefined) = resolved.place {
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
        let resolved_type = fallback_place.unwrap_with_diagnostic(|lookup_err| match lookup_err {
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
                                .report_lint(&POSSIBLY_MISSING_ATTRIBUTE, attribute)
                            {
                                let mut diag = builder.into_diagnostic(format_args!(
                                    "Submodule `{attr_name}` may not be available as an attribute \
                                    on module `{module_name}`"
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
                        ) && !defined_type.member(db, attr_name).place.is_undefined()
                        {
                            diag.help(format_args!(
                                "Objects with type `{ty}` have a{maybe_n} `{attr_name}` attribute, but the symbol \
                                `{special_form}` does not itself inhabit the type `{ty}`",
                                maybe_n = if attr_name.starts_with(['a', 'e', 'i', 'o', 'u']) {
                                    "n"
                                } else {
                                    ""
                                },
                                ty = defined_type.display(self.db())
                            ));
                            if is_dotted_name(value) {
                                let source = &source_text(self.db(), self.file())[value.range()];
                                diag.help(format_args!(
                                    "This error may indicate that `{source}` was defined as \
                                    `{source} = {special_form}` when `{source}: {special_form}` \
                                    was intended"
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

                let diagnostic = match value_type {
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

                hint_if_stdlib_attribute_exists_on_other_versions(
                    db,
                    diagnostic,
                    value_type,
                    attr_name,
                    &format!("resolving the `{attr_name}` attribute"),
                );

                fallback()
            }
            LookupError::PossiblyUndefined(type_when_bound) => {
                report_possibly_missing_attribute(&self.context, attribute, &attr.id, value_type);

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
        match (op, operand_type) {
            (_, Type::Dynamic(_)) => operand_type,
            (_, Type::Never) => Type::Never,

            (_, Type::TypeAlias(alias)) => {
                self.infer_unary_expression_type(op, alias.value_type(self.db()), unary)
            }

            (ast::UnaryOp::UAdd, Type::IntLiteral(value)) => Type::IntLiteral(value),
            (ast::UnaryOp::USub, Type::IntLiteral(value)) => Type::IntLiteral(-value),
            (ast::UnaryOp::Invert, Type::IntLiteral(value)) => Type::IntLiteral(!value),

            (ast::UnaryOp::UAdd, Type::BooleanLiteral(bool)) => Type::IntLiteral(i64::from(bool)),
            (ast::UnaryOp::USub, Type::BooleanLiteral(bool)) => Type::IntLiteral(-i64::from(bool)),
            (ast::UnaryOp::Invert, Type::BooleanLiteral(bool)) => {
                Type::IntLiteral(!i64::from(bool))
            }

            (
                ast::UnaryOp::Invert,
                Type::KnownInstance(KnownInstanceType::ConstraintSet(constraints)),
            ) => {
                let constraints = constraints.constraints(self.db());
                let result = constraints.negate(self.db());
                Type::KnownInstance(KnownInstanceType::ConstraintSet(TrackedConstraintSet::new(
                    self.db(),
                    result,
                )))
            }

            (ast::UnaryOp::Not, ty) => ty
                .try_bool(self.db())
                .unwrap_or_else(|err| {
                    err.report_diagnostic(&self.context, unary);
                    err.fallback_truthiness()
                })
                .negate()
                .into_type(self.db()),
            (
                op @ (ast::UnaryOp::UAdd | ast::UnaryOp::USub | ast::UnaryOp::Invert),
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
                | Type::StringLiteral(_)
                | Type::LiteralString
                | Type::BytesLiteral(_)
                | Type::EnumLiteral(_)
                | Type::BoundSuper(_)
                | Type::TypeVar(_)
                | Type::TypeIs(_)
                | Type::TypedDict(_)
                | Type::NewTypeInstance(_),
            ) => {
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
                        if let Some(builder) =
                            self.context.report_lint(&UNSUPPORTED_OPERATOR, unary)
                        {
                            builder.into_diagnostic(format_args!(
                                "Unary operator `{op}` is unsupported for type `{}`",
                                operand_type.display(self.db()),
                            ));
                        }
                        e.fallback_return_type(self.db())
                    }
                }
            }
        }
    }

    fn infer_binary_expression(
        &mut self,
        binary: &ast::ExprBinOp,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        if tcx.is_typealias() {
            return self.infer_pep_604_union_type_alias(binary, tcx);
        }

        let ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
            node_index: _,
        } = binary;

        let left_ty = self.infer_expression(left, TypeContext::default());
        let right_ty = self.infer_expression(right, TypeContext::default());

        self.infer_binary_expression_type(binary.into(), false, left_ty, right_ty, *op)
            .unwrap_or_else(|| {
                let db = self.db();

                if let Some(builder) = self.context.report_lint(&UNSUPPORTED_OPERATOR, binary) {
                    let mut diag = builder.into_diagnostic(format_args!(
                        "Operator `{op}` is unsupported between objects of type `{}` and `{}`",
                        left_ty.display(db),
                        right_ty.display(db)
                    ));

                    if op == &ast::Operator::BitOr
                        && (left_ty.is_subtype_of(db, KnownClass::Type.to_instance(db))
                            || right_ty.is_subtype_of(db, KnownClass::Type.to_instance(db)))
                        && Program::get(db).python_version(db) < PythonVersion::PY310
                    {
                        diag.info(
                            "Note that `X | Y` PEP 604 union syntax is only available in Python 3.10 and later",
                        );
                        add_inferred_python_version_hint_to_diagnostic(db, &mut diag, "resolving types");
                    }
                }
                Type::unknown()
            })
    }

    fn infer_pep_604_union_type_alias(
        &mut self,
        node: &ast::ExprBinOp,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
            node_index: _,
        } = node;

        if *op != ast::Operator::BitOr {
            // TODO diagnostic?
            return Type::unknown();
        }

        let left_ty = self.infer_expression(left, tcx);
        let right_ty = self.infer_expression(right, tcx);

        // TODO this is overly aggressive; if the operands' `__or__` does not actually return a
        // `UnionType` at runtime, we should ideally not infer one here. But this is unlikely to be
        // a problem in practice: it would require someone having an explicitly annotated
        // `TypeAlias`, which uses `X | Y` syntax, where the returned type is not actually a union.
        // And attempting to enforce this more tightly showed a lot of potential false positives in
        // the ecosystem.
        if left_ty.is_equivalent_to(self.db(), right_ty) {
            left_ty
        } else {
            UnionTypeInstance::from_value_expression_types(
                self.db(),
                [left_ty, right_ty],
                self.scope(),
                self.typevar_binding_context,
            )
        }
    }

    fn infer_binary_expression_type(
        &mut self,
        node: AnyNodeRef<'_>,
        mut emitted_division_by_zero_diagnostic: bool,
        left_ty: Type<'db>,
        right_ty: Type<'db>,
        op: ast::Operator,
    ) -> Option<Type<'db>> {
        // Check for division by zero; this doesn't change the inferred type for the expression, but
        // may emit a diagnostic
        if !emitted_division_by_zero_diagnostic
            && matches!(
                (op, right_ty),
                (
                    ast::Operator::Div | ast::Operator::FloorDiv | ast::Operator::Mod,
                    Type::IntLiteral(0) | Type::BooleanLiteral(false)
                )
            )
        {
            emitted_division_by_zero_diagnostic = self.check_division_by_zero(node, op, left_ty);
        }

        let pep_604_unions_allowed = || {
            Program::get(self.db()).python_version(self.db()) >= PythonVersion::PY310
                || self.file().is_stub(self.db())
                || self.scope().scope(self.db()).in_type_checking_block()
        };

        match (left_ty, right_ty, op) {
            (Type::Union(lhs_union), rhs, _) => lhs_union.try_map(self.db(), |lhs_element| {
                self.infer_binary_expression_type(
                    node,
                    emitted_division_by_zero_diagnostic,
                    *lhs_element,
                    rhs,
                    op,
                )
            }),
            (lhs, Type::Union(rhs_union), _) => rhs_union.try_map(self.db(), |rhs_element| {
                self.infer_binary_expression_type(
                    node,
                    emitted_division_by_zero_diagnostic,
                    lhs,
                    *rhs_element,
                    op,
                )
            }),

            (Type::TypeAlias(alias), rhs, _) => self.infer_binary_expression_type(
                node,
                emitted_division_by_zero_diagnostic,
                alias.value_type(self.db()),
                rhs,
                op,
            ),

            (lhs, Type::TypeAlias(alias), _) => self.infer_binary_expression_type(
                node,
                emitted_division_by_zero_diagnostic,
                lhs,
                alias.value_type(self.db()),
                op,
            ),

            // Non-todo Anys take precedence over Todos (as if we fix this `Todo` in the future,
            // the result would then become Any or Unknown, respectively).
            (div @ Type::Dynamic(DynamicType::Divergent(_)), _, _)
            | (_, div @ Type::Dynamic(DynamicType::Divergent(_)), _) => Some(div),

            (any @ Type::Dynamic(DynamicType::Any), _, _)
            | (_, any @ Type::Dynamic(DynamicType::Any), _) => Some(any),

            (unknown @ Type::Dynamic(DynamicType::Unknown), _, _)
            | (_, unknown @ Type::Dynamic(DynamicType::Unknown), _) => Some(unknown),

            (unknown @ Type::Dynamic(DynamicType::UnknownGeneric(_)), _, _)
            | (_, unknown @ Type::Dynamic(DynamicType::UnknownGeneric(_)), _) => Some(unknown),

            (
                todo @ Type::Dynamic(
                    DynamicType::Todo(_)
                    | DynamicType::TodoUnpack
                    | DynamicType::TodoStarredExpression,
                ),
                _,
                _,
            )
            | (
                _,
                todo @ Type::Dynamic(
                    DynamicType::Todo(_)
                    | DynamicType::TodoUnpack
                    | DynamicType::TodoStarredExpression,
                ),
                _,
            ) => Some(todo),

            (Type::Never, _, _) | (_, Type::Never, _) => Some(Type::Never),

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Add) => Some(
                n.checked_add(m)
                    .map(Type::IntLiteral)
                    .unwrap_or_else(|| KnownClass::Int.to_instance(self.db())),
            ),

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Sub) => Some(
                n.checked_sub(m)
                    .map(Type::IntLiteral)
                    .unwrap_or_else(|| KnownClass::Int.to_instance(self.db())),
            ),

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Mult) => Some(
                n.checked_mul(m)
                    .map(Type::IntLiteral)
                    .unwrap_or_else(|| KnownClass::Int.to_instance(self.db())),
            ),

            (Type::IntLiteral(_), Type::IntLiteral(_), ast::Operator::Div) => {
                Some(KnownClass::Float.to_instance(self.db()))
            }

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::FloorDiv) => Some({
                let mut q = n.checked_div(m);
                let r = n.checked_rem(m);
                // Division works differently in Python than in Rust. If the result is negative and
                // there is a remainder, the division rounds down (instead of towards zero):
                if n.is_negative() != m.is_negative() && r.unwrap_or(0) != 0 {
                    q = q.map(|q| q - 1);
                }
                q.map(Type::IntLiteral)
                    .unwrap_or_else(|| KnownClass::Int.to_instance(self.db()))
            }),

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Mod) => Some({
                let mut r = n.checked_rem(m);
                // Division works differently in Python than in Rust. If the result is negative and
                // there is a remainder, the division rounds down (instead of towards zero). Adjust
                // the remainder to compensate so that q * m + r == n:
                if n.is_negative() != m.is_negative() && r.unwrap_or(0) != 0 {
                    r = r.map(|x| x + m);
                }
                r.map(Type::IntLiteral)
                    .unwrap_or_else(|| KnownClass::Int.to_instance(self.db()))
            }),

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Pow) => Some({
                if m < 0 {
                    KnownClass::Float.to_instance(self.db())
                } else {
                    u32::try_from(m)
                        .ok()
                        .and_then(|m| n.checked_pow(m))
                        .map(Type::IntLiteral)
                        .unwrap_or_else(|| KnownClass::Int.to_instance(self.db()))
                }
            }),

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::BitOr) => {
                Some(Type::IntLiteral(n | m))
            }

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::BitAnd) => {
                Some(Type::IntLiteral(n & m))
            }

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::BitXor) => {
                Some(Type::IntLiteral(n ^ m))
            }

            (Type::BytesLiteral(lhs), Type::BytesLiteral(rhs), ast::Operator::Add) => {
                let bytes = [lhs.value(self.db()), rhs.value(self.db())].concat();
                Some(Type::bytes_literal(self.db(), &bytes))
            }

            (Type::StringLiteral(lhs), Type::StringLiteral(rhs), ast::Operator::Add) => {
                let lhs_value = lhs.value(self.db()).to_string();
                let rhs_value = rhs.value(self.db());
                let ty = if lhs_value.len() + rhs_value.len() <= Self::MAX_STRING_LITERAL_SIZE {
                    Type::string_literal(self.db(), &(lhs_value + rhs_value))
                } else {
                    Type::LiteralString
                };
                Some(ty)
            }

            (
                Type::StringLiteral(_) | Type::LiteralString,
                Type::StringLiteral(_) | Type::LiteralString,
                ast::Operator::Add,
            ) => Some(Type::LiteralString),

            (Type::StringLiteral(s), Type::IntLiteral(n), ast::Operator::Mult)
            | (Type::IntLiteral(n), Type::StringLiteral(s), ast::Operator::Mult) => {
                let ty = if n < 1 {
                    Type::string_literal(self.db(), "")
                } else if let Ok(n) = usize::try_from(n) {
                    if n.checked_mul(s.value(self.db()).len())
                        .is_some_and(|new_length| new_length <= Self::MAX_STRING_LITERAL_SIZE)
                    {
                        let new_literal = s.value(self.db()).repeat(n);
                        Type::string_literal(self.db(), &new_literal)
                    } else {
                        Type::LiteralString
                    }
                } else {
                    Type::LiteralString
                };
                Some(ty)
            }

            (Type::LiteralString, Type::IntLiteral(n), ast::Operator::Mult)
            | (Type::IntLiteral(n), Type::LiteralString, ast::Operator::Mult) => {
                let ty = if n < 1 {
                    Type::string_literal(self.db(), "")
                } else {
                    Type::LiteralString
                };
                Some(ty)
            }

            (Type::BooleanLiteral(b1), Type::BooleanLiteral(b2), ast::Operator::BitOr) => {
                Some(Type::BooleanLiteral(b1 | b2))
            }

            (Type::BooleanLiteral(b1), Type::BooleanLiteral(b2), ast::Operator::BitAnd) => {
                Some(Type::BooleanLiteral(b1 & b2))
            }

            (Type::BooleanLiteral(b1), Type::BooleanLiteral(b2), ast::Operator::BitXor) => {
                Some(Type::BooleanLiteral(b1 ^ b2))
            }

            (Type::BooleanLiteral(b1), Type::BooleanLiteral(_) | Type::IntLiteral(_), op) => self
                .infer_binary_expression_type(
                    node,
                    emitted_division_by_zero_diagnostic,
                    Type::IntLiteral(i64::from(b1)),
                    right_ty,
                    op,
                ),
            (Type::IntLiteral(_), Type::BooleanLiteral(b2), op) => self
                .infer_binary_expression_type(
                    node,
                    emitted_division_by_zero_diagnostic,
                    left_ty,
                    Type::IntLiteral(i64::from(b2)),
                    op,
                ),

            (
                Type::KnownInstance(KnownInstanceType::ConstraintSet(left)),
                Type::KnownInstance(KnownInstanceType::ConstraintSet(right)),
                ast::Operator::BitAnd,
            ) => {
                let left = left.constraints(self.db());
                let right = right.constraints(self.db());
                let result = left.and(self.db(), || right);
                Some(Type::KnownInstance(KnownInstanceType::ConstraintSet(
                    TrackedConstraintSet::new(self.db(), result),
                )))
            }

            (
                Type::KnownInstance(KnownInstanceType::ConstraintSet(left)),
                Type::KnownInstance(KnownInstanceType::ConstraintSet(right)),
                ast::Operator::BitOr,
            ) => {
                let left = left.constraints(self.db());
                let right = right.constraints(self.db());
                let result = left.or(self.db(), || right);
                Some(Type::KnownInstance(KnownInstanceType::ConstraintSet(
                    TrackedConstraintSet::new(self.db(), result),
                )))
            }

            // PEP 604-style union types using the `|` operator.
            (
                Type::ClassLiteral(..)
                | Type::SubclassOf(..)
                | Type::GenericAlias(..)
                | Type::SpecialForm(_)
                | Type::KnownInstance(
                    KnownInstanceType::UnionType(_)
                    | KnownInstanceType::Literal(_)
                    | KnownInstanceType::Annotated(_)
                    | KnownInstanceType::TypeGenericAlias(_)
                    | KnownInstanceType::Callable(_)
                    | KnownInstanceType::TypeVar(_),
                ),
                Type::ClassLiteral(..)
                | Type::SubclassOf(..)
                | Type::GenericAlias(..)
                | Type::SpecialForm(_)
                | Type::KnownInstance(
                    KnownInstanceType::UnionType(_)
                    | KnownInstanceType::Literal(_)
                    | KnownInstanceType::Annotated(_)
                    | KnownInstanceType::TypeGenericAlias(_)
                    | KnownInstanceType::Callable(_)
                    | KnownInstanceType::TypeVar(_),
                ),
                ast::Operator::BitOr,
            ) if pep_604_unions_allowed() => {
                if left_ty.is_equivalent_to(self.db(), right_ty) {
                    Some(left_ty)
                } else {
                    Some(UnionTypeInstance::from_value_expression_types(
                        self.db(),
                        [left_ty, right_ty],
                        self.scope(),
                        self.typevar_binding_context,
                    ))
                }
            }
            (
                Type::ClassLiteral(..)
                | Type::SubclassOf(..)
                | Type::GenericAlias(..)
                | Type::KnownInstance(..)
                | Type::SpecialForm(..),
                Type::NominalInstance(instance),
                ast::Operator::BitOr,
            )
            | (
                Type::NominalInstance(instance),
                Type::ClassLiteral(..)
                | Type::SubclassOf(..)
                | Type::GenericAlias(..)
                | Type::KnownInstance(..)
                | Type::SpecialForm(..),
                ast::Operator::BitOr,
            ) if pep_604_unions_allowed()
                && instance.has_known_class(self.db(), KnownClass::NoneType) =>
            {
                Some(UnionTypeInstance::from_value_expression_types(
                    self.db(),
                    [left_ty, right_ty],
                    self.scope(),
                    self.typevar_binding_context,
                ))
            }

            // We avoid calling `type.__(r)or__`, as typeshed annotates these methods as
            // accepting `Any` (since typeforms are inexpressable in the type system currently).
            // This means that many common errors would not be caught if we fell back to typeshed's stubs here.
            //
            // Note that if a class had a custom metaclass that overrode `__(r)or__`, we would also ignore
            // that custom method as we'd take one of the earlier branches.
            // This seems like it's probably rare enough that it's acceptable, however.
            (
                Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..),
                _,
                ast::Operator::BitOr,
            )
            | (
                _,
                Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..),
                ast::Operator::BitOr,
            ) if pep_604_unions_allowed() => Type::try_call_bin_op_with_policy(
                self.db(),
                left_ty,
                ast::Operator::BitOr,
                right_ty,
                MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
            )
            .ok()
            .map(|binding| binding.return_type(self.db())),

            // We've handled all of the special cases that we support for literals, so we need to
            // fall back on looking for dunder methods on one of the operand types.
            (
                Type::FunctionLiteral(_)
                | Type::BooleanLiteral(_)
                | Type::Callable(..)
                | Type::BoundMethod(_)
                | Type::WrapperDescriptor(_)
                | Type::KnownBoundMethod(_)
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
                | Type::Intersection(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::IntLiteral(_)
                | Type::StringLiteral(_)
                | Type::LiteralString
                | Type::BytesLiteral(_)
                | Type::EnumLiteral(_)
                | Type::BoundSuper(_)
                | Type::TypeVar(_)
                | Type::TypeIs(_)
                | Type::TypedDict(_)
                | Type::NewTypeInstance(_),
                Type::FunctionLiteral(_)
                | Type::BooleanLiteral(_)
                | Type::Callable(..)
                | Type::BoundMethod(_)
                | Type::WrapperDescriptor(_)
                | Type::KnownBoundMethod(_)
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
                | Type::Intersection(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::IntLiteral(_)
                | Type::StringLiteral(_)
                | Type::LiteralString
                | Type::BytesLiteral(_)
                | Type::EnumLiteral(_)
                | Type::BoundSuper(_)
                | Type::TypeVar(_)
                | Type::TypeIs(_)
                | Type::TypedDict(_)
                | Type::NewTypeInstance(_),
                op,
            ) => Type::try_call_bin_op(self.db(), left_ty, op, right_ty)
                .map(|outcome| outcome.return_type(self.db()))
                .ok(),
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
                    builder.infer_standalone_expression(value, TypeContext::default())
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

                let ty = builder
                    .infer_binary_type_comparison(
                        left_ty,
                        *op,
                        right_ty,
                        range,
                        &BinaryComparisonVisitor::new(Ok(Type::BooleanLiteral(true))),
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
                            ast::CmpOp::In
                            | ast::CmpOp::NotIn
                            | ast::CmpOp::Is
                            | ast::CmpOp::IsNot => KnownClass::Bool.to_instance(builder.db()),
                            // Other operators can return arbitrary types
                            _ => Type::unknown(),
                        }
                    });

                (ty, range)
            },
        )
    }

    fn infer_binary_intersection_type_comparison(
        &mut self,
        intersection: IntersectionType<'db>,
        op: ast::CmpOp,
        other: Type<'db>,
        intersection_on: IntersectionOn,
        range: TextRange,
        visitor: &BinaryComparisonVisitor<'db>,
    ) -> Result<Type<'db>, UnsupportedComparisonError<'db>> {
        enum State<'db> {
            // We have not seen any positive elements (yet)
            NoPositiveElements,
            // The operator was unsupported on all elements that we have seen so far.
            // Contains the first error we encountered.
            UnsupportedOnAllElements(UnsupportedComparisonError<'db>),
            // The operator was supported on at least one positive element.
            Supported,
        }

        // If a comparison yields a definitive true/false answer on a (positive) part
        // of an intersection type, it will also yield a definitive answer on the full
        // intersection type, which is even more specific.
        for pos in intersection.positive(self.db()) {
            let result = match intersection_on {
                IntersectionOn::Left => {
                    self.infer_binary_type_comparison(*pos, op, other, range, visitor)
                }
                IntersectionOn::Right => {
                    self.infer_binary_type_comparison(other, op, *pos, range, visitor)
                }
            };

            if let Ok(Type::BooleanLiteral(_)) = result {
                return result;
            }
        }

        // For negative contributions to the intersection type, there are only a few
        // special cases that allow us to narrow down the result type of the comparison.
        for neg in intersection.negative(self.db()) {
            let result = match intersection_on {
                IntersectionOn::Left => self
                    .infer_binary_type_comparison(*neg, op, other, range, visitor)
                    .ok(),
                IntersectionOn::Right => self
                    .infer_binary_type_comparison(other, op, *neg, range, visitor)
                    .ok(),
            };

            match (op, result) {
                (ast::CmpOp::Is, Some(Type::BooleanLiteral(true))) => {
                    return Ok(Type::BooleanLiteral(false));
                }
                (ast::CmpOp::IsNot, Some(Type::BooleanLiteral(false))) => {
                    return Ok(Type::BooleanLiteral(true));
                }
                _ => {}
            }
        }

        // If none of the simplifications above apply, we still need to return *some*
        // result type for the comparison 'T_inter `op` T_other' (or reversed), where
        //
        //    T_inter = P1 & P2 & ... & Pn & ~N1 & ~N2 & ... & ~Nm
        //
        // is the intersection type. If f(T) is the function that computes the result
        // type of a `op`-comparison with `T_other`, we are interested in f(T_inter).
        // Since we can't compute it exactly, we return the following approximation:
        //
        //   f(T_inter) = f(P1) & f(P2) & ... & f(Pn)
        //
        // The reason for this is the following: In general, for any function 'f', the
        // set f(A) & f(B) is *larger than or equal to* the set f(A & B). This means
        // that we will return a type that is possibly wider than it could be, but
        // never wrong.
        //
        // However, we do have to leave out the negative contributions. If we were to
        // add a contribution like ~f(N1), we would potentially infer result types
        // that are too narrow.
        //
        // As an example for this, consider the intersection type `int & ~Literal[1]`.
        // If 'f' would be the `==`-comparison with 2, we obviously can't tell if that
        // answer would be true or false, so we need to return `bool`. And indeed, we
        // we have (glossing over notational details):
        //
        //   f(int & ~1)
        //       = f({..., -1, 0, 2, 3, ...})
        //       = {..., False, False, True, False, ...}
        //       = bool
        //
        // On the other hand, if we were to compute
        //
        //   f(int) & ~f(1)
        //       = bool & ~False
        //       = True
        //
        // we would get a result type `Literal[True]` which is too narrow.
        //
        let mut builder = IntersectionBuilder::new(self.db());

        builder = builder.add_positive(KnownClass::Bool.to_instance(self.db()));

        let mut state = State::NoPositiveElements;

        for pos in intersection.positive(self.db()) {
            let result = match intersection_on {
                IntersectionOn::Left => {
                    self.infer_binary_type_comparison(*pos, op, other, range, visitor)
                }
                IntersectionOn::Right => {
                    self.infer_binary_type_comparison(other, op, *pos, range, visitor)
                }
            };

            match result {
                Ok(ty) => {
                    state = State::Supported;
                    builder = builder.add_positive(ty);
                }
                Err(error) => {
                    match state {
                        State::NoPositiveElements => {
                            // This is the first positive element, but the operation is not supported.
                            // Store the error and continue.
                            state = State::UnsupportedOnAllElements(error);
                        }
                        State::UnsupportedOnAllElements(_) => {
                            // We already have an error stored, and continue to see elements on which
                            // the operator is not supported. Continue with the same state (only keep
                            // the first error).
                        }
                        State::Supported => {
                            // We previously saw a positive element that supported the operator,
                            // so the overall operation is still supported.
                        }
                    }
                }
            }
        }

        match state {
            State::Supported => Ok(builder.build()),
            State::NoPositiveElements => {
                // We didn't see any positive elements, check if the operation is supported on `object`:
                match intersection_on {
                    IntersectionOn::Left => {
                        self.infer_binary_type_comparison(Type::object(), op, other, range, visitor)
                    }
                    IntersectionOn::Right => {
                        self.infer_binary_type_comparison(other, op, Type::object(), range, visitor)
                    }
                }
            }
            State::UnsupportedOnAllElements(error) => Err(error),
        }
    }

    /// Infers the type of a binary comparison (e.g. 'left == right'). See
    /// `infer_compare_expression` for the higher level logic dealing with multi-comparison
    /// expressions.
    ///
    /// If the operation is not supported, return None (we need upstream context to emit a
    /// diagnostic).
    fn infer_binary_type_comparison(
        &mut self,
        left: Type<'db>,
        op: ast::CmpOp,
        right: Type<'db>,
        range: TextRange,
        visitor: &BinaryComparisonVisitor<'db>,
    ) -> Result<Type<'db>, UnsupportedComparisonError<'db>> {
        // Note: identity (is, is not) for equal builtin types is unreliable and not part of the
        // language spec.
        // - `[ast::CompOp::Is]`: return `false` if unequal, `bool` if equal
        // - `[ast::CompOp::IsNot]`: return `true` if unequal, `bool` if equal
        let db = self.db();
        let try_dunder = |inference: &mut Self, policy: MemberLookupPolicy| {
            let rich_comparison = |op| inference.infer_rich_comparison(left, right, op, policy);
            let membership_test_comparison = |op, range: TextRange| {
                inference.infer_membership_test_comparison(left, right, op, range)
            };

            match op {
                ast::CmpOp::Eq => rich_comparison(RichCompareOperator::Eq),
                ast::CmpOp::NotEq => rich_comparison(RichCompareOperator::Ne),
                ast::CmpOp::Lt => rich_comparison(RichCompareOperator::Lt),
                ast::CmpOp::LtE => rich_comparison(RichCompareOperator::Le),
                ast::CmpOp::Gt => rich_comparison(RichCompareOperator::Gt),
                ast::CmpOp::GtE => rich_comparison(RichCompareOperator::Ge),
                ast::CmpOp::In => {
                    membership_test_comparison(MembershipTestCompareOperator::In, range)
                }
                ast::CmpOp::NotIn => {
                    membership_test_comparison(MembershipTestCompareOperator::NotIn, range)
                }
                ast::CmpOp::Is => {
                    if left.is_disjoint_from(db, right) {
                        Ok(Type::BooleanLiteral(false))
                    } else if left.is_singleton(db) && left.is_equivalent_to(db, right) {
                        Ok(Type::BooleanLiteral(true))
                    } else {
                        Ok(KnownClass::Bool.to_instance(db))
                    }
                }
                ast::CmpOp::IsNot => {
                    if left.is_disjoint_from(db, right) {
                        Ok(Type::BooleanLiteral(true))
                    } else if left.is_singleton(db) && left.is_equivalent_to(db, right) {
                        Ok(Type::BooleanLiteral(false))
                    } else {
                        Ok(KnownClass::Bool.to_instance(db))
                    }
                }
            }
        };

        let comparison_result = match (left, right) {
            (Type::Union(union), other) => {
                let mut builder = UnionBuilder::new(self.db());
                for element in union.elements(self.db()) {
                    builder =
                        builder.add(self.infer_binary_type_comparison(*element, op, other, range, visitor)?);
                }
                Some(Ok(builder.build()))
            }
            (other, Type::Union(union)) => {
                let mut builder = UnionBuilder::new(self.db());
                for element in union.elements(self.db()) {
                    builder =
                        builder.add(self.infer_binary_type_comparison(other, op, *element, range, visitor)?);
                }
                Some(Ok(builder.build()))
            }

            (Type::Intersection(intersection), right) => {
                Some(self.infer_binary_intersection_type_comparison(
                    intersection,
                    op,
                    right,
                    IntersectionOn::Left,
                    range,
                    visitor,
                ).map_err(|err|UnsupportedComparisonError { op, left_ty: left, right_ty: err.right_ty }))
            }
            (left, Type::Intersection(intersection)) => {
                Some(self.infer_binary_intersection_type_comparison(
                    intersection,
                    op,
                    left,
                    IntersectionOn::Right,
                    range,
                    visitor,
                ).map_err(|err|UnsupportedComparisonError { op, left_ty: err.left_ty, right_ty: right }))
            }

            (Type::TypeAlias(alias), right) => Some(
                visitor.visit((left, op, right), || { self.infer_binary_type_comparison(
                    alias.value_type(self.db()),
                    op,
                    right,
                    range,
                    visitor,
                )
            })),

            (left, Type::TypeAlias(alias)) => Some(
                visitor.visit((left, op, right), || { self.infer_binary_type_comparison(
                    left,
                    op,
                    alias.value_type(self.db()),
                    range,
                    visitor,
                )
            })),

            (Type::IntLiteral(n), Type::IntLiteral(m)) => Some(match op {
                ast::CmpOp::Eq => Ok(Type::BooleanLiteral(n == m)),
                ast::CmpOp::NotEq => Ok(Type::BooleanLiteral(n != m)),
                ast::CmpOp::Lt => Ok(Type::BooleanLiteral(n < m)),
                ast::CmpOp::LtE => Ok(Type::BooleanLiteral(n <= m)),
                ast::CmpOp::Gt => Ok(Type::BooleanLiteral(n > m)),
                ast::CmpOp::GtE => Ok(Type::BooleanLiteral(n >= m)),
                // We cannot say that two equal int Literals will return True from an `is` or `is not` comparison.
                // Even if they are the same value, they may not be the same object.
                ast::CmpOp::Is => {
                    if n == m {
                        Ok(KnownClass::Bool.to_instance(self.db()))
                    } else {
                        Ok(Type::BooleanLiteral(false))
                    }
                }
                ast::CmpOp::IsNot => {
                    if n == m {
                        Ok(KnownClass::Bool.to_instance(self.db()))
                    } else {
                        Ok(Type::BooleanLiteral(true))
                    }
                }
                // Undefined for (int, int)
                ast::CmpOp::In | ast::CmpOp::NotIn => Err(UnsupportedComparisonError {
                    op,
                    left_ty: left,
                    right_ty: right,
                }),
            }),
            (Type::IntLiteral(_), Type::NominalInstance(_)) => {
                Some(self.infer_binary_type_comparison(
                    KnownClass::Int.to_instance(self.db()),
                    op,
                    right,
                    range,
                    visitor,
                ).map_err(|_| UnsupportedComparisonError {op, left_ty: left, right_ty: right}))
            }
            (Type::NominalInstance(_), Type::IntLiteral(_)) => {
                Some(self.infer_binary_type_comparison(
                    left,
                    op,
                    KnownClass::Int.to_instance(self.db()),
                    range,
                    visitor,
                ).map_err(|_|UnsupportedComparisonError { op, left_ty: left, right_ty: right }))
            }

            // Booleans are coded as integers (False = 0, True = 1)
            (Type::IntLiteral(n), Type::BooleanLiteral(b)) => {
                Some(self.infer_binary_type_comparison(
                    Type::IntLiteral(n),
                    op,
                    Type::IntLiteral(i64::from(b)),
                    range,
                    visitor,
                ).map_err(|_|UnsupportedComparisonError {op, left_ty: left, right_ty: right}))
            }
            (Type::BooleanLiteral(b), Type::IntLiteral(m)) => {
                Some(self.infer_binary_type_comparison(
                    Type::IntLiteral(i64::from(b)),
                    op,
                    Type::IntLiteral(m),
                    range,
                    visitor,
                ).map_err(|_|UnsupportedComparisonError {op, left_ty: left, right_ty: right}))
            }
            (Type::BooleanLiteral(a), Type::BooleanLiteral(b)) => {
                Some(self.infer_binary_type_comparison(
                    Type::IntLiteral(i64::from(a)),
                    op,
                    Type::IntLiteral(i64::from(b)),
                    range,
                    visitor,
                ).map_err(|_|UnsupportedComparisonError {op, left_ty: left, right_ty: right}))
            }

            (Type::StringLiteral(salsa_s1), Type::StringLiteral(salsa_s2)) => {
                let s1 = salsa_s1.value(self.db());
                let s2 = salsa_s2.value(self.db());
                let result = match op {
                    ast::CmpOp::Eq => Type::BooleanLiteral(s1 == s2),
                    ast::CmpOp::NotEq => Type::BooleanLiteral(s1 != s2),
                    ast::CmpOp::Lt => Type::BooleanLiteral(s1 < s2),
                    ast::CmpOp::LtE => Type::BooleanLiteral(s1 <= s2),
                    ast::CmpOp::Gt => Type::BooleanLiteral(s1 > s2),
                    ast::CmpOp::GtE => Type::BooleanLiteral(s1 >= s2),
                    ast::CmpOp::In => Type::BooleanLiteral(s2.contains(s1)),
                    ast::CmpOp::NotIn => Type::BooleanLiteral(!s2.contains(s1)),
                    ast::CmpOp::Is => {
                        if s1 == s2 {
                            KnownClass::Bool.to_instance(self.db())
                        } else {
                            Type::BooleanLiteral(false)
                        }
                    }
                    ast::CmpOp::IsNot => {
                        if s1 == s2 {
                            KnownClass::Bool.to_instance(self.db())
                        } else {
                            Type::BooleanLiteral(true)
                        }
                    }
                };
                Some(Ok(result))
            }
            (Type::StringLiteral(_), _) => Some(self.infer_binary_type_comparison(
                KnownClass::Str.to_instance(self.db()),
                op,
                right,
                range,
                visitor,
            ).map_err(|err|UnsupportedComparisonError {op, left_ty: left, right_ty: err.right_ty})),
            (_, Type::StringLiteral(_)) => Some(self.infer_binary_type_comparison(
                left,
                op,
                KnownClass::Str.to_instance(self.db()),
                range,
                visitor,
            ).map_err(|err|UnsupportedComparisonError {op, left_ty: err.left_ty, right_ty: right})),

            (Type::LiteralString, _) => Some(self.infer_binary_type_comparison(
                KnownClass::Str.to_instance(self.db()),
                op,
                right,
                range,
                visitor,
            ).map_err(|err|UnsupportedComparisonError {op, left_ty: left, right_ty: err.right_ty})),
            (_, Type::LiteralString) => Some(self.infer_binary_type_comparison(
                left,
                op,
                KnownClass::Str.to_instance(self.db()),
                range,
                visitor,
            ).map_err(|err|UnsupportedComparisonError {op, left_ty: err.left_ty, right_ty: right})),

            (Type::BytesLiteral(salsa_b1), Type::BytesLiteral(salsa_b2)) => {
                let b1 = salsa_b1.value(self.db());
                let b2 = salsa_b2.value(self.db());
                let result = match op {
                    ast::CmpOp::Eq => Type::BooleanLiteral(b1 == b2),
                    ast::CmpOp::NotEq => Type::BooleanLiteral(b1 != b2),
                    ast::CmpOp::Lt => Type::BooleanLiteral(b1 < b2),
                    ast::CmpOp::LtE => Type::BooleanLiteral(b1 <= b2),
                    ast::CmpOp::Gt => Type::BooleanLiteral(b1 > b2),
                    ast::CmpOp::GtE => Type::BooleanLiteral(b1 >= b2),
                    ast::CmpOp::In => {
                        Type::BooleanLiteral(memchr::memmem::find(b2, b1).is_some())
                    }
                    ast::CmpOp::NotIn => {
                        Type::BooleanLiteral(memchr::memmem::find(b2, b1).is_none())
                    }
                    ast::CmpOp::Is => {
                        if b1 == b2 {
                            KnownClass::Bool.to_instance(self.db())
                        } else {
                            Type::BooleanLiteral(false)
                        }
                    }
                    ast::CmpOp::IsNot => {
                        if b1 == b2 {
                            KnownClass::Bool.to_instance(self.db())
                        } else {
                            Type::BooleanLiteral(true)
                        }
                    }
                };
                Some(Ok(result))
            }
            (Type::BytesLiteral(_), _) => Some(self.infer_binary_type_comparison(
                KnownClass::Bytes.to_instance(self.db()),
                op,
                right,
                range,
                visitor,
            ).map_err(|err| UnsupportedComparisonError { op, left_ty: left, right_ty: err.right_ty })),
            (_, Type::BytesLiteral(_)) => Some(self.infer_binary_type_comparison(
                left,
                op,
                KnownClass::Bytes.to_instance(self.db()),
                range,
                visitor,
            ).map_err(|err| UnsupportedComparisonError { op, left_ty: err.left_ty, right_ty: right })),

            (Type::EnumLiteral(literal_1), Type::EnumLiteral(literal_2))
                if op == ast::CmpOp::Eq =>
            {
                Some(Ok(match try_dunder(self, MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK) {
                    Ok(ty) => ty,
                    Err(_) => Type::BooleanLiteral(literal_1 == literal_2),
                }))
            }
            (Type::EnumLiteral(literal_1), Type::EnumLiteral(literal_2))
                if op == ast::CmpOp::NotEq =>
            {
                Some(Ok(match try_dunder(self, MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK) {
                    Ok(ty) => ty,
                    Err(_) => Type::BooleanLiteral(literal_1 != literal_2),
                }))
            }

            (
                Type::KnownInstance(KnownInstanceType::ConstraintSet(left)),
                Type::KnownInstance(KnownInstanceType::ConstraintSet(right)),
            ) => {
                let result = match op {
                    ast::CmpOp::Eq => Some(
                        left.constraints(self.db()).iff(self.db(), right.constraints(self.db()))
                    ),
                    ast::CmpOp::NotEq => Some(
                        left.constraints(self.db()).iff(self.db(), right.constraints(self.db())).negate(self.db())
                    ),
                    _ => None,
                };
                result.map(|constraints| Ok(Type::KnownInstance(KnownInstanceType::ConstraintSet(
                    TrackedConstraintSet::new(self.db(), constraints)
                ))))
            }

            (
                Type::NominalInstance(nominal1),
                Type::NominalInstance(nominal2),
            ) => nominal1.tuple_spec(self.db())
                .and_then(|lhs_tuple| Some((lhs_tuple, nominal2.tuple_spec(self.db())?)))
                .map(|(lhs_tuple, rhs_tuple)| {
                    let mut tuple_rich_comparison =
                        |rich_op| visitor.visit((left, op, right), || {
                            self.infer_tuple_rich_comparison(&lhs_tuple, rich_op, &rhs_tuple, range, visitor)
                        });

                    match op {
                        ast::CmpOp::Eq => tuple_rich_comparison(RichCompareOperator::Eq),
                        ast::CmpOp::NotEq => tuple_rich_comparison(RichCompareOperator::Ne),
                        ast::CmpOp::Lt => tuple_rich_comparison(RichCompareOperator::Lt),
                        ast::CmpOp::LtE => tuple_rich_comparison(RichCompareOperator::Le),
                        ast::CmpOp::Gt => tuple_rich_comparison(RichCompareOperator::Gt),
                        ast::CmpOp::GtE => tuple_rich_comparison(RichCompareOperator::Ge),
                        ast::CmpOp::In | ast::CmpOp::NotIn => {
                            let mut any_eq = false;
                            let mut any_ambiguous = false;

                            for ty in rhs_tuple.all_elements().copied() {
                                let eq_result = self.infer_binary_type_comparison(
                                left,
                                ast::CmpOp::Eq,
                                ty,
                                range,
                                visitor
                            ).expect("infer_binary_type_comparison should never return None for `CmpOp::Eq`");

                                match eq_result {
                                    todo @ Type::Dynamic(DynamicType::Todo(_)) => return Ok(todo),
                                    // It's okay to ignore errors here because Python doesn't call `__bool__`
                                    // for different union variants. Instead, this is just for us to
                                    // evaluate a possibly truthy value to `false` or `true`.
                                    ty => match ty.bool(self.db()) {
                                        Truthiness::AlwaysTrue => any_eq = true,
                                        Truthiness::AlwaysFalse => (),
                                        Truthiness::Ambiguous => any_ambiguous = true,
                                    },
                                }
                            }

                            if any_eq {
                                Ok(Type::BooleanLiteral(op.is_in()))
                            } else if !any_ambiguous {
                                Ok(Type::BooleanLiteral(op.is_not_in()))
                            } else {
                                Ok(KnownClass::Bool.to_instance(self.db()))
                            }
                        }
                        ast::CmpOp::Is | ast::CmpOp::IsNot => {
                            // - `[ast::CmpOp::Is]`: returns `false` if the elements are definitely unequal, otherwise `bool`
                            // - `[ast::CmpOp::IsNot]`: returns `true` if the elements are definitely unequal, otherwise `bool`
                            let eq_result = tuple_rich_comparison(RichCompareOperator::Eq).expect(
                            "infer_binary_type_comparison should never return None for `CmpOp::Eq`",
                        );

                            Ok(match eq_result {
                                todo @ Type::Dynamic(DynamicType::Todo(_)) => todo,
                                // It's okay to ignore errors here because Python doesn't call `__bool__`
                                // for `is` and `is not` comparisons. This is an implementation detail
                                // for how we determine the truthiness of a type.
                                ty => match ty.bool(self.db()) {
                                    Truthiness::AlwaysFalse => Type::BooleanLiteral(op.is_is_not()),
                                    _ => KnownClass::Bool.to_instance(self.db()),
                                },
                            })
                        }
                    }
                }
            ),

            _ => None,
        };

        if let Some(result) = comparison_result {
            return result;
        }

        // Final generalized fallback: lookup the rich comparison `__dunder__` methods
        try_dunder(self, MemberLookupPolicy::default())
    }

    /// Rich comparison in Python are the operators `==`, `!=`, `<`, `<=`, `>`, and `>=`. Their
    /// behaviour can be edited for classes by implementing corresponding dunder methods.
    /// This function performs rich comparison between two types and returns the resulting type.
    /// see `<https://docs.python.org/3/reference/datamodel.html#object.__lt__>`
    fn infer_rich_comparison(
        &self,
        left: Type<'db>,
        right: Type<'db>,
        op: RichCompareOperator,
        policy: MemberLookupPolicy,
    ) -> Result<Type<'db>, UnsupportedComparisonError<'db>> {
        let db = self.db();
        // The following resource has details about the rich comparison algorithm:
        // https://snarky.ca/unravelling-rich-comparison-operators/
        let call_dunder = |op: RichCompareOperator, left: Type<'db>, right: Type<'db>| {
            left.try_call_dunder_with_policy(
                db,
                op.dunder(),
                &mut CallArguments::positional([right]),
                TypeContext::default(),
                policy,
            )
            .map(|outcome| outcome.return_type(db))
            .ok()
        };

        // The reflected dunder has priority if the right-hand side is a strict subclass of the left-hand side.
        if left != right && right.is_subtype_of(db, left) {
            call_dunder(op.reflect(), right, left).or_else(|| call_dunder(op, left, right))
        } else {
            call_dunder(op, left, right).or_else(|| call_dunder(op.reflect(), right, left))
        }
        .or_else(|| {
            // When no appropriate method returns any value other than NotImplemented,
            // the `==` and `!=` operators will fall back to `is` and `is not`, respectively.
            // refer to `<https://docs.python.org/3/reference/datamodel.html#object.__eq__>`
            if matches!(op, RichCompareOperator::Eq | RichCompareOperator::Ne)
                // This branch implements specific behavior of the `__eq__` and `__ne__` methods
                // on `object`, so it does not apply if we skip looking up attributes on `object`.
                && !policy.mro_no_object_fallback()
            {
                Some(KnownClass::Bool.to_instance(db))
            } else {
                None
            }
        })
        .ok_or_else(|| UnsupportedComparisonError {
            op: op.into(),
            left_ty: left,
            right_ty: right,
        })
    }

    /// Performs a membership test (`in` and `not in`) between two instances and returns the resulting type, or `None` if the test is unsupported.
    /// The behavior can be customized in Python by implementing `__contains__`, `__iter__`, or `__getitem__` methods.
    /// See `<https://docs.python.org/3/reference/datamodel.html#object.__contains__>`
    /// and `<https://docs.python.org/3/reference/expressions.html#membership-test-details>`
    fn infer_membership_test_comparison(
        &self,
        left: Type<'db>,
        right: Type<'db>,
        op: MembershipTestCompareOperator,
        range: TextRange,
    ) -> Result<Type<'db>, UnsupportedComparisonError<'db>> {
        let db = self.db();

        let contains_dunder = right.class_member(db, "__contains__".into()).place;
        let compare_result_opt = match contains_dunder {
            Place::Defined(contains_dunder, _, Definedness::AlwaysDefined) => {
                // If `__contains__` is available, it is used directly for the membership test.
                contains_dunder
                    .try_call(db, &CallArguments::positional([right, left]))
                    .map(|bindings| bindings.return_type(db))
                    .ok()
            }
            _ => {
                // iteration-based membership test
                right
                    .try_iterate(db)
                    .map(|_| KnownClass::Bool.to_instance(db))
                    .ok()
            }
        };

        compare_result_opt
            .map(|ty| {
                if matches!(ty, Type::Dynamic(DynamicType::Todo(_))) {
                    return ty;
                }

                let truthiness = ty.try_bool(db).unwrap_or_else(|err| {
                    err.report_diagnostic(&self.context, range);
                    err.fallback_truthiness()
                });

                match op {
                    MembershipTestCompareOperator::In => truthiness.into_type(db),
                    MembershipTestCompareOperator::NotIn => truthiness.negate().into_type(db),
                }
            })
            .ok_or_else(|| UnsupportedComparisonError {
                op: op.into(),
                left_ty: left,
                right_ty: right,
            })
    }

    /// Simulates rich comparison between tuples and returns the inferred result.
    /// This performs a lexicographic comparison, returning a union of all possible return types that could result from the comparison.
    ///
    /// basically it's based on cpython's `tuple_richcompare`
    /// see `<https://github.com/python/cpython/blob/9d6366b60d01305fc5e45100e0cd13e358aa397d/Objects/tupleobject.c#L637>`
    fn infer_tuple_rich_comparison(
        &mut self,
        left: &TupleSpec<'db>,
        op: RichCompareOperator,
        right: &TupleSpec<'db>,
        range: TextRange,
        visitor: &BinaryComparisonVisitor<'db>,
    ) -> Result<Type<'db>, UnsupportedComparisonError<'db>> {
        // If either tuple is variable length, we can make no assumptions about the relative
        // lengths of the tuples, and therefore neither about how they compare lexicographically.
        // TODO: Consider comparing the prefixes of the tuples, since that could give a comparison
        // result regardless of how long the variable-length tuple is.
        let (TupleSpec::Fixed(left), TupleSpec::Fixed(right)) = (left, right) else {
            return Ok(Type::unknown());
        };

        let left_iter = left.elements().copied();
        let right_iter = right.elements().copied();

        let mut builder = UnionBuilder::new(self.db());

        for (l_ty, r_ty) in left_iter.zip(right_iter) {
            let pairwise_eq_result = self
                .infer_binary_type_comparison(l_ty, ast::CmpOp::Eq, r_ty, range, visitor)
                .expect("infer_binary_type_comparison should never return None for `CmpOp::Eq`");

            match pairwise_eq_result
                .try_bool(self.db())
                .unwrap_or_else(|err| {
                    // TODO: We should, whenever possible, pass the range of the left and right elements
                    //   instead of the range of the whole tuple.
                    err.report_diagnostic(&self.context, range);
                    err.fallback_truthiness()
                }) {
                // - AlwaysTrue : Continue to the next pair for lexicographic comparison
                Truthiness::AlwaysTrue => continue,
                // - AlwaysFalse:
                // Lexicographic comparisons will always terminate with this pair.
                // Complete the comparison and return the result.
                // - Ambiguous:
                // Lexicographic comparisons might continue to the next pair (if eq_result is true),
                // or terminate here (if eq_result is false).
                // To account for cases where the comparison terminates here, add the pairwise comparison result to the union builder.
                eq_truthiness @ (Truthiness::AlwaysFalse | Truthiness::Ambiguous) => {
                    let pairwise_compare_result = match op {
                        RichCompareOperator::Lt
                        | RichCompareOperator::Le
                        | RichCompareOperator::Gt
                        | RichCompareOperator::Ge => self.infer_binary_type_comparison(
                            l_ty,
                            op.into(),
                            r_ty,
                            range,
                            visitor,
                        )?,
                        // For `==` and `!=`, we already figure out the result from `pairwise_eq_result`
                        // NOTE: The CPython implementation does not account for non-boolean return types
                        // or cases where `!=` is not the negation of `==`, we also do not consider these cases.
                        RichCompareOperator::Eq => Type::BooleanLiteral(false),
                        RichCompareOperator::Ne => Type::BooleanLiteral(true),
                    };

                    builder = builder.add(pairwise_compare_result);

                    if eq_truthiness.is_ambiguous() {
                        continue;
                    }

                    return Ok(builder.build());
                }
            }
        }

        // if no more items to compare, we just compare sizes
        let (left_len, right_len) = (left.len(), right.len());

        builder = builder.add(Type::BooleanLiteral(match op {
            RichCompareOperator::Eq => left_len == right_len,
            RichCompareOperator::Ne => left_len != right_len,
            RichCompareOperator::Lt => left_len < right_len,
            RichCompareOperator::Le => left_len <= right_len,
            RichCompareOperator::Gt => left_len > right_len,
            RichCompareOperator::Ge => left_len >= right_len,
        }));

        Ok(builder.build())
    }

    fn infer_subscript_expression(&mut self, subscript: &ast::ExprSubscript) -> Type<'db> {
        let ast::ExprSubscript {
            value,
            slice,
            range: _,
            node_index: _,
            ctx,
        } = subscript;

        match ctx {
            ExprContext::Load => self.infer_subscript_load(subscript),
            ExprContext::Store => {
                let value_ty = self.infer_expression(value, TypeContext::default());
                let slice_ty = self.infer_expression(slice, TypeContext::default());
                self.infer_subscript_expression_types(subscript, value_ty, slice_ty, *ctx);
                Type::Never
            }
            ExprContext::Del => {
                self.infer_subscript_load(subscript);
                Type::Never
            }
            ExprContext::Invalid => {
                let value_ty = self.infer_expression(value, TypeContext::default());
                let slice_ty = self.infer_expression(slice, TypeContext::default());
                self.infer_subscript_expression_types(subscript, value_ty, slice_ty, *ctx);
                Type::unknown()
            }
        }
    }

    fn infer_subscript_load(&mut self, subscript: &ast::ExprSubscript) -> Type<'db> {
        let value_ty = self.infer_expression(&subscript.value, TypeContext::default());

        // If we have an implicit type alias like `MyList = list[T]`, and if `MyList` is being
        // used in another implicit type alias like `Numbers = MyList[int]`, then we infer the
        // right hand side as a value expression, and need to handle the specialization here.
        if value_ty.is_generic_alias() {
            return self.infer_explicit_type_alias_specialization(subscript, value_ty, false);
        }

        self.infer_subscript_load_impl(value_ty, subscript)
    }

    fn infer_subscript_load_impl(
        &mut self,
        value_ty: Type<'db>,
        subscript: &ast::ExprSubscript,
    ) -> Type<'db> {
        let ast::ExprSubscript {
            range: _,
            node_index: _,
            value: _,
            slice,
            ctx,
        } = subscript;

        let mut constraint_keys = vec![];

        // If `value` is a valid reference, we attempt type narrowing by assignment.
        if !value_ty.is_unknown() {
            if let Some(expr) = PlaceExpr::try_from_expr(subscript) {
                let (place, keys) = self.infer_place_load(
                    PlaceExprRef::from(&expr),
                    ast::ExprRef::Subscript(subscript),
                );
                constraint_keys.extend(keys);
                if let Place::Defined(ty, _, Definedness::AlwaysDefined) = place.place {
                    // Even if we can obtain the subscript type based on the assignments, we still perform default type inference
                    // (to store the expression type and to report errors).
                    let slice_ty = self.infer_expression(slice, TypeContext::default());
                    self.infer_subscript_expression_types(subscript, value_ty, slice_ty, *ctx);
                    return ty;
                }
            }
        }

        let tuple_generic_alias = |db: &'db dyn Db, tuple: Option<TupleType<'db>>| {
            let tuple = tuple.unwrap_or_else(|| TupleType::homogeneous(db, Type::unknown()));
            Type::from(tuple.to_class_type(db))
        };

        match value_ty {
            Type::ClassLiteral(class) => {
                // HACK ALERT: If we are subscripting a generic class, short-circuit the rest of the
                // subscript inference logic and treat this as an explicit specialization.
                // TODO: Move this logic into a custom callable, and update `find_name_in_mro` to return
                // this callable as the `__class_getitem__` method on `type`. That probably requires
                // updating all of the subscript logic below to use custom callables for all of the _other_
                // special cases, too.
                if class.is_tuple(self.db()) {
                    return tuple_generic_alias(self.db(), self.infer_tuple_type_expression(slice));
                } else if class.is_known(self.db(), KnownClass::Type) {
                    let argument_ty = self.infer_type_expression(slice);
                    return Type::KnownInstance(KnownInstanceType::TypeGenericAlias(
                        InternedType::new(self.db(), argument_ty),
                    ));
                }

                if let Some(generic_context) = class.generic_context(self.db()) {
                    return self.infer_explicit_class_specialization(
                        subscript,
                        value_ty,
                        class,
                        generic_context,
                    );
                }
            }
            Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::ManualPEP695(
                _,
            ))) => {
                let slice_ty = self.infer_expression(slice, TypeContext::default());
                let mut variables = FxOrderSet::default();
                slice_ty.bind_and_find_all_legacy_typevars(
                    self.db(),
                    self.typevar_binding_context,
                    &mut variables,
                );
                let generic_context = GenericContext::from_typevar_instances(self.db(), variables);
                return Type::Dynamic(DynamicType::UnknownGeneric(generic_context));
            }
            Type::KnownInstance(KnownInstanceType::TypeAliasType(type_alias)) => {
                if let Some(generic_context) = type_alias.generic_context(self.db()) {
                    return self.infer_explicit_type_alias_type_specialization(
                        subscript,
                        value_ty,
                        type_alias,
                        generic_context,
                    );
                }
            }
            Type::SpecialForm(SpecialFormType::Tuple) => {
                return tuple_generic_alias(self.db(), self.infer_tuple_type_expression(slice));
            }
            Type::SpecialForm(SpecialFormType::Literal) => {
                match self.infer_literal_parameter_type(slice) {
                    Ok(result) => {
                        return Type::KnownInstance(KnownInstanceType::Literal(InternedType::new(
                            self.db(),
                            result,
                        )));
                    }
                    Err(nodes) => {
                        for node in nodes {
                            let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, node)
                            else {
                                continue;
                            };
                            builder.into_diagnostic(
                                "Type arguments for `Literal` must be `None`, \
                            a literal value (int, bool, str, or bytes), or an enum member",
                            );
                        }
                        return Type::unknown();
                    }
                }
            }
            Type::SpecialForm(SpecialFormType::Annotated) => {
                let ast::Expr::Tuple(ast::ExprTuple {
                    elts: ref arguments,
                    ..
                }) = **slice
                else {
                    report_invalid_arguments_to_annotated(&self.context, subscript);

                    return self.infer_expression(slice, TypeContext::default());
                };

                if arguments.len() < 2 {
                    report_invalid_arguments_to_annotated(&self.context, subscript);
                }

                let [type_expr, metadata @ ..] = &arguments[..] else {
                    for argument in arguments {
                        self.infer_expression(argument, TypeContext::default());
                    }
                    self.store_expression_type(slice, Type::unknown());
                    return Type::unknown();
                };

                for element in metadata {
                    self.infer_expression(element, TypeContext::default());
                }

                let ty = self.infer_type_expression(type_expr);

                return Type::KnownInstance(KnownInstanceType::Annotated(InternedType::new(
                    self.db(),
                    ty,
                )));
            }
            Type::SpecialForm(SpecialFormType::Optional) => {
                if matches!(**slice, ast::Expr::Tuple(_)) {
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`typing.Optional` requires exactly one argument"
                        ));
                    }
                }

                let ty = self.infer_expression(slice, TypeContext::default());

                // `Optional[None]` is equivalent to `None`:
                if ty.is_none(self.db()) {
                    return ty;
                }

                return UnionTypeInstance::from_value_expression_types(
                    self.db(),
                    [ty, Type::none(self.db())],
                    self.scope(),
                    self.typevar_binding_context,
                );
            }
            Type::SpecialForm(SpecialFormType::Union) => {
                let db = self.db();

                match **slice {
                    ast::Expr::Tuple(ref tuple) => {
                        let mut elements = tuple
                            .elts
                            .iter()
                            .map(|elt| self.infer_type_expression(elt))
                            .peekable();

                        let is_empty = elements.peek().is_none();
                        let union_type = Type::KnownInstance(KnownInstanceType::UnionType(
                            UnionTypeInstance::new(
                                db,
                                None,
                                Ok(UnionType::from_elements(db, elements)),
                            ),
                        ));

                        if is_empty {
                            if let Some(builder) =
                                self.context.report_lint(&INVALID_TYPE_FORM, subscript)
                            {
                                builder.into_diagnostic(
                                    "`typing.Union` requires at least one type argument",
                                );
                            }
                        }

                        return union_type;
                    }
                    _ => {
                        return self.infer_expression(slice, TypeContext::default());
                    }
                }
            }
            Type::SpecialForm(SpecialFormType::Type) => {
                // Similar to the branch above that handles `type[…]`, handle `typing.Type[…]`
                let argument_ty = self.infer_type_expression(slice);
                return Type::KnownInstance(KnownInstanceType::TypeGenericAlias(
                    InternedType::new(self.db(), argument_ty),
                ));
            }
            Type::SpecialForm(SpecialFormType::Callable) => {
                let arguments = if let ast::Expr::Tuple(tuple) = &*subscript.slice {
                    &*tuple.elts
                } else {
                    std::slice::from_ref(&*subscript.slice)
                };

                // TODO: Remove this once we support Concatenate properly. This is necessary
                // to avoid a lot of false positives downstream, because we can't represent the typevar-
                // specialized `Callable` types yet.
                let num_arguments = arguments.len();
                if num_arguments == 2 {
                    let first_arg = &arguments[0];
                    let second_arg = &arguments[1];

                    if first_arg.is_subscript_expr() {
                        let first_arg_ty = self.infer_expression(first_arg, TypeContext::default());
                        if let Type::Dynamic(DynamicType::UnknownGeneric(generic_context)) =
                            first_arg_ty
                        {
                            let mut variables = generic_context
                                .variables(self.db())
                                .collect::<FxOrderSet<_>>();

                            let return_ty =
                                self.infer_expression(second_arg, TypeContext::default());
                            return_ty.bind_and_find_all_legacy_typevars(
                                self.db(),
                                self.typevar_binding_context,
                                &mut variables,
                            );

                            let generic_context =
                                GenericContext::from_typevar_instances(self.db(), variables);
                            return Type::Dynamic(DynamicType::UnknownGeneric(generic_context));
                        }

                        if let Some(builder) =
                            self.context.report_lint(&INVALID_TYPE_FORM, subscript)
                        {
                            builder.into_diagnostic(format_args!(
                                "The first argument to `Callable` must be either a list of types, \
                                     ParamSpec, Concatenate, or `...`",
                            ));
                        }
                        return Type::KnownInstance(KnownInstanceType::Callable(
                            CallableType::unknown(self.db()),
                        ));
                    }
                }

                let callable = self
                    .infer_callable_type(subscript)
                    .as_callable()
                    .expect("always returns Type::Callable");

                return Type::KnownInstance(KnownInstanceType::Callable(callable));
            }
            // `typing` special forms with a single generic argument
            Type::SpecialForm(
                special_form @ (SpecialFormType::List
                | SpecialFormType::Set
                | SpecialFormType::FrozenSet
                | SpecialFormType::Counter
                | SpecialFormType::Deque),
            ) => {
                let slice_ty = self.infer_type_expression(slice);

                let element_ty = if matches!(**slice, ast::Expr::Tuple(_)) {
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`typing.{}` requires exactly one argument",
                            special_form.name()
                        ));
                    }
                    Type::unknown()
                } else {
                    slice_ty
                };

                let class = special_form
                    .aliased_stdlib_class()
                    .expect("A known stdlib class is available");

                return class
                    .to_specialized_class_type(self.db(), [element_ty])
                    .map(Type::from)
                    .unwrap_or_else(Type::unknown);
            }
            // `typing` special forms with two generic arguments
            Type::SpecialForm(
                special_form @ (SpecialFormType::Dict
                | SpecialFormType::ChainMap
                | SpecialFormType::DefaultDict
                | SpecialFormType::OrderedDict),
            ) => {
                let (first_ty, second_ty) = if let ast::Expr::Tuple(ast::ExprTuple {
                    elts: ref arguments,
                    ..
                }) = **slice
                {
                    if arguments.len() != 2 {
                        if let Some(builder) =
                            self.context.report_lint(&INVALID_TYPE_FORM, subscript)
                        {
                            builder.into_diagnostic(format_args!(
                                "`typing.{}` requires exactly two arguments, got {}",
                                special_form.name(),
                                arguments.len()
                            ));
                        }
                    }

                    if let [first_expr, second_expr] = &arguments[..] {
                        let first_ty = self.infer_type_expression(first_expr);
                        let second_ty = self.infer_type_expression(second_expr);

                        (first_ty, second_ty)
                    } else {
                        for argument in arguments {
                            self.infer_type_expression(argument);
                        }

                        (Type::unknown(), Type::unknown())
                    }
                } else {
                    let first_ty = self.infer_type_expression(slice);

                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`typing.{}` requires exactly two arguments, got 1",
                            special_form.name()
                        ));
                    }

                    (first_ty, Type::unknown())
                };

                let class = special_form
                    .aliased_stdlib_class()
                    .expect("Stdlib class available");

                return class
                    .to_specialized_class_type(self.db(), [first_ty, second_ty])
                    .map(Type::from)
                    .unwrap_or_else(Type::unknown);
            }
            Type::KnownInstance(
                KnownInstanceType::UnionType(_)
                | KnownInstanceType::Annotated(_)
                | KnownInstanceType::Callable(_)
                | KnownInstanceType::TypeGenericAlias(_),
            ) => {
                return self.infer_explicit_type_alias_specialization(subscript, value_ty, false);
            }
            Type::Dynamic(DynamicType::Unknown) => {
                let slice_ty = self.infer_expression(slice, TypeContext::default());
                let mut variables = FxOrderSet::default();
                slice_ty.bind_and_find_all_legacy_typevars(
                    self.db(),
                    self.typevar_binding_context,
                    &mut variables,
                );
                let generic_context = GenericContext::from_typevar_instances(self.db(), variables);
                return Type::Dynamic(DynamicType::UnknownGeneric(generic_context));
            }
            _ => {}
        }

        let slice_ty = self.infer_expression(slice, TypeContext::default());
        let result_ty = self.infer_subscript_expression_types(subscript, value_ty, slice_ty, *ctx);
        self.narrow_expr_with_applicable_constraints(subscript, result_ty, &constraint_keys)
    }

    fn infer_explicit_class_specialization(
        &mut self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        generic_class: ClassLiteral<'db>,
        generic_context: GenericContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let specialize = |types: &[Option<Type<'db>>]| {
            Type::from(generic_class.apply_specialization(db, |_| {
                generic_context.specialize_partial(db, types.iter().copied())
            }))
        };

        self.infer_explicit_callable_specialization(
            subscript,
            value_ty,
            generic_context,
            specialize,
        )
    }

    fn infer_explicit_type_alias_type_specialization(
        &mut self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        generic_type_alias: TypeAliasType<'db>,
        generic_context: GenericContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let specialize = |types: &[Option<Type<'db>>]| {
            let type_alias = generic_type_alias.apply_specialization(db, |_| {
                generic_context.specialize_partial(db, types.iter().copied())
            });

            Type::KnownInstance(KnownInstanceType::TypeAliasType(type_alias))
        };

        self.infer_explicit_callable_specialization(
            subscript,
            value_ty,
            generic_context,
            specialize,
        )
    }

    fn infer_explicit_callable_specialization(
        &mut self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        generic_context: GenericContext<'db>,
        specialize: impl FnOnce(&[Option<Type<'db>>]) -> Type<'db>,
    ) -> Type<'db> {
        fn add_typevar_definition<'db>(
            db: &'db dyn Db,
            diagnostic: &mut Diagnostic,
            typevar: BoundTypeVarInstance<'db>,
        ) {
            let Some(definition) = typevar.typevar(db).definition(db) else {
                return;
            };
            let file = definition.file(db);
            let module = parsed_module(db, file).load(db);
            let range = definition.focus_range(db, &module).range();
            diagnostic.annotate(
                Annotation::secondary(Span::from(file).with_range(range))
                    .message("Type variable defined here"),
            );
        }

        let db = self.db();
        let slice_node = subscript.slice.as_ref();

        let exactly_one_paramspec = generic_context.exactly_one_paramspec(db);
        let (type_arguments, store_inferred_type_arguments) = match slice_node {
            ast::Expr::Tuple(tuple) => {
                if exactly_one_paramspec {
                    (std::slice::from_ref(slice_node), false)
                } else {
                    (tuple.elts.as_slice(), true)
                }
            }
            _ => (std::slice::from_ref(slice_node), false),
        };
        let mut inferred_type_arguments = Vec::with_capacity(type_arguments.len());

        let typevars = generic_context.variables(db);
        let typevars_len = typevars.len();

        let mut specialization_types = Vec::with_capacity(typevars_len);
        let mut typevar_with_defaults = 0;
        let mut missing_typevars = vec![];
        let mut first_excess_type_argument_index = None;

        // Helper to get the AST node corresponding to the type argument at `index`.
        let get_node = |index: usize| -> ast::AnyNodeRef<'_> {
            match slice_node {
                ast::Expr::Tuple(ast::ExprTuple { elts, .. }) if !exactly_one_paramspec => elts
                    .get(index)
                    .expect("type argument index should not be out of range")
                    .into(),
                _ => slice_node.into(),
            }
        };

        let mut has_error = false;

        for (index, item) in typevars.zip_longest(type_arguments.iter()).enumerate() {
            match item {
                EitherOrBoth::Both(typevar, expr) => {
                    if typevar.default_type(db).is_some() {
                        typevar_with_defaults += 1;
                    }

                    let provided_type = if typevar.is_paramspec(db) {
                        match self.infer_paramspec_explicit_specialization_value(
                            expr,
                            exactly_one_paramspec,
                        ) {
                            Ok(paramspec_value) => paramspec_value,
                            Err(()) => {
                                has_error = true;
                                Type::unknown()
                            }
                        }
                    } else {
                        self.infer_type_expression(expr)
                    };

                    inferred_type_arguments.push(provided_type);

                    // TODO consider just accepting the given specialization without checking
                    // against bounds/constraints, but recording the expression for deferred
                    // checking at end of scope. This would avoid a lot of cycles caused by eagerly
                    // doing assignment checks here.
                    match typevar.typevar(db).bound_or_constraints(db) {
                        Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                            if provided_type
                                .when_assignable_to(db, bound, InferableTypeVars::None)
                                .is_never_satisfied(db)
                            {
                                let node = get_node(index);
                                if let Some(builder) =
                                    self.context.report_lint(&INVALID_TYPE_ARGUMENTS, node)
                                {
                                    let mut diagnostic = builder.into_diagnostic(format_args!(
                                        "Type `{}` is not assignable to upper bound `{}` \
                                            of type variable `{}`",
                                        provided_type.display(db),
                                        bound.display(db),
                                        typevar.identity(db).display(db),
                                    ));
                                    add_typevar_definition(db, &mut diagnostic, typevar);
                                }
                                has_error = true;
                                continue;
                            }
                        }
                        Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                            // TODO: this is wrong, the given specialization needs to be assignable
                            // to _at least one_ of the individual constraints, not to the union of
                            // all of them. `int | str` is not a valid specialization of a typevar
                            // constrained to `(int, str)`.
                            if provided_type
                                .when_assignable_to(
                                    db,
                                    constraints.as_type(db),
                                    InferableTypeVars::None,
                                )
                                .is_never_satisfied(db)
                            {
                                let node = get_node(index);
                                if let Some(builder) =
                                    self.context.report_lint(&INVALID_TYPE_ARGUMENTS, node)
                                {
                                    let mut diagnostic = builder.into_diagnostic(format_args!(
                                        "Type `{}` does not satisfy constraints `{}` \
                                            of type variable `{}`",
                                        provided_type.display(db),
                                        constraints
                                            .elements(db)
                                            .iter()
                                            .map(|c| c.display(db))
                                            .format("`, `"),
                                        typevar.identity(db).display(db),
                                    ));
                                    add_typevar_definition(db, &mut diagnostic, typevar);
                                }
                                has_error = true;
                                continue;
                            }
                        }
                        None => {}
                    }

                    specialization_types.push(Some(provided_type));
                }
                EitherOrBoth::Left(typevar) => {
                    if typevar.default_type(db).is_none() {
                        // This is an error case, so no need to push into the specialization types.
                        missing_typevars.push(typevar);
                    } else {
                        typevar_with_defaults += 1;
                        specialization_types.push(None);
                    }
                }
                EitherOrBoth::Right(expr) => {
                    inferred_type_arguments.push(self.infer_type_expression(expr));
                    first_excess_type_argument_index.get_or_insert(index);
                }
            }
        }

        if !missing_typevars.is_empty() {
            if let Some(builder) = self.context.report_lint(&INVALID_TYPE_ARGUMENTS, subscript) {
                let description = CallableDescription::new(db, value_ty);
                let s = if missing_typevars.len() > 1 { "s" } else { "" };
                builder.into_diagnostic(format_args!(
                    "No type argument{s} provided for required type variable{s} `{}`{}",
                    missing_typevars
                        .iter()
                        .map(|tv| tv.typevar(db).name(db))
                        .format("`, `"),
                    if let Some(CallableDescription { kind, name }) = description {
                        format!(" of {kind} `{name}`")
                    } else {
                        String::new()
                    }
                ));
            }
            has_error = true;
        }

        if let Some(first_excess_type_argument_index) = first_excess_type_argument_index {
            let node = get_node(first_excess_type_argument_index);
            if let Some(builder) = self.context.report_lint(&INVALID_TYPE_ARGUMENTS, node) {
                let description = CallableDescription::new(db, value_ty);
                builder.into_diagnostic(format_args!(
                    "Too many type arguments{}: expected {}, got {}",
                    if let Some(CallableDescription { kind, name }) = description {
                        format!(" to {kind} `{name}`")
                    } else {
                        String::new()
                    },
                    if typevar_with_defaults == 0 {
                        format!("{typevars_len}")
                    } else {
                        format!(
                            "between {} and {}",
                            typevars_len - typevar_with_defaults,
                            typevars_len
                        )
                    },
                    type_arguments.len(),
                ));
            }
            has_error = true;
        }

        if store_inferred_type_arguments {
            self.store_expression_type(
                slice_node,
                Type::heterogeneous_tuple(db, inferred_type_arguments),
            );
        }

        if has_error {
            let unknowns = generic_context
                .variables(self.db())
                .map(|typevar| {
                    Some(if typevar.is_paramspec(db) {
                        Type::paramspec_value_callable(db, Parameters::unknown())
                    } else {
                        Type::unknown()
                    })
                })
                .collect::<Vec<_>>();
            return specialize(&unknowns);
        }

        specialize(&specialization_types)
    }

    fn infer_subscript_expression_types(
        &self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        slice_ty: Type<'db>,
        expr_context: ExprContext,
    ) -> Type<'db> {
        let db = self.db();
        let context = &self.context;

        let value_node = subscript.value.as_ref();

        let inferred = match (value_ty, slice_ty) {
            (Type::Union(union), _) => Some(union.map(db, |element| {
                self.infer_subscript_expression_types(subscript, *element, slice_ty, expr_context)
            })),

            // TODO: we can map over the intersection and fold the results back into an intersection,
            // but we need to make sure we avoid emitting a diagnostic if one positive element has a `__getitem__`
            // method but another does not. This means `infer_subscript_expression_types`
            // needs to return a `Result` rather than eagerly emitting diagnostics.
            (Type::Intersection(_), _) => {
                Some(todo_type!("Subscript expressions on intersections"))
            }

            // Ex) Given `("a", "b", "c", "d")[1]`, return `"b"`
            (Type::NominalInstance(nominal), Type::IntLiteral(i64_int)) => nominal
                .tuple_spec(db)
                .and_then(|tuple| Some((tuple, i32::try_from(i64_int).ok()?)))
                .map(|(tuple, i32_int)| {
                    tuple.py_index(db, i32_int).unwrap_or_else(|_| {
                        report_index_out_of_bounds(
                            context,
                            "tuple",
                            value_node.into(),
                            value_ty,
                            tuple.len().display_minimum(),
                            i64_int,
                        );
                        Type::unknown()
                    })
                }),

            // Ex) Given `("a", 1, Null)[0:2]`, return `("a", 1)`
            (
                Type::NominalInstance(maybe_tuple_nominal),
                Type::NominalInstance(maybe_slice_nominal),
            ) => maybe_tuple_nominal
                .tuple_spec(db)
                .as_deref()
                .and_then(|tuple_spec| Some((tuple_spec, maybe_slice_nominal.slice_literal(db)?)))
                .map(|(tuple, SliceLiteral { start, stop, step })| match tuple {
                    TupleSpec::Fixed(tuple) => {
                        if let Ok(new_elements) = tuple.py_slice(db, start, stop, step) {
                            Type::heterogeneous_tuple(db, new_elements)
                        } else {
                            report_slice_step_size_zero(context, value_node.into());
                            Type::unknown()
                        }
                    }
                    TupleSpec::Variable(_) => {
                        todo_type!("slice into variable-length tuple")
                    }
                }),

            // Ex) Given `"value"[1]`, return `"a"`
            (Type::StringLiteral(literal_ty), Type::IntLiteral(i64_int)) => {
                i32::try_from(i64_int).ok().map(|i32_int| {
                    let literal_value = literal_ty.value(db);
                    (&mut literal_value.chars())
                        .py_index(db, i32_int)
                        .map(|ch| Type::string_literal(db, &ch.to_string()))
                        .unwrap_or_else(|_| {
                            report_index_out_of_bounds(
                                context,
                                "string",
                                value_node.into(),
                                value_ty,
                                literal_value.chars().count(),
                                i64_int,
                            );
                            Type::unknown()
                        })
                })
            }

            // Ex) Given `"value"[1:3]`, return `"al"`
            (Type::StringLiteral(literal_ty), Type::NominalInstance(nominal)) => nominal
                .slice_literal(db)
                .map(|SliceLiteral { start, stop, step }| {
                    let literal_value = literal_ty.value(db);
                    let chars: Vec<_> = literal_value.chars().collect();

                    if let Ok(new_chars) = chars.py_slice(db, start, stop, step) {
                        let literal: String = new_chars.collect();
                        Type::string_literal(db, &literal)
                    } else {
                        report_slice_step_size_zero(context, value_node.into());
                        Type::unknown()
                    }
                }),

            // Ex) Given `b"value"[1]`, return `97` (i.e., `ord(b"a")`)
            (Type::BytesLiteral(literal_ty), Type::IntLiteral(i64_int)) => {
                i32::try_from(i64_int).ok().map(|i32_int| {
                    let literal_value = literal_ty.value(db);
                    literal_value
                        .py_index(db, i32_int)
                        .map(|byte| Type::IntLiteral((*byte).into()))
                        .unwrap_or_else(|_| {
                            report_index_out_of_bounds(
                                context,
                                "bytes literal",
                                value_node.into(),
                                value_ty,
                                literal_value.len(),
                                i64_int,
                            );
                            Type::unknown()
                        })
                })
            }

            // Ex) Given `b"value"[1:3]`, return `b"al"`
            (Type::BytesLiteral(literal_ty), Type::NominalInstance(nominal)) => nominal
                .slice_literal(db)
                .map(|SliceLiteral { start, stop, step }| {
                    let literal_value = literal_ty.value(db);

                    if let Ok(new_bytes) = literal_value.py_slice(db, start, stop, step) {
                        let new_bytes: Vec<u8> = new_bytes.collect();
                        Type::bytes_literal(db, &new_bytes)
                    } else {
                        report_slice_step_size_zero(context, value_node.into());
                        Type::unknown()
                    }
                }),

            // Ex) Given `"value"[True]`, return `"a"`
            (Type::StringLiteral(_) | Type::BytesLiteral(_), Type::BooleanLiteral(bool)) => {
                Some(self.infer_subscript_expression_types(
                    subscript,
                    value_ty,
                    Type::IntLiteral(i64::from(bool)),
                    expr_context,
                ))
            }

            (Type::NominalInstance(nominal), Type::BooleanLiteral(bool))
                if nominal.tuple_spec(db).is_some() =>
            {
                Some(self.infer_subscript_expression_types(
                    subscript,
                    value_ty,
                    Type::IntLiteral(i64::from(bool)),
                    expr_context,
                ))
            }

            (Type::SpecialForm(SpecialFormType::Protocol), typevars) => Some(
                self.legacy_generic_class_context(
                    value_node,
                    typevars,
                    LegacyGenericBase::Protocol,
                )
                .map(|context| Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(context)))
                .unwrap_or_else(GenericContextError::into_type),
            ),

            (Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(_)), _) => {
                // TODO: emit a diagnostic
                Some(todo_type!("doubly-specialized typing.Protocol"))
            }

            (
                Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(alias))),
                _,
            ) if alias.generic_context(db).is_none() => {
                if let Some(builder) = self.context.report_lint(&NON_SUBSCRIPTABLE, subscript) {
                    builder
                        .into_diagnostic(format_args!("Cannot subscript non-generic type alias"));
                }

                Some(Type::unknown())
            }

            (Type::SpecialForm(SpecialFormType::Generic), typevars) => Some(
                self.legacy_generic_class_context(value_node, typevars, LegacyGenericBase::Generic)
                    .map(|context| {
                        Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(context))
                    })
                    .unwrap_or_else(GenericContextError::into_type),
            ),

            (Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(_)), _) => {
                // TODO: emit a diagnostic
                Some(todo_type!("doubly-specialized typing.Generic"))
            }

            (Type::SpecialForm(SpecialFormType::Unpack), _) => {
                Some(Type::Dynamic(DynamicType::TodoUnpack))
            }

            (Type::SpecialForm(SpecialFormType::Concatenate), _) => {
                // TODO: Add proper support for `Concatenate`
                let mut variables = FxOrderSet::default();
                slice_ty.bind_and_find_all_legacy_typevars(
                    db,
                    self.typevar_binding_context,
                    &mut variables,
                );
                let generic_context = GenericContext::from_typevar_instances(self.db(), variables);
                Some(Type::Dynamic(DynamicType::UnknownGeneric(generic_context)))
            }

            (Type::SpecialForm(special_form), _) if special_form.class().is_special_form() => {
                Some(todo_type!("Inference of subscript on special form"))
            }

            (Type::KnownInstance(known_instance), _)
                if known_instance.class(db).is_special_form() =>
            {
                Some(todo_type!("Inference of subscript on special form"))
            }

            _ => None,
        };

        if let Some(inferred) = inferred {
            return inferred;
        }

        // If the class defines `__getitem__`, return its return type.
        //
        // See: https://docs.python.org/3/reference/datamodel.html#class-getitem-versus-getitem
        match value_ty.try_call_dunder(
            db,
            "__getitem__",
            CallArguments::positional([slice_ty]),
            TypeContext::default(),
        ) {
            Ok(outcome) => {
                return outcome.return_type(db);
            }
            Err(err @ CallDunderError::PossiblyUnbound { .. }) => {
                if let Some(builder) =
                    context.report_lint(&POSSIBLY_MISSING_IMPLICIT_CALL, value_node)
                {
                    builder.into_diagnostic(format_args!(
                        "Method `__getitem__` of type `{}` may be missing",
                        value_ty.display(db),
                    ));
                }

                return err.fallback_return_type(db);
            }
            Err(CallDunderError::CallError(call_error_kind, bindings)) => {
                match call_error_kind {
                    CallErrorKind::NotCallable => {
                        if let Some(builder) = context.report_lint(&CALL_NON_CALLABLE, value_node) {
                            builder.into_diagnostic(format_args!(
                                "Method `__getitem__` of type `{}` \
                                is not callable on object of type `{}`",
                                bindings.callable_type().display(db),
                                value_ty.display(db),
                            ));
                        }
                    }
                    CallErrorKind::BindingError => {
                        if let Some(typed_dict) = value_ty.as_typed_dict() {
                            let slice_node = subscript.slice.as_ref();

                            report_invalid_key_on_typed_dict(
                                context,
                                value_node.into(),
                                slice_node.into(),
                                value_ty,
                                None,
                                slice_ty,
                                typed_dict.items(db),
                            );
                        } else {
                            if let Some(builder) =
                                context.report_lint(&INVALID_ARGUMENT_TYPE, value_node)
                            {
                                builder.into_diagnostic(format_args!(
                                    "Method `__getitem__` of type `{}` cannot be called with key of \
                                    type `{}` on object of type `{}`",
                                    bindings.callable_type().display(db),
                                    slice_ty.display(db),
                                    value_ty.display(db),
                                ));
                            }
                        }
                    }
                    CallErrorKind::PossiblyNotCallable => {
                        if let Some(builder) = context.report_lint(&CALL_NON_CALLABLE, value_node) {
                            builder.into_diagnostic(format_args!(
                                "Method `__getitem__` of type `{}` may not be callable on object of type `{}`",
                                bindings.callable_type().display(db),
                                value_ty.display(db),
                            ));
                        }
                    }
                }

                return bindings.return_type(db);
            }
            Err(CallDunderError::MethodNotAvailable) => {
                // try `__class_getitem__`
            }
        }

        // Otherwise, if the value is itself a class and defines `__class_getitem__`,
        // return its return type.
        //
        // TODO: lots of classes are only subscriptable at runtime on Python 3.9+,
        // *but* we should also allow them to be subscripted in stubs
        // (and in annotations if `from __future__ import annotations` is enabled),
        // even if the target version is Python 3.8 or lower,
        // despite the fact that there will be no corresponding `__class_getitem__`
        // method in these `sys.version_info` branches.
        if value_ty.is_subtype_of(db, KnownClass::Type.to_instance(db)) {
            let dunder_class_getitem_method = value_ty.member(db, "__class_getitem__").place;

            match dunder_class_getitem_method {
                Place::Undefined => {}
                Place::Defined(ty, _, boundness) => {
                    if boundness == Definedness::PossiblyUndefined {
                        if let Some(builder) =
                            context.report_lint(&POSSIBLY_MISSING_IMPLICIT_CALL, value_node)
                        {
                            builder.into_diagnostic(format_args!(
                                "Method `__class_getitem__` of type `{}` may be missing",
                                value_ty.display(db),
                            ));
                        }
                    }

                    match ty.try_call(db, &CallArguments::positional([slice_ty])) {
                        Ok(bindings) => return bindings.return_type(db),
                        Err(CallError(_, bindings)) => {
                            if let Some(builder) =
                                context.report_lint(&CALL_NON_CALLABLE, value_node)
                            {
                                builder.into_diagnostic(format_args!(
                                    "Method `__class_getitem__` of type `{}` \
                                        is not callable on object of type `{}`",
                                    bindings.callable_type().display(db),
                                    value_ty.display(db),
                                ));
                            }
                            return bindings.return_type(db);
                        }
                    }
                }
            }

            if let Type::ClassLiteral(class) = value_ty {
                if class.is_known(db, KnownClass::Type) {
                    return KnownClass::GenericAlias.to_instance(db);
                }

                if class.generic_context(db).is_some() {
                    // TODO: specialize the generic class using these explicit type
                    // variable assignments. This branch is only encountered when an
                    // explicit class specialization appears inside of some other subscript
                    // expression, e.g. `tuple[list[int], ...]`. We have already inferred
                    // the type of the outer subscript slice as a value expression, which
                    // means we can't re-infer the inner specialization here as a type
                    // expression.
                    return value_ty;
                }
            }

            // TODO: properly handle old-style generics; get rid of this temporary hack
            if !value_ty
                .as_class_literal()
                .is_some_and(|class| class.iter_mro(db, None).contains(&ClassBase::Generic))
            {
                report_non_subscriptable(context, subscript, value_ty, "__class_getitem__");
            }
        } else {
            if expr_context != ExprContext::Store {
                report_non_subscriptable(context, subscript, value_ty, "__getitem__");
            }
        }

        Type::unknown()
    }

    fn legacy_generic_class_context(
        &self,
        value_node: &ast::Expr,
        typevars: Type<'db>,
        origin: LegacyGenericBase,
    ) -> Result<GenericContext<'db>, GenericContextError> {
        let typevars_class_tuple_spec = typevars.exact_tuple_instance_spec(self.db());

        let typevars = if let Some(tuple_spec) = typevars_class_tuple_spec.as_deref() {
            match tuple_spec {
                Tuple::Fixed(typevars) => typevars.elements_slice(),
                // TODO: emit a diagnostic
                Tuple::Variable(_) => return Err(GenericContextError::VariadicTupleArguments),
            }
        } else {
            std::slice::from_ref(&typevars)
        };

        let typevars: Result<FxOrderSet<_>, GenericContextError> = typevars
            .iter()
            .map(|typevar| {
                if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = typevar {
                    bind_typevar(
                        self.db(),
                        self.index,
                        self.scope().file_scope_id(self.db()),
                        self.typevar_binding_context,
                        *typevar,
                    )
                    .ok_or(GenericContextError::InvalidArgument)
                } else if any_over_type(
                    self.db(),
                    *typevar,
                    &|ty| match ty {
                        Type::Dynamic(
                            DynamicType::TodoUnpack | DynamicType::TodoStarredExpression,
                        ) => true,
                        Type::NominalInstance(nominal) => {
                            nominal.has_known_class(self.db(), KnownClass::TypeVarTuple)
                        }
                        _ => false,
                    },
                    true,
                ) {
                    Err(GenericContextError::NotYetSupported)
                } else {
                    if let Some(builder) =
                        self.context.report_lint(&INVALID_ARGUMENT_TYPE, value_node)
                    {
                        builder.into_diagnostic(format_args!(
                            "`{}` is not a valid argument to `{origin}`",
                            typevar.display(self.db()),
                        ));
                    }
                    Err(GenericContextError::InvalidArgument)
                }
            })
            .collect();
        typevars.map(|typevars| GenericContext::from_typevar_instances(self.db(), typevars))
    }

    fn infer_slice_expression(&mut self, slice: &ast::ExprSlice) -> Type<'db> {
        enum SliceArg<'db> {
            Arg(Type<'db>),
            Unsupported,
        }

        let ast::ExprSlice {
            range: _,
            node_index: _,
            lower,
            upper,
            step,
        } = slice;

        let ty_lower = self.infer_optional_expression(lower.as_deref(), TypeContext::default());
        let ty_upper = self.infer_optional_expression(upper.as_deref(), TypeContext::default());
        let ty_step = self.infer_optional_expression(step.as_deref(), TypeContext::default());

        let type_to_slice_argument = |ty: Option<Type<'db>>| match ty {
            Some(ty @ (Type::IntLiteral(_) | Type::BooleanLiteral(_))) => SliceArg::Arg(ty),
            Some(ty @ Type::NominalInstance(instance))
                if instance.has_known_class(self.db(), KnownClass::NoneType) =>
            {
                SliceArg::Arg(ty)
            }
            None => SliceArg::Arg(Type::none(self.db())),
            _ => SliceArg::Unsupported,
        };

        match (
            type_to_slice_argument(ty_lower),
            type_to_slice_argument(ty_upper),
            type_to_slice_argument(ty_step),
        ) {
            (SliceArg::Arg(lower), SliceArg::Arg(upper), SliceArg::Arg(step)) => {
                KnownClass::Slice.to_specialized_instance(self.db(), [lower, upper, step])
            }
            _ => KnownClass::Slice.to_instance(self.db()),
        }
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
            deferred_state: _,
            multi_inference_state: _,
            inner_expression_inference_state: _,
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
            // builder only state
            dataclass_field_specifiers: _,
            all_definitely_bound: _,
            typevar_binding_context: _,
            deferred_state: _,
            multi_inference_state: _,
            inner_expression_inference_state: _,
            called_functions: _,
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
            || !deferred.is_empty())
        .then(|| {
            Box::new(DefinitionInferenceExtra {
                string_annotations,
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
            deferred_state: _,
            multi_inference_state: _,
            inner_expression_inference_state: _,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GenericContextError {
    /// It's invalid to subscript `Generic` or `Protocol` with this type
    InvalidArgument,
    /// It's invalid to subscript `Generic` or `Protocol` with a variadic tuple type.
    /// We should emit a diagnostic for this, but we don't yet.
    VariadicTupleArguments,
    /// It's valid to subscribe `Generic` or `Protocol` with this type,
    /// but the type is not yet supported.
    NotYetSupported,
}

impl GenericContextError {
    const fn into_type<'db>(self) -> Type<'db> {
        match self {
            GenericContextError::InvalidArgument | GenericContextError::VariadicTupleArguments => {
                Type::unknown()
            }
            GenericContextError::NotYetSupported => todo_type!("ParamSpecs and TypeVarTuples"),
        }
    }
}

/// Dictates the behavior when an expression is inferred multiple times.
#[derive(Default, Debug, Clone, Copy)]
enum MultiInferenceState {
    /// Panic if the expression has already been inferred.
    #[default]
    Panic,

    /// Overwrite the previously inferred value.
    ///
    /// Note that `Overwrite` does not interact well with nested inferences:
    /// it overwrites values that were written with `MultiInferenceState::Intersect`.
    Overwrite,

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RichCompareOperator {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

impl From<RichCompareOperator> for ast::CmpOp {
    fn from(value: RichCompareOperator) -> Self {
        match value {
            RichCompareOperator::Eq => ast::CmpOp::Eq,
            RichCompareOperator::Ne => ast::CmpOp::NotEq,
            RichCompareOperator::Lt => ast::CmpOp::Lt,
            RichCompareOperator::Le => ast::CmpOp::LtE,
            RichCompareOperator::Gt => ast::CmpOp::Gt,
            RichCompareOperator::Ge => ast::CmpOp::GtE,
        }
    }
}

impl RichCompareOperator {
    #[must_use]
    const fn dunder(self) -> &'static str {
        match self {
            RichCompareOperator::Eq => "__eq__",
            RichCompareOperator::Ne => "__ne__",
            RichCompareOperator::Lt => "__lt__",
            RichCompareOperator::Le => "__le__",
            RichCompareOperator::Gt => "__gt__",
            RichCompareOperator::Ge => "__ge__",
        }
    }

    #[must_use]
    const fn reflect(self) -> Self {
        match self {
            RichCompareOperator::Eq => RichCompareOperator::Eq,
            RichCompareOperator::Ne => RichCompareOperator::Ne,
            RichCompareOperator::Lt => RichCompareOperator::Gt,
            RichCompareOperator::Le => RichCompareOperator::Ge,
            RichCompareOperator::Gt => RichCompareOperator::Lt,
            RichCompareOperator::Ge => RichCompareOperator::Le,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MembershipTestCompareOperator {
    In,
    NotIn,
}

impl From<MembershipTestCompareOperator> for ast::CmpOp {
    fn from(value: MembershipTestCompareOperator) -> Self {
        match value {
            MembershipTestCompareOperator::In => ast::CmpOp::In,
            MembershipTestCompareOperator::NotIn => ast::CmpOp::NotIn,
        }
    }
}

/// Context for a failed comparison operation.
///
/// `left_ty` and `right_ty` are the "low-level" types
/// that cannot be compared using `op`. For example,
/// when evaluating `(1, "foo") < (2, 3)`, the "high-level"
/// types of the operands are `tuple[Literal[1], Literal["foo"]]`
/// and `tuple[Literal[2], Literal[3]]`. Those aren't captured
/// in this struct, but the "low-level" types that mean that
/// the high-level types cannot be compared *are* captured in
/// this struct. In this case, those would be `Literal["foo"]`
/// and `Literal[3]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnsupportedComparisonError<'db> {
    pub(crate) op: ast::CmpOp,
    pub(crate) left_ty: Type<'db>,
    pub(crate) right_ty: Type<'db>,
}

fn format_import_from_module(level: u32, module: Option<&str>) -> String {
    format!(
        "{}{}",
        ".".repeat(level as usize),
        module.unwrap_or_default()
    )
}

/// Struct collecting string parts when inferring a formatted string. Infers a string literal if the
/// concatenated string is small enough, otherwise infers a literal string.
///
/// If the formatted string contains an expression (with a representation unknown at compile time),
/// infers an instance of `builtins.str`.
#[derive(Debug)]
struct StringPartsCollector {
    concatenated: Option<String>,
    expression: bool,
}

impl StringPartsCollector {
    fn new() -> Self {
        Self {
            concatenated: Some(String::new()),
            expression: false,
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

    fn add_expression(&mut self) {
        self.concatenated = None;
        self.expression = true;
    }

    fn string_type(self, db: &dyn Db) -> Type<'_> {
        if self.expression {
            KnownClass::Str.to_instance(db)
        } else if let Some(concatenated) = self.concatenated {
            Type::string_literal(db, &concatenated)
        } else {
            Type::LiteralString
        }
    }
}

fn contains_string_literal(expr: &ast::Expr) -> bool {
    struct ContainsStringLiteral(bool);

    impl<'a> Visitor<'a> for ContainsStringLiteral {
        fn visit_expr(&mut self, expr: &'a ast::Expr) {
            self.0 |= matches!(expr, ast::Expr::StringLiteral(_));
            walk_expr(self, expr);
        }
    }

    let mut visitor = ContainsStringLiteral(false);
    visitor.visit_expr(expr);
    visitor.0
}

/// Map based on a `Vec`. It doesn't enforce
/// uniqueness on insertion. Instead, it relies on the caller
/// that elements are unique. For example, the way we visit definitions
/// in the `TypeInference` builder already implicitly guarantees that each definition
/// is only visited once.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct VecMap<K, V>(Vec<(K, V)>);

impl<K, V> VecMap<K, V>
where
    K: Eq,
    K: std::fmt::Debug,
    V: std::fmt::Debug,
{
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn iter(&self) -> impl ExactSizeIterator<Item = (&K, &V)> {
        self.0.iter().map(|(k, v)| (k, v))
    }

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
