use crate::checkers::ast::Checker;
use crate::fix::edits::delete_stmt;
use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{identifier, Expr, Stmt, StmtExpr, StmtFor};
use ruff_text_size::{Ranged, TextRange};

/// ## What it does
/// Checks for branches whose bodies contain only `pass` and `...` statements.
///
/// ## Why is this bad?
/// Such a branch is unnecessary.
///
/// ## Example
///
/// ```python
/// if foo:
///     bar()
/// else:
///     pass
/// ```
///
/// ```python
/// if foo:
///     bar()
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct EmptyBranch {
    fixable: bool,
}

impl Violation for EmptyBranch {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Empty code branch".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        if self.fixable {
            Some("Remove branch".to_string())
        } else {
            Some("Refactor this branch".to_string())
        }
    }
}

/// RUF050
fn empty_branch(checker: &mut Checker, range: TextRange, fix: Option<Fix>) {
    let fixable = fix.is_some();
    let mut diagnostic = Diagnostic::new(EmptyBranch { fixable }, range);

    diagnostic.try_set_optional_fix(|| Ok(fix));
    checker.diagnostics.push(diagnostic)
}

/// RUF050: `if`-`elif`-`else`
pub(crate) fn empty_branch_if(checker: &mut Checker, stmt: &Stmt) {
    let Stmt::If(stmt_if) = stmt else {
        return;
    };
    let (body, elif_else_clauses) = (&stmt_if.body, &stmt_if.elif_else_clauses);

    let (indexer, locator, semantic) = (checker.indexer(), checker.locator(), checker.semantic());

    let Some(if_body_end) = body.last().map(Ranged::end) else {
        return;
    };

    if body_is_empty(body) {
        let parent_stmt = semantic.current_statement_parent();
        let no_elif_else = elif_else_clauses.iter().all(|it| body_is_empty(&it.body));

        let range = if no_elif_else {
            stmt.range()
        } else {
            TextRange::new(stmt.start(), if_body_end)
        };

        let edit = no_elif_else.then(|| delete_stmt(stmt, parent_stmt, locator, indexer));
        let fix = edit.map(|it| make_fix(checker, it, Applicability::Unsafe));

        empty_branch(checker, range, fix);

        if no_elif_else {
            return;
        }
    }

    for (index, clause) in elif_else_clauses.iter().enumerate() {
        if !body_is_empty(&clause.body) {
            continue;
        }

        let clause_is_else = clause.test.is_none();
        let clause_is_last = index == elif_else_clauses.len() - 1;

        let base_applicability = match (clause_is_else, clause_is_last) {
            (true, _) => Applicability::Safe,
            (_, true) => Applicability::Unsafe,
            (_, false) => Applicability::DisplayOnly,
        };

        let previous_clause = elif_else_clauses[..index].last();
        let previous_clause_end = previous_clause.map(Ranged::end).unwrap_or(if_body_end);

        let edit = Edit::deletion(previous_clause_end, clause.end());
        let fix = make_fix(checker, edit, base_applicability);

        empty_branch(checker, clause.range, Some(fix));
    }
}

/// RUF050: `for`-`else`
pub(crate) fn empty_branch_for(checker: &mut Checker, stmt: &Stmt) {
    let Stmt::For(StmtFor { body, orelse, .. }) = stmt else {
        return;
    };

    let (indexer, locator, semantic) = (checker.indexer(), checker.locator(), checker.semantic());

    let for_body_is_empty = body_is_empty(body);
    let else_body_is_empty = body_is_empty(orelse);

    if for_body_is_empty {
        let parent_stmt = semantic.current_statement_parent();
        let no_else_branch = orelse.is_empty() || else_body_is_empty;

        let edit = no_else_branch.then(|| delete_stmt(stmt, parent_stmt, locator, indexer));
        let fix = edit.map(|it| make_fix(checker, it, Applicability::Unsafe));

        empty_branch(checker, stmt.range(), fix);

        if no_else_branch {
            return;
        }
    }

    let Some(for_body_end) = body.last().map(Ranged::end) else {
        return;
    };

    if else_body_is_empty {
        let Some(else_keyword_range) = identifier::else_loop(stmt, locator.contents()) else {
            return;
        };
        let Some(else_branch_end) = orelse.last().map(Ranged::end) else {
            return;
        };
        let range = TextRange::new(else_keyword_range.start(), else_branch_end);

        let edit = Edit::deletion(for_body_end, stmt.end());
        let fix = make_fix(checker, edit, Applicability::Safe);

        empty_branch(checker, range, Some(fix));
    }
}

