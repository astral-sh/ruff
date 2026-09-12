use crate::ProgramEnvironment;
use std::borrow::Cow;
use std::debug_assert_matches;

use ruff_db::parsed::ParsedModuleRef;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::Ranged;

use crate::Db;
use crate::types::infer::{ExpressionInference, FrozenMap};
use crate::types::tuple::promotion::TupleSizePromotionConstraints;
use crate::types::tuple::{
    ResizeTupleError, Tuple, TupleBuilder, TupleElement, TupleLength, TupleSpec, TupleSpecBuilder,
    TupleType, VariableLengthTuple,
};
use crate::types::{
    KnownClass, Type, TypeCheckDiagnostics, TypeContext, UnionBuilder, UnionType,
    infer_expression_types,
};
use ty_python_core::ExpressionNodeKey;
use ty_python_core::ProgramFile;
use ty_python_core::definition::Definition;
use ty_python_core::expression::Expression;
use ty_python_core::scope::ScopeId;
use ty_python_core::unpack::{UnpackKind, UnpackValue};

use super::context::InferContext;
use super::diagnostic::INVALID_ASSIGNMENT;

/// Supplies expression inference and ordinary write validation as targets are matched.
pub(super) trait UnpackInference<'db, 'ast> {
    fn expression_type(&self, expression: &ast::Expr) -> Type<'db>;
    fn has_inferred_expression(&self, expression: &ast::Expr) -> bool;
    fn infer_enclosing_expression(&mut self, expression: &'ast ast::Expr);
    fn infer_target(
        &mut self,
        target: &'ast ast::Expr,
        assigned: UnpackAssignedValue<'_, 'db, 'ast>,
    ) -> Option<Type<'db>>;
    fn target_type_context(&mut self, target: &'ast ast::Expr) -> TypeContext<'db>;
    /// Whether a starred target can use individual matched elements when inferring its list.
    fn capture_needs_context(&mut self, target: &'ast ast::Expr) -> bool;
}

/// The expressions of a literal capture or the elements of each inferred alternative.
#[derive(Clone, Copy)]
pub(super) enum UnpackCaptured<'source, 'db, 'ast> {
    Expressions(&'source [Option<&'ast ast::Expr>]),
    Alternatives(&'source [Vec<UnpackElement<'db, 'ast>>]),
}

/// The source expression, assigned type, or captured elements matched to a target.
pub(super) enum UnpackAssignedValue<'source, 'db, 'ast> {
    Expression(&'ast ast::Expr, TypeContext<'db>),
    Value(UnpackElement<'db, 'ast>),
    Captured(UnpackCaptured<'source, 'db, 'ast>),
}

impl<'db, 'ast> UnpackInference<'db, 'ast> for &ExpressionInference<'db> {
    fn expression_type(&self, expression: &ast::Expr) -> Type<'db> {
        ExpressionInference::expression_type(self, expression)
    }

    fn has_inferred_expression(&self, _expression: &ast::Expr) -> bool {
        true
    }

    fn infer_enclosing_expression(&mut self, _expression: &'ast ast::Expr) {}

    fn infer_target(
        &mut self,
        _target: &'ast ast::Expr,
        _assigned: UnpackAssignedValue<'_, 'db, 'ast>,
    ) -> Option<Type<'db>> {
        None
    }

    fn target_type_context(&mut self, _target: &'ast ast::Expr) -> TypeContext<'db> {
        TypeContext::default()
    }

    fn capture_needs_context(&mut self, _target: &'ast ast::Expr) -> bool {
        false
    }
}

struct InferredUnpackTarget<'db, 'ast> {
    ty: UnionBuilder<'db>,
    expression: Option<&'ast ast::Expr>,
    promote_literals: bool,
    /// Whether this capture needs its matched elements for contextual inference, cached across
    /// source alternatives. Unannotated names need only the inferred list type of each alternative.
    capture_needs_context: Option<bool>,
    /// Each valid alternative creates a separate list; invalid alternatives supply no elements.
    /// A non-union source has one capture alternative.
    captured_alternatives: SmallVec<[Vec<UnpackElement<'db, 'ast>>; 1]>,
}

/// Whether one source alternative can supply elements to an unpacking assignment.
///
/// ```python
/// def assign(source: tuple[int] | int) -> None:
///     rest: list[object]
///     (*rest,) = source
/// ```
///
/// The `int` alternative reports an iteration error and cannot construct a captured list.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceIteration {
    Iterable,
    NotIterable,
}

/// Records an `Unknown` type for every expression in a malformed unpack target subtree.
struct UnknownTargetCollector<'db, 'map> {
    targets: &'map mut FxHashMap<ExpressionNodeKey, Type<'db>>,
}

impl<'ast> Visitor<'ast> for UnknownTargetCollector<'_, '_> {
    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        self.targets.insert(expr.into(), Type::unknown());
        visitor::walk_expr(self, expr);
    }
}

/// Unpacks the value expression type to their respective targets.
pub(crate) struct Unpacker<'db, 'ast> {
    context: InferContext<'db, 'ast>,
    targets: FxHashMap<ExpressionNodeKey, Type<'db>>,
}

