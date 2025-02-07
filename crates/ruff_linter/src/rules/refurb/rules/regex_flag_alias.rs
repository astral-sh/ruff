use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
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
#[derive(ViolationMetadata)]
pub(crate) struct RegexFlagAlias {
    flag: RegexFlag,
}

impl AlwaysFixableViolation for RegexFlagAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RegexFlagAlias { flag } = self;
        format!("Use of regular expression alias `re.{}`", flag.alias())
    }

    fn fix_title(&self) -> String {
        let RegexFlagAlias { flag } = self;
        format!("Replace with `re.{}`", flag.full_name())
    }
}

/// FURB167
pub(crate) fn regex_flag_alias(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::RE) {
        return;
    }

    let Some(flag) = checker
        .semantic()
        .resolve_qualified_name(expr)
        .and_then(|qualified_name| match qualified_name.segments() {
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

    let mut diagnostic = Diagnostic::new(RegexFlagAlias { flag }, expr.range());
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
    checker.report_diagnostic(diagnostic);
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
    #[must_use]
    const fn alias(self) -> &'static str {
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

    #[must_use]
    const fn full_name(self) -> &'static str {
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
