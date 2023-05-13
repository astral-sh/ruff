use rustpython_parser::ast::Expr;

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing::{to_pep585_generic, ModuleMember, SymbolReplacement};

use crate::autofix::actions::get_or_import_symbol;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct NonPEP585Annotation {
    from: ModuleMember<'static>,
    to: ModuleMember<'static>,
}

impl Violation for NonPEP585Annotation {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NonPEP585Annotation { from, to } = self;
        format!("Use `{to}` instead of `{from}` for type annotation")
    }

    fn autofix_title(&self) -> Option<String> {
        let NonPEP585Annotation { to, .. } = self;
        Some(format!("Replace with `{to}`"))
    }
}

/// UP006
pub(crate) fn use_pep585_annotation(checker: &mut Checker, expr: &Expr) {
    let Some(SymbolReplacement { from, to }) = to_pep585_generic(expr, &checker.ctx) else {
        return;
    };

    let fixable = !checker.ctx.in_complex_string_type_definition();
    let mut diagnostic = Diagnostic::new(
        NonPEP585Annotation {
            from: from.clone(),
            to: to.clone(),
        },
        expr.range(),
    );
    if fixable && checker.patch(diagnostic.kind.rule()) {
        if to.is_builtin() {
            // Built-in type, like `list`.
            if checker.ctx.is_builtin(to.member()) {
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                    to.member().to_string(),
                    expr.range(),
                )));
            }
        } else {
            // Imported type, like `collections.deque`.
            diagnostic.try_set_fix(|| {
                let (import_edit, binding) = get_or_import_symbol(
                    to.module(),
                    to.member(),
                    expr.start(),
                    &checker.ctx,
                    &checker.importer,
                    checker.locator,
                )?;
                let reference_edit = Edit::range_replacement(binding, expr.range());
                Ok(Fix::suggested_edits(import_edit, [reference_edit]))
            });
        }
    }
    checker.diagnostics.push(diagnostic);
}