impl<'db, 'ast> Unpacker<'db, 'ast> {
    pub(crate) fn new(
        db: &'db dyn Db,
        env: &'ast ProgramEnvironment<'db>,
        target_scope: ScopeId<'db>,
        program_file: ProgramFile<'db>,
        module: &'ast ParsedModuleRef,
    ) -> Self {
        Self {
            context: InferContext::new(
                db,
                env,
                target_scope,
                program_file.file(db),
                program_file,
                module,
            ),
            targets: FxHashMap::default(),
        }
    }

    fn db(&self) -> &'db dyn Db {
        self.context.db()
    }

    fn module(&self) -> &'ast ParsedModuleRef {
        self.context.module()
    }

    /// Unpack the value to the target expression.
    pub(crate) fn unpack(
        &mut self,
        target: &'ast ast::Expr,
        value: UnpackValue<'db>,
        inference: &mut dyn UnpackInference<'db, 'ast>,
    ) {
        let db = self.db();
        debug_assert_matches!(
            target,
            ast::Expr::List(_) | ast::Expr::Tuple(_),
            "Unpacking target must be a list or tuple expression"
        );

        let value_expr = value.expression().node_ref(self.db()).node(self.module());

        let value_type = inference.expression_type(value_expr);

        let value_type = match value.kind() {
            UnpackKind::Assign => {
                if self.context.in_stub() && value_expr.is_ellipsis_literal_expr() {
                    Type::unknown()
                } else {
                    value_type
                }
            }
            UnpackKind::Iterable { mode } => {
                let env = self.context.program_environment();
                value_type
                    .try_iterate_with_mode(db, env, mode)
                    .map(|tuple| tuple.homogeneous_element_type(db, env))
                    .unwrap_or_else(|err| {
                        err.report_diagnostic(
                            &self.context,
                            value_type,
                            value.as_any_node_ref(self.db(), self.module()),
                        );
                        err.fallback_element_type(db, env)
                    })
            }
            UnpackKind::ContextManager { mode } => {
                let env = self.context.program_environment();
                value_type
                    .try_enter_with_mode(db, env, mode)
                    .unwrap_or_else(|err| {
                        err.report_diagnostic(
                            &self.context,
                            value_type,
                            value.as_any_node_ref(self.db(), self.module()),
                        );
                        err.fallback_enter_type(db, env)
                    })
            }
        };

        self.unpack_inner(
            target,
            value_expr.into(),
            UnpackElement {
                ty: value_type,
                expression: matches!(value.kind(), UnpackKind::Assign).then_some(value_expr),
                promote_literals: false,
            },
            inference,
        );
    }

    /// Records `Unknown` for a malformed unpack target and all of its descendant expressions.
    fn record_unknown_target_subtree(&mut self, target: &ast::Expr) {
        UnknownTargetCollector {
            targets: &mut self.targets,
        }
        .visit_expr(target);
    }

    /// Combine the write contexts observed for an unpacking target into a tuple context
    /// for a source expression whose elements cannot be matched syntactically.
    fn target_type_context(
        &self,
        target: &'ast ast::Expr,
        inference: &mut dyn UnpackInference<'db, 'ast>,
    ) -> TypeContext<'db> {
        let Some(targets) = sequence_elts(target) else {
            return inference.target_type_context(target);
        };

        let db = self.db();
        let env = self.context.program_environment();
        let mut tuple = TupleSpecBuilder::with_capacity(targets.len());
        let mut has_context = false;
        let mut has_starred = false;

        for target in targets {
            if let ast::Expr::Starred(starred) = target {
                if has_starred {
                    return TypeContext::default();
                }
                has_starred = true;
                let annotation = self
                    .target_type_context(&starred.value, inference)
                    .annotation;
                has_context |= annotation.is_some();
                let element_ty = annotation
                    .and_then(|ty| ty.try_iterate(db, env).ok())
                    .map(|elements| elements.homogeneous_element_type(db, env))
                    .unwrap_or_else(Type::unknown);
                tuple = tuple.concat(db, env, &TupleSpec::homogeneous(element_ty));
            } else {
                let annotation = self.target_type_context(target, inference).annotation;
                has_context |= annotation.is_some();
                tuple.push(annotation.unwrap_or_else(Type::unknown));
            }
        }

        TypeContext::new(has_context.then(|| Type::tuple(TupleType::new(db, env, &tuple.build()))))
    }

    /// In assignments from tuple or list literals, map each target to the corresponding element
    /// types on the right, including the elements collected by a starred target. This preserves
    /// element positions in list literals, whose inferred type combines all element types.
    ///
    /// We avoid infinitely growing types in cycle resolution by preserving only the
    /// topmost/outermost part of types that have `Divergent` components. For example, if the
    /// assignment `x = (0, x)` shows up in a loop, we need to avoid infinite looping on a
    /// never-ending type like `tuple[Literal[0], tuple[Literal[0], tuple[...]]]`. So when we see
    /// an intermediate result like `tuple[Literal[0], tuple[Literal[0], Divergent]]`, we simplify
    /// that to `tuple[Literal[0], Divergent]`.
    ///
    /// The problem here is that, when `Divergent` shows up on the RHS, we end up simplifying that
    /// tuple to e.g. `tuple[Divergent, Divergent]`. If we proceed by unpacking that type, we won't
    /// accumulate any information about the elements, and the user will end up seeing `Divergent`
    /// as the type of their variables.
    ///
    /// This function avoids that problem by walking the AST on the RHS and looking directly at the
    /// individual element types. That gives us one more level of structure for those types, which
    /// is enough to resolve a lot of common cycles.
    fn unpack_inner(
        &mut self,
        target: &'ast ast::Expr,
        value_expr: AnyNodeRef<'_>,
        mut value: UnpackElement<'db, 'ast>,
        inference: &mut dyn UnpackInference<'db, 'ast>,
    ) {
        let db = self.db();
        let env = self.context.program_environment();

        if let Some(expression) = value.expression
            && !inference.has_inferred_expression(expression)
        {
            if let Some(targets) = sequence_elts(target)
                && let Some(values) = sequence_elts(expression)
                && expansions_have_literal_elements(values)
                && targets
                    .iter()
                    .filter(|target| target.is_starred_expr())
                    .count()
                    <= 1
                && let Some(matched) = unpack_literal_values(targets, expression)
            {
                // One match drives both contextual inference and assignment:
                //
                //     rest: list[list[object]]
                //     (first, *rest), last = ((0, [1], [2]), 3)
                //
                // Infer [1] and [2] through rest's list context before constructing
                // either enclosing tuple from the inferred child expressions.
                let promote = literal_sequence_elements(expression, value.promote_literals)
                    .is_some_and(|(_, promote)| promote);
                for (target, source) in targets.iter().zip(matched.into_all_elements_with_kind()) {
                    match source {
                        TupleElement::Variable(elements)
                            if let ast::Expr::Starred(starred) = target =>
                        {
                            let contextual_ty = inference.infer_target(
                                &starred.value,
                                UnpackAssignedValue::Captured(UnpackCaptured::Expressions(
                                    &elements,
                                )),
                            );
                            let ty = contextual_ty.unwrap_or_else(|| {
                                collected_list_type(
                                    db,
                                    env,
                                    elements.iter().map(|element| {
                                        (
                                            element
                                                .map(|expr| inference.expression_type(expr))
                                                .unwrap_or_else(Type::unknown),
                                            *element,
                                        )
                                    }),
                                )
                            });
                            self.unpack_inner(
                                target,
                                value_expr,
                                UnpackElement::from_type(ty),
                                inference,
                            );
                        }
                        TupleElement::Fixed(element)
                        | TupleElement::Prefix(element)
                        | TupleElement::Suffix(element) => {
                            let ty = element
                                .map(|expr| inference.expression_type(expr))
                                .unwrap_or_else(Type::unknown);
                            self.unpack_inner(
                                target,
                                element.map(AnyNodeRef::from).unwrap_or(value_expr),
                                UnpackElement {
                                    ty: if promote { ty.promote(db, env) } else { ty },
                                    expression: element,
                                    promote_literals: promote,
                                },
                                inference,
                            );
                        }
                        TupleElement::Variable(_) => {}
                    }
                }
                inference.infer_enclosing_expression(expression);
                return;
            }

            // Calls, ambiguous expansions, and length mismatches cannot pair
            // each source expression with a target:
            //
            //     def pair[T]() -> tuple[list[T], int]:
            //         return [], 0
            //
            //     items: list[int]
            //     items, last = pair()
            //
            // Infer the source as a whole, then match its element types to the targets
            // or report an invalid shape. An assignment may infer this source again
            // using contexts observed from the matched target writes.
            let tcx = if sequence_elts(target).is_some() {
                self.target_type_context(target, inference)
            } else {
                // A matched expression asks its own target for context during inference.
                TypeContext::default()
            };
            inference.infer_target(target, UnpackAssignedValue::Expression(expression, tcx));
            let ty = inference.expression_type(expression);
            value.ty = if self.context.in_stub() && expression.is_ellipsis_literal_expr() {
                Type::unknown()
            } else if value.promote_literals {
                ty.promote(db, env)
            } else {
                ty
            };
        }

        let targets = match target {
            ast::Expr::Name(_) | ast::Expr::Attribute(_) | ast::Expr::Subscript(_) => {
                self.targets.insert(target.into(), value.ty);
                inference.infer_target(target, UnpackAssignedValue::Value(value));
                return;
            }
            ast::Expr::Starred(starred) => {
                self.unpack_inner(&starred.value, value_expr, value, inference);
                return;
            }
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => elts,
            _ => {
                // Recovered syntax can still create assignment definitions for descendants of
                // malformed targets. Give the whole subtree an unknown type so later lookups
                // don't panic.
                self.record_unknown_target_subtree(target);
                return;
            }
        };
        let target_len = target_length(targets);
        let literal = value.expression.and_then(|expression| {
            literal_sequence(
                expression,
                value.promote_literals,
                &|expression, promote| {
                    let ty = inference.expression_type(expression);
                    UnpackElement {
                        ty: if promote { ty.promote(db, env) } else { ty },
                        expression: Some(expression),
                        promote_literals: promote,
                    }
                },
                &|expression, promote, known_length| {
                    // The starred expression's inference has already reported iteration errors.
                    // For `a, *rest = [1, *items]`, retain the shape of `items`' iterator even
                    // though the enclosing list's type has erased positions and length.
                    let ty = inference.expression_type(expression);
                    let ty = if promote { ty.promote(db, env) } else { ty };
                    let mut tuple = ty.iterate(db, env);
                    if let Some(length) = known_length
                        && let Ok(resized) = tuple.resize(db, env, TupleLength::Fixed(length))
                    {
                        tuple = Cow::Owned(resized);
                    }
                    sequence_from_type(db, &tuple)
                },
            )
        });

        let sequences = if let Some(literal) = literal {
            vec![(literal, SourceIteration::Iterable)]
        } else {
            // N.B. `Type::try_iterate` internally handles unions, but in a lossy way.
            // For our purposes here, we get better error messages and more precise inference
            // if we manually map over the union and call `try_iterate` on each union element.
            // See <https://github.com/astral-sh/ruff/pull/20377#issuecomment-3401380305>
            // for more discussion.
            let unpack_types = match value.ty {
                Type::Union(union_ty) => union_ty.elements(db),
                _ => std::slice::from_ref(&value.ty),
            };
            unpack_types
                .iter()
                .map(|ty| {
                    let (tuple, iteration) = match ty.try_iterate(db, env) {
                        Ok(tuple) => (tuple, SourceIteration::Iterable),
                        Err(err) => {
                            err.report_diagnostic(&self.context, *ty, value_expr);
                            (
                                Cow::Owned(TupleSpec::homogeneous(
                                    err.fallback_element_type(db, env),
                                )),
                                SourceIteration::NotIterable,
                            )
                        }
                    };
                    (sequence_from_type(db, &tuple), iteration)
                })
                .collect()
        };

        let mut inferred_targets: Vec<_> = targets
            .iter()
            .map(|_| InferredUnpackTarget {
                ty: UnionBuilder::new(db, env).unpack_aliases(false),
                expression: None,
                promote_literals: false,
                capture_needs_context: None,
                captured_alternatives: SmallVec::new(),
            })
            .collect();
        for (sequence, iteration) in sequences {
            let matched = sequence.unpack(target_len, Clone::clone, |elements| {
                UnpackElement::from_type(UnionType::from_elements_leave_aliases(
                    db,
                    env,
                    elements.iter().map(|element| element.ty),
                ))
            });
            match matched {
                Ok(matched) => {
                    for ((target, inferred), element) in targets
                        .iter()
                        .zip(&mut inferred_targets)
                        .zip(matched.into_all_elements_with_kind())
                    {
                        let element = match element {
                            TupleElement::Fixed(value)
                            | TupleElement::Prefix(value)
                            | TupleElement::Suffix(value) => value,
                            TupleElement::Variable(values) => {
                                // Use only the elements matched to this capture: in
                                // `first, *rest = [0, *strings]`, `0` belongs to `first`.
                                // Each union arm constructs its own list. For a source typed
                                // `tuple[int] | tuple[str]`, infer `list[int] | list[str]`;
                                // combining elements first would infer `list[int | str]`.
                                let ty = collected_list_type(
                                    db,
                                    env,
                                    values.iter().map(|value| (value.ty, value.expression)),
                                );

                                if iteration == SourceIteration::Iterable
                                    && let ast::Expr::Starred(starred) = target
                                    && *inferred.capture_needs_context.get_or_insert_with(|| {
                                        inference.capture_needs_context(&starred.value)
                                    })
                                {
                                    inferred.captured_alternatives.push(values);
                                }

                                UnpackElement::from_type(ty)
                            }
                        };
                        inferred.ty.add_in_place(element.ty);
                        // Literal sources contribute exactly one sequence. Only the type-based
                        // path combines multiple union arms, and those have no source expressions.
                        inferred.expression = element.expression;
                        inferred.promote_literals = element.promote_literals;
                    }
                }
                Err(err) => {
                    // A length mismatch has no valid correspondence, e.g. `a, *b, c = [1]`.
                    // Recover every target at this level, without discarding sibling literals
                    // handled by the enclosing recursive call.
                    for (target, inferred) in targets.iter().zip(&mut inferred_targets) {
                        inferred.ty.add_in_place(if target.is_starred_expr() {
                            KnownClass::List.to_specialized_instance(db, env, &[Type::unknown()])
                        } else {
                            Type::unknown()
                        });
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_ASSIGNMENT, target) {
                        let (message, actual) = match err {
                            ResizeTupleError::TooManyValues => (
                                "Too many values to unpack",
                                sequence.len().display_minimum(),
                            ),
                            ResizeTupleError::TooFewValues => (
                                "Not enough values to unpack",
                                sequence.len().display_maximum(),
                            ),
                        };
                        let mut diag = builder.into_diagnostic(message);
                        diag.set_primary_annotation_message(format_args!(
                            "Expected {}",
                            target_len.display_minimum()
                        ));
                        diag.annotate(
                            self.context
                                .secondary(value_expr)
                                .message(format_args!("Got {actual}")),
                        );
                    }
                }
            }
        }

        for (target, inferred) in targets.iter().zip(inferred_targets) {
            let contextual_ty = if let ast::Expr::Starred(starred) = target
                && !inferred.captured_alternatives.is_empty()
            {
                inference.infer_target(
                    &starred.value,
                    UnpackAssignedValue::Captured(UnpackCaptured::Alternatives(
                        &inferred.captured_alternatives,
                    )),
                )
            } else {
                None
            };
            let ty = contextual_ty.unwrap_or_else(|| inferred.ty.build());

            self.unpack_inner(
                target,
                inferred
                    .expression
                    .map(AnyNodeRef::from)
                    .unwrap_or(value_expr),
                UnpackElement {
                    ty,
                    expression: inferred.expression,
                    promote_literals: inferred.promote_literals,
                },
                inference,
            );
        }
    }

    pub(crate) fn finish(self) -> UnpackResult<'db> {
        UnpackResult {
            diagnostics: self.context.finish(),
            targets: FrozenMap::from(self.targets),
            bindings: FrozenMap::default(),
            cycle_recovery: None,
            value_inference: UnpackValueInference::Shared,
        }
    }
}

