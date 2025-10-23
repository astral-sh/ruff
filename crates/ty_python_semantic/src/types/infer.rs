//! We have Salsa queries for inferring types at three different granularities: scope-level,
//! definition-level, and expression-level.
//!
//! Scope-level inference is for when we are actually checking a file, and need to check types for
//! everything in that file's scopes, or give a linter access to types of arbitrary expressions
//! (via the [`HasType`](crate::semantic_model::HasType) trait).
//!
//! Definition-level inference allows us to look up the types of places in other scopes (e.g. for
//! imports) with the minimum inference necessary, so that if we're looking up one place from a
//! very large module, we can avoid a bunch of unnecessary work. Definition-level inference also
//! allows us to handle import cycles without getting into a cycle of scope-level inference
//! queries.
//!
//! The expression-level inference query is needed in only a few cases. Since some assignments can
//! have multiple targets (via `x = y = z` or unpacking `(x, y) = z`, they can be associated with
//! multiple definitions (one per assigned place). In order to avoid inferring the type of the
//! right-hand side once per definition, we infer it as a standalone query, so its result will be
//! cached by Salsa. We also need the expression-level query for inferring types in type guard
//! expressions (e.g. the test clause of an `if` statement.)
//!
//! Inferring types at any of the three region granularities returns a [`ExpressionInference`],
//! [`DefinitionInference`], or [`ScopeInference`], which hold the types for every expression
//! within the inferred region. Some inference types also expose the type of every definition
//! within the inferred region.
//!
//! Some type expressions can require deferred evaluation. This includes all type expressions in
//! stub files, or annotation expressions in modules with `from __future__ import annotations`, or
//! stringified annotations. We have a fourth Salsa query for inferring the deferred types
//! associated with a particular definition. Scope-level inference infers deferred types for all
//! definitions once the rest of the types in the scope have been inferred.
//!
//! Many of our type inference Salsa queries implement cycle recovery via fixed-point iteration. In
//! general, they initiate fixed-point iteration by returning an `Inference` type that returns
//! `Type::Never` for all expressions, bindings, and declarations, and then they continue iterating
//! the query cycle until a fixed-point is reached. Salsa has a built-in fixed limit on the number
//! of iterations, so if we fail to converge, Salsa will eventually panic. (This should of course
//! be considered a bug.)

use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;
use rustc_hash::FxHashMap;
use salsa;
use salsa::plumbing::AsId;

use crate::Db;
use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::scope::ScopeId;
use crate::semantic_index::{SemanticIndex, semantic_index};
use crate::types::diagnostic::TypeCheckDiagnostics;
use crate::types::function::FunctionType;
use crate::types::generics::Specialization;
use crate::types::unpacker::{UnpackResult, Unpacker};
use crate::types::{ClassLiteral, KnownClass, Truthiness, Type, TypeAndQualifiers};
use crate::unpack::Unpack;
use builder::TypeInferenceBuilder;

mod builder;
#[cfg(test)]
mod tests;

/// How many fixpoint iterations to allow before falling back to Divergent type.
const ITERATIONS_BEFORE_FALLBACK: u32 = 10;

/// Infer all types for a [`ScopeId`], including all definitions and expressions in that scope.
/// Use when checking a scope, or needing to provide a type for an arbitrary expression in the
/// scope.
#[salsa::tracked(returns(ref), cycle_initial=scope_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn infer_scope_types<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> ScopeInference<'db> {
    let file = scope.file(db);
    let _span = tracing::trace_span!("infer_scope_types", scope=?scope.as_id(), ?file).entered();

    let module = parsed_module(db, file).load(db);

    // Using the index here is fine because the code below depends on the AST anyway.
    // The isolation of the query is by the return inferred types.
    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(db, InferenceRegion::Scope(scope), index, &module).finish_scope()
}

fn scope_cycle_initial<'db>(_db: &'db dyn Db, scope: ScopeId<'db>) -> ScopeInference<'db> {
    ScopeInference::cycle_initial(scope)
}

