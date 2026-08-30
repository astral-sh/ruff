//! Logic for reporting boolean tests that are unintentionally always truthy or always falsy.
//! These may be reported under either `redundant-condition` or `redundant-condition-strict`.
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
use ty_python_core::Truthiness;

use crate::{
    lint::LintMetadata,
    types::{
        KnownClass, KnownInstanceType, Type,
        diagnostic::{REDUNDANT_CONDITION, REDUNDANT_CONDITION_STRICT},
        infer::TypeInferenceBuilder,
    },
};

use self::exemptions::{SuiteExitKind, is_special_cased_condition_expression};

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

/// A condition with known truthiness, and the rule category needed to report it.
///
/// We may or may not eventually report a diagnostic for this condition! A condition is classified
/// before we examine the context the condition occurs in, so an instance of this struct represents
/// a "diagnostic candidate" rather than a guarantee that a diagnostic will be emitted.
///
/// The value type is retained for diagnostic messages. The `is_truthy` field describes the condition's
/// outcome, which can depend on short-circuit evaluation as well as the value type (see the doc-comment
/// for [`ExpressionEvaluationStrategy`] for more details).
#[derive(Clone, Copy)]
struct RedundantCondition<'ast, 'db> {
    expression: &'ast ast::Expr,
    value_type: Type<'db>,
    is_truthy: bool,
    kind: ConditionKind,
}

/// The strategy ty should use to evaluate the truthiness of an expression.
///
/// In some situations, ty can know that a condition will always be true, or it can know that a
/// condition will always be false, even when this is not guaranteed by the inferred type of that
/// condition. This is because of the way that Python short-circuits evaluation of conditions in the
/// context of `if` tests, `while` tests and `assert` statements.
///
/// Consider a class whose comparison method has an `object` return type:
///
/// ```py
/// from typing_extensions import reveal_type
///
///
/// class Comparable:
///     def __lt__(self, other: int) -> object: ...
///
///
/// def check(value: Comparable):
///     reveal_type(value < 1 < 0)  # revealed: ~AlwaysTruthy
///
///     if value < 1 < 0:  # error: [redundant-condition-strict] "always false"
///         pass
/// ```
///
/// Outside the context of an `if` test, the revealed type of the condition here is `~AlwaysTruthy`:
/// in other words, ty knows that this expression is not *always true*, but cannot guarantee that it is
/// definitely *always false*. It could be an object that is sometimes true and sometimes false -- for
/// example, a `list` (which is falsy when it is empty, and truthy otherwise).
///
/// Nonetheless, when `value < 1 < 0` is used directly as a condition, ty knows that the condition will
/// always be falsy and the `if` branch will never be taken. Python tests the truthiness of the object
/// returned by `Comparable.__lt__` once: if it is falsy, the condition fails immediately. If it is
/// truthy, Python evaluates `1 < 0`, which is false. There is no second truthiness test of the object
/// returned by `__lt__`.
///
/// If the chained comparison is saved as a variable first, its value can be the object returned by
/// `__lt__`, if that object was falsy when first tested. The `if result` statement then tests that
/// object's truthiness again. A user-defined `__bool__` method can return a different result on that
/// second call, so ty cannot guarantee that the saved value is still falsy, and no diagnostic is
/// emitted:
///
/// ```py
/// def check_saved(value: Comparable):
///     result = value < 1 < 0
///     if result:  # no diagnostic
///         pass
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpressionEvaluationStrategy {
    /// ty can predict the truthy or falsy branch directly from the expression.
    Condition,

    /// ty must evaluate the expression and test the truthiness of the resulting type.
    Value,
}

