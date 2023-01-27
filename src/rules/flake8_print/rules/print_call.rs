use log::error;
use rustpython_ast::{Expr, Keyword, Stmt, StmtKind};

use crate::ast::helpers::is_const_none;
use crate::ast::types::Range;
use crate::autofix::helpers;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

/// T201, T203
pub fn print_call(checker: &mut Checker, func: &Expr, keywords: &[Keyword]) {
    let mut diagnostic = {
        let call_path = checker.resolve_call_path(func);
        if call_path
            .as_ref()
            .map_or(false, |call_path| *call_path.as_slice() == ["", "print"])
        {
            // If the print call has a `file=` argument (that isn't `None`, `"sys.stdout"`,
            // or `"sys.stderr"`), don't trigger T201.
            if let Some(keyword) = keywords
                .iter()
                .find(|keyword| keyword.node.arg.as_ref().map_or(false, |arg| arg == "file"))
            {
                if !is_const_none(&keyword.node.value) {
                    if checker
                        .resolve_call_path(&keyword.node.value)
                        .map_or(true, |call_path| {
                            call_path.as_slice() != ["sys", "stdout"]
                                && call_path.as_slice() != ["sys", "stderr"]
                        })
                    {
                        return;
                    }
                }
            }
            Diagnostic::new(violations::PrintFound, Range::from_located(func))
        } else if call_path.as_ref().map_or(false, |call_path| {
            *call_path.as_slice() == ["pprint", "pprint"]
        }) {
            Diagnostic::new(violations::PPrintFound, Range::from_located(func))
        } else {
            return;
        }
    };

    if !checker.settings.rules.enabled(diagnostic.kind.rule()) {
        return;
    }

    if checker.patch(diagnostic.kind.rule()) {
        let defined_by = checker.current_stmt();
        let defined_in = checker.current_stmt_parent();
        if matches!(defined_by.node, StmtKind::Expr { .. }) {
            let deleted: Vec<&Stmt> = checker
                .deletions
                .iter()
                .map(std::convert::Into::into)
                .collect();
            match helpers::delete_stmt(
                defined_by.into(),
                defined_in.map(std::convert::Into::into),
                &deleted,
                checker.locator,
                checker.indexer,
                checker.stylist,
            ) {
                Ok(fix) => {
                    if fix.content.is_empty() || fix.content == "pass" {
                        checker.deletions.insert(defined_by.clone());
                    }
                    diagnostic.amend(fix);
                }
                Err(e) => error!("Failed to remove print call: {e}"),
            }
        }
    }

    checker.diagnostics.push(diagnostic);
}
