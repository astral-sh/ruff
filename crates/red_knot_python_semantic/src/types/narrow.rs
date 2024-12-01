use crate::semantic_index::ast_ids::HasScopedExpressionId;
use crate::semantic_index::constraint::{Constraint, ConstraintNode, PatternConstraint};
use crate::semantic_index::definition::Definition;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{ScopeId, ScopedSymbolId, SymbolTable};
use crate::semantic_index::symbol_table;
use crate::types::{
    infer_expression_types, ClassLiteralType, IntersectionBuilder, KnownClass,
    KnownConstraintFunction, KnownFunction, Truthiness, Type, UnionBuilder, UnionType,
};
use crate::Db;
use itertools::Itertools;
use ruff_python_ast as ast;
use ruff_python_ast::{BoolOp, ExprBoolOp};
use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

/// Return the type constraint that `test` (if true) would place on `definition`, if any.
///
/// For example, if we have this code:
///
/// ```python
/// y = 1 if flag else None
/// x = 1 if flag else None
/// if x is not None:
///     ...
/// ```
///
/// The `test` expression `x is not None` places the constraint "not None" on the definition of
/// `x`, so in that case we'd return `Some(Type::Intersection(negative=[Type::None]))`.
///
/// But if we called this with the same `test` expression, but the `definition` of `y`, no
/// constraint is applied to that definition, so we'd just return `None`.
pub(crate) fn narrowing_constraint<'db>(
    db: &'db dyn Db,
    constraint: Constraint<'db>,
    definition: Definition<'db>,
) -> Option<NarrowingType<'db>> {
    let constraints = match constraint.node {
        ConstraintNode::Expression(expression) => {
            if constraint.is_positive {
                all_narrowing_constraints_for_expression(db, expression)
            } else {
                all_negative_narrowing_constraints_for_expression(db, expression)
            }
        }
        ConstraintNode::Pattern(pattern) => all_narrowing_constraints_for_pattern(db, pattern),
    };
    if let Some(constraints) = constraints {
        constraints.get(&definition.symbol(db)).copied()
    } else {
        None
    }
}

#[salsa::tracked(return_ref)]
fn all_narrowing_constraints_for_pattern<'db>(
    db: &'db dyn Db,
    pattern: PatternConstraint<'db>,
) -> Option<NarrowingConstraints<'db>> {
    NarrowingConstraintsBuilder::new(db, ConstraintNode::Pattern(pattern), true).finish()
}

#[salsa::tracked(return_ref)]
fn all_narrowing_constraints_for_expression<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> Option<NarrowingConstraints<'db>> {
    NarrowingConstraintsBuilder::new(db, ConstraintNode::Expression(expression), true).finish()
}

#[salsa::tracked(return_ref)]
fn all_negative_narrowing_constraints_for_expression<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> Option<NarrowingConstraints<'db>> {
    NarrowingConstraintsBuilder::new(db, ConstraintNode::Expression(expression), false).finish()
}

