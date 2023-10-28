use std::iter::zip;

use super::{LogicalLine, LogicalLineToken};
use crate::checkers::logical_lines::LogicalLinesContext;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::TokenKind;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

/// ## What it does
/// Checks for continuation lines not indented as far as they should be or indented too far.
///
/// ## Why is this bad?
/// This makes reading code harder.
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
#[violation]
pub struct MissingOrOutdentedIndentation;

impl Violation for MissingOrOutdentedIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continuation line missing indentation or outdented.")
    }
}

#[derive(Debug)]
struct TokenInfo<'a> {
    start_physical_line_idx: usize,
    end_physical_line_idx: usize,
    token_start_within_physical_line: usize,
    token_end_within_physical_line: usize,
    line: &'a str,
}

/// Compute the TokenInfo of each token.
fn get_token_infos<'a>(logical_line: &LogicalLine, locator: &'a Locator) -> Vec<TokenInfo<'a>> {
    let mut token_infos = Vec::new();
    let mut current_line_idx: usize = 0;
    // The first physical line occupied by the token, since a token can span multiple physical lines.
    let mut first_physical_line_start = if let Some(first_token) = logical_line.first_token() {
        first_token.range.start()
    } else {
        return token_infos;
    };
    let mut current_physical_line_start: TextSize;
    let mut prev_token: Option<&LogicalLineToken> = None;
    for token in logical_line.tokens() {
        let mut start_physical_line_idx = current_line_idx;
        current_physical_line_start = first_physical_line_start;

        // Check for escaped ('\') continuation lines between the previous and current tokens.
        if let Some(prev_token) = prev_token {
            let trivia = locator.slice(TextRange::new(prev_token.range.end(), token.range.start()));
            for (index, _text) in trivia.match_indices("\n") {
                start_physical_line_idx += 1;
                current_line_idx = start_physical_line_idx;
                first_physical_line_start =
                    prev_token.range.end() + TextSize::try_from(index + 1).unwrap();
                current_physical_line_start = first_physical_line_start;
            }
        }

        if !matches!(
            token.kind,
            TokenKind::NonLogicalNewline | TokenKind::Newline
        ) {
            // Look for newlines within strings.
            let trivia = locator.slice(TextRange::new(token.range.start(), token.range.end()));
            for (index, _text) in trivia.match_indices("\n") {
                current_line_idx += 1;
                current_physical_line_start =
                    token.range.start() + TextSize::try_from(index + 1).unwrap();
            }
        }

        token_infos.push(TokenInfo {
            start_physical_line_idx,
            end_physical_line_idx: current_line_idx,
            token_start_within_physical_line: (token.range.start() - first_physical_line_start)
                .into(),
            token_end_within_physical_line: (token.range.end() - current_physical_line_start)
                .into(),
            line: locator.slice(locator.full_lines_range(token.range)),
        });

        if matches!(
            token.kind,
            TokenKind::NonLogicalNewline | TokenKind::Newline
        ) {
            current_line_idx += 1;
            first_physical_line_start = token.range.end();
        } else {
            first_physical_line_start = current_physical_line_start;
        }
        prev_token = Some(&token);
    }

    token_infos
}

