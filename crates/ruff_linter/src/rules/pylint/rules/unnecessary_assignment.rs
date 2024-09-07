use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{name::Name, Expr, ExprName, Stmt, StmtAssign, StmtIf};
use ruff_python_codegen::Generator;
use ruff_python_index::Indexer;
use ruff_python_semantic::SemanticModel;
use ruff_source_file::Locator;
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
#[violation]
pub struct UnnecessaryAssignment {
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

type AssignmentBeforeIfStmt<'a> = (Expr, ExprName, StmtAssign);

/// PLR6103
pub(crate) fn unnecessary_assignment(checker: &mut Checker, stmt: &StmtIf) {
    if checker.settings.target_version < PythonVersion::Py38 {
        return;
    }

    let if_test = *stmt.test.clone();
    let semantic = checker.semantic();
    let mut errors: Vec<AssignmentBeforeIfStmt> = Vec::new();

    // case - if check (`if test1:`)
    if let Some(unreferenced_binding) = find_assignment_before_if_stmt(semantic, &if_test, &if_test)
    {
        errors.push(unreferenced_binding);
    };

    // case - bool operations (`if test1 and test2:`)
    if let Expr::BoolOp(expr) = if_test.clone() {
        errors.extend(
            expr.values
                .iter()
                .filter_map(|bool_test| {
                    find_assignment_before_if_stmt(semantic, &if_test, bool_test)
                })
                .collect::<Vec<AssignmentBeforeIfStmt>>(),
        );
    }

    // case - compare (`if test1 is not None:`)
    if let Expr::Compare(compare) = if_test.clone() {
        if let Some(error) = find_assignment_before_if_stmt(semantic, &if_test, &compare.left) {
            errors.push(error);
        };
    }

    // case - elif else clauses (`elif test1:`)
    let elif_else_clauses = stmt.elif_else_clauses.clone();
    errors.extend(
        elif_else_clauses
            .iter()
            .filter(|elif_else_clause| elif_else_clause.test.is_some())
            .filter_map(|elif_else_clause| {
                let elif_check = elif_else_clause.test.clone().unwrap();
                find_assignment_before_if_stmt(semantic, &elif_check, &elif_check)
            })
            .collect::<Vec<AssignmentBeforeIfStmt>>(),
    );

    // add found diagnostics
    checker.diagnostics.extend(
        errors
            .into_iter()
            .map(|error| {
                create_diagnostic(
                    checker.locator(),
                    checker.indexer(),
                    checker.generator(),
                    error,
                )
            })
            .collect::<Vec<Diagnostic>>(),
    );
}

/// Find possible assignment before if statement
///
/// * `if_test` - the complete if test (`if test1 and test2`)
/// * `if_test_part` - part of the if test (`test1`)
fn find_assignment_before_if_stmt<'a>(
    semantic: &'a SemanticModel,
    if_test: &Expr,
    if_test_part: &Expr,
) -> Option<AssignmentBeforeIfStmt<'a>> {
    // early exit when the test part is not a variable
    let Expr::Name(test_variable) = if_test_part.clone() else {
        return None;
    };

    let current_statement = semantic.current_statement();
    let previous_statement = semantic
        .previous_statement(current_statement)?
        .as_assign_stmt()?;

    // only care about single assignment target like `x = 'example'`
    let [assigned_variable] = &previous_statement.targets[..] else {
        return None;
    };

    // check whether the check variable is the assignment variable
    if test_variable.id != assigned_variable.as_name_expr()?.id {
        return None;
    }

    Some((if_test.clone(), test_variable, previous_statement.clone()))
}

fn create_diagnostic(
    locator: &Locator,
    indexer: &Indexer,
    generator: Generator,
    error: AssignmentBeforeIfStmt,
) -> Diagnostic {
    let (origin, expr_name, assignment) = error;
    let assignment_expr = generator.expr(&assignment.value.clone());
    let use_parentheses = origin.is_bool_op_expr() || !assignment.value.is_name_expr();

    let mut diagnostic = Diagnostic::new(
        UnnecessaryAssignment {
            name: expr_name.clone().id,
            assignment: assignment_expr.clone(),
            parentheses: use_parentheses,
        },
        expr_name.clone().range(),
    );

    let format = if use_parentheses {
        format!("({} := {})", expr_name.clone().id, assignment_expr.clone())
    } else {
        format!("{} := {}", expr_name.clone().id, assignment_expr.clone())
    };

    let delete_assignment_edit = delete_stmt(&Stmt::from(assignment), None, locator, indexer);
    let use_walrus_edit = Edit::range_replacement(format, diagnostic.range());

    diagnostic.set_fix(Fix::unsafe_edits(delete_assignment_edit, [use_walrus_edit]));

    diagnostic
}
