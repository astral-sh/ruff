use ruff_python_ast::{self as ast, Arguments, Expr};
use rustc_hash::{FxBuildHasher, FxHashSet};

use ruff_diagnostics::{Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};

/// ## What it does
/// Checks for duplicate base classes in class definitions.
///
/// ## Why is this bad?
/// Including duplicate base classes will raise a `TypeError` at runtime.
///
/// ## Example
/// ```python
/// class Foo:
///     pass
///
///
/// class Bar(Foo, Foo):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     pass
///
///
/// class Bar(Foo):
///     pass
/// ```
///
/// ## References
/// - [Python documentation: Class definitions](https://docs.python.org/3/reference/compound_stmts.html#class-definitions)
#[derive(ViolationMetadata)]
pub(crate) struct DuplicateBases {
    base: String,
    class: String,
}

impl Violation for DuplicateBases {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateBases { base, class } = self;
        format!("Duplicate base `{base}` for class `{class}`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove duplicate base".to_string())
    }
}

/// PLE0241
pub(crate) fn duplicate_bases(checker: &Checker, name: &str, arguments: Option<&Arguments>) {
    let Some(arguments) = arguments else {
        return;
    };
    let bases = &arguments.args;

    let mut seen: FxHashSet<&str> = FxHashSet::with_capacity_and_hasher(bases.len(), FxBuildHasher);
    for base in bases {
        if let Expr::Name(ast::ExprName { id, .. }) = base {
            if !seen.insert(id) {
                let mut diagnostic = Diagnostic::new(
                    DuplicateBases {
                        base: id.to_string(),
                        class: name.to_string(),
                    },
                    base.range(),
                );
                diagnostic.try_set_fix(|| {
                    remove_argument(
                        base,
                        arguments,
                        Parentheses::Remove,
                        checker.locator().contents(),
                    )
                    .map(Fix::safe_edit)
                });
                checker.report_diagnostic(diagnostic);
            }
        }
    }
}