/// The source expression is shared by every target of a chained assignment. Contextual
/// inference replaces it; an ordinary member write adds only the expressions of that write.
///
/// ```python
/// class Holder:
///     pass
///
/// def make_values() -> tuple[int, int]:
///     return (1, 2)
///
/// holder = Holder()
/// first, second = holder.value, other = make_values()
/// ```
///
/// Both unpacked targets use the same inference of `make_values()`, but the second also
/// records the expressions visited while validating the `holder.value` assignment.
#[derive(Debug, Default, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum UnpackValueInference<'db> {
    #[default]
    Shared,
    Contextual(ExpressionInference<'db>),
    SharedWithWrites(ExpressionInference<'db>),
}

impl<'db> UnpackValueInference<'db> {
    fn cycle_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: &Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        match (self, previous) {
            (Self::Contextual(inference), Self::Contextual(previous)) => {
                Self::Contextual(inference.cycle_normalized(db, env, previous, cycle))
            }
            (Self::SharedWithWrites(inference), Self::SharedWithWrites(previous)) => {
                Self::SharedWithWrites(inference.cycle_normalized(db, env, previous, cycle))
            }
            // Different modes retain different expression keys. The ordinary source is
            // normalized by its own query; no previous local type exists for a new mode.
            (current, _) => current,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct UnpackResult<'db> {
    targets: FrozenMap<ExpressionNodeKey, Type<'db>>,
    /// Validated assignment-target bindings; excludes bindings created by the source expression.
    bindings: FrozenMap<Definition<'db>, Type<'db>>,
    diagnostics: TypeCheckDiagnostics,
    /// The source inference mode, which reuses the ordinary source query when no target supplies
    /// context and retains member-write expressions separately when necessary.
    value_inference: UnpackValueInference<'db>,
    /// The fallback type for missing expressions.
    ///
    /// This is used only when constructing a cycle-recovery `UnpackResult`.
    cycle_recovery: Option<Type<'db>>,
}

impl<'db> UnpackResult<'db> {
    pub(super) fn target_types(&self) -> impl Iterator<Item = (ExpressionNodeKey, Type<'db>)> + '_ {
        self.targets.iter().map(|(target, ty)| (*target, *ty))
    }

    pub(super) fn binding_types(&self) -> impl Iterator<Item = (Definition<'db>, Type<'db>)> + '_ {
        self.bindings
            .iter()
            .map(|(definition, ty)| (*definition, *ty))
    }

    pub(super) fn with_bindings(
        mut self,
        bindings: impl IntoIterator<Item = (Definition<'db>, Type<'db>)>,
    ) -> Self {
        self.bindings = bindings.into_iter().collect();
        self
    }

    pub(super) fn binding_type(&self, definition: Definition<'db>) -> Option<Type<'db>> {
        self.bindings
            .get(&definition)
            .copied()
            .or(self.cycle_recovery)
    }

    pub(super) fn with_value_inference(
        mut self,
        value_inference: UnpackValueInference<'db>,
    ) -> Self {
        self.value_inference = value_inference;
        self
    }

    /// Return the shared source, the contextual result, or the shared source followed by
    /// expressions from an ordinary member write. `value` must be the unpacked expression.
    pub(crate) fn value_inferences(
        &self,
        db: &'db dyn Db,
        value: Expression<'db>,
    ) -> impl Iterator<Item = &ExpressionInference<'db>> {
        let (shared, local) = match &self.value_inference {
            UnpackValueInference::Shared => (
                Some(infer_expression_types(db, value, TypeContext::default())),
                None,
            ),
            UnpackValueInference::Contextual(local) => (None, Some(local)),
            UnpackValueInference::SharedWithWrites(local) => (
                Some(infer_expression_types(db, value, TypeContext::default())),
                Some(local),
            ),
        };
        shared.into_iter().chain(local)
    }

    /// Returns the inferred type for a given sub-expression of the left-hand side target
    /// of an unpacking assignment.
    ///
    /// # Panics
    ///
    /// May panic if a scoped expression ID is passed in that does not correspond to a sub-
    /// expression of the target.
    #[track_caller]
    pub(crate) fn expression_type(&self, expr_id: impl Into<ExpressionNodeKey>) -> Type<'db> {
        self.try_expression_type(expr_id).expect(
            "expression should belong to this `UnpackResult` and \
            `Unpacker` should have inferred a type for it",
        )
    }

    fn try_expression_type(&self, expr: impl Into<ExpressionNodeKey>) -> Option<Type<'db>> {
        self.targets
            .get(&expr.into())
            .copied()
            .or(self.cycle_recovery)
    }

    /// Returns the diagnostics in this unpacking assignment.
    pub(crate) fn diagnostics(&self) -> &TypeCheckDiagnostics {
        &self.diagnostics
    }

    pub(crate) fn cycle_initial(scope: ScopeId<'db>, cycle_recovery: Type<'db>) -> Self {
        Self {
            targets: FrozenMap::default(),
            bindings: FrozenMap::default(),
            diagnostics: TypeCheckDiagnostics::default(),
            cycle_recovery: Some(cycle_recovery),
            value_inference: UnpackValueInference::Contextual(ExpressionInference::cycle_initial(
                scope,
                cycle_recovery,
            )),
        }
    }

    pub(crate) fn cycle_normalized(
        mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous_cycle_result: &UnpackResult<'db>,
        cycle: &salsa::Cycle,
    ) -> Self {
        for (expr, ty) in &mut self.targets {
            let previous_ty = previous_cycle_result.expression_type(*expr);
            *ty = ty.cycle_normalized(db, env, previous_ty, cycle);
        }

        for (definition, ty) in &mut self.bindings {
            *ty = if let Some(previous_ty) = previous_cycle_result.binding_type(*definition) {
                ty.cycle_normalized(db, env, previous_ty, cycle)
            } else {
                ty.recursive_type_normalized(db, env, cycle)
            };
        }

        self.value_inference = std::mem::take(&mut self.value_inference).cycle_normalized(
            db,
            env,
            &previous_cycle_result.value_inference,
            cycle,
        );

        self
    }
}

