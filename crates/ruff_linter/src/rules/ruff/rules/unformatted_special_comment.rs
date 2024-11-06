use crate::Locator;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::CommentRanges;
use ruff_text_size::TextRange;

use crate::noqa::{Directive, ParseError, ParsedFileExemption};

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
///
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
    original_text: &str,
    range: TextRange,
) {
    let formatted_comment = comment.formatted();

    if original_text == formatted_comment {
        return;
    }

    let edit = Edit::range_replacement(formatted_comment, range);
    let fix = Fix::safe_edit(edit);

    let violation = UnformattedSpecialComment(comment);
    let diagnostic = Diagnostic::new(violation, range).with_fix(fix);

    diagnostics.push(diagnostic);
}

#[inline]
fn all_to_owned(codes: Vec<&str>) -> Vec<String> {
    codes
        .iter()
        .map(|code| code.to_string())
        .collect::<Vec<_>>()
}

#[inline]
fn remove_comment_start_and_leading_whitespace(text: &str) -> Option<&str> {
    text.strip_prefix("#").map(|text| text.trim_ascii_start())
}

#[inline]
fn try_parse_file_level_noqa(text: &str) -> Result<Option<ParsedFileExemption>, ParseError> {
    ParsedFileExemption::try_extract(text)
}

#[inline]
fn file_level_noqa_special_comment(
    hint: String,
    file_level_noqa: ParsedFileExemption,
) -> SpecialComment {
    let codes = match file_level_noqa {
        ParsedFileExemption::All => None,
        ParsedFileExemption::Codes(codes) => Some(all_to_owned(codes)),
    };

    SpecialComment::FileLevelNoqa { hint, codes }
}

#[inline]
fn try_parse_noqa(text: &str) -> Result<Option<Directive>, ParseError> {
    Directive::try_extract(text, 0.into())
}

#[inline]
fn noqa_special_comment(noqa: Directive) -> SpecialComment {
    let codes = match noqa {
        Directive::All(..) => None,
        Directive::Codes(codes) => {
            let as_vec = codes.iter().map(|code| code.as_str()).collect::<Vec<_>>();

            Some(all_to_owned(as_vec))
        }
    };

    SpecialComment::Noqa(codes)
}

#[inline]
fn consume_hint(text: &str) -> Option<(&str, &str)> {
    let hint_end = text.chars().take_while(char::is_ascii_alphanumeric).count();
    let hint = &text[..hint_end];

    if hint.len() == 0 {
        return None;
    }

    let before_colon = text.strip_prefix(hint).unwrap().trim_ascii_start();

    let Some(rest) = before_colon
        .strip_prefix(':')
        .map(|text| text.trim_ascii_start())
    else {
        return None;
    };

    return Some((hint, rest));
}

fn try_parse_generic_hint_comment(comment_body: &str) -> Option<SpecialComment> {
    let Some((hint, rest)) = consume_hint(comment_body) else {
        return None;
    };

    match &hint.to_lowercase() {
        hint if KNOWN_COMMON_HINT.contains(&hint.as_str()) => Some(SpecialComment::Common {
            hint: hint.to_lowercase(),
            rest: rest.to_owned(),
        }),
        hint if hint == "ruff" => {
            let Some((second_hint, rest)) = consume_hint(rest) else {
                return None;
            };

            if second_hint != "isort" {
                return None;
            }

            Some(SpecialComment::RuffIsort(rest.to_owned()))
        }
        _ => None,
    }
}

fn check_single_comment(diagnostics: &mut Vec<Diagnostic>, text: &str, range: TextRange) {
    let Some(comment_body) = remove_comment_start_and_leading_whitespace(text) else {
        return;
    };

    // FIXME: Remove this once everything is done
    println!("Comment: {text:?}");

    if let Ok(Some(file_level_noqa)) = try_parse_file_level_noqa(text) {
        let hint = match comment_body.chars().into_iter().next() {
            Some('f') => "flake8",
            Some('r') => "ruff",
            _ => panic!("Unexpected parsed file exemption hint"),
        };

        let comment = file_level_noqa_special_comment(hint.to_owned(), file_level_noqa);
        add_diagnostic_if_applicable(diagnostics, comment, text, range);
        return;
    }

    if let Ok(Some(noqa)) = try_parse_noqa(text) {
        let comment = noqa_special_comment(noqa);
        add_diagnostic_if_applicable(diagnostics, comment, text, range);
        return;
    }

    let Some(comment) = try_parse_generic_hint_comment(comment_body) else {
        return;
    };

    add_diagnostic_if_applicable(diagnostics, comment, text, range);
}

/// RUF104
pub(crate) fn unformatted_special_comment(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    comment_ranges: &CommentRanges,
) {
    for range in comment_ranges {
        let text = locator.slice(range);

        check_single_comment(diagnostics, text, range);
    }
}
