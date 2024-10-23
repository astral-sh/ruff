use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::identifier::Identifier;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Checks for classes that inherit from both `str` and `enum.Enum`.
///
/// ## Why is this bad?
/// Python 3.11 introduced `enum.StrEnum`, which is preferred over inheriting
/// from both `str` and `enum.Enum`.
///
/// ## Example
///
/// ```python
/// import enum
///
///
/// class Foo(str, enum.Enum): ...
/// ```
///
/// Use instead:
///
/// ```python
/// import enum
///
///
/// class Foo(enum.StrEnum): ...
/// ```
///
/// ## Fix safety
///
/// Python 3.11 introduced a [breaking change] for enums that inherit from both
/// `str` and `enum.Enum`. Consider the following enum:
///
/// ```python
/// from enum import Enum
///
///
/// class Foo(str, Enum):
///     BAR = "bar"
/// ```
///
/// In Python 3.11, the formatted representation of `Foo.BAR` changed as
/// follows:
///
/// ```python
/// # Python 3.10
/// f"{Foo.BAR}"  # > bar
/// # Python 3.11
/// f"{Foo.BAR}"  # > Foo.BAR
/// ```
///
/// Migrating from `str` and `enum.Enum` to `enum.StrEnum` will restore the
/// previous behavior, such that:
///
/// ```python
/// from enum import StrEnum
///
///
/// class Foo(StrEnum):
///     BAR = "bar"
///
///
/// f"{Foo.BAR}"  # > bar
/// ```
///
/// As such, migrating to `enum.StrEnum` will introduce a behavior change for
/// code that relies on the Python 3.11 behavior.
///
/// ## References
/// - [enum.StrEnum](https://docs.python.org/3/library/enum.html#enum.StrEnum)
///
/// [breaking change]: https://blog.pecar.me/python-enum

#[violation]
pub struct ReplaceStrEnum {
    name: String,
}

impl Violation for ReplaceStrEnum {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ReplaceStrEnum { name } = self;
        format!("Class {name} inherits from both `str` and `enum.Enum`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Inherit from `enum.StrEnum`".to_string())
    }
}

/// UP042
pub(crate) fn replace_str_enum(checker: &mut Checker, class_def: &ast::StmtClassDef) {
    let Some(arguments) = class_def.arguments.as_deref() else {
        // class does not inherit anything, exit early
        return;
    };

    // Determine whether the class inherits from both `str` and `enum.Enum`.
    let mut inherits_str = false;
    let mut inherits_enum = false;
    for base in &*arguments.args {
        if let Some(qualified_name) = checker.semantic().resolve_qualified_name(base) {
            match qualified_name.segments() {
                ["" | "builtins", "str"] => inherits_str = true,
                ["enum", "Enum"] => inherits_enum = true,
                _ => {}
            }
        }

        // Short-circuit if both `str` and `enum.Enum` are found.
        if inherits_str && inherits_enum {
            break;
        }
    }

    // If the class does not inherit both `str` and `enum.Enum`, exit early.
    if !inherits_str || !inherits_enum {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        ReplaceStrEnum {
            name: class_def.name.to_string(),
        },
        class_def.identifier(),
    );

    // If the base classes are _exactly_ `str` and `enum.Enum`, apply a fix.
    // TODO(charlie): As an alternative, we could remove both arguments, and replace one of the two
    // with `StrEnum`. However, `remove_argument` can't be applied multiple times within a single
    // fix; doing so leads to a syntax error.
    if arguments.len() == 2 {
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import("enum", "StrEnum"),
                class_def.start(),
                checker.semantic(),
            )?;
            Ok(Fix::unsafe_edits(
                import_edit,
                [Edit::range_replacement(
                    format!("({binding})"),
                    arguments.range(),
                )],
            ))
        });
    }

    checker.diagnostics.push(diagnostic);
}
