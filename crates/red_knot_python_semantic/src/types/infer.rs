//! We have Salsa queries for inferring types at three different granularities: scope-level,
//! definition-level, and expression-level.
//!
//! Scope-level inference is for when we are actually checking a file, and need to check types for
//! everything in that file's scopes, or give a linter access to types of arbitrary expressions
//! (via the [`HasType`](crate::semantic_model::HasType) trait).
//!
//! Definition-level inference allows us to look up the types of symbols in other scopes (e.g. for
//! imports) with the minimum inference necessary, so that if we're looking up one symbol from a
//! very large module, we can avoid a bunch of unnecessary work. Definition-level inference also
//! allows us to handle import cycles without getting into a cycle of scope-level inference
//! queries.
//!
//! The expression-level inference query is needed in only a few cases. Since some assignments can
//! have multiple targets (via `x = y = z` or unpacking `(x, y) = z`, they can be associated with
//! multiple definitions (one per assigned symbol). In order to avoid inferring the type of the
//! right-hand side once per definition, we infer it as a standalone query, so its result will be
//! cached by Salsa. We also need the expression-level query for inferring types in type guard
//! expressions (e.g. the test clause of an `if` statement.)
//!
//! Inferring types at any of the three region granularities returns a [`TypeInference`], which
//! holds types for every [`Definition`] and expression within the inferred region.
//!
//! Some type expressions can require deferred evaluation. This includes all type expressions in
//! stub files, or annotation expressions in modules with `from __future__ import annotations`, or
//! stringified annotations. We have a fourth Salsa query for inferring the deferred types
//! associated with a particular definition. Scope-level inference infers deferred types for all
//! definitions once the rest of the types in the scope have been inferred.
//!
//! Many of our type inference Salsa queries implement cycle recovery via fixed-point iteration. In
//! general, they initiate fixed-point iteration by returning a `TypeInference` that returns
//! `Type::Never` for all expressions, bindings, and declarations, and then they continue iterating
//! the query cycle until a fixed-point is reached. Salsa has a built-in fixed limit on the number
//! of iterations, so if we fail to converge, Salsa will eventually panic. (This should of course
//! be considered a bug.)
use itertools::{Either, Itertools};
use ruff_db::diagnostic::{Annotation, DiagnosticId, Severity};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::{walk_expr, Visitor};
use ruff_python_ast::{self as ast, AnyNodeRef, ExprContext};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::{FxHashMap, FxHashSet};
use salsa;
use salsa::plumbing::AsId;

use crate::module_name::{ModuleName, ModuleNameResolutionError};
use crate::module_resolver::resolve_module;
use crate::node_key::NodeKey;
use crate::semantic_index::ast_ids::{HasScopedExpressionId, HasScopedUseId, ScopedExpressionId};
use crate::semantic_index::definition::{
    AnnotatedAssignmentDefinitionKind, AssignmentDefinitionKind, ComprehensionDefinitionKind,
    Definition, DefinitionKind, DefinitionNodeKey, ExceptHandlerDefinitionKind,
    ForStmtDefinitionKind, TargetKind, WithItemDefinitionKind,
};
use crate::semantic_index::expression::{Expression, ExpressionKind};
use crate::semantic_index::symbol::{
    FileScopeId, NodeWithScopeKind, NodeWithScopeRef, ScopeId, ScopeKind,
};
use crate::semantic_index::{semantic_index, EagerBindingsResult, SemanticIndex};
use crate::symbol::{
    builtins_module_scope, builtins_symbol, explicit_global_symbol,
    module_type_implicit_global_symbol, symbol, symbol_from_bindings, symbol_from_declarations,
    typing_extensions_symbol, Boundness, LookupError,
};
use crate::types::call::{Argument, Bindings, CallArgumentTypes, CallArguments, CallError};
use crate::types::class::MetaclassErrorKind;
use crate::types::diagnostic::{
    report_implicit_return_type, report_invalid_arguments_to_annotated,
    report_invalid_arguments_to_callable, report_invalid_assignment,
    report_invalid_attribute_assignment, report_invalid_return_type,
    report_possibly_unbound_attribute, TypeCheckDiagnostics, CALL_NON_CALLABLE,
    CALL_POSSIBLY_UNBOUND_METHOD, CONFLICTING_DECLARATIONS, CONFLICTING_METACLASS,
    CYCLIC_CLASS_DEFINITION, DIVISION_BY_ZERO, DUPLICATE_BASE, INCONSISTENT_MRO,
    INVALID_ASSIGNMENT, INVALID_ATTRIBUTE_ACCESS, INVALID_BASE, INVALID_DECLARATION,
    INVALID_LEGACY_TYPE_VARIABLE, INVALID_PARAMETER_DEFAULT, INVALID_TYPE_FORM,
    INVALID_TYPE_VARIABLE_CONSTRAINTS, POSSIBLY_UNBOUND_IMPORT, UNDEFINED_REVEAL,
    UNRESOLVED_ATTRIBUTE, UNRESOLVED_IMPORT, UNSUPPORTED_OPERATOR,
};
use crate::types::generics::GenericContext;
use crate::types::mro::MroErrorKind;
use crate::types::unpacker::{UnpackResult, Unpacker};
use crate::types::{
    binding_type, todo_type, CallDunderError, CallableSignature, CallableType, ClassLiteral,
    ClassType, DataclassParams, DynamicType, FunctionDecorators, FunctionType, GenericAlias,
    IntersectionBuilder, IntersectionType, KnownClass, KnownFunction, KnownInstanceType,
    MemberLookupPolicy, MetaclassCandidate, Parameter, ParameterForm, Parameters, Signature,
    Signatures, SliceLiteralType, StringLiteralType, SubclassOfType, Symbol, SymbolAndQualifiers,
    Truthiness, TupleType, Type, TypeAliasType, TypeAndQualifiers, TypeArrayDisplay,
    TypeQualifiers, TypeVarBoundOrConstraints, TypeVarInstance, TypeVarKind, UnionBuilder,
    UnionType,
};
use crate::unpack::{Unpack, UnpackPosition};
use crate::util::subscript::{PyIndex, PySlice};
use crate::Db;

use super::context::{InNoTypeCheck, InferContext};
use super::diagnostic::{
    report_attempted_protocol_instantiation, report_bad_argument_to_get_protocol_members,
    report_index_out_of_bounds, report_invalid_exception_caught, report_invalid_exception_cause,
    report_invalid_exception_raised, report_invalid_type_checking_constant,
    report_non_subscriptable, report_possibly_unresolved_reference,
    report_runtime_check_against_non_runtime_checkable_protocol, report_slice_step_size_zero,
    report_unresolved_reference, INVALID_METACLASS, INVALID_PROTOCOL, REDUNDANT_CAST,
    STATIC_ASSERT_ERROR, SUBCLASS_OF_FINAL_CLASS, TYPE_ASSERTION_FAILURE,
};
use super::slots::check_class_slots;
use super::string_annotation::{
    parse_string_annotation, BYTE_STRING_TYPE_ANNOTATION, FSTRING_TYPE_ANNOTATION,
};
use super::subclass_of::SubclassOfInner;
use super::{BoundSuperError, BoundSuperType, ClassBase};

/// Infer all types for a [`ScopeId`], including all definitions and expressions in that scope.
/// Use when checking a scope, or needing to provide a type for an arbitrary expression in the
/// scope.
#[salsa::tracked(return_ref, cycle_fn=scope_cycle_recover, cycle_initial=scope_cycle_initial)]
pub(crate) fn infer_scope_types<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> TypeInference<'db> {
    let file = scope.file(db);
    let _span = tracing::trace_span!("infer_scope_types", scope=?scope.as_id(), ?file).entered();

    // Using the index here is fine because the code below depends on the AST anyway.
    // The isolation of the query is by the return inferred types.
    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(db, InferenceRegion::Scope(scope), index).finish()
}

fn scope_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &TypeInference<'db>,
    _count: u32,
    _scope: ScopeId<'db>,
) -> salsa::CycleRecoveryAction<TypeInference<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn scope_cycle_initial<'db>(_db: &'db dyn Db, scope: ScopeId<'db>) -> TypeInference<'db> {
    TypeInference::cycle_fallback(scope, Type::Never)
}

/// Infer all types for a [`Definition`] (including sub-expressions).
/// Use when resolving a symbol name use or public type of a symbol.
#[salsa::tracked(return_ref, cycle_fn=definition_cycle_recover, cycle_initial=definition_cycle_initial)]
pub(crate) fn infer_definition_types<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> TypeInference<'db> {
    let file = definition.file(db);
    let _span = tracing::trace_span!(
        "infer_definition_types",
        range = ?definition.kind(db).target_range(),
        ?file
    )
    .entered();

    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(db, InferenceRegion::Definition(definition), index).finish()
}

fn definition_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &TypeInference<'db>,
    _count: u32,
    _definition: Definition<'db>,
) -> salsa::CycleRecoveryAction<TypeInference<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn definition_cycle_initial<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> TypeInference<'db> {
    TypeInference::cycle_fallback(definition.scope(db), Type::Never)
}

/// Infer types for all deferred type expressions in a [`Definition`].
///
/// Deferred expressions are type expressions (annotations, base classes, aliases...) in a stub
/// file, or in a file with `from __future__ import annotations`, or stringified annotations.
#[salsa::tracked(return_ref, cycle_fn=deferred_cycle_recover, cycle_initial=deferred_cycle_initial)]
pub(crate) fn infer_deferred_types<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> TypeInference<'db> {
    let file = definition.file(db);
    let _span = tracing::trace_span!(
        "infer_deferred_types",
        definition = ?definition.as_id(),
        range = ?definition.kind(db).target_range(),
        ?file
    )
    .entered();

    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(db, InferenceRegion::Deferred(definition), index).finish()
}

fn deferred_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &TypeInference<'db>,
    _count: u32,
    _definition: Definition<'db>,
) -> salsa::CycleRecoveryAction<TypeInference<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn deferred_cycle_initial<'db>(db: &'db dyn Db, definition: Definition<'db>) -> TypeInference<'db> {
    TypeInference::cycle_fallback(definition.scope(db), Type::Never)
}

/// Infer all types for an [`Expression`] (including sub-expressions).
/// Use rarely; only for cases where we'd otherwise risk double-inferring an expression: RHS of an
/// assignment, which might be unpacking/multi-target and thus part of multiple definitions, or a
/// type narrowing guard expression (e.g. if statement test node).
#[salsa::tracked(return_ref, cycle_fn=expression_cycle_recover, cycle_initial=expression_cycle_initial)]
pub(crate) fn infer_expression_types<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> TypeInference<'db> {
    let file = expression.file(db);
    let _span = tracing::trace_span!(
        "infer_expression_types",
        expression = ?expression.as_id(),
        range = ?expression.node_ref(db).range(),
        ?file
    )
    .entered();

    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(db, InferenceRegion::Expression(expression), index).finish()
}

fn expression_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &TypeInference<'db>,
    _count: u32,
    _expression: Expression<'db>,
) -> salsa::CycleRecoveryAction<TypeInference<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn expression_cycle_initial<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> TypeInference<'db> {
    TypeInference::cycle_fallback(expression.scope(db), Type::Never)
}

/// Infers the type of an `expression` that is guaranteed to be in the same file as the calling query.
///
/// This is a small helper around [`infer_expression_types()`] to reduce the boilerplate.
/// Use [`infer_expression_type()`] if it isn't guaranteed that `expression` is in the same file to
/// avoid cross-file query dependencies.
pub(super) fn infer_same_file_expression_type<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> Type<'db> {
    let inference = infer_expression_types(db, expression);
    let scope = expression.scope(db);
    inference.expression_type(expression.node_ref(db).scoped_expression_id(db, scope))
}

/// Infers the type of an expression where the expression might come from another file.
///
/// Use this over [`infer_expression_types`] if the expression might come from another file than the
/// enclosing query to avoid cross-file query dependencies.
///
/// Use [`infer_same_file_expression_type`] if it is guaranteed that  `expression` is in the same
/// to avoid unnecessary salsa ingredients. This is normally the case inside the `TypeInferenceBuilder`.
#[salsa::tracked(cycle_fn=single_expression_cycle_recover, cycle_initial=single_expression_cycle_initial)]
pub(crate) fn infer_expression_type<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> Type<'db> {
    // It's okay to call the "same file" version here because we're inside a salsa query.
    infer_same_file_expression_type(db, expression)
}

fn single_expression_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &Type<'db>,
    _count: u32,
    _expression: Expression<'db>,
) -> salsa::CycleRecoveryAction<Type<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn single_expression_cycle_initial<'db>(
    _db: &'db dyn Db,
    _expression: Expression<'db>,
) -> Type<'db> {
    Type::Never
}

/// Infer the types for an [`Unpack`] operation.
///
/// This infers the expression type and performs structural match against the target expression
/// involved in an unpacking operation. It returns a result-like object that can be used to get the
/// type of the variables involved in this unpacking along with any violations that are detected
/// during this unpacking.
#[salsa::tracked(return_ref)]
pub(super) fn infer_unpack_types<'db>(db: &'db dyn Db, unpack: Unpack<'db>) -> UnpackResult<'db> {
    let file = unpack.file(db);
    let _span =
        tracing::trace_span!("infer_unpack_types", range=?unpack.range(db), ?file).entered();

    let mut unpacker = Unpacker::new(db, unpack.target_scope(db), unpack.value_scope(db));
    unpacker.unpack(unpack.target(db), unpack.value(db));
    unpacker.finish()
}

