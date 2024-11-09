use crate::Locator;
use regex::Regex;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::CommentRanges;
use ruff_text_size::TextRange;
use ruff_text_size::TextSize;
use std::sync::LazyLock;

type EndIndex = usize;

#[derive(Debug, Eq, PartialEq)]
enum SpecialComment {
    /// `# noqa`, `# noqa: A123, B456`
    Noqa(Option<Vec<String>>),
    /// `# ruff: noqa`, `# ruff: noqa: A123, B456`,
    /// `# flake8: noqa`, `# flake8: noqa: A123, B456`
    FileLevelNoqa {
        hint: String,
        codes: Option<Vec<String>>,
    },

    /// `# fmt: on`, `# fmt: off`, `fmt: skip`
    Fmt(String),
    /// `# isort: skip_file`
    Isort(String),
    /// `# mypy: ignore-errors`
    Mypy(String),
    /// `# nopycln: import`
    Nopycln(String),
    /// `# pyright: strict`, `# pyright: ignore[reportFoo]`
    Pyright(String),
    /// `# ruff: isort: skip_file`
    RuffIsort(String),
    /// `# type: int`, `# type: ignore`
    Type(String),
}

impl SpecialComment {
    fn formatted(&self) -> Option<String> {
        match self {
            SpecialComment::Noqa(None) => Some("# noqa".to_string()),

            // Avoid suggesting the unsafe fix
            // `# noqa:` (ignore nothing) -> `# noqa` (ignore everything).
            SpecialComment::Noqa(Some(codes)) if codes.is_empty() => None,

            SpecialComment::Noqa(Some(codes)) => Some(format!("# noqa: {}", codes.join(", "))),

            SpecialComment::FileLevelNoqa { hint, codes: None } => Some(format!("# {hint}: noqa")),

            // Avoid suggesting the unsafe fix
            // `# ruff: noqa:` (ignore nothing) -> `# ruff: noqa` (ignore everything).
            SpecialComment::FileLevelNoqa {
                codes: Some(codes), ..
            } if codes.is_empty() => None,

            SpecialComment::FileLevelNoqa {
                hint,
                codes: Some(codes),
            } => Some(format!("# {hint}: noqa: {}", codes.join(", "))),

            SpecialComment::Nopycln(rest) => Some(format!("# nopycln: {}", rest.to_lowercase())),

            SpecialComment::Fmt(rest) => Some(format!("# fmt: {rest}")),
            SpecialComment::Isort(rest) => Some(format!("# isort: {rest}")),
            SpecialComment::Mypy(rest) => Some(format!("# mypy: {rest}")),
            SpecialComment::Pyright(rest) => Some(format!("# pyright: {rest}")),
            SpecialComment::RuffIsort(rest) => Some(format!("# ruff: isort: {rest}")),
            SpecialComment::Type(rest) => Some(format!("# type: {rest}")),
        }
    }
}

/// ## What it does
/// Checks special comments' formatting.
///
/// ## Why is this bad?
/// Special comments are often written in the following format
/// (hash, space, directive, colon, space, directive body):
///
/// ```python
/// # name: body
/// ```
///
/// ## Example
///
/// ```python
/// # ruff: noqa:A123 B456
/// # fmt:off
/// #type:ignore
/// ```
///
/// Use instead:
///
/// ```python
/// # ruff: noqa: A123, B456
/// # fmt: off
/// # type: ignore
/// ```
#[violation]
pub(crate) struct UnformattedSpecialComment(SpecialComment);

impl AlwaysFixableViolation for UnformattedSpecialComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unformatted special comment".to_string()
    }

    fn fix_title(&self) -> String {
        "Format comment".to_string()
    }
}

