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
//! the `Divergent` type for all expressions, bindings, and declarations, and then they continue iterating
//! the query cycle until a fixed-point is reached. Salsa has a built-in fixed limit on the number
//! of iterations, so if we fail to converge, Salsa will eventually panic. (This should of course
//! be considered a bug.)

use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_text_size::Ranged;
use rustc_hash::FxHashMap;
use salsa;
use salsa::plumbing::AsId;

use crate::Db;
use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::scope::ScopeId;
use crate::semantic_index::{SemanticIndex, semantic_index, use_def_map};
use crate::types::diagnostic::TypeCheckDiagnostics;
use crate::types::unpacker::{UnpackResult, Unpacker};
use crate::types::{
    ClassLiteral, DivergentType, DynamicType, NormalizedVisitor, Truthiness, Type,
    TypeAndQualifiers, UnionBuilder,
};
use crate::unpack::Unpack;
use builder::TypeInferenceBuilder;

mod builder;
#[cfg(test)]
mod tests;

/// A scope that may be recursive.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(super) struct PossiblyRecursiveScope<'db> {
    scope: ScopeId<'db>,
    cycle_recovery: DivergentType<'db>,
}

/// Infer all types for a [`ScopeId`], including all definitions and expressions in that scope.
/// Use when checking a scope, or needing to provide a type for an arbitrary expression in the
/// scope.
pub(crate) fn infer_scope_types<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
) -> &'db ScopeInference<'db> {
    infer_scope_types_impl(
        db,
        PossiblyRecursiveScope::new(db, scope, DivergentType::should_not_diverge(db, scope)),
    )
}

pub(crate) fn infer_function_scope_types<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    cycle_recovery: DivergentType<'db>,
) -> &'db ScopeInference<'db> {
    infer_scope_types_impl(db, PossiblyRecursiveScope::new(db, scope, cycle_recovery))
}

#[salsa::tracked(returns(ref), cycle_fn=scope_cycle_recover, cycle_initial=scope_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
fn infer_scope_types_impl<'db>(
    db: &'db dyn Db,
    scope: PossiblyRecursiveScope<'db>,
) -> ScopeInference<'db> {
    let file = scope.scope(db).file(db);
    let _span = tracing::trace_span!("infer_scope_types", scope=?scope.as_id(), ?file).entered();

    let module = parsed_module(db, file).load(db);

    // Using the index here is fine because the code below depends on the AST anyway.
    // The isolation of the query is by the return inferred types.
    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(db, InferenceRegion::Scope(scope.scope(db)), index, &module)
        .finish_scope()
}

fn scope_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &ScopeInference<'db>,
    _count: u32,
    _scope: PossiblyRecursiveScope<'db>,
) -> salsa::CycleRecoveryAction<ScopeInference<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn scope_cycle_initial<'db>(
    db: &'db dyn Db,
    scope: PossiblyRecursiveScope<'db>,
) -> ScopeInference<'db> {
    ScopeInference::cycle_initial(scope.cycle_recovery(db), scope.scope(db))
}

/// A [`Definition`] that may be recursive.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(super) struct PossiblyRecursiveDefinition<'db> {
    definition: Definition<'db>,
    cycle_recovery: DivergentType<'db>,
}

/// Infer all types for a [`Definition`] (including sub-expressions).
/// Use when resolving a place use or public type of a place.
pub(crate) fn infer_definition_types<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> &'db DefinitionInference<'db> {
    infer_definition_types_impl(
        db,
        PossiblyRecursiveDefinition::new(
            db,
            definition,
            DivergentType::should_not_diverge(db, definition.scope(db)),
        ),
    )
}

#[salsa::tracked(returns(ref), cycle_fn=definition_cycle_recover, cycle_initial=definition_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
fn infer_definition_types_impl<'db>(
    db: &'db dyn Db,
    definition: PossiblyRecursiveDefinition<'db>,
) -> DefinitionInference<'db> {
    let file = definition.definition(db).file(db);
    let module = parsed_module(db, file).load(db);
    let _span = tracing::trace_span!(
        "infer_definition_types",
        range = ?definition.definition(db).kind(db).target_range(&module),
        ?file
    )
    .entered();

    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(
        db,
        InferenceRegion::Definition(definition.definition(db)),
        index,
        &module,
    )
    .finish_definition()
}