/// The context in which a boolean test occurs.
///
/// This is used to help determine whether a test should be exempt from one or both
/// redundant-condition rules. For example, the same always-true comparison can be reported in
/// an `if` condition but exempt in an assertion.
///
/// [`ConditionKind`] determines the rule that will be applied if the condition is not exempted.
/// This context determines whether the test serves a purpose that makes reporting it undesirable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RedundantConditionContext {
    /// A boolean test checked without the additional exemptions represented by the other variants.
    ///
    /// This includes ordinary `if` conditions. For example:
    ///
    /// ```python
    /// def check(value: str):
    ///     if isinstance(value, str):  # Always true; flagged by `redundant-condition-strict`.
    ///         print(value)
    /// ```
    ///
    /// The special cases for assertions, tests within larger expressions, and checks that reject
    /// unexpected input are described by [`Self::Assertion`], [`Self::CompoundOperand`], and
    /// [`Self::DefensiveExit`].
    Standalone,

    /// A boolean test that occurs inside another boolean test, such as an operand of `and`,
    /// `or`, or `not` expression, or a test inside an assertion's expression.
    ///
    /// We do not report the individual sub-expression tests if they belong to
    /// [`ConditionKind::Boolean`] or [`ConditionKind::ShortCircuit`]. Instead, we check whether
    /// the whole condition should be reported. For example, each `isinstance()` call below is
    /// always true, but we only report the complete `and` expression:
    ///
    /// ```python
    /// def check(value: str):
    ///     # One `redundant-condition-strict` diagnostic on the complete condition;
    ///     # no diagnostics on the sub-expressions.
    ///     if isinstance(value, str) and isinstance(value, str):
    ///         print(value)
    /// ```
    ///
    /// Tests in the other categories can still be reported individually, as with an uncalled
    /// function in `if not ready`.
    CompoundOperand,

    /// The complete test of an assertion, where boolean and integer checks are exempt.
    ///
    /// ```python
    /// def check(value: str):
    ///     assert isinstance(value, str)  # Deliberately defensive runtime check;
    ///                                    # exempt from both rules.
    /// ```
    Assertion,

    /// An `if` or `elif` test that guards code intended to reject unexpected input or an
    /// unsupported operation, even though the inferred types rule that situation out.
    ///
    /// We call that rejection a "defensive exit". For example, a function might raise `TypeError`
    /// if its argument has the wrong type. Type annotations do not enforce this at runtime, so
    /// the check can still be useful when the function is called from untyped code. We therefore
    /// exempt conditions in [`ConditionKind::Boolean`] and [`ConditionKind::ShortCircuit`] from
    /// being flagged if the condition is identified as being in this context.
    ///
    /// ```python
    /// def check(value: int):
    ///     # Always false according to the annotation,
    ///     # but exempted from diagnostics due to the defensive exit in the branch body:
    ///     if not isinstance(value, int):  
    ///         raise TypeError("expected an integer")
    /// ```
    ///
    /// The code that rejects the input can also be in an `else` branch:
    ///
    /// ```python
    /// def check(value: int):
    ///     # Always true according to the annotation,
    ///     # but exempted from diagnostics due to the defensive exit in the `else`-branch body:
    ///     if isinstance(value, int):  
    ///         ...
    ///     else:
    ///         raise TypeError("expected an integer")
    /// ```
    ///
    /// Or after an always-true final `if` or `elif` whose body ends in a recognized exit:
    ///
    /// ```python
    /// def check(value: int):
    ///     # Always true according to the annotation,
    ///     # but exempted from diagnostics due to the defensive exit in the body
    ///     # of the "implicit `else`" after the final `if`:
    ///     if isinstance(value, int):  # Always true according to the annotation; exempt.
    ///         return value
    ///     raise TypeError("expected an integer")
    /// ```
    ///
    /// [`TypeInferenceBuilder::suite_ends_with_exit`] describes the forms of rejection we recognize
    /// with [`SuiteExitKind::Defensive`].
    ///
    /// This context only exempts the complete `if` or `elif` test. Mistakes within the test,
    /// such as testing the truthiness of an uncalled function, can still be reported.
    DefensiveExit,
}

impl RedundantConditionContext {
    /// Return `true` if this context exempts a condition of the given category from being
    /// reported by the applicable rule.
    fn exempts(self, condition_kind: ConditionKind) -> bool {
        match self {
            Self::Assertion => condition_kind == ConditionKind::Boolean,
            Self::CompoundOperand | Self::DefensiveExit => {
                matches!(
                    condition_kind,
                    ConditionKind::Boolean | ConditionKind::ShortCircuit
                )
            }
            Self::Standalone => false,
        }
    }

    /// Return the context for a boolean test inside another expression, such as a `not`
    /// expression passed as a call argument.
    ///
    /// An [`Self::Assertion`] or [`Self::CompoundOperand`] context also exempts boolean and integer
    /// tests within call arguments. For example, the assertion in `assert consume(not flag)`
    /// exempts the test of `flag` if it has a boolean or integer type.
    ///
    /// The exemption for a [defensive exit](Self::DefensiveExit), such as raising on an invalid
    /// argument, applies only to the complete `if` or `elif` condition. Tests within its call
    /// arguments are checked in the [`Self::Standalone`] context instead.
    const fn nested_test(self) -> Self {
        match self {
            // Assertions also exempt boolean tests embedded in calls or other value expressions.
            Self::Assertion | Self::CompoundOperand => Self::CompoundOperand,
            Self::Standalone | Self::DefensiveExit => Self::Standalone,
        }
    }
}

