//! Logic for reporting boolean tests that are unintentionally always truthy or always falsy.
//! These may be reported under either `redundant-condition` or `redundant-condition-strict`.
//!
//! This module classifies tests and selects which expressions to report. [`exemptions`] handles
//! assertions, defensive branches, and environment checks; [`diagnostic`] builds messages and fixes.
//!
//! ## How we check compound conditions
//!
//! Python tests the truthiness of the whole expression after `if`, `while`, or `assert`. It also
//! tests the truthiness of subexpressions used within `and`, `or`, and `not` expressions.
//!
//! For example, `ready or flag` below is a compound condition: a condition built using a boolean
//! operator. The operands of the `or` expression are `ready` and `flag`. The redundant-condition
//! checker reports that the function object `ready` is always truthy, since the author probably
//! intended to call it:
//!
//! ```python
//! def ready() -> bool:
//!     return True
//!
//! def check(flag: bool):
//!     if ready or flag:  # `redundant-condition` on `ready`.
//!         print("ready")
//! ```
//!
//! The whole condition is also always truthy, but reporting the fixed truthiness of the outer
//! condition as well the fixed truthiness of its subexpressions would flag the same mistake
//! twice. To choose which expression to report, the redundant-condition checker therefore needs
//! to see the context of the complete condition. We therefore wait until the
//! [`TypeInferenceBuilder`] has inferred the whole condition before checking any subexpression
//! boolean tests within it. See [`TypeInferenceBuilder::check_condition_redundancy`] for how a
//! check of a smaller expression is postponed until that point.
//!
//! A boolean test can also occur inside an expression that is being passed to a function or
//! stored in a variable. For example, `not ready` below tests the truthiness of `ready` while
//! producing the argument to `consume`. The redundant-condition checker reports that test too,
//! even though there is no `if`, `while`, or `assert` statement.
//!
//! ```python
//! def ready() -> bool:
//!     return True
//!
//! def consume(flag: bool):
//!     print(flag)
//!
//! consume(not ready)  # `redundant-condition` on `ready`.
//! ```

mod diagnostic;
mod exemptions;

use ruff_python_ast::{
    self as ast,
    helpers::any_over_expr,
    visitor::{Visitor, walk_expr},
};
use ruff_text_size::Ranged;
use ty_python_core::{Truthiness, expression::ExpressionContext, predicate::StatementCall};

use crate::{
    lint::LintMetadata,
    reachability::{analyze_condition_expression, is_non_terminal_call},
    types::{
        KnownClass, KnownInstanceType, Type,
        diagnostic::{REDUNDANT_CONDITION, REDUNDANT_CONDITION_STRICT},
        infer::TypeInferenceBuilder,
    },
};

use self::exemptions::RedundantConditionContext;

/// Classification of a redundant condition.
///
/// This is used to determine which diagnostic rule would apply to the condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConditionKind {
    /// A condition whose value type is assignable to `int`, including `bool`.
    ///
    /// These tests commonly enforce runtime invariants, so they are only flagged by the
    /// opt-in `redundant-condition-strict` rule and are exempted entirely in `assert` tests.
    ///
    /// ```python
    /// def check(value: str):
    ///     if isinstance(value, str):  # Always true; flagged only by `redundant-condition-strict`.
    ///         print(value)
    ///     assert isinstance(value, str)  # Deliberate defensive check; exempted from both rules
    ///                                    # due to being in an `assert` test.
    /// ```
    Boolean,

    /// A condition whose outcome is fixed by short-circuit evaluation, despite the fact that its
    /// inferred value type does not have fixed truthiness. These conditions are only flagged by
    /// the opt-in `redundant-condition-strict` rule.
    ///
    /// ```python
    /// def check(value: object):
    ///     if value and False:  # Always false; strict rule.
    ///         print("unreachable")
    /// ```
    ShortCircuit,

    /// An always-truthy or always-falsy value test containing a walrus expression.
    ///
    /// In general, it is hard to know for sure whether an expression could have a side effect.
    /// Walrus expressions are an exception to this, however: they *always* have a side effect,
    /// and the user is probably only using a walrus expression specifically to get that side effect.
    /// Therefore, we only report a test under the opt-in `redundant-condition-strict` rule if any
    /// subexpression (even if that subexpression is in a nested scope, such as the body of a lambda
    /// expression or generator expression) is a walrus expression.
    ///
    /// ```python
    /// if value := (1, 2):  # Always truthy; only reported by `redundant-condition-strict`
    ///     print(value)
    /// ```
    ContainsWalrus,

    /// A value with fixed truthiness that does not require the strict rule.
    ///
    /// Unlike the other three categories, these tests are reported by the enabled-by-default
    /// `redundant-condition` rule.
    ///
    /// ```python
    /// def ready() -> bool:
    ///     return True
    ///
    /// if ready:  # The function object is always truthy; reported by `redundant-condition`.
    ///     print("ready")
    /// ```
    Value,
}