fn definition_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &DefinitionInference<'db>,
    _count: u32,
    _definition: PossiblyRecursiveDefinition<'db>,
) -> salsa::CycleRecoveryAction<DefinitionInference<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn definition_cycle_initial<'db>(
    db: &'db dyn Db,
    definition: PossiblyRecursiveDefinition<'db>,
) -> DefinitionInference<'db> {
    DefinitionInference::cycle_initial(
        definition.definition(db).scope(db),
        definition.cycle_recovery(db),
    )
}

/// Infer types for all deferred type expressions in a [`Definition`].
///
/// Deferred expressions are type expressions (annotations, base classes, aliases...) in a stub
/// file, or in a file with `from __future__ import annotations`, or stringified annotations.
#[salsa::tracked(returns(ref), cycle_fn=deferred_cycle_recover, cycle_initial=deferred_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
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

fn deferred_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &DefinitionInference<'db>,
    _count: u32,
    _definition: Definition<'db>,
) -> salsa::CycleRecoveryAction<DefinitionInference<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn deferred_cycle_initial<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> DefinitionInference<'db> {
    DefinitionInference::cycle_initial(
        definition.scope(db),
        DivergentType::should_not_diverge(db, definition.scope(db)),
    )
}

/// Infer all types for an [`Expression`] (including sub-expressions).
/// Use rarely; only for cases where we'd otherwise risk double-inferring an expression: RHS of an
/// assignment, which might be unpacking/multi-target and thus part of multiple definitions, or a
/// type narrowing guard expression (e.g. if statement test node).
pub(crate) fn infer_expression_types<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
    tcx: TypeContext<'db>,
    cycle_recovery: DivergentType<'db>,
) -> &'db ExpressionInference<'db> {
    infer_expression_types_impl(
        db,
        InferExpression::new(db, expression, tcx, cycle_recovery),
    )
}

#[salsa::tracked(returns(ref), cycle_fn=expression_cycle_recover, cycle_initial=expression_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
pub(super) fn infer_expression_types_impl<'db>(
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

fn expression_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &ExpressionInference<'db>,
    _count: u32,
    _input: InferExpression<'db>,
) -> salsa::CycleRecoveryAction<ExpressionInference<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn expression_cycle_initial<'db>(
    db: &'db dyn Db,
    input: InferExpression<'db>,
) -> ExpressionInference<'db> {
    ExpressionInference::cycle_initial(input.expression(db).scope(db), input.cycle_recovery(db))
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
    cycle_recovery: DivergentType<'db>,
    parsed: &ParsedModuleRef,
) -> Type<'db> {
    let inference = infer_expression_types(db, expression, tcx, cycle_recovery);
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
    infer_expression_type_impl(
        db,
        InferExpression::new(
            db,
            expression,
            tcx,
            DivergentType::should_not_diverge(db, expression.scope(db)),
        ),
    )
}

pub(crate) fn infer_implicit_attribute_expression_type<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
    tcx: TypeContext<'db>,
    cycle_recovery: DivergentType<'db>,
) -> Type<'db> {
    infer_expression_type_impl(
        db,
        InferExpression::new(db, expression, tcx, cycle_recovery),
    )
}

#[salsa::tracked(cycle_fn=single_expression_cycle_recover, cycle_initial=single_expression_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
fn infer_expression_type_impl<'db>(db: &'db dyn Db, input: InferExpression<'db>) -> Type<'db> {
    let file = input.expression(db).file(db);
    let module = parsed_module(db, file).load(db);

    // It's okay to call the "same file" version here because we're inside a salsa query.
    let inference = infer_expression_types_impl(db, input);
    inference.expression_type(input.expression(db).node_ref(db, &module))
}

fn single_expression_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &Type<'db>,
    _count: u32,
    _input: InferExpression<'db>,
) -> salsa::CycleRecoveryAction<Type<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn single_expression_cycle_initial<'db>(db: &'db dyn Db, input: InferExpression<'db>) -> Type<'db> {
    Type::Dynamic(DynamicType::Divergent(input.cycle_recovery(db)))
}