/// The result of checking a boolean test, indicating whether an expression containing that test
/// should also be checked for redundancy.
///
/// This is returned by [`RedundantConditionChecker::check`] to prevent duplicate diagnostics on
/// an expression and its subexpressions. It does not indicate whether a diagnostic was emitted:
/// a diagnostic that is disabled or ignored can still suppress a diagnostic on a larger expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConditionCheckResult {
    /// The containing boolean or conditional expression can still be reported.
    ///
    /// For example, the boolean result of `isinstance()` is exempt when checked as a subexpression,
    /// so we can report the complete `not` expression instead:
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
    /// Create a redundant-condition checker.
    ///
    /// Returns `None` if we shouldn't check this file for redundant conditions.
    /// This can be because:
    ///
    /// - It's a third-party file
    /// - It's a stub file
    /// - Neither `redundant-condition` nor `redundant-condition-strict` is enabled
    ///   in the user's configuration.
    fn redundant_condition_checker(&self) -> Option<RedundantConditionChecker<'_, 'db>> {
        if !self.db().should_check_file(self.file()) {
            return None;
        }

        if self.file().is_stub(self.db()) {
            return None;
        }

        if !(self.context.is_lint_enabled(&REDUNDANT_CONDITION)
            || self.context.is_lint_enabled(&REDUNDANT_CONDITION_STRICT))
        {
            return None;
        }

        Some(RedundantConditionChecker { builder: self })
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
    pub(super) fn check_condition_redundancy(
        &self,
        test: &ast::Expr,
        test_type: Type<'db>,
        truthiness: Truthiness,
    ) {
        let Some(checker) = self.redundant_condition_checker() else {
            return;
        };

        if !self
            .index
            .enclosing_boolean_test(self.scope().file_scope_id(self.db()), test.range())
            .is_none_or(|outer| outer == test.range())
        {
            return;
        }

        checker.check(
            test,
            test_type,
            truthiness,
            ExpressionEvaluationStrategy::Condition,
            RedundantConditionContext::Standalone,
        );
    }

    /// Check an `assert` statement for redundant boolean tests.
    ///
    /// Assertions of boolean and integer values are exempt from both redundant-condition rules.
    /// They commonly check runtime invariants, so we assume they are deliberate even when the
    /// inferred types determine their outcome:
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
    pub(super) fn check_assertion_redundancy(
        &self,
        test: &ast::Expr,
        test_type: Type<'db>,
        truthiness: Truthiness,
    ) {
        if let Some(checker) = self.redundant_condition_checker() {
            checker.check(
                test,
                test_type,
                truthiness,
                ExpressionEvaluationStrategy::Condition,
                RedundantConditionContext::Assertion,
            );
        }
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
        // The operand, rather than `not` itself, is recorded as a boolean test. An enclosing
        // test containing the whole `not` expression will visit it with the correct evaluation.
        let Some(checker) = self.redundant_condition_checker() else {
            return;
        };

        if self
            .index
            .enclosing_boolean_test(self.scope().file_scope_id(self.db()), unary.range())
            .is_some()
        {
            return;
        }

        checker.check(
            &unary.operand,
            operand_type,
            operand_truthiness,
            ExpressionEvaluationStrategy::Value,
            RedundantConditionContext::Standalone,
        );
    }

    /// Sweep over an entire suite of statements to examine if any direct `if`-statement conditions
    /// or `elif`-statement conditions in that suite are redundant.
    ///
    /// We suppress conditions in [`ConditionKind::Boolean`] and [`ConditionKind::ShortCircuit`] when
    /// the code they make unreachable is a "defensive exit". See the doc-comment for
    /// [`RedundantConditionContext::DefensiveExit`] for more details.
    ///
    /// All types in the suite must already be inferred before this method is called. This is so we
    /// can recognize terminal statements from their types, including calls returning `Never` and
    /// `return NotImplemented` statements.
    pub(super) fn check_suite_for_redundant_if_statements(&self, suite: &[ast::Stmt]) {
        let Some(checker) = self.redundant_condition_checker() else {
            return;
        };

        for (i, statement) in suite.iter().enumerate() {
            let ast::Stmt::If(ast::StmtIf {
                test,
                body,
                elif_else_clauses,
                ..
            }) = statement
            else {
                continue;
            };

            let branches = std::iter::once((test.as_ref(), body.as_slice())).chain(
                elif_else_clauses
                    .iter()
                    .map_while(|clause| Some((clause.test.as_ref()?, clause.body.as_slice()))),
            );

            for (branch_index, (test, body)) in branches.enumerate() {
                let test_type = self.expression_type(test);
                let truthiness = self.condition_truthiness(test);
                let following_clauses = &elif_else_clauses[branch_index..];

                let unreachable_suite = match truthiness {
                    Truthiness::AlwaysFalse => Some(body),
                    Truthiness::AlwaysTrue => match following_clauses {
                        [else_clause] if else_clause.test.is_none() => {
                            Some(else_clause.body.as_slice())
                        }
                        [] if self.suite_ends_with_exit(body, SuiteExitKind::Any) => {
                            Some(&suite[i + 1..])
                        }
                        _ => None,
                    },
                    Truthiness::Ambiguous => None,
                };

                let position = if let Some(unreachable_suite) = unreachable_suite
                    && self.context.is_lint_enabled(&REDUNDANT_CONDITION_STRICT)
                    && matches!(
                        checker
                            .classify_condition(test, test_type, truthiness)
                            .map(|condition| condition.kind),
                        Some(ConditionKind::Boolean | ConditionKind::ShortCircuit)
                    )
                    && self.suite_ends_with_exit(unreachable_suite, SuiteExitKind::Defensive)
                {
                    RedundantConditionContext::DefensiveExit
                } else {
                    RedundantConditionContext::Standalone
                };

                checker.check(
                    test,
                    test_type,
                    truthiness,
                    ExpressionEvaluationStrategy::Condition,
                    position,
                );
            }
        }
    }
}