impl ConditionKind {
    /// Return the rule responsible for reporting this category of redundant condition.
    const fn rule(self) -> &'static LintMetadata {
        match self {
            Self::Value => &REDUNDANT_CONDITION,
            Self::Boolean | Self::ShortCircuit | Self::ContainsWalrus => {
                &REDUNDANT_CONDITION_STRICT
            }
        }
    }
}

/// An already-inferred boolean test.
///
/// The `evaluation` field records whether the expression is tested directly as a condition or
/// first evaluated as a value, as described by [`ExpressionContext`].
///
/// The truthiness of an `and`, `or`, or `not` expression is determined by its operands' truthiness,
/// so the checker visits those operands with the same evaluation context. For example, both
/// `enabled` and `value is not None` below are checked with [`ExpressionContext::Condition`],
/// like the complete `if` test:
///
/// ```python
/// def check(value: int, enabled: bool):
///     if enabled and value is not None:
///         print(value)
/// ```
///
/// The two branches of a conditional expression also inherit its evaluation context: whichever
/// branch is selected supplies the value whose truthiness is tested.
///
/// A truthiness test inside a call argument is checked separately from the truthiness of the
/// call's return value. For example:
///
/// ```python
/// def wrap(flag: bool) -> tuple[bool]:
///     return (flag,)
///
/// def check(value: int):
///     if wrap(not (value is None)):
///         print(value)
/// ```
///
/// Here, `not` tests `value is None` to produce the argument to `wrap`, so that operand is checked
/// with [`ExpressionContext::Value`]. The `if` tests the tuple returned by `wrap`, using
/// [`ExpressionContext::Condition`]. We call these "independent tests": reporting the
/// always-false comparison does not replace the diagnostic on the always-truthy tuple.
#[derive(Clone, Copy)]
struct BooleanTest<'ast, 'db> {
    expression: &'ast ast::Expr,
    value_type: Type<'db>,
    truthiness: Truthiness,
    evaluation: ExpressionContext,
}

/// A condition with known truthiness, and the rule category needed to report it.
///
/// We may or may not eventually report a diagnostic for this condition! A condition is classified
/// before we examine the context the condition occurs in, so an instance of this struct represents
/// a "diagnostic candidate" rather than a guarantee that a diagnostic will be emitted.
///
/// The value type is retained for diagnostic messages. The `is_truthy` field describes the condition's
/// outcome, which can depend on short-circuit evaluation as well as the value type (see the doc-comment
/// for [`ExpressionContext`] for more details).
#[derive(Debug)]
struct RedundantCondition<'ast, 'db> {
    expression: &'ast ast::Expr,
    value_type: Type<'db>,
    is_truthy: bool,
    kind: ConditionKind,
}

/// Determination of whether `redundant-condition-strict` diagnostics should be reported
/// on subexpressions of a compound condition, or should instead be only reported on the
/// outer expression.
///
/// `redundant-condition` diagnostics are always reported, regardless of this preference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BooleanDiagnosticPreference {
    /// No enclosing condition has fixed truthiness, so this test can be reported individually.
    CurrentCondition,

    /// A condition containing this test has fixed truthiness. `redundant-condition-strict`
    /// diagnostics should be suppressed for this subexpression.
    EnclosingCondition,
}