fn add_diagnostic_if_applicable(
    diagnostics: &mut Vec<Diagnostic>,
    comment: SpecialComment,
    original_comment_text: &str,
    original_comment_text_range: TextRange,
) {
    let Some(formatted_comment_text) = comment.formatted() else {
        return;
    };

    if original_comment_text == formatted_comment_text {
        return;
    }

    let edit = Edit::range_replacement(formatted_comment_text, original_comment_text_range);
    let fix = Fix::safe_edit(edit);

    let violation = UnformattedSpecialComment(comment);
    let diagnostic = Diagnostic::new(violation, original_comment_text_range).with_fix(fix);

    diagnostics.push(diagnostic);
}

fn parse_code_list(code_list: &str) -> Vec<String> {
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Z]+[A-Za-z0-9]+").unwrap());

    PATTERN
        .find_iter(code_list)
        .map(|code| code.as_str().to_owned())
        .collect()
}

macro_rules! try_parse_common {
    ($pattern:ident, $text:ident, $special_comment:expr) => {{
        let result = $pattern.captures($text)?;

        let end_index = result.get(0).unwrap().end();
        let rest = result.name("rest").unwrap().as_str().to_owned();

        Some((end_index, $special_comment(rest)))
    }};
}

fn try_parse_noqa(text: &str) -> Option<(EndIndex, SpecialComment)> {
    // ruff_linter::noqa::Directive::try_extract
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?x)
            ^
            \#\s*
            (?i:noqa)
            (?:
                :\s*
                (?<code_list>
                    [A-Z]+[A-Za-z0-9]+
                    (?:[\s,]+[A-Z]+[A-Za-z0-9]+)*
                )?
            )?
            $
            ",
        )
        .unwrap()
    });

    let result = PATTERN.captures(text)?;

    let end_index = result.get(0).unwrap().end();
    let codes = result
        .name("code_list")
        .map(|it| parse_code_list(it.as_str()));

    Some((end_index, SpecialComment::Noqa(codes)))
}

fn try_parse_file_level_noqa(text: &str) -> Option<(EndIndex, SpecialComment)> {
    // ruff_linter::noqa::ParsedFileExemption::try_extract
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?x)
            ^
            \#\s*
            (?<hint>flake8|ruff)\s*:\s*
            (?i:noqa)\s*
            (?:
                :\s*
                (?<code_list>
                    [A-Z]+[A-Za-z0-9]+
                    (?:[\s,]\s*[A-Z]+[A-Za-z0-9]+)*
                )?
                \s*
            )?
            $
            ",
        )
        .unwrap()
    });

    let result = PATTERN.captures(text)?;

    let end_index = result.get(0).unwrap().end();
    let hint = result.name("hint").unwrap().as_str().to_owned();
    let codes = result
        .name("code_list")
        .map(|it| parse_code_list(it.as_str()));

    Some((end_index, SpecialComment::FileLevelNoqa { hint, codes }))
}

fn try_parse_fmt(text: &str) -> Option<(EndIndex, SpecialComment)> {
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^#\s*fmt:\s*(?<rest>on|off|skip)").unwrap());

    try_parse_common!(PATTERN, text, SpecialComment::Fmt)
}

fn try_parse_isort(text: &str) -> Option<(EndIndex, SpecialComment)> {
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^# isort:(?<rest>skip_file|skip)").unwrap());

    try_parse_common!(PATTERN, text, SpecialComment::Isort)
}

fn try_parse_mypy(text: &str) -> Option<(EndIndex, SpecialComment)> {
    // https://github.com/python/mypy/blob/3b00002acd/mypy/util.py#L228
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^# mypy:\s*(?<rest>\S+)").unwrap());

    try_parse_common!(PATTERN, text, SpecialComment::Mypy)
}

fn try_parse_nopycln(text: &str) -> Option<(EndIndex, SpecialComment)> {
    // https://github.com/hadialqattan/pycln/blob/d0aeb62860/pycln/utils/regexu.py#L18-L19
    // https://github.com/hadialqattan/pycln/blob/d0aeb62860/pycln/utils/regexu.py#L127
    // https://github.com/hadialqattan/pycln/blob/d0aeb62860/pycln/utils/regexu.py#L136
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^#\s*nopycln\s*:\s*(?<rest>file|import)").unwrap());

    try_parse_common!(PATTERN, text, SpecialComment::Nopycln)
}