/// Generate a constraint from the type of a `classinfo` argument to `isinstance` or `issubclass`.
///
/// The `classinfo` argument can be a class literal, a tuple of (tuples of) class literals. PEP 604
/// union types are not yet supported. Returns `None` if the `classinfo` argument has a wrong type.
fn generate_classinfo_constraint<'db, F>(
    db: &'db dyn Db,
    classinfo: &Type<'db>,
    to_constraint: F,
) -> Option<Type<'db>>
where
    F: Fn(ClassLiteralType<'db>) -> Type<'db> + Copy,
{
    match classinfo {
        Type::Tuple(tuple) => {
            let mut builder = UnionBuilder::new(db);
            for element in tuple.elements(db) {
                builder = builder.add(generate_classinfo_constraint(db, element, to_constraint)?);
            }
            Some(builder.build())
        }
        Type::ClassLiteral(class_literal_type) => Some(to_constraint(*class_literal_type)),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub(crate) enum NarrowingType<'db> {
    Deferred(DeferredType),
    Eager(Type<'db>),
    Intersection(NarrowingIntersectionType<'db>),
    Union(NarrowingUnionType<'db>),
}

impl<'db> NarrowingType<'db> {
    /// A simplified version of `NarrowingIntersectionBuilder`.
    ///
    /// TODO: Further normalization needed (e.g., ignore if an identical `DeferredType` already exists).
    fn intersection(
        db: &'db dyn Db,
        left: NarrowingType<'db>,
        right: NarrowingType<'db>,
    ) -> NarrowingType<'db> {
        match (left, right) {
            (NarrowingType::Deferred(deferred_left), NarrowingType::Deferred(deferred_right)) => {
                deferred_left.intersection(db, deferred_right)
            }
            (NarrowingType::Eager(eager_left), NarrowingType::Eager(eager_right)) => {
                NarrowingType::Eager(
                    IntersectionBuilder::new(db)
                        .add_positive(eager_left)
                        .add_positive(eager_right)
                        .build(),
                )
            }
            (left, right) => NarrowingType::Intersection(NarrowingIntersectionType::new(
                db,
                Box::from([left, right]),
            )),
        }
    }

    /// A simplified version of `NarrowingUnionBuilder`.
    ///
    /// TODO: Further normalization needed (e.g., ignore if an identical `DeferredType` already exists).
    fn union(
        db: &'db dyn Db,
        left: NarrowingType<'db>,
        right: NarrowingType<'db>,
    ) -> NarrowingType<'db> {
        match (left, right) {
            (NarrowingType::Deferred(deferred_left), NarrowingType::Deferred(deferred_right)) => {
                deferred_left.union(db, deferred_right)
            }
            (NarrowingType::Eager(eager_left), NarrowingType::Eager(eager_right)) => {
                NarrowingType::Eager(UnionType::from_elements(db, [eager_left, eager_right]))
            }
            (left, right) => NarrowingType::Union(NarrowingUnionType::new(
                db,
                Box::new([left, right]) as Box<[NarrowingType<'db>]>,
            )),
        }
    }

    pub(crate) fn evaluate(&self, db: &'db dyn Db, base_type: Type<'db>) -> Type<'db> {
        match self {
            NarrowingType::Deferred(deferred_type) => deferred_type.evaluate(db, base_type),
            NarrowingType::Eager(ty) => *ty,
            NarrowingType::Intersection(intersection) => {
                let mut builder = IntersectionBuilder::new(db);
                for element in intersection.elements(db) {
                    builder = builder.add_positive(element.evaluate(db, base_type));
                }
                builder.build()
            }
            NarrowingType::Union(union) => {
                let elements = union
                    .elements(db)
                    .iter()
                    .map(|element| element.evaluate(db, base_type));

                UnionType::from_elements(db, elements)
            }
        }
    }
}

/// A collection of temporary types that can only be used during the narrowing phase.
/// The sets in this enum must meet the following criteria:
/// - Represent sets that are difficult to express with a single `Type`.
/// - Ensure no information is lost during the process of building narrowing constraints.
///
/// These enums remain unevaluated during the narrowing constraint building phase
/// and are eventually intersected with bindings at the final stage.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub(crate) enum DeferredType {
    // Truthiness Sets
    Truthy, // AlwaysTruthy + AmbiguousTruthiness
    Falsy,  // AlwaysFalsy + AmbiguousTruthiness
            // TODO?: EqualTo Sets
            // EqualTo(Type<'db>) ? // AlwaysEqualTo(..) + AmbiguousEqualTo(..)
            // NotEqualTo(Type<'db>)  ? // NeverEqualTo(..) + AmbiguousEqualTo(..)
}

impl DeferredType {
    fn evaluate<'db>(self, db: &'db dyn Db, base_type: Type<'db>) -> Type<'db> {
        match self {
            DeferredType::Truthy => base_type.exclude_always_falsy(db),
            DeferredType::Falsy => base_type.exclude_always_truthy(db),
        }
    }

    fn intersection<'db>(self, db: &'db dyn Db, other: DeferredType) -> NarrowingType<'db> {
        match (self, other) {
            (DeferredType::Truthy, DeferredType::Truthy)
            | (DeferredType::Falsy, DeferredType::Falsy) => NarrowingType::Deferred(self),
            (DeferredType::Truthy, DeferredType::Falsy)
            | (DeferredType::Falsy, DeferredType::Truthy) => {
                // (Truthy & Falsy) is not an empty set.
                // The result will be an AmbiguousTruthiness set,
                // which includes mutable instances like (`list()`, `set()`),
                // or instances with custom __bool__ implementations that return random results.
                NarrowingType::Intersection(NarrowingIntersectionType::<'db>::new(
                    db,
                    Box::from([
                        NarrowingType::Deferred(self),
                        NarrowingType::Deferred(other),
                    ]),
                ))
            }
        }
    }

    fn union(self, db: &'_ dyn Db, other: DeferredType) -> NarrowingType<'_> {
        match (self, other) {
            (DeferredType::Truthy, DeferredType::Truthy)
            | (DeferredType::Falsy, DeferredType::Falsy) => NarrowingType::Deferred(self),
            (DeferredType::Truthy, DeferredType::Falsy)
            | (DeferredType::Falsy, DeferredType::Truthy) => {
                NarrowingType::Eager(KnownClass::Object.to_instance(db))
            }
        }
    }
}

