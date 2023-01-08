use log::error;
use rustpython_ast::{Expr, Keyword, Stmt, StmtKind};

use crate::ast::helpers::{collect_call_paths, dealias_call_path, is_const_none, match_call_path};
use crate::ast::types::Range;
use crate::autofix::helpers;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// T201, T203
pub fn print_call(xxxxxxxx: &mut xxxxxxxx, func: &Expr, keywords: &[Keyword]) {
    let mut check = {
        let call_path = dealias_call_path(collect_call_paths(func), &xxxxxxxx.import_aliases);
        if match_call_path(&call_path, "", "print", &xxxxxxxx.from_imports) {
            // If the print call has a `file=` argument (that isn't `None`, `"sys.stdout"`,
            // or `"sys.stderr"`), don't trigger T201.
            if let Some(keyword) = keywords
                .iter()
                .find(|keyword| keyword.node.arg.as_ref().map_or(false, |arg| arg == "file"))
            {
                if !is_const_none(&keyword.node.value) {
                    let call_path = collect_call_paths(&keyword.node.value);
                    if !(match_call_path(&call_path, "sys", "stdout", &xxxxxxxx.from_imports)
                        || match_call_path(&call_path, "sys", "stderr", &xxxxxxxx.from_imports))
                    {
                        return;
                    }
                }
            }
            Diagnostic::new(violations::PrintFound, Range::from_located(func))
        } else if match_call_path(&call_path, "pprint", "pprint", &xxxxxxxx.from_imports) {
            Diagnostic::new(violations::PPrintFound, Range::from_located(func))
        } else {
            return;
        }
    };

    if !xxxxxxxx.settings.enabled.contains(check.kind.code()) {
        return;
    }

    if xxxxxxxx.patch(check.kind.code()) {
        let defined_by = xxxxxxxx.current_stmt();
        let defined_in = xxxxxxxx.current_stmt_parent();
        if matches!(defined_by.node, StmtKind::Expr { .. }) {
            let deleted: Vec<&Stmt> = xxxxxxxx
                .deletions
                .iter()
                .map(std::convert::Into::into)
                .collect();
            match helpers::delete_stmt(
                defined_by.into(),
                defined_in.map(std::convert::Into::into),
                &deleted,
                xxxxxxxx.locator,
            ) {
                Ok(fix) => {
                    if fix.content.is_empty() || fix.content == "pass" {
                        xxxxxxxx.deletions.insert(defined_by.clone());
                    }
                    check.amend(fix);
                }
                Err(e) => error!("Failed to remove print call: {e}"),
            }
        }
    }

    xxxxxxxx.diagnostics.push(check);
}
