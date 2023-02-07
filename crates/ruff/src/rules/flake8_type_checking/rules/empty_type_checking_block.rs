use log::error;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Stmt, StmtKind};

use crate::ast::types::{Range, RefEquality};
use crate::autofix::helpers::delete_stmt;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct EmptyTypeCheckingBlock;
);
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
        let mut diagnostic = Diagnostic::new(EmptyTypeCheckingBlock, Range::from_located(&body[0]));

        // Delete the entire type-checking block.
        if checker.patch(diagnostic.kind.rule()) {
            let parent = checker
                .child_to_parent
                .get(&RefEquality(stmt))
                .map(std::convert::Into::into);
            let deleted: Vec<&Stmt> = checker
                .deletions
                .iter()
                .map(std::convert::Into::into)
                .collect();
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