#[salsa::interned]
pub(crate) struct NarrowingIntersectionType<'db> {
    #[return_ref]
    elements_boxed: Box<[NarrowingType<'db>]>,
}

impl<'db> NarrowingIntersectionType<'db> {
    fn elements(self, db: &'db dyn Db) -> &'db [NarrowingType<'db>] {
        self.elements_boxed(db)
    }
}

#[salsa::interned]
pub(crate) struct NarrowingUnionType<'db> {
    #[return_ref]
    elements_boxed: Box<[NarrowingType<'db>]>,
}

impl<'db> NarrowingUnionType<'db> {
    fn elements(self, db: &'db dyn Db) -> &'db [NarrowingType<'db>] {
        self.elements_boxed(db)
    }
}

type NarrowingConstraints<'db> = FxHashMap<ScopedSymbolId, NarrowingType<'db>>;

fn merge_constraints_and<'db>(
    into: &mut NarrowingConstraints<'db>,
    from: NarrowingConstraints<'db>,
    db: &'db dyn Db,
) {
    for (key, value) in from {
        match into.entry(key) {
            Entry::Occupied(mut entry) => {
                *entry.get_mut() = NarrowingType::intersection(db, *entry.get(), value);
            }
            Entry::Vacant(entry) => {
                entry.insert(value);
            }
        }
    }
}

fn merge_constraints_or<'db>(
    into: &mut NarrowingConstraints<'db>,
    from: &NarrowingConstraints<'db>,
    db: &'db dyn Db,
) {
    for (key, value) in from {
        match into.entry(*key) {
            Entry::Occupied(mut entry) => {
                *entry.get_mut() = NarrowingType::union(db, *entry.get(), *value);
            }
            Entry::Vacant(entry) => {
                entry.insert(NarrowingType::Eager(KnownClass::Object.to_instance(db)));
            }
        }
    }
    for (key, value) in into.iter_mut() {
        if !from.contains_key(key) {
            *value = NarrowingType::Eager(KnownClass::Object.to_instance(db));
        }
    }
}

struct NarrowingConstraintsBuilder<'db> {
    db: &'db dyn Db,
    constraint: ConstraintNode<'db>,
    is_positive: bool,
}

impl<'db> NarrowingConstraintsBuilder<'db> {
    fn new(db: &'db dyn Db, constraint: ConstraintNode<'db>, is_positive: bool) -> Self {
        Self {
            db,
            constraint,
            is_positive,
        }
    }

