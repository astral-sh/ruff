use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::registry::AsRule;

#[violation]
pub struct DatetimeTimezoneUTC;

impl Violation for DatetimeTimezoneUTC {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `datetime.UTC` alias")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Convert to `datetime.UTC` alias".to_string())
    }
}

/// UP017
pub(crate) fn datetime_utc_alias(checker: &mut Checker, expr: &Expr) {
    if checker
        .semantic()
        .resolve_call_path(expr)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["datetime", "timezone", "utc"])
        })
    {
        let mut diagnostic = Diagnostic::new(DatetimeTimezoneUTC, expr.range());
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.try_set_fix(|| {
                let (import_edit, binding) = checker.importer.get_or_import_symbol(
                    &ImportRequest::import_from("datetime", "UTC"),
                    expr.start(),
                    checker.semantic(),
                )?;
                let reference_edit = Edit::range_replacement(binding, expr.range());
                Ok(Fix::suggested_edits(import_edit, [reference_edit]))
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}
