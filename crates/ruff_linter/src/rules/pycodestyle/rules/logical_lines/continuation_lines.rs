use std::iter::zip;

use super::{LogicalLine, LogicalLineToken};
use crate::checkers::logical_lines::LogicalLinesContext;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_python_parser::TokenKind;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};
use rustc_hash::FxHashMap;

/// ## What it does
/// Checks for continuation lines without enough indentation.
///
/// ## Why is this bad?
/// This makes distinguishing continuation lines more difficult.
///
/// ## Example
/// ```python
/// print("Python", (
/// "Rules"))
/// ```
///
/// Use instead:
/// ```python
/// print("Python", (
///     "Rules"))
/// ```
///
/// [PEP 8]: https://www.python.org/dev/peps/pep-0008/#indentation
#[violation]
pub struct MissingOrOutdentedIndentation;

impl Violation for MissingOrOutdentedIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continuation line missing indentation or outdented.")
    }
}

#[derive(Debug)]
struct TokenInfo {
    start_physical_line_idx: usize,
    end_physical_line_idx: usize,
    token_start_within_physical_line: i64,
    token_end_within_physical_line: i64,
}

#[derive(Debug, Clone)]
enum IndentFlag {
    /// The pycodestyle's True
    Standard,
    /// The pycodestyle's text (str instance)
    Token(TokenKind),
    /// The pycodestyle's str class
    StringOrComment,
}

/// Compute the `TokenInfo` of each token.
fn get_token_infos<'a>(
    logical_line: &LogicalLine,
    locator: &'a Locator,
    indexer: &'a Indexer,
) -> Vec<TokenInfo> {
    let mut token_infos = Vec::new();
    let mut current_line_idx: usize = 0;
    // The first physical line occupied by the token, since a token can span multiple physical lines.
    let mut first_physical_line_start: usize;
    let mut next_continuation;
    if let Some(first_token) = logical_line.first_token() {
        first_physical_line_start = first_token.range.start().into();
        next_continuation = continuation_line_end(first_token, locator, indexer);
    } else {
        return token_infos;
    };

    let mut current_physical_line_start: usize;
    let mut prev_token: Option<&LogicalLineToken> = None;
    for token in logical_line.tokens() {
        let mut start_physical_line_idx = current_line_idx;
        current_physical_line_start = first_physical_line_start;

        // Check for escaped ('\') continuation lines between the previous and current tokens.
        if let Some(prev_token) = prev_token {
            if next_continuation.is_some() && token.start() >= next_continuation.unwrap() {
                next_continuation = continuation_line_end(token, locator, indexer);

                let trivia =
                    locator.slice(TextRange::new(prev_token.range.end(), token.range.start()));
                for (index, _text) in trivia.match_indices('\n') {
                    start_physical_line_idx += 1;
                    current_line_idx = start_physical_line_idx;
                    first_physical_line_start = usize::from(prev_token.range.end()) + index + 1;
                    current_physical_line_start = first_physical_line_start;
                }
            }
        }

        if matches!(
            token.kind,
            TokenKind::String
                | TokenKind::FStringStart
                | TokenKind::FStringMiddle
                | TokenKind::FStringEnd
        ) {
            // Look for newlines within strings.
            let trivia = locator.slice(token.range);
            for (index, _text) in trivia.match_indices('\n') {
                current_line_idx += 1;
                current_physical_line_start = usize::from(token.range.start()) + index + 1;
            }
        }

        token_infos.push(TokenInfo {
            start_physical_line_idx,
            end_physical_line_idx: current_line_idx,
            token_start_within_physical_line: i64::try_from(
                usize::from(token.range.start()) - first_physical_line_start,
            )
            .expect("Lines are expected to be relatively short."),
            token_end_within_physical_line: i64::try_from(
                usize::from(token.range.end()) - current_physical_line_start,
            )
            .expect("Lines are expected to be relatively short."),
        });

        if matches!(
            token.kind,
            TokenKind::NonLogicalNewline | TokenKind::Newline
        ) {
            current_line_idx += 1;
            first_physical_line_start = token.range.end().into();
        } else {
            first_physical_line_start = current_physical_line_start;
        }
        prev_token = Some(token);
    }

    token_infos
}

