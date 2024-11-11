use crate::Locator;
use regex::Regex;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::CommentRanges;
use ruff_text_size::TextRange;
use ruff_text_size::TextSize;
use std::ops::Range;
use std::sync::LazyLock;

type HintRelativeRange = Range<usize>;
type SpecialCommentDescriptor<'a> = Option<(HintRelativeRange, &'a str, SpecialComment)>;

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
    RuffIsort(String),
    /// `# type: int`, `# type: ignore`
    Type(String),
}

impl SpecialComment {
    fn formatted(&self) -> String {
        match self {
            SpecialComment::Noqa(None) => "# noqa".to_string(),

            SpecialComment::Noqa(Some(codes)) if codes.is_empty() => "# noqa:".to_string(),

            SpecialComment::Noqa(Some(codes)) => format!("# noqa: {}", codes.join(", ")),

            SpecialComment::FileLevelNoqa { hint, codes: None } => format!("# {hint}: noqa"),

            SpecialComment::FileLevelNoqa {
                hint,
                codes: Some(codes),
            } if codes.is_empty() => format!("# {hint}: noqa:"),

            SpecialComment::FileLevelNoqa {
                hint,
                codes: Some(codes),
            } => format!("# {hint}: noqa: {}", codes.join(", ")),

            SpecialComment::Fmt(rest) => format!("# fmt: {rest}"),
            SpecialComment::Isort(rest) => format!("# isort: {rest}"),
            SpecialComment::RuffIsort(rest) => format!("# ruff: isort: {rest}"),
            SpecialComment::Type(rest) => format!("# type: {rest}"),
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
    comment_text: &str,
    comment_range: TextRange,
    hint_range: TextRange,
) {
    let formatted = comment.formatted();

    if comment_text == formatted {
        return;
    }

    let edit = Edit::range_replacement(formatted, comment_range);
    let fix = Fix::safe_edit(edit);

    let violation = UnformattedSpecialComment(comment);
    let diagnostic = Diagnostic::new(violation, hint_range).with_fix(fix);

    diagnostics.push(diagnostic);
}

macro_rules! try_parse_common {
    ($pattern:ident, $text:ident, $special_comment:expr) => {{
        let result = $pattern.captures($text)?;

        let comment = result.get(0).unwrap();
        let hint = result.name("hint").unwrap();
        let rest = result.name("rest").unwrap();

        Some((
            hint.range(),
            comment.as_str(),
            $special_comment(rest.as_str().to_owned()),
        ))
    }};
}

fn try_parse_noqa(text: &str) -> SpecialCommentDescriptor {
    fn parse_code_list(code_list: &str) -> Vec<String> {
        static PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Z]+[0-9]+").unwrap());

        PATTERN
            .find_iter(code_list)
            .map(|code| code.as_str().to_owned())
            .collect()
    }

    // ruff_linter::noqa::Directive::try_extract
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?x)
            ^
            \#\s*
            (?<hint>(?i:noqa))
            (?<code_list>
                :
                (?:
                    \s*
                    [A-Z]+[0-9]+
                    (?:[\s,]+[A-Z]+[0-9]+)*
                )?
            )?
            ",
        )
        .unwrap()
    });

    let result = PATTERN.captures(text)?;

    let comment = result.get(0).unwrap();
    let hint = result.name("hint").unwrap();
    let codes = result
        .name("code_list")
        .map(|it| parse_code_list(it.as_str()));

    Some((hint.range(), comment.as_str(), SpecialComment::Noqa(codes)))
}

fn try_parse_file_level_noqa(text: &str) -> SpecialCommentDescriptor {
    fn parse_code_list(code_list: &str) -> Vec<String> {
        static PATTERN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"[A-Z]+[A-Za-z0-9]+").unwrap());

        PATTERN
            .find_iter(code_list)
            .map(|code| code.as_str().to_owned())
            .collect()
    }

    // ruff_linter::noqa::ParsedFileExemption::try_extract
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?x)
            ^
            \#\s*
            (?<hint>flake8|ruff)\s*:\s*
            (?i:noqa)\s*
            (?<code_list>
                :
                (?:
                    \s*
                    [A-Z]+[A-Za-z0-9]+
                    (?:[\s,]\s*[A-Z]+[A-Za-z0-9]+)*
                )?
            )?
            ",
        )
        .unwrap()
    });

    let result = PATTERN.captures(text)?;

    let comment = result.get(0).unwrap();
    let hint = result.name("hint").unwrap();
    let codes = result
        .name("code_list")
        .map(|it| parse_code_list(it.as_str()));

    let special_comment = SpecialComment::FileLevelNoqa {
        hint: hint.as_str().to_owned(),
        codes,
    };

    Some((hint.range(), comment.as_str(), special_comment))
}

