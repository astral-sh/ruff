use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{name::Name, AstNode, Expr, ExprName, Stmt, StmtAssign, StmtIf};
use ruff_python_codegen::Generator;
use ruff_python_index::Indexer;
use ruff_python_semantic::{
    Binding, BindingKind, NodeId, NodeRef, ResolvedReferenceId, Scope, SemanticModel,
};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, fix::edits::delete_stmt};

/// ## What it does
/// Check for cases where an assignment expression could be used.
///
/// ## Why is this bad?
/// The code can written more concise, often improving readability.
///
/// ## Example
///
/// ```python
/// test1 = "example"
/// if test1:
///     print("example!")
/// ```
///
/// Use instead:
///
/// ```python
/// if test1 := "example":
///     print("example!")
/// ``
#[violation]
pub struct UnnecessaryAssignment {
    check: bool,
    name: Name,
    assignment: String,
    parentheses: bool,
}

impl Violation for UnnecessaryAssignment {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryAssignment {
            check,
            name,
            assignment,
            parentheses,
        } = self;
        if !check {
            return format!("Unnecessary assignment");
        }
        if *parentheses {
            format!("Use walrus operator `({name} := {assignment})`.")
        } else {
            format!("Use walrus operator `{name} := {assignment}`.")
        }
    }

    fn fix_title(&self) -> Option<String> {
        let UnnecessaryAssignment {
            name,
            assignment,
            parentheses,
            ..
        } = self;
        if *parentheses {
            Some(format!(
                "Moves assignment into check using walrus operator (`{name} := {assignment}`)."
            ))
        } else {
            Some(format!(
                "Moves assignment into check using walrus operator `{name} := {assignment}`."
            ))
        }
    }
}

type AssignmentBeforeIf<'a> = (Expr, ExprName, StmtAssign);

/// PLR6103
pub(crate) fn unnecessary_assignment(checker: &mut Checker, stmt: &StmtIf) {
    let if_check = *stmt.test.clone();
    let semantic = checker.semantic();
    let mut errors: Vec<AssignmentBeforeIf> = Vec::new();

    // case - if check (`if test1:`)
    if let Some(unreferenced_binding) =
        find_assignment_before_if(&if_check, checker.semantic(), &if_check)
    {
        errors.push(unreferenced_binding);
    };

    // case - bool operations (`if test1 and test2:`)
    if let Expr::BoolOp(expr) = if_check.clone() {
        errors.extend(
            expr.values
                .iter()
                .filter_map(|value| find_assignment_before_if(&if_check, semantic, value))
                .collect::<Vec<AssignmentBeforeIf>>(),
        );
    }

    // case - compare (`if test1 is not None:`)
    if let Expr::Compare(compare) = if_check.clone() {
        if let Some(error) = find_assignment_before_if(&if_check, checker.semantic(), &compare.left)
        {
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
                find_assignment_before_if(&elif_check, semantic, &elif_check)
            })
            .collect::<Vec<AssignmentBeforeIf>>(),
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

fn find_assignment_before_if<'a>(
    origin: &Expr,
    semantic: &'a SemanticModel,
    check: &Expr,
) -> Option<AssignmentBeforeIf<'a>> {
    // only care about variable expressions, like `if test1:`
    let Expr::Name(check_variable) = check.clone() else {
        return None;
    };

    let current_statement = semantic.current_statement();
    let previous_statement = semantic
        .previous_statement(current_statement)?
        .as_assign_stmt()?;

    // only care about single assignment target like `x = 'example'`
    let [target] = &previous_statement.targets[..] else {
        return None;
    };

    // check whether the check variable is the assignment variable
    if check_variable.id != target.as_name_expr()?.id {
        return None;
    }

    Some((origin.clone(), check_variable, previous_statement.clone()))
}

fn create_diagnostic(
    locator: &Locator,
    indexer: &Indexer,
    generator: Generator,
    error: AssignmentBeforeIf,
) -> Diagnostic {
    let (origin, expr_name, assignment) = error;
    let value_expr = generator.expr(&assignment.value.clone());
    let use_parentheses = origin.is_bool_op_expr() || !assignment.value.is_name_expr();

    let mut diagnostic = Diagnostic::new(
        UnnecessaryAssignment {
            check: true,
            name: expr_name.clone().id,
            assignment: value_expr.clone(),
            parentheses: use_parentheses,
        },
        expr_name.clone().range(),
    );

    let format = if use_parentheses {
        format!("({} := {})", expr_name.clone().id, value_expr.clone())
    } else {
        format!("{} := {}", expr_name.clone().id, value_expr.clone())
    };

    let delete_assignment_edit = delete_stmt(&Stmt::from(assignment), None, locator, indexer);
    let use_walrus_edit = Edit::range_replacement(format, diagnostic.range());

    diagnostic.set_fix(Fix::unsafe_edits(delete_assignment_edit, [use_walrus_edit]));

    diagnostic
}
