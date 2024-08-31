use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{name::Name, Expr, ExprName, Stmt, StmtAssign, StmtIf};
use ruff_python_codegen::Generator;
use ruff_python_index::Indexer;
use ruff_python_semantic::{Binding, BindingKind, NodeRef, ResolvedReferenceId, SemanticModel};
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

type UnreferencedBinding<'a> = (Expr, &'a Binding<'a>, ExprName, StmtAssign);

fn find_unreferenced_binding_from_check<'a>(
    origin: &Expr,
    semantic: &'a SemanticModel,
    check: &Expr,
) -> Option<UnreferencedBinding<'a>> {
    // only care about variable expressions, like `if test1:`
    let Expr::Name(check_variable) = check.clone() else {
        return None;
    };

    let scope: &ruff_python_semantic::Scope<'_> = semantic.current_scope();
    let Some(binding) = scope
        .bindings()
        .find(|(_, binding_id)| {
            let binding = semantic.binding(*binding_id);

            // only bindings that come before the check
            if binding.range().start() > check_variable.range().start() {
                return false;
            }

            // we are only interested in assignment bindings
            let BindingKind::Assignment = binding.kind else {
                return false;
            };

            let assignment = semantic.node(binding.source.unwrap());

            // then we only care for expressions
            let NodeRef::Expr(assignment_expr) = assignment else {
                return false;
            };

            // and only if those are named expressions
            let Expr::Name(assignment_variable) = assignment_expr else {
                return false;
            };

            // ignore if the ids do not match
            if check_variable.id != assignment_variable.id {
                return false;
            }

            // we can only walrus if the binding has no references
            if !binding
                .references
                .iter()
                // only keep references that come before the `if_value`
                .filter(|&&reference_id| {
                    let reference = semantic.reference(reference_id);
                    reference.range().start() < check_variable.range().start()
                })
                .collect::<Vec<&ResolvedReferenceId>>()
                .is_empty()
            {
                return false;
            }

            true
        })
        .map(|(_, binding_id)| semantic.binding(binding_id))
    else {
        // we did not find a binding matching our criteria
        return None;
    };

    let assignment: &ruff_python_ast::StmtAssign = semantic
        .statement(binding.source.unwrap())
        .as_assign_stmt()
        .unwrap();

    Some((origin.clone(), binding, check_variable, assignment.clone()))
}

fn create_diagnostic(
    locator: &Locator,
    indexer: &Indexer,
    generator: Generator,
    unreferenced_binding: UnreferencedBinding,
) -> Diagnostic {
    let (origin, _, expr_name, assignment) = unreferenced_binding;
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
        // assignment.clone().range(),
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

/// PLR6103
pub(crate) fn unnecessary_assignment(checker: &mut Checker, stmt: &StmtIf) {
    let if_check = *stmt.test.clone();
    let semantic = checker.semantic();
    let mut unreferenced_bindings: Vec<UnreferencedBinding> = Vec::new();

    // case - if check (`if test1:`)
    if let Some(unreferenced_binding) =
        find_unreferenced_binding_from_check(&if_check, checker.semantic(), &if_check)
    {
        unreferenced_bindings.push(unreferenced_binding);
    };

    // case - bool operations (`if test1 and test2:`)
    if let Expr::BoolOp(expr) = if_check.clone() {
        unreferenced_bindings.extend(
            expr.values
                .iter()
                .filter_map(|value| {
                    find_unreferenced_binding_from_check(&if_check, semantic, value)
                })
                .collect::<Vec<UnreferencedBinding>>(),
        );
    }

    // case - compare (`if test1 is not None:`)
    if let Expr::Compare(compare) = if_check.clone() {
        if let Some(unreferenced_binding) =
            find_unreferenced_binding_from_check(&if_check, checker.semantic(), &compare.left)
        {
            unreferenced_bindings.push(unreferenced_binding);
        };
    }

    // case - elif else clauses (`elif test1:`)
    let elif_else_clauses = stmt.elif_else_clauses.clone();
    unreferenced_bindings.extend(
        elif_else_clauses
            .iter()
            .filter(|elif_else_clause| elif_else_clause.test.is_some())
            .filter_map(|elif_else_clause| {
                let elif_check = elif_else_clause.test.clone().unwrap();
                find_unreferenced_binding_from_check(&elif_check, semantic, &elif_check)
            })
            .collect::<Vec<UnreferencedBinding>>(),
    );

    // add found diagnostics
    checker.diagnostics.extend(
        unreferenced_bindings
            .into_iter()
            .map(|unreferenced_binding| {
                create_diagnostic(
                    checker.locator(),
                    checker.indexer(),
                    checker.generator(),
                    unreferenced_binding,
                )
            })
            .collect::<Vec<Diagnostic>>(),
    );
}