/// Return a tuple or list's elements when they correspond exactly to a fixed-length sequence.
pub(super) fn fixed_sequence_elements(
    expression: &ast::Expr,
    expected_length: usize,
) -> Option<&[ast::Expr]> {
    let elements = sequence_elts(expression)?;

    if elements.len() != expected_length {
        return None;
    }

    elements
        .iter()
        .all(|element| !element.is_starred_expr())
        .then_some(elements)
}

/// Find the expression assigned to one target in a tuple or list unpacking.
///
/// For `first, (second, third) = (0, (1, 2))`, this associates `second` with `1`.
/// Explicit values before or after a starred element remain unambiguous. Literal expansions
/// retain their source expressions too; values supplied by arbitrary iterables do not.
pub(super) fn unpacked_assignment_value<'ast>(
    unpack_target: &ast::Expr,
    value: &'ast ast::Expr,
    requested_target: &ast::Expr,
) -> Option<&'ast ast::Expr> {
    assignment_values_for_target(unpack_target, value, requested_target)
        .and_then(UnpackedAssignmentValues::into_single)
}

/// Return the explicit values collected by a starred assignment target.
///
/// For `first, *middle, last = (0, 1, 2, 3)`, `middle` collects the expressions `1` and `2`.
/// Literal expansions are flattened; an unknown source in the collected portion makes the
/// correspondence ambiguous.
pub(super) fn starred_assignment_values<'ast>(
    unpack_target: &ast::Expr,
    value: &'ast ast::Expr,
    requested_target: &ast::Expr,
) -> Option<Vec<&'ast ast::Expr>> {
    assignment_values_for_target(unpack_target, value, requested_target)
        .and_then(UnpackedAssignmentValues::into_collected)
}

