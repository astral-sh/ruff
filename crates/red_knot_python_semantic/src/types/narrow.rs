use crate::semantic_index::ast_ids::HasScopedExpressionId;
use crate::semantic_index::constraint::{
    Constraint, ConstraintNode, PatternConstraint, PatternConstraintKind,
};
use crate::semantic_index::definition::Definition;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{ScopeId, ScopedSymbolId, SymbolTable};
use crate::semantic_index::symbol_table;
use crate::types::{
    infer_expression_types, ClassLiteralType, IntersectionBuilder, KnownClass, KnownFunction,
    SubclassOfType, Truthiness, Type, UnionBuilder,
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
) -> Option<Type<'db>> {
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

#[allow(clippy::ref_option)]
#[salsa::tracked(return_ref)]
fn all_narrowing_constraints_for_pattern<'db>(
    db: &'db dyn Db,
    pattern: PatternConstraint<'db>,
) -> Option<NarrowingConstraints<'db>> {
    NarrowingConstraintsBuilder::new(db, ConstraintNode::Pattern(pattern), true).finish()
}

#[allow(clippy::ref_option)]
#[salsa::tracked(return_ref)]
fn all_narrowing_constraints_for_expression<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> Option<NarrowingConstraints<'db>> {
    NarrowingConstraintsBuilder::new(db, ConstraintNode::Expression(expression), true).finish()
}

#[allow(clippy::ref_option)]
#[salsa::tracked(return_ref)]
fn all_negative_narrowing_constraints_for_expression<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> Option<NarrowingConstraints<'db>> {
    NarrowingConstraintsBuilder::new(db, ConstraintNode::Expression(expression), false).finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnownConstraintFunction {
    /// `builtins.isinstance`
    IsInstance,
    /// `builtins.issubclass`
    IsSubclass,
}

impl KnownConstraintFunction {
    /// Generate a constraint from the type of a `classinfo` argument to `isinstance` or `issubclass`.
    ///
    /// The `classinfo` argument can be a class literal, a tuple of (tuples of) class literals. PEP 604
    /// union types are not yet supported. Returns `None` if the `classinfo` argument has a wrong type.
    fn generate_constraint<'db>(self, db: &'db dyn Db, classinfo: Type<'db>) -> Option<Type<'db>> {
        let constraint_fn = |class| match self {
            KnownConstraintFunction::IsInstance => Type::instance(class),
            KnownConstraintFunction::IsSubclass => SubclassOfType::from(db, class),
        };

        match classinfo {
            Type::Tuple(tuple) => {
                let mut builder = UnionBuilder::new(db);
                for element in tuple.elements(db) {
                    builder = builder.add(self.generate_constraint(db, *element)?);
                }
                Some(builder.build())
            }
            Type::ClassLiteral(ClassLiteralType { class }) => Some(constraint_fn(class)),
            Type::SubclassOf(subclass_of_ty) => {
                subclass_of_ty.subclass_of().into_class().map(constraint_fn)
            }
            _ => None,
        }
    }
}

type NarrowingConstraints<'db> = FxHashMap<ScopedSymbolId, Type<'db>>;

