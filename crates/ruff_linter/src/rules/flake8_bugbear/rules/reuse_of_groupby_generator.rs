use ruff_python_ast::{self as ast, Comprehension, Expr, ExprKind, Stmt, StmtKind, Suite};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for multiple usage of the generator returned from
/// `itertools.groupby()`.
///
/// ## Why is this bad?
/// Using the generator more than once will do nothing on the second usage.
/// If that data is needed later, it should be stored as a list.
///
/// ## Example:
/// ```python
/// import itertools
///
/// for name, group in itertools.groupby(data):
///     for _ in range(5):
///         do_something_with_the_group(group)
/// ```
///
/// Use instead:
/// ```python
/// import itertools
///
/// for name, group in itertools.groupby(data):
///     values = list(group)
///     for _ in range(5):
///         do_something_with_the_group(values)
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.260", category = Category::Suspicious)]
pub(crate) struct ReuseOfGroupbyGenerator;

impl Violation for ReuseOfGroupbyGenerator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Using the generator returned from `itertools.groupby()` more than once \
            will do nothing on the second usage"
            .to_string()
    }
}

/// A [`Visitor`] that collects all the usage of `group_name` in the body of
/// a `for` loop.
struct GroupNameFinder<'a> {
    /// Variable name for the group.
    group_name: &'a str,
    /// Number of times the `group_name` variable was seen during the visit.
    usage_count: u32,
    /// A flag indicating that the visitor is inside a nested `for` or `while`
    /// loop or inside a `dict`, `list` or `set` comprehension.
    nested: bool,
    /// A flag indicating that the `group_name` variable has been overridden
    /// during the visit.
    overridden: bool,
    /// A stack of counters where each counter is itself a list of usage count.
    /// This is used specifically for mutually exclusive statements such as an
    /// `if` or `match`.
    ///
    /// The stack element represents an entire `if` or `match` statement while
    /// the number inside the element represents the usage count for one of
    /// the branches of the statement. The order of the count corresponds the
    /// branch order.
    counter_stack: Vec<Vec<u32>>,
    /// A list of reused expressions.
    exprs: Vec<Expr<'a>>,
}

impl<'a> GroupNameFinder<'a> {
    fn new(group_name: &'a str) -> Self {
        GroupNameFinder {
            group_name,
            usage_count: 0,
            nested: false,
            overridden: false,
            counter_stack: Vec::new(),
            exprs: Vec::new(),
        }
    }

    fn name_matches(&self, expr: Expr<'_>) -> bool {
        if let ExprKind::Name(name_node_view) = expr.classify() {
            let id = name_node_view.id();
            id == self.group_name
        } else {
            false
        }
    }

    /// Increment the usage count for the group name by the given value.
    /// If we're in one of the branches of a mutually exclusive statement,
    /// then increment the count for that branch. Otherwise, increment the
    /// global count.
    fn increment_usage_count(&mut self, value: u32) {
        if let Some(last) = self.counter_stack.last_mut() {
            *last.last_mut().unwrap() += value;
        } else {
            self.usage_count += value;
        }
    }

    /// Reset the usage count for the group name by the given value.
    /// This function is called when there is a `continue`, `break`, or `return` statement.
    fn reset_usage_count(&mut self) {
        if let Some(last) = self.counter_stack.last_mut() {
            *last.last_mut().unwrap() = 0;
        } else {
            self.usage_count = 0;
        }
    }
}