/// The result of checking a boolean test, indicating whether an expression containing that test
/// should also be checked for redundancy.
///
/// This is returned by [`TypeInferenceBuilder::check_boolean_test`] to prevent duplicate diagnostics
/// on an expression and its subexpressions. A condition can suppress an enclosing diagnostic even
/// when its own diagnostic is disabled, exempt, or ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConditionCheckResult {
    /// The containing boolean or conditional expression can still be reported.
    ///
    /// For example, the boolean result of `isinstance()` is not reported as a subexpression when
    /// the complete `not` expression has fixed truthiness:
    ///
    /// ```python
    /// def check(value: str):
    ///     if not isinstance(value, str):  # `redundant-condition-strict` on the whole condition.
    ///         print("unreachable")
    /// ```
    CheckEnclosingCondition,

    /// The containing boolean or conditional expression should not be reported.
    ///
    /// For example, reporting that `ready` is always truthy suppresses a second diagnostic saying
    /// that `not ready` is always false. This also applies if the diagnostic on `ready` is ignored
    /// or its rule is disabled:
    ///
    /// ```python
    /// def ready() -> bool:
    ///     return True
    ///
    /// if not ready:  # Only `ready` is reported, under `redundant-condition`.
    ///     print("unreachable")
    /// ```
    SuppressEnclosingCondition,
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Whether we should check for redundant conditions in the current inference context.
    ///
    /// We skip the checks if:
    ///
    /// - It's a third-party file
    /// - It's a stub file
    /// - We're inside a string annotation. Its expressions are absent from the semantic index,
    ///   so we cannot group nested tests to avoid duplicate diagnostics.
    /// - Neither `redundant-condition` nor `redundant-condition-strict` is enabled
    ///   in the user's configuration.
    fn should_check_redundant_conditions(&self) -> bool {
        !self.in_string_annotation()
            && self.db().should_check_file(self.file())
            && !self.file().is_stub(self.db())
            && (self.context.is_lint_enabled(&REDUNDANT_CONDITION)
                || self.context.is_lint_enabled(&REDUNDANT_CONDITION_STRICT))
    }

    /// Check a condition, for which types have already been inferred, to see if it is redundant.
    /// Report a diagnostic if so.
    ///
    /// If `test` is part of a larger expression that Python also tests for truthiness, we
    /// short-circuit without reporting anything. The call that checks the largest such
    /// expression in this scope will check the smaller tests within it. We call that largest
    /// expression the "outermost boolean test".
    ///
    /// For example, `not ready or flag` below is the outermost boolean test: it is the whole
    /// expression tested by `if`. The test of `ready` performed by `not` is a nested test because
    /// it occurs inside that expression. The whole expression is a compound condition because
    /// it combines tests using `not` and `or`:
    ///
    /// ```python
    /// def ready() -> bool:
    ///     return True
    ///
    /// def check(flag: bool):
    ///     if not ready or flag:
    ///         print("ready")
    /// ```
    ///
    /// Checking the complete condition lets us report that `ready` is an always-truthy function
    /// object, without also reporting a duplicate diagnostic regarding the fact that `not ready` is
    /// always false.
    ///
    /// Tests in a different scope, such as a lambda body, are checked when that scope is inferred.
    pub(super) fn check_condition_redundancy(&self, test: &ast::Expr, test_type: Type<'db>) {
        if !self.should_check_redundant_conditions() {
            return;
        }

        if !self.index.is_boolean_test_root(test) {
            return;
        }

        let truthiness = self.condition_truthiness(test);

        for condition in self.redundant_conditions(
            BooleanTest {
                expression: test,
                value_type: test_type,
                truthiness,
                evaluation: ExpressionContext::Condition,
            },
            RedundantConditionContext::Standalone,
        ) {
            self.report_redundant_condition(&condition);
        }
    }

    /// Evaluates an already-inferred expression as a direct condition.
    ///
    /// Unlike testing a saved expression's value, this does not re-test intermediate
    /// short-circuit results, whose truthiness may have changed.
    fn condition_truthiness(&self, test: &ast::Expr) -> Truthiness {
        let db = self.db();
        let env = self.program_environment();
        analyze_condition_expression(test, &|node| {
            self.comparison_truthiness
                .get(&node.into())
                .copied()
                .or_else(|| self.expression_type(node).bool_if_inhabited(db, env))
        })
        .unwrap_or(Truthiness::Ambiguous)
    }

    /// Check whether a `not` expression used as a value contains a redundant boolean test.
    /// Report a diagnostic if so.
    ///
    /// For example, `result = not func` will be reported if `func` is an always-truthy function object.
    ///
    /// If the `not` expression is nested inside another boolean test, we leave the diagnostic to
    /// the call that checks the complete condition. For example, the `if` condition below reports
    /// the test of `ready`, so this method does not report it separately while inferring `not ready`:
    ///
    /// ```python
    /// def ready() -> bool:
    ///     return True
    ///
    /// if not ready:  # One `redundant-condition` diagnostic on `ready`.
    ///     print("unreachable")
    /// ```
    pub(super) fn check_negation_redundancy(
        &self,
        unary: &ast::ExprUnaryOp,
        operand_type: Type<'db>,
        operand_truthiness: Truthiness,
    ) {
        if !self.should_check_redundant_conditions() {
            return;
        }

        // Avoid duplicate diagnostics if we're visiting a nested boolean test.
        if !self.index.is_boolean_test_root(&unary.operand) {
            return;
        }

        for condition in self.redundant_conditions(
            BooleanTest {
                expression: &unary.operand,
                value_type: operand_type,
                truthiness: operand_truthiness,
                evaluation: ExpressionContext::Value,
            },
            RedundantConditionContext::Standalone,
        ) {
            self.report_redundant_condition(&condition);
        }
    }

    /// Sweep over an entire suite of statements to examine if any direct `if`-statement conditions,
    /// `elif`-statement conditions or `assert`-statement conditionsin that suite are redundant.
    ///
    /// We suppress conditions in [`ConditionKind::Boolean`] and [`ConditionKind::ShortCircuit`] when
    /// the code they make unreachable is a "defensive exit". See the doc-comment for
    /// [`RedundantConditionContext::DefensiveExit`] for more details.
    ///
    /// All types in the suite must already be inferred before this method is called. This is so we
    /// can recognize terminal statements from their types, including calls returning `Never` and
    /// `return NotImplemented` statements.
    ///
    /// ## Assertions
    ///
    /// Assertions commonly check runtime invariants, so tests classified as
    /// [`ConditionKind::Boolean`] or [`ConditionKind::ShortCircuit`] are exempt. This applies to
    /// both complete assertion tests and their subexpressions:
    ///
    /// ```python
    /// from typing import Literal
    ///
    /// def check(value: str, count: Literal[1]):
    ///     assert isinstance(value, str)  # Always true, but exempt from both rules: a boolean assertion.
    ///     assert count  # Always truthy, but exempt from both rules: an integer assertion.
    /// ```
    ///
    /// Other assertions can still indicate a mistake. For example, asserting a function object
    /// rather than calling it does not check the function's return value:
    ///
    /// ```python
    /// def ready() -> bool:
    ///     return True
    ///
    /// assert ready  # `redundant-condition`: the function object is always truthy.
    /// ```
    ///
    /// Always-falsy values are also eligible for the ordinary rule: `assert value` is reported
    /// when `value: None`, although a literal `assert None` is exempt. Tests classified as
    /// [`ConditionKind::ContainsWalrus`], such as `assert (value := "foo")`, remain eligible for
    /// `redundant-condition-strict`.
    pub(super) fn check_suite_for_redundant_conditions(&self, suite: &[ast::Stmt]) {
        if !self.should_check_redundant_conditions() {
            return;
        }

        for (i, statement) in suite.iter().enumerate() {
            match statement {
                ast::Stmt::If(if_stmt) => {
                    let ast::StmtIf {
                        test,
                        body,
                        elif_else_clauses,
                        ..
                    } = if_stmt;

                    let branches = std::iter::once((test.as_ref(), body.as_slice())).chain(
                        elif_else_clauses.iter().map_while(|clause| {
                            Some((clause.test.as_ref()?, clause.body.as_slice()))
                        }),
                    );

                    for (branch_index, (test, body)) in branches.enumerate() {
                        let following_clauses = &elif_else_clauses[branch_index..];

                        let context = RedundantConditionContext::for_if_statement(
                            self,
                            body,
                            following_clauses,
                            &suite[i + 1..],
                        );

                        for condition in self.redundant_conditions(
                            self.boolean_test(test, ExpressionContext::Condition),
                            context,
                        ) {
                            if let Some(mut diagnostic) =
                                self.report_redundant_condition(&condition)
                                && condition.expression.range() == test.range()
                            {
                                self.annotate_redundant_if_or_elif(
                                    &condition,
                                    &mut diagnostic,
                                    if_stmt,
                                );
                            }
                        }
                    }
                }
                ast::Stmt::Assert(assert_statement) => {
                    let test_type = self.expression_type(&assert_statement.test);
                    let truthiness = self.condition_truthiness(&assert_statement.test);
                    for condition in self.redundant_conditions(
                        BooleanTest {
                            expression: &assert_statement.test,
                            value_type: test_type,
                            truthiness,
                            evaluation: ExpressionContext::Condition,
                        },
                        RedundantConditionContext::Assertion,
                    ) {
                        self.report_redundant_condition(&condition);
                    }
                }
                ast::Stmt::While(while_statement) => {
                    let test_type = self.expression_type(&while_statement.test);
                    let truthiness = self.condition_truthiness(&while_statement.test);
                    for condition in self.redundant_conditions(
                        BooleanTest {
                            expression: &while_statement.test,
                            value_type: test_type,
                            truthiness,
                            evaluation: ExpressionContext::Condition,
                        },
                        RedundantConditionContext::Standalone,
                    ) {
                        self.report_redundant_condition(&condition);
                    }
                }
                _ => continue,
            }
        }
    }

    /// Classify a condition using its inferred type and truthiness.
    ///
    /// We return `None` if we cannot determine whether the test is always truthy or always falsy.
    /// We also return `None` for literal boolean and integer expressions: a condition written as
    /// `True`, `False`, `1`, or `0` is almost certainly deliberate. A variable with an inferred
    /// literal type is still classified, however, because its truthiness may be less obvious to
    /// the author:
    ///
    /// ```python
    /// from typing import Literal
    ///
    /// def check(flag: bool, known: Literal[True]):
    ///     if flag:   # No classification: the outcome is unknown.
    ///         pass
    ///     if True:   # No classification: a literal boolean expression.
    ///         pass
    ///     if 1:      # No classification: a literal integer expression.
    ///         pass
    ///     if known:  # `ConditionKind::Boolean`: the value type is boolean.
    ///         pass
    /// ```
    fn classify_redundant_condition<'expr>(
        &self,
        test: BooleanTest<'expr, 'db>,
    ) -> Option<RedundantCondition<'expr, 'db>> {
        let BooleanTest {
            expression,
            value_type,
            truthiness,
            ..
        } = test;
        if truthiness.is_ambiguous() {
            return None;
        }

        if matches!(
            expression,
            ast::Expr::BooleanLiteral(_)
                | ast::Expr::NoneLiteral(_)
                | ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: ast::Number::Int(_),
                    ..
                })
        ) {
            // Literal flags such as `if False` and `while 1` are almost certainly deliberate.
            // `assert None` also comes up a weird amount in certain ecosystem projects, too!
            return None;
        }

        let db = self.db();
        let env = self.program_environment();

        let kind = if value_type.is_assignable_to(db, env, KnownClass::Int.to_instance(db, env)) {
            ConditionKind::Boolean
        } else if value_type.bool(db, env).is_ambiguous() {
            ConditionKind::ShortCircuit
        } else if any_over_expr(expression, ast::Expr::is_named_expr) {
            // Include deferred bodies: a surrounding call may execute a lambda or generator.
            ConditionKind::ContainsWalrus
        } else {
            ConditionKind::Value
        };

        Some(RedundantCondition {
            expression,
            value_type,
            is_truthy: truthiness.is_always_true(),
            kind,
        })
    }

    /// Read an already-inferred expression's type and truthiness using the requested
    /// [`ExpressionContext`]. Return a [`BooleanTest`] instance.
    ///
    /// Use short-circuit evaluation when the expression is tested directly as a condition.
    /// Otherwise, determine truthiness from its inferred value type.
    ///
    /// See the [`ExpressionContext`] doc-comment for more details.
    fn boolean_test<'expr>(
        &self,
        expression: &'expr ast::Expr,
        evaluation: ExpressionContext,
    ) -> BooleanTest<'expr, 'db> {
        let value_type = self.expression_type(expression);
        let truthiness = match evaluation {
            ExpressionContext::Condition => self.condition_truthiness(expression),
            ExpressionContext::Value => value_type.bool(self.db(), self.program_environment()),
        };
        BooleanTest {
            expression,
            value_type,
            truthiness,
            evaluation,
        }
    }

    /// Check `test` and its operands for redundancy, returning the conditions to report.
    ///
    /// The operands of `and`, `or`, and `not`, and the branches of conditional expressions,
    /// are examined before deciding whether to report the expression containing them.
    /// This ensures that the same mistake is not reported twice.
    ///
    /// Callers construct diagnostics for the selected conditions using the [`diagnostic`] module,
    /// and can add annotations using the surrounding statement context.
    ///
    /// Independent tests within subexpressions are included in the same result.
    ///
    /// If the truthiness of the complete condition is fixed, we only report a single diagnostic
    /// for the entire condition. If its truthiness is ambiguous, however, subexpressions with
    /// fixed truthiness can be reported individually:
    ///
    /// ```python
    /// def check(value: int, flag: bool):
    ///     if flag and value is not None:                    # Report `value is not None`.
    ///         print(value)
    ///     if isinstance(value, int) and value is not None:  # Report the complete condition.
    ///         print(value)
    /// ```
    #[must_use]
    fn redundant_conditions<'ast>(
        &self,
        test: BooleanTest<'ast, 'db>,
        condition_context: RedundantConditionContext,
    ) -> Vec<RedundantCondition<'ast, 'db>> {
        let mut conditions = Vec::new();
        self.check_boolean_test(
            test,
            condition_context,
            BooleanDiagnosticPreference::CurrentCondition,
            &mut conditions,
        );
        conditions
    }

    /// Check `test` and the boolean tests contributing to its outcome.
    ///
    /// Diagnostic selection follows two rules:
    ///
    /// - Within a condition with fixed truthiness, boolean and short-circuit operands are not
    ///   reported. The enclosing condition represents their redundancy, even if its diagnostic
    ///   is exempt or ignored.
    /// - Other operands, such as uncalled functions, take precedence over their enclosing
    ///   conditions because they identify the likely mistake more directly.
    ///
    /// The first rule is decided before visiting operands, since the whole condition's truthiness
    /// is already known. The second is decided after visiting them, using [`ConditionCheckResult`].
    ///
    /// Return [`ConditionCheckResult::SuppressEnclosingCondition`] if a caller checking a boolean
    /// or conditional expression containing `test` should suppress the diagnostic on that larger
    /// expression. For example, the code below receives a diagnostic on `ready` because the
    /// function object is always truthy. It does not also receive a diagnostic saying that
    /// `not ready` is always false:
    ///
    /// ```python
    /// def ready() -> bool:
    ///     return True
    ///
    /// if not ready:  # `redundant-condition` on `ready`.
    ///     print("unreachable")
    /// ```
    ///
    /// Checking `ready` returns [`ConditionCheckResult::SuppressEnclosingCondition`], so the call
    /// checking `not ready` knows not to report a diagnostic regarding the larger expression.
    ///
    /// Return the same result if the selected diagnostic is disabled or ignored. Disabling the
    /// diagnostic on `ready` should not cause a different diagnostic on `not ready` to appear.
    /// Conditions selected for reporting are appended to `conditions` in traversal order.
    fn check_boolean_test<'ast>(
        &self,
        test: BooleanTest<'ast, 'db>,
        condition_context: RedundantConditionContext,
        preference: BooleanDiagnosticPreference,
        conditions: &mut Vec<RedundantCondition<'ast, 'db>>,
    ) -> ConditionCheckResult {
        let operand_preference = if test.truthiness.is_ambiguous() {
            preference
        } else {
            BooleanDiagnosticPreference::EnclosingCondition
        };

        let mut operand_result = ConditionCheckResult::CheckEnclosingCondition;

        let mut check_operand = |expression: &'ast ast::Expr, context, conditions: &mut Vec<_>| {
            if self.check_boolean_test(
                self.boolean_test(expression, test.evaluation),
                context,
                operand_preference,
                conditions,
            ) == ConditionCheckResult::SuppressEnclosingCondition
            {
                operand_result = ConditionCheckResult::SuppressEnclosingCondition;
            }
        };

        match test.expression {
            ast::Expr::BoolOp(ast::ExprBoolOp { values, .. }) => {
                // Include the final operand: `if flag and func` tests `func` when `flag` is true.
                for value in values {
                    check_operand(value, condition_context, conditions);
                }
            }

            ast::Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::Not,
                operand,
                ..
            }) => {
                check_operand(operand, condition_context.negated(), conditions);
            }

            ast::Expr::If(ast::ExprIf {
                test, body, orelse, ..
            }) => {
                // This test chooses a branch independently of the enclosing truthiness check.
                // The selected branch supplies the value tested by the enclosing condition.
                self.check_boolean_test(
                    self.boolean_test(test, ExpressionContext::Condition),
                    condition_context.nested_test(),
                    BooleanDiagnosticPreference::CurrentCondition,
                    conditions,
                );

                for branch in [body, orelse] {
                    check_operand(branch, condition_context, conditions);
                }
            }

            _ => self.check_nested_conditions(
                test.expression,
                condition_context.nested_test(),
                conditions,
            ),
        }

        if operand_result == ConditionCheckResult::SuppressEnclosingCondition {
            return operand_result;
        }

        if matches!(
            test.value_type,
            Type::KnownInstance(KnownInstanceType::ConstraintSet(_))
        ) {
            // Boolean tests against `ty_extensions._internal.ConstraintSet` are (hopefully)
            // only going to occur in the context of our test suite, and it's very annoying
            // if we report them as redundant.
            return ConditionCheckResult::SuppressEnclosingCondition;
        }

        let Some(condition) = self.classify_redundant_condition(test) else {
            return ConditionCheckResult::CheckEnclosingCondition;
        };

        if preference == BooleanDiagnosticPreference::EnclosingCondition
            && matches!(
                &condition.kind,
                ConditionKind::Boolean | ConditionKind::ShortCircuit
            )
        {
            return ConditionCheckResult::CheckEnclosingCondition;
        }

        let rule = condition.kind.rule();

        if self.context.is_lint_enabled(rule) && !condition_context.exempts(self, &condition) {
            conditions.push(condition);
        }

        ConditionCheckResult::SuppressEnclosingCondition
    }

    /// Find and check boolean tests within subexpressions of `test`. This includes tests inside
    /// call arguments and collection elements.
    ///
    /// A `not` expression or conditional expression can test truthiness while producing a value
    /// that is passed to a function. That test is checked separately from any test of the call's
    /// return value.
    ///
    /// For example, the following code has two mistakes: `not ready` tests an always-truthy
    /// function object, and the `if` tests a call that always returns a nonempty tuple.
    /// Two diagnostics are therefore reported on the code: one for `ready`, and one for
    /// `consume(not ready)`:
    ///
    /// ```python
    /// def ready() -> bool:
    ///     return True
    ///
    /// def consume(flag: bool) -> tuple[int]:
    ///     return (1,)
    ///
    /// if consume(not ready):
    ///     print("ready")
    /// ```
    ///
    /// We use the [`SubexpressionChecker`] visitor to find subexpression tests in the current
    /// scope. Reporting them does not suppress a diagnostic on `test` itself, since that
    /// would hide the second mistake in the above example.
    fn check_nested_conditions<'ast>(
        &self,
        test: &'ast ast::Expr,
        condition_context: RedundantConditionContext,
        conditions: &mut Vec<RedundantCondition<'ast, 'db>>,
    ) {
        let mut visitor = SubexpressionChecker {
            builder: self,
            condition_context,
            conditions,
        };
        visitor.visit_expr(test);
    }
}