/// An `Expression` with an optional `TypeContext`.
///
/// This is a Salsa supertype used as the input to `infer_expression_types` to avoid
/// interning an `ExpressionWithContext` unnecessarily when no type context is provided.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, salsa::Supertype, salsa::Update)]
pub(super) enum InferExpression<'db> {
    Bare(PossiblyRecursiveExpression<'db>),
    WithContext(PossiblyRecursiveExpressionWithContext<'db>),
}

impl<'db> InferExpression<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        expression: Expression<'db>,
        tcx: TypeContext<'db>,
        cycle_recovery: DivergentType<'db>,
    ) -> InferExpression<'db> {
        if tcx.annotation.is_some() {
            InferExpression::WithContext(PossiblyRecursiveExpressionWithContext::new(
                db,
                expression,
                tcx,
                cycle_recovery,
            ))
        } else {
            // Drop the empty `TypeContext` to avoid the interning cost.
            InferExpression::Bare(PossiblyRecursiveExpression::new(
                db,
                expression,
                cycle_recovery,
            ))
        }
    }

    #[cfg(test)]
    pub(super) fn bare(
        db: &'db dyn Db,
        expression: Expression<'db>,
        cycle_recovery: DivergentType<'db>,
    ) -> InferExpression<'db> {
        InferExpression::Bare(PossiblyRecursiveExpression::new(
            db,
            expression,
            cycle_recovery,
        ))
    }

    fn expression(self, db: &'db dyn Db) -> Expression<'db> {
        match self {
            InferExpression::Bare(bare) => bare.expression(db),
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

    fn cycle_recovery(self, db: &'db dyn Db) -> DivergentType<'db> {
        match self {
            InferExpression::Bare(bare) => bare.cycle_recovery(db),
            InferExpression::WithContext(expression_with_context) => {
                expression_with_context.cycle_recovery(db)
            }
        }
    }
}

/// An [`Expression`] that may be recursive.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(super) struct PossiblyRecursiveExpression<'db> {
    expression: Expression<'db>,
    cycle_recovery: DivergentType<'db>,
}

/// An [`Expression`] with a [`TypeContext`], that may be recursive.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(super) struct PossiblyRecursiveExpressionWithContext<'db> {
    expression: Expression<'db>,
    tcx: TypeContext<'db>,
    cycle_recovery: DivergentType<'db>,
}

/// The type context for a given expression, namely the type annotation
/// in an annotated assignment.
///
/// Knowing the outer type context when inferring an expression can enable
/// more precise inference results, aka "bidirectional type inference".
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) struct TypeContext<'db> {
    annotation: Option<Type<'db>>,
}

impl<'db> TypeContext<'db> {
    pub(crate) fn new(annotation: Type<'db>) -> Self {
        Self {
            annotation: Some(annotation),
        }
    }
}

/// Returns the statically-known truthiness of a given expression.
///
/// Returns [`Truthiness::Ambiguous`] in case any non-definitely bound places
/// were encountered while inferring the type of the expression.
#[salsa::tracked(cycle_fn=static_expression_truthiness_cycle_recover, cycle_initial=static_expression_truthiness_cycle_initial, heap_size=get_size2::GetSize::get_heap_size)]
pub(crate) fn static_expression_truthiness<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> Truthiness {
    let inference = infer_expression_types_impl(
        db,
        InferExpression::Bare(PossiblyRecursiveExpression::new(
            db,
            expression,
            DivergentType::should_not_diverge(db, expression.scope(db)),
        )),
    );

    if !inference.all_places_definitely_bound() {
        return Truthiness::Ambiguous;
    }

    let file = expression.file(db);
    let module = parsed_module(db, file).load(db);
    let node = expression.node_ref(db, &module);

    inference.expression_type(node).bool(db)
}

#[expect(clippy::trivially_copy_pass_by_ref)]
fn static_expression_truthiness_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &Truthiness,
    _count: u32,
    _expression: Expression<'db>,
) -> salsa::CycleRecoveryAction<Truthiness> {
    salsa::CycleRecoveryAction::Iterate
}

fn static_expression_truthiness_cycle_initial<'db>(
    _db: &'db dyn Db,
    _expression: Expression<'db>,
) -> Truthiness {
    Truthiness::Ambiguous
}

pub(super) fn infer_unpack_implicit_attribute_types<'db>(
    db: &'db dyn Db,
    unpack: Unpack<'db>,
    cycle_recovery: DivergentType<'db>,
) -> &'db UnpackResult<'db> {
    infer_unpack_types_impl(db, unpack, cycle_recovery)
}