/// Infer all types for a [`Definition`] (including sub-expressions).
/// Use when resolving a place use or public type of a place.
#[salsa::tracked(returns(ref), cycle_fn=definition_cycle_recover, cycle_initial=definition_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn infer_definition_types<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> DefinitionInference<'db> {
    let file = definition.file(db);
    let module = parsed_module(db, file).load(db);
    let _span = tracing::trace_span!(
        "infer_definition_types",
        range = ?definition.kind(db).target_range(&module),
        ?file
    )
    .entered();

    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(db, InferenceRegion::Definition(definition), index, &module)
        .finish_definition()
}

fn definition_cycle_recover<'db>(
    db: &'db dyn Db,
    _id: salsa::Id,
    _last_provisional_value: &DefinitionInference<'db>,
    _value: &DefinitionInference<'db>,
    count: u32,
    definition: Definition<'db>,
) -> salsa::CycleRecoveryAction<DefinitionInference<'db>> {
    if count == ITERATIONS_BEFORE_FALLBACK {
        salsa::CycleRecoveryAction::Fallback(DefinitionInference::cycle_fallback(
            definition.scope(db),
        ))
    } else {
        salsa::CycleRecoveryAction::Iterate
    }
}

fn definition_cycle_initial<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> DefinitionInference<'db> {
    DefinitionInference::cycle_initial(definition.scope(db))
}

/// Infer types for all deferred type expressions in a [`Definition`].
///
/// Deferred expressions are type expressions (annotations, base classes, aliases...) in a stub
/// file, or in a file with `from __future__ import annotations`, or stringified annotations.
#[salsa::tracked(returns(ref), cycle_initial=deferred_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn infer_deferred_types<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> DefinitionInference<'db> {
    let file = definition.file(db);
    let module = parsed_module(db, file).load(db);
    let _span = tracing::trace_span!(
        "infer_deferred_types",
        definition = ?definition.as_id(),
        range = ?definition.kind(db).target_range(&module),
        ?file
    )
    .entered();

    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(db, InferenceRegion::Deferred(definition), index, &module)
        .finish_definition()
}

fn deferred_cycle_initial<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> DefinitionInference<'db> {
    DefinitionInference::cycle_initial(definition.scope(db))
}

/// Infer all types for an [`Expression`] (including sub-expressions).
/// Use rarely; only for cases where we'd otherwise risk double-inferring an expression: RHS of an
/// assignment, which might be unpacking/multi-target and thus part of multiple definitions, or a
/// type narrowing guard expression (e.g. if statement test node).
pub(crate) fn infer_expression_types<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
    tcx: TypeContext<'db>,
) -> &'db ExpressionInference<'db> {
    infer_expression_types_impl(db, InferExpression::new(db, expression, tcx))
}

#[salsa::tracked(returns(ref), cycle_fn=expression_cycle_recover, cycle_initial=expression_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
fn infer_expression_types_impl<'db>(
    db: &'db dyn Db,
    input: InferExpression<'db>,
) -> ExpressionInference<'db> {
    let (expression, tcx) = (input.expression(db), input.tcx(db));

    let file = expression.file(db);
    let module = parsed_module(db, file).load(db);
    let _span = tracing::trace_span!(
        "infer_expression_types",
        expression = ?expression.as_id(),
        range = ?expression.node_ref(db, &module).range(),
        ?file
    )
    .entered();

    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(
        db,
        InferenceRegion::Expression(expression, tcx),
        index,
        &module,
    )
    .finish_expression()
}

/// Infer the type of an expression in isolation.
///
/// The type returned by this function may be different than the type of the expression
/// if it was inferred within its region, as it does not account for surrounding type context.
/// This can be useful to re-infer the type of an expression for diagnostics.
pub(crate) fn infer_isolated_expression<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    expr: &ast::Expr,
) -> Type<'db> {
    let file = scope.file(db);
    let module = parsed_module(db, file).load(db);
    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(db, InferenceRegion::Scope(scope), index, &module)
        .infer_isolated_expression(expr)
}

