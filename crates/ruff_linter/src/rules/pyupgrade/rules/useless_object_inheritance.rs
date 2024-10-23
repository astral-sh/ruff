use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};

/// ## What it does
/// Checks for classes that inherit from `object`.
///
/// ## Why is this bad?
/// Since Python 3, all classes inherit from `object` by default, so `object` can
/// be omitted from the list of base classes.
///
/// ## Example
///
/// ```python
/// class Foo(object): ...
/// ```
///
/// Use instead:
///
/// ```python
/// class Foo: ...
/// ```
///
/// ## References
/// - [PEP 3115](https://www.python.org/dev/peps/pep-3115/)
#[violation]
pub struct UselessObjectInheritance {
    name: String,
}

impl AlwaysFixableViolation for UselessObjectInheritance {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UselessObjectInheritance { name } = self;
        format!("Class `{name}` inherits from `object`")
    }

    fn fix_title(&self) -> String {
        "Remove `object` inheritance".to_string()
    }
}

/// UP004
pub(crate) fn useless_object_inheritance(checker: &mut Checker, class_def: &ast::StmtClassDef) {
    let Some(arguments) = class_def.arguments.as_deref() else {
        return;
    };

    for base in &*arguments.args {
        if !checker.semantic().match_builtin_expr(base, "object") {
            continue;
        }

        let mut diagnostic = Diagnostic::new(
            UselessObjectInheritance {
                name: class_def.name.to_string(),
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
        checker.diagnostics.push(diagnostic);
    }
}
