use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for classes that inherit from both `str` and `enum.Enum`.
///
/// ## Why is this bad?
/// Since Python 3.11, `enum.StrEnum` exists and is preferred over
/// inheriting from `str` and `enum.Enum`.
///
/// ## Example
/// ```python
/// class Foo(str, enum.Enum):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// class Foo(enum.StrEnum):
///     ...
/// ```
///
/// ## References
/// - [enum.StrEnum](https://docs.python.org/3/library/enum.html#enum.StrEnum)

#[violation]
pub struct ReplaceStrEnum {
    name: String,
    args_len: usize,
}

impl Violation for ReplaceStrEnum {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ReplaceStrEnum { name, .. } = self;
        format!(
            "Class {name} inherits from both `str` and `enum.Enum`. Prefer `enum.StrEnum` instead."
        )
    }

    fn fix_title(&self) -> Option<String> {
        let ReplaceStrEnum { args_len, .. } = self;

        if *args_len == 2 {
            Some("Replace `str` and `enum.Enum` with `enum.StrEnum`".to_string())
        } else {
            None
        }
    }
}

/// UP042
pub(crate) fn replace_str_enum(checker: &mut Checker, class_def: &ast::StmtClassDef) {
    let Some(arguments) = class_def.arguments.as_deref() else {
        // class does not inherit anything, exit early
        return;
    };

    let mut inherits_str = false;
    let mut inherits_enum = false;
    for base in arguments.args.iter() {
        if let Some(qualified_name) = checker.semantic().resolve_qualified_name(base) {
            if matches!(qualified_name.segments(), ["", "str"]) {
                inherits_str = true;
            } else if matches!(qualified_name.segments(), ["enum", "Enum"]) {
                inherits_enum = true;
            }
        }

        if inherits_str && inherits_enum {
            // no need to check other inherited classes, we found both str & enum.Enum
            break;
        }
    }

    if !inherits_str || !inherits_enum {
        // exit early if class does not inherit both str and enum.Enum
        return;
    };

    let mut diagnostic = Diagnostic::new(
        ReplaceStrEnum {
            name: class_def.name.to_string(),
            args_len: arguments.len(),
        },
        class_def.range(),
    );

    if arguments.len() == 2 {
        // a fix is available only for classes that inherit exactly 2 arguments: str, Enum,
        // because `remove_argument` cannot be called multiple times consecutively...
        // for classes that inherit str, Enum and something else, generate a warning.
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import("enum", "StrEnum"),
                class_def.start(),
                checker.semantic(),
            )?;

            // `binding` here is `StrEnum`.

            // class inherits exactly 2 arguments.
            // replace all `(str, Enum)` arguments with `(StrEnum)`.
            let fix = Fix::unsafe_edits(
                import_edit,
                [Edit::range_replacement(
                    format!("({binding})"),
                    arguments.range(),
                )],
            );

            Ok(fix)
        });
    }

    checker.diagnostics.push(diagnostic);
}
