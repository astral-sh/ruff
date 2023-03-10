use log::error;
use rustpython_parser::ast::{Stmt, StmtKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::{Range, RefEquality};

use crate::autofix::helpers::delete_stmt;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct EmptyTypeCheckingBlock;

impl AlwaysAutofixableViolation for EmptyTypeCheckingBlock {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Found empty type-checking block")
    }

    fn autofix_title(&self) -> String {
        format!("Delete empty type-checking block")
    }
}

/// TCH005
pub fn empty_type_checking_block<'a, 'b>(
    checker: &mut Checker<'a>,
    stmt: &'a Stmt,
    body: &'a [Stmt],
) where
    'b: 'a,
{
    if body.len() == 1 && matches!(body[0].node, StmtKind::Pass) {
        let mut diagnostic = Diagnostic::new(EmptyTypeCheckingBlock, Range::from(&body[0]));

        // Delete the entire type-checking block.
        if checker.patch(diagnostic.kind.rule()) {
            let parent = checker
                .ctx
                .child_to_parent
                .get(&RefEquality(stmt))
                .map(Into::into);
            let deleted: Vec<&Stmt> = checker.deletions.iter().map(Into::into).collect();
            match delete_stmt(
                stmt,
                parent,
                &deleted,
                checker.locator,
                checker.indexer,
                checker.stylist,
            ) {
                Ok(fix) => {
                    if fix.content.is_empty() || fix.content == "pass" {
                        checker.deletions.insert(RefEquality(stmt));
                    }
                    diagnostic.amend(fix);
                }
                Err(e) => error!("Failed to remove empty type-checking block: {e}"),
            }
        }

        checker.diagnostics.push(diagnostic);
    }
}