/// RUF050: `try`-`except`-`else`-`finally`
pub(crate) fn empty_branch_try(checker: &mut Checker, stmt: &Stmt) {
    let Stmt::Try(stmt_try) = stmt else {
        return;
    };
    let (body, handlers, orelse, finalbody) = (
        &stmt_try.body,
        &stmt_try.handlers,
        &stmt_try.orelse,
        &stmt_try.finalbody,
    );

    let (indexer, locator, semantic) = (checker.indexer(), checker.locator(), checker.semantic());

    let try_body_is_empty = body_is_empty(body);
    let else_body_is_empty = body_is_empty(orelse);
    let finally_body_is_empty = body_is_empty(finalbody);

    let Some(try_body_end) = body.last().map(Ranged::end) else {
        return;
    };
    let last_handler_end = handlers.last().map(Ranged::end);
    let else_branch_end = orelse.last().map(Ranged::end);

    if try_body_is_empty {
        let parent_stmt = semantic.current_statement_parent();
        let no_else = orelse.is_empty() || else_body_is_empty;
        let no_finally = finalbody.is_empty() || finally_body_is_empty;
        let no_else_no_finally = no_else && no_finally;

        let range = if no_else_no_finally {
            stmt.range()
        } else {
            TextRange::new(stmt_try.start(), try_body_end)
        };

        let edit = no_else_no_finally.then(|| delete_stmt(stmt, parent_stmt, locator, indexer));
        let fix = edit.map(|it| make_fix(checker, it, Applicability::Safe));

        empty_branch(checker, range, fix);

        if no_else_no_finally {
            return;
        }
    }

    if body_is_empty(orelse) {
        let Some(else_keyword_range) = identifier::else_try(stmt, locator.contents()) else {
            return;
        };
        let else_branch_end = else_branch_end.unwrap();
        let range = TextRange::new(else_keyword_range.start(), else_branch_end);

        let edit = Edit::deletion(last_handler_end.unwrap_or(try_body_end), else_branch_end);
        let fix = make_fix(checker, edit, Applicability::Safe);

        empty_branch(checker, range, Some(fix));
    }

    if body_is_empty(finalbody) {
        let Some(finally_keyword_range) = identifier::finally(stmt, locator.contents()) else {
            return;
        };
        let Some(finally_branch_end) = finalbody.last().map(Ranged::end) else {
            return;
        };
        let range = TextRange::new(finally_keyword_range.start(), finally_branch_end);

        let last_branch_end = else_branch_end.or(last_handler_end).unwrap_or(try_body_end);
        let edit = Edit::deletion(last_branch_end, finally_branch_end);
        let fix = make_fix(checker, edit, Applicability::Safe);

        empty_branch(checker, range, Some(fix));
    }
}

/// Whether `body` contains only `pass` or `...` statement.
fn body_is_empty(body: &[Stmt]) -> bool {
    if body.len() == 0 {
        return false;
    }

    body.iter().all(|stmt| match stmt {
        Stmt::Pass(..) => true,
        Stmt::Expr(StmtExpr { value, .. }) => matches!(value.as_ref(), Expr::EllipsisLiteral(..)),
        _ => false,
    })
}

fn make_fix(checker: &Checker, edit: Edit, base_applicability: Applicability) -> Fix {
    let source = checker.source();

    let min_applicability = if checker.comment_ranges().has_comments(&edit, source) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };
    let applicability = match (base_applicability, min_applicability) {
        (Applicability::DisplayOnly, _) | (_, Applicability::DisplayOnly) => {
            Applicability::DisplayOnly
        }
        (Applicability::Unsafe, _) | (_, Applicability::Unsafe) => Applicability::Unsafe,
        _ => Applicability::Safe,
    };

    Fix::applicable_edit(edit, applicability)
}
