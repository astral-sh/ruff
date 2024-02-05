use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Checks for the use of shorthand aliases for regular expression flags
/// (e.g., `re.I` instead of `re.IGNORECASE`).
///
/// ## Why is this bad?
/// The regular expression module provides descriptive names for each flag,
/// along with single-letter aliases. Prefer the descriptive names, as they
/// are more readable and self-documenting.
///
/// ## Example
/// ```python
/// import re
///
/// if re.match("^hello", "hello world", re.I):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import re
///
/// if re.match("^hello", "hello world", re.IGNORECASE):
///     ...
/// ```
///
#[violation]
pub struct RegexFlagAlias {
    alias: &'static str,
    full_name: &'static str,
}

impl AlwaysFixableViolation for RegexFlagAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RegexFlagAlias { alias, .. } = self;
        format!("Use of regular expression alias `re.{alias}`")
    }

    fn fix_title(&self) -> String {
        let RegexFlagAlias { full_name, .. } = self;
        format!("Replace with `re.{full_name}`")
    }
}

/// FURB167
pub(crate) fn regex_flag_alias(checker: &mut Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::RE) {
        return;
    }

    let Some(flag) =
        checker
            .semantic()
            .resolve_call_path(expr)
            .and_then(|call_path| match call_path.as_slice() {
                ["re", "A"] => Some(RegexFlag::Ascii),
                ["re", "I"] => Some(RegexFlag::IgnoreCase),
                ["re", "L"] => Some(RegexFlag::Locale),
                ["re", "M"] => Some(RegexFlag::Multiline),
                ["re", "S"] => Some(RegexFlag::DotAll),
                ["re", "T"] => Some(RegexFlag::Template),
                ["re", "U"] => Some(RegexFlag::Unicode),
                ["re", "X"] => Some(RegexFlag::Verbose),
                _ => None,
            })
    else {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        RegexFlagAlias {
            alias: flag.alias(),
            full_name: flag.full_name(),
        },
        expr.range(),
    );
    diagnostic.try_set_fix(|| {
        let (edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("re", flag.full_name()),
            expr.start(),
            checker.semantic(),
        )?;
        Ok(Fix::safe_edits(
            Edit::range_replacement(binding, expr.range()),
            [edit],
        ))
    });
    checker.diagnostics.push(diagnostic);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegexFlag {
    Ascii,
    IgnoreCase,
    Locale,
    Multiline,
    DotAll,
    Template,
    Unicode,
    Verbose,
}

impl RegexFlag {
    fn alias(self) -> &'static str {
        match self {
            Self::Ascii => "A",
            Self::IgnoreCase => "I",
            Self::Locale => "L",
            Self::Multiline => "M",
            Self::DotAll => "S",
            Self::Template => "T",
            Self::Unicode => "U",
            Self::Verbose => "X",
        }
    }

    fn full_name(self) -> &'static str {
        match self {
            Self::Ascii => "ASCII",
            Self::IgnoreCase => "IGNORECASE",
            Self::Locale => "LOCALE",
            Self::Multiline => "MULTILINE",
            Self::DotAll => "DOTALL",
            Self::Template => "TEMPLATE",
            Self::Unicode => "UNICODE",
            Self::Verbose => "VERBOSE",
        }
    }
}