#[derive(Debug, Clone)]
enum UnpackedAssignmentValues<'ast> {
    Single(&'ast ast::Expr),
    Collected(Vec<&'ast ast::Expr>),
}

impl<'ast> UnpackedAssignmentValues<'ast> {
    fn into_single(self) -> Option<&'ast ast::Expr> {
        match self {
            Self::Single(value) => Some(value),
            Self::Collected(_) => None,
        }
    }

    fn into_collected(self) -> Option<Vec<&'ast ast::Expr>> {
        match self {
            Self::Single(_) => None,
            Self::Collected(values) => Some(values),
        }
    }
}

fn assignment_values_for_target<'ast>(
    unpack_target: &ast::Expr,
    value: &'ast ast::Expr,
    requested_target: &ast::Expr,
) -> Option<UnpackedAssignmentValues<'ast>> {
    if ExpressionNodeKey::from(unpack_target) == ExpressionNodeKey::from(requested_target) {
        return Some(UnpackedAssignmentValues::Single(value));
    }

    let targets = sequence_elts(unpack_target)?;
    let matched = unpack_literal_values(targets, value)?;
    let (target, source) = targets
        .iter()
        .zip(matched.into_all_elements_with_kind())
        .find(|(target, _)| target.range().contains_range(requested_target.range()))?;
    match source {
        TupleElement::Variable(values) => {
            let ast::Expr::Starred(starred) = target else {
                return None;
            };
            if ExpressionNodeKey::from(starred.value.as_ref())
                != ExpressionNodeKey::from(requested_target)
            {
                return None;
            }
            Some(UnpackedAssignmentValues::Collected(
                values.into_iter().collect::<Option<Vec<_>>>()?,
            ))
        }
        TupleElement::Fixed(value) | TupleElement::Prefix(value) | TupleElement::Suffix(value) => {
            assignment_values_for_target(target, value?, requested_target)
        }
    }
}