    fn finish(mut self) -> Option<NarrowingConstraints<'db>> {
        let constraints: Option<NarrowingConstraints<'db>> = match self.constraint {
            ConstraintNode::Expression(expression) => {
                self.evaluate_expression_constraint(expression, self.is_positive)
            }
            ConstraintNode::Pattern(pattern) => self.evaluate_pattern_constraint(pattern),
        };
        if let Some(mut constraints) = constraints {
            constraints.shrink_to_fit();
            Some(constraints)
        } else {
            None
        }
    }

    fn evaluate_expression_constraint(
        &mut self,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        let expression_node = expression.node_ref(self.db).node();
        self.evaluate_expression_node_constraint(expression_node, expression, is_positive)
    }

    fn evaluate_expression_node_constraint(
        &mut self,
        expression_node: &ruff_python_ast::Expr,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        match expression_node {
            ast::Expr::Name(name) => Some(self.evaluate_expr_name(name, is_positive)),
            ast::Expr::Compare(expr_compare) => {
                self.evaluate_expr_compare(expr_compare, expression, is_positive)
            }
            ast::Expr::Call(expr_call) => {
                self.evaluate_expr_call(expr_call, expression, is_positive)
            }
            ast::Expr::UnaryOp(unary_op) if unary_op.op == ast::UnaryOp::Not => self
                .evaluate_expression_node_constraint(&unary_op.operand, expression, !is_positive),
            ast::Expr::BoolOp(bool_op) => self.evaluate_bool_op(bool_op, expression, is_positive),
            _ => None, // TODO other test expression kinds
        }
    }

    fn evaluate_pattern_constraint(
        &mut self,
        pattern: PatternConstraint<'db>,
    ) -> Option<NarrowingConstraints<'db>> {
        let subject = pattern.subject(self.db);

        match pattern.pattern(self.db).node() {
            ast::Pattern::MatchValue(_) => {
                None // TODO
            }
            ast::Pattern::MatchSingleton(singleton_pattern) => {
                self.evaluate_match_pattern_singleton(subject, singleton_pattern)
            }
            ast::Pattern::MatchSequence(_) => {
                None // TODO
            }
            ast::Pattern::MatchMapping(_) => {
                None // TODO
            }
            ast::Pattern::MatchClass(_) => {
                None // TODO
            }
            ast::Pattern::MatchStar(_) => {
                None // TODO
            }
            ast::Pattern::MatchAs(_) => {
                None // TODO
            }
            ast::Pattern::MatchOr(_) => {
                None // TODO
            }
        }
    }

    fn symbols(&self) -> Arc<SymbolTable> {
        symbol_table(self.db, self.scope())
    }

    fn scope(&self) -> ScopeId<'db> {
        match self.constraint {
            ConstraintNode::Expression(expression) => expression.scope(self.db),
            ConstraintNode::Pattern(pattern) => pattern.scope(self.db),
        }
    }

    fn evaluate_expr_name(
        &mut self,
        expr_name: &ast::ExprName,
        is_positive: bool,
    ) -> NarrowingConstraints<'db> {
        let ast::ExprName { id, .. } = expr_name;

        let symbol = self
            .symbols()
            .symbol_id_by_name(id)
            .expect("Should always have a symbol for every Name node");
        let mut constraints = NarrowingConstraints::default();

        if is_positive {
            constraints.insert(symbol, NarrowingType::Deferred(DeferredType::Truthy));
        } else {
            constraints.insert(symbol, NarrowingType::Deferred(DeferredType::Falsy));
        }

        constraints
    }

    fn evaluate_expr_compare(
        &mut self,
        expr_compare: &ast::ExprCompare,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        fn is_narrowing_target_candidate(expr: &ast::Expr) -> bool {
            matches!(expr, ast::Expr::Name(_) | ast::Expr::Call(_))
        }

        let ast::ExprCompare {
            range: _,
            left,
            ops,
            comparators,
        } = expr_compare;

        // Performance optimization: early return if there are no potential narrowing targets.
        if !is_narrowing_target_candidate(left)
            && comparators
                .iter()
                .all(|c| !is_narrowing_target_candidate(c))
        {
            return None;
        }

        if !is_positive && comparators.len() > 1 {
            // We can't negate a constraint made by a multi-comparator expression, since we can't
            // know which comparison part is the one being negated.
            // For example, the negation of  `x is 1 is y is 2`, would be `(x is not 1) or (y is not 1) or (y is not 2)`
            // and that requires cross-symbol constraints, which we don't support yet.
            return None;
        }
        let scope = self.scope();
        let inference = infer_expression_types(self.db, expression);

        let comparator_tuples = std::iter::once(&**left)
            .chain(comparators)
            .tuple_windows::<(&ruff_python_ast::Expr, &ruff_python_ast::Expr)>();
        let mut constraints = NarrowingConstraints::default();
        for (op, (left, right)) in std::iter::zip(&**ops, comparator_tuples) {
            let rhs_ty = inference.expression_ty(right.scoped_expression_id(self.db, scope));

            match left {
                ast::Expr::Name(ast::ExprName {
                    range: _,
                    id,
                    ctx: _,
                }) => {
                    let symbol = self
                        .symbols()
                        .symbol_id_by_name(id)
                        .expect("Should always have a symbol for every Name node");

                    match if is_positive { *op } else { op.negate() } {
                        ast::CmpOp::IsNot => {
                            if rhs_ty.is_singleton(self.db) {
                                let ty = IntersectionBuilder::new(self.db)
                                    .add_negative(rhs_ty)
                                    .build();
                                constraints.insert(symbol, NarrowingType::Eager(ty));
                            } else {
                                // Non-singletons cannot be safely narrowed using `is not`
                            }
                        }
                        ast::CmpOp::Is => {
                            constraints.insert(symbol, NarrowingType::Eager(rhs_ty));
                        }
                        ast::CmpOp::NotEq => {
                            if rhs_ty.is_single_valued(self.db) {
                                let ty = IntersectionBuilder::new(self.db)
                                    .add_negative(rhs_ty)
                                    .build();
                                constraints.insert(symbol, NarrowingType::Eager(ty));
                            }
                        }
                        _ => {
                            // TODO other comparison types
                        }
                    }
                }
                ast::Expr::Call(ast::ExprCall {
                    range: _,
                    func: callable,
                    arguments:
                        ast::Arguments {
                            args,
                            keywords,
                            range: _,
                        },
                }) if rhs_ty.is_class_literal() && keywords.is_empty() => {
                    let [ast::Expr::Name(ast::ExprName { id, .. })] = &**args else {
                        continue;
                    };

                    let is_valid_constraint = if is_positive {
                        op == &ast::CmpOp::Is
                    } else {
                        op == &ast::CmpOp::IsNot
                    };

                    if !is_valid_constraint {
                        continue;
                    }

                    let callable_ty =
                        inference.expression_ty(callable.scoped_expression_id(self.db, scope));

                    if callable_ty
                        .into_class_literal()
                        .is_some_and(|c| c.class.is_known(self.db, KnownClass::Type))
                    {
                        let symbol = self
                            .symbols()
                            .symbol_id_by_name(id)
                            .expect("Should always have a symbol for every Name node");
                        constraints
                            .insert(symbol, NarrowingType::Eager(rhs_ty.to_instance(self.db)));
                    }
                }
                _ => {}
            }
        }
        Some(constraints)
    }

    fn evaluate_expr_call(
        &mut self,
        expr_call: &ast::ExprCall,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        let scope = self.scope();
        let inference = infer_expression_types(self.db, expression);

        // TODO: add support for PEP 604 union types on the right hand side of `isinstance`
        // and `issubclass`, for example `isinstance(x, str | (int | float))`.
        match inference
            .expression_ty(expr_call.func.scoped_expression_id(self.db, scope))
            .into_function_literal()
            .and_then(|f| f.known(self.db))
            .and_then(KnownFunction::constraint_function)
        {
            Some(function) if expr_call.arguments.keywords.is_empty() => {
                if let [ast::Expr::Name(ast::ExprName { id, .. }), class_info] =
                    &*expr_call.arguments.args
                {
                    let symbol = self.symbols().symbol_id_by_name(id).unwrap();

                    let class_info_ty =
                        inference.expression_ty(class_info.scoped_expression_id(self.db, scope));

                    let to_constraint = match function {
                        KnownConstraintFunction::IsInstance => {
                            |class_literal: ClassLiteralType<'db>| {
                                Type::instance(class_literal.class)
                            }
                        }
                        KnownConstraintFunction::IsSubclass => {
                            |class_literal: ClassLiteralType<'db>| {
                                Type::subclass_of(class_literal.class)
                            }
                        }
                    };

                    generate_classinfo_constraint(self.db, &class_info_ty, to_constraint).map(
                        |constraint| {
                            let mut constraints = NarrowingConstraints::default();
                            constraints.insert(
                                symbol,
                                NarrowingType::Eager(constraint.negate_if(self.db, !is_positive)),
                            );
                            constraints
                        },
                    )
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn evaluate_match_pattern_singleton(
        &mut self,
        subject: &ast::Expr,
        pattern: &ast::PatternMatchSingleton,
    ) -> Option<NarrowingConstraints<'db>> {
        if let Some(ast::ExprName { id, .. }) = subject.as_name_expr() {
            // SAFETY: we should always have a symbol for every Name node.
            let symbol = self.symbols().symbol_id_by_name(id).unwrap();

            let ty = match pattern.value {
                ast::Singleton::None => Type::none(self.db),
                ast::Singleton::True => Type::BooleanLiteral(true),
                ast::Singleton::False => Type::BooleanLiteral(false),
            };
            let mut constraints = NarrowingConstraints::default();
            constraints.insert(symbol, NarrowingType::Eager(ty));
            Some(constraints)
        } else {
            None
        }
    }

    fn evaluate_bool_op(
        &mut self,
        expr_bool_op: &ExprBoolOp,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        let inference = infer_expression_types(self.db, expression);
        let scope = self.scope();
        let mut sub_constraints = expr_bool_op
            .values
            .iter()
            // filter our arms with statically known truthiness
            .filter(|expr| {
                inference
                    .expression_ty(expr.scoped_expression_id(self.db, scope))
                    .bool(self.db)
                    != match expr_bool_op.op {
                        BoolOp::And => Truthiness::AlwaysTrue,
                        BoolOp::Or => Truthiness::AlwaysFalse,
                    }
            })
            .map(|sub_expr| {
                self.evaluate_expression_node_constraint(sub_expr, expression, is_positive)
            })
            .collect::<Vec<_>>();
        match (expr_bool_op.op, is_positive) {
            (BoolOp::And, true) | (BoolOp::Or, false) => {
                let mut aggregation: Option<NarrowingConstraints> = None;
                for sub_constraint in sub_constraints.into_iter().flatten() {
                    if let Some(ref mut some_aggregation) = aggregation {
                        merge_constraints_and(some_aggregation, sub_constraint, self.db);
                    } else {
                        aggregation = Some(sub_constraint);
                    }
                }
                aggregation
            }
            (BoolOp::Or, true) | (BoolOp::And, false) => {
                let (first, rest) = sub_constraints.split_first_mut()?;
                if let Some(ref mut first) = first {
                    for rest_constraint in rest {
                        if let Some(rest_constraint) = rest_constraint {
                            merge_constraints_or(first, rest_constraint, self.db);
                        } else {
                            return None;
                        }
                    }
                }
                first.clone()
            }
        }
    }
}