fn expression_cycle_recover<'db>(
    db: &'db dyn Db,
    _id: salsa::Id,
    _last_provisional_value: &ExpressionInference<'db>,
    _value: &ExpressionInference<'db>,
    count: u32,
    input: InferExpression<'db>,
) -> salsa::CycleRecoveryAction<ExpressionInference<'db>> {
    if count == ITERATIONS_BEFORE_FALLBACK {
        salsa::CycleRecoveryAction::Fallback(ExpressionInference::cycle_fallback(
            input.expression(db).scope(db),
        ))
    } else {
        salsa::CycleRecoveryAction::Iterate
    }
}

fn expression_cycle_initial<'db>(
    db: &'db dyn Db,
    input: InferExpression<'db>,
) -> ExpressionInference<'db> {
    ExpressionInference::cycle_initial(input.expression(db).scope(db))
}

/// Infers the type of an `expression` that is guaranteed to be in the same file as the calling query.
///
/// This is a small helper around [`infer_expression_types()`] to reduce the boilerplate.
/// Use [`infer_expression_type()`] if it isn't guaranteed that `expression` is in the same file to
/// avoid cross-file query dependencies.
pub(super) fn infer_same_file_expression_type<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
    tcx: TypeContext<'db>,
    parsed: &ParsedModuleRef,
) -> Type<'db> {
    let inference = infer_expression_types(db, expression, tcx);
    inference.expression_type(expression.node_ref(db, parsed))
}

/// Infers the type of an expression where the expression might come from another file.
///
/// Use this over [`infer_expression_types`] if the expression might come from another file than the
/// enclosing query to avoid cross-file query dependencies.
///
/// Use [`infer_same_file_expression_type`] if it is guaranteed that  `expression` is in the same
/// to avoid unnecessary salsa ingredients. This is normally the case inside the `TypeInferenceBuilder`.
pub(crate) fn infer_expression_type<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
    tcx: TypeContext<'db>,
) -> Type<'db> {
    infer_expression_type_impl(db, InferExpression::new(db, expression, tcx))
}

#[salsa::tracked(cycle_initial=single_expression_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
fn infer_expression_type_impl<'db>(db: &'db dyn Db, input: InferExpression<'db>) -> Type<'db> {
    let file = input.expression(db).file(db);
    let module = parsed_module(db, file).load(db);

    // It's okay to call the "same file" version here because we're inside a salsa query.
    let inference = infer_expression_types_impl(db, input);
    inference.expression_type(input.expression(db).node_ref(db, &module))
}

fn single_expression_cycle_initial<'db>(
    _db: &'db dyn Db,
    _input: InferExpression<'db>,
) -> Type<'db> {
    Type::Never
}

/// An `Expression` with an optional `TypeContext`.
///
/// This is a Salsa supertype used as the input to `infer_expression_types` to avoid
/// interning an `ExpressionWithContext` unnecessarily when no type context is provided.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, salsa::Supertype, salsa::Update)]
enum InferExpression<'db> {
    Bare(Expression<'db>),
    WithContext(ExpressionWithContext<'db>),
}

impl<'db> InferExpression<'db> {
    fn new(
        db: &'db dyn Db,
        expression: Expression<'db>,
        tcx: TypeContext<'db>,
    ) -> InferExpression<'db> {
        if tcx.annotation.is_some() {
            InferExpression::WithContext(ExpressionWithContext::new(db, expression, tcx))
        } else {
            // Drop the empty `TypeContext` to avoid the interning cost.
            InferExpression::Bare(expression)
        }
    }

    fn expression(self, db: &'db dyn Db) -> Expression<'db> {
        match self {
            InferExpression::Bare(expression) => expression,
            InferExpression::WithContext(expression_with_context) => {
                expression_with_context.expression(db)
            }
        }
    }

    fn tcx(self, db: &'db dyn Db) -> TypeContext<'db> {
        match self {
            InferExpression::Bare(_) => TypeContext::default(),
            InferExpression::WithContext(expression_with_context) => {
                expression_with_context.tcx(db)
            }
        }
    }
}

/// An `Expression` with a `TypeContext`.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct ExpressionWithContext<'db> {
    expression: Expression<'db>,
    tcx: TypeContext<'db>,
}

/// The type context for a given expression, namely the type annotation
/// in an annotated assignment.
///
/// Knowing the outer type context when inferring an expression can enable
/// more precise inference results, aka "bidirectional type inference".
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) struct TypeContext<'db> {
    pub(crate) annotation: Option<Type<'db>>,
}

