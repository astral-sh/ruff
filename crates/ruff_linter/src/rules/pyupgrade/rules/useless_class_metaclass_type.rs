use crate::checkers::ast::Checker;
use crate::fix::edits::{Parentheses, remove_argument};
use crate::{Fix, FixAvailability, Violation};
use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::StmtClassDef;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for `metaclass=type` in class definitions.
///
/// ## Why is this bad?
/// Since Python 3, the default metaclass is `type`, so specifying it explicitly is redundant.
///
/// Even though `__prepare__` is not required, the default metaclass (`type`) implements it,
/// for the convenience of subclasses calling it via `super()`.
/// ## Example
///
/// ```python
/// class Foo(metaclass=type): ...
/// ```
///
/// Use instead:
///
/// ```python
/// class Foo: ...
/// ```
///
/// ## References
/// - [PEP 3115 – Metaclasses in Python 3000](https://peps.python.org/pep-3115/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.13.0")]
pub(crate) struct UselessClassMetaclassType {
    name: String,
}

impl Violation for UselessClassMetaclassType {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UselessClassMetaclassType { name } = self;
        format!("Class `{name}` uses `metaclass=type`, which is redundant")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove `metaclass=type`".to_string())
    }
}

/// UP050
pub(crate) fn useless_class_metaclass_type(checker: &Checker, class_def: &StmtClassDef) {
    let Some(arguments) = class_def.arguments.as_deref() else {
        return;
    };

    for keyword in &arguments.keywords {
        if let (Some("metaclass"), expr) = (keyword.arg.as_deref(), &keyword.value) {
            if checker.semantic().match_builtin_expr(expr, "type") {
                let mut diagnostic = checker.report_diagnostic(
                    UselessClassMetaclassType {
                        name: class_def.name.to_string(),
                    },
                    keyword.range(),
                );

                diagnostic.try_set_fix(|| {
                    let edit = remove_argument(
                        keyword,
                        arguments,
                        Parentheses::Remove,
                        checker.locator().contents(),
                        checker.comment_ranges(),
                    )?;

                    let range = edit.range();
                    let applicability = if checker.comment_ranges().intersects(range) {
                        Applicability::Unsafe
                    } else {
                        Applicability::Safe
                    };

                    Ok(Fix::applicable_edit(edit, applicability))
                });
            }
        }
    }
}
