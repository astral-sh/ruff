use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::compose_call_path;
use ruff_python_semantic::analyze::typing::ModuleMember;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct NonPEP585Annotation {
    from: String,
    to: String,
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
pub(crate) fn use_pep585_annotation(
    checker: &mut Checker,
    expr: &Expr,
    replacement: &ModuleMember,
) {
    let Some(from) = compose_call_path(expr) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(
        NonPEP585Annotation {
            from,
            to: replacement.to_string(),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        if !checker.semantic_model().in_complex_string_type_definition() {
            match replacement {
                ModuleMember::BuiltIn(name) => {
                    // Built-in type, like `list`.
                    if checker.semantic_model().is_builtin(name) {
                        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                            (*name).to_string(),
                            expr.range(),
                        )));
                    }
                }
                ModuleMember::Member(module, member) => {
                    // Imported type, like `collections.deque`.
                    diagnostic.try_set_fix(|| {
                        let (import_edit, binding) = checker.importer.get_or_import_symbol(
                            module,
                            member,
                            expr.start(),
                            checker.semantic_model(),
                        )?;
                        let reference_edit = Edit::range_replacement(binding, expr.range());
                        Ok(Fix::suggested_edits(import_edit, [reference_edit]))
                    });
                }
            }
        }
    }
    checker.diagnostics.push(diagnostic);
}