impl<'db> TypeContext<'db> {
    pub(crate) fn new(annotation: Option<Type<'db>>) -> Self {
        Self { annotation }
    }

    // If the type annotation is a specialized instance of the given `KnownClass`, returns the
    // specialization.
    fn known_specialization(
        &self,
        db: &'db dyn Db,
        known_class: KnownClass,
    ) -> Option<Specialization<'db>> {
        self.annotation
            .and_then(|ty| ty.known_specialization(db, known_class))
    }

    pub(crate) fn map(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Self {
        Self {
            annotation: self.annotation.map(f),
        }
    }
}

/// Returns the statically-known truthiness of a given expression.
///
/// Returns [`Truthiness::Ambiguous`] in case any non-definitely bound places
/// were encountered while inferring the type of the expression.
#[salsa::tracked(cycle_initial=static_expression_truthiness_cycle_initial, heap_size=get_size2::GetSize::get_heap_size)]
pub(crate) fn static_expression_truthiness<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> Truthiness {
    let inference = infer_expression_types_impl(db, InferExpression::Bare(expression));

    if !inference.all_places_definitely_bound() {
        return Truthiness::Ambiguous;
    }

    let file = expression.file(db);
    let module = parsed_module(db, file).load(db);
    let node = expression.node_ref(db, &module);

    inference.expression_type(node).bool(db)
}

fn static_expression_truthiness_cycle_initial<'db>(
    _db: &'db dyn Db,
    _expression: Expression<'db>,
) -> Truthiness {
    Truthiness::Ambiguous
}

/// Infer the types for an [`Unpack`] operation.
///
/// This infers the expression type and performs structural match against the target expression
/// involved in an unpacking operation. It returns a result-like object that can be used to get the
/// type of the variables involved in this unpacking along with any violations that are detected
/// during this unpacking.
#[salsa::tracked(returns(ref), cycle_initial=unpack_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
pub(super) fn infer_unpack_types<'db>(db: &'db dyn Db, unpack: Unpack<'db>) -> UnpackResult<'db> {
    let file = unpack.file(db);
    let module = parsed_module(db, file).load(db);
    let _span = tracing::trace_span!("infer_unpack_types", range=?unpack.range(db, &module), ?file)
        .entered();

    let mut unpacker = Unpacker::new(db, unpack.target_scope(db), &module);
    unpacker.unpack(unpack.target(db, &module), unpack.value(db));
    unpacker.finish()
}

fn unpack_cycle_initial<'db>(_db: &'db dyn Db, _unpack: Unpack<'db>) -> UnpackResult<'db> {
    UnpackResult::cycle_initial(Type::Never)
}

/// Returns the type of the nearest enclosing class for the given scope.
///
/// This function walks up the ancestor scopes starting from the given scope,
/// and finds the closest class definition. This is different to the behaviour of
/// [`TypeInferenceBuilder::class_context_of_current_method`], which will only return
/// `Some(class)` if either the immediate parent scope is a class OR the immediate parent
/// scope is a type-parameters scope and the grandparent scope is a class.
///
/// Returns `None` if no enclosing class is found.
pub(crate) fn nearest_enclosing_class<'db>(
    db: &'db dyn Db,
    semantic: &SemanticIndex<'db>,
    scope: ScopeId,
) -> Option<ClassLiteral<'db>> {
    semantic
        .ancestor_scopes(scope.file_scope_id(db))
        .find_map(|(_, ancestor_scope)| {
            let class = ancestor_scope.node().as_class()?;
            let definition = semantic.expect_single_definition(class);
            infer_definition_types(db, definition)
                .declaration_type(definition)
                .inner_type()
                .as_class_literal()
        })
}