pub(super) fn infer_unpack_types<'db>(
    db: &'db dyn Db,
    unpack: Unpack<'db>,
) -> &'db UnpackResult<'db> {
    infer_unpack_types_impl(
        db,
        unpack,
        DivergentType::should_not_diverge(db, unpack.target_scope(db)),
    )
}

/// Infer the types for an [`Unpack`] operation.
///
/// This infers the expression type and performs structural match against the target expression
/// involved in an unpacking operation. It returns a result-like object that can be used to get the
/// type of the variables involved in this unpacking along with any violations that are detected
/// during this unpacking.
#[salsa::tracked(returns(ref), cycle_fn=unpack_cycle_recover, cycle_initial=unpack_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
fn infer_unpack_types_impl<'db>(
    db: &'db dyn Db,
    unpack: Unpack<'db>,
    cycle_recovery: DivergentType<'db>,
) -> UnpackResult<'db> {
    let file = unpack.file(db);
    let module = parsed_module(db, file).load(db);
    let _span = tracing::trace_span!("infer_unpack_types", range=?unpack.range(db, &module), ?file)
        .entered();

    let mut unpacker = Unpacker::new(db, unpack.target_scope(db), &module);
    unpacker.unpack(unpack.target(db, &module), unpack.value(db), cycle_recovery);
    unpacker.finish()
}

fn unpack_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &UnpackResult<'db>,
    _count: u32,
    _unpack: Unpack<'db>,
    _cycle_recovery: DivergentType<'db>,
) -> salsa::CycleRecoveryAction<UnpackResult<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn unpack_cycle_initial<'db>(
    _db: &'db dyn Db,
    _unpack: Unpack<'db>,
    cycle_recovery: DivergentType<'db>,
) -> UnpackResult<'db> {
    UnpackResult::cycle_initial(Type::divergent(cycle_recovery))
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
                .into_class_literal()
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

/// The inferred types for a scope region.
#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
pub(crate) struct ScopeInference<'db> {
    /// The types of every expression in this region.
    expressions: FxHashMap<ExpressionNodeKey, Type<'db>>,

    /// The extra data that is only present for few inference regions.
    extra: Option<Box<ScopeInferenceExtra<'db>>>,

    scope: ScopeId<'db>,
}

#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::Update, Default)]
struct ScopeInferenceExtra<'db> {
    /// The fallback type for missing expressions/bindings/declarations or recursive type inference.
    cycle_recovery: Option<DivergentType<'db>>,

    /// The diagnostics for this region.
    diagnostics: TypeCheckDiagnostics,

    /// The returnees of this region (if this is a function body).
    ///
    /// These are stored in `Vec` to delay the creation of the union type as long as possible.
    returnees: Vec<Option<ExpressionNodeKey>>,
}