fn try_parse_pyright(text: &str) -> Option<(EndIndex, SpecialComment)> {
    // https://github.com/microsoft/pyright/blob/9d60c434c4/packages/pyright-internal/src/parser/tokenizer.ts#L1314
    // https://github.com/microsoft/pyright/blob/9d60c434c4/packages/pyright-internal/src/analyzer/commentUtils.ts#L138
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^#\s*pyright:\s*(?<rest>\S+)").unwrap());

    try_parse_common!(PATTERN, text, SpecialComment::Pyright)
}

fn try_parse_ruff_isort(text: &str) -> Option<(EndIndex, SpecialComment)> {
    // ruff_linter::directives::extract_isort_directives
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^# ruff: isort: ?(?<rest>skip_file|skip)").unwrap());

    try_parse_common!(PATTERN, text, SpecialComment::RuffIsort)
}

fn try_parse_type(text: &str) -> Option<(EndIndex, SpecialComment)> {
    // https://github.com/python/cpython/blob/c222441fa7/Parser/lexer/lexer.c#L45-L47
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^#\s*type:\s*(?<rest>\S+)").unwrap());

    try_parse_common!(PATTERN, text, SpecialComment::Type)
}

macro_rules! parse_and_handle_comment {
    ($parse:ident, $text:ident, $diagnostics:ident, $absolute_start_index:ident) => {
        if let Some((relative_end_index, comment)) = $parse($text) {
            let comment_text = &$text[..relative_end_index];
            let absolute_end_index = $absolute_start_index + relative_end_index;

            let Ok(start) = TryInto::<TextSize>::try_into($absolute_start_index) else {
                return;
            };
            let Ok(end) = TryInto::<TextSize>::try_into(absolute_end_index) else {
                return;
            };
            let range = TextRange::new(start, end);

            add_diagnostic_if_applicable($diagnostics, comment, comment_text, range);
            return;
        }
    };
}

fn check_single_comment(diagnostics: &mut Vec<Diagnostic>, text: &str, start_index: usize) {
    parse_and_handle_comment!(try_parse_noqa, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_file_level_noqa, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_fmt, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_isort, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_mypy, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_nopycln, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_pyright, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_ruff_isort, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_type, text, diagnostics, start_index);
}