/// A redundant-condition checker that borrows the type-inference state for the current scope.
///
/// The [`TypeInferenceBuilder`] stores the types and truthiness inferred for expressions in that
/// scope. The redundant-condition checker reads those results to decide which boolean tests to
/// report.
///
/// The redundant-condition checker examines the operands of `and`, `or`, and `not`, and the branches
/// of conditional expressions, before deciding whether to report the expression containing them.
/// This ensures that the same mistake is not reported twice.
///
/// The [`diagnostic`] module constructs diagnostic messages and fixes.
struct RedundantConditionChecker<'a, 'db> {
    builder: &'a TypeInferenceBuilder<'db, 'a>,
}

impl<'db> RedundantConditionChecker<'_, 'db> {
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
    fn classify_condition<'expr>(
        &self,
        expression: &'expr ast::Expr,
        value_type: Type<'db>,
        truthiness: Truthiness,
    ) -> Option<RedundantCondition<'expr, 'db>> {
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

        let db = self.builder.db();
        let env = self.builder.program_environment();

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

    /// Determine an already-inferred expression's truthiness using the requested
    /// [`ExpressionEvaluationStrategy`].
    ///
    /// Use short-circuit evaluation when the expression is tested directly as a condition.
    /// Otherwise, determine truthiness from its inferred value type, as described by that enum.
    fn expression_truthiness(
        &self,
        expression: &ast::Expr,
        evaluation: ExpressionEvaluationStrategy,
    ) -> Truthiness {
        match evaluation {
            ExpressionEvaluationStrategy::Condition => {
                self.builder.condition_truthiness(expression)
            }
            ExpressionEvaluationStrategy::Value => self
                .builder
                .expression_type(expression)
                .bool(self.builder.db(), self.builder.program_environment()),
        }
    }

    /// Check `test` and its operands for redundancy, and report diagnostics where appropriate.
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
    /// Boolean tests exempted by [`RedundantConditionContext::CompoundOperand`] return
    /// [`ConditionCheckResult::CheckEnclosingCondition`], allowing the complete condition to be
    /// reported instead.
    fn check(
        &self,
        test: &ast::Expr,
        value_type: Type<'db>,
        truthiness: Truthiness,
        evaluation_strategy: ExpressionEvaluationStrategy,
        condition_context: RedundantConditionContext,
    ) -> ConditionCheckResult {
        let mut operand_result = ConditionCheckResult::CheckEnclosingCondition;

        match test {
            ast::Expr::BoolOp(ast::ExprBoolOp { values, .. }) => {
                // Include the final operand: `if flag and func` tests `func` when `flag` is true.
                for value in values {
                    if self.check(
                        value,
                        self.builder.expression_type(value),
                        self.expression_truthiness(value, evaluation_strategy),
                        evaluation_strategy,
                        RedundantConditionContext::CompoundOperand,
                    ) == ConditionCheckResult::SuppressEnclosingCondition
                    {
                        operand_result = ConditionCheckResult::SuppressEnclosingCondition;
                    }
                }
            }

            ast::Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::Not,
                operand,
                ..
            }) => {
                operand_result = self.check(
                    operand,
                    self.builder.expression_type(operand),
                    self.expression_truthiness(operand, evaluation_strategy),
                    evaluation_strategy,
                    RedundantConditionContext::CompoundOperand,
                );
            }

            ast::Expr::If(ast::ExprIf {
                test, body, orelse, ..
            }) => {
                // This test chooses a branch independently of the enclosing truthiness check.
                // The selected branch supplies the value tested by the enclosing condition.
                self.check(
                    test,
                    self.builder.expression_type(test),
                    self.builder.condition_truthiness(test),
                    ExpressionEvaluationStrategy::Condition,
                    condition_context.nested_test(),
                );

                for branch in [body, orelse] {
                    if self.check(
                        branch,
                        self.builder.expression_type(branch),
                        self.expression_truthiness(branch, evaluation_strategy),
                        evaluation_strategy,
                        RedundantConditionContext::CompoundOperand,
                    ) == ConditionCheckResult::SuppressEnclosingCondition
                    {
                        operand_result = ConditionCheckResult::SuppressEnclosingCondition;
                    }
                }
            }

            _ => self.check_nested_conditions(test, condition_context.nested_test()),
        }

        if operand_result == ConditionCheckResult::SuppressEnclosingCondition {
            return operand_result;
        }

        if matches!(
            value_type,
            Type::KnownInstance(KnownInstanceType::ConstraintSet(_))
        ) {
            // Boolean tests against `ty_extensions._internal.ConstraintSet` are (hopefully)
            // only going to occur in the context of our test suite, and it's very annoying
            // if we report them as redundant.
            return ConditionCheckResult::SuppressEnclosingCondition;
        }

        let Some(condition) = self.classify_condition(test, value_type, truthiness) else {
            return ConditionCheckResult::CheckEnclosingCondition;
        };

        if condition_context.exempts(condition.kind) {
            return ConditionCheckResult::CheckEnclosingCondition;
        }

        let rule = condition.kind.rule();

        if self.builder.context.is_lint_enabled(rule)
            && !any_over_expr(test, |expression| {
                is_special_cased_condition_expression(
                    self.builder.db(),
                    self.builder.program_file(),
                    expression,
                    |expr| self.builder.expression_type(expr),
                )
            })
        {
            self.builder.report_redundant_condition(condition);
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
    fn check_nested_conditions(
        &self,
        test: &ast::Expr,
        condition_context: RedundantConditionContext,
    ) {
        let mut visitor = SubexpressionChecker {
            checker: self,
            condition_context,
        };
        visitor.visit_expr(test);
    }
}