/// An AST visitor for boolean tests that exist as subexpressions of an outer expression being
/// checked by [`TypeInferenceBuilder::check_boolean_test`].
///
/// The visitor passes each `not` operand and each conditional expression's `if` test to the
/// redundant-condition checker. These tests can occur in call arguments or collection elements, as
/// illustrated by [`TypeInferenceBuilder::check_nested_conditions`]. They are checked separately
/// from the truthiness of the containing call or collection.
///
/// The visitor stays within the current inference scope. Lambda bodies and comprehension scopes
/// are checked in a separate pass of the [`TypeInferenceBuilder`].
struct SubexpressionChecker<'a, 'ast, 'db> {
    builder: &'a TypeInferenceBuilder<'db, 'a>,
    condition_context: RedundantConditionContext,
    conditions: &'a mut Vec<RedundantCondition<'ast, 'db>>,
}

impl<'ast> Visitor<'ast> for SubexpressionChecker<'_, 'ast, '_> {
    fn visit_expr(&mut self, expression: &'ast ast::Expr) {
        let builder = self.builder;

        // Lambda bodies and comprehension scopes have their own inference and test owners.
        if builder.index.try_expression_scope_id(expression)
            != Some(builder.scope().file_scope_id(builder.db()))
        {
            return;
        }

        if builder.try_expression_type(expression).is_none() {
            return;
        }

        match expression {
            ast::Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::Not,
                operand,
                ..
            }) => {
                builder.check_boolean_test(
                    builder.boolean_test(operand, ExpressionContext::Value),
                    self.condition_context,
                    BooleanDiagnosticPreference::CurrentCondition,
                    self.conditions,
                );
            }

            ast::Expr::If(ast::ExprIf {
                test, body, orelse, ..
            }) => {
                builder.check_boolean_test(
                    builder.boolean_test(test, ExpressionContext::Condition),
                    self.condition_context,
                    BooleanDiagnosticPreference::CurrentCondition,
                    self.conditions,
                );
                self.visit_expr(body);
                self.visit_expr(orelse);
            }

            _ => walk_expr(self, expression),
        }
    }
}

