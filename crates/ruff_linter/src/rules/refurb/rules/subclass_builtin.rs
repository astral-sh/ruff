use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Arguments;
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, importer::ImportRequest};

/// ## What it does
/// Checks for subclasses of `dict`, `list` or `str`.
///
/// ## Why is this bad?
/// Subclassing `dict`, `list`, or `str` objects can be error prone, use the
/// `UserDict`, `UserList`, and `UserStr` objects from the `collections` module
/// instead.
///
/// ## Example
/// ```python
/// class CaseInsensitiveDict(dict): ...
/// ```
///
/// Use instead:
/// ```python
/// from collections import UserDict
///
///
/// class CaseInsensitiveDict(UserDict): ...
/// ```
///
/// ## Fix safety
/// This fix is marked as unsafe because `isinstance()` checks for `dict`,
/// `list`, and `str` types will fail when using the corresponding User class.
/// If you need to pass custom `dict` or `list` objects to code you don't
/// control, ignore this check. If you do control the code, consider using
/// the following type checks instead:
///
/// * `dict` -> `collections.abc.MutableMapping`
/// * `list` -> `collections.abc.MutableSequence`
/// * `str` -> No such conversion exists
///
/// ## References
///
/// - [Python documentation: `collections`](https://docs.python.org/3/library/collections.html)
#[violation]
pub struct SubclassBuiltin {
    subclass: String,
    replacement: String,
}

impl AlwaysFixableViolation for SubclassBuiltin {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SubclassBuiltin { subclass, .. } = self;
        format!("Subclass of `{subclass}`")
    }

    fn fix_title(&self) -> String {
        let SubclassBuiltin {
            subclass,
            replacement,
        } = self;
        format!("Replace subclass `{subclass}` with `{replacement}`")
    }
}

enum Builtins {
    Str,
    List,
    Dict,
}

/// FURB189
pub(crate) fn subclass_builtin(checker: &mut Checker, arguments: Option<&Arguments>) {
    let Some(Arguments { args, .. }) = arguments else {
        return;
    };

    if args.len() == 0 {
        return;
    }

    for base in args {
        for symbol_type in [Builtins::Dict, Builtins::Str, Builtins::List] {
            let symbol = match symbol_type {
                Builtins::Dict => "dict",
                Builtins::List => "list",
                Builtins::Str => "str",
            };
            if checker.semantic().match_builtin_expr(base, symbol) {
                let replacement_symbol = match symbol_type {
                    Builtins::Dict => "UserDict",
                    Builtins::List => "UserList",
                    Builtins::Str => "UserStr",
                };

                let mut diagnostic = Diagnostic::new(
                    SubclassBuiltin {
                        subclass: symbol.to_string(),
                        replacement: replacement_symbol.to_string(),
                    },
                    base.range(),
                );
                diagnostic.try_set_fix(|| {
                    let (import_edit, binding) = checker.importer().get_or_import_symbol(
                        &ImportRequest::import_from("collections", replacement_symbol),
                        base.start(),
                        checker.semantic(),
                    )?;
                    let other_edit = Edit::range_replacement(binding, base.range());
                    Ok(Fix::unsafe_edits(import_edit, [other_edit]))
                });
                checker.diagnostics.push(diagnostic);

                // inheritance of these builtins is mutually exclusive
                continue;
            }
        }
    }
}