fn try_parse_fmt(text: &str) -> SpecialCommentDescriptor {
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^#\s*(?<hint>fmt):\s*(?<rest>on|off|skip)").unwrap());

    try_parse_common!(PATTERN, text, SpecialComment::Fmt)
}

fn try_parse_isort(text: &str) -> SpecialCommentDescriptor {
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^# (?<hint>isort):(?<rest>skip_file|skip)").unwrap());

    try_parse_common!(PATTERN, text, SpecialComment::Isort)
}

fn try_parse_ruff_isort(text: &str) -> SpecialCommentDescriptor {
    // ruff_linter::directives::extract_isort_directives
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^# (?<hint>ruff): isort: ?(?<rest>skip_file|skip)").unwrap());

    try_parse_common!(PATTERN, text, SpecialComment::RuffIsort)
}

fn try_parse_type(text: &str) -> SpecialCommentDescriptor {
    // https://github.com/python/cpython/blob/c222441fa7/Parser/lexer/lexer.c#L45-L47
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^#\s*(?<hint>type):\s*(?<rest>\S)").unwrap());

    try_parse_common!(PATTERN, text, SpecialComment::Type)
}

fn text_range(start: usize, end: usize) -> Option<TextRange> {
    let Ok(start) = TextSize::try_from(start) else {
        return None;
    };
    let Ok(end) = TextSize::try_from(end) else {
        return None;
    };

    Some(TextRange::new(start, end))
}

macro_rules! parse_and_handle_comment {
    ($parse:ident, $text:ident, $diagnostics:ident, $comment_start:ident) => {
        if let Some((hint_relative_range, comment_text, comment)) = $parse($text) {
            let comment_end = $comment_start + comment_text.len();
            let Some(comment_range) = text_range($comment_start, comment_end) else {
                return;
            };

            let hint_start = $comment_start + hint_relative_range.start;
            let hint_end = $comment_start + hint_relative_range.end;
            let Some(hint_range) = text_range(hint_start, hint_end) else {
                return;
            };

            add_diagnostic_if_applicable(
                $diagnostics,
                comment,
                comment_text,
                comment_range,
                hint_range,
            );
            return;
        }
    };
}

fn check_single_comment(diagnostics: &mut Vec<Diagnostic>, text: &str, start_index: usize) {
    parse_and_handle_comment!(try_parse_noqa, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_file_level_noqa, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_fmt, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_isort, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_ruff_isort, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_type, text, diagnostics, start_index);
}

fn check_composite_comment(diagnostics: &mut Vec<Diagnostic>, text: &str, range_start: usize) {
    for (char_index, char) in text.char_indices() {
        let next_char = text[char_index..].chars().nth(1);

        if char == '#' && !matches!(next_char, Some('#')) {
            let subcomment_absolute_index = range_start + char_index;

            check_single_comment(diagnostics, &text[char_index..], subcomment_absolute_index);
        }
    }
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

        check_composite_comment(diagnostics, text, range_start);
    }
}

#[cfg(test)]
mod tests {
    use ruff_diagnostics::Diagnostic;

    use super::{check_composite_comment, check_single_comment};

    fn check_single(text: &str) -> Vec<Diagnostic> {
        let mut diagnostics = vec![];
        let start_index = 0;

        check_single_comment(&mut diagnostics, text, start_index);

        diagnostics
    }

    fn check_composite(text: &str) -> Vec<Diagnostic> {
        let mut diagnostics = vec![];
        let start_index = 0;

        check_composite_comment(&mut diagnostics, text, start_index);

        diagnostics
    }

    fn has_unformatted(text: &str) {
        let diagnostics = check_single(text);

        assert_eq!(diagnostics.len(), 1);
    }

    fn no_unformatted(text: &str) {
        let diagnostics = check_single(text);

        assert!(diagnostics.is_empty());
    }

    fn composite_has_unformatted(text: &str, count: usize) {
        let diagnostics = check_composite(text);

        assert_eq!(diagnostics.len(), count);
    }

    fn composite_no_unformatted(text: &str) {
        composite_has_unformatted(text, 0);
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

        no_unformatted("# nopycln: file");
        no_unformatted("# nopycln: import");

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
        composite_no_unformatted("# type: ignore  # noqa: A123, B456");

        composite_has_unformatted("#isort:skip#noqa:A123", 2);
        composite_has_unformatted("# fmt:off#   noqa: A123", 2);
        composite_has_unformatted("# noqa:A123 - Lorem ipsum dolor sit amet", 1);
    }
}
