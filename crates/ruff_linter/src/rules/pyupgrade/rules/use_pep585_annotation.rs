use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::compose_call_path;
use ruff_python_semantic::analyze::typing::ModuleMember;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Checks for the use of generics that can be replaced with standard library
/// variants based on [PEP 585].
///
/// ## Why is this bad?
/// [PEP 585] enabled collections in the Python standard library (like `list`)
/// to be used as generics directly, instead of importing analogous members
/// from the `typing` module (like `typing.List`).
///
/// When available, the [PEP 585] syntax should be used instead of importing
/// members from the `typing` module, as it's more concise and readable.
/// Importing those members from `typing` is considered deprecated as of [PEP
/// 585].
///
/// This rule is enabled when targeting Python 3.9 or later (see:
/// [`target-version`]). By default, it's _also_ enabled for earlier Python
/// versions if `from __future__ import annotations` is present, as
/// `__future__` annotations are not evaluated at runtime. If your code relies
/// on runtime type annotations (either directly or via a library like
/// Pydantic), you can disable this behavior for Python versions prior to 3.9
/// by setting [`pyupgrade.keep-runtime-typing`] to `true`.
///
/// ## Example
/// ```python
/// from typing import List
///
/// foo: List[int] = [1, 2, 3]
/// ```
///
/// Use instead:
/// ```python
/// foo: list[int] = [1, 2, 3]
/// ```
///
/// ## Options
/// - `target-version`
/// - `pyupgrade.keep-runtime-typing`
///
/// [PEP 585]: https://peps.python.org/pep-0585/
#[violation]
pub struct NonPEP585Annotation {
    from: String,
    to: String,
}

impl Violation for NonPEP585Annotation {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NonPEP585Annotation { from, to } = self;
        format!("Use `{to}` instead of `{from}` for type annotation")
    }

    fn fix_title(&self) -> Option<String> {
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
    if !checker.semantic().in_complex_string_type_definition() {
        match replacement {
            ModuleMember::BuiltIn(name) => {
                // Built-in type, like `list`.
                if checker.semantic().is_builtin(name) {
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        (*name).to_string(),
                        expr.range(),
                    )));
                }
            }
            ModuleMember::Member(module, member) => {
                // Imported type, like `collections.deque`.
                diagnostic.try_set_fix(|| {
                    let (import_edit, binding) = checker.importer().get_or_import_symbol(
                        &ImportRequest::import_from(module, member),
                        expr.start(),
                        checker.semantic(),
                    )?;
                    let reference_edit = Edit::range_replacement(binding, expr.range());
                    Ok(Fix::unsafe_edits(import_edit, [reference_edit]))
                });
            }
        }
    }
    checker.diagnostics.push(diagnostic);
}