/// Match literal source expressions to targets without inferring their types.
/// An ambiguous position has no expression; a starred target retains its individual sources.
pub(super) fn literal_source_expressions(
    value: &ast::Expr,
) -> Option<Tuple<Option<&ast::Expr>, Vec<Option<&ast::Expr>>>> {
    literal_sequence(
        value,
        false,
        &|expression, _| Some(expression),
        &|_, _, known_length| {
            if let Some(length) = known_length {
                Tuple::heterogeneous(std::iter::repeat_n(None, length))
            } else {
                VariableLengthTuple::mixed([], vec![None], [])
            }
        },
    )
}

fn unpack_literal_values<'ast>(
    targets: &[ast::Expr],
    value: &'ast ast::Expr,
) -> Option<Tuple<Option<&'ast ast::Expr>, Vec<Option<&'ast ast::Expr>>>> {
    literal_source_expressions(value)?
        .unpack(target_length(targets), Clone::clone, |_| None)
        .ok()
}

pub(super) fn target_length(targets: &[ast::Expr]) -> TupleLength {
    match targets.iter().position(ast::Expr::is_starred_expr) {
        Some(index) => TupleLength::Variable(index, targets.len() - index - 1),
        None => TupleLength::Fixed(targets.len()),
    }
}

/// A source expression accompanies a type only when its position is unambiguous. We keep this
/// transient information while unpacking, without giving mutable lists fixed-length types.
#[derive(Clone, Copy)]
pub(super) struct UnpackElement<'db, 'ast> {
    pub(super) ty: Type<'db>,
    pub(super) expression: Option<&'ast ast::Expr>,
    /// Widening a large tuple also widens nested tuple elements. Do not undo that widening
    /// when following the source expression during nested unpacking.
    promote_literals: bool,
}