impl<'a> Visitor<'a> for GroupNameFinder<'a> {
    fn visit_stmt(&mut self, stmt: Stmt<'a>) {
        if self.overridden {
            return;
        }
        match stmt.classify() {
            StmtKind::For(stmt_for) => {
                let target = stmt_for.target();
                let iter = stmt_for.iter();
                let body = stmt_for.body();

                if self.name_matches(target) {
                    self.overridden = true;
                } else {
                    if self.name_matches(iter) {
                        self.increment_usage_count(1);
                        // This could happen when the group is being looped
                        // over multiple times:
                        //      for item in group:
                        //          ...
                        //
                        //      # Group is being reused here
                        //      for item in group:
                        //          ...
                        if self.usage_count > 1 {
                            self.exprs.push(iter);
                        }
                    }
                    self.nested = true;
                    visitor::walk_body(self, body);
                    self.nested = false;
                }
            }
            StmtKind::While(stmt_while) => {
                let body = stmt_while.body();

                self.nested = true;
                visitor::walk_body(self, body);
                self.nested = false;
            }
            StmtKind::If(stmt_if) => {
                let test = stmt_if.test();
                let body = stmt_if.body();
                let elif_else_clauses = stmt_if.elif_else_clauses();

                // Visit the test before pushing the branch counters as it
                // is evaluated unconditionally.
                self.visit_expr(test);

                // base if plus branches
                let mut if_stack = Vec::with_capacity(1 + elif_else_clauses.len());
                // Initialize the vector with the count for the if branch.
                if_stack.push(0);
                self.counter_stack.push(if_stack);

                self.visit_body(body);

                for clause in elif_else_clauses {
                    self.counter_stack.last_mut().unwrap().push(0);
                    self.visit_elif_else_clause(clause);
                }

                if let Some(last) = self.counter_stack.pop() {
                    // This is the max number of group usage from all the
                    // branches of this `if` statement.
                    let max_count = last.into_iter().max().unwrap_or(0);
                    self.increment_usage_count(max_count);
                }
            }
            StmtKind::Match(stmt_match) => {
                let subject = stmt_match.subject();
                let cases = stmt_match.cases();

                // Visit the subject before pushing the branch counters as it
                // is evaluated unconditionally.
                self.visit_expr(subject);
                self.counter_stack.push(Vec::with_capacity(cases.len()));
                for match_case in cases {
                    self.counter_stack.last_mut().unwrap().push(0);
                    self.visit_match_case(match_case);
                }
                if let Some(last) = self.counter_stack.pop() {
                    // This is the max number of group usage from all the
                    // branches of this `match` statement.
                    let max_count = last.into_iter().max().unwrap_or(0);
                    self.increment_usage_count(max_count);
                }
            }
            StmtKind::Assign(stmt_assign) => {
                let targets = stmt_assign.targets();
                let value = stmt_assign.value();

                if targets.iter().any(|target| self.name_matches(target)) {
                    self.overridden = true;
                } else {
                    self.visit_expr(value);
                }
            }
            StmtKind::AnnAssign(stmt_ann_assign) => {
                let target = stmt_ann_assign.target();
                let value = stmt_ann_assign.value();

                if self.name_matches(target) {
                    self.overridden = true;
                } else if let Some(expr) = value {
                    self.visit_expr(expr);
                }
            }
            StmtKind::Continue(_) | StmtKind::Break(_) => {
                self.reset_usage_count();
            }
            StmtKind::Return(stmt_return) => {
                let value = stmt_return.value();

                if let Some(expr) = value {
                    self.visit_expr(expr);
                }
                self.reset_usage_count();
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_comprehension(&mut self, comprehension: Comprehension<'a>) {
        if self.name_matches(comprehension.target()) {
            self.overridden = true;
        }
        if self.overridden {
            return;
        }
        if self.name_matches(comprehension.iter()) {
            self.increment_usage_count(1);
            if self.usage_count > 1 {
                self.exprs.push(comprehension.iter());
            }
        }
    }

    fn visit_expr(&mut self, expr: Expr<'a>) {
        if let ExprKind::Named(named) = expr.classify() {
            let target = named.target();
            if self.name_matches(target) {
                self.overridden = true;
            }
        }
        if self.overridden {
            return;
        }

        match expr.fields() {
            ast::ExprFields::ListComp(ast::ExprListCompFields {
                elt,
                generators,
                range: _,
                node: _,
            })
            | ast::ExprFields::SetComp(ast::ExprSetCompFields {
                elt,
                generators,
                range: _,
                node: _,
            }) => {
                for comprehension in generators {
                    self.visit_comprehension(comprehension);
                }
                if !self.overridden {
                    self.nested = true;
                    visitor::walk_expr(self, elt);
                    self.nested = false;
                }
            }
            ast::ExprFields::DictComp(ast::ExprDictCompFields {
                key,
                value,
                generators,
                range: _,
                node: _,
            }) => {
                for comprehension in generators {
                    self.visit_comprehension(comprehension);
                }
                if !self.overridden {
                    self.nested = true;
                    if let Some(key) = key {
                        visitor::walk_expr(self, key);
                    }
                    visitor::walk_expr(self, value);
                    self.nested = false;
                }
            }
            _ => {
                if self.name_matches(expr) {
                    self.increment_usage_count(1);

                    let current_usage_count = self.usage_count
                        + self
                            .counter_stack
                            .iter()
                            .map(|count| count.last().unwrap_or(&0))
                            .sum::<u32>();

                    // For nested loops, the variable usage could be once but the
                    // loop makes it being used multiple times.
                    if self.nested || current_usage_count > 1 {
                        self.exprs.push(expr);
                    }
                } else {
                    visitor::walk_expr(self, expr);
                }
            }
        }
    }
}

/// B031
pub(crate) fn reuse_of_groupby_generator(
    checker: &Checker,
    target: Expr<'_>,
    body: Suite<'_>,
    iter: Expr<'_>,
) {
    let ExprKind::Call(call_node) = iter.classify() else {
        return;
    };
    let func = call_node.func();
    let ExprKind::Tuple(tuple) = target.classify() else {
        // Ignore any `groupby()` invocation that isn't unpacked
        return;
    };
    if tuple.elts().len() != 2 {
        return;
    }
    // We have an invocation of groupby which is a simple unpacking
    let ExprKind::Name(name_node) = tuple.elts().at(1).classify() else {
        return;
    };
    let group_name = name_node.id();
    // Check if the function call is `itertools.groupby`
    if !checker
        .semantic()
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["itertools", "groupby"]))
    {
        return;
    }
    let mut finder = GroupNameFinder::new(group_name);
    for stmt in body {
        finder.visit_stmt(stmt);
    }
    for expr in finder.exprs {
        checker.report_diagnostic(ReuseOfGroupbyGenerator, expr.range());
    }
}