/// Returns the type of the nearest enclosing function for the given scope.
///
/// This function walks up the ancestor scopes starting from the given scope,
/// and finds the closest (non-lambda) function definition.
///
/// Returns `None` if no enclosing function is found.
pub(crate) fn nearest_enclosing_function<'db>(
    db: &'db dyn Db,
    semantic: &SemanticIndex<'db>,
    scope: ScopeId,
) -> Option<FunctionType<'db>> {
    semantic
        .ancestor_scopes(scope.file_scope_id(db))
        .find_map(|(_, ancestor_scope)| {
            let func = ancestor_scope.node().as_function()?;
            let definition = semantic.expect_single_definition(func);
            let inference = infer_definition_types(db, definition);
            inference
                .undecorated_type()
                .unwrap_or_else(|| inference.declaration_type(definition).inner_type())
                .as_function_literal()
        })
}

/// A region within which we can infer types.
#[derive(Copy, Clone, Debug)]
pub(crate) enum InferenceRegion<'db> {
    /// infer types for a standalone [`Expression`]
    Expression(Expression<'db>, TypeContext<'db>),
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
            InferenceRegion::Expression(expression, _) => expression.scope(db),
            InferenceRegion::Definition(definition) | InferenceRegion::Deferred(definition) => {
                definition.scope(db)
            }
            InferenceRegion::Scope(scope) => scope,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, get_size2::GetSize, salsa::Update)]
enum CycleRecovery<'db> {
    /// An initial-value for fixpoint iteration; all types are `Type::Never`.
    Initial,
    /// A divergence-fallback value for fixpoint iteration; all types are `Divergent`.
    Divergent(ScopeId<'db>),
}

impl<'db> CycleRecovery<'db> {
    fn merge(self, other: Option<CycleRecovery<'db>>) -> Self {
        if let Some(other) = other {
            match (self, other) {
                // It's important here that we keep the scope of `self` if merging two `Divergent`.
                (Self::Divergent(scope), _) | (_, Self::Divergent(scope)) => Self::Divergent(scope),
                _ => Self::Initial,
            }
        } else {
            self
        }
    }

    fn fallback_type(self) -> Type<'db> {
        match self {
            Self::Initial => Type::Never,
            Self::Divergent(scope) => Type::divergent(Some(scope)),
        }
    }
}

/// The inferred types for a scope region.
#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
pub(crate) struct ScopeInference<'db> {
    /// The types of every expression in this region.
    expressions: FxHashMap<ExpressionNodeKey, Type<'db>>,

    /// The extra data that is only present for few inference regions.
    extra: Option<Box<ScopeInferenceExtra<'db>>>,
}

#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::Update, Default)]
struct ScopeInferenceExtra<'db> {
    /// Is this a cycle-recovery inference result, and if so, what kind?
    cycle_recovery: Option<CycleRecovery<'db>>,

    /// The diagnostics for this region.
    diagnostics: TypeCheckDiagnostics,
}

impl<'db> ScopeInference<'db> {
    fn cycle_initial(scope: ScopeId<'db>) -> Self {
        let _ = scope;

        Self {
            extra: Some(Box::new(ScopeInferenceExtra {
                cycle_recovery: Some(CycleRecovery::Initial),
                ..ScopeInferenceExtra::default()
            })),
            expressions: FxHashMap::default(),
        }
    }

    pub(crate) fn diagnostics(&self) -> Option<&TypeCheckDiagnostics> {
        self.extra.as_deref().map(|extra| &extra.diagnostics)
    }

    pub(crate) fn expression_type(&self, expression: impl Into<ExpressionNodeKey>) -> Type<'db> {
        self.try_expression_type(expression)
            .unwrap_or_else(Type::unknown)
    }

    pub(crate) fn try_expression_type(
        &self,
        expression: impl Into<ExpressionNodeKey>,
    ) -> Option<Type<'db>> {
        self.expressions
            .get(&expression.into())
            .copied()
            .or_else(|| self.fallback_type())
    }

    fn fallback_type(&self) -> Option<Type<'db>> {
        self.extra
            .as_ref()
            .and_then(|extra| extra.cycle_recovery.map(CycleRecovery::fallback_type))
    }
}