fn continuation_line_end(
    token: &LogicalLineToken,
    locator: &Locator,
    indexer: &Indexer,
) -> Option<TextSize> {
    let continuation_lines = indexer.continuation_line_starts();
    let continuation_line_index = continuation_lines
        .binary_search(&token.start())
        .unwrap_or_else(|err_index| err_index);
    let continuation_line_start = continuation_lines.get(continuation_line_index)?;
    Some(locator.full_line_end(*continuation_line_start))
}

/// Return the amount of indentation of the given line.
/// Tabs are expanded to the next multiple of 8.
fn expand_indent(line: &str) -> i64 {
    if !line.contains('\t') {
        // If there are no tabs in the line, return the leading space count
        return i64::try_from(line.len() - line.trim_start().len())
            .expect("Line length to be relatively small.");
    }
    let mut indent = 0;

    for ch in line.chars() {
        if ch == '\t' {
            indent = indent / 8 * 8 + 8;
        } else if ch == ' ' {
            indent += 1;
        } else {
            break;
        }
    }

    indent
}

/// E122
pub(crate) fn continuation_lines(
    line: &LogicalLine,
    indent_char: char,
    indent_size: usize,
    locator: &Locator,
    indexer: &Indexer,
    context: &mut LogicalLinesContext,
) {
    // The pycodestyle implementation makes use of negative values,
    // converting the indent_size type at the start avoids converting it multiple times later.
    let indent_size = i64::try_from(indent_size).expect("Indent size to be relatively small.");
    let token_infos = get_token_infos(line, locator, indexer);
    let nb_physical_lines = if let Some(last_token_info) = token_infos.last() {
        1 + last_token_info.start_physical_line_idx
    } else {
        1
    };

    if nb_physical_lines == 1 {
        return;
    }

    // Indent of the first physical line.
    let start_indent_level = expand_indent(
        locator.line(
            line.first_token()
                .expect("Would have returned earlier if the logical line was empty")
                .start(),
        ),
    );

    // indent_next tells us whether the next block is indented.
    // Assuming that it is indented by 4 spaces, then we should not allow 4-space indents on the final continuation line.
    // In turn, some other indents are allowed to have an extra 4 spaces.
    let indent_next = line.text().trim_end().ends_with(':');

    // Here "row" is the physical line index (within the logical line).
    let mut row = 0;
    let mut depth = 0;
    let valid_hangs = if indent_char == '\t' {
        vec![indent_size, indent_size * 2]
    } else {
        vec![indent_size]
    };
    // Remember how many brackets were opened on each line.
    let mut parens = vec![0; nb_physical_lines];
    // Relative indents of physical lines.
    let mut rel_indent: Vec<i64> = vec![0; nb_physical_lines];
    // For each depth, collect a list of opening rows.
    let mut open_rows = vec![vec![0]];
    // For each depth, memorize the hanging indentation.
    let mut hangs: Vec<Option<i64>> = vec![None];
    let mut hang: i64 = 0;
    let mut hanging_indent: bool = false;
    // Visual indents
    let mut indent_chances: FxHashMap<i64, IndentFlag> = FxHashMap::default();
    let mut last_indent = start_indent_level;
    let mut visual_indent: Option<IndentFlag> = None;
    let mut last_token_multiline = false;
    // For each depth, memorize the visual indent column.
    let mut indent = vec![start_indent_level];

    for (token, token_info) in zip(line.tokens(), &token_infos) {
        let mut is_newline = row < token_info.start_physical_line_idx;
        if is_newline {
            row = token_info.start_physical_line_idx;
            is_newline = !last_token_multiline
                && !matches!(
                    token.kind,
                    TokenKind::NonLogicalNewline | TokenKind::Newline
                );
        }

        let is_closing_bracket = matches!(
            token.kind,
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace
        );

        // This is the beginning of a continuation line.
        if is_newline {
            last_indent = token_info.token_start_within_physical_line;

            // Record the initial indent.
            rel_indent[row] = expand_indent(locator.full_lines(token.range)) - start_indent_level;

            // Is the indent relative to an opening bracket line ?
            for open_row in open_rows[depth].iter().rev() {
                hang = rel_indent[row] - rel_indent[*open_row];
                hanging_indent = valid_hangs.contains(&hang);
                if hanging_indent {
                    break;
                }
            }
            if let Some(depth_hang) = hangs[depth] {
                hanging_indent = hang == depth_hang;
            }

            // Is there any chance of visual indent ?
            visual_indent = if !is_closing_bracket && hang > 0 {
                indent_chances
                    .get(&token_info.token_start_within_physical_line)
                    .cloned()
            } else {
                None
            };

            if is_closing_bracket && indent[depth] != 0 {
            } else if is_closing_bracket && hang == 0 {
            } else if indent[depth] != 0
                && token_info.token_start_within_physical_line < indent[depth]
            {
            } else if hanging_indent || (indent_next && rel_indent[row] == (2 * indent_size)) {
                hangs[depth] = Some(hang);
            } else {
                match visual_indent {
                    Some(IndentFlag::Standard) => {
                        // Visual indent is verified.
                        indent[depth] = token_info.token_start_within_physical_line;
                    }
                    Some(IndentFlag::StringOrComment) => {
                        // Ignore token lined up with matching one from a previous line.
                    }
                    Some(IndentFlag::Token(t)) if t == token.kind => {
                        // Ignore token lined up with matching one from a previous line.
                    }
                    _ => {
                        // Indent is broken.
                        if hang <= 0 {
                            // E122.
                            let diagnostic =
                                Diagnostic::new(MissingOrOutdentedIndentation, token.range);
                            context.push_diagnostic(diagnostic);
                        }
                    }
                }
            }
        }

        // Look for visual indenting.
        if parens[row] != 0
            && !matches!(
                token.kind,
                TokenKind::Newline | TokenKind::NonLogicalNewline | TokenKind::Comment
            )
            && indent[depth] == 0
        {
            indent[depth] = token_info.token_start_within_physical_line;
            indent_chances.insert(
                token_info.token_start_within_physical_line,
                IndentFlag::Standard,
            );
        }
        // Deal with implicit string concatenation.
        else if matches!(token.kind, TokenKind::Comment | TokenKind::String) {
            indent_chances.insert(
                token_info.token_start_within_physical_line,
                IndentFlag::StringOrComment,
            );
        }
        // Visual indent after assert/raise/with.
        else if (row == 0
            && depth == 0
            && matches!(
                token.kind,
                TokenKind::Assert | TokenKind::Raise | TokenKind::With
            ))
        // Special case for the "if" statement because "if (".len() == 4
       || (indent_chances.is_empty()
            && row == 0
            && depth == 0
            && matches!(token.kind, TokenKind::If))
        {
            indent_chances.insert(
                token_info.token_end_within_physical_line + 1,
                IndentFlag::Standard,
            );
        } else if matches!(token.kind, TokenKind::Colon)
            && locator.full_lines(token.range)[usize::try_from(
                token_info.token_end_within_physical_line,
            )
            .expect("Line to be relatively short.")..]
                .trim()
                .is_empty()
        {
            open_rows[depth].push(row);
        }

        let is_opening_bracket = matches!(
            token.kind,
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace
        );

        // Keep track of bracket depth.
        if is_opening_bracket || is_closing_bracket {
            if is_opening_bracket {
                depth += 1;
                indent.push(0);
                hangs.push(None);
                if open_rows.len() == depth {
                    open_rows.push(Vec::new());
                }
                open_rows[depth].push(row);
                parens[row] += 1;
            } else if is_closing_bracket && depth > 0 {
                // Parent indents should not be more than this one.
                let prev_indent = if let Some(i) = indent.pop() {
                    if i > 0 {
                        i
                    } else {
                        last_indent
                    }
                } else {
                    last_indent
                };
                hangs.pop();
                for ind in indent.iter_mut().take(depth) {
                    if *ind > prev_indent {
                        *ind = 0;
                    }
                }
                indent_chances.retain(|&ind, _| ind < prev_indent);
                open_rows.truncate(depth);
                depth -= 1;
                if depth > 0 {
                    indent_chances.insert(indent[depth], IndentFlag::Standard);
                }
                for idx in (0..=row).rev() {
                    if parens[idx] != 0 {
                        parens[idx] -= 1;
                        break;
                    }
                }
            }
            indent_chances
                .entry(token_info.token_start_within_physical_line)
                .or_insert(IndentFlag::Token(token.kind));
        }

        last_token_multiline =
            token_info.start_physical_line_idx != token_info.end_physical_line_idx;
        if last_token_multiline {
            rel_indent[token_info.end_physical_line_idx] = rel_indent[row];
        }
    }
}
