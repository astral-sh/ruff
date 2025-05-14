use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{helpers::map_subscript, Arguments, StmtClassDef};
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, importer::ImportRequest};

/// ## What it does
/// Checks for subclasses of `dict`, `list` or `str`.
///
/// ## Why is this bad?
/// Built-in types don't consistently use their own dunder methods. For example,
/// `dict.__init__` and `dict.update()` bypass `__setitem__`, making inheritance unreliable.
///
/// Use the `UserDict`, `UserList`, and `UserString` objects from the `collections` module
/// instead.
///
/// ## Example
///
/// ```python
/// class UppercaseDict(dict):
///     def __setitem__(self, key, value):
///         super().__setitem__(key.upper(), value)
///
///
/// d = UppercaseDict({"a": 1, "b": 2})  # Bypasses __setitem__
/// print(d)  # {'a': 1, 'b': 2}
/// ```
///
/// Use instead:
///
/// ```python
/// from collections import UserDict
///
///
/// class UppercaseDict(UserDict):
///     def __setitem__(self, key, value):
///         super().__setitem__(key.upper(), value)
///
///
/// d = UppercaseDict({"a": 1, "b": 2})  # Uses __setitem__
/// print(d)  # {'A': 1, 'B': 2}
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
#[derive(ViolationMetadata)]
pub(crate) struct SubclassBuiltin {
    subclass: String,
    replacement: String,
}

impl AlwaysFixableViolation for SubclassBuiltin {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SubclassBuiltin {
            subclass,
            replacement,
        } = self;
        format!(
            "Subclassing `{subclass}` can be error prone, use `collections.{replacement}` instead"
        )
    }

    fn fix_title(&self) -> String {
        let SubclassBuiltin { replacement, .. } = self;
        format!("Replace with `collections.{replacement}`")
    }
}

/// FURB189
pub(crate) fn subclass_builtin(checker: &Checker, class: &StmtClassDef) {
    let Some(Arguments { args: bases, .. }) = class.arguments.as_deref() else {
        return;
    };

    // Expect only one base class else return
    let [base] = &**bases else {
        return;
    };

    // Check if the base class is a subscript expression so that only the name expr
    // is checked and modified.
    let base_expr = map_subscript(base);

    let Some(symbol) = checker.semantic().resolve_builtin_symbol(base_expr) else {
        return;
    };

    let Some(supported_builtin) = SupportedBuiltins::from_symbol(symbol) else {
        return;
    };

    let user_symbol = supported_builtin.user_symbol();

    let mut diagnostic = Diagnostic::new(
        SubclassBuiltin {
            subclass: symbol.to_string(),
            replacement: user_symbol.to_string(),
        },
        base_expr.range(),
    );
    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import_from("collections", user_symbol),
            base.start(),
            checker.semantic(),
        )?;
        let other_edit = Edit::range_replacement(binding, base_expr.range());
        Ok(Fix::unsafe_edits(import_edit, [other_edit]))
    });
    checker.report_diagnostic(diagnostic);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum SupportedBuiltins {
    Str,
    List,
    Dict,
}

impl SupportedBuiltins {
    fn from_symbol(value: &str) -> Option<SupportedBuiltins> {
        match value {
            "str" => Some(Self::Str),
            "dict" => Some(Self::Dict),
            "list" => Some(Self::List),
            _ => None,
        }
    }

    const fn user_symbol(self) -> &'static str {
        match self {
            SupportedBuiltins::Dict => "UserDict",
            SupportedBuiltins::List => "UserList",
            SupportedBuiltins::Str => "UserString",
        }
    }
}