fn merge_constraints_and<'db>(
    into: &mut NarrowingConstraints<'db>,
    from: NarrowingConstraints<'db>,
    db: &'db dyn Db,
) {
    for (key, value) in from {
        match into.entry(key) {
            Entry::Occupied(mut entry) => {
                *entry.get_mut() = IntersectionBuilder::new(db)
                    .add_positive(*entry.get())
                    .add_positive(value)
                    .build();
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
                *entry.get_mut() = UnionBuilder::new(db).add(*entry.get()).add(*value).build();
            }
            Entry::Vacant(entry) => {
                entry.insert(KnownClass::Object.to_instance(db));
            }
        }
    }
    for (key, value) in into.iter_mut() {
        if !from.contains_key(key) {
            *value = KnownClass::Object.to_instance(db);
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

        match pattern.kind(self.db) {
            PatternConstraintKind::Singleton(singleton, _guard) => {
                self.evaluate_match_pattern_singleton(*subject, *singleton)
            }
            PatternConstraintKind::Class(cls, _guard) => {
                self.evaluate_match_pattern_class(*subject, *cls)
            }
            // TODO: support more pattern kinds
            PatternConstraintKind::Value(..) | PatternConstraintKind::Unsupported => None,
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

        constraints.insert(
            symbol,
            if is_positive {
                Type::AlwaysFalsy.negate(self.db)
            } else {
                Type::AlwaysTruthy.negate(self.db)
            },
        );

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

        let mut last_rhs_ty: Option<Type> = None;

        for (op, (left, right)) in std::iter::zip(&**ops, comparator_tuples) {
            let lhs_ty = last_rhs_ty.unwrap_or_else(|| {
                inference.expression_type(left.scoped_expression_id(self.db, scope))
            });
            let rhs_ty = inference.expression_type(right.scoped_expression_id(self.db, scope));
            last_rhs_ty = Some(rhs_ty);

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
                                constraints.insert(symbol, ty);
                            } else {
                                // Non-singletons cannot be safely narrowed using `is not`
                            }
                        }
                        ast::CmpOp::Is => {
                            constraints.insert(symbol, rhs_ty);
                        }
                        ast::CmpOp::NotEq => {
                            if rhs_ty.is_single_valued(self.db) {
                                let ty = IntersectionBuilder::new(self.db)
                                    .add_negative(rhs_ty)
                                    .build();
                                constraints.insert(symbol, ty);
                            }
                        }
                        ast::CmpOp::Eq if lhs_ty.is_literal_string() => {
                            constraints.insert(symbol, rhs_ty);
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
                        inference.expression_type(callable.scoped_expression_id(self.db, scope));

                    if callable_ty
                        .into_class_literal()
                        .is_some_and(|c| c.class.is_known(self.db, KnownClass::Type))
                    {
                        let symbol = self
                            .symbols()
                            .symbol_id_by_name(id)
                            .expect("Should always have a symbol for every Name node");
                        constraints.insert(symbol, rhs_ty.to_instance(self.db));
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

        let callable_ty =
            inference.expression_type(expr_call.func.scoped_expression_id(self.db, scope));

        // TODO: add support for PEP 604 union types on the right hand side of `isinstance`
        // and `issubclass`, for example `isinstance(x, str | (int | float))`.
        match callable_ty {
            Type::FunctionLiteral(function_type) if expr_call.arguments.keywords.is_empty() => {
                let function = function_type
                    .known(self.db)
                    .and_then(KnownFunction::constraint_function)?;

                let [ast::Expr::Name(ast::ExprName { id, .. }), class_info] =
                    &*expr_call.arguments.args
                else {
                    return None;
                };

                let symbol = self.symbols().symbol_id_by_name(id).unwrap();

                let class_info_ty =
                    inference.expression_type(class_info.scoped_expression_id(self.db, scope));

                function
                    .generate_constraint(self.db, class_info_ty)
                    .map(|constraint| {
                        let mut constraints = NarrowingConstraints::default();
                        constraints.insert(symbol, constraint.negate_if(self.db, !is_positive));
                        constraints
                    })
            }
            // for the expression `bool(E)`, we further narrow the type based on `E`
            Type::ClassLiteral(class_type)
                if expr_call.arguments.args.len() == 1
                    && expr_call.arguments.keywords.is_empty()
                    && class_type.class.is_known(self.db, KnownClass::Bool) =>
            {
                self.evaluate_expression_node_constraint(
                    &expr_call.arguments.args[0],
                    expression,
                    is_positive,
                )
            }
            _ => None,
        }
    }

    fn evaluate_match_pattern_singleton(
        &mut self,
        subject: Expression<'db>,
        singleton: ast::Singleton,
    ) -> Option<NarrowingConstraints<'db>> {
        if let Some(ast::ExprName { id, .. }) = subject.node_ref(self.db).as_name_expr() {
            // SAFETY: we should always have a symbol for every Name node.
            let symbol = self.symbols().symbol_id_by_name(id).unwrap();

            let ty = match singleton {
                ast::Singleton::None => Type::none(self.db),
                ast::Singleton::True => Type::BooleanLiteral(true),
                ast::Singleton::False => Type::BooleanLiteral(false),
            };
            let mut constraints = NarrowingConstraints::default();
            constraints.insert(symbol, ty);
            Some(constraints)
        } else {
            None
        }
    }

    fn evaluate_match_pattern_class(
        &mut self,
        subject: Expression<'db>,
        cls: Expression<'db>,
    ) -> Option<NarrowingConstraints<'db>> {
        if let Some(ast::ExprName { id, .. }) = subject.node_ref(self.db).as_name_expr() {
            // SAFETY: we should always have a symbol for every Name node.
            let symbol = self.symbols().symbol_id_by_name(id).unwrap();
            let scope = self.scope();
            let inference = infer_expression_types(self.db, cls);
            let ty = inference
                .expression_type(cls.node_ref(self.db).scoped_expression_id(self.db, scope))
                .to_instance(self.db);
            let mut constraints = NarrowingConstraints::default();
            constraints.insert(symbol, ty);
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
                    .expression_type(expr.scoped_expression_id(self.db, scope))
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