/// RUF104
pub(crate) fn unformatted_special_comment(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    comment_ranges: &CommentRanges,
) {
    for range in comment_ranges {
        let text = locator.slice(range);
        let range_start: usize = range.start().into();

        for (char_index, char) in text.char_indices() {
            let next_char = text[char_index..].chars().next();

            if char != '#' || matches!(next_char, Some('#')) {
                continue;
            }

            let absolute_start_index = range_start + char_index;

            check_single_comment(diagnostics, &text[char_index..], absolute_start_index);
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_diagnostics::Diagnostic;

    use super::check_single_comment;

    fn test(text: &str) -> Vec<Diagnostic> {
        let mut diagnostics = vec![];
        let start_index = 0;

        check_single_comment(&mut diagnostics, text, start_index);

        diagnostics
    }

    fn has_unformatted(text: &str) {
        let diagnostics = test(text);

        assert!(!diagnostics.is_empty());
    }

    fn no_unformatted(text: &str) {
        let diagnostics = test(text);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn unknown() {
        no_unformatted("#foo");
        no_unformatted("# foo");

        no_unformatted("#ruff: foo-bar");
        no_unformatted("# flake8:foo-bar");

        no_unformatted("#  black:skip");
    }

    #[test]
    fn incorrect_casing() {
        no_unformatted("# FMT:OFF");
        no_unformatted("# isort: On");
        no_unformatted("# Mypy: disallow-subclassing-any");
        no_unformatted("# PyRight: basic");
        no_unformatted("# Type: ignore");
    }

    #[test]
    fn already_formatted() {
        no_unformatted("# noqa");
        no_unformatted("# noqa: A123");
        no_unformatted("# noqa: A123, B456");

        no_unformatted("# ruff: noqa");
        no_unformatted("# ruff: noqa: A123");
        no_unformatted("# ruff: noqa: A123, B456");

        no_unformatted("# flake8: noqa");
        no_unformatted("# flake8: noqa: A123");
        no_unformatted("# flake8: noqa: A123, B456");

        no_unformatted("# fmt: on");
        no_unformatted("# fmt: off");
        no_unformatted("# fmt: skip");

        no_unformatted("# isort: on");
        no_unformatted("# isort: off");
        no_unformatted("# isort: split");
        no_unformatted("# isort: skip");
        no_unformatted("# isort: skip_file");

        no_unformatted("# mypy: enable-error-codes=");

        no_unformatted("# nopycln: file");
        no_unformatted("# nopycln: import");

        no_unformatted("# pyright: basic");
        no_unformatted("# pyright: standard");
        no_unformatted("# pyright: strict");
        no_unformatted("# pyright: ignore");
        no_unformatted("# pyright: ignore [reportFoo]");

        no_unformatted("# ruff: isort: on");
        no_unformatted("# ruff: isort: skip_file");

        no_unformatted("# type: ignore");
        no_unformatted("# type: int");
        no_unformatted("# type: list[str]");
    }

    #[test]
    fn whitespace() {
        has_unformatted("# noqa:A123");
        has_unformatted("#noqa:   A123");

        has_unformatted("#    type:ignore");
        has_unformatted("#type:\tint");

        has_unformatted("# fmt:off");
        has_unformatted("#fmt: on");
        has_unformatted("# \t fmt:\t skip");

        has_unformatted("# isort:skip");
        has_unformatted("# isort:skip_file");

        has_unformatted("# mypy:  disallow-subclassing-any");

        has_unformatted("#   nopycln: \t \t\tfile");
        has_unformatted("# \tnopycln:\t   import");

        has_unformatted("#pyright:ignore[]");
        has_unformatted("#\t\t\tpyright:    ignore[]");

        has_unformatted("# ruff: isort:skip");
        has_unformatted("# ruff: isort:skip_file");

        has_unformatted("#    type:\t\t\tignore");
        has_unformatted("#\t \t \ttype:\t\t \tint");
    }

    #[test]
    fn casing() {
        has_unformatted("# NoQA: A123, B456");
        has_unformatted("# ruff: NoQA: A123, B456");
        has_unformatted("# flake8: NoQA: A123, B456");

        has_unformatted("# NoPyCLN: File");
        has_unformatted("# NoPycln: Import");
        has_unformatted("# nOpYcLn: iMpOrT");
    }

    #[test]
    fn rule_code_separators() {
        has_unformatted("# noqa: A123 B456");
        has_unformatted("# ruff: noqa: A123 B456");
        has_unformatted("# flake8: noqa: A123 B456");

        has_unformatted("# noqa: A123,B456");
        has_unformatted("# ruff: noqa: A123,B456");
        has_unformatted("# flake8: noqa: A123,B456");

        has_unformatted("# noqa: A123,,B456");
        has_unformatted("# noqa: A123 , \t,\t \tB456");
        has_unformatted("# noqa: A123\tB456");
        has_unformatted("# noqa: A123\t\t\tB456");
        has_unformatted("# noqa: A123\t\t\t\tB456");

        has_unformatted("# noqa: A123 ,B456");
        has_unformatted("# ruff: noqa: A123\tB456");
        has_unformatted("# flake8: noqa: A123   B456");
    }

    #[test]
    fn composite() {
        has_unformatted("# type: ignore  # noqa:A123");
        has_unformatted("# noqa:A123 - Lorem ipsum dolor sit amet");
    }
}