/// Which exits are accepted when checking the final statement of a suite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SuiteExitKind {
    /// Accept any return statement, as well as defensive exits.
    Any,
    /// Only accept exits that can indicate a defensive runtime check.
    Defensive,
}

/// Return `true` if `suite` ends in an exit recognized by the redundant-condition heuristic.
///
/// Both kinds of exit accept the following final statements:
/// - a `raise` statement
/// - a potentially failing assertion
/// - a call returning `Never`, including an awaited call
/// - or a nested `if` statement with an explicit `else` where every branch of the
///   `if`/`elif`/`else` ends in a recognized exit of the requested kind.
///
/// [`SuiteExitKind::Any`] also accepts every `return` statement, while
/// [`SuiteExitKind::Defensive`] only accepts `return NotImplemented`.
///
/// Potentially failing assertions count as exits even when they might succeed. This heuristic
/// prioritises avoiding false positives on intentional runtime checks.
fn suite_ends_with_exit(
    builder: &TypeInferenceBuilder<'_, '_>,
    suite: &[ast::Stmt],
    kind: SuiteExitKind,
) -> bool {
    let db = builder.db();
    let env = builder.program_environment();

    suite
        .iter()
        .rev()
        .find(|stmt| !is_trivial_statement(stmt))
        .is_some_and(|stmt| match stmt {
            ast::Stmt::Raise(_) => true,
            ast::Stmt::Assert(ast::StmtAssert { test, .. }) => {
                builder.condition_truthiness(test).may_be_false()
            }
            ast::Stmt::Expr(ast::StmtExpr { value, .. })
                if let Some(StatementCall { call, is_await }) =
                    StatementCall::from_expression(value) =>
            {
                let callable_type = builder.expression_type(&call.func);
                // In a statically unreachable branch, even an ordinary callable can have type
                // `Never`. Preserve the exemption in that case to avoid false positives on
                // defensive checks made unreachable by type annotations.
                callable_type.is_never()
                    || is_non_terminal_call(db, env, callable_type, is_await, || {
                        builder.expression_type(value)
                    })
                    .is_always_false()
            }
            ast::Stmt::Return(ast::StmtReturn { value, .. }) => match kind {
                SuiteExitKind::Any => true,
                SuiteExitKind::Defensive => value.as_ref().is_some_and(|expr| {
                    // Known limitation: `Any`, `Unknown`, and `Never` are also assignable to
                    // `NotImplementedType`, so an ordinary return *can* suppress a diagnostic here.
                    // We prioritise minimising false positives over minimising false negatives
                    // when recognizing potentially deliberate defensive checks.
                    builder.expression_type(expr).is_assignable_to(
                        db,
                        env,
                        KnownClass::NotImplementedType.to_instance(db, env),
                    )
                }),
            },
            ast::Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                elif_else_clauses
                    .last()
                    .is_some_and(|last_clause| last_clause.test.is_none())
                    && suite_ends_with_exit(builder, body, kind)
                    && elif_else_clauses
                        .iter()
                        .all(|clause| suite_ends_with_exit(builder, &clause.body, kind))
            }
            _ => false,
        })
}

/// Return `true` if `stmt` is a "trivial statement"
/// that has no effect nor side effect at runtime.
fn is_trivial_statement(stmt: &ast::Stmt) -> bool {
    match stmt {
        ast::Stmt::Pass(_) => true,
        ast::Stmt::Expr(ast::StmtExpr { value, .. }) => matches!(
            &**value,
            ast::Expr::StringLiteral(_) | ast::Expr::EllipsisLiteral(_)
        ),
        _ => false,
    }
}