impl<'db> ScopeInference<'db> {
    fn cycle_initial(cycle_recovery: DivergentType<'db>, scope: ScopeId<'db>) -> Self {
        Self {
            extra: Some(Box::new(ScopeInferenceExtra {
                cycle_recovery: Some(cycle_recovery),
                ..ScopeInferenceExtra::default()
            })),
            expressions: FxHashMap::default(),
            scope,
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
            .and_then(|extra| extra.cycle_recovery.map(Type::divergent))
    }

    /// Returns the inferred return type of this function body (union of all possible return types),
    /// or `None` if the region is not a function body.
    /// In the case of methods, the return type of the superclass method is further unioned.
    /// If there is no superclass method and this method is not `final`, it will be unioned with `Unknown`.
    pub(crate) fn infer_return_type(&self, db: &'db dyn Db, callee_ty: Type<'db>) -> Type<'db> {
        // TODO: coroutine function type inference
        // TODO: generator function type inference
        if self.scope.is_coroutine_function(db) || self.scope.is_generator_function(db) {
            return Type::unknown();
        }

        let mut union = UnionBuilder::new(db);
        let div = Type::divergent(DivergentType::function_return_type(db, self.scope));
        if let Some(fallback_type) = self.fallback_type() {
            union = union.add(fallback_type);
        }
        let visitor = NormalizedVisitor::default().recursive(div);
        // Here, we use the dynamic type `Divergent` to detect divergent type inference and ensure that we obtain finite results.
        // For example, consider the following recursive function:
        // ```py
        // def div(n: int):
        //     if n == 0:
        //         return None
        //     else:
        //         return (div(n-1),)
        // ```
        // If we try to infer the return type of this function naively, we will get `tuple[tuple[tuple[...] | None] | None] | None`, which never converges.
        // So, when we detect a cycle, we set the cycle initial type to `Divergent`. Then the type obtained in the first cycle is `tuple[Divergent] | None`.
        // Let's call such a type containing `Divergent` a "recursive type".
        // Next, if there is a type containing a recursive type (let's call this a nested recursive type), we replace the inner recursive type with the `Divergent` type.
        // All recursive types are flattened in the next cycle, resulting in a convergence of the return type in finite cycles.
        // 0th: Divergent
        // 1st: tuple[Divergent] | None
        // 2nd: tuple[tuple[Divergent] | None] | None => tuple[Divergent] | None
        let previous_type = callee_ty.infer_return_type(db).unwrap();
        // In fixed-point iteration of return type inference, the return type must be monotonically widened and not "oscillate".
        // Here, monotonicity is guaranteed by pre-unioning the type of the previous iteration into the current result.
        union = union.add(previous_type.normalized_impl(db, &visitor));

        let Some(extra) = &self.extra else {
            unreachable!(
                "infer_return_type should only be called on a function body scope inference"
            );
        };
        for returnee in &extra.returnees {
            let ty = returnee.map_or(Type::none(db), |expression| {
                self.expression_type(expression)
            });
            union = union.add(ty.normalized_impl(db, &visitor));
        }
        let use_def = use_def_map(db, self.scope);
        if use_def.can_implicitly_return_none(db) {
            union = union.add(Type::none(db));
        }
        if let Type::BoundMethod(method_ty) = callee_ty {
            // If the method is not final and the typing is implicit, the inferred return type should be unioned with `Unknown`.
            // If any method in a base class does not have an annotated return type, `base_return_type` will include `Unknown`.
            // On the other hand, if the return types of all methods in the base classes are annotated, there is no need to include `Unknown`.
            if !method_ty.is_final(db) {
                union = union.add(
                    method_ty
                        .base_return_type(db)
                        .unwrap_or(Type::unknown())
                        .normalized_impl(db, &visitor),
                );
            }
        }

        union.build()
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
    /// The fallback type for missing expressions/bindings/declarations or recursive type inference.
    cycle_recovery: Option<DivergentType<'db>>,

    /// The definitions that are deferred.
    deferred: Box<[Definition<'db>]>,

    /// The diagnostics for this region.
    diagnostics: TypeCheckDiagnostics,

    /// For function definitions, the undecorated type of the function.
    undecorated_type: Option<Type<'db>>,
}

impl<'db> DefinitionInference<'db> {
    fn cycle_initial(scope: ScopeId<'db>, cycle_recovery: DivergentType<'db>) -> Self {
        let _ = scope;

        Self {
            expressions: FxHashMap::default(),
            bindings: Box::default(),
            declarations: Box::default(),
            #[cfg(debug_assertions)]
            scope,
            extra: Some(Box::new(DefinitionInferenceExtra {
                cycle_recovery: Some(cycle_recovery),
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
            .or_else(|| self.fallback_type().map(Into::into))
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
            .and_then(|extra| extra.cycle_recovery.map(Type::divergent))
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

    /// The fallback type for missing expressions/bindings/declarations or recursive type inference.
    cycle_recovery: Option<DivergentType<'db>>,

    /// `true` if all places in this expression are definitely bound
    all_definitely_bound: bool,
}

impl<'db> ExpressionInference<'db> {
    fn cycle_initial(scope: ScopeId<'db>, cycle_recovery: DivergentType<'db>) -> Self {
        let _ = scope;
        Self {
            extra: Some(Box::new(ExpressionInferenceExtra {
                cycle_recovery: Some(cycle_recovery),
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
            .and_then(|extra| extra.cycle_recovery.map(Type::divergent))
    }

    /// Returns true if all places in this expression are definitely bound.
    pub(crate) fn all_places_definitely_bound(&self) -> bool {
        self.extra
            .as_ref()
            .map(|e| e.all_definitely_bound)
            .unwrap_or(true)
    }
}
