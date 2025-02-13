use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{name::Name, Expr, ExprName, Stmt, StmtAssign, StmtIf};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, fix::edits::delete_stmt, settings::types::PythonVersion};

/// ## What it does
/// Check for cases where an variable assignment is directly followed by an if statement, these can be combined into a single statement using the `:=` operator.
///
/// ## Why is this bad?
/// The code can written more concise, often improving readability.
///
/// ## Example
///
/// ```python
/// test1 = "example"
/// if test1:
///     print(test1)
/// ```
///
/// Use instead:
///
/// ```python
/// if test1 := "example":
///     print(test1)
/// ```
///
/// ## References
/// - [PEP 572 – Assignment Expressions](https://peps.python.org/pep-0572/)
/// - [What’s New In Python 3.8 - Assignment Expressions](https://docs.python.org/3/whatsnew/3.8.html#assignment-expressions)
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryAssignment {
    name: Name,
    assignment: String,
    parentheses: bool,
}

impl UnnecessaryAssignment {
    fn get_fix(&self) -> String {
        let UnnecessaryAssignment {
            name,
            assignment,
            parentheses,
        } = self;
        if *parentheses {
            format!("({name} := {assignment})")
        } else {
            format!("{name} := {assignment}")
        }
    }
}

impl Violation for UnnecessaryAssignment {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use walrus operator `{}`.", self.get_fix())
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!(
            "Move variable assignment into if statement using walrus operator `{}`.",
            self.get_fix()
        ))
    }
}

type AssignmentBeforeIfStmt<'a> = (&'a Expr, &'a ExprName, &'a StmtAssign);

/// PLR6103
pub(crate) fn unnecessary_assignment(checker: &mut Checker, stmt: &StmtIf) {
    // early out - unsupported python versions
    if checker.settings.target_version < PythonVersion::Py38 {
        return;
    }

    let if_test = &*stmt.test;

    let previous_assignment = PreviousAssignment::new(checker.semantic());
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // early out - only simple if checks, without elif or else clauses
    if !&stmt.elif_else_clauses.is_empty() {
        return;
    }

    // case - simple if checks (`if test1:`)
    if let Some(unreferenced_binding) =
        find_assignment_before_if_stmt(&previous_assignment, if_test, if_test)
    {
        diagnostics.push(create_diagnostic(checker, unreferenced_binding));
    };

    match &*stmt.test {
        // case - bool operations (`if test1 and test2:`)
        Expr::BoolOp(expr) => diagnostics.extend(expr.values.iter().filter_map(|bool_test| {
            Some(create_diagnostic(
                checker,
                find_assignment_before_if_stmt(&previous_assignment, if_test, bool_test)?,
            ))
        })),

        // case - compare (`if test1 is not None:`)
        Expr::Compare(compare) => {
            if let Some(error) =
                find_assignment_before_if_stmt(&previous_assignment, if_test, &compare.left)
            {
                diagnostics.push(create_diagnostic(checker, error));
            };
        }

        _ => {}
    }

    // add found diagnostics
    checker.report_diagnostics(diagnostics);
}

/// Find possible assignment before if statement
///
/// * `if_test` - the complete if test (`if test1 and test2`)
/// * `if_test_part` - part of the if test (`test1`)
fn find_assignment_before_if_stmt<'a>(
    previous_assignment: &PreviousAssignment<'a>,
    if_test: &'a Expr,
    if_test_part: &'a Expr,
) -> Option<AssignmentBeforeIfStmt<'a>> {
    // early exit when the test part is not a variable
    let test_variable = if_test_part.as_name_expr()?;

    let assignment = previous_assignment.get()?;

    // only care about single assignment target like `x = 'example'`
    let [assigned_variable] = &assignment.targets[..] else {
        return None;
    };

    // check whether the check variable is the assignment variable
    if test_variable.id != assigned_variable.as_name_expr()?.id {
        return None;
    }

    Some((if_test, test_variable, assignment))
}

fn create_diagnostic(checker: &Checker, error: AssignmentBeforeIfStmt) -> Diagnostic {
    let (origin, expr_name, assignment) = error;
    let assignment_expr = checker.generator().expr(&assignment.value);
    let use_parentheses = origin.is_bool_op_expr() || !assignment.value.is_name_expr();

    let mut diagnostic = Diagnostic::new(
        UnnecessaryAssignment {
            name: expr_name.id.clone(),
            assignment: assignment_expr.clone(),
            parentheses: use_parentheses,
        },
        expr_name.range(),
    );

    let format = if use_parentheses {
        format!("({} := {})", expr_name.id, assignment_expr)
    } else {
        format!("{} := {}", expr_name.id, assignment_expr)
    };

    let delete_assignment_edit = delete_stmt(
        &Stmt::from(assignment.clone()),
        None,
        checker.locator(),
        checker.indexer(),
    );
    let use_walrus_edit = Edit::range_replacement(format, diagnostic.range());

    diagnostic.set_fix(Fix::unsafe_edits(delete_assignment_edit, [use_walrus_edit]));

    diagnostic
}

struct PreviousAssignment<'a> {
    memory: std::cell::OnceCell<Option<&'a StmtAssign>>,
    semantic: &'a SemanticModel<'a>,
}

impl<'a> PreviousAssignment<'a> {
    fn new(semantic: &'a SemanticModel<'a>) -> Self {
        Self {
            memory: std::cell::OnceCell::new(),
            semantic,
        }
    }

    fn get(&self) -> Option<&'a StmtAssign> {
        *self.memory.get_or_init(|| {
            let current_statement = self.semantic.current_statement();

            self.semantic
                .previous_statement(current_statement)
                .and_then(|stmt| stmt.as_assign_stmt())
        })
    }
}
