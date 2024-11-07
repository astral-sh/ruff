use crate::Locator;
use std::sync::LazyLock;
use regex::Regex;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::CommentRanges;
use ruff_text_size::TextRange;
use ruff_text_size::TextSize;

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
            SpecialComment::Common { hint, rest } => {
                format!("# {}: {}", hint.to_lowercase(), rest)
            }
            SpecialComment::RuffIsort(rest) => {
                format!("# ruff: isort: {rest}")
            }
            SpecialComment::Noqa(None) => "# noqa".to_string(),
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
    original_comment_text_range: TextRange,
) {
    let formatted_comment_text = comment.formatted();

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

fn try_parse_file_level_noqa(text: &str) -> Option<(EndIndex, SpecialComment)> {
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

fn try_parse_noqa(text: &str) -> Option<(EndIndex, SpecialComment)> {
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

fn try_parse_ruff_isort(text: &str) -> Option<(EndIndex, SpecialComment)> {
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?x)
            ^
            \#\s*
            ruff\s*:\s*
            isort\s*:\s*
            (?<rest>.+)
            ",
        )
        .unwrap()
    });

    let result = PATTERN.captures(text)?;

    let end_index = result.get(0).unwrap().end();
    let rest = result.name("rest").unwrap().as_str().to_owned();

    Some((end_index, SpecialComment::RuffIsort(rest)))
}

fn try_parse_common(text: &str) -> Option<(EndIndex, SpecialComment)> {
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?x)
            ^
            \#\s*
            (?<hint>type|mypy|pyright|fmt|isort|nopycln):\s*
            (?<rest>.+)
            ",
        )
        .unwrap()
    });

    let result = PATTERN.captures(text)?;

    let end_index = result.get(0).unwrap().end();
    let hint = result.name("hint").unwrap().as_str().to_owned();
    let rest = result.name("rest").unwrap().as_str().to_owned();

    Some((end_index, SpecialComment::Common { hint, rest }))
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
    parse_and_handle_comment!(try_parse_file_level_noqa, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_noqa, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_ruff_isort, text, diagnostics, start_index);
    parse_and_handle_comment!(try_parse_common, text, diagnostics, start_index);
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

            check_single_comment(diagnostics, &text[index..], index);
        }
    }
}