impl<'db> UnpackElement<'db, '_> {
    fn from_type(ty: Type<'db>) -> Self {
        Self {
            ty,
            expression: None,
            promote_literals: false,
        }
    }
}

fn sequence_from_type<'db, 'ast>(
    db: &'db dyn Db,
    tuple: &TupleSpec<'db>,
) -> Tuple<UnpackElement<'db, 'ast>, Vec<UnpackElement<'db, 'ast>>> {
    match tuple {
        Tuple::Fixed(values) => {
            Tuple::heterogeneous(values.iter_all_elements().map(UnpackElement::from_type))
        }
        Tuple::Variable(values) => VariableLengthTuple::mixed(
            values.iter_prefix_elements().map(UnpackElement::from_type),
            vec![UnpackElement::from_type(values.variable().element_type(db))],
            values.iter_suffix_elements().map(UnpackElement::from_type),
        ),
    }
}

/// Infers the fresh list made by a starred assignment target or sequence-pattern capture.
/// Both `first, *rest = values` and `case [first, *rest]:` create a new list whose inferred
/// literal elements can widen without changing the type of the original sequence.
pub(super) fn collected_list_type<'db, 'ast>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    values: impl ExactSizeIterator<Item = (Type<'db>, Option<&'ast ast::Expr>)>,
) -> Type<'db> {
    let is_empty = values.len() == 0;
    let mut elements = UnionBuilder::new(db, env).unpack_aliases(false);
    let mut allow_tuple_size_promotion = true;
    for (ty, expression) in values {
        let ty = ty.promote(db, env);
        allow_tuple_size_promotion &=
            TupleSizePromotionConstraints::allows_expression(db, env, expression, ty);
        elements.add_in_place(ty);
    }
    // `first, *rest = (1,)` constructs an empty list, just as `rest = []` does.
    let ty = if is_empty {
        Type::unknown()
    } else {
        elements.build()
    };
    let ty = ty.promote_collection_element_type(db, env, allow_tuple_size_promotion, true);
    KnownClass::List.to_specialized_instance(db, env, &[ty])
}