/// The inferred types for a definition region.
#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
pub(crate) struct DefinitionInference<'db> {
    /// The types of every expression in this region.
    expressions: FxHashMap<ExpressionNodeKey, Type<'db>>,

    /// The scope this region is part of.
    #[cfg(debug_assertions)]
    scope: ScopeId<'db>,

    /// The types of every binding in this region.
    ///
    /// Almost all definition regions have less than 10 bindings. There are very few with more than 10 (but still less than 20).
    /// Because of that, use a slice with linear search over a hash map.
    bindings: Box<[(Definition<'db>, Type<'db>)]>,

    /// The types and type qualifiers of every declaration in this region.
    ///
    /// About 50% of the definition inference regions have no declarations.
    /// The other 50% have less than 10 declarations. Because of that, use a
    /// slice with linear search over a hash map.
    declarations: Box<[(Definition<'db>, TypeAndQualifiers<'db>)]>,

    /// The extra data that is only present for few inference regions.
    extra: Option<Box<DefinitionInferenceExtra<'db>>>,
}

#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::Update, Default)]
struct DefinitionInferenceExtra<'db> {
    /// Is this a cycle-recovery inference result, and if so, what kind?
    cycle_recovery: Option<CycleRecovery<'db>>,

    /// The definitions that have some deferred parts.
    deferred: Box<[Definition<'db>]>,

    /// The diagnostics for this region.
    diagnostics: TypeCheckDiagnostics,

    /// For function definitions, the undecorated type of the function.
    undecorated_type: Option<Type<'db>>,
}

impl<'db> DefinitionInference<'db> {
    fn cycle_initial(scope: ScopeId<'db>) -> Self {
        let _ = scope;

        Self {
            expressions: FxHashMap::default(),
            bindings: Box::default(),
            declarations: Box::default(),
            #[cfg(debug_assertions)]
            scope,
            extra: Some(Box::new(DefinitionInferenceExtra {
                cycle_recovery: Some(CycleRecovery::Initial),
                ..DefinitionInferenceExtra::default()
            })),
        }
    }

    fn cycle_fallback(scope: ScopeId<'db>) -> Self {
        let _ = scope;

        Self {
            expressions: FxHashMap::default(),
            bindings: Box::default(),
            declarations: Box::default(),
            #[cfg(debug_assertions)]
            scope,
            extra: Some(Box::new(DefinitionInferenceExtra {
                cycle_recovery: Some(CycleRecovery::Divergent(scope)),
                ..DefinitionInferenceExtra::default()
            })),
        }
    }

    pub(crate) fn expression_type(&self, expression: impl Into<ExpressionNodeKey>) -> Type<'db> {
        self.try_expression_type(expression)
            .unwrap_or_else(Type::unknown)
    }

    pub(crate) fn try_expression_type(
        &self,
        expression: impl Into<ExpressionNodeKey>,
    ) -> Option<Type<'db>> {
        self.expressions
            .get(&expression.into())
            .copied()
            .or_else(|| self.fallback_type())
    }

    #[track_caller]
    pub(crate) fn binding_type(&self, definition: Definition<'db>) -> Type<'db> {
        self.bindings
            .iter()
            .find_map(
                |(def, ty)| {
                    if def == &definition { Some(*ty) } else { None }
                },
            )
            .or_else(|| self.fallback_type())
            .expect(
                "definition should belong to this TypeInference region and \
                TypeInferenceBuilder should have inferred a type for it",
            )
    }

    fn bindings(&self) -> impl ExactSizeIterator<Item = (Definition<'db>, Type<'db>)> {
        self.bindings.iter().copied()
    }

    #[track_caller]
    pub(crate) fn declaration_type(&self, definition: Definition<'db>) -> TypeAndQualifiers<'db> {
        self.declarations
            .iter()
            .find_map(|(def, qualifiers)| {
                if def == &definition {
                    Some(*qualifiers)
                } else {
                    None
                }
            })
            .or_else(|| self.fallback_type().map(TypeAndQualifiers::declared))
            .expect(
                "definition should belong to this TypeInference region and \
                TypeInferenceBuilder should have inferred a type for it",
            )
    }

    fn declarations(
        &self,
    ) -> impl ExactSizeIterator<Item = (Definition<'db>, TypeAndQualifiers<'db>)> {
        self.declarations.iter().copied()
    }

    fn declaration_types(&self) -> impl ExactSizeIterator<Item = TypeAndQualifiers<'db>> {
        self.declarations.iter().map(|(_, qualifiers)| *qualifiers)
    }

    fn fallback_type(&self) -> Option<Type<'db>> {
        self.extra
            .as_ref()
            .and_then(|extra| extra.cycle_recovery.map(CycleRecovery::fallback_type))
    }

    pub(crate) fn undecorated_type(&self) -> Option<Type<'db>> {
        self.extra.as_ref().and_then(|extra| extra.undecorated_type)
    }
}

