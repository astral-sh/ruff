use ruff_python_ast::Expr;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Fix};

/// ## What it does
/// Checks for missing `from __future__ import annotations` imports upon
/// detecting type annotations that can be written more succinctly under
/// PEP 563.
///
/// ## Why is this bad?
/// PEP 585 enabled the use of a number of convenient type annotations, such as
/// `list[str]` instead of `List[str]`. However, these annotations are only
/// available on Python 3.9 and higher, _unless_ the `from __future__ import annotations`
/// import is present.
///
/// Similarly, PEP 604 enabled the use of the `|` operator for unions, such as
/// `str | None` instead of `Optional[str]`. However, these annotations are only
/// available on Python 3.10 and higher, _unless_ the `from __future__ import annotations`
/// import is present.
///
/// By adding the `__future__` import, the pyupgrade rules can automatically
/// migrate existing code to use the new syntax, even for older Python versions.
/// This rule thus pairs well with pyupgrade and with Ruff's pyupgrade rules.
///
/// This rule respects the [`target-version`] setting. For example, if your
/// project targets Python 3.10 and above, adding `from __future__ import annotations`
/// does not impact your ability to leverage PEP 604-style unions (e.g., to
/// convert `Optional[str]` to `str | None`). As such, this rule will only
/// flag such usages if your project targets Python 3.9 or below.
///
/// ## Example
///
/// ```python
/// from typing import List, Dict, Optional
///
///
/// def func(obj: Dict[str, Optional[int]]) -> None: ...
/// ```
///
/// Use instead:
///
/// ```python
/// from __future__ import annotations
///
/// from typing import List, Dict, Optional
///
///
/// def func(obj: Dict[str, Optional[int]]) -> None: ...
/// ```
///
/// After running the additional pyupgrade rules:
///
/// ```python
/// from __future__ import annotations
///
///
/// def func(obj: dict[str, int | None]) -> None: ...
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as adding `from __future__ import annotations`
/// may change the semantics of the program.
///
/// ## Options
/// - `target-version`
#[derive(ViolationMetadata)]
pub(crate) struct FutureRewritableTypeAnnotation {
    name: String,
}

impl AlwaysFixableViolation for FutureRewritableTypeAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FutureRewritableTypeAnnotation { name } = self;
        format!("Add `from __future__ import annotations` to simplify `{name}`")
    }

    fn fix_title(&self) -> String {
        "Add `from __future__ import annotations`".to_string()
    }
}

/// FA100
pub(crate) fn future_rewritable_type_annotation(checker: &Checker, expr: &Expr) {
    let name = checker
        .semantic()
        .resolve_qualified_name(expr)
        .map(|binding| binding.to_string());

    let Some(name) = name else { return };

    checker
        .report_diagnostic(FutureRewritableTypeAnnotation { name }, expr.range())
        .set_fix(Fix::unsafe_edit(checker.importer().add_future_import()));
}