/// A region within which we can infer types.
#[derive(Copy, Clone, Debug)]
pub(crate) enum InferenceRegion<'db> {
    /// infer types for a standalone [`Expression`]
    Expression(Expression<'db>),
    /// infer types for a [`Definition`]
    Definition(Definition<'db>),
    /// infer deferred types for a [`Definition`]
    Deferred(Definition<'db>),
    /// infer types for an entire [`ScopeId`]
    Scope(ScopeId<'db>),
}

impl<'db> InferenceRegion<'db> {
    fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        match self {
            InferenceRegion::Expression(expression) => expression.scope(db),
            InferenceRegion::Definition(definition) | InferenceRegion::Deferred(definition) => {
                definition.scope(db)
            }
            InferenceRegion::Scope(scope) => scope,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct TypeAndRange<'db> {
    ty: Type<'db>,
    range: TextRange,
}

/// The inferred types for a single region.
#[derive(Debug, Eq, PartialEq, salsa::Update)]
pub(crate) struct TypeInference<'db> {
    /// The types of every expression in this region.
    expressions: FxHashMap<ScopedExpressionId, Type<'db>>,

    /// The types of every binding in this region.
    bindings: FxHashMap<Definition<'db>, Type<'db>>,

    /// The types and type qualifiers of every declaration in this region.
    declarations: FxHashMap<Definition<'db>, TypeAndQualifiers<'db>>,

    /// The definitions that are deferred.
    deferred: FxHashSet<Definition<'db>>,

    /// The diagnostics for this region.
    diagnostics: TypeCheckDiagnostics,

    /// The scope this region is part of.
    scope: ScopeId<'db>,

    /// The fallback type for missing expressions/bindings/declarations.
    ///
    /// This is used only when constructing a cycle-recovery `TypeInference`.
    cycle_fallback_type: Option<Type<'db>>,
}

impl<'db> TypeInference<'db> {
    pub(crate) fn empty(scope: ScopeId<'db>) -> Self {
        Self {
            expressions: FxHashMap::default(),
            bindings: FxHashMap::default(),
            declarations: FxHashMap::default(),
            deferred: FxHashSet::default(),
            diagnostics: TypeCheckDiagnostics::default(),
            scope,
            cycle_fallback_type: None,
        }
    }

    fn cycle_fallback(scope: ScopeId<'db>, cycle_fallback_type: Type<'db>) -> Self {
        Self {
            expressions: FxHashMap::default(),
            bindings: FxHashMap::default(),
            declarations: FxHashMap::default(),
            deferred: FxHashSet::default(),
            diagnostics: TypeCheckDiagnostics::default(),
            scope,
            cycle_fallback_type: Some(cycle_fallback_type),
        }
    }

    #[track_caller]
    pub(crate) fn expression_type(&self, expression: ScopedExpressionId) -> Type<'db> {
        self.try_expression_type(expression).expect(
            "expression should belong to this TypeInference region and \
            TypeInferenceBuilder should have inferred a type for it",
        )
    }

    pub(crate) fn try_expression_type(&self, expression: ScopedExpressionId) -> Option<Type<'db>> {
        self.expressions
            .get(&expression)
            .copied()
            .or(self.cycle_fallback_type)
    }

    #[track_caller]
    pub(crate) fn binding_type(&self, definition: Definition<'db>) -> Type<'db> {
        self.bindings
            .get(&definition)
            .copied()
            .or(self.cycle_fallback_type)
            .expect(
                "definition should belong to this TypeInference region and
                TypeInferenceBuilder should have inferred a type for it",
            )
    }

    #[track_caller]
    pub(crate) fn declaration_type(&self, definition: Definition<'db>) -> TypeAndQualifiers<'db> {
        self.declarations
            .get(&definition)
            .copied()
            .or(self.cycle_fallback_type.map(Into::into))
            .expect(
                "definition should belong to this TypeInference region and
                TypeInferenceBuilder should have inferred a type for it",
            )
    }

    pub(crate) fn diagnostics(&self) -> &TypeCheckDiagnostics {
        &self.diagnostics
    }

    fn shrink_to_fit(&mut self) {
        self.expressions.shrink_to_fit();
        self.bindings.shrink_to_fit();
        self.declarations.shrink_to_fit();
        self.diagnostics.shrink_to_fit();
        self.deferred.shrink_to_fit();
    }
}

/// Whether the intersection type is on the left or right side of the comparison.
#[derive(Debug, Clone, Copy)]
enum IntersectionOn {
    Left,
    Right,
}

/// A helper to track if we already know that declared and inferred types are the same.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DeclaredAndInferredType<'db> {
    /// We know that both the declared and inferred types are the same.
    AreTheSame(Type<'db>),
    /// Declared and inferred types might be different, we need to check assignability.
    MightBeDifferent {
        declared_ty: TypeAndQualifiers<'db>,
        inferred_ty: Type<'db>,
    },
}

/// Builder to infer all types in a region.
///
/// A builder is used by creating it with [`new()`](TypeInferenceBuilder::new), and then calling
/// [`finish()`](TypeInferenceBuilder::finish) on it, which returns the resulting
/// [`TypeInference`].
///
/// There are a few different kinds of methods in the type inference builder, and the naming
/// distinctions are a bit subtle.
///
/// The `finish` method calls [`infer_region`](TypeInferenceBuilder::infer_region), which delegates
/// to one of [`infer_region_scope`](TypeInferenceBuilder::infer_region_scope),
/// [`infer_region_definition`](TypeInferenceBuilder::infer_region_definition), or
/// [`infer_region_expression`](TypeInferenceBuilder::infer_region_expression), depending which
/// kind of [`InferenceRegion`] we are inferring types for.
///
/// Scope inference starts with the scope body, walking all statements and expressions and
/// recording the types of each expression in the [`TypeInference`] result. Most of the methods
/// here (with names like `infer_*_statement` or `infer_*_expression` or some other node kind) take
/// a single AST node and are called as part of this AST visit.
///
/// When the visit encounters a node which creates a [`Definition`], we look up the definition in
/// the semantic index and call the [`infer_definition_types()`] query on it, which creates another
/// [`TypeInferenceBuilder`] just for that definition, and we merge the returned [`TypeInference`]
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
pub(super) struct TypeInferenceBuilder<'db> {
    context: InferContext<'db>,
    index: &'db SemanticIndex<'db>,
    region: InferenceRegion<'db>,

    /// The type inference results
    types: TypeInference<'db>,

    /// The returned types and their corresponding ranges of the region, if it is a function body.
    return_types_and_ranges: Vec<TypeAndRange<'db>>,

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
}

impl<'db> TypeInferenceBuilder<'db> {
    /// How big a string do we build before bailing?
    ///
    /// This is a fairly arbitrary number. It should be *far* more than enough
    /// for most use cases, but we can reevaluate it later if useful.
    const MAX_STRING_LITERAL_SIZE: usize = 4096;

    /// Creates a new builder for inferring types in a region.
    pub(super) fn new(
        db: &'db dyn Db,
        region: InferenceRegion<'db>,
        index: &'db SemanticIndex<'db>,
    ) -> Self {
        let scope = region.scope(db);

        Self {
            context: InferContext::new(db, scope),
            index,
            region,
            return_types_and_ranges: vec![],
            deferred_state: DeferredExpressionState::None,
            types: TypeInference::empty(scope),
        }
    }

    fn extend(&mut self, inference: &TypeInference<'db>) {
        debug_assert_eq!(self.types.scope, inference.scope);

        self.types.bindings.extend(inference.bindings.iter());
        self.types
            .declarations
            .extend(inference.declarations.iter());
        self.types.expressions.extend(inference.expressions.iter());
        self.types.deferred.extend(inference.deferred.iter());
        self.context.extend(inference.diagnostics());
    }

    fn file(&self) -> File {
        self.context.file()
    }

    fn db(&self) -> &'db dyn Db {
        self.context.db()
    }

    fn scope(&self) -> ScopeId<'db> {
        self.types.scope
    }

    /// Are we currently inferring types in file with deferred types?
    /// This is true for stub files and files with `__future__.annotations`
    fn defer_annotations(&self) -> bool {
        self.index.has_future_annotations() || self.in_stub()
    }

    /// Are we currently inferring deferred types?
    fn is_deferred(&self) -> bool {
        matches!(self.region, InferenceRegion::Deferred(_)) || self.deferred_state.is_deferred()
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

    /// Get the already-inferred type of an expression node.
    ///
    /// ## Panics
    /// If the expression is not within this region, or if no type has yet been inferred for
    /// this node.
    #[track_caller]
    fn expression_type(&self, expr: &ast::Expr) -> Type<'db> {
        self.types
            .expression_type(expr.scoped_expression_id(self.db(), self.scope()))
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
        let expr_id = expression.scoped_expression_id(self.db(), expr_scope);
        match self.region {
            InferenceRegion::Scope(scope) if scope == expr_scope => {
                self.expression_type(expression)
            }
            _ => infer_scope_types(self.db(), expr_scope).expression_type(expr_id),
        }
    }

    /// Infers types in the given [`InferenceRegion`].
    fn infer_region(&mut self) {
        match self.region {
            InferenceRegion::Scope(scope) => self.infer_region_scope(scope),
            InferenceRegion::Definition(definition) => self.infer_region_definition(definition),
            InferenceRegion::Deferred(definition) => self.infer_region_deferred(definition),
            InferenceRegion::Expression(expression) => self.infer_region_expression(expression),
        }
    }

    fn infer_region_scope(&mut self, scope: ScopeId<'db>) {
        let node = scope.node(self.db());
        match node {
            NodeWithScopeKind::Module => {
                let parsed = parsed_module(self.db().upcast(), self.file());
                self.infer_module(parsed.syntax());
            }
            NodeWithScopeKind::Function(function) => self.infer_function_body(function.node()),
            NodeWithScopeKind::Lambda(lambda) => self.infer_lambda_body(lambda.node()),
            NodeWithScopeKind::Class(class) => self.infer_class_body(class.node()),
            NodeWithScopeKind::ClassTypeParameters(class) => {
                self.infer_class_type_params(class.node());
            }
            NodeWithScopeKind::FunctionTypeParameters(function) => {
                self.infer_function_type_params(function.node());
            }
            NodeWithScopeKind::TypeAliasTypeParameters(type_alias) => {
                self.infer_type_alias_type_params(type_alias.node());
            }
            NodeWithScopeKind::TypeAlias(type_alias) => {
                self.infer_type_alias(type_alias.node());
            }
            NodeWithScopeKind::ListComprehension(comprehension) => {
                self.infer_list_comprehension_expression_scope(comprehension.node());
            }
            NodeWithScopeKind::SetComprehension(comprehension) => {
                self.infer_set_comprehension_expression_scope(comprehension.node());
            }
            NodeWithScopeKind::DictComprehension(comprehension) => {
                self.infer_dict_comprehension_expression_scope(comprehension.node());
            }
            NodeWithScopeKind::GeneratorExpression(generator) => {
                self.infer_generator_expression_scope(generator.node());
            }
        }

        // Infer the deferred types for the definitions here to consider the end-of-scope
        // semantics.
        for definition in std::mem::take(&mut self.types.deferred) {
            self.extend(infer_deferred_types(self.db(), definition));
        }
        assert!(
            self.types.deferred.is_empty(),
            "Inferring deferred types should not add more deferred definitions"
        );

        // TODO: Only call this function when diagnostics are enabled.
        self.check_class_definitions();
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
        let class_definitions = self
            .types
            .declarations
            .iter()
            .filter_map(|(definition, ty)| {
                // Filter out class literals that result from imports
                if let DefinitionKind::Class(class) = definition.kind(self.db()) {
                    ty.inner_type()
                        .into_class_literal()
                        .map(|ty| (ty, class.node()))
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
                // Attempting to determine the MRO of a class or if the class has a metaclass conflict
                // is impossible if the class is cyclically defined; there's nothing more to do here.
                continue;
            }

            let is_protocol = class.is_protocol(self.db());

            // (2) Iterate through the class's explicit bases to check for various possible errors:
            //     - Check for inheritance from plain `Generic`,
            //     - Check for inheritance from a `@final` classes
            //     - If the class is a protocol class: check for inheritance from a non-protocol class
            for (i, base_class) in class.explicit_bases(self.db()).iter().enumerate() {
                let base_class = match base_class {
                    Type::KnownInstance(KnownInstanceType::Generic) => {
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
                    Type::ClassLiteral(class) => class,
                    // dynamic/unknown bases are never `@final`
                    _ => continue,
                };

                if is_protocol
                    && !(base_class.is_protocol(self.db())
                        || base_class.is_known(self.db(), KnownClass::Object))
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

            // (3) Check that the class's MRO is resolvable
            match class.try_mro(self.db(), None).as_ref() {
                Err(mro_error) => {
                    match mro_error.reason() {
                        MroErrorKind::DuplicateBases(duplicates) => {
                            let base_nodes = class_node.bases();
                            for (index, duplicate) in duplicates {
                                let Some(builder) = self
                                    .context
                                    .report_lint(&DUPLICATE_BASE, &base_nodes[*index])
                                else {
                                    continue;
                                };
                                builder.into_diagnostic(format_args!(
                                    "Duplicate base class `{}`",
                                    duplicate.name(self.db())
                                ));
                            }
                        }
                        MroErrorKind::InvalidBases(bases) => {
                            let base_nodes = class_node.bases();
                            for (index, base_ty) in bases {
                                if base_ty.is_never() {
                                    // A class base of type `Never` can appear in unreachable code. It
                                    // does not indicate a problem, since the actual construction of the
                                    // class will never happen.
                                    continue;
                                }
                                let Some(builder) =
                                    self.context.report_lint(&INVALID_BASE, &base_nodes[*index])
                                else {
                                    continue;
                                };
                                builder.into_diagnostic(format_args!(
                                    "Invalid class base with type `{}` \
                                     (all bases must be a class, `Any`, `Unknown` or `Todo`)",
                                    base_ty.display(self.db())
                                ));
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
                    }
                }
                Ok(_) => check_class_slots(&self.context, class, class_node),
            }

            // (4) Check that the class's metaclass can be determined without error.
            if let Err(metaclass_error) = class.try_metaclass(self.db()) {
                match metaclass_error.reason() {
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
        }
    }

    fn infer_region_definition(&mut self, definition: Definition<'db>) {
        match definition.kind(self.db()) {
            DefinitionKind::Function(function) => {
                self.infer_function_definition(function.node(), definition);
            }
            DefinitionKind::Class(class) => self.infer_class_definition(class.node(), definition),
            DefinitionKind::TypeAlias(type_alias) => {
                self.infer_type_alias_definition(type_alias.node(), definition);
            }
            DefinitionKind::Import(import) => {
                self.infer_import_definition(import.import(), import.alias(), definition);
            }
            DefinitionKind::ImportFrom(import_from) => {
                self.infer_import_from_definition(
                    import_from.import(),
                    import_from.alias(),
                    definition,
                );
            }
            DefinitionKind::StarImport(import) => {
                self.infer_import_from_definition(import.import(), import.alias(), definition);
            }
            DefinitionKind::Assignment(assignment) => {
                self.infer_assignment_definition(assignment, definition);
            }
            DefinitionKind::AnnotatedAssignment(annotated_assignment) => {
                self.infer_annotated_assignment_definition(annotated_assignment, definition);
            }
            DefinitionKind::AugmentedAssignment(augmented_assignment) => {
                self.infer_augment_assignment_definition(augmented_assignment.node(), definition);
            }
            DefinitionKind::For(for_statement_definition) => {
                self.infer_for_statement_definition(for_statement_definition, definition);
            }
            DefinitionKind::NamedExpression(named_expression) => {
                self.infer_named_expression_definition(named_expression.node(), definition);
            }
            DefinitionKind::Comprehension(comprehension) => {
                self.infer_comprehension_definition(comprehension, definition);
            }
            DefinitionKind::VariadicPositionalParameter(parameter) => {
                self.infer_variadic_positional_parameter_definition(parameter, definition);
            }
            DefinitionKind::VariadicKeywordParameter(parameter) => {
                self.infer_variadic_keyword_parameter_definition(parameter, definition);
            }
            DefinitionKind::Parameter(parameter_with_default) => {
                self.infer_parameter_definition(parameter_with_default, definition);
            }
            DefinitionKind::WithItem(with_item_definition) => {
                self.infer_with_item_definition(with_item_definition, definition);
            }
            DefinitionKind::MatchPattern(match_pattern) => {
                self.infer_match_pattern_definition(
                    match_pattern.pattern(),
                    match_pattern.index(),
                    definition,
                );
            }
            DefinitionKind::ExceptHandler(except_handler_definition) => {
                self.infer_except_handler_definition(except_handler_definition, definition);
            }
            DefinitionKind::TypeVar(node) => {
                self.infer_typevar_definition(node, definition);
            }
            DefinitionKind::ParamSpec(node) => {
                self.infer_paramspec_definition(node, definition);
            }
            DefinitionKind::TypeVarTuple(node) => {
                self.infer_typevartuple_definition(node, definition);
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
            DefinitionKind::Function(function) => self.infer_function_deferred(function.node()),
            DefinitionKind::Class(class) => self.infer_class_deferred(class.node()),
            _ => {}
        }
    }

    fn infer_region_expression(&mut self, expression: Expression<'db>) {
        match expression.kind(self.db()) {
            ExpressionKind::Normal => {
                self.infer_expression_impl(expression.node_ref(self.db()));
            }
            ExpressionKind::TypeExpression => {
                self.infer_type_expression(expression.node_ref(self.db()));
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
                    instance.class().known(self.db()),
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

    fn add_binding(&mut self, node: AnyNodeRef, binding: Definition<'db>, ty: Type<'db>) {
        debug_assert!(binding
            .kind(self.db())
            .category(self.context.in_stub())
            .is_binding());
        let use_def = self.index.use_def_map(binding.file_scope(self.db()));
        let declarations = use_def.declarations_at_binding(binding);
        let mut bound_ty = ty;
        let declared_ty = symbol_from_declarations(self.db(), declarations)
            .map(|SymbolAndQualifiers { symbol, .. }| {
                symbol.ignore_possibly_unbound().unwrap_or(Type::unknown())
            })
            .unwrap_or_else(|(ty, conflicting)| {
                // TODO point out the conflicting declarations in the diagnostic?
                let symbol_table = self.index.symbol_table(binding.file_scope(self.db()));
                let symbol_name = symbol_table.symbol(binding.symbol(self.db())).name();
                if let Some(builder) = self.context.report_lint(&CONFLICTING_DECLARATIONS, node) {
                    builder.into_diagnostic(format_args!(
                        "Conflicting declared types for `{symbol_name}`: {}",
                        conflicting.display(self.db())
                    ));
                }
                ty.inner_type()
            });
        if !bound_ty.is_assignable_to(self.db(), declared_ty) {
            report_invalid_assignment(&self.context, node, declared_ty, bound_ty);
            // allow declarations to override inference in case of invalid assignment
            bound_ty = declared_ty;
        }

        self.types.bindings.insert(binding, bound_ty);
    }

    fn add_declaration(
        &mut self,
        node: AnyNodeRef,
        declaration: Definition<'db>,
        ty: TypeAndQualifiers<'db>,
    ) {
        debug_assert!(declaration
            .kind(self.db())
            .category(self.context.in_stub())
            .is_declaration());
        let use_def = self.index.use_def_map(declaration.file_scope(self.db()));
        let prior_bindings = use_def.bindings_at_declaration(declaration);
        // unbound_ty is Never because for this check we don't care about unbound
        let inferred_ty = symbol_from_bindings(self.db(), prior_bindings)
            .ignore_possibly_unbound()
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
            TypeAndQualifiers::unknown()
        };
        self.types.declarations.insert(declaration, ty);
    }

    fn add_declaration_with_binding(
        &mut self,
        node: AnyNodeRef,
        definition: Definition<'db>,
        declared_and_inferred_ty: &DeclaredAndInferredType<'db>,
    ) {
        debug_assert!(definition
            .kind(self.db())
            .category(self.context.in_stub())
            .is_binding());
        debug_assert!(definition
            .kind(self.db())
            .category(self.context.in_stub())
            .is_declaration());

        let (declared_ty, inferred_ty) = match *declared_and_inferred_ty {
            DeclaredAndInferredType::AreTheSame(ty) => (ty.into(), ty),
            DeclaredAndInferredType::MightBeDifferent {
                declared_ty,
                inferred_ty,
            } => {
                if inferred_ty.is_assignable_to(self.db(), declared_ty.inner_type()) {
                    (declared_ty, inferred_ty)
                } else {
                    report_invalid_assignment(
                        &self.context,
                        node,
                        declared_ty.inner_type(),
                        inferred_ty,
                    );
                    // if the assignment is invalid, fall back to assuming the annotation is correct
                    (declared_ty, declared_ty.inner_type())
                }
            }
        };
        self.types.declarations.insert(definition, declared_ty);
        self.types.bindings.insert(definition, inferred_ty);
    }

    fn add_unknown_declaration_with_binding(
        &mut self,
        node: AnyNodeRef,
        definition: Definition<'db>,
    ) {
        self.add_declaration_with_binding(
            node,
            definition,
            &DeclaredAndInferredType::AreTheSame(Type::unknown()),
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

        self.infer_type_parameters(type_params);

        if let Some(arguments) = class.arguments.as_deref() {
            let call_arguments = Self::parse_arguments(arguments);
            let argument_forms = vec![Some(ParameterForm::Value); call_arguments.len()];
            self.infer_argument_types(arguments, call_arguments, &argument_forms);
        }
    }

    fn infer_class_body(&mut self, class: &ast::StmtClassDef) {
        self.infer_body(&class.body);
    }

    fn infer_function_type_params(&mut self, function: &ast::StmtFunctionDef) {
        let type_params = function
            .type_params
            .as_deref()
            .expect("function type params scope without type params");

        self.infer_optional_annotation_expression(
            function.returns.as_deref(),
            DeferredExpressionState::None,
        );
        self.infer_type_parameters(type_params);
        self.infer_parameters(&function.parameters);
    }

    fn infer_type_alias_type_params(&mut self, type_alias: &ast::StmtTypeAlias) {
        let type_params = type_alias
            .type_params
            .as_ref()
            .expect("type alias type params scope without type params");

        self.infer_type_parameters(type_params);
    }

    fn infer_type_alias(&mut self, type_alias: &ast::StmtTypeAlias) {
        self.infer_annotation_expression(&type_alias.value, DeferredExpressionState::Deferred);
    }

    /// Returns `true` if the current scope is the function body scope of a method of a protocol
    /// (that is, a class which directly inherits `typing.Protocol`.)
    fn in_protocol_class(&self) -> bool {
        let current_scope_id = self.scope().file_scope_id(self.db());
        let current_scope = self.index.scope(current_scope_id);
        let Some(parent_scope_id) = current_scope.parent() else {
            return false;
        };
        let parent_scope = self.index.scope(parent_scope_id);

        let class_scope = match parent_scope.kind() {
            ScopeKind::Class => parent_scope,
            ScopeKind::Annotation => {
                let Some(class_scope_id) = parent_scope.parent() else {
                    return false;
                };
                let potentially_class_scope = self.index.scope(class_scope_id);

                match potentially_class_scope.kind() {
                    ScopeKind::Class => potentially_class_scope,
                    _ => return false,
                }
            }
            _ => return false,
        };

        let NodeWithScopeKind::Class(node_ref) = class_scope.node() else {
            return false;
        };

        let class_definition = self.index.expect_single_definition(node_ref.node());

        let Type::ClassLiteral(class) = binding_type(self.db(), class_definition) else {
            return false;
        };

        class.is_protocol(self.db())
    }

    /// Returns `true` if the current scope is the function body scope of a function overload (that
    /// is, the stub declaration decorated with `@overload`, not the implementation), or an
    /// abstract method (decorated with `@abstractmethod`.)
    fn in_function_overload_or_abstractmethod(&self) -> bool {
        let current_scope_id = self.scope().file_scope_id(self.db());
        let current_scope = self.index.scope(current_scope_id);

        let function_scope = match current_scope.kind() {
            ScopeKind::Function => current_scope,
            _ => return false,
        };

        let NodeWithScopeKind::Function(node_ref) = function_scope.node() else {
            return false;
        };

        node_ref.decorator_list.iter().any(|decorator| {
            let decorator_type = self.file_expression_type(&decorator.expression);

            match decorator_type {
                Type::FunctionLiteral(function) => matches!(
                    function.known(self.db()),
                    Some(KnownFunction::Overload | KnownFunction::AbstractMethod)
                ),
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

        if let Some(declared_ty) = function
            .returns
            .as_deref()
            .map(|ret| self.file_expression_type(ret))
        {
            fn is_stub_suite(suite: &[ast::Stmt]) -> bool {
                match suite {
                    [ast::Stmt::Expr(ast::StmtExpr { value: first, .. }), ast::Stmt::Expr(ast::StmtExpr { value: second, .. }), ..] => {
                        first.is_string_literal_expr() && second.is_ellipsis_literal_expr()
                    }
                    [ast::Stmt::Expr(ast::StmtExpr { value, .. }), ast::Stmt::Pass(_), ..] => {
                        value.is_string_literal_expr()
                    }
                    [ast::Stmt::Expr(ast::StmtExpr { value, .. }), ..] => {
                        value.is_ellipsis_literal_expr() || value.is_string_literal_expr()
                    }
                    [ast::Stmt::Pass(_)] => true,
                    _ => false,
                }
            }

            if (self.in_stub()
                || self.in_function_overload_or_abstractmethod()
                || self.in_protocol_class())
                && self.return_types_and_ranges.is_empty()
                && is_stub_suite(&function.body)
            {
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
                .filter(|ty_range| !ty_range.ty.is_assignable_to(self.db(), declared_ty))
            {
                report_invalid_return_type(
                    &self.context,
                    invalid.range,
                    function.returns.as_ref().unwrap().range(),
                    declared_ty,
                    invalid.ty,
                );
            }
            let scope_id = self.index.node_scope(NodeWithScopeRef::Function(function));
            let use_def = self.index.use_def_map(scope_id);
            if use_def.can_implicit_return(self.db())
                && !KnownClass::NoneType
                    .to_instance(self.db())
                    .is_assignable_to(self.db(), declared_ty)
            {
                report_implicit_return_type(
                    &self.context,
                    function.returns.as_ref().unwrap().range(),
                    declared_ty,
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
            ast::Stmt::Expr(ast::StmtExpr { range: _, value }) => {
                self.infer_expression(value);
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
            ast::Stmt::Break(_)
            | ast::Stmt::Continue(_)
            | ast::Stmt::Pass(_)
            | ast::Stmt::IpyEscapeCommand(_)
            | ast::Stmt::Global(_)
            | ast::Stmt::Nonlocal(_) => {
                // No-op
            }
        }
    }

    fn infer_definition(&mut self, node: impl Into<DefinitionNodeKey> + std::fmt::Debug + Copy) {
        let definition = self.index.expect_single_definition(node);
        let result = infer_definition_types(self.db(), definition);
        self.extend(result);
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
        let mut dataclass_transformer_params = None;

        for decorator in decorator_list {
            let decorator_ty = self.infer_decorator(decorator);

            match decorator_ty {
                Type::FunctionLiteral(function) => {
                    match function.known(self.db()) {
                        Some(KnownFunction::NoTypeCheck) => {
                            // If the function is decorated with the `no_type_check` decorator,
                            // we need to suppress any errors that come after the decorators.
                            self.context.set_in_no_type_check(InNoTypeCheck::Yes);
                            function_decorators |= FunctionDecorators::NO_TYPE_CHECK;
                            continue;
                        }
                        Some(KnownFunction::Overload) => {
                            function_decorators |= FunctionDecorators::OVERLOAD;
                            continue;
                        }
                        Some(KnownFunction::AbstractMethod) => {
                            function_decorators |= FunctionDecorators::ABSTRACT_METHOD;
                            continue;
                        }
                        Some(KnownFunction::Final) => {
                            function_decorators |= FunctionDecorators::FINAL;
                            continue;
                        }
                        Some(KnownFunction::Override) => {
                            function_decorators |= FunctionDecorators::OVERRIDE;
                            continue;
                        }
                        _ => {}
                    }
                }
                Type::ClassLiteral(class) => {
                    if class.is_known(self.db(), KnownClass::Classmethod) {
                        function_decorators |= FunctionDecorators::CLASSMETHOD;
                        continue;
                    }
                }
                Type::DataclassTransformer(params) => {
                    dataclass_transformer_params = Some(params);
                }
                _ => {}
            }

            decorator_types_and_nodes.push((decorator_ty, decorator));
        }

        for default in parameters
            .iter_non_variadic_params()
            .filter_map(|param| param.default.as_deref())
        {
            self.infer_expression(default);
        }

        // If there are type params, parameters and returns are evaluated in that scope, that is, in
        // `infer_function_type_params`, rather than here.
        if type_params.is_none() {
            if self.defer_annotations() {
                self.types.deferred.insert(definition);
            } else {
                self.infer_optional_annotation_expression(
                    returns.as_deref(),
                    DeferredExpressionState::None,
                );
                self.infer_parameters(parameters);
            }
        }

        let function_kind =
            KnownFunction::try_from_definition_and_name(self.db(), definition, name);

        let body_scope = self
            .index
            .node_scope(NodeWithScopeRef::Function(function))
            .to_scope_id(self.db(), self.file());

        let inherited_generic_context = None;
        let specialization = None;

        let mut inferred_ty = Type::FunctionLiteral(FunctionType::new(
            self.db(),
            &name.id,
            function_kind,
            body_scope,
            function_decorators,
            dataclass_transformer_params,
            inherited_generic_context,
            specialization,
        ));

        for (decorator_ty, decorator_node) in decorator_types_and_nodes.iter().rev() {
            inferred_ty = match decorator_ty
                .try_call(self.db(), CallArgumentTypes::positional([inferred_ty]))
                .map(|bindings| bindings.return_type(self.db()))
            {
                Ok(return_ty) => return_ty,
                Err(CallError(_, bindings)) => {
                    bindings.report_diagnostics(&self.context, (*decorator_node).into());
                    bindings.return_type(self.db())
                }
            };
        }

        self.add_declaration_with_binding(
            function.into(),
            definition,
            &DeclaredAndInferredType::AreTheSame(inferred_ty),
        );
    }

    fn infer_parameters(&mut self, parameters: &ast::Parameters) {
        let ast::Parameters {
            range: _,
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
            parameter,
            default: _,
        } = parameter_with_default;

        self.infer_optional_annotation_expression(
            parameter.annotation.as_deref(),
            DeferredExpressionState::None,
        );
    }

    fn infer_parameter(&mut self, parameter: &ast::Parameter) {
        let ast::Parameter {
            range: _,
            name: _,
            annotation,
        } = parameter;

        self.infer_optional_annotation_expression(
            annotation.as_deref(),
            DeferredExpressionState::None,
        );
    }

    /// Set initial declared type (if annotated) and inferred type for a function-parameter symbol,
    /// in the function body scope.
    ///
    /// The declared type is the annotated type, if any, or `Unknown`.
    ///
    /// The inferred type is the annotated type, unioned with the type of the default value, if
    /// any. If both types are fully static, this union is a no-op (it should simplify to just the
    /// annotated type.) But in a case like `f(x=None)` with no annotated type, we want to infer
    /// the type `Unknown | None` for `x`, not just `Unknown`, so that we can error on usage of `x`
    /// that would not be valid for `None`.
    ///
    /// If the default-value type is not assignable to the declared (annotated) type, we ignore the
    /// default-value type and just infer the annotated type; this is the same way we handle
    /// assignments, and allows an explicit annotation to override a bad inference.
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
        } = parameter_with_default;
        let default_ty = default
            .as_ref()
            .map(|default| self.file_expression_type(default));
        if let Some(annotation) = parameter.annotation.as_ref() {
            let declared_ty = self.file_expression_type(annotation);
            let declared_and_inferred_ty = if let Some(default_ty) = default_ty {
                if default_ty.is_assignable_to(self.db(), declared_ty) {
                    DeclaredAndInferredType::MightBeDifferent {
                        declared_ty: declared_ty.into(),
                        inferred_ty: UnionType::from_elements(self.db(), [declared_ty, default_ty]),
                    }
                } else if (self.in_stub()
                    || self.in_function_overload_or_abstractmethod()
                    || self.in_protocol_class())
                    && default
                        .as_ref()
                        .is_some_and(|d| d.is_ellipsis_literal_expr())
                {
                    DeclaredAndInferredType::AreTheSame(declared_ty)
                } else {
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
                    DeclaredAndInferredType::AreTheSame(declared_ty)
                }
            } else {
                DeclaredAndInferredType::AreTheSame(declared_ty)
            };
            self.add_declaration_with_binding(
                parameter.into(),
                definition,
                &declared_and_inferred_ty,
            );
        } else {
            let ty = if let Some(default_ty) = default_ty {
                UnionType::from_elements(self.db(), [Type::unknown(), default_ty])
            } else {
                Type::unknown()
            };
            self.add_binding(parameter.into(), definition, ty);
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
            let _annotated_ty = self.file_expression_type(annotation);
            // TODO `tuple[annotated_type, ...]`
            let ty = KnownClass::Tuple.to_instance(self.db());
            self.add_declaration_with_binding(
                parameter.into(),
                definition,
                &DeclaredAndInferredType::AreTheSame(ty),
            );
        } else {
            self.add_binding(
                parameter.into(),
                definition,
                // TODO `tuple[Unknown, ...]`
                KnownClass::Tuple.to_instance(self.db()),
            );
        }
    }

    /// Set initial declared/inferred types for a `*args` variadic positional parameter.
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
            let _annotated_ty = self.file_expression_type(annotation);
            // TODO `dict[str, annotated_type]`
            let ty = KnownClass::Dict.to_instance(self.db());
            self.add_declaration_with_binding(
                parameter.into(),
                definition,
                &DeclaredAndInferredType::AreTheSame(ty),
            );
        } else {
            self.add_binding(
                parameter.into(),
                definition,
                // TODO `dict[str, Unknown]`
                KnownClass::Dict.to_instance(self.db()),
            );
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
            name,
            type_params,
            decorator_list,
            arguments: _,
            body: _,
        } = class_node;

        let mut dataclass_params = None;
        let mut dataclass_transformer_params = None;
        for decorator in decorator_list {
            let decorator_ty = self.infer_decorator(decorator);
            if decorator_ty
                .into_function_literal()
                .is_some_and(|function| function.is_known(self.db(), KnownFunction::Dataclass))
            {
                dataclass_params = Some(DataclassParams::default());
                continue;
            }

            if let Type::DataclassDecorator(params) = decorator_ty {
                dataclass_params = Some(params);
                continue;
            }

            if let Type::FunctionLiteral(f) = decorator_ty {
                if let Some(params) = f.dataclass_transformer_params(self.db()) {
                    dataclass_params = Some(params.into());
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

        let class_ty = Type::from(ClassLiteral::new(
            self.db(),
            name.id.clone(),
            body_scope,
            maybe_known_class,
            dataclass_params,
            dataclass_transformer_params,
        ));

        self.add_declaration_with_binding(
            class_node.into(),
            definition,
            &DeclaredAndInferredType::AreTheSame(class_ty),
        );

        // if there are type parameters, then the keywords and bases are within that scope
        // and we don't need to run inference here
        if type_params.is_none() {
            for keyword in class_node.keywords() {
                self.infer_expression(&keyword.value);
            }

            // Inference of bases deferred in stubs
            // TODO: Only defer the references that are actually string literals, instead of
            // deferring the entire class definition if a string literal occurs anywhere in the
            // base class list.
            if self.in_stub() || class_node.bases().iter().any(contains_string_literal) {
                self.types.deferred.insert(definition);
            } else {
                for base in class_node.bases() {
                    self.infer_expression(base);
                }
            }
        }
    }

    fn infer_function_deferred(&mut self, function: &ast::StmtFunctionDef) {
        self.infer_optional_annotation_expression(
            function.returns.as_deref(),
            DeferredExpressionState::Deferred,
        );
        self.infer_parameters(function.parameters.as_ref());
    }

    fn infer_class_deferred(&mut self, class: &ast::StmtClassDef) {
        for base in class.bases() {
            self.infer_expression(base);
        }
    }

    fn infer_type_alias_definition(
        &mut self,
        type_alias: &ast::StmtTypeAlias,
        definition: Definition<'db>,
    ) {
        self.infer_expression(&type_alias.name);

        let rhs_scope = self
            .index
            .node_scope(NodeWithScopeRef::TypeAlias(type_alias))
            .to_scope_id(self.db(), self.file());

        let type_alias_ty =
            Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::new(
                self.db(),
                &type_alias.name.as_name_expr().unwrap().id,
                rhs_scope,
            )));

        self.add_declaration_with_binding(
            type_alias.into(),
            definition,
            &DeclaredAndInferredType::AreTheSame(type_alias_ty),
        );
    }

    fn infer_if_statement(&mut self, if_statement: &ast::StmtIf) {
        let ast::StmtIf {
            range: _,
            test,
            body,
            elif_else_clauses,
        } = if_statement;

        let test_ty = self.infer_standalone_expression(test);

        if let Err(err) = test_ty.try_bool(self.db()) {
            err.report_diagnostic(&self.context, &**test);
        }

        self.infer_body(body);

        for clause in elif_else_clauses {
            let ast::ElifElseClause {
                range: _,
                test,
                body,
            } = clause;

            if let Some(test) = &test {
                let test_ty = self.infer_standalone_expression(test);

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
            } = handler;

            // If `symbol_name` is `Some()` and `handled_exceptions` is `None`,
            // it's invalid syntax (something like `except as e:`).
            // However, it's obvious that the user *wanted* `e` to be bound here,
            // so we'll have created a definition in the semantic-index stage anyway.
            if symbol_name.is_some() {
                self.infer_definition(handler);
            } else {
                self.infer_optional_expression(handled_exceptions.as_deref());
            }

            self.infer_body(body);
        }

        self.infer_body(orelse);
        self.infer_body(finalbody);
    }

    fn infer_with_statement(&mut self, with_statement: &ast::StmtWith) {
        let ast::StmtWith {
            range: _,
            is_async,
            items,
            body,
        } = with_statement;
        for item in items {
            let target = item.optional_vars.as_deref();
            if let Some(target) = target {
                self.infer_target(target, &item.context_expr, |builder, context_expr| {
                    // TODO: `infer_with_statement_definition` reports a diagnostic if `ctx_manager_ty` isn't a context manager
                    //  but only if the target is a name. We should report a diagnostic here if the target isn't a name:
                    //  `with not_context_manager as a.x: ...
                    builder
                        .infer_standalone_expression(context_expr)
                        .enter(builder.db())
                });
            } else {
                // Call into the context expression inference to validate that it evaluates
                // to a valid context manager.
                let context_expression_ty = self.infer_expression(&item.context_expr);
                self.infer_context_expression(&item.context_expr, context_expression_ty, *is_async);
                self.infer_optional_expression(target);
            }
        }

        self.infer_body(body);
    }

    fn infer_with_item_definition(
        &mut self,
        with_item: &WithItemDefinitionKind<'db>,
        definition: Definition<'db>,
    ) {
        let context_expr = with_item.context_expr();
        let target = with_item.target();

        let context_expr_ty = self.infer_standalone_expression(context_expr);

        let target_ty = if with_item.is_async() {
            todo_type!("async `with` statement")
        } else {
            match with_item.target_kind() {
                TargetKind::Sequence(unpack_position, unpack) => {
                    let unpacked = infer_unpack_types(self.db(), unpack);
                    let target_ast_id = target.scoped_expression_id(self.db(), self.scope());
                    if unpack_position == UnpackPosition::First {
                        self.context.extend(unpacked.diagnostics());
                    }
                    unpacked.expression_type(target_ast_id)
                }
                TargetKind::NameOrAttribute => self.infer_context_expression(
                    context_expr,
                    context_expr_ty,
                    with_item.is_async(),
                ),
            }
        };

        self.store_expression_type(target, target_ty);
        self.add_binding(target.into(), definition, target_ty);
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
        // TODO: Handle async with statements (they use `aenter` and `aexit`)
        if is_async {
            return todo_type!("async `with` statement");
        }

        context_expression_type
            .try_enter(self.db())
            .unwrap_or_else(|err| {
                err.report_diagnostic(
                    &self.context,
                    context_expression_type,
                    context_expression.into(),
                );
                err.fallback_enter_type(self.db())
            })
    }

    fn infer_except_handler_definition(
        &mut self,
        except_handler_definition: &ExceptHandlerDefinitionKind,
        definition: Definition<'db>,
    ) {
        let node = except_handler_definition.handled_exceptions();

        // If there is no handled exception, it's invalid syntax;
        // a diagnostic will have already been emitted
        let node_ty = node.map_or(Type::unknown(), |ty| self.infer_expression(ty));

        // If it's an `except*` handler, this won't actually be the type of the bound symbol;
        // it will actually be the type of the generic parameters to `BaseExceptionGroup` or `ExceptionGroup`.
        let symbol_ty = if let Type::Tuple(tuple) = node_ty {
            let type_base_exception = KnownClass::BaseException.to_subclass_of(self.db());
            let mut builder = UnionBuilder::new(self.db());
            for element in tuple.elements(self.db()).iter().copied() {
                builder = builder.add(
                    if element.is_assignable_to(self.db(), type_base_exception) {
                        element.to_instance(self.db()).expect(
                            "`Type::to_instance()` should always return `Some()` \
                                if called on a type assignable to `type[BaseException]`",
                        )
                    } else {
                        if let Some(node) = node {
                            report_invalid_exception_caught(&self.context, node, element);
                        }
                        Type::unknown()
                    },
                );
            }
            builder.build()
        } else if node_ty.is_subtype_of(self.db(), KnownClass::Tuple.to_instance(self.db())) {
            todo_type!("Homogeneous tuple in exception handler")
        } else {
            let type_base_exception = KnownClass::BaseException.to_subclass_of(self.db());
            if node_ty.is_assignable_to(self.db(), type_base_exception) {
                node_ty.to_instance(self.db()).expect(
                    "`Type::to_instance()` should always return `Some()` \
                        if called on a type assignable to `type[BaseException]`",
                )
            } else {
                if let Some(node) = node {
                    report_invalid_exception_caught(&self.context, node, node_ty);
                }
                Type::unknown()
            }
        };

        let symbol_ty = if except_handler_definition.is_star() {
            // TODO: we should infer `ExceptionGroup` if `node_ty` is a subtype of `tuple[type[Exception], ...]`
            // (needs support for homogeneous tuples).
            //
            // TODO: should be generic with `symbol_ty` as the generic parameter
            KnownClass::BaseExceptionGroup.to_instance(self.db())
        } else {
            symbol_ty
        };

        self.add_binding(
            except_handler_definition.node().into(),
            definition,
            symbol_ty,
        );
    }

    fn infer_typevar_definition(
        &mut self,
        node: &ast::TypeParamTypeVar,
        definition: Definition<'db>,
    ) {
        let ast::TypeParamTypeVar {
            range: _,
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
                    self.infer_expression(expr);
                    None
                } else {
                    // We don't use UnionType::from_elements or UnionBuilder here, because we don't
                    // want to simplify the list of constraints like we do with the elements of an
                    // actual union type.
                    // TODO: Consider using a new `OneOfType` connective here instead, since that
                    // more accurately represents the actual semantics of typevar constraints.
                    let elements = UnionType::new(
                        self.db(),
                        elts.iter()
                            .map(|expr| self.infer_type_expression(expr))
                            .collect::<Box<[_]>>(),
                    );
                    let constraints = TypeVarBoundOrConstraints::Constraints(elements);
                    // But when we construct an actual union type for the constraint expression as
                    // a whole, we do use UnionType::from_elements to maintain the invariant that
                    // all union types are simplified.
                    self.store_expression_type(
                        expr,
                        UnionType::from_elements(self.db(), elements.elements(self.db())),
                    );
                    Some(constraints)
                }
            }
            Some(expr) => Some(TypeVarBoundOrConstraints::UpperBound(
                self.infer_type_expression(expr),
            )),
            None => None,
        };
        let default_ty = self.infer_optional_type_expression(default.as_deref());
        let ty = Type::KnownInstance(KnownInstanceType::TypeVar(TypeVarInstance::new(
            self.db(),
            name.id.clone(),
            definition,
            bound_or_constraint,
            default_ty,
            TypeVarKind::Pep695,
        )));
        self.add_declaration_with_binding(
            node.into(),
            definition,
            &DeclaredAndInferredType::AreTheSame(ty),
        );
    }

    fn infer_paramspec_definition(
        &mut self,
        node: &ast::TypeParamParamSpec,
        definition: Definition<'db>,
    ) {
        let ast::TypeParamParamSpec {
            range: _,
            name: _,
            default,
        } = node;
        self.infer_optional_expression(default.as_deref());
        let pep_695_todo = todo_type!("PEP-695 ParamSpec definition types");
        self.add_declaration_with_binding(
            node.into(),
            definition,
            &DeclaredAndInferredType::AreTheSame(pep_695_todo),
        );
    }

    fn infer_typevartuple_definition(
        &mut self,
        node: &ast::TypeParamTypeVarTuple,
        definition: Definition<'db>,
    ) {
        let ast::TypeParamTypeVarTuple {
            range: _,
            name: _,
            default,
        } = node;
        self.infer_optional_expression(default.as_deref());
        let pep_695_todo = todo_type!("PEP-695 TypeVarTuple definition types");
        self.add_declaration_with_binding(
            node.into(),
            definition,
            &DeclaredAndInferredType::AreTheSame(pep_695_todo),
        );
    }

    fn infer_match_statement(&mut self, match_statement: &ast::StmtMatch) {
        let ast::StmtMatch {
            range: _,
            subject,
            cases,
        } = match_statement;

        self.infer_standalone_expression(subject);

        for case in cases {
            let ast::MatchCase {
                range: _,
                body,
                pattern,
                guard,
            } = case;
            self.infer_match_pattern(pattern);

            if let Some(guard) = guard.as_deref() {
                let guard_ty = self.infer_standalone_expression(guard);

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
        self.add_binding(
            pattern.into(),
            definition,
            todo_type!("`match` pattern definition types"),
        );
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
                self.infer_standalone_expression(&match_value.value);
            }
            ast::Pattern::MatchClass(match_class) => {
                let ast::PatternMatchClass {
                    range: _,
                    cls,
                    arguments,
                } = match_class;
                for pattern in &arguments.patterns {
                    self.infer_nested_match_pattern(pattern);
                }
                for keyword in &arguments.keywords {
                    self.infer_nested_match_pattern(&keyword.pattern);
                }
                self.infer_standalone_expression(cls);
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
                self.infer_expression(&match_value.value);
            }
            ast::Pattern::MatchSequence(match_sequence) => {
                for pattern in &match_sequence.patterns {
                    self.infer_nested_match_pattern(pattern);
                }
            }
            ast::Pattern::MatchMapping(match_mapping) => {
                let ast::PatternMatchMapping {
                    range: _,
                    keys,
                    patterns,
                    rest: _,
                } = match_mapping;
                for key in keys {
                    self.infer_expression(key);
                }
                for pattern in patterns {
                    self.infer_nested_match_pattern(pattern);
                }
            }
            ast::Pattern::MatchClass(match_class) => {
                let ast::PatternMatchClass {
                    range: _,
                    cls,
                    arguments,
                } = match_class;
                for pattern in &arguments.patterns {
                    self.infer_nested_match_pattern(pattern);
                }
                for keyword in &arguments.keywords {
                    self.infer_nested_match_pattern(&keyword.pattern);
                }
                self.infer_expression(cls);
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
            targets,
            value,
        } = assignment;

        for target in targets {
            self.infer_target(target, value, |builder, value_expr| {
                builder.infer_standalone_expression(value_expr)
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
        F: Fn(&mut TypeInferenceBuilder<'db>, &ast::Expr) -> Type<'db>,
    {
        let assigned_ty = match target {
            ast::Expr::Name(_) => None,
            _ => Some(infer_value_expr(self, value)),
        };
        self.infer_target_impl(target, assigned_ty);
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
        value_ty: Type<'db>,
        emit_diagnostics: bool,
    ) -> bool {
        let db = self.db();

        let ensure_assignable_to = |attr_ty| -> bool {
            let assignable = value_ty.is_assignable_to(db, attr_ty);
            if !assignable && emit_diagnostics {
                report_invalid_attribute_assignment(
                    &self.context,
                    target.into(),
                    attr_ty,
                    value_ty,
                    attribute,
                );
            }
            assignable
        };

        match object_ty {
            Type::Union(union) => {
                if union.elements(self.db()).iter().all(|elem| {
                    self.validate_attribute_assignment(target, *elem, attribute, value_ty, false)
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
                // TODO: Handle negative intersection elements
                if intersection.positive(db).iter().any(|elem| {
                    self.validate_attribute_assignment(target, *elem, attribute, value_ty, false)
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

            // Super instances do not allow attribute assignment
            Type::NominalInstance(instance) if instance.class().is_known(db, KnownClass::Super) => {
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

            Type::Dynamic(..) | Type::Never => true,

            Type::NominalInstance(..)
            | Type::ProtocolInstance(_)
            | Type::BooleanLiteral(..)
            | Type::IntLiteral(..)
            | Type::StringLiteral(..)
            | Type::BytesLiteral(..)
            | Type::LiteralString
            | Type::SliceLiteral(..)
            | Type::Tuple(..)
            | Type::KnownInstance(..)
            | Type::PropertyInstance(..)
            | Type::FunctionLiteral(..)
            | Type::Callable(..)
            | Type::BoundMethod(_)
            | Type::MethodWrapper(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::TypeVar(..)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy => {
                match object_ty.class_member(db, attribute.into()) {
                    meta_attr @ SymbolAndQualifiers { .. } if meta_attr.is_class_var() => {
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
                    SymbolAndQualifiers {
                        symbol: Symbol::Type(meta_attr_ty, meta_attr_boundness),
                        qualifiers: _,
                    } => {
                        let assignable_to_meta_attr = if let Symbol::Type(meta_dunder_set, _) =
                            meta_attr_ty.class_member(db, "__set__".into()).symbol
                        {
                            let successful_call = meta_dunder_set
                                .try_call(
                                    db,
                                    CallArgumentTypes::positional([
                                        meta_attr_ty,
                                        object_ty,
                                        value_ty,
                                    ]),
                                )
                                .is_ok();

                            if !successful_call && emit_diagnostics {
                                if let Some(builder) =
                                    self.context.report_lint(&INVALID_ASSIGNMENT, target)
                                {
                                    // TODO: Here, it would be nice to emit an additional diagnostic that explains why the call failed
                                    builder.into_diagnostic(format_args!(
                                        "Invalid assignment to data descriptor attribute \
                                         `{attribute}` on type `{}` with custom `__set__` method",
                                        object_ty.display(db)
                                    ));
                                }
                            }

                            successful_call
                        } else {
                            ensure_assignable_to(meta_attr_ty)
                        };

                        let assignable_to_instance_attribute = if meta_attr_boundness
                            == Boundness::PossiblyUnbound
                        {
                            let (assignable, boundness) =
                                if let Symbol::Type(instance_attr_ty, instance_attr_boundness) =
                                    object_ty.instance_member(db, attribute).symbol
                                {
                                    (
                                        ensure_assignable_to(instance_attr_ty),
                                        instance_attr_boundness,
                                    )
                                } else {
                                    (true, Boundness::PossiblyUnbound)
                                };

                            if boundness == Boundness::PossiblyUnbound {
                                report_possibly_unbound_attribute(
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

                    SymbolAndQualifiers {
                        symbol: Symbol::Unbound,
                        ..
                    } => {
                        if let Symbol::Type(instance_attr_ty, instance_attr_boundness) =
                            object_ty.instance_member(db, attribute).symbol
                        {
                            if instance_attr_boundness == Boundness::PossiblyUnbound {
                                report_possibly_unbound_attribute(
                                    &self.context,
                                    target,
                                    attribute,
                                    object_ty,
                                );
                            }

                            ensure_assignable_to(instance_attr_ty)
                        } else {
                            let result = object_ty.try_call_dunder_with_policy(
                                db,
                                "__setattr__",
                                &mut CallArgumentTypes::positional([
                                    Type::StringLiteral(StringLiteralType::new(
                                        db,
                                        Box::from(attribute),
                                    )),
                                    value_ty,
                                ]),
                                MemberLookupPolicy::NO_INSTANCE_FALLBACK
                                    | MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                            );

                            match result {
                                Ok(_) | Err(CallDunderError::PossiblyUnbound(_)) => true,
                                Err(CallDunderError::CallError(..)) => {
                                    if emit_diagnostics {
                                        if let Some(builder) =
                                            self.context.report_lint(&UNRESOLVED_ATTRIBUTE, target)
                                        {
                                            builder.into_diagnostic(format_args!(
                                                "Can not assign object of `{}` to attribute \
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
                    SymbolAndQualifiers {
                        symbol: Symbol::Type(meta_attr_ty, meta_attr_boundness),
                        qualifiers: _,
                    } => {
                        let assignable_to_meta_attr = if let Symbol::Type(meta_dunder_set, _) =
                            meta_attr_ty.class_member(db, "__set__".into()).symbol
                        {
                            let successful_call = meta_dunder_set
                                .try_call(
                                    db,
                                    CallArgumentTypes::positional([
                                        meta_attr_ty,
                                        object_ty,
                                        value_ty,
                                    ]),
                                )
                                .is_ok();

                            if !successful_call && emit_diagnostics {
                                if let Some(builder) =
                                    self.context.report_lint(&INVALID_ASSIGNMENT, target)
                                {
                                    // TODO: Here, it would be nice to emit an additional diagnostic that explains why the call failed
                                    builder.into_diagnostic(format_args!(
                                        "Invalid assignment to data descriptor attribute \
                                         `{attribute}` on type `{}` with custom `__set__` method",
                                        object_ty.display(db)
                                    ));
                                }
                            }

                            successful_call
                        } else {
                            ensure_assignable_to(meta_attr_ty)
                        };

                        let assignable_to_class_attr = if meta_attr_boundness
                            == Boundness::PossiblyUnbound
                        {
                            let (assignable, boundness) = if let Symbol::Type(
                                class_attr_ty,
                                class_attr_boundness,
                            ) = object_ty
                                .find_name_in_mro(db, attribute)
                                .expect("called on Type::ClassLiteral or Type::SubclassOf")
                                .symbol
                            {
                                (ensure_assignable_to(class_attr_ty), class_attr_boundness)
                            } else {
                                (true, Boundness::PossiblyUnbound)
                            };

                            if boundness == Boundness::PossiblyUnbound {
                                report_possibly_unbound_attribute(
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
                    SymbolAndQualifiers {
                        symbol: Symbol::Unbound,
                        ..
                    } => {
                        if let Symbol::Type(class_attr_ty, class_attr_boundness) = object_ty
                            .find_name_in_mro(db, attribute)
                            .expect("called on Type::ClassLiteral or Type::SubclassOf")
                            .symbol
                        {
                            if class_attr_boundness == Boundness::PossiblyUnbound {
                                report_possibly_unbound_attribute(
                                    &self.context,
                                    target,
                                    attribute,
                                    object_ty,
                                );
                            }

                            ensure_assignable_to(class_attr_ty)
                        } else {
                            let attribute_is_bound_on_instance =
                                object_ty.to_instance(self.db()).is_some_and(|instance| {
                                    !instance
                                        .instance_member(self.db(), attribute)
                                        .symbol
                                        .is_unbound()
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
                if let Symbol::Type(attr_ty, _) = module.static_member(db, attribute) {
                    let assignable = value_ty.is_assignable_to(db, attr_ty);
                    if !assignable {
                        report_invalid_attribute_assignment(
                            &self.context,
                            target.into(),
                            attr_ty,
                            value_ty,
                            attribute,
                        );
                    }

                    false
                } else {
                    if let Some(builder) = self.context.report_lint(&UNRESOLVED_ATTRIBUTE, target) {
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

    fn infer_target_impl(&mut self, target: &ast::Expr, assigned_ty: Option<Type<'db>>) {
        match target {
            ast::Expr::Name(name) => self.infer_definition(name),
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                let mut assigned_tys = match assigned_ty {
                    Some(Type::Tuple(tuple)) => {
                        Either::Left(tuple.elements(self.db()).into_iter().copied())
                    }
                    Some(_) | None => Either::Right(std::iter::empty()),
                };

                for element in elts {
                    self.infer_target_impl(element, assigned_tys.next());
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
                self.store_expression_type(target, Type::Never);

                let object_ty = self.infer_expression(object);

                if let Some(assigned_ty) = assigned_ty {
                    self.validate_attribute_assignment(
                        attr_expr,
                        object_ty,
                        attr.id(),
                        assigned_ty,
                        true,
                    );
                }
            }
            _ => {
                // TODO: Remove this once we handle all possible assignment targets.
                self.infer_expression(target);
            }
        }
    }

    fn infer_assignment_definition(
        &mut self,
        assignment: &AssignmentDefinitionKind<'db>,
        definition: Definition<'db>,
    ) {
        let value = assignment.value();
        let target = assignment.target();

        let value_ty = self.infer_standalone_expression(value);

        let mut target_ty = match assignment.target_kind() {
            TargetKind::Sequence(unpack_position, unpack) => {
                let unpacked = infer_unpack_types(self.db(), unpack);
                // Only copy the diagnostics if this is the first assignment to avoid duplicating the
                // unpack assignments.
                if unpack_position == UnpackPosition::First {
                    self.context.extend(unpacked.diagnostics());
                }

                let target_ast_id = target.scoped_expression_id(self.db(), self.scope());
                unpacked.expression_type(target_ast_id)
            }
            TargetKind::NameOrAttribute => {
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

        if let Some(known_instance) = target.as_name_expr().and_then(|name| {
            KnownInstanceType::try_from_file_and_name(self.db(), self.file(), &name.id)
        }) {
            target_ty = Type::KnownInstance(known_instance);
        }

        self.store_expression_type(target, target_ty);
        self.add_binding(target.into(), definition, target_ty);
    }

    fn infer_annotated_assignment_statement(&mut self, assignment: &ast::StmtAnnAssign) {
        // assignments to non-Names are not Definitions
        if matches!(*assignment.target, ast::Expr::Name(_)) {
            self.infer_definition(assignment);
        } else {
            let ast::StmtAnnAssign {
                range: _,
                annotation,
                value,
                target,
                simple: _,
            } = assignment;
            self.infer_annotation_expression(annotation, DeferredExpressionState::None);
            self.infer_optional_expression(value.as_deref());
            self.infer_expression(target);
        }
    }

    /// Infer the types in an annotated assignment definition.
    fn infer_annotated_assignment_definition(
        &mut self,
        assignment: &'db AnnotatedAssignmentDefinitionKind,
        definition: Definition<'db>,
    ) {
        let annotation = assignment.annotation();
        let target = assignment.target();
        let value = assignment.value();

        let mut declared_ty = self.infer_annotation_expression(
            annotation,
            DeferredExpressionState::from(self.defer_annotations()),
        );

        if target
            .as_name_expr()
            .is_some_and(|name| &name.id == "TYPE_CHECKING")
        {
            if !KnownClass::Bool
                .to_instance(self.db())
                .is_assignable_to(self.db(), declared_ty.inner_type())
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
            declared_ty.inner = Type::BooleanLiteral(true);
        }

        // Handle various singletons.
        if let Type::NominalInstance(instance) = declared_ty.inner_type() {
            if instance
                .class()
                .is_known(self.db(), KnownClass::SpecialForm)
            {
                if let Some(name_expr) = target.as_name_expr() {
                    if let Some(known_instance) = KnownInstanceType::try_from_file_and_name(
                        self.db(),
                        self.file(),
                        &name_expr.id,
                    ) {
                        declared_ty.inner = Type::KnownInstance(known_instance);
                    }
                }
            }
        }

        if let Some(value) = value {
            let inferred_ty = self.infer_expression(value);
            let inferred_ty = if target
                .as_name_expr()
                .is_some_and(|name| &name.id == "TYPE_CHECKING")
            {
                Type::BooleanLiteral(true)
            } else if self.in_stub() && value.is_ellipsis_literal_expr() {
                declared_ty.inner_type()
            } else {
                inferred_ty
            };
            self.add_declaration_with_binding(
                target.into(),
                definition,
                &DeclaredAndInferredType::MightBeDifferent {
                    declared_ty,
                    inferred_ty,
                },
            );
        } else {
            if self.in_stub() {
                self.add_declaration_with_binding(
                    target.into(),
                    definition,
                    &DeclaredAndInferredType::AreTheSame(declared_ty.inner_type()),
                );
            } else {
                self.add_declaration(target.into(), definition, declared_ty);
            }
        }

        self.infer_expression(target);
    }

    fn infer_augmented_assignment_statement(&mut self, assignment: &ast::StmtAugAssign) {
        if assignment.target.is_name_expr() {
            self.infer_definition(assignment);
        } else {
            // TODO currently we don't consider assignments to non-Names to be Definitions
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
                    CallArgumentTypes::positional([value_type]),
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
        self.add_binding(assignment.into(), definition, target_ty);
    }

    fn infer_augment_assignment(&mut self, assignment: &ast::StmtAugAssign) -> Type<'db> {
        let ast::StmtAugAssign {
            range: _,
            target,
            op: _,
            value,
        } = assignment;

        // Resolve the target type, assuming a load context.
        let target_type = match &**target {
            ast::Expr::Name(name) => {
                self.store_expression_type(target, Type::Never);
                self.infer_name_load(name)
            }
            ast::Expr::Attribute(attr) => {
                self.store_expression_type(target, Type::Never);
                self.infer_attribute_load(attr)
            }
            _ => self.infer_expression(target),
        };
        let value_type = self.infer_expression(value);

        self.infer_augmented_op(assignment, target_type, value_type)
    }

    fn infer_type_alias_statement(&mut self, node: &ast::StmtTypeAlias) {
        self.infer_definition(node);
    }

    fn infer_for_statement(&mut self, for_statement: &ast::StmtFor) {
        let ast::StmtFor {
            range: _,
            target,
            iter,
            body,
            orelse,
            is_async: _,
        } = for_statement;

        self.infer_target(target, iter, |builder, iter_expr| {
            // TODO: `infer_for_statement_definition` reports a diagnostic if `iter_ty` isn't iterable
            //  but only if the target is a name. We should report a diagnostic here if the target isn't a name:
            //  `for a.x in not_iterable: ...
            builder
                .infer_standalone_expression(iter_expr)
                .iterate(builder.db())
        });

        self.infer_body(body);
        self.infer_body(orelse);
    }

    fn infer_for_statement_definition(
        &mut self,
        for_stmt: &ForStmtDefinitionKind<'db>,
        definition: Definition<'db>,
    ) {
        let iterable = for_stmt.iterable();
        let target = for_stmt.target();

        let iterable_type = self.infer_standalone_expression(iterable);

        let loop_var_value_type = if for_stmt.is_async() {
            todo_type!("async iterables/iterators")
        } else {
            match for_stmt.target_kind() {
                TargetKind::Sequence(unpack_position, unpack) => {
                    let unpacked = infer_unpack_types(self.db(), unpack);
                    if unpack_position == UnpackPosition::First {
                        self.context.extend(unpacked.diagnostics());
                    }
                    let target_ast_id = target.scoped_expression_id(self.db(), self.scope());
                    unpacked.expression_type(target_ast_id)
                }
                TargetKind::NameOrAttribute => {
                    iterable_type.try_iterate(self.db()).unwrap_or_else(|err| {
                        err.report_diagnostic(&self.context, iterable_type, iterable.into());
                        err.fallback_element_type(self.db())
                    })
                }
            }
        };

        self.store_expression_type(target, loop_var_value_type);
        self.add_binding(target.into(), definition, loop_var_value_type);
    }

    fn infer_while_statement(&mut self, while_statement: &ast::StmtWhile) {
        let ast::StmtWhile {
            range: _,
            test,
            body,
            orelse,
        } = while_statement;

        let test_ty = self.infer_standalone_expression(test);

        if let Err(err) = test_ty.try_bool(self.db()) {
            err.report_diagnostic(&self.context, &**test);
        }

        self.infer_body(body);
        self.infer_body(orelse);
    }

    fn infer_import_statement(&mut self, import: &ast::StmtImport) {
        let ast::StmtImport { range: _, names } = import;

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
        builder.into_diagnostic(format_args!(
            "Cannot resolve import `{}{}`",
            ".".repeat(level as usize),
            module.unwrap_or_default()
        ));
    }

    fn infer_import_definition(
        &mut self,
        node: &ast::StmtImport,
        alias: &'db ast::Alias,
        definition: Definition<'db>,
    ) {
        let ast::Alias {
            range: _,
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
            &DeclaredAndInferredType::AreTheSame(binding_ty),
        );
    }

    fn infer_import_from_statement(&mut self, import: &ast::StmtImportFrom) {
        let ast::StmtImportFrom {
            range: _,
            module: _,
            names,
            level: _,
        } = import;

        for alias in names {
            let definitions = self.index.definitions(alias);
            if definitions.is_empty() {
                // If the module couldn't be resolved while constructing the semantic index,
                // this node won't have any definitions associated with it -- but we need to
                // make sure that we still emit the diagnostic for the unresolvable module,
                // since this will cause the import to fail at runtime.
                self.resolve_import_from_module(import, alias);
            } else {
                for definition in definitions {
                    self.extend(infer_definition_types(self.db(), *definition));
                }
            }
        }
    }

    fn infer_assert_statement(&mut self, assert: &ast::StmtAssert) {
        let ast::StmtAssert {
            range: _,
            test,
            msg,
        } = assert;

        let test_ty = self.infer_standalone_expression(test);

        if let Err(err) = test_ty.try_bool(self.db()) {
            err.report_diagnostic(&self.context, &**test);
        }

        self.infer_optional_expression(msg.as_deref());
    }

    fn infer_raise_statement(&mut self, raise: &ast::StmtRaise) {
        let ast::StmtRaise {
            range: _,
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
            let raised_type = self.infer_expression(raised);

            if !raised_type.is_assignable_to(self.db(), can_be_raised) {
                report_invalid_exception_raised(&self.context, raised, raised_type);
            }
        }

        if let Some(cause) = cause {
            let cause_type = self.infer_expression(cause);

            if !cause_type.is_assignable_to(self.db(), can_be_exception_cause) {
                report_invalid_exception_cause(&self.context, cause, cause_type);
            }
        }
    }

    /// Resolve the [`ModuleName`], and the type of the module, being referred to by an
    /// [`ast::StmtImportFrom`] node. Emit a diagnostic if the module cannot be resolved.
    fn resolve_import_from_module(
        &mut self,
        import_from: &ast::StmtImportFrom,
        alias: &ast::Alias,
    ) -> Option<(ModuleName, Type<'db>)> {
        let ast::StmtImportFrom { module, level, .. } = import_from;

        // For diagnostics, we want to highlight the unresolvable
        // module and not the entire `from ... import ...` statement.
        let module_ref = module
            .as_ref()
            .map(AnyNodeRef::from)
            .unwrap_or_else(|| AnyNodeRef::from(import_from));
        let module = module.as_deref();

        tracing::trace!(
            "Resolving imported object `{}` from module `{}` into file `{}`",
            alias.name,
            format_import_from_module(*level, module),
            self.file().path(self.db()),
        );
        let module_name = ModuleName::from_import_statement(self.db(), self.file(), import_from);

        let module_name = match module_name {
            Ok(module_name) => module_name,
            Err(ModuleNameResolutionError::InvalidSyntax) => {
                tracing::debug!("Failed to resolve import due to invalid syntax");
                // Invalid syntax diagnostics are emitted elsewhere.
                return None;
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
                return None;
            }
            Err(ModuleNameResolutionError::UnknownCurrentModule) => {
                tracing::debug!(
                    "Relative module resolution `{}` failed; could not resolve file `{}` to a module",
                    format_import_from_module(*level, module),
                    self.file().path(self.db())
                );
                self.report_unresolved_import(
                    import_from.into(),
                    module_ref.range(),
                    *level,
                    module,
                );
                return None;
            }
        };

        let Some(module_ty) = self.module_type_from_name(&module_name) else {
            self.report_unresolved_import(import_from.into(), module_ref.range(), *level, module);
            return None;
        };

        Some((module_name, module_ty))
    }

    fn infer_import_from_definition(
        &mut self,
        import_from: &'db ast::StmtImportFrom,
        alias: &ast::Alias,
        definition: Definition<'db>,
    ) {
        let Some((module_name, module_ty)) = self.resolve_import_from_module(import_from, alias)
        else {
            self.add_unknown_declaration_with_binding(alias.into(), definition);
            return;
        };

        // The indirection of having `star_import_info` as a separate variable
        // is required in order to make the borrow checker happy.
        let star_import_info = definition
            .kind(self.db())
            .as_star_import()
            .map(|star_import| {
                let symbol_table = self
                    .index
                    .symbol_table(self.scope().file_scope_id(self.db()));
                (star_import, symbol_table)
            });

        let name = if let Some((star_import, symbol_table)) = star_import_info.as_ref() {
            symbol_table.symbol(star_import.symbol_id()).name()
        } else {
            &alias.name.id
        };

        // First try loading the requested attribute from the module.
        if let Symbol::Type(ty, boundness) = module_ty.member(self.db(), name).symbol {
            if &alias.name != "*" && boundness == Boundness::PossiblyUnbound {
                // TODO: Consider loading _both_ the attribute and any submodule and unioning them
                // together if the attribute exists but is possibly-unbound.
                if let Some(builder) = self
                    .context
                    .report_lint(&POSSIBLY_UNBOUND_IMPORT, AnyNodeRef::Alias(alias))
                {
                    builder.into_diagnostic(format_args!(
                        "Member `{name}` of module `{module_name}` is possibly unbound",
                    ));
                }
            }
            self.add_declaration_with_binding(
                alias.into(),
                definition,
                &DeclaredAndInferredType::AreTheSame(ty),
            );
            return;
        }

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
        if let Some(submodule_name) = ModuleName::new(name) {
            let mut full_submodule_name = module_name.clone();
            full_submodule_name.extend(&submodule_name);
            if let Some(submodule_ty) = self.module_type_from_name(&full_submodule_name) {
                self.add_declaration_with_binding(
                    alias.into(),
                    definition,
                    &DeclaredAndInferredType::AreTheSame(submodule_ty),
                );
                return;
            }
        }

        if &alias.name != "*" {
            let is_import_reachable = self.is_reachable(import_from);

            if is_import_reachable {
                if let Some(builder) = self
                    .context
                    .report_lint(&UNRESOLVED_IMPORT, AnyNodeRef::Alias(alias))
                {
                    builder.into_diagnostic(format_args!(
                        "Module `{module_name}` has no member `{name}`"
                    ));
                }
            }
        }

        self.add_unknown_declaration_with_binding(alias.into(), definition);
    }

    fn infer_return_statement(&mut self, ret: &ast::StmtReturn) {
        if let Some(ty) = self.infer_optional_expression(ret.value.as_deref()) {
            let range = ret
                .value
                .as_ref()
                .map_or(ret.range(), |value| value.range());
            self.record_return_type(ty, range);
        } else {
            self.record_return_type(KnownClass::NoneType.to_instance(self.db()), ret.range());
        }
    }

    fn infer_delete_statement(&mut self, delete: &ast::StmtDelete) {
        let ast::StmtDelete { range: _, targets } = delete;
        for target in targets {
            self.infer_expression(target);
        }
    }

    fn module_type_from_name(&self, module_name: &ModuleName) -> Option<Type<'db>> {
        resolve_module(self.db(), module_name)
            .map(|module| Type::module_literal(self.db(), self.file(), module))
    }

    fn infer_decorator(&mut self, decorator: &ast::Decorator) -> Type<'db> {
        let ast::Decorator {
            range: _,
            expression,
        } = decorator;

        self.infer_expression(expression)
    }

    fn parse_arguments(arguments: &ast::Arguments) -> CallArguments<'_> {
        arguments
            .arguments_source_order()
            .map(|arg_or_keyword| {
                match arg_or_keyword {
                    ast::ArgOrKeyword::Arg(arg) => match arg {
                        ast::Expr::Starred(ast::ExprStarred { .. }) => Argument::Variadic,
                        // TODO diagnostic if after a keyword argument
                        _ => Argument::Positional,
                    },
                    ast::ArgOrKeyword::Keyword(ast::Keyword { arg, .. }) => {
                        if let Some(arg) = arg {
                            Argument::Keyword(&arg.id)
                        } else {
                            // TODO diagnostic if not last
                            Argument::Keywords
                        }
                    }
                }
            })
            .collect()
    }

    fn infer_argument_types<'a>(
        &mut self,
        ast_arguments: &ast::Arguments,
        arguments: CallArguments<'a>,
        argument_forms: &[Option<ParameterForm>],
    ) -> CallArgumentTypes<'a, 'db> {
        let mut ast_arguments = ast_arguments.arguments_source_order();
        CallArgumentTypes::new(arguments, |index, _| {
            let arg_or_keyword = ast_arguments
                .next()
                .expect("argument lists should have consistent lengths");
            match arg_or_keyword {
                ast::ArgOrKeyword::Arg(arg) => match arg {
                    ast::Expr::Starred(ast::ExprStarred { value, .. }) => {
                        let ty = self.infer_argument_type(value, argument_forms[index]);
                        self.store_expression_type(arg, ty);
                        ty
                    }
                    _ => self.infer_argument_type(arg, argument_forms[index]),
                },
                ast::ArgOrKeyword::Keyword(ast::Keyword { value, .. }) => {
                    self.infer_argument_type(value, argument_forms[index])
                }
            }
        })
    }

    fn infer_argument_type(
        &mut self,
        ast_argument: &ast::Expr,
        form: Option<ParameterForm>,
    ) -> Type<'db> {
        match form {
            None | Some(ParameterForm::Value) => self.infer_expression(ast_argument),
            Some(ParameterForm::Type) => self.infer_type_expression(ast_argument),
        }
    }

    fn infer_optional_expression(&mut self, expression: Option<&ast::Expr>) -> Option<Type<'db>> {
        expression.map(|expr| self.infer_expression(expr))
    }

    #[track_caller]
    fn infer_expression(&mut self, expression: &ast::Expr) -> Type<'db> {
        debug_assert_eq!(
            self.index.try_expression(expression),
            None,
            "Calling `self.infer_expression` on a standalone-expression is not allowed because it can lead to double-inference. Use `self.infer_standalone_expression` instead."
        );

        self.infer_expression_impl(expression)
    }

    fn infer_standalone_expression(&mut self, expression: &ast::Expr) -> Type<'db> {
        let standalone_expression = self.index.expression(expression);
        let types = infer_expression_types(self.db(), standalone_expression);
        self.extend(types);
        self.expression_type(expression)
    }

    fn infer_expression_impl(&mut self, expression: &ast::Expr) -> Type<'db> {
        let ty = match expression {
            ast::Expr::NoneLiteral(ast::ExprNoneLiteral { range: _ }) => Type::none(self.db()),
            ast::Expr::NumberLiteral(literal) => self.infer_number_literal_expression(literal),
            ast::Expr::BooleanLiteral(literal) => self.infer_boolean_literal_expression(literal),
            ast::Expr::StringLiteral(literal) => self.infer_string_literal_expression(literal),
            ast::Expr::BytesLiteral(bytes_literal) => {
                self.infer_bytes_literal_expression(bytes_literal)
            }
            ast::Expr::FString(fstring) => self.infer_fstring_expression(fstring),
            ast::Expr::EllipsisLiteral(literal) => self.infer_ellipsis_literal_expression(literal),
            ast::Expr::Tuple(tuple) => self.infer_tuple_expression(tuple),
            ast::Expr::List(list) => self.infer_list_expression(list),
            ast::Expr::Set(set) => self.infer_set_expression(set),
            ast::Expr::Dict(dict) => self.infer_dict_expression(dict),
            ast::Expr::Generator(generator) => self.infer_generator_expression(generator),
            ast::Expr::ListComp(listcomp) => self.infer_list_comprehension_expression(listcomp),
            ast::Expr::DictComp(dictcomp) => self.infer_dict_comprehension_expression(dictcomp),
            ast::Expr::SetComp(setcomp) => self.infer_set_comprehension_expression(setcomp),
            ast::Expr::Name(name) => self.infer_name_expression(name),
            ast::Expr::Attribute(attribute) => self.infer_attribute_expression(attribute),
            ast::Expr::UnaryOp(unary_op) => self.infer_unary_expression(unary_op),
            ast::Expr::BinOp(binary) => self.infer_binary_expression(binary),
            ast::Expr::BoolOp(bool_op) => self.infer_boolean_expression(bool_op),
            ast::Expr::Compare(compare) => self.infer_compare_expression(compare),
            ast::Expr::Subscript(subscript) => self.infer_subscript_expression(subscript),
            ast::Expr::Slice(slice) => self.infer_slice_expression(slice),
            ast::Expr::Named(named) => self.infer_named_expression(named),
            ast::Expr::If(if_expression) => self.infer_if_expression(if_expression),
            ast::Expr::Lambda(lambda_expression) => self.infer_lambda_expression(lambda_expression),
            ast::Expr::Call(call_expression) => {
                self.infer_call_expression(expression, call_expression)
            }
            ast::Expr::Starred(starred) => self.infer_starred_expression(starred),
            ast::Expr::Yield(yield_expression) => self.infer_yield_expression(yield_expression),
            ast::Expr::YieldFrom(yield_from) => self.infer_yield_from_expression(yield_from),
            ast::Expr::Await(await_expression) => self.infer_await_expression(await_expression),
            ast::Expr::IpyEscapeCommand(_) => {
                todo_type!("Ipy escape command support")
            }
        };

        self.store_expression_type(expression, ty);

        ty
    }

    fn store_expression_type(&mut self, expression: &impl HasScopedExpressionId, ty: Type<'db>) {
        if self.deferred_state.in_string_annotation() {
            // Avoid storing the type of expressions that are part of a string annotation because
            // the expression ids don't exists in the semantic index. Instead, we'll store the type
            // on the string expression itself that represents the annotation.
            return;
        }
        let expr_id = expression.scoped_expression_id(self.db(), self.scope());
        let previous = self.types.expressions.insert(expr_id, ty);
        assert_eq!(previous, None);
    }

    fn infer_number_literal_expression(&mut self, literal: &ast::ExprNumberLiteral) -> Type<'db> {
        let ast::ExprNumberLiteral { range: _, value } = literal;
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

    #[allow(clippy::unused_self)]
    fn infer_boolean_literal_expression(&mut self, literal: &ast::ExprBooleanLiteral) -> Type<'db> {
        let ast::ExprBooleanLiteral { range: _, value } = literal;

        Type::BooleanLiteral(*value)
    }

    fn infer_string_literal_expression(&mut self, literal: &ast::ExprStringLiteral) -> Type<'db> {
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
        let ast::ExprFString { range: _, value } = fstring;

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
                            ast::FStringElement::Expression(expression) => {
                                let ast::FStringExpressionElement {
                                    range: _,
                                    expression,
                                    debug_text: _,
                                    conversion,
                                    format_spec,
                                } = expression;
                                let ty = self.infer_expression(expression);

                                if let Some(ref format_spec) = format_spec {
                                    for element in format_spec.elements.expressions() {
                                        self.infer_expression(&element.expression);
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
                            ast::FStringElement::Literal(literal) => {
                                collector.push_str(&literal.value);
                            }
                        }
                    }
                }
            }
        }
        collector.string_type(self.db())
    }

    fn infer_ellipsis_literal_expression(
        &mut self,
        _literal: &ast::ExprEllipsisLiteral,
    ) -> Type<'db> {
        KnownClass::EllipsisType.to_instance(self.db())
    }

    fn infer_tuple_expression(&mut self, tuple: &ast::ExprTuple) -> Type<'db> {
        let ast::ExprTuple {
            range: _,
            elts,
            ctx: _,
            parenthesized: _,
        } = tuple;

        // Collecting all elements is necessary to infer all sub-expressions even if some
        // element types are `Never` (which leads `from_elements` to return early without
        // consuming the whole iterator).
        let element_types: Vec<_> = elts.iter().map(|elt| self.infer_expression(elt)).collect();

        TupleType::from_elements(self.db(), element_types)
    }

    fn infer_list_expression(&mut self, list: &ast::ExprList) -> Type<'db> {
        let ast::ExprList {
            range: _,
            elts,
            ctx: _,
        } = list;

        for elt in elts {
            self.infer_expression(elt);
        }

        // TODO generic
        KnownClass::List.to_instance(self.db())
    }

    fn infer_set_expression(&mut self, set: &ast::ExprSet) -> Type<'db> {
        let ast::ExprSet { range: _, elts } = set;

        for elt in elts {
            self.infer_expression(elt);
        }

        // TODO generic
        KnownClass::Set.to_instance(self.db())
    }

    fn infer_dict_expression(&mut self, dict: &ast::ExprDict) -> Type<'db> {
        let ast::ExprDict { range: _, items } = dict;

        for item in items {
            self.infer_optional_expression(item.key.as_ref());
            self.infer_expression(&item.value);
        }

        // TODO generic
        KnownClass::Dict.to_instance(self.db())
    }

    /// Infer the type of the `iter` expression of the first comprehension.
    fn infer_first_comprehension_iter(&mut self, comprehensions: &[ast::Comprehension]) {
        let mut comprehensions_iter = comprehensions.iter();
        let Some(first_comprehension) = comprehensions_iter.next() else {
            unreachable!("Comprehension must contain at least one generator");
        };
        self.infer_standalone_expression(&first_comprehension.iter);
    }

    fn infer_generator_expression(&mut self, generator: &ast::ExprGenerator) -> Type<'db> {
        let ast::ExprGenerator {
            range: _,
            elt: _,
            generators,
            parenthesized: _,
        } = generator;

        self.infer_first_comprehension_iter(generators);

        todo_type!("generator type")
    }

    fn infer_list_comprehension_expression(&mut self, listcomp: &ast::ExprListComp) -> Type<'db> {
        let ast::ExprListComp {
            range: _,
            elt: _,
            generators,
        } = listcomp;

        self.infer_first_comprehension_iter(generators);

        todo_type!("list comprehension type")
    }

    fn infer_dict_comprehension_expression(&mut self, dictcomp: &ast::ExprDictComp) -> Type<'db> {
        let ast::ExprDictComp {
            range: _,
            key: _,
            value: _,
            generators,
        } = dictcomp;

        self.infer_first_comprehension_iter(generators);

        todo_type!("dict comprehension type")
    }

    fn infer_set_comprehension_expression(&mut self, setcomp: &ast::ExprSetComp) -> Type<'db> {
        let ast::ExprSetComp {
            range: _,
            elt: _,
            generators,
        } = setcomp;

        self.infer_first_comprehension_iter(generators);

        todo_type!("set comprehension type")
    }

    fn infer_generator_expression_scope(&mut self, generator: &ast::ExprGenerator) {
        let ast::ExprGenerator {
            range: _,
            elt,
            generators,
            parenthesized: _,
        } = generator;

        self.infer_expression(elt);
        self.infer_comprehensions(generators);
    }

    fn infer_list_comprehension_expression_scope(&mut self, listcomp: &ast::ExprListComp) {
        let ast::ExprListComp {
            range: _,
            elt,
            generators,
        } = listcomp;

        self.infer_expression(elt);
        self.infer_comprehensions(generators);
    }

    fn infer_dict_comprehension_expression_scope(&mut self, dictcomp: &ast::ExprDictComp) {
        let ast::ExprDictComp {
            range: _,
            key,
            value,
            generators,
        } = dictcomp;

        self.infer_expression(key);
        self.infer_expression(value);
        self.infer_comprehensions(generators);
    }

    fn infer_set_comprehension_expression_scope(&mut self, setcomp: &ast::ExprSetComp) {
        let ast::ExprSetComp {
            range: _,
            elt,
            generators,
        } = setcomp;

        self.infer_expression(elt);
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
            target,
            iter,
            ifs,
            is_async: _,
        } = comprehension;

        self.infer_target(target, iter, |builder, iter_expr| {
            // TODO: `infer_comprehension_definition` reports a diagnostic if `iter_ty` isn't iterable
            //  but only if the target is a name. We should report a diagnostic here if the target isn't a name:
            //  `[... for a.x in not_iterable]
            if is_first {
                infer_same_file_expression_type(builder.db(), builder.index.expression(iter_expr))
            } else {
                builder.infer_standalone_expression(iter_expr)
            }
            .iterate(builder.db())
        });
        for expr in ifs {
            self.infer_expression(expr);
        }
    }

    fn infer_comprehension_definition(
        &mut self,
        comprehension: &ComprehensionDefinitionKind<'db>,
        definition: Definition<'db>,
    ) {
        let iterable = comprehension.iterable();
        let target = comprehension.target();

        let expression = self.index.expression(iterable);
        let result = infer_expression_types(self.db(), expression);

        // Two things are different if it's the first comprehension:
        // (1) We must lookup the `ScopedExpressionId` of the iterable expression in the outer scope,
        //     because that's the scope we visit it in in the semantic index builder
        // (2) We must *not* call `self.extend()` on the result of the type inference,
        //     because `ScopedExpressionId`s are only meaningful within their own scope, so
        //     we'd add types for random wrong expressions in the current scope
        let iterable_type = if comprehension.is_first() {
            let lookup_scope = self
                .index
                .parent_scope_id(self.scope().file_scope_id(self.db()))
                .expect("A comprehension should never be the top-level scope")
                .to_scope_id(self.db(), self.file());
            result.expression_type(iterable.scoped_expression_id(self.db(), lookup_scope))
        } else {
            self.extend(result);
            result.expression_type(iterable.scoped_expression_id(self.db(), self.scope()))
        };

        let target_type = if comprehension.is_async() {
            // TODO: async iterables/iterators! -- Alex
            todo_type!("async iterables/iterators")
        } else {
            match comprehension.target_kind() {
                TargetKind::Sequence(unpack_position, unpack) => {
                    let unpacked = infer_unpack_types(self.db(), unpack);
                    if unpack_position == UnpackPosition::First {
                        self.context.extend(unpacked.diagnostics());
                    }
                    let target_ast_id = target.scoped_expression_id(self.db(), self.scope());
                    unpacked.expression_type(target_ast_id)
                }
                TargetKind::NameOrAttribute => {
                    iterable_type.try_iterate(self.db()).unwrap_or_else(|err| {
                        err.report_diagnostic(&self.context, iterable_type, iterable.into());
                        err.fallback_element_type(self.db())
                    })
                }
            }
        };

        self.types.expressions.insert(
            target.scoped_expression_id(self.db(), self.scope()),
            target_type,
        );
        self.add_binding(target.into(), definition, target_type);
    }

    fn infer_named_expression(&mut self, named: &ast::ExprNamed) -> Type<'db> {
        // See https://peps.python.org/pep-0572/#differences-between-assignment-expressions-and-assignment-statements
        if named.target.is_name_expr() {
            let definition = self.index.expect_single_definition(named);
            let result = infer_definition_types(self.db(), definition);
            self.extend(result);
            result.binding_type(definition)
        } else {
            // For syntactically invalid targets, we still need to run type inference:
            self.infer_expression(&named.target);
            self.infer_expression(&named.value);
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
            target,
            value,
        } = named;

        let value_ty = self.infer_expression(value);
        self.infer_expression(target);

        self.add_binding(named.into(), definition, value_ty);

        value_ty
    }

    fn infer_if_expression(&mut self, if_expression: &ast::ExprIf) -> Type<'db> {
        let ast::ExprIf {
            range: _,
            test,
            body,
            orelse,
        } = if_expression;

        let test_ty = self.infer_standalone_expression(test);
        let body_ty = self.infer_expression(body);
        let orelse_ty = self.infer_expression(orelse);

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
        self.infer_expression(&lambda_expression.body);
    }

    fn infer_lambda_expression(&mut self, lambda_expression: &ast::ExprLambda) -> Type<'db> {
        let ast::ExprLambda {
            range: _,
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
                        parameter = parameter.with_default_type(self.infer_expression(default));
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
                        parameter = parameter.with_default_type(self.infer_expression(default));
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
                        parameter = parameter.with_default_type(self.infer_expression(default));
                    }
                    parameter
                })
                .collect::<Vec<_>>();
            let keyword_variadic = parameters
                .kwarg
                .as_ref()
                .map(|param| Parameter::keyword_variadic(param.name().id.clone()));

            Parameters::new(
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
        Type::Callable(CallableType::single(
            self.db(),
            Signature::new(parameters, Some(Type::unknown())),
        ))
    }

    /// Returns the type of the first parameter if the given scope is function-like (i.e. function or lambda).
    /// Returns `None` if the scope is not function-like, or has no parameters.
    fn first_param_type_in_scope(&self, scope: ScopeId) -> Option<Type<'db>> {
        let first_param = match scope.node(self.db()) {
            NodeWithScopeKind::Function(f) => f.parameters.iter().next(),
            NodeWithScopeKind::Lambda(l) => l.parameters.as_ref()?.iter().next(),
            _ => None,
        }?;

        let definition = self.index.expect_single_definition(first_param);

        Some(infer_definition_types(self.db(), definition).binding_type(definition))
    }

    /// Returns the type of the nearest enclosing class for the given scope.
    ///
    /// This function walks up the ancestor scopes starting from the given scope,
    /// and finds the closest class definition.
    ///
    /// Returns `None` if no enclosing class is found.a
    fn enclosing_class_symbol(&self, scope: ScopeId) -> Option<Type<'db>> {
        self.index
            .ancestor_scopes(scope.file_scope_id(self.db()))
            .find_map(|(_, ancestor_scope)| {
                if let NodeWithScopeKind::Class(class) = ancestor_scope.node() {
                    let definition = self.index.expect_single_definition(class.node());
                    let result = infer_definition_types(self.db(), definition);

                    Some(result.declaration_type(definition).inner_type())
                } else {
                    None
                }
            })
    }

    fn infer_call_expression(
        &mut self,
        call_expression_node: &ast::Expr,
        call_expression: &ast::ExprCall,
    ) -> Type<'db> {
        let ast::ExprCall {
            range: _,
            func,
            arguments,
        } = call_expression;

        // We don't call `Type::try_call`, because we want to perform type inference on the
        // arguments after matching them to parameters, but before checking that the argument types
        // are assignable to any parameter annotations.
        let mut call_arguments = Self::parse_arguments(arguments);
        let callable_type = self.infer_expression(func);

        // It might look odd here that we emit an error for class-literals but not `type[]` types.
        // But it's deliberate! The typing spec explicitly mandates that `type[]` types can be called
        // even though class-literals cannot. This is because even though a protocol class `SomeProtocol`
        // is always an abstract class, `type[SomeProtocol]` can be a concrete subclass of that protocol
        // -- and indeed, according to the spec, type checkers must disallow abstract subclasses of the
        // protocol to be passed to parameters that accept `type[SomeProtocol]`.
        // <https://typing.python.org/en/latest/spec/protocol.html#type-and-class-objects-vs-protocols>.
        if let Some(protocol_class) = callable_type
            .into_class_literal()
            .and_then(|class| class.into_protocol_class(self.db()))
        {
            report_attempted_protocol_instantiation(&self.context, call_expression, protocol_class);
        }

        // For class literals we model the entire class instantiation logic, so it is handled
        // in a separate function. For some known classes we have manual signatures defined and use
        // the `try_call` path below.
        // TODO: it should be possible to move these special cases into the `try_call_constructor`
        // path instead, or even remove some entirely once we support overloads fully.
        let (call_constructor, known_class) = match callable_type {
            Type::ClassLiteral(class) => (true, class.known(self.db())),
            Type::GenericAlias(generic) => (true, ClassType::Generic(generic).known(self.db())),
            Type::SubclassOf(subclass) => (
                true,
                subclass
                    .subclass_of()
                    .into_class()
                    .and_then(|class| class.known(self.db())),
            ),
            _ => (false, None),
        };

        if call_constructor
            && !matches!(
                known_class,
                Some(
                    KnownClass::Bool
                        | KnownClass::Str
                        | KnownClass::Type
                        | KnownClass::Object
                        | KnownClass::Property
                        | KnownClass::Super
                        | KnownClass::TypeVar
                )
            )
        {
            let argument_forms = vec![Some(ParameterForm::Value); call_arguments.len()];
            let call_argument_types =
                self.infer_argument_types(arguments, call_arguments, &argument_forms);

            return callable_type
                .try_call_constructor(self.db(), call_argument_types)
                .unwrap_or_else(|err| {
                    err.report_diagnostic(&self.context, callable_type, call_expression.into());
                    err.return_type()
                });
        }

        let signatures = callable_type.signatures(self.db());
        let bindings = Bindings::match_parameters(signatures, &mut call_arguments);
        let mut call_argument_types =
            self.infer_argument_types(arguments, call_arguments, &bindings.argument_forms);

        match bindings.check_types(self.db(), &mut call_argument_types) {
            Ok(mut bindings) => {
                for binding in &mut bindings {
                    let binding_type = binding.callable_type;
                    let Some((_, overload)) = binding.matching_overload_mut() else {
                        continue;
                    };

                    match binding_type {
                        Type::FunctionLiteral(function_literal) => {
                            let Some(known_function) = function_literal.known(self.db()) else {
                                continue;
                            };

                            match known_function {
                                KnownFunction::RevealType => {
                                    if let [Some(revealed_type)] = overload.parameter_types() {
                                        if let Some(builder) = self.context.report_diagnostic(
                                            DiagnosticId::RevealedType,
                                            Severity::Info,
                                        ) {
                                            let mut diag = builder.into_diagnostic("Revealed type");
                                            let span = self.context.span(call_expression);
                                            diag.annotate(Annotation::primary(span).message(
                                                format_args!(
                                                    "`{}`",
                                                    revealed_type.display(self.db())
                                                ),
                                            ));
                                        }
                                    }
                                }
                                KnownFunction::AssertType => {
                                    if let [Some(actual_ty), Some(asserted_ty)] =
                                        overload.parameter_types()
                                    {
                                        if !actual_ty
                                            .is_gradual_equivalent_to(self.db(), *asserted_ty)
                                        {
                                            if let Some(builder) = self.context.report_lint(
                                                &TYPE_ASSERTION_FAILURE,
                                                call_expression,
                                            ) {
                                                builder.into_diagnostic(format_args!(
                                                    "Actual type `{}` is not the same \
                                                         as asserted type `{}`",
                                                    actual_ty.display(self.db()),
                                                    asserted_ty.display(self.db()),
                                                ));
                                            }
                                        }
                                    }
                                }
                                KnownFunction::AssertNever => {
                                    if let [Some(actual_ty)] = overload.parameter_types() {
                                        if !actual_ty.is_equivalent_to(self.db(), Type::Never) {
                                            if let Some(builder) = self.context.report_lint(
                                                &TYPE_ASSERTION_FAILURE,
                                                call_expression,
                                            ) {
                                                builder.into_diagnostic(format_args!(
                                                    "Expected type `Never`, got `{}` instead",
                                                    actual_ty.display(self.db()),
                                                ));
                                            }
                                        }
                                    }
                                }
                                KnownFunction::StaticAssert => {
                                    if let [Some(parameter_ty), message] =
                                        overload.parameter_types()
                                    {
                                        let truthiness = match parameter_ty.try_bool(self.db()) {
                                            Ok(truthiness) => truthiness,
                                            Err(err) => {
                                                let condition = arguments
                                                    .find_argument("condition", 0)
                                                    .map(|argument| match argument {
                                                        ruff_python_ast::ArgOrKeyword::Arg(
                                                            expr,
                                                        ) => ast::AnyNodeRef::from(expr),
                                                        ruff_python_ast::ArgOrKeyword::Keyword(
                                                            keyword,
                                                        ) => ast::AnyNodeRef::from(keyword),
                                                    })
                                                    .unwrap_or(ast::AnyNodeRef::from(
                                                        call_expression,
                                                    ));

                                                err.report_diagnostic(&self.context, condition);

                                                continue;
                                            }
                                        };

                                        if let Some(builder) = self
                                            .context
                                            .report_lint(&STATIC_ASSERT_ERROR, call_expression)
                                        {
                                            if !truthiness.is_always_true() {
                                                if let Some(message) = message
                                                    .and_then(Type::into_string_literal)
                                                    .map(|s| &**s.value(self.db()))
                                                {
                                                    builder.into_diagnostic(format_args!(
                                                        "Static assertion error: {message}"
                                                    ));
                                                } else if *parameter_ty
                                                    == Type::BooleanLiteral(false)
                                                {
                                                    builder.into_diagnostic(
                                                        "Static assertion error: \
                                                        argument evaluates to `False`",
                                                    );
                                                } else if truthiness.is_always_false() {
                                                    builder.into_diagnostic(format_args!(
                                                        "Static assertion error: \
                                                        argument of type `{parameter_ty}` \
                                                        is statically known to be falsy",
                                                        parameter_ty =
                                                            parameter_ty.display(self.db())
                                                    ));
                                                } else {
                                                    builder.into_diagnostic(format_args!(
                                                        "Static assertion error: \
                                                         argument of type `{parameter_ty}` \
                                                         has an ambiguous static truthiness",
                                                        parameter_ty =
                                                            parameter_ty.display(self.db())
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                                KnownFunction::Cast => {
                                    if let [Some(casted_type), Some(source_type)] =
                                        overload.parameter_types()
                                    {
                                        let db = self.db();
                                        if (source_type.is_equivalent_to(db, *casted_type)
                                            || source_type.normalized(db)
                                                == casted_type.normalized(db))
                                            && !source_type.contains_todo(db)
                                        {
                                            if let Some(builder) = self
                                                .context
                                                .report_lint(&REDUNDANT_CAST, call_expression)
                                            {
                                                builder.into_diagnostic(format_args!(
                                                    "Value is already of type `{}`",
                                                    casted_type.display(db),
                                                ));
                                            }
                                        }
                                    }
                                }
                                KnownFunction::GetProtocolMembers => {
                                    if let [Some(Type::ClassLiteral(class))] =
                                        overload.parameter_types()
                                    {
                                        if !class.is_protocol(self.db()) {
                                            report_bad_argument_to_get_protocol_members(
                                                &self.context,
                                                call_expression,
                                                *class,
                                            );
                                        }
                                    }
                                }
                                KnownFunction::IsInstance | KnownFunction::IsSubclass => {
                                    if let [_, Some(Type::ClassLiteral(class))] =
                                        overload.parameter_types()
                                    {
                                        if let Some(protocol_class) =
                                            class.into_protocol_class(self.db())
                                        {
                                            if !protocol_class.is_runtime_checkable(self.db()) {
                                                report_runtime_check_against_non_runtime_checkable_protocol(
                                                    &self.context,
                                                    call_expression,
                                                    protocol_class,
                                                    known_function
                                                );
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }

                        Type::ClassLiteral(class) => {
                            let Some(known_class) = class.known(self.db()) else {
                                continue;
                            };

                            match known_class {
                                KnownClass::Super => {
                                    // Handle the case where `super()` is called with no arguments.
                                    // In this case, we need to infer the two arguments:
                                    //   1. The nearest enclosing class
                                    //   2. The first parameter of the current function (typically `self` or `cls`)
                                    match overload.parameter_types() {
                                        [] => {
                                            let scope = self.scope();

                                            let Some(enclosing_class) =
                                                self.enclosing_class_symbol(scope)
                                            else {
                                                overload.set_return_type(Type::unknown());
                                                BoundSuperError::UnavailableImplicitArguments
                                                    .report_diagnostic(
                                                        &self.context,
                                                        call_expression.into(),
                                                    );
                                                continue;
                                            };

                                            let Some(first_param) =
                                                self.first_param_type_in_scope(scope)
                                            else {
                                                overload.set_return_type(Type::unknown());
                                                BoundSuperError::UnavailableImplicitArguments
                                                    .report_diagnostic(
                                                        &self.context,
                                                        call_expression.into(),
                                                    );
                                                continue;
                                            };

                                            let bound_super = BoundSuperType::build(
                                                self.db(),
                                                enclosing_class,
                                                first_param,
                                            )
                                            .unwrap_or_else(|err| {
                                                err.report_diagnostic(
                                                    &self.context,
                                                    call_expression.into(),
                                                );
                                                Type::unknown()
                                            });

                                            overload.set_return_type(bound_super);
                                        }
                                        [Some(pivot_class_type), Some(owner_type)] => {
                                            let bound_super = BoundSuperType::build(
                                                self.db(),
                                                *pivot_class_type,
                                                *owner_type,
                                            )
                                            .unwrap_or_else(|err| {
                                                err.report_diagnostic(
                                                    &self.context,
                                                    call_expression.into(),
                                                );
                                                Type::unknown()
                                            });

                                            overload.set_return_type(bound_super);
                                        }
                                        _ => (),
                                    }
                                }

                                KnownClass::TypeVar => {
                                    let assigned_to = (self.index)
                                        .try_expression(call_expression_node)
                                        .and_then(|expr| expr.assigned_to(self.db()));

                                    let Some(target) =
                                        assigned_to.as_ref().and_then(|assigned_to| {
                                            match assigned_to.node().targets.as_slice() {
                                                [ast::Expr::Name(target)] => Some(target),
                                                _ => None,
                                            }
                                        })
                                    else {
                                        if let Some(builder) = self.context.report_lint(
                                            &INVALID_LEGACY_TYPE_VARIABLE,
                                            call_expression,
                                        ) {
                                            builder.into_diagnostic(format_args!(
                                                "A legacy `typing.TypeVar` must be immediately assigned to a variable",
                                            ));
                                        }
                                        continue;
                                    };

                                    let [Some(name_param), constraints, bound, default, _contravariant, _covariant, _infer_variance] =
                                        overload.parameter_types()
                                    else {
                                        continue;
                                    };

                                    let name_param = name_param
                                        .into_string_literal()
                                        .map(|name| name.value(self.db()).as_ref());
                                    if name_param.is_none_or(|name_param| name_param != target.id) {
                                        if let Some(builder) = self.context.report_lint(
                                            &INVALID_LEGACY_TYPE_VARIABLE,
                                            call_expression,
                                        ) {
                                            builder.into_diagnostic(format_args!(
                                                "The name of a legacy `typing.TypeVar`{} must match \
                                                the name of the variable it is assigned to (`{}`)",
                                                if let Some(name_param) = name_param {
                                                    format!(" (`{name_param}`)")
                                                } else {
                                                    String::new()
                                                },
                                                target.id,
                                            ));
                                        }
                                        continue;
                                    }

                                    let bound_or_constraint = match (bound, constraints) {
                                        (Some(bound), None) => {
                                            Some(TypeVarBoundOrConstraints::UpperBound(*bound))
                                        }

                                        (None, Some(_constraints)) => {
                                            // We don't use UnionType::from_elements or UnionBuilder here,
                                            // because we don't want to simplify the list of constraints like
                                            // we do with the elements of an actual union type.
                                            // TODO: Consider using a new `OneOfType` connective here instead,
                                            // since that more accurately represents the actual semantics of
                                            // typevar constraints.
                                            let elements = UnionType::new(
                                                self.db(),
                                                overload
                                                    .arguments_for_parameter(
                                                        &call_argument_types,
                                                        1,
                                                    )
                                                    .map(|(_, ty)| ty)
                                                    .collect::<Box<_>>(),
                                            );
                                            Some(TypeVarBoundOrConstraints::Constraints(elements))
                                        }

                                        // TODO: Emit a diagnostic that TypeVar cannot be both bounded and
                                        // constrained
                                        (Some(_), Some(_)) => continue,

                                        (None, None) => None,
                                    };

                                    let containing_assignment =
                                        self.index.expect_single_definition(target);
                                    overload.set_return_type(Type::KnownInstance(
                                        KnownInstanceType::TypeVar(TypeVarInstance::new(
                                            self.db(),
                                            target.id.clone(),
                                            containing_assignment,
                                            bound_or_constraint,
                                            *default,
                                            TypeVarKind::Legacy,
                                        )),
                                    ));
                                }

                                _ => (),
                            }
                        }
                        _ => (),
                    }
                }
                bindings.return_type(self.db())
            }

            Err(CallError(_, bindings)) => {
                bindings.report_diagnostics(&self.context, call_expression.into());
                bindings.return_type(self.db())
            }
        }
    }

    fn infer_starred_expression(&mut self, starred: &ast::ExprStarred) -> Type<'db> {
        let ast::ExprStarred {
            range: _,
            value,
            ctx: _,
        } = starred;

        let iterable_type = self.infer_expression(value);
        iterable_type.try_iterate(self.db()).unwrap_or_else(|err| {
            err.report_diagnostic(&self.context, iterable_type, value.as_ref().into());
            err.fallback_element_type(self.db())
        });

        // TODO
        todo_type!("starred expression")
    }

    fn infer_yield_expression(&mut self, yield_expression: &ast::ExprYield) -> Type<'db> {
        let ast::ExprYield { range: _, value } = yield_expression;
        self.infer_optional_expression(value.as_deref());
        todo_type!("yield expressions")
    }

    fn infer_yield_from_expression(&mut self, yield_from: &ast::ExprYieldFrom) -> Type<'db> {
        let ast::ExprYieldFrom { range: _, value } = yield_from;

        let iterable_type = self.infer_expression(value);
        iterable_type.try_iterate(self.db()).unwrap_or_else(|err| {
            err.report_diagnostic(&self.context, iterable_type, value.as_ref().into());
            err.fallback_element_type(self.db())
        });

        // TODO get type from `ReturnType` of generator
        todo_type!("Generic `typing.Generator` type")
    }

    fn infer_await_expression(&mut self, await_expression: &ast::ExprAwait) -> Type<'db> {
        let ast::ExprAwait { range: _, value } = await_expression;
        self.infer_expression(value);
        todo_type!("generic `typing.Awaitable` type")
    }

    /// Infer the type of a [`ast::ExprName`] expression, assuming a load context.
    fn infer_name_load(&mut self, name_node: &ast::ExprName) -> Type<'db> {
        let ast::ExprName {
            range: _,
            id: symbol_name,
            ctx: _,
        } = name_node;

        let db = self.db();
        let scope = self.scope();
        let file_scope_id = scope.file_scope_id(db);
        let symbol_table = self.index.symbol_table(file_scope_id);
        let use_def = self.index.use_def_map(file_scope_id);

        // If we're inferring types of deferred expressions, always treat them as public symbols
        let local_scope_symbol = if self.is_deferred() {
            if let Some(symbol_id) = symbol_table.symbol_id_by_name(symbol_name) {
                symbol_from_bindings(db, use_def.public_bindings(symbol_id))
            } else {
                assert!(
                    self.deferred_state.in_string_annotation(),
                    "Expected the symbol table to create a symbol for every Name node"
                );
                Symbol::Unbound
            }
        } else {
            let use_id = name_node.scoped_use_id(db, scope);
            symbol_from_bindings(db, use_def.bindings_at_use(use_id))
        };

        let symbol = SymbolAndQualifiers::from(local_scope_symbol).or_fall_back_to(db, || {
            let has_bindings_in_this_scope = match symbol_table.symbol_by_name(symbol_name) {
                Some(symbol) => symbol.is_bound(),
                None => {
                    assert!(
                        self.deferred_state.in_string_annotation(),
                        "Expected the symbol table to create a symbol for every Name node"
                    );
                    false
                }
            };

            // If it's a function-like scope and there is one or more binding in this scope (but
            // none of those bindings are visible from where we are in the control flow), we cannot
            // fallback to any bindings in enclosing scopes. As such, we can immediately short-circuit
            // here and return `Symbol::Unbound`.
            //
            // This is because Python is very strict in its categorisation of whether a variable is
            // a local variable or not in function-like scopes. If a variable has any bindings in a
            // function-like scope, it is considered a local variable; it never references another
            // scope. (At runtime, it would use the `LOAD_FAST` opcode.)
            if has_bindings_in_this_scope && scope.is_function_like(db) {
                return Symbol::Unbound.into();
            }

            let current_file = self.file();

            // Walk up parent scopes looking for a possible enclosing scope that may have a
            // definition of this name visible to us (would be `LOAD_DEREF` at runtime.)
            // Note that we skip the scope containing the use that we are resolving, since we
            // already looked for the symbol there up above.
            for (enclosing_scope_file_id, _) in self.index.ancestor_scopes(file_scope_id).skip(1) {
                // Class scopes are not visible to nested scopes, and we need to handle global
                // scope differently (because an unbound name there falls back to builtins), so
                // check only function-like scopes.
                // There is one exception to this rule: type parameter scopes can see
                // names defined in an immediately-enclosing class scope.
                let enclosing_scope_id = enclosing_scope_file_id.to_scope_id(db, current_file);
                let is_immediately_enclosing_scope = scope.is_type_parameter(db)
                    && scope
                        .scope(db)
                        .parent()
                        .is_some_and(|parent| parent == enclosing_scope_file_id);
                if !enclosing_scope_id.is_function_like(db) && !is_immediately_enclosing_scope {
                    continue;
                }

                // If the reference is in a nested eager scope, we need to look for the symbol at
                // the point where the previous enclosing scope was defined, instead of at the end
                // of the scope. (Note that the semantic index builder takes care of only
                // registering eager bindings for nested scopes that are actually eager, and for
                // enclosing scopes that actually contain bindings that we should use when
                // resolving the reference.)
                if !self.is_deferred() {
                    match self.index.eager_bindings(
                        enclosing_scope_file_id,
                        symbol_name,
                        file_scope_id,
                    ) {
                        EagerBindingsResult::Found(bindings) => {
                            return symbol_from_bindings(db, bindings).into();
                        }
                        // There are no visible bindings here.
                        // Don't fall back to non-eager symbol resolution.
                        EagerBindingsResult::NotFound => {
                            continue;
                        }
                        EagerBindingsResult::NoLongerInEagerContext => {}
                    }
                }

                let enclosing_symbol_table = self.index.symbol_table(enclosing_scope_file_id);
                let Some(enclosing_symbol) = enclosing_symbol_table.symbol_by_name(symbol_name)
                else {
                    continue;
                };
                if enclosing_symbol.is_bound() {
                    // We can return early here, because the nearest function-like scope that
                    // defines a name must be the only source for the nonlocal reference (at
                    // runtime, it is the scope that creates the cell for our closure.) If the name
                    // isn't bound in that scope, we should get an unbound name, not continue
                    // falling back to other scopes / globals / builtins.
                    return symbol(db, enclosing_scope_id, symbol_name);
                }
            }

            SymbolAndQualifiers::from(Symbol::Unbound)
                // No nonlocal binding? Check the module's explicit globals.
                // Avoid infinite recursion if `self.scope` already is the module's global scope.
                .or_fall_back_to(db, || {
                    if file_scope_id.is_global() {
                        return Symbol::Unbound.into();
                    }

                    if !self.is_deferred() {
                        match self.index.eager_bindings(
                            FileScopeId::global(),
                            symbol_name,
                            file_scope_id,
                        ) {
                            EagerBindingsResult::Found(bindings) => {
                                return symbol_from_bindings(db, bindings).into();
                            }
                            // There are no visible bindings here.
                            EagerBindingsResult::NotFound => {
                                return Symbol::Unbound.into();
                            }
                            EagerBindingsResult::NoLongerInEagerContext => {}
                        }
                    }

                    explicit_global_symbol(db, self.file(), symbol_name)
                })
                // Not found in the module's explicitly declared global symbols?
                // Check the "implicit globals" such as `__doc__`, `__file__`, `__name__`, etc.
                // These are looked up as attributes on `types.ModuleType`.
                .or_fall_back_to(db, || module_type_implicit_global_symbol(db, symbol_name))
                // Not found in globals? Fallback to builtins
                // (without infinite recursion if we're already in builtins.)
                .or_fall_back_to(db, || {
                    if Some(self.scope()) == builtins_module_scope(db) {
                        Symbol::Unbound.into()
                    } else {
                        builtins_symbol(db, symbol_name)
                    }
                })
                // Still not found? It might be `reveal_type`...
                .or_fall_back_to(db, || {
                    if symbol_name == "reveal_type" {
                        if let Some(builder) =
                            self.context.report_lint(&UNDEFINED_REVEAL, name_node)
                        {
                            let mut diag =
                                builder.into_diagnostic("`reveal_type` used without importing it");
                            diag.info(
                                "This is allowed for debugging convenience but will fail at runtime"
                            );
                        }
                        typing_extensions_symbol(db, symbol_name)
                    } else {
                        Symbol::Unbound.into()
                    }
                })
        });

        symbol
            .unwrap_with_diagnostic(|lookup_error| match lookup_error {
                LookupError::Unbound(qualifiers) => {
                    if self.is_reachable(name_node) {
                        report_unresolved_reference(&self.context, name_node);
                    }
                    TypeAndQualifiers::new(Type::unknown(), qualifiers)
                }
                LookupError::PossiblyUnbound(type_when_bound) => {
                    if self.is_reachable(name_node) {
                        report_possibly_unresolved_reference(&self.context, name_node);
                    }
                    type_when_bound
                }
            })
            .inner_type()
    }

    fn infer_name_expression(&mut self, name: &ast::ExprName) -> Type<'db> {
        match name.ctx {
            ExprContext::Load => self.infer_name_load(name),
            ExprContext::Store | ExprContext::Del => Type::Never,
            ExprContext::Invalid => Type::unknown(),
        }
    }

    /// Infer the type of a [`ast::ExprAttribute`] expression, assuming a load context.
    fn infer_attribute_load(&mut self, attribute: &ast::ExprAttribute) -> Type<'db> {
        let ast::ExprAttribute {
            value,
            attr,
            range: _,
            ctx: _,
        } = attribute;

        let value_type = self.infer_expression(value);
        let db = self.db();

        value_type
            .member(db, &attr.id)
            .unwrap_with_diagnostic(|lookup_error| match lookup_error {
                LookupError::Unbound(_) => {
                    let report_unresolved_attribute = self.is_reachable(attribute);

                    if report_unresolved_attribute {
                        let bound_on_instance = match value_type {
                            Type::ClassLiteral(class) => {
                                !class.instance_member(db, None, attr).symbol.is_unbound()
                            }
                            Type::SubclassOf(subclass_of @ SubclassOfType { .. }) => {
                                match subclass_of.subclass_of() {
                                    SubclassOfInner::Class(class) => {
                                        !class.instance_member(db, attr).symbol.is_unbound()
                                    }
                                    SubclassOfInner::Dynamic(_) => unreachable!(
                                        "Attribute lookup on a dynamic `SubclassOf` type should always return a bound symbol"
                                    ),
                                }
                            }
                            _ => false,
                        };

                        if let Some(builder) = self
                            .context
                            .report_lint(&UNRESOLVED_ATTRIBUTE, attribute)
                        {
                        if bound_on_instance {
                            builder.into_diagnostic(
                                format_args!(
                                    "Attribute `{}` can only be accessed on instances, \
                                     not on the class object `{}` itself.",
                                    attr.id,
                                    value_type.display(db)
                                ),
                            );
                        } else {
                            builder.into_diagnostic(
                                format_args!(
                                    "Type `{}` has no attribute `{}`",
                                    value_type.display(db),
                                    attr.id
                                ),
                            );
                        }
                        }
                    }

                    Type::unknown().into()
                }
                LookupError::PossiblyUnbound(type_when_bound) => {
                    report_possibly_unbound_attribute(
                        &self.context,
                        attribute,
                        &attr.id,
                        value_type,
                    );

                    type_when_bound
                }
            }).inner_type()
    }

    fn infer_attribute_expression(&mut self, attribute: &ast::ExprAttribute) -> Type<'db> {
        let ast::ExprAttribute {
            value,
            attr: _,
            range: _,
            ctx,
        } = attribute;

        match ctx {
            ExprContext::Load => self.infer_attribute_load(attribute),
            ExprContext::Store | ExprContext::Del => {
                self.infer_expression(value);
                Type::Never
            }
            ExprContext::Invalid => {
                self.infer_expression(value);
                Type::unknown()
            }
        }
    }

    fn infer_unary_expression(&mut self, unary: &ast::ExprUnaryOp) -> Type<'db> {
        let ast::ExprUnaryOp {
            range: _,
            op,
            operand,
        } = unary;

        let operand_type = self.infer_expression(operand);

        match (op, operand_type) {
            (_, Type::Dynamic(_)) => operand_type,
            (_, Type::Never) => Type::Never,

            (ast::UnaryOp::UAdd, Type::IntLiteral(value)) => Type::IntLiteral(value),
            (ast::UnaryOp::USub, Type::IntLiteral(value)) => Type::IntLiteral(-value),
            (ast::UnaryOp::Invert, Type::IntLiteral(value)) => Type::IntLiteral(!value),

            (ast::UnaryOp::UAdd, Type::BooleanLiteral(bool)) => Type::IntLiteral(i64::from(bool)),
            (ast::UnaryOp::USub, Type::BooleanLiteral(bool)) => Type::IntLiteral(-i64::from(bool)),
            (ast::UnaryOp::Invert, Type::BooleanLiteral(bool)) => {
                Type::IntLiteral(!i64::from(bool))
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
                | Type::MethodWrapper(_)
                | Type::DataclassDecorator(_)
                | Type::DataclassTransformer(_)
                | Type::BoundMethod(_)
                | Type::ModuleLiteral(_)
                | Type::ClassLiteral(_)
                | Type::GenericAlias(_)
                | Type::SubclassOf(_)
                | Type::NominalInstance(_)
                | Type::ProtocolInstance(_)
                | Type::KnownInstance(_)
                | Type::PropertyInstance(_)
                | Type::Union(_)
                | Type::Intersection(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::StringLiteral(_)
                | Type::LiteralString
                | Type::BytesLiteral(_)
                | Type::SliceLiteral(_)
                | Type::Tuple(_)
                | Type::BoundSuper(_)
                | Type::TypeVar(_),
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
                    CallArgumentTypes::none(),
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

    fn infer_binary_expression(&mut self, binary: &ast::ExprBinOp) -> Type<'db> {
        let ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
        } = binary;

        let left_ty = self.infer_expression(left);
        let right_ty = self.infer_expression(right);

        self.infer_binary_expression_type(binary.into(), false, left_ty, right_ty, *op)
            .unwrap_or_else(|| {
                if let Some(builder) = self.context.report_lint(&UNSUPPORTED_OPERATOR, binary) {
                    builder.into_diagnostic(format_args!(
                        "Operator `{op}` is unsupported between objects of type `{}` and `{}`",
                        left_ty.display(self.db()),
                        right_ty.display(self.db())
                    ));
                }
                Type::unknown()
            })
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

        match (left_ty, right_ty, op) {
            (Type::Union(lhs_union), rhs, _) => {
                let mut union = UnionBuilder::new(self.db());
                for lhs in lhs_union.elements(self.db()) {
                    let result = self.infer_binary_expression_type(
                        node,
                        emitted_division_by_zero_diagnostic,
                        *lhs,
                        rhs,
                        op,
                    )?;
                    union = union.add(result);
                }
                Some(union.build())
            }
            (lhs, Type::Union(rhs_union), _) => {
                let mut union = UnionBuilder::new(self.db());
                for rhs in rhs_union.elements(self.db()) {
                    let result = self.infer_binary_expression_type(
                        node,
                        emitted_division_by_zero_diagnostic,
                        lhs,
                        *rhs,
                        op,
                    )?;
                    union = union.add(result);
                }
                Some(union.build())
            }

            // Non-todo Anys take precedence over Todos (as if we fix this `Todo` in the future,
            // the result would then become Any or Unknown, respectively).
            (any @ Type::Dynamic(DynamicType::Any), _, _)
            | (_, any @ Type::Dynamic(DynamicType::Any), _) => Some(any),
            (unknown @ Type::Dynamic(DynamicType::Unknown), _, _)
            | (_, unknown @ Type::Dynamic(DynamicType::Unknown), _) => Some(unknown),
            (todo @ Type::Dynamic(DynamicType::Todo(_)), _, _)
            | (_, todo @ Type::Dynamic(DynamicType::Todo(_)), _) => Some(todo),
            (todo @ Type::Dynamic(DynamicType::SubscriptedProtocol), _, _)
            | (_, todo @ Type::Dynamic(DynamicType::SubscriptedProtocol), _) => Some(todo),
            (todo @ Type::Dynamic(DynamicType::SubscriptedGeneric), _, _)
            | (_, todo @ Type::Dynamic(DynamicType::SubscriptedGeneric), _) => Some(todo),
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

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::FloorDiv) => Some(
                n.checked_div(m)
                    .map(Type::IntLiteral)
                    .unwrap_or_else(|| KnownClass::Int.to_instance(self.db())),
            ),

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Mod) => Some(
                n.checked_rem(m)
                    .map(Type::IntLiteral)
                    .unwrap_or_else(|| KnownClass::Int.to_instance(self.db())),
            ),

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

            (Type::BytesLiteral(lhs), Type::BytesLiteral(rhs), ast::Operator::Add) => {
                let bytes = [&**lhs.value(self.db()), &**rhs.value(self.db())].concat();
                Some(Type::bytes_literal(self.db(), &bytes))
            }

            (Type::StringLiteral(lhs), Type::StringLiteral(rhs), ast::Operator::Add) => {
                let lhs_value = lhs.value(self.db()).to_string();
                let rhs_value = rhs.value(self.db()).as_ref();
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

            (Type::BooleanLiteral(bool_value), right, op) => self.infer_binary_expression_type(
                node,
                emitted_division_by_zero_diagnostic,
                Type::IntLiteral(i64::from(bool_value)),
                right,
                op,
            ),
            (left, Type::BooleanLiteral(bool_value), op) => self.infer_binary_expression_type(
                node,
                emitted_division_by_zero_diagnostic,
                left,
                Type::IntLiteral(i64::from(bool_value)),
                op,
            ),

            (Type::Tuple(lhs), Type::Tuple(rhs), ast::Operator::Add) => {
                // Note: this only works on heterogeneous tuples.
                let lhs_elements = lhs.elements(self.db());
                let rhs_elements = rhs.elements(self.db());

                Some(TupleType::from_elements(
                    self.db(),
                    lhs_elements
                        .iter()
                        .copied()
                        .chain(rhs_elements.iter().copied()),
                ))
            }

            // We've handled all of the special cases that we support for literals, so we need to
            // fall back on looking for dunder methods on one of the operand types.
            (
                Type::FunctionLiteral(_)
                | Type::Callable(..)
                | Type::BoundMethod(_)
                | Type::WrapperDescriptor(_)
                | Type::MethodWrapper(_)
                | Type::DataclassDecorator(_)
                | Type::DataclassTransformer(_)
                | Type::ModuleLiteral(_)
                | Type::ClassLiteral(_)
                | Type::GenericAlias(_)
                | Type::SubclassOf(_)
                | Type::NominalInstance(_)
                | Type::ProtocolInstance(_)
                | Type::KnownInstance(_)
                | Type::PropertyInstance(_)
                | Type::Intersection(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::IntLiteral(_)
                | Type::StringLiteral(_)
                | Type::LiteralString
                | Type::BytesLiteral(_)
                | Type::SliceLiteral(_)
                | Type::Tuple(_)
                | Type::BoundSuper(_)
                | Type::TypeVar(_),
                Type::FunctionLiteral(_)
                | Type::Callable(..)
                | Type::BoundMethod(_)
                | Type::WrapperDescriptor(_)
                | Type::MethodWrapper(_)
                | Type::DataclassDecorator(_)
                | Type::DataclassTransformer(_)
                | Type::ModuleLiteral(_)
                | Type::ClassLiteral(_)
                | Type::GenericAlias(_)
                | Type::SubclassOf(_)
                | Type::NominalInstance(_)
                | Type::ProtocolInstance(_)
                | Type::KnownInstance(_)
                | Type::PropertyInstance(_)
                | Type::Intersection(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::IntLiteral(_)
                | Type::StringLiteral(_)
                | Type::LiteralString
                | Type::BytesLiteral(_)
                | Type::SliceLiteral(_)
                | Type::Tuple(_)
                | Type::BoundSuper(_)
                | Type::TypeVar(_),
                op,
            ) => {
                // We either want to call lhs.__op__ or rhs.__rop__. The full decision tree from
                // the Python spec [1] is:
                //
                //   - If rhs is a (proper) subclass of lhs, and it provides a different
                //     implementation of __rop__, use that.
                //   - Otherwise, if lhs implements __op__, use that.
                //   - Otherwise, if lhs and rhs are different types, and rhs implements __rop__,
                //     use that.
                //
                // [1] https://docs.python.org/3/reference/datamodel.html#object.__radd__

                // Technically we don't have to check left_ty != right_ty here, since if the types
                // are the same, they will trivially have the same implementation of the reflected
                // dunder, and so we'll fail the inner check. But the type equality check will be
                // faster for the common case, and allow us to skip the (two) class member lookups.
                let left_class = left_ty.to_meta_type(self.db());
                let right_class = right_ty.to_meta_type(self.db());
                if left_ty != right_ty && right_ty.is_subtype_of(self.db(), left_ty) {
                    let reflected_dunder = op.reflected_dunder();
                    let rhs_reflected = right_class.member(self.db(), reflected_dunder).symbol;
                    // TODO: if `rhs_reflected` is possibly unbound, we should union the two possible
                    // Bindings together
                    if !rhs_reflected.is_unbound()
                        && rhs_reflected != left_class.member(self.db(), reflected_dunder).symbol
                    {
                        return right_ty
                            .try_call_dunder(
                                self.db(),
                                reflected_dunder,
                                CallArgumentTypes::positional([left_ty]),
                            )
                            .map(|outcome| outcome.return_type(self.db()))
                            .or_else(|_| {
                                left_ty
                                    .try_call_dunder(
                                        self.db(),
                                        op.dunder(),
                                        CallArgumentTypes::positional([right_ty]),
                                    )
                                    .map(|outcome| outcome.return_type(self.db()))
                            })
                            .ok();
                    }
                }

                let call_on_left_instance = left_ty
                    .try_call_dunder(
                        self.db(),
                        op.dunder(),
                        CallArgumentTypes::positional([right_ty]),
                    )
                    .map(|outcome| outcome.return_type(self.db()))
                    .ok();

                call_on_left_instance.or_else(|| {
                    if left_ty == right_ty {
                        None
                    } else {
                        right_ty
                            .try_call_dunder(
                                self.db(),
                                op.reflected_dunder(),
                                CallArgumentTypes::positional([left_ty]),
                            )
                            .map(|outcome| outcome.return_type(self.db()))
                            .ok()
                    }
                })
            }
        }
    }

    fn infer_boolean_expression(&mut self, bool_op: &ast::ExprBoolOp) -> Type<'db> {
        let ast::ExprBoolOp {
            range: _,
            op,
            values,
        } = bool_op;
        self.infer_chained_boolean_types(
            *op,
            values.iter().enumerate(),
            |builder, (index, value)| {
                let ty = if index == values.len() - 1 {
                    builder.infer_expression(value)
                } else {
                    builder.infer_standalone_expression(value)
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
                    if done {
                        Type::Never
                    } else {
                        ty
                    }
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
            left,
            ops,
            comparators,
        } = compare;

        self.infer_expression(left);

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
                let right_ty = builder.infer_expression(right);

                let range = TextRange::new(left.start(), right.end());

                let ty = builder
                    .infer_binary_type_comparison(left_ty, *op, right_ty, range)
                    .unwrap_or_else(|error| {
                        if let Some(diagnostic_builder) =
                            builder.context.report_lint(&UNSUPPORTED_OPERATOR, range)
                        {
                            // Handle unsupported operators (diagnostic, `bool`/`Unknown` outcome)
                            diagnostic_builder.into_diagnostic(format_args!(
                                "Operator `{}` is not supported for types `{}` and `{}`{}",
                                error.op,
                                error.left_ty.display(builder.db()),
                                error.right_ty.display(builder.db()),
                                if (left_ty, right_ty) == (error.left_ty, error.right_ty) {
                                    String::new()
                                } else {
                                    format!(
                                        ", in comparing `{}` with `{}`",
                                        left_ty.display(builder.db()),
                                        right_ty.display(builder.db())
                                    )
                                }
                            ));
                        }

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
    ) -> Result<Type<'db>, CompareUnsupportedError<'db>> {
        // If a comparison yields a definitive true/false answer on a (positive) part
        // of an intersection type, it will also yield a definitive answer on the full
        // intersection type, which is even more specific.
        for pos in intersection.positive(self.db()) {
            let result = match intersection_on {
                IntersectionOn::Left => {
                    self.infer_binary_type_comparison(*pos, op, other, range)?
                }
                IntersectionOn::Right => {
                    self.infer_binary_type_comparison(other, op, *pos, range)?
                }
            };
            if let Type::BooleanLiteral(b) = result {
                return Ok(Type::BooleanLiteral(b));
            }
        }

        // For negative contributions to the intersection type, there are only a few
        // special cases that allow us to narrow down the result type of the comparison.
        for neg in intersection.negative(self.db()) {
            let result = match intersection_on {
                IntersectionOn::Left => self
                    .infer_binary_type_comparison(*neg, op, other, range)
                    .ok(),
                IntersectionOn::Right => self
                    .infer_binary_type_comparison(other, op, *neg, range)
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

        for pos in intersection.positive(self.db()) {
            let result = match intersection_on {
                IntersectionOn::Left => {
                    self.infer_binary_type_comparison(*pos, op, other, range)?
                }
                IntersectionOn::Right => {
                    self.infer_binary_type_comparison(other, op, *pos, range)?
                }
            };
            builder = builder.add_positive(result);
        }

        Ok(builder.build())
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
    ) -> Result<Type<'db>, CompareUnsupportedError<'db>> {
        // Note: identity (is, is not) for equal builtin types is unreliable and not part of the
        // language spec.
        // - `[ast::CompOp::Is]`: return `false` if unequal, `bool` if equal
        // - `[ast::CompOp::IsNot]`: return `true` if unequal, `bool` if equal
        match (left, right) {
            (Type::Union(union), other) => {
                let mut builder = UnionBuilder::new(self.db());
                for element in union.elements(self.db()) {
                    builder =
                        builder.add(self.infer_binary_type_comparison(*element, op, other, range)?);
                }
                Ok(builder.build())
            }
            (other, Type::Union(union)) => {
                let mut builder = UnionBuilder::new(self.db());
                for element in union.elements(self.db()) {
                    builder =
                        builder.add(self.infer_binary_type_comparison(other, op, *element, range)?);
                }
                Ok(builder.build())
            }

            (Type::Intersection(intersection), right) => self
                .infer_binary_intersection_type_comparison(
                    intersection,
                    op,
                    right,
                    IntersectionOn::Left,
                    range,
                ),
            (left, Type::Intersection(intersection)) => self
                .infer_binary_intersection_type_comparison(
                    intersection,
                    op,
                    left,
                    IntersectionOn::Right,
                    range,
                ),

            (Type::IntLiteral(n), Type::IntLiteral(m)) => match op {
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
                ast::CmpOp::In | ast::CmpOp::NotIn => Err(CompareUnsupportedError {
                    op,
                    left_ty: left,
                    right_ty: right,
                }),
            },
            (Type::IntLiteral(_), Type::NominalInstance(_)) => self.infer_binary_type_comparison(
                KnownClass::Int.to_instance(self.db()),
                op,
                right,
                range,
            ),
            (Type::NominalInstance(_), Type::IntLiteral(_)) => self.infer_binary_type_comparison(
                left,
                op,
                KnownClass::Int.to_instance(self.db()),
                range,
            ),

            // Booleans are coded as integers (False = 0, True = 1)
            (Type::IntLiteral(n), Type::BooleanLiteral(b)) => self.infer_binary_type_comparison(
                Type::IntLiteral(n),
                op,
                Type::IntLiteral(i64::from(b)),
                range,
            ),
            (Type::BooleanLiteral(b), Type::IntLiteral(m)) => self.infer_binary_type_comparison(
                Type::IntLiteral(i64::from(b)),
                op,
                Type::IntLiteral(m),
                range,
            ),
            (Type::BooleanLiteral(a), Type::BooleanLiteral(b)) => self
                .infer_binary_type_comparison(
                    Type::IntLiteral(i64::from(a)),
                    op,
                    Type::IntLiteral(i64::from(b)),
                    range,
                ),

            (Type::StringLiteral(salsa_s1), Type::StringLiteral(salsa_s2)) => {
                let s1 = salsa_s1.value(self.db());
                let s2 = salsa_s2.value(self.db());
                match op {
                    ast::CmpOp::Eq => Ok(Type::BooleanLiteral(s1 == s2)),
                    ast::CmpOp::NotEq => Ok(Type::BooleanLiteral(s1 != s2)),
                    ast::CmpOp::Lt => Ok(Type::BooleanLiteral(s1 < s2)),
                    ast::CmpOp::LtE => Ok(Type::BooleanLiteral(s1 <= s2)),
                    ast::CmpOp::Gt => Ok(Type::BooleanLiteral(s1 > s2)),
                    ast::CmpOp::GtE => Ok(Type::BooleanLiteral(s1 >= s2)),
                    ast::CmpOp::In => Ok(Type::BooleanLiteral(s2.contains(s1.as_ref()))),
                    ast::CmpOp::NotIn => Ok(Type::BooleanLiteral(!s2.contains(s1.as_ref()))),
                    ast::CmpOp::Is => {
                        if s1 == s2 {
                            Ok(KnownClass::Bool.to_instance(self.db()))
                        } else {
                            Ok(Type::BooleanLiteral(false))
                        }
                    }
                    ast::CmpOp::IsNot => {
                        if s1 == s2 {
                            Ok(KnownClass::Bool.to_instance(self.db()))
                        } else {
                            Ok(Type::BooleanLiteral(true))
                        }
                    }
                }
            }
            (Type::StringLiteral(_), _) => self.infer_binary_type_comparison(
                KnownClass::Str.to_instance(self.db()),
                op,
                right,
                range,
            ),
            (_, Type::StringLiteral(_)) => self.infer_binary_type_comparison(
                left,
                op,
                KnownClass::Str.to_instance(self.db()),
                range,
            ),

            (Type::LiteralString, _) => self.infer_binary_type_comparison(
                KnownClass::Str.to_instance(self.db()),
                op,
                right,
                range,
            ),
            (_, Type::LiteralString) => self.infer_binary_type_comparison(
                left,
                op,
                KnownClass::Str.to_instance(self.db()),
                range,
            ),

            (Type::BytesLiteral(salsa_b1), Type::BytesLiteral(salsa_b2)) => {
                let b1 = &**salsa_b1.value(self.db());
                let b2 = &**salsa_b2.value(self.db());
                match op {
                    ast::CmpOp::Eq => Ok(Type::BooleanLiteral(b1 == b2)),
                    ast::CmpOp::NotEq => Ok(Type::BooleanLiteral(b1 != b2)),
                    ast::CmpOp::Lt => Ok(Type::BooleanLiteral(b1 < b2)),
                    ast::CmpOp::LtE => Ok(Type::BooleanLiteral(b1 <= b2)),
                    ast::CmpOp::Gt => Ok(Type::BooleanLiteral(b1 > b2)),
                    ast::CmpOp::GtE => Ok(Type::BooleanLiteral(b1 >= b2)),
                    ast::CmpOp::In => {
                        Ok(Type::BooleanLiteral(memchr::memmem::find(b2, b1).is_some()))
                    }
                    ast::CmpOp::NotIn => {
                        Ok(Type::BooleanLiteral(memchr::memmem::find(b2, b1).is_none()))
                    }
                    ast::CmpOp::Is => {
                        if b1 == b2 {
                            Ok(KnownClass::Bool.to_instance(self.db()))
                        } else {
                            Ok(Type::BooleanLiteral(false))
                        }
                    }
                    ast::CmpOp::IsNot => {
                        if b1 == b2 {
                            Ok(KnownClass::Bool.to_instance(self.db()))
                        } else {
                            Ok(Type::BooleanLiteral(true))
                        }
                    }
                }
            }
            (Type::BytesLiteral(_), _) => self.infer_binary_type_comparison(
                KnownClass::Bytes.to_instance(self.db()),
                op,
                right,
                range,
            ),
            (_, Type::BytesLiteral(_)) => self.infer_binary_type_comparison(
                left,
                op,
                KnownClass::Bytes.to_instance(self.db()),
                range,
            ),
            (Type::Tuple(_), Type::NominalInstance(instance))
                if instance
                    .class()
                    .is_known(self.db(), KnownClass::VersionInfo) =>
            {
                self.infer_binary_type_comparison(
                    left,
                    op,
                    Type::version_info_tuple(self.db()),
                    range,
                )
            }
            (Type::NominalInstance(instance), Type::Tuple(_))
                if instance
                    .class()
                    .is_known(self.db(), KnownClass::VersionInfo) =>
            {
                self.infer_binary_type_comparison(
                    Type::version_info_tuple(self.db()),
                    op,
                    right,
                    range,
                )
            }
            (Type::Tuple(lhs), Type::Tuple(rhs)) => {
                // Note: This only works on heterogeneous tuple types.
                let lhs_elements = lhs.elements(self.db());
                let rhs_elements = rhs.elements(self.db());

                let mut tuple_rich_comparison =
                    |op| self.infer_tuple_rich_comparison(lhs_elements, op, rhs_elements, range);

                match op {
                    ast::CmpOp::Eq => tuple_rich_comparison(RichCompareOperator::Eq),
                    ast::CmpOp::NotEq => tuple_rich_comparison(RichCompareOperator::Ne),
                    ast::CmpOp::Lt => tuple_rich_comparison(RichCompareOperator::Lt),
                    ast::CmpOp::LtE => tuple_rich_comparison(RichCompareOperator::Le),
                    ast::CmpOp::Gt => tuple_rich_comparison(RichCompareOperator::Gt),
                    ast::CmpOp::GtE => tuple_rich_comparison(RichCompareOperator::Ge),
                    ast::CmpOp::In | ast::CmpOp::NotIn => {
                        let mut eq_count = 0usize;
                        let mut not_eq_count = 0usize;

                        for ty in rhs_elements {
                            let eq_result = self.infer_binary_type_comparison(
                                Type::Tuple(lhs),
                                ast::CmpOp::Eq,
                                *ty,
                                range,
                            ).expect("infer_binary_type_comparison should never return None for `CmpOp::Eq`");

                            match eq_result {
                                todo @ Type::Dynamic(DynamicType::Todo(_)) => return Ok(todo),
                                // It's okay to ignore errors here because Python doesn't call `__bool__`
                                // for different union variants. Instead, this is just for us to
                                // evaluate a possibly truthy value to `false` or `true`.
                                ty => match ty.bool(self.db()) {
                                    Truthiness::AlwaysTrue => eq_count += 1,
                                    Truthiness::AlwaysFalse => not_eq_count += 1,
                                    Truthiness::Ambiguous => (),
                                },
                            }
                        }

                        if eq_count >= 1 {
                            Ok(Type::BooleanLiteral(op.is_in()))
                        } else if not_eq_count == rhs_elements.len() {
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

            // Lookup the rich comparison `__dunder__` methods
            _ => {
                let rich_comparison = |op| self.infer_rich_comparison(left, right, op);
                let membership_test_comparison = |op, range: TextRange| {
                    self.infer_membership_test_comparison(left, right, op, range)
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
                        if left.is_disjoint_from(self.db(), right) {
                            Ok(Type::BooleanLiteral(false))
                        } else if left.is_singleton(self.db())
                            && left.is_equivalent_to(self.db(), right)
                        {
                            Ok(Type::BooleanLiteral(true))
                        } else {
                            Ok(KnownClass::Bool.to_instance(self.db()))
                        }
                    }
                    ast::CmpOp::IsNot => {
                        if left.is_disjoint_from(self.db(), right) {
                            Ok(Type::BooleanLiteral(true))
                        } else if left.is_singleton(self.db())
                            && left.is_equivalent_to(self.db(), right)
                        {
                            Ok(Type::BooleanLiteral(false))
                        } else {
                            Ok(KnownClass::Bool.to_instance(self.db()))
                        }
                    }
                }
            }
        }
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
    ) -> Result<Type<'db>, CompareUnsupportedError<'db>> {
        let db = self.db();
        // The following resource has details about the rich comparison algorithm:
        // https://snarky.ca/unravelling-rich-comparison-operators/
        let call_dunder = |op: RichCompareOperator, left: Type<'db>, right: Type<'db>| {
            left.try_call_dunder(db, op.dunder(), CallArgumentTypes::positional([right]))
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
            if matches!(op, RichCompareOperator::Eq | RichCompareOperator::Ne) {
                Some(KnownClass::Bool.to_instance(db))
            } else {
                None
            }
        })
        .ok_or_else(|| CompareUnsupportedError {
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
    ) -> Result<Type<'db>, CompareUnsupportedError<'db>> {
        let db = self.db();

        let contains_dunder = right.class_member(db, "__contains__".into()).symbol;
        let compare_result_opt = match contains_dunder {
            Symbol::Type(contains_dunder, Boundness::Bound) => {
                // If `__contains__` is available, it is used directly for the membership test.
                contains_dunder
                    .try_call(db, CallArgumentTypes::positional([right, left]))
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
            .ok_or_else(|| CompareUnsupportedError {
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
        left: &[Type<'db>],
        op: RichCompareOperator,
        right: &[Type<'db>],
        range: TextRange,
    ) -> Result<Type<'db>, CompareUnsupportedError<'db>> {
        let left_iter = left.iter().copied();
        let right_iter = right.iter().copied();

        let mut builder = UnionBuilder::new(self.db());

        for (l_ty, r_ty) in left_iter.zip(right_iter) {
            let pairwise_eq_result = self
                .infer_binary_type_comparison(l_ty, ast::CmpOp::Eq, r_ty, range)
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
                        | RichCompareOperator::Ge => {
                            self.infer_binary_type_comparison(l_ty, op.into(), r_ty, range)?
                        }
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
            range: _,
            value,
            slice,
            ctx: _,
        } = subscript;

        // HACK ALERT: If we are subscripting a generic class, short-circuit the rest of the
        // subscript inference logic and treat this as an explicit specialization.
        // TODO: Move this logic into a custom callable, and update `find_name_in_mro` to return
        // this callable as the `__class_getitem__` method on `type`. That probably requires
        // updating all of the subscript logic below to use custom callables for all of the _other_
        // special cases, too.
        let value_ty = self.infer_expression(value);
        if let Type::ClassLiteral(class) = value_ty {
            if let Some(generic_context) = class.generic_context(self.db()) {
                return self.infer_explicit_class_specialization(
                    subscript,
                    value_ty,
                    class,
                    generic_context,
                );
            }
        }

        let slice_ty = self.infer_expression(slice);
        self.infer_subscript_expression_types(value, value_ty, slice_ty)
    }

    fn infer_explicit_class_specialization(
        &mut self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        generic_class: ClassLiteral<'db>,
        generic_context: GenericContext<'db>,
    ) -> Type<'db> {
        let slice_node = subscript.slice.as_ref();
        let mut call_argument_types = match slice_node {
            ast::Expr::Tuple(tuple) => CallArgumentTypes::positional(
                tuple.elts.iter().map(|elt| self.infer_type_expression(elt)),
            ),
            _ => CallArgumentTypes::positional([self.infer_type_expression(slice_node)]),
        };
        let signatures = Signatures::single(CallableSignature::single(
            value_ty,
            generic_context.signature(self.db()),
        ));
        let bindings = match Bindings::match_parameters(signatures, &mut call_argument_types)
            .check_types(self.db(), &mut call_argument_types)
        {
            Ok(bindings) => bindings,
            Err(CallError(_, bindings)) => {
                bindings.report_diagnostics(&self.context, subscript.into());
                return Type::unknown();
            }
        };
        let callable = bindings
            .into_iter()
            .next()
            .expect("valid bindings should have one callable");
        let (_, overload) = callable
            .matching_overload()
            .expect("valid bindings should have matching overload");
        let specialization = generic_context.specialize(
            self.db(),
            overload
                .parameter_types()
                .iter()
                .map(|ty| ty.unwrap_or(Type::unknown()))
                .collect(),
        );
        Type::from(GenericAlias::new(self.db(), generic_class, specialization))
    }

    fn infer_subscript_expression_types(
        &mut self,
        value_node: &ast::Expr,
        value_ty: Type<'db>,
        slice_ty: Type<'db>,
    ) -> Type<'db> {
        match (value_ty, slice_ty) {
            (
                Type::NominalInstance(instance),
                Type::IntLiteral(_) | Type::BooleanLiteral(_) | Type::SliceLiteral(_),
            ) if instance
                .class()
                .is_known(self.db(), KnownClass::VersionInfo) =>
            {
                self.infer_subscript_expression_types(
                    value_node,
                    Type::version_info_tuple(self.db()),
                    slice_ty,
                )
            }

            // Ex) Given `("a", "b", "c", "d")[1]`, return `"b"`
            (Type::Tuple(tuple_ty), Type::IntLiteral(int)) if i32::try_from(int).is_ok() => {
                let elements = tuple_ty.elements(self.db());
                elements
                    .iter()
                    .py_index(i32::try_from(int).expect("checked in branch arm"))
                    .copied()
                    .unwrap_or_else(|_| {
                        report_index_out_of_bounds(
                            &self.context,
                            "tuple",
                            value_node.into(),
                            value_ty,
                            elements.len(),
                            int,
                        );
                        Type::unknown()
                    })
            }
            // Ex) Given `("a", 1, Null)[0:2]`, return `("a", 1)`
            (Type::Tuple(tuple_ty), Type::SliceLiteral(slice_ty)) => {
                let elements = tuple_ty.elements(self.db());
                let (start, stop, step) = slice_ty.as_tuple(self.db());

                if let Ok(new_elements) = elements.py_slice(start, stop, step) {
                    TupleType::from_elements(self.db(), new_elements)
                } else {
                    report_slice_step_size_zero(&self.context, value_node.into());
                    Type::unknown()
                }
            }
            // Ex) Given `"value"[1]`, return `"a"`
            (Type::StringLiteral(literal_ty), Type::IntLiteral(int))
                if i32::try_from(int).is_ok() =>
            {
                let literal_value = literal_ty.value(self.db());
                literal_value
                    .chars()
                    .py_index(i32::try_from(int).expect("checked in branch arm"))
                    .map(|ch| Type::string_literal(self.db(), &ch.to_string()))
                    .unwrap_or_else(|_| {
                        report_index_out_of_bounds(
                            &self.context,
                            "string",
                            value_node.into(),
                            value_ty,
                            literal_value.chars().count(),
                            int,
                        );
                        Type::unknown()
                    })
            }
            // Ex) Given `"value"[1:3]`, return `"al"`
            (Type::StringLiteral(literal_ty), Type::SliceLiteral(slice_ty)) => {
                let literal_value = literal_ty.value(self.db());
                let (start, stop, step) = slice_ty.as_tuple(self.db());

                let chars: Vec<_> = literal_value.chars().collect();
                let result = if let Ok(new_chars) = chars.py_slice(start, stop, step) {
                    let literal: String = new_chars.collect();
                    Type::string_literal(self.db(), &literal)
                } else {
                    report_slice_step_size_zero(&self.context, value_node.into());
                    Type::unknown()
                };
                result
            }
            // Ex) Given `b"value"[1]`, return `b"a"`
            (Type::BytesLiteral(literal_ty), Type::IntLiteral(int))
                if i32::try_from(int).is_ok() =>
            {
                let literal_value = literal_ty.value(self.db());
                literal_value
                    .iter()
                    .py_index(i32::try_from(int).expect("checked in branch arm"))
                    .map(|byte| Type::bytes_literal(self.db(), &[*byte]))
                    .unwrap_or_else(|_| {
                        report_index_out_of_bounds(
                            &self.context,
                            "bytes literal",
                            value_node.into(),
                            value_ty,
                            literal_value.len(),
                            int,
                        );
                        Type::unknown()
                    })
            }
            // Ex) Given `b"value"[1:3]`, return `b"al"`
            (Type::BytesLiteral(literal_ty), Type::SliceLiteral(slice_ty)) => {
                let literal_value = literal_ty.value(self.db());
                let (start, stop, step) = slice_ty.as_tuple(self.db());

                if let Ok(new_bytes) = literal_value.py_slice(start, stop, step) {
                    let new_bytes: Vec<u8> = new_bytes.copied().collect();
                    Type::bytes_literal(self.db(), &new_bytes)
                } else {
                    report_slice_step_size_zero(&self.context, value_node.into());
                    Type::unknown()
                }
            }
            // Ex) Given `"value"[True]`, return `"a"`
            (
                Type::Tuple(_) | Type::StringLiteral(_) | Type::BytesLiteral(_),
                Type::BooleanLiteral(bool),
            ) => self.infer_subscript_expression_types(
                value_node,
                value_ty,
                Type::IntLiteral(i64::from(bool)),
            ),
            (Type::KnownInstance(KnownInstanceType::Protocol), _) => {
                Type::Dynamic(DynamicType::SubscriptedProtocol)
            }
            (Type::KnownInstance(KnownInstanceType::Generic), _) => {
                Type::Dynamic(DynamicType::SubscriptedGeneric)
            }
            (Type::KnownInstance(known_instance), _)
                if known_instance.class().is_special_form() =>
            {
                todo_type!("Inference of subscript on special form")
            }
            (value_ty, slice_ty) => {
                // If the class defines `__getitem__`, return its return type.
                //
                // See: https://docs.python.org/3/reference/datamodel.html#class-getitem-versus-getitem
                match value_ty.try_call_dunder(
                    self.db(),
                    "__getitem__",
                    CallArgumentTypes::positional([slice_ty]),
                ) {
                    Ok(outcome) => return outcome.return_type(self.db()),
                    Err(err @ CallDunderError::PossiblyUnbound { .. }) => {
                        if let Some(builder) = self
                            .context
                            .report_lint(&CALL_POSSIBLY_UNBOUND_METHOD, value_node)
                        {
                            builder.into_diagnostic(format_args!(
                                "Method `__getitem__` of type `{}` is possibly unbound",
                                value_ty.display(self.db()),
                            ));
                        }

                        return err.fallback_return_type(self.db());
                    }
                    Err(CallDunderError::CallError(_, bindings)) => {
                        if let Some(builder) =
                            self.context.report_lint(&CALL_NON_CALLABLE, value_node)
                        {
                            builder.into_diagnostic(format_args!(
                                "Method `__getitem__` of type `{}` \
                                 is not callable on object of type `{}`",
                                bindings.callable_type().display(self.db()),
                                value_ty.display(self.db()),
                            ));
                        }

                        return bindings.return_type(self.db());
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
                if value_ty.is_subtype_of(self.db(), KnownClass::Type.to_instance(self.db())) {
                    let dunder_class_getitem_method =
                        value_ty.member(self.db(), "__class_getitem__").symbol;

                    match dunder_class_getitem_method {
                        Symbol::Unbound => {}
                        Symbol::Type(ty, boundness) => {
                            if boundness == Boundness::PossiblyUnbound {
                                if let Some(builder) = self
                                    .context
                                    .report_lint(&CALL_POSSIBLY_UNBOUND_METHOD, value_node)
                                {
                                    builder.into_diagnostic(format_args!(
                                        "Method `__class_getitem__` of type `{}` \
                                        is possibly unbound",
                                        value_ty.display(self.db()),
                                    ));
                                }
                            }

                            match ty.try_call(
                                self.db(),
                                CallArgumentTypes::positional([value_ty, slice_ty]),
                            ) {
                                Ok(bindings) => return bindings.return_type(self.db()),
                                Err(CallError(_, bindings)) => {
                                    if let Some(builder) =
                                        self.context.report_lint(&CALL_NON_CALLABLE, value_node)
                                    {
                                        builder.into_diagnostic(format_args!(
                                            "Method `__class_getitem__` of type `{}` \
                                             is not callable on object of type `{}`",
                                            bindings.callable_type().display(self.db()),
                                            value_ty.display(self.db()),
                                        ));
                                    }
                                    return bindings.return_type(self.db());
                                }
                            }
                        }
                    }

                    if let Type::ClassLiteral(class) = value_ty {
                        if class.is_known(self.db(), KnownClass::Type) {
                            return KnownClass::GenericAlias.to_instance(self.db());
                        }

                        if class.generic_context(self.db()).is_some() {
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
                    if !value_ty.into_class_literal().is_some_and(|class| {
                        class
                            .iter_mro(self.db(), None)
                            .contains(&ClassBase::Dynamic(DynamicType::SubscriptedGeneric))
                    }) {
                        report_non_subscriptable(
                            &self.context,
                            value_node.into(),
                            value_ty,
                            "__class_getitem__",
                        );
                    }
                } else {
                    report_non_subscriptable(
                        &self.context,
                        value_node.into(),
                        value_ty,
                        "__getitem__",
                    );
                }

                match value_ty {
                    Type::ClassLiteral(_) => {
                        // TODO: proper support for generic classes
                        // For now, just infer `Sequence`, if we see something like `Sequence[str]`. This allows us
                        // to look up attributes on generic base classes, even if we don't understand generics yet.
                        // Note that this isn't handled by the clause up above for generic classes
                        // that use legacy type variables and an explicit `Generic` base class.
                        // Once we handle legacy typevars, this special case will be removed in
                        // favor of the specialization logic above.
                        value_ty
                    }
                    _ => Type::unknown(),
                }
            }
        }
    }

    fn infer_slice_expression(&mut self, slice: &ast::ExprSlice) -> Type<'db> {
        enum SliceArg {
            Arg(Option<i32>),
            Unsupported,
        }

        let ast::ExprSlice {
            range: _,
            lower,
            upper,
            step,
        } = slice;

        let ty_lower = self.infer_optional_expression(lower.as_deref());
        let ty_upper = self.infer_optional_expression(upper.as_deref());
        let ty_step = self.infer_optional_expression(step.as_deref());

        let type_to_slice_argument = |ty: Option<Type<'db>>| match ty {
            Some(Type::IntLiteral(n)) => match i32::try_from(n) {
                Ok(n) => SliceArg::Arg(Some(n)),
                Err(_) => SliceArg::Unsupported,
            },
            Some(Type::BooleanLiteral(b)) => SliceArg::Arg(Some(i32::from(b))),
            Some(Type::NominalInstance(instance))
                if instance.class().is_known(self.db(), KnownClass::NoneType) =>
            {
                SliceArg::Arg(None)
            }
            None => SliceArg::Arg(None),
            _ => SliceArg::Unsupported,
        };

        match (
            type_to_slice_argument(ty_lower),
            type_to_slice_argument(ty_upper),
            type_to_slice_argument(ty_step),
        ) {
            (SliceArg::Arg(lower), SliceArg::Arg(upper), SliceArg::Arg(step)) => {
                Type::SliceLiteral(SliceLiteralType::new(self.db(), lower, upper, step))
            }
            _ => KnownClass::Slice.to_instance(self.db()),
        }
    }

    fn infer_type_parameters(&mut self, type_parameters: &ast::TypeParams) {
        let ast::TypeParams {
            range: _,
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

    pub(super) fn finish(mut self) -> TypeInference<'db> {
        self.infer_region();
        self.types.diagnostics = self.context.finish();
        self.types.shrink_to_fit();
        self.types
    }
}

/// Annotation expressions.
impl<'db> TypeInferenceBuilder<'db> {
    /// Infer the type of an annotation expression with the given [`DeferredExpressionState`].
    fn infer_annotation_expression(
        &mut self,
        annotation: &ast::Expr,
        deferred_state: DeferredExpressionState,
    ) -> TypeAndQualifiers<'db> {
        let previous_deferred_state = std::mem::replace(&mut self.deferred_state, deferred_state);
        let annotation_ty = self.infer_annotation_expression_impl(annotation);
        self.deferred_state = previous_deferred_state;
        annotation_ty
    }

    /// Similar to [`infer_annotation_expression`], but accepts an optional annotation expression
    /// and returns [`None`] if the annotation is [`None`].
    ///
    /// [`infer_annotation_expression`]: TypeInferenceBuilder::infer_annotation_expression
    fn infer_optional_annotation_expression(
        &mut self,
        annotation: Option<&ast::Expr>,
        deferred_state: DeferredExpressionState,
    ) -> Option<TypeAndQualifiers<'db>> {
        annotation.map(|expr| self.infer_annotation_expression(expr, deferred_state))
    }

    /// Implementation of [`infer_annotation_expression`].
    ///
    /// [`infer_annotation_expression`]: TypeInferenceBuilder::infer_annotation_expression
    fn infer_annotation_expression_impl(
        &mut self,
        annotation: &ast::Expr,
    ) -> TypeAndQualifiers<'db> {
        // https://typing.python.org/en/latest/spec/annotations.html#grammar-token-expression-grammar-annotation_expression
        let annotation_ty = match annotation {
            // String annotations: https://typing.python.org/en/latest/spec/annotations.html#string-annotations
            ast::Expr::StringLiteral(string) => self.infer_string_annotation_expression(string),

            // Annotation expressions also get special handling for `*args` and `**kwargs`.
            ast::Expr::Starred(starred) => self.infer_starred_expression(starred).into(),

            ast::Expr::BytesLiteral(bytes) => {
                if let Some(builder) = self
                    .context
                    .report_lint(&BYTE_STRING_TYPE_ANNOTATION, bytes)
                {
                    builder.into_diagnostic("Type expressions cannot use bytes literal");
                }
                TypeAndQualifiers::unknown()
            }

            ast::Expr::FString(fstring) => {
                if let Some(builder) = self.context.report_lint(&FSTRING_TYPE_ANNOTATION, fstring) {
                    builder.into_diagnostic("Type expressions cannot use f-strings");
                }
                self.infer_fstring_expression(fstring);
                TypeAndQualifiers::unknown()
            }

            ast::Expr::Name(name) => match name.ctx {
                ast::ExprContext::Load => {
                    let name_expr_ty = self.infer_name_expression(name);
                    match name_expr_ty {
                        Type::KnownInstance(KnownInstanceType::ClassVar) => {
                            TypeAndQualifiers::new(Type::unknown(), TypeQualifiers::CLASS_VAR)
                        }
                        Type::KnownInstance(KnownInstanceType::Final) => {
                            TypeAndQualifiers::new(Type::unknown(), TypeQualifiers::FINAL)
                        }
                        _ => name_expr_ty
                            .in_type_expression(self.db())
                            .unwrap_or_else(|error| {
                                error.into_fallback_type(
                                    &self.context,
                                    annotation,
                                    self.is_reachable(annotation),
                                )
                            })
                            .into(),
                    }
                }
                ast::ExprContext::Invalid => TypeAndQualifiers::unknown(),
                ast::ExprContext::Store | ast::ExprContext::Del => {
                    todo_type!("Name expression annotation in Store/Del context").into()
                }
            },

            ast::Expr::Subscript(subscript @ ast::ExprSubscript { value, slice, .. }) => {
                let value_ty = self.infer_expression(value);

                let slice = &**slice;

                match value_ty {
                    Type::KnownInstance(KnownInstanceType::Annotated) => {
                        // This branch is similar to the corresponding branch in `infer_parameterized_known_instance_type_expression`, but
                        // `Annotated[…]` can appear both in annotation expressions and in type expressions, and needs to be handled slightly
                        // differently in each case (calling either `infer_type_expression_*` or `infer_annotation_expression_*`).
                        if let ast::Expr::Tuple(ast::ExprTuple {
                            elts: arguments, ..
                        }) = slice
                        {
                            if arguments.len() < 2 {
                                report_invalid_arguments_to_annotated(&self.context, subscript);
                            }

                            if let [inner_annotation, metadata @ ..] = &arguments[..] {
                                for element in metadata {
                                    self.infer_expression(element);
                                }

                                let inner_annotation_ty =
                                    self.infer_annotation_expression_impl(inner_annotation);

                                self.store_expression_type(slice, inner_annotation_ty.inner_type());
                                inner_annotation_ty
                            } else {
                                self.infer_type_expression(slice);
                                TypeAndQualifiers::unknown()
                            }
                        } else {
                            report_invalid_arguments_to_annotated(&self.context, subscript);
                            self.infer_annotation_expression_impl(slice)
                        }
                    }
                    Type::KnownInstance(
                        known_instance @ (KnownInstanceType::ClassVar | KnownInstanceType::Final),
                    ) => match slice {
                        ast::Expr::Tuple(..) => {
                            if let Some(builder) =
                                self.context.report_lint(&INVALID_TYPE_FORM, subscript)
                            {
                                builder.into_diagnostic(format_args!(
                                    "Type qualifier `{type_qualifier}` \
                                     expects exactly one type parameter",
                                    type_qualifier = known_instance.repr(),
                                ));
                            }
                            Type::unknown().into()
                        }
                        _ => {
                            let mut type_and_qualifiers =
                                self.infer_annotation_expression_impl(slice);
                            match known_instance {
                                KnownInstanceType::ClassVar => {
                                    type_and_qualifiers.add_qualifier(TypeQualifiers::CLASS_VAR);
                                }
                                KnownInstanceType::Final => {
                                    type_and_qualifiers.add_qualifier(TypeQualifiers::FINAL);
                                }
                                _ => unreachable!(),
                            }
                            type_and_qualifiers
                        }
                    },
                    _ => self
                        .infer_subscript_type_expression_no_store(subscript, slice, value_ty)
                        .into(),
                }
            }

            // All other annotation expressions are (possibly) valid type expressions, so handle
            // them there instead.
            type_expr => self.infer_type_expression_no_store(type_expr).into(),
        };

        self.store_expression_type(annotation, annotation_ty.inner_type());

        annotation_ty
    }

    /// Infer the type of a string annotation expression.
    fn infer_string_annotation_expression(
        &mut self,
        string: &ast::ExprStringLiteral,
    ) -> TypeAndQualifiers<'db> {
        match parse_string_annotation(&self.context, string) {
            Some(parsed) => {
                // String annotations are always evaluated in the deferred context.
                self.infer_annotation_expression(
                    parsed.expr(),
                    DeferredExpressionState::InStringAnnotation(
                        self.enclosing_node_key(string.into()),
                    ),
                )
            }
            None => TypeAndQualifiers::unknown(),
        }
    }
}

/// Type expressions
impl<'db> TypeInferenceBuilder<'db> {
    /// Infer the type of a type expression.
    fn infer_type_expression(&mut self, expression: &ast::Expr) -> Type<'db> {
        let ty = self.infer_type_expression_no_store(expression);
        self.store_expression_type(expression, ty);
        ty
    }

    /// Similar to [`infer_type_expression`], but accepts an optional type expression and returns
    /// [`None`] if the expression is [`None`].
    ///
    /// [`infer_type_expression`]: TypeInferenceBuilder::infer_type_expression
    fn infer_optional_type_expression(
        &mut self,
        expression: Option<&ast::Expr>,
    ) -> Option<Type<'db>> {
        expression.map(|expr| self.infer_type_expression(expr))
    }

    /// Similar to [`infer_type_expression`], but accepts a [`DeferredExpressionState`].
    ///
    /// [`infer_type_expression`]: TypeInferenceBuilder::infer_type_expression
    fn infer_type_expression_with_state(
        &mut self,
        expression: &ast::Expr,
        deferred_state: DeferredExpressionState,
    ) -> Type<'db> {
        let previous_deferred_state = std::mem::replace(&mut self.deferred_state, deferred_state);
        let annotation_ty = self.infer_type_expression(expression);
        self.deferred_state = previous_deferred_state;
        annotation_ty
    }

    fn report_invalid_type_expression(
        &mut self,
        expression: &ast::Expr,
        message: std::fmt::Arguments,
    ) -> Type<'db> {
        if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, expression) {
            builder.into_diagnostic(message);
        }
        Type::unknown()
    }

    /// Infer the type of a type expression without storing the result.
    fn infer_type_expression_no_store(&mut self, expression: &ast::Expr) -> Type<'db> {
        // https://typing.python.org/en/latest/spec/annotations.html#grammar-token-expression-grammar-type_expression
        match expression {
            ast::Expr::Name(name) => match name.ctx {
                ast::ExprContext::Load => self
                    .infer_name_expression(name)
                    .in_type_expression(self.db())
                    .unwrap_or_else(|error| {
                        error.into_fallback_type(
                            &self.context,
                            expression,
                            self.is_reachable(expression),
                        )
                    }),
                ast::ExprContext::Invalid => Type::unknown(),
                ast::ExprContext::Store | ast::ExprContext::Del => {
                    todo_type!("Name expression annotation in Store/Del context")
                }
            },

            ast::Expr::Attribute(attribute_expression) => match attribute_expression.ctx {
                ast::ExprContext::Load => self
                    .infer_attribute_expression(attribute_expression)
                    .in_type_expression(self.db())
                    .unwrap_or_else(|error| {
                        error.into_fallback_type(
                            &self.context,
                            expression,
                            self.is_reachable(expression),
                        )
                    }),
                ast::ExprContext::Invalid => Type::unknown(),
                ast::ExprContext::Store | ast::ExprContext::Del => {
                    todo_type!("Attribute expression annotation in Store/Del context")
                }
            },

            ast::Expr::NoneLiteral(_literal) => Type::none(self.db()),

            // https://typing.python.org/en/latest/spec/annotations.html#string-annotations
            ast::Expr::StringLiteral(string) => self.infer_string_type_expression(string),

            ast::Expr::Subscript(subscript) => {
                let ast::ExprSubscript {
                    value,
                    slice,
                    ctx: _,
                    range: _,
                } = subscript;

                let value_ty = self.infer_expression(value);

                self.infer_subscript_type_expression_no_store(subscript, slice, value_ty)
            }

            ast::Expr::BinOp(binary) => {
                match binary.op {
                    // PEP-604 unions are okay, e.g., `int | str`
                    ast::Operator::BitOr => {
                        let left_ty = self.infer_type_expression(&binary.left);
                        let right_ty = self.infer_type_expression(&binary.right);
                        UnionType::from_elements(self.db(), [left_ty, right_ty])
                    }
                    // anything else is an invalid annotation:
                    _ => {
                        self.infer_binary_expression(binary);
                        Type::unknown()
                    }
                }
            }

            // Avoid inferring the types of invalid type expressions that have been parsed from a
            // string annotation, as they are not present in the semantic index.
            _ if self.deferred_state.in_string_annotation() => Type::unknown(),

            // =====================================================================================
            // Forms which are invalid in the context of annotation expressions: we infer their
            // nested expressions as normal expressions, but the type of the top-level expression is
            // always `Type::unknown` in these cases.
            // =====================================================================================

            // TODO: add a subdiagnostic linking to type-expression grammar
            // and stating that it is only valid in `typing.Literal[]` or `typing.Annotated[]`
            ast::Expr::BytesLiteral(_) => self.report_invalid_type_expression(
                expression,
                format_args!("Bytes literals are not allowed in this context in a type expression"),
            ),

            // TODO: add a subdiagnostic linking to type-expression grammar
            // and stating that it is only valid in `typing.Literal[]` or `typing.Annotated[]`
            ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(_),
                ..
            }) => self.report_invalid_type_expression(
                expression,
                format_args!("Int literals are not allowed in this context in a type expression"),
            ),

            ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Float(_),
                ..
            }) => self.report_invalid_type_expression(
                expression,
                format_args!("Float literals are not allowed in type expressions"),
            ),

            ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Complex { .. },
                ..
            }) => self.report_invalid_type_expression(
                expression,
                format_args!("Complex literals are not allowed in type expressions"),
            ),

            // TODO: add a subdiagnostic linking to type-expression grammar
            // and stating that it is only valid in `typing.Literal[]` or `typing.Annotated[]`
            ast::Expr::BooleanLiteral(_) => self.report_invalid_type_expression(
                expression,
                format_args!(
                    "Boolean literals are not allowed in this context in a type expression"
                ),
            ),

            // TODO: add a subdiagnostic linking to type-expression grammar
            // and stating that it is only valid as first argument to `typing.Callable[]`
            ast::Expr::List(list) => {
                self.infer_list_expression(list);
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "List literals are not allowed in this context in a type expression"
                    ),
                )
            }

            ast::Expr::BoolOp(bool_op) => {
                self.infer_boolean_expression(bool_op);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Boolean operations are not allowed in type expressions"),
                )
            }

            ast::Expr::Named(named) => {
                self.infer_named_expression(named);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Named expressions are not allowed in type expressions"),
                )
            }

            ast::Expr::UnaryOp(unary) => {
                self.infer_unary_expression(unary);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Unary operations are not allowed in type expressions"),
                )
            }

            ast::Expr::Lambda(lambda_expression) => {
                self.infer_lambda_expression(lambda_expression);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("`lambda` expressions are not allowed in type expressions"),
                )
            }

            ast::Expr::If(if_expression) => {
                self.infer_if_expression(if_expression);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("`if` expressions are not allowed in type expressions"),
                )
            }

            ast::Expr::Dict(dict) => {
                self.infer_dict_expression(dict);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Dict literals are not allowed in type expressions"),
                )
            }

            ast::Expr::Set(set) => {
                self.infer_set_expression(set);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Set literals are not allowed in type expressions"),
                )
            }

            ast::Expr::DictComp(dictcomp) => {
                self.infer_dict_comprehension_expression(dictcomp);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Dict comprehensions are not allowed in type expressions"),
                )
            }

            ast::Expr::ListComp(listcomp) => {
                self.infer_list_comprehension_expression(listcomp);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("List comprehensions are not allowed in type expressions"),
                )
            }

            ast::Expr::SetComp(setcomp) => {
                self.infer_set_comprehension_expression(setcomp);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Set comprehensions are not allowed in type expressions"),
                )
            }

            ast::Expr::Generator(generator) => {
                self.infer_generator_expression(generator);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Generator expressions are not allowed in type expressions"),
                )
            }

            ast::Expr::Await(await_expression) => {
                self.infer_await_expression(await_expression);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("`await` expressions are not allowed in type expressions"),
                )
            }

            ast::Expr::Yield(yield_expression) => {
                self.infer_yield_expression(yield_expression);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("`yield` expressions are not allowed in type expressions"),
                )
            }

            ast::Expr::YieldFrom(yield_from) => {
                self.infer_yield_from_expression(yield_from);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("`yield from` expressions are not allowed in type expressions"),
                )
            }

            ast::Expr::Compare(compare) => {
                self.infer_compare_expression(compare);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Comparison expressions are not allowed in type expressions"),
                )
            }

            ast::Expr::Call(call_expr) => {
                self.infer_call_expression(expression, call_expr);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Function calls are not allowed in type expressions"),
                )
            }

            ast::Expr::FString(fstring) => {
                self.infer_fstring_expression(fstring);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("F-strings are not allowed in type expressions"),
                )
            }

            ast::Expr::Slice(slice) => {
                self.infer_slice_expression(slice);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Slices are not allowed in type expressions"),
                )
            }

            // =================================================================================
            // Branches where we probably should emit diagnostics in some context, but don't yet
            // =================================================================================
            ast::Expr::IpyEscapeCommand(_) => todo!("Implement Ipy escape command support"),

            ast::Expr::EllipsisLiteral(_) => {
                todo_type!("ellipsis literal in type expression")
            }

            ast::Expr::Tuple(tuple) => {
                self.infer_tuple_expression(tuple);
                Type::unknown()
            }

            ast::Expr::Starred(starred) => {
                self.infer_starred_expression(starred);
                todo_type!("PEP 646")
            }
        }
    }

    fn infer_subscript_type_expression_no_store(
        &mut self,
        subscript: &ast::ExprSubscript,
        slice: &ast::Expr,
        value_ty: Type<'db>,
    ) -> Type<'db> {
        match value_ty {
            Type::ClassLiteral(class_literal) => match class_literal.known(self.db()) {
                Some(KnownClass::Tuple) => self.infer_tuple_type_expression(slice),
                Some(KnownClass::Type) => self.infer_subclass_of_type_expression(slice),
                _ => self.infer_subscript_type_expression(subscript, value_ty),
            },
            _ => self.infer_subscript_type_expression(subscript, value_ty),
        }
    }

    /// Infer the type of a string type expression.
    fn infer_string_type_expression(&mut self, string: &ast::ExprStringLiteral) -> Type<'db> {
        match parse_string_annotation(&self.context, string) {
            Some(parsed) => {
                // String annotations are always evaluated in the deferred context.
                self.infer_type_expression_with_state(
                    parsed.expr(),
                    DeferredExpressionState::InStringAnnotation(
                        self.enclosing_node_key(string.into()),
                    ),
                )
            }
            None => Type::unknown(),
        }
    }

    /// Given the slice of a `tuple[]` annotation, return the type that the annotation represents
    fn infer_tuple_type_expression(&mut self, tuple_slice: &ast::Expr) -> Type<'db> {
        /// In most cases, if a subelement of the tuple is inferred as `Todo`,
        /// we should only infer `Todo` for that specific subelement.
        /// Certain specific AST nodes can however change the meaning of the entire tuple,
        /// however: for example, `tuple[int, ...]` or `tuple[int, *tuple[str, ...]]` are a
        /// homogeneous tuple and a partly homogeneous tuple (respectively) due to the `...`
        /// and the starred expression (respectively), Neither is supported by us right now,
        /// so we should infer `Todo` for the *entire* tuple if we encounter one of those elements.
        fn element_could_alter_type_of_whole_tuple(
            element: &ast::Expr,
            element_ty: Type,
            builder: &TypeInferenceBuilder,
        ) -> bool {
            if !element_ty.is_todo() {
                return false;
            }

            match element {
                ast::Expr::EllipsisLiteral(_) | ast::Expr::Starred(_) => true,
                ast::Expr::Subscript(ast::ExprSubscript { value, .. }) => {
                    matches!(
                        builder.expression_type(value),
                        Type::KnownInstance(KnownInstanceType::Unpack)
                    )
                }
                _ => false,
            }
        }

        // TODO:
        // - homogeneous tuples
        // - PEP 646
        match tuple_slice {
            ast::Expr::Tuple(elements) => {
                let mut element_types = Vec::with_capacity(elements.len());

                // Whether to infer `Todo` for the whole tuple
                // (see docstring for `element_could_alter_type_of_whole_tuple`)
                let mut return_todo = false;

                for element in elements {
                    let element_ty = self.infer_type_expression(element);
                    return_todo |=
                        element_could_alter_type_of_whole_tuple(element, element_ty, self);
                    element_types.push(element_ty);
                }

                let ty = if return_todo {
                    todo_type!("full tuple[...] support")
                } else {
                    TupleType::from_elements(self.db(), element_types)
                };

                // Here, we store the type for the inner `int, str` tuple-expression,
                // while the type for the outer `tuple[int, str]` slice-expression is
                // stored in the surrounding `infer_type_expression` call:
                self.store_expression_type(tuple_slice, ty);

                ty
            }
            single_element => {
                let single_element_ty = self.infer_type_expression(single_element);
                if element_could_alter_type_of_whole_tuple(single_element, single_element_ty, self)
                {
                    todo_type!("full tuple[...] support")
                } else {
                    TupleType::from_elements(self.db(), std::iter::once(single_element_ty))
                }
            }
        }
    }

    /// Given the slice of a `type[]` annotation, return the type that the annotation represents
    fn infer_subclass_of_type_expression(&mut self, slice: &ast::Expr) -> Type<'db> {
        match slice {
            ast::Expr::Name(_) | ast::Expr::Attribute(_) => {
                let name_ty = self.infer_expression(slice);
                match name_ty {
                    Type::ClassLiteral(class_literal) => {
                        if class_literal.is_known(self.db(), KnownClass::Any) {
                            SubclassOfType::subclass_of_any()
                        } else {
                            SubclassOfType::from(
                                self.db(),
                                class_literal.default_specialization(self.db()),
                            )
                        }
                    }
                    Type::KnownInstance(KnownInstanceType::Any) => {
                        SubclassOfType::subclass_of_any()
                    }
                    Type::KnownInstance(KnownInstanceType::Unknown) => {
                        SubclassOfType::subclass_of_unknown()
                    }
                    _ => todo_type!("unsupported type[X] special form"),
                }
            }
            ast::Expr::BinOp(binary) if binary.op == ast::Operator::BitOr => {
                let union_ty = UnionType::from_elements(
                    self.db(),
                    [
                        self.infer_subclass_of_type_expression(&binary.left),
                        self.infer_subclass_of_type_expression(&binary.right),
                    ],
                );
                self.store_expression_type(slice, union_ty);

                union_ty
            }
            ast::Expr::Tuple(_) => {
                self.infer_type_expression(slice);
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, slice) {
                    builder.into_diagnostic("type[...] must have exactly one type argument");
                }
                Type::unknown()
            }
            ast::Expr::Subscript(ast::ExprSubscript {
                value,
                slice: parameters,
                ..
            }) => {
                let parameters_ty = match self.infer_expression(value) {
                    Type::KnownInstance(KnownInstanceType::Union) => match &**parameters {
                        ast::Expr::Tuple(tuple) => {
                            let ty = UnionType::from_elements(
                                self.db(),
                                tuple
                                    .iter()
                                    .map(|element| self.infer_subclass_of_type_expression(element)),
                            );
                            self.store_expression_type(parameters, ty);
                            ty
                        }
                        _ => self.infer_subclass_of_type_expression(parameters),
                    },
                    _ => {
                        self.infer_type_expression(parameters);
                        todo_type!("unsupported nested subscript in type[X]")
                    }
                };
                self.store_expression_type(slice, parameters_ty);
                parameters_ty
            }
            // TODO: subscripts, etc.
            _ => {
                self.infer_type_expression(slice);
                todo_type!("unsupported type[X] special form")
            }
        }
    }

    fn infer_subscript_type_expression(
        &mut self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
    ) -> Type<'db> {
        let ast::ExprSubscript {
            range: _,
            value: _,
            slice,
            ctx: _,
        } = subscript;

        match value_ty {
            Type::ClassLiteral(literal) if literal.is_known(self.db(), KnownClass::Any) => {
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    builder.into_diagnostic("Type `typing.Any` expected no type parameter");
                }
                Type::unknown()
            }
            Type::KnownInstance(known_instance) => {
                self.infer_parameterized_known_instance_type_expression(subscript, known_instance)
            }
            Type::Dynamic(DynamicType::Todo(_)) => {
                self.infer_type_expression(slice);
                value_ty
            }
            Type::ClassLiteral(class) => {
                match class.generic_context(self.db()) {
                    Some(generic_context) => {
                        let specialized_class = self.infer_explicit_class_specialization(
                            subscript,
                            value_ty,
                            class,
                            generic_context,
                        );
                        specialized_class
                            .in_type_expression(self.db())
                            .unwrap_or(Type::unknown())
                    }
                    None => {
                        // TODO: Once we know that e.g. `list` is generic, emit a diagnostic if you try to
                        // specialize a non-generic class.
                        self.infer_type_expression(slice);
                        todo_type!("specialized non-generic class")
                    }
                }
            }
            _ => {
                // TODO: Emit a diagnostic once we've implemented all valid subscript type
                // expressions.
                self.infer_type_expression(slice);
                todo_type!("unknown type subscript")
            }
        }
    }

    fn infer_parameterized_known_instance_type_expression(
        &mut self,
        subscript: &ast::ExprSubscript,
        known_instance: KnownInstanceType,
    ) -> Type<'db> {
        let db = self.db();
        let arguments_slice = &*subscript.slice;
        match known_instance {
            KnownInstanceType::Annotated => {
                let ast::Expr::Tuple(ast::ExprTuple {
                    elts: arguments, ..
                }) = arguments_slice
                else {
                    report_invalid_arguments_to_annotated(&self.context, subscript);

                    // `Annotated[]` with less than two arguments is an error at runtime.
                    // However, we still treat `Annotated[T]` as `T` here for the purpose of
                    // giving better diagnostics later on.
                    // Pyright also does this. Mypy doesn't; it falls back to `Any` instead.
                    return self.infer_type_expression(arguments_slice);
                };

                if arguments.len() < 2 {
                    report_invalid_arguments_to_annotated(&self.context, subscript);
                }

                let [type_expr, metadata @ ..] = &arguments[..] else {
                    self.infer_type_expression(arguments_slice);
                    return Type::unknown();
                };

                for element in metadata {
                    self.infer_expression(element);
                }

                let ty = self.infer_type_expression(type_expr);
                self.store_expression_type(arguments_slice, ty);
                ty
            }
            KnownInstanceType::Literal => {
                match self.infer_literal_parameter_type(arguments_slice) {
                    Ok(ty) => ty,
                    Err(nodes) => {
                        for node in nodes {
                            if let Some(builder) =
                                self.context.report_lint(&INVALID_TYPE_FORM, node)
                            {
                                builder.into_diagnostic(
                                    "Type arguments for `Literal` must be `None`, \
                                     a literal value (int, bool, str, or bytes), or an enum value",
                                );
                            }
                        }
                        Type::unknown()
                    }
                }
            }
            KnownInstanceType::Optional => {
                let param_type = self.infer_type_expression(arguments_slice);
                UnionType::from_elements(db, [param_type, Type::none(db)])
            }
            KnownInstanceType::Union => match arguments_slice {
                ast::Expr::Tuple(t) => {
                    let union_ty = UnionType::from_elements(
                        db,
                        t.iter().map(|elt| self.infer_type_expression(elt)),
                    );
                    self.store_expression_type(arguments_slice, union_ty);
                    union_ty
                }
                _ => self.infer_type_expression(arguments_slice),
            },
            KnownInstanceType::TypeVar(_) => {
                self.infer_type_expression(arguments_slice);
                todo_type!("TypeVar annotations")
            }
            KnownInstanceType::TypeAliasType(_) => {
                self.infer_type_expression(arguments_slice);
                todo_type!("Generic PEP-695 type alias")
            }
            KnownInstanceType::Callable => {
                let mut arguments = match arguments_slice {
                    ast::Expr::Tuple(tuple) => Either::Left(tuple.iter()),
                    _ => {
                        self.infer_callable_parameter_types(arguments_slice);
                        Either::Right(std::iter::empty::<&ast::Expr>())
                    }
                };

                let first_argument = arguments.next();

                let parameters =
                    first_argument.and_then(|arg| self.infer_callable_parameter_types(arg));

                let return_type = arguments.next().map(|arg| self.infer_type_expression(arg));

                let correct_argument_number = if let Some(third_argument) = arguments.next() {
                    self.infer_type_expression(third_argument);
                    for argument in arguments {
                        self.infer_type_expression(argument);
                    }
                    false
                } else {
                    return_type.is_some()
                };

                if !correct_argument_number {
                    report_invalid_arguments_to_callable(&self.context, subscript);
                }

                let callable_type = if let (Some(parameters), Some(return_type), true) =
                    (parameters, return_type, correct_argument_number)
                {
                    CallableType::single(db, Signature::new(parameters, Some(return_type)))
                } else {
                    CallableType::unknown(db)
                };

                let callable_type = Type::Callable(callable_type);

                // `Signature` / `Parameters` are not a `Type` variant, so we're storing
                // the outer callable type on the these expressions instead.
                self.store_expression_type(arguments_slice, callable_type);
                if let Some(first_argument) = first_argument {
                    self.store_expression_type(first_argument, callable_type);
                }

                callable_type
            }

            // Type API special forms
            KnownInstanceType::Not => match arguments_slice {
                ast::Expr::Tuple(_) => {
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "Special form `{}` expected exactly one type parameter",
                            known_instance.repr()
                        ));
                    }
                    Type::unknown()
                }
                _ => {
                    let argument_type = self.infer_type_expression(arguments_slice);
                    argument_type.negate(db)
                }
            },
            KnownInstanceType::Intersection => {
                let elements = match arguments_slice {
                    ast::Expr::Tuple(tuple) => Either::Left(tuple.iter()),
                    element => Either::Right(std::iter::once(element)),
                };

                elements
                    .fold(IntersectionBuilder::new(db), |builder, element| {
                        builder.add_positive(self.infer_type_expression(element))
                    })
                    .build()
            }
            KnownInstanceType::TypeOf => match arguments_slice {
                ast::Expr::Tuple(_) => {
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "Special form `{}` expected exactly one type parameter",
                            known_instance.repr()
                        ));
                    }
                    Type::unknown()
                }
                _ => {
                    // NB: This calls `infer_expression` instead of `infer_type_expression`.
                    let argument_type = self.infer_expression(arguments_slice);
                    argument_type
                }
            },
            KnownInstanceType::CallableTypeOf => match arguments_slice {
                ast::Expr::Tuple(_) => {
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "Special form `{}` expected exactly one type parameter",
                            known_instance.repr()
                        ));
                    }
                    Type::unknown()
                }
                _ => {
                    let argument_type = self.infer_expression(arguments_slice);
                    let signatures = argument_type.signatures(db);

                    // SAFETY: This is enforced by the constructor methods on `Signatures` even in
                    // the case of a non-callable union.
                    let callable_signature = signatures
                        .iter()
                        .next()
                        .expect("`Signatures` should have at least one `CallableSignature`");

                    let mut signature_iter = callable_signature.iter().map(|signature| {
                        if argument_type.is_bound_method() {
                            signature.bind_self()
                        } else {
                            signature.clone()
                        }
                    });

                    let Some(signature) = signature_iter.next() else {
                        if let Some(builder) = self
                            .context
                            .report_lint(&INVALID_TYPE_FORM, arguments_slice)
                        {
                            builder.into_diagnostic(format_args!(
                                "Expected the first argument to `{}` \
                                 to be a callable object, \
                                 but got an object of type `{}`",
                                known_instance.repr(),
                                argument_type.display(db)
                            ));
                        }
                        return Type::unknown();
                    };

                    Type::Callable(CallableType::from_overloads(
                        db,
                        std::iter::once(signature).chain(signature_iter),
                    ))
                }
            },

            // TODO: Generics
            KnownInstanceType::ChainMap => {
                self.infer_type_expression(arguments_slice);
                KnownClass::ChainMap.to_instance(db)
            }
            KnownInstanceType::OrderedDict => {
                self.infer_type_expression(arguments_slice);
                KnownClass::OrderedDict.to_instance(db)
            }
            KnownInstanceType::Dict => {
                self.infer_type_expression(arguments_slice);
                KnownClass::Dict.to_instance(db)
            }
            KnownInstanceType::List => {
                self.infer_type_expression(arguments_slice);
                KnownClass::List.to_instance(db)
            }
            KnownInstanceType::DefaultDict => {
                self.infer_type_expression(arguments_slice);
                KnownClass::DefaultDict.to_instance(db)
            }
            KnownInstanceType::Counter => {
                self.infer_type_expression(arguments_slice);
                KnownClass::Counter.to_instance(db)
            }
            KnownInstanceType::Set => {
                self.infer_type_expression(arguments_slice);
                KnownClass::Set.to_instance(db)
            }
            KnownInstanceType::FrozenSet => {
                self.infer_type_expression(arguments_slice);
                KnownClass::FrozenSet.to_instance(db)
            }
            KnownInstanceType::Deque => {
                self.infer_type_expression(arguments_slice);
                KnownClass::Deque.to_instance(db)
            }

            KnownInstanceType::ReadOnly => {
                self.infer_type_expression(arguments_slice);
                todo_type!("`ReadOnly[]` type qualifier")
            }
            KnownInstanceType::NotRequired => {
                self.infer_type_expression(arguments_slice);
                todo_type!("`NotRequired[]` type qualifier")
            }
            KnownInstanceType::ClassVar | KnownInstanceType::Final => {
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    builder.into_diagnostic(format_args!(
                        "Type qualifier `{}` is not allowed in type expressions \
                         (only in annotation expressions)",
                        known_instance.repr()
                    ));
                }
                self.infer_type_expression(arguments_slice)
            }
            KnownInstanceType::Required => {
                self.infer_type_expression(arguments_slice);
                todo_type!("`Required[]` type qualifier")
            }
            KnownInstanceType::TypeIs => {
                self.infer_type_expression(arguments_slice);
                todo_type!("`TypeIs[]` special form")
            }
            KnownInstanceType::TypeGuard => {
                self.infer_type_expression(arguments_slice);
                todo_type!("`TypeGuard[]` special form")
            }
            KnownInstanceType::Concatenate => {
                self.infer_type_expression(arguments_slice);
                todo_type!("`Concatenate[]` special form")
            }
            KnownInstanceType::Unpack => {
                self.infer_type_expression(arguments_slice);
                todo_type!("`Unpack[]` special form")
            }
            KnownInstanceType::Protocol => {
                self.infer_type_expression(arguments_slice);
                Type::Dynamic(DynamicType::SubscriptedProtocol)
            }
            KnownInstanceType::Generic => {
                self.infer_type_expression(arguments_slice);
                Type::Dynamic(DynamicType::SubscriptedGeneric)
            }
            KnownInstanceType::NoReturn
            | KnownInstanceType::Never
            | KnownInstanceType::Any
            | KnownInstanceType::AlwaysTruthy
            | KnownInstanceType::AlwaysFalsy => {
                self.infer_type_expression(arguments_slice);

                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    builder.into_diagnostic(format_args!(
                        "Type `{}` expected no type parameter",
                        known_instance.repr()
                    ));
                }
                Type::unknown()
            }
            KnownInstanceType::TypingSelf
            | KnownInstanceType::TypeAlias
            | KnownInstanceType::TypedDict
            | KnownInstanceType::Unknown => {
                self.infer_type_expression(arguments_slice);

                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    builder.into_diagnostic(format_args!(
                        "Special form `{}` expected no type parameter",
                        known_instance.repr()
                    ));
                }
                Type::unknown()
            }
            KnownInstanceType::LiteralString => {
                self.infer_type_expression(arguments_slice);

                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    let mut diag = builder.into_diagnostic(format_args!(
                        "Type `{}` expected no type parameter",
                        known_instance.repr()
                    ));
                    diag.info("Did you mean to use `Literal[...]` instead?");
                }
                Type::unknown()
            }
            KnownInstanceType::Type => self.infer_subclass_of_type_expression(arguments_slice),
            KnownInstanceType::Tuple => self.infer_tuple_type_expression(arguments_slice),
        }
    }

    fn infer_literal_parameter_type<'ast>(
        &mut self,
        parameters: &'ast ast::Expr,
    ) -> Result<Type<'db>, Vec<&'ast ast::Expr>> {
        Ok(match parameters {
            // TODO handle type aliases
            ast::Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                let value_ty = self.infer_expression(value);
                if matches!(value_ty, Type::KnownInstance(KnownInstanceType::Literal)) {
                    let ty = self.infer_literal_parameter_type(slice)?;

                    // This branch deals with annotations such as `Literal[Literal[1]]`.
                    // Here, we store the type for the inner `Literal[1]` expression:
                    self.store_expression_type(parameters, ty);
                    ty
                } else {
                    self.store_expression_type(parameters, Type::unknown());

                    return Err(vec![parameters]);
                }
            }
            ast::Expr::Tuple(tuple) if !tuple.parenthesized => {
                let mut errors = vec![];
                let mut builder = UnionBuilder::new(self.db());
                for elt in tuple {
                    match self.infer_literal_parameter_type(elt) {
                        Ok(ty) => {
                            builder = builder.add(ty);
                        }
                        Err(nodes) => {
                            errors.extend(nodes);
                        }
                    }
                }
                if errors.is_empty() {
                    let union_type = builder.build();

                    // This branch deals with annotations such as `Literal[1, 2]`. Here, we
                    // store the type for the inner `1, 2` tuple-expression:
                    self.store_expression_type(parameters, union_type);

                    union_type
                } else {
                    self.store_expression_type(parameters, Type::unknown());

                    return Err(errors);
                }
            }

            literal @ (ast::Expr::StringLiteral(_)
            | ast::Expr::BytesLiteral(_)
            | ast::Expr::BooleanLiteral(_)
            | ast::Expr::NoneLiteral(_)) => self.infer_expression(literal),
            literal @ ast::Expr::NumberLiteral(ref number) if number.value.is_int() => {
                self.infer_expression(literal)
            }
            // For enum values
            ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
                let value_ty = self.infer_expression(value);
                // TODO: Check that value type is enum otherwise return None
                value_ty
                    .member(self.db(), &attr.id)
                    .symbol
                    .ignore_possibly_unbound()
                    .unwrap_or(Type::unknown())
            }
            // for negative and positive numbers
            ast::Expr::UnaryOp(ref u)
                if matches!(u.op, ast::UnaryOp::USub | ast::UnaryOp::UAdd)
                    && u.operand.is_number_literal_expr() =>
            {
                self.infer_unary_expression(u)
            }
            _ => {
                self.infer_expression(parameters);
                return Err(vec![parameters]);
            }
        })
    }

    /// Infer the first argument to a `typing.Callable` type expression and returns the
    /// corresponding [`Parameters`].
    ///
    /// It returns `None` if the argument is invalid i.e., not a list of types, parameter
    /// specification, `typing.Concatenate`, or `...`.
    fn infer_callable_parameter_types(
        &mut self,
        parameters: &ast::Expr,
    ) -> Option<Parameters<'db>> {
        Some(match parameters {
            ast::Expr::EllipsisLiteral(ast::ExprEllipsisLiteral { .. }) => {
                Parameters::gradual_form()
            }
            ast::Expr::List(ast::ExprList { elts: params, .. }) => {
                let mut parameter_types = Vec::with_capacity(params.len());

                // Whether to infer `Todo` for the parameters
                let mut return_todo = false;

                for param in params {
                    let param_type = self.infer_type_expression(param);
                    // This is similar to what we currently do for inferring tuple type expression.
                    // We currently infer `Todo` for the parameters to avoid invalid diagnostics
                    // when trying to check for assignability or any other relation. For example,
                    // `*tuple[int, str]`, `Unpack[]`, etc. are not yet supported.
                    return_todo |= param_type.is_todo()
                        && matches!(param, ast::Expr::Starred(_) | ast::Expr::Subscript(_));
                    parameter_types.push(param_type);
                }

                if return_todo {
                    // TODO: `Unpack`
                    Parameters::todo()
                } else {
                    Parameters::new(parameter_types.iter().map(|param_type| {
                        Parameter::positional_only(None).with_annotated_type(*param_type)
                    }))
                }
            }
            ast::Expr::Subscript(_) => {
                // TODO: Support `Concatenate[...]`
                Parameters::todo()
            }
            ast::Expr::Name(name) if name.is_invalid() => {
                // This is a special case to avoid raising the error suggesting what the first
                // argument should be. This only happens when there's already a syntax error like
                // `Callable[]`.
                return None;
            }
            _ => {
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, parameters) {
                    // TODO: Check whether `Expr::Name` is a ParamSpec
                    builder.into_diagnostic(format_args!(
                        "The first argument to `Callable` \
                         must be either a list of types, \
                         ParamSpec, Concatenate, or `...`",
                    ));
                }
                return None;
            }
        })
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompareUnsupportedError<'db> {
    op: ast::CmpOp,
    left_ty: Type<'db>,
    right_ty: Type<'db>,
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

    fn string_type(self, db: &dyn Db) -> Type {
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

#[cfg(test)]
mod tests {
    use crate::db::tests::{setup_db, TestDb};
    use crate::semantic_index::definition::Definition;
    use crate::semantic_index::symbol::FileScopeId;
    use crate::semantic_index::{global_scope, semantic_index, symbol_table, use_def_map};
    use crate::symbol::global_symbol;
    use crate::types::check_types;
    use ruff_db::diagnostic::Diagnostic;
    use ruff_db::files::{system_path_to_file, File};
    use ruff_db::system::DbWithWritableSystem as _;
    use ruff_db::testing::{assert_function_query_was_not_run, assert_function_query_was_run};

    use super::*;

    #[track_caller]
    fn get_symbol<'db>(
        db: &'db TestDb,
        file_name: &str,
        scopes: &[&str],
        symbol_name: &str,
    ) -> Symbol<'db> {
        let file = system_path_to_file(db, file_name).expect("file to exist");
        let index = semantic_index(db, file);
        let mut file_scope_id = FileScopeId::global();
        let mut scope = file_scope_id.to_scope_id(db, file);
        for expected_scope_name in scopes {
            file_scope_id = index
                .child_scopes(file_scope_id)
                .next()
                .unwrap_or_else(|| panic!("scope of {expected_scope_name}"))
                .0;
            scope = file_scope_id.to_scope_id(db, file);
            assert_eq!(scope.name(db), *expected_scope_name);
        }

        symbol(db, scope, symbol_name).symbol
    }

    #[track_caller]
    fn assert_diagnostic_messages(diagnostics: &TypeCheckDiagnostics, expected: &[&str]) {
        let messages: Vec<&str> = diagnostics
            .iter()
            .map(Diagnostic::primary_message)
            .collect();
        assert_eq!(&messages, expected);
    }

    #[track_caller]
    fn assert_file_diagnostics(db: &TestDb, filename: &str, expected: &[&str]) {
        let file = system_path_to_file(db, filename).unwrap();
        let diagnostics = check_types(db, file);

        assert_diagnostic_messages(diagnostics, expected);
    }

    #[test]
    fn not_literal_string() -> anyhow::Result<()> {
        let mut db = setup_db();
        let content = format!(
            r#"
            from typing_extensions import Literal, assert_type

            assert_type(not "{y}", bool)
            assert_type(not 10*"{y}", bool)
            assert_type(not "{y}"*10, bool)
            assert_type(not 0*"{y}", Literal[True])
            assert_type(not (-100)*"{y}", Literal[True])
            "#,
            y = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1),
        );
        db.write_dedented("src/a.py", &content)?;

        assert_file_diagnostics(&db, "src/a.py", &[]);

        Ok(())
    }

    #[test]
    fn multiplied_string() -> anyhow::Result<()> {
        let mut db = setup_db();
        let content = format!(
            r#"
            from typing_extensions import Literal, LiteralString, assert_type

            assert_type(2 * "hello", Literal["hellohello"])
            assert_type("goodbye" * 3, Literal["goodbyegoodbyegoodbye"])
            assert_type("a" * {y}, Literal["{a_repeated}"])
            assert_type({z} * "b", LiteralString)
            assert_type(0 * "hello", Literal[""])
            assert_type(-3 * "hello", Literal[""])
            "#,
            y = TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE,
            z = TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1,
            a_repeated = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE),
        );
        db.write_dedented("src/a.py", &content)?;

        assert_file_diagnostics(&db, "src/a.py", &[]);

        Ok(())
    }

    #[test]
    fn multiplied_literal_string() -> anyhow::Result<()> {
        let mut db = setup_db();
        let content = format!(
            r#"
            from typing_extensions import Literal, LiteralString, assert_type

            assert_type("{y}", LiteralString)
            assert_type(10*"{y}", LiteralString)
            assert_type("{y}"*10, LiteralString)
            assert_type(0*"{y}", Literal[""])
            assert_type((-100)*"{y}", Literal[""])
            "#,
            y = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1),
        );
        db.write_dedented("src/a.py", &content)?;

        assert_file_diagnostics(&db, "src/a.py", &[]);

        Ok(())
    }

    #[test]
    fn truncated_string_literals_become_literal_string() -> anyhow::Result<()> {
        let mut db = setup_db();
        let content = format!(
            r#"
            from typing_extensions import LiteralString, assert_type

            assert_type("{y}", LiteralString)
            assert_type("a" + "{z}", LiteralString)
            "#,
            y = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1),
            z = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE),
        );
        db.write_dedented("src/a.py", &content)?;

        assert_file_diagnostics(&db, "src/a.py", &[]);

        Ok(())
    }

    #[test]
    fn adding_string_literals_and_literal_string() -> anyhow::Result<()> {
        let mut db = setup_db();
        let content = format!(
            r#"
            from typing_extensions import LiteralString, assert_type

            assert_type("{y}", LiteralString)
            assert_type("{y}" + "a", LiteralString)
            assert_type("a" + "{y}", LiteralString)
            assert_type("{y}" + "{y}", LiteralString)
            "#,
            y = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1),
        );
        db.write_dedented("src/a.py", &content)?;

        assert_file_diagnostics(&db, "src/a.py", &[]);

        Ok(())
    }

    #[test]
    fn pep695_type_params() {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            def f[T, U: A, V: (A, B), W = A, X: A = A1, Y: (int,)]():
                pass

            class A: ...
            class B: ...
            class A1(A): ...
            ",
        )
        .unwrap();

        let check_typevar = |var: &'static str,
                             upper_bound: Option<&'static str>,
                             constraints: Option<&[&'static str]>,
                             default: Option<&'static str>| {
            let var_ty = get_symbol(&db, "src/a.py", &["f"], var).expect_type();
            assert_eq!(var_ty.display(&db).to_string(), "typing.TypeVar");

            let expected_name_ty = format!(r#"Literal["{var}"]"#);
            let name_ty = var_ty.member(&db, "__name__").symbol.expect_type();
            assert_eq!(name_ty.display(&db).to_string(), expected_name_ty);

            let KnownInstanceType::TypeVar(typevar) = var_ty.expect_known_instance() else {
                panic!("expected TypeVar");
            };

            assert_eq!(
                typevar
                    .upper_bound(&db)
                    .map(|ty| ty.display(&db).to_string()),
                upper_bound.map(std::borrow::ToOwned::to_owned)
            );
            assert_eq!(
                typevar.constraints(&db).map(|tys| tys
                    .iter()
                    .map(|ty| ty.display(&db).to_string())
                    .collect::<Vec<_>>()),
                constraints.map(|strings| strings
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>())
            );
            assert_eq!(
                typevar
                    .default_ty(&db)
                    .map(|ty| ty.display(&db).to_string()),
                default.map(std::borrow::ToOwned::to_owned)
            );
        };

        check_typevar("T", None, None, None);
        check_typevar("U", Some("A"), None, None);
        check_typevar("V", None, Some(&["A", "B"]), None);
        check_typevar("W", None, None, Some("A"));
        check_typevar("X", Some("A"), None, Some("A1"));

        // a typevar with less than two constraints is treated as unconstrained
        check_typevar("Y", None, None, None);
    }

    /// Test that a symbol known to be unbound in a scope does not still trigger cycle-causing
    /// visibility-constraint checks in that scope.
    #[test]
    fn unbound_symbol_no_visibility_constraint_check() {
        let mut db = setup_db();

        // If the bug we are testing for is not fixed, what happens is that when inferring the
        // `flag: bool = True` definitions, we look up `bool` as a deferred name (thus from end of
        // scope), and because of the early return its "unbound" binding has a visibility
        // constraint of `~flag`, which we evaluate, meaning we have to evaluate the definition of
        // `flag` -- and we are in a cycle. With the fix, we short-circuit evaluating visibility
        // constraints on "unbound" if a symbol is otherwise not bound.
        db.write_dedented(
            "src/a.py",
            "
            from __future__ import annotations

            def f():
                flag: bool = True
                if flag:
                    return True
            ",
        )
        .unwrap();

        db.clear_salsa_events();
        assert_file_diagnostics(&db, "src/a.py", &[]);
        let events = db.take_salsa_events();
        let cycles = salsa::attach(&db, || {
            events
                .iter()
                .filter_map(|event| {
                    if let salsa::EventKind::WillIterateCycle { database_key, .. } = event.kind {
                        Some(format!("{database_key:?}"))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        });
        let expected: Vec<String> = vec![];
        assert_eq!(cycles, expected);
    }

    // Incremental inference tests
    #[track_caller]
    fn first_public_binding<'db>(db: &'db TestDb, file: File, name: &str) -> Definition<'db> {
        let scope = global_scope(db, file);
        use_def_map(db, scope)
            .public_bindings(symbol_table(db, scope).symbol_id_by_name(name).unwrap())
            .find_map(|b| b.binding)
            .expect("no binding found")
    }

    #[test]
    fn dependency_public_symbol_type_change() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("/src/a.py", "from foo import x"),
            ("/src/foo.py", "x: int = 10\ndef foo(): ..."),
        ])?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let x_ty = global_symbol(&db, a, "x").symbol.expect_type();

        assert_eq!(x_ty.display(&db).to_string(), "int");

        // Change `x` to a different value
        db.write_file("/src/foo.py", "x: bool = True\ndef foo(): ...")?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        let x_ty_2 = global_symbol(&db, a, "x").symbol.expect_type();

        assert_eq!(x_ty_2.display(&db).to_string(), "bool");

        Ok(())
    }

    #[test]
    fn dependency_internal_symbol_change() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("/src/a.py", "from foo import x"),
            ("/src/foo.py", "x: int = 10\ndef foo(): y = 1"),
        ])?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let x_ty = global_symbol(&db, a, "x").symbol.expect_type();

        assert_eq!(x_ty.display(&db).to_string(), "int");

        db.write_file("/src/foo.py", "x: int = 10\ndef foo(): pass")?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        db.clear_salsa_events();

        let x_ty_2 = global_symbol(&db, a, "x").symbol.expect_type();

        assert_eq!(x_ty_2.display(&db).to_string(), "int");

        let events = db.take_salsa_events();

        assert_function_query_was_not_run(
            &db,
            infer_definition_types,
            first_public_binding(&db, a, "x"),
            &events,
        );

        Ok(())
    }

    #[test]
    fn dependency_unrelated_symbol() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("/src/a.py", "from foo import x"),
            ("/src/foo.py", "x: int = 10\ny: bool = True"),
        ])?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let x_ty = global_symbol(&db, a, "x").symbol.expect_type();

        assert_eq!(x_ty.display(&db).to_string(), "int");

        db.write_file("/src/foo.py", "x: int = 10\ny: bool = False")?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        db.clear_salsa_events();

        let x_ty_2 = global_symbol(&db, a, "x").symbol.expect_type();

        assert_eq!(x_ty_2.display(&db).to_string(), "int");

        let events = db.take_salsa_events();

        assert_function_query_was_not_run(
            &db,
            infer_definition_types,
            first_public_binding(&db, a, "x"),
            &events,
        );
        Ok(())
    }

    #[test]
    fn dependency_implicit_instance_attribute() -> anyhow::Result<()> {
        fn x_rhs_expression(db: &TestDb) -> Expression<'_> {
            let file_main = system_path_to_file(db, "/src/main.py").unwrap();
            let ast = parsed_module(db, file_main);
            // Get the second statement in `main.py` (x = …) and extract the expression
            // node on the right-hand side:
            let x_rhs_node = &ast.syntax().body[1].as_assign_stmt().unwrap().value;

            let index = semantic_index(db, file_main);
            index.expression(x_rhs_node.as_ref())
        }

        let mut db = setup_db();

        db.write_dedented(
            "/src/mod.py",
            r#"
            class C:
                def f(self):
                    self.attr: int | None = None
            "#,
        )?;
        db.write_dedented(
            "/src/main.py",
            r#"
            from mod import C
            x = C().attr
            "#,
        )?;

        let file_main = system_path_to_file(&db, "/src/main.py").unwrap();
        let attr_ty = global_symbol(&db, file_main, "x").symbol.expect_type();
        assert_eq!(attr_ty.display(&db).to_string(), "Unknown | int | None");

        // Change the type of `attr` to `str | None`; this should trigger the type of `x` to be re-inferred
        db.write_dedented(
            "/src/mod.py",
            r#"
            class C:
                def f(self):
                    self.attr: str | None = None
            "#,
        )?;

        let events = {
            db.clear_salsa_events();
            let attr_ty = global_symbol(&db, file_main, "x").symbol.expect_type();
            assert_eq!(attr_ty.display(&db).to_string(), "Unknown | str | None");
            db.take_salsa_events()
        };
        assert_function_query_was_run(&db, infer_expression_types, x_rhs_expression(&db), &events);

        // Add a comment; this should not trigger the type of `x` to be re-inferred
        db.write_dedented(
            "/src/mod.py",
            r#"
            class C:
                def f(self):
                    # a comment!
                    self.attr: str | None = None
            "#,
        )?;

        let events = {
            db.clear_salsa_events();
            let attr_ty = global_symbol(&db, file_main, "x").symbol.expect_type();
            assert_eq!(attr_ty.display(&db).to_string(), "Unknown | str | None");
            db.take_salsa_events()
        };

        assert_function_query_was_not_run(
            &db,
            infer_expression_types,
            x_rhs_expression(&db),
            &events,
        );

        Ok(())
    }

    /// This test verifies that changing a class's declaration in a non-meaningful way (e.g. by adding a comment)
    /// doesn't trigger type inference for expressions that depend on the class's members.
    #[test]
    fn dependency_own_instance_member() -> anyhow::Result<()> {
        fn x_rhs_expression(db: &TestDb) -> Expression<'_> {
            let file_main = system_path_to_file(db, "/src/main.py").unwrap();
            let ast = parsed_module(db, file_main);
            // Get the second statement in `main.py` (x = …) and extract the expression
            // node on the right-hand side:
            let x_rhs_node = &ast.syntax().body[1].as_assign_stmt().unwrap().value;

            let index = semantic_index(db, file_main);
            index.expression(x_rhs_node.as_ref())
        }

        let mut db = setup_db();

        db.write_dedented(
            "/src/mod.py",
            r#"
            class C:
                if random.choice([True, False]):
                    attr: int = 42
                else:
                    attr: None = None
            "#,
        )?;
        db.write_dedented(
            "/src/main.py",
            r#"
            from mod import C
            x = C().attr
            "#,
        )?;

        let file_main = system_path_to_file(&db, "/src/main.py").unwrap();
        let attr_ty = global_symbol(&db, file_main, "x").symbol.expect_type();
        assert_eq!(attr_ty.display(&db).to_string(), "Unknown | int | None");

        // Change the type of `attr` to `str | None`; this should trigger the type of `x` to be re-inferred
        db.write_dedented(
            "/src/mod.py",
            r#"
            class C:
                if random.choice([True, False]):
                    attr: str = "42"
                else:
                    attr: None = None
            "#,
        )?;

        let events = {
            db.clear_salsa_events();
            let attr_ty = global_symbol(&db, file_main, "x").symbol.expect_type();
            assert_eq!(attr_ty.display(&db).to_string(), "Unknown | str | None");
            db.take_salsa_events()
        };
        assert_function_query_was_run(&db, infer_expression_types, x_rhs_expression(&db), &events);

        // Add a comment; this should not trigger the type of `x` to be re-inferred
        db.write_dedented(
            "/src/mod.py",
            r#"
            class C:
                # comment
                if random.choice([True, False]):
                    attr: str = "42"
                else:
                    attr: None = None
            "#,
        )?;

        let events = {
            db.clear_salsa_events();
            let attr_ty = global_symbol(&db, file_main, "x").symbol.expect_type();
            assert_eq!(attr_ty.display(&db).to_string(), "Unknown | str | None");
            db.take_salsa_events()
        };

        assert_function_query_was_not_run(
            &db,
            infer_expression_types,
            x_rhs_expression(&db),
            &events,
        );

        Ok(())
    }
}
