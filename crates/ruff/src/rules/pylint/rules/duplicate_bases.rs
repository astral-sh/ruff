use std::hash::BuildHasherDefault;

use ruff_python_ast::{self as ast, Arguments, Expr};
use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

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
#[violation]
pub struct DuplicateBases {
    base: String,
    class: String,
}

impl Violation for DuplicateBases {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateBases { base, class } = self;
        format!("Duplicate base `{base}` for class `{class}`")
    }
}

/// PLE0241
pub(crate) fn duplicate_bases(checker: &mut Checker, name: &str, arguments: Option<&Arguments>) {
    let Some(Arguments { args: bases, .. }) = arguments else {
        return;
    };

    let mut seen: FxHashSet<&str> =
        FxHashSet::with_capacity_and_hasher(bases.len(), BuildHasherDefault::default());
    for base in bases {
        if let Expr::Name(ast::ExprName { id, .. }) = base {
            if !seen.insert(id) {
                checker.diagnostics.push(Diagnostic::new(
                    DuplicateBases {
                        base: id.to_string(),
                        class: name.to_string(),
                    },
                    base.range(),
                ));
            }
        }
    }
}