/// The inferred types for an expression region.
#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
pub(crate) struct ExpressionInference<'db> {
    /// The types of every expression in this region.
    expressions: FxHashMap<ExpressionNodeKey, Type<'db>>,

    extra: Option<Box<ExpressionInferenceExtra<'db>>>,

    /// The scope this region is part of.
    #[cfg(debug_assertions)]
    scope: ScopeId<'db>,
}

/// Extra data that only exists for few inferred expression regions.
#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize, Default)]
struct ExpressionInferenceExtra<'db> {
    /// The types of every binding in this expression region.
    ///
    /// Only very few expression regions have bindings (around 0.1%).
    bindings: Box<[(Definition<'db>, Type<'db>)]>,

    /// The diagnostics for this region.
    diagnostics: TypeCheckDiagnostics,

    /// Is this a cycle recovery inference result, and if so, what kind?
    cycle_recovery: Option<CycleRecovery<'db>>,

    /// `true` if all places in this expression are definitely bound
    all_definitely_bound: bool,
}

impl<'db> ExpressionInference<'db> {
    fn cycle_initial(scope: ScopeId<'db>) -> Self {
        let _ = scope;
        Self {
            extra: Some(Box::new(ExpressionInferenceExtra {
                cycle_recovery: Some(CycleRecovery::Initial),
                all_definitely_bound: true,
                ..ExpressionInferenceExtra::default()
            })),
            expressions: FxHashMap::default(),

            #[cfg(debug_assertions)]
            scope,
        }
    }

    fn cycle_fallback(scope: ScopeId<'db>) -> Self {
        let _ = scope;
        Self {
            extra: Some(Box::new(ExpressionInferenceExtra {
                cycle_recovery: Some(CycleRecovery::Divergent(scope)),
                all_definitely_bound: true,
                ..ExpressionInferenceExtra::default()
            })),
            expressions: FxHashMap::default(),

            #[cfg(debug_assertions)]
            scope,
        }
    }

    pub(crate) fn try_expression_type(
        &self,
        expression: impl Into<ExpressionNodeKey>,
    ) -> Option<Type<'db>> {
        self.expressions
            .get(&expression.into())
            .copied()
            .or_else(|| self.fallback_type())
    }

    pub(crate) fn expression_type(&self, expression: impl Into<ExpressionNodeKey>) -> Type<'db> {
        self.try_expression_type(expression)
            .unwrap_or_else(Type::unknown)
    }

    fn fallback_type(&self) -> Option<Type<'db>> {
        self.extra
            .as_ref()
            .and_then(|extra| extra.cycle_recovery.map(CycleRecovery::fallback_type))
    }

    /// Returns true if all places in this expression are definitely bound.
    pub(crate) fn all_places_definitely_bound(&self) -> bool {
        self.extra
            .as_ref()
            .map(|e| e.all_definitely_bound)
            .unwrap_or(true)
    }
}