/// Return the amount of indentation of the given line.
/// Tabs are expanded to the next multiple of 8.
fn expand_indent(line: &str) -> usize {
    if !line.contains('\t') {
        // If there are no tabs in the line, return the leading space count
        return line.len() - line.trim_start().len();
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
pub(crate) fn continuation_line_missing_indentation_or_outdented(
    context: &mut LogicalLinesContext,
    logical_line: &LogicalLine,
    locator: &Locator,
    indent_char: char,
    indent_size: usize,
) {
    let token_infos = get_token_infos(logical_line, locator);
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
            logical_line
                .first_token()
                .expect("Would have returned earlier if the logical line was empty")
                .start(),
        ),
    );

    // indent_next tells us whether the next block is indented.
    // Assuming that it is indented by 4 spaces, then we should not allow 4-space indents on the final continuation line.
    // In turn, some other indents are allowed to have an extra 4 spaces.
    let indent_next = logical_line.text().ends_with(':');

    // Here "row" is the physical line index (within the logical line).
    let mut row = 0;
    let mut depth = 0;
    let valid_hangs = if indent_char != '\t' {
        vec![indent_size as i64]
    } else {
        vec![indent_size as i64, indent_size as i64 * 2]
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
    let mut indent_chances: Vec<usize> = Vec::new();
    let mut last_indent = start_indent_level;
    let mut visual_indent = false;
    let mut last_token_multiline = false;
    // For each depth, memorize the visual indent column.
    let mut indent = vec![start_indent_level];

    // TODO: config option: hang closing bracket instead of matching indentation of opening bracket's line.
    let hang_closing = false;

    for (token, token_info) in zip(logical_line.tokens(), token_infos) {
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
            rel_indent[row] = expand_indent(token_info.line) as i64 - start_indent_level as i64;

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
            visual_indent = !is_closing_bracket
                && hang > 0
                && indent_chances.contains(&token_info.token_start_within_physical_line.into());

            if is_closing_bracket && indent[depth] != 0 {
                // Closing bracket for visual indent.
                if token_info.token_start_within_physical_line != indent[depth] {
                    dbg!("E124");
                    // TODO: Raise E124.
                }
            } else if is_closing_bracket && hang == 0 {
                // Closing bracket matches indentation of opening bracket's line
                if hang_closing {
                    dbg!("E133");
                    // TODO: Raise E133.
                }
            } else if indent[depth] != 0
                && token_info.token_start_within_physical_line < indent[depth]
            {
                // visual indent is broken
                if !visual_indent {
                    dbg!("E128");
                    // TODO: Raise E128.
                }
            } else if hanging_indent || (indent_next && rel_indent[row] == (2 * indent_size) as i64)
            {
                // hanging indent is verified
                if is_closing_bracket && !hang_closing {
                    dbg!("E123");
                    // TODO: Raise E123.
                }
                hangs[depth] = Some(hang);
            } else if visual_indent {
                // Visual indent is verified.
                indent[depth] = token_info.token_start_within_physical_line;
            } else {
                // Indent is broken.
                if hang <= 0 {
                    // E122.
                    let diagnostic = Diagnostic::new(MissingOrOutdentedIndentation, token.range);
                    // if autofix_after_open_bracket {
                    //     diagnostic
                    //         .set_fix(Fix::automatic(Edit::range_deletion(diagnostic.range())));
                    // }
                    context.push_diagnostic(diagnostic);
                } else if indent[depth] != 0 {
                    // TODO: Raise E127.
                } else if !is_closing_bracket && hangs[depth].is_some_and(|hang| hang > 0) {
                    // TODO: Raise 131.
                } else {
                    hangs[depth] = Some(hang);
                    if hang > indent_size as i64 {
                        // TODO: Raise 126.
                    } else {
                        // TODO: Raise E121.
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
            indent_chances.push(token_info.token_start_within_physical_line);
        }
        // Deal with implicit string concatenation.
        else if matches!(token.kind, TokenKind::Comment | TokenKind::String) {
            indent_chances.push(token_info.token_start_within_physical_line);
        }
        // Visual indent after assert/raise/with.
        else if row == 0
            && depth == 0
            && matches!(
                token.kind,
                TokenKind::Assert | TokenKind::Raise | TokenKind::With
            )
        {
            indent_chances.push(token_info.token_end_within_physical_line + 1);
        }
        // Special case for the "if" statement because "if (".len() == 4
        else if indent_chances.len() == 0
            && row == 0
            && depth == 0
            && matches!(token.kind, TokenKind::If)
        {
            indent_chances.push(token_info.token_end_within_physical_line + 1);
        } else if matches!(token.kind, TokenKind::Colon)
            && token_info.line[token_info.token_end_within_physical_line..]
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
                for d in 0..depth {
                    if indent[d] > prev_indent {
                        indent[d] = 0
                    }
                }
                indent_chances = indent_chances
                    .into_iter()
                    .filter(|&ind| ind < prev_indent)
                    .collect();
                open_rows.truncate(depth);
                depth -= 1;
                if depth > 0 {
                    indent_chances.push(indent[depth]);
                }
                for idx in (0..row + 1).rev() {
                    if parens[idx] != 0 {
                        parens[idx] -= 1;
                        break;
                    }
                }
            }
            if !indent_chances.contains(&token_info.token_start_within_physical_line) {
                // Allow lining up tokens
                indent_chances.push(token_info.token_start_within_physical_line);
            }
        }

        last_token_multiline =
            token_info.start_physical_line_idx != token_info.end_physical_line_idx;
        if last_token_multiline {
            rel_indent[token_info.end_physical_line_idx] = rel_indent[row]
        }

        if indent_next && expand_indent(token_info.line) == start_indent_level + indent_size {
            if visual_indent {
                // TODO: Raise 129.
            } else {
                // TODO: Raise 125.
            }
        }
    }
}