/// An AST visitor for boolean tests that exist as subexpressions of an outer expression being
/// checked by a [`RedundantConditionChecker`].
///
/// The visitor passes each `not` operand and each conditional expression's `if` test to the
/// redundant-condition checker. These tests can occur in call arguments or collection elements, as
/// illustrated by [`RedundantConditionChecker::check_nested_conditions`]. They are checked separately
/// from the truthiness of the containing call or collection.
///
/// The visitor stays within the current inference scope. Lambda bodies and comprehension scopes
/// are checked in a separate pass of the [`TypeInferenceBuilder`].
struct SubexpressionChecker<'a, 'db> {
    checker: &'a RedundantConditionChecker<'a, 'db>,
    condition_context: RedundantConditionContext,
}

impl<'ast> Visitor<'ast> for SubexpressionChecker<'_, '_> {
    fn visit_expr(&mut self, expression: &'ast ast::Expr) {
        let builder = self.checker.builder;

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
                self.checker.check(
                    operand,
                    builder.expression_type(operand),
                    self.checker
                        .expression_truthiness(operand, ExpressionEvaluationStrategy::Value),
                    ExpressionEvaluationStrategy::Value,
                    self.condition_context,
                );
            }

            ast::Expr::If(ast::ExprIf {
                test, body, orelse, ..
            }) => {
                self.checker.check(
                    test,
                    builder.expression_type(test),
                    builder.condition_truthiness(test),
                    ExpressionEvaluationStrategy::Condition,
                    self.condition_context,
                );
                self.visit_expr(body);
                self.visit_expr(orelse);
            }

            _ => walk_expr(self, expression),
        }
    }
}
