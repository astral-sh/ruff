use crate::noqa::{Directive, ParseError, ParsedFileExemption};
use crate::Locator;
use once_cell::sync::Lazy;
use regex::Regex;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::CommentRanges;
use ruff_text_size::TextRange;

type EndIndex = usize;

const KNOWN_COMMON_HINT: [&str; 6] = ["fmt", "isort", "mypy", "nopycln", "pyright", "type"];

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
    /// `# ruff: isort: skip_file`
    RuffIsort(String),
    /// `# mypy: ignore-errors`, `# type: ignore`/`# type: int`,
    /// `# pyright: strict`/`# pyright: ignore[reportFoo]`,
    /// `# isort: skip_file`, `# nopycln: import`,
    /// `# fmt: on`/`# fmt: off`/`fmt: skip`.
    Common { hint: String, rest: String },
}

impl SpecialComment {
    fn formatted(&self) -> String {
        match self {
            SpecialComment::Common { hint, rest } => format!("# {}: {}", hint.to_lowercase(), rest)
                .trim_ascii_end()
                .to_owned(),
            SpecialComment::RuffIsort(rest) => {
                format!("# ruff: isort: {rest}")
            }
            SpecialComment::Noqa(None) => {
                format!("# noqa")
            }
            SpecialComment::Noqa(Some(codes)) => {
                format!("# noqa: {}", codes.join(", "))
            }
            SpecialComment::FileLevelNoqa { hint, codes: None } => {
                format!("# {hint}: noqa")
            }
            SpecialComment::FileLevelNoqa {
                hint,
                codes: Some(codes),
            } => {
                format!("# {hint}: noqa: {}", codes.join(", "))
            }
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
    range: TextRange,
) {
    let formatted_comment = comment.formatted();

    if original_comment_text == formatted_comment {
        return;
    }

    let edit = Edit::range_replacement(formatted_comment, range);
    let fix = Fix::safe_edit(edit);

    let violation = UnformattedSpecialComment(comment);
    let diagnostic = Diagnostic::new(violation, range).with_fix(fix);

    diagnostics.push(diagnostic);
}

fn parse_code_list(code_list: &str) -> Vec<String> {
    static PATTERN: Regex = Lazy::new(Regex::new(r"[A-Z]+[A-Za-z0-9]+"));

    PATTERN
        .find_iter(list)
        .map(|code| code.as_str().to_owned())
        .collect()
}

fn try_parse_file_level_noqa(text: &str) -> Option<(EndIndex, SpecialComment)> {
    static PATTERN: Regex = Lazy::new(
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
        .unwrap(),
    );

    let Some(result) = PATTERN.captures(text) else {
        return None;
    };

    let end_index = result.get(0).unwrap().end();
    let hint = result.name("hint").unwrap().as_str().to_owned();
    let codes = result
        .name("code_list")
        .map(|it| parse_code_list(it.as_str()));

    Some((end_index, SpecialComment::FileLevelNoqa { hint, codes }))
}

fn try_parse_noqa(text: &str) -> Option<(EndIndex, SpecialComment)> {
    static PATTERN: Regex = Lazy::new(
        Regex::new(
            r"(?x)
            ^
            \#\s*
            (?i:noqa)
            (?:
                :\s*
                (?<code_list>
                    [A-Z]+[A-Za-z0-9]+
                    (?:[\h,]+[A-Z]+[A-Za-z0-9]+)*
                )?
            )?
            $
            ",
        )
        .unwrap(),
    );

    let Some(result) = PATTERN.captures(text) else {
        return None;
    };

    let end_index = result.get(0).unwrap().end();
    let codes = result
        .name("code_list")
        .map(|it| parse_code_list(it.as_str()));

    Some((end_index, SpecialComment::Noqa(codes)))
}

fn try_parse_ruff_isort(text: &str) -> Option<(EndIndex, SpecialComment)> {
    static PATTERN: Regex = Lazy::new(
        Regex::new(
            r"(?x)
            ^
            \#\s*
            ruff\s*:\s*
            isort\s*:\s*
            (?<rest>.+)
            ",
        )
        .unwrap(),
    );

    let Some(result) = PATTERN.captures(text) else {
        return None;
    };

    let end_index = result.get(0).unwrap().end();
    let rest = result.name("rest").unwrap().as_str().to_owned();

    Some((end_index, SpecialComment::RuffIsort(rest)))
}

fn try_parse_common(text: &str) -> Option<(EndIndex, SpecialComment)> {
    static PATTERN: Lazy<Regex> = Lazy::new(
        Regex::new(
            r"(?x)
            ^
            \#\s*
            (?<hint>type|mypy|pyright|fmt|isort|nopycln):\s*
            (?<rest>.+)
            "
        )
        .unwrap()
    );

    let Some(result) = PATTERN.captures(text);

    let end_index = result.get(0).unwrap().end();
    let hint = result.name("hint").unwrap().as_str().to_owned();
    let rest = result.name("rest").unwrap().as_str().to_owned();

    Some((end_index, SpecialComment::Common(hint, rest)))
}

macro_rules! parse_and_handle_comment {
    ($parse:ident, $text:ident, $diagnostics:ident, $range:ident) => {
        if let Some((end_index, comment)) = $parse($text) {
            let comment_text = &$text[..end_index];

            add_diagnostic_if_applicable($diagnostics, comment, comment_text, $range);
            return;
        }
    };
}

fn check_single_comment(diagnostics: &mut Vec<Diagnostic>, text: &str, range: TextRange) {
    parse_and_handle_comment!(try_parse_file_level_noqa, text, diagnostics, range);
    parse_and_handle_comment!(try_parse_noqa, text, diagnostics, range);
    parse_and_handle_comment!(try_parse_ruff_isort, text, diagnostics, range);
    parse_and_handle_comment!(try_parse_common, text, diagnostics, range);
}

/// RUF104
pub(crate) fn unformatted_special_comment(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    comment_ranges: &CommentRanges,
) {
    for range in comment_ranges {
        let text = locator.slice(range);

        for (index, char) in text.char_indices() {
            if !matches!(char, '#') {
                continue;
            }

            check_single_comment(diagnostics, text, range);
        }
    }
}
