use rustpython_parser::ast::{Expr, Keyword};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct Print;

impl Violation for Print {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`print` found")
    }
}

#[violation]
pub struct PPrint;

impl Violation for PPrint {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`pprint` found")
    }
}

/// T201, T203
pub fn print_call(checker: &mut Checker, func: &Expr, keywords: &[Keyword]) {
    let diagnostic = {
        let call_path = checker.ctx.resolve_call_path(func);
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
                    if checker.ctx.resolve_call_path(&keyword.node.value).map_or(
                        true,
                        |call_path| {
                            call_path.as_slice() != ["sys", "stdout"]
                                && call_path.as_slice() != ["sys", "stderr"]
                        },
                    ) {
                        return;
                    }
                }
            }
            Diagnostic::new(Print, Range::from(func))
        } else if call_path.as_ref().map_or(false, |call_path| {
            *call_path.as_slice() == ["pprint", "pprint"]
        }) {
            Diagnostic::new(PPrint, Range::from(func))
        } else {
            return;
        }
    };

    if !checker.settings.rules.enabled(diagnostic.kind.rule()) {
        return;
    }

    checker.diagnostics.push(diagnostic);
}