/// Describes literal positions for both inference and diagnostics. A starred literal is expanded
/// recursively; other starred expressions contribute the shape supplied by the caller. For
/// `first, *rest, last = [1, *items, 2]`, the unknown width of `items` leaves both ends intact.
fn literal_sequence<'ast, T: Clone>(
    expression: &'ast ast::Expr,
    promote: bool,
    element: &impl Fn(&'ast ast::Expr, bool) -> T,
    spread: &impl Fn(&'ast ast::Expr, bool, Option<usize>) -> Tuple<T, Vec<T>>,
) -> Option<Tuple<T, Vec<T>>> {
    let (values, promote) = literal_sequence_elements(expression, promote)?;
    Some(sequence_from_literal_elements(
        values,
        promote,
        element,
        spread,
        &|builder, unpacked| {
            builder.concat_with(unpacked, |suffix, left, right, prefix| {
                // For `[*a, *b, *c, ...]`, retain the accumulated elements instead of copying
                // them again for every expansion. Positions within this segment are unknown.
                left.extend(suffix.iter().chain(prefix).chain(right).cloned());
            })
        },
    ))
}

fn literal_sequence_elements(
    expression: &ast::Expr,
    promote: bool,
) -> Option<(&[ast::Expr], bool)> {
    // `a, *rest = (items := [1, "two"])` has the same elements as the list itself.
    let expression = expression.expression_value();
    let values = sequence_elts(expression)?;
    let promote = promote || (expression.is_tuple_expr() && tuple_literal_needs_promotion(values));
    Some((values, promote))
}

/// Only literal expansions have expressions that can be matched to individual targets.
/// For `first, second = (*[[1]], 0)`, `[1]` can receive `first`'s context. An
/// expansion of an existing iterable, as in `(*values, 0)`, has no such correspondence.
fn expansions_have_literal_elements(values: &[ast::Expr]) -> bool {
    values.iter().all(|value| match value {
        ast::Expr::Starred(starred) => {
            sequence_elts(&starred.value).is_some_and(expansions_have_literal_elements)
        }
        _ => true,
    })
}

/// Applies the tuple precision limit after expanding literal elements. For `(*[1, 2], 3)`,
/// all three positions count, even though the outer tuple has only two AST elements.
/// Other starred iterables count as one item because their elements are not recovered from
/// literal syntax. Stop counting as soon as the limit is exceeded.
pub(super) fn tuple_literal_needs_promotion(values: &[ast::Expr]) -> bool {
    /// Limit literal precision in large tuple expressions to avoid pathological inference costs.
    const MAX_TUPLE_LENGTH_FOR_UNANNOTATED_LITERAL_INFERENCE: usize = 64;

    fn remaining_budget(values: &[ast::Expr], remaining: usize) -> Option<usize> {
        values.iter().try_fold(remaining, |remaining, value| {
            if let ast::Expr::Starred(starred) = value
                && let Some(values) = sequence_elts(starred.value.expression_value())
            {
                remaining_budget(values, remaining)
            } else {
                remaining.checked_sub(1)
            }
        })
    }

    remaining_budget(values, MAX_TUPLE_LENGTH_FOR_UNANNOTATED_LITERAL_INFERENCE).is_none()
}

/// Builds a literal's sequence shape from already-inferred elements and iterable shapes.
/// In `source = (*[1, "two"],)`, expanding the list syntax preserves both tuple positions.
/// The caller chooses the variable-segment representation and how to concatenate it: tuple
/// inference retains symbolic `TypeVarTuple` segments, while unpacking retains source expressions.
pub(super) fn sequence_from_literal_elements<'ast, T, V>(
    values: &'ast [ast::Expr],
    promote: bool,
    element: &impl Fn(&'ast ast::Expr, bool) -> T,
    spread: &impl Fn(&'ast ast::Expr, bool, Option<usize>) -> Tuple<T, V>,
    concat: &impl Fn(TupleBuilder<T, V>, &Tuple<T, V>) -> TupleBuilder<T, V>,
) -> Tuple<T, V> {
    let mut builder = TupleBuilder::with_capacity(values.len());
    for value in values {
        if let ast::Expr::Starred(starred) = value {
            let unpacked = literal_sequence_elements(&starred.value, promote)
                .map(|(values, promote)| {
                    sequence_from_literal_elements(values, promote, element, spread, concat)
                })
                .unwrap_or_else(|| spread(value, promote, literal_iterable_length(&starred.value)));
            builder = concat(builder, &unpacked);
        } else {
            builder.push(element(value, promote));
        }
    }
    builder.build()
}

/// The literal element count used when inferring expansions such as `(*{"key": 1},)`.
/// An expansion within a set or dictionary, as in `{*items}` or `{**items}`, makes that count unknown.
fn literal_iterable_length(expression: &ast::Expr) -> Option<usize> {
    match expression {
        ast::Expr::Set(ast::ExprSet { elts, .. }) => elts
            .iter()
            .all(|element| !element.is_starred_expr())
            .then_some(elts.len()),
        ast::Expr::Dict(ast::ExprDict { items, .. }) => items
            .iter()
            .all(|item| item.key.is_some())
            .then_some(items.len()),
        _ => None,
    }
}

/// Extract the element slice from a list or tuple expression.
pub(super) fn sequence_elts(expr: &ast::Expr) -> Option<&[ast::Expr]> {
    match expr {
        ast::Expr::List(list) => Some(&list.elts),
        ast::Expr::Tuple(tuple) => Some(&tuple.elts),
        _ => None,
    }
}
