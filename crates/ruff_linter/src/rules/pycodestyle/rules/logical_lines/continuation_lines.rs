use std::iter::zip;

use super::{LogicalLine, LogicalLineToken};
use crate::checkers::logical_lines::LogicalLinesContext;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::TokenKind;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

/// ## What it does
/// Checks for lines less indented than they should be for hanging indents.
///
/// ## Why is this bad?
/// This makes reading continuation line harder.
///
/// ## Example
/// ```python
/// result = {
///    'key1': 'value',
///    'key2': 'value',
/// }
/// ```
///
/// Use instead:
/// ```python
/// result = {
///     'key1': 'value',
///     'key2': 'value',
/// }
/// ```
///
/// [PEP 8]: https://www.python.org/dev/peps/pep-0008/#indentation
#[violation]
pub struct UnderIndentedHangingIndent;

impl Violation for UnderIndentedHangingIndent {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hanging indent under-indented.")
    }
}

/// ## What it does
/// Checks for continuation lines not indented as far as they should be or indented too far.
///
/// ## Why is this bad?
/// This makes distinguishing continuation line harder.
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

/// ## What it does
/// Checks for brackets that do not match the indentation level of the line that their opening bracket started on.
///
/// ## Why is this bad?
/// This makes identifying brakets pair harder.
///
/// ## Example
/// ```python
/// result = function_that_takes_arguments(
///     'a', 'b', 'c',
///     'd', 'e', 'f',
///     )
/// ```
///
/// Use instead:
/// ```python
/// result = function_that_takes_arguments(
///     'a', 'b', 'c',
///     'd', 'e', 'f',
/// )
/// ```
///
/// [PEP 8]: https://www.python.org/dev/peps/pep-0008/#indentation
#[violation]
pub struct ClosingBracketNotMatchingOpeningBracketIndentation;

impl Violation for ClosingBracketNotMatchingOpeningBracketIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Closing bracket not matching its corresponding opening bracket's indentation.")
    }
}

/// ## What it does
/// Checks for closing brackets that do not match the indentation of the opening bracket.
///
/// ## Why is this bad?
/// This makes identifying brakets pair harder.
///
/// ## Example
/// ```python
/// result = function_that_takes_arguments('a', 'b', 'c',
///                                        'd', 'e', 'f',
/// )
/// ```
///
/// Use instead:
/// ```python
/// result = function_that_takes_arguments('a', 'b', 'c',
///                                        'd', 'e', 'f',
///                                       )
/// ```
///
/// [PEP 8]: https://www.python.org/dev/peps/pep-0008/#indentation
#[violation]
pub struct ClosingBracketNotMatchingOpeningBracketVisualIndentation;

impl Violation for ClosingBracketNotMatchingOpeningBracketVisualIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Closing bracket not matching its corresponding opening bracket's visual indentation."
        )
    }
}

/// ## What it does
/// Checks for continuation lines with the same indent as the next logical line.
///
/// ## Why is this bad?
/// Continuation lines should not be indented at the same level as the next logical line.
/// Instead, they should be indented to one more level so as to distinguish them from the next line.
///
/// ## Example
/// ```python
/// if user is not None and user.is_admin or \
///     user.name == 'Grant':
///     blah = 'yeahnah'
/// ```
///
/// Use instead:
/// ```python
/// if user is not None and user.is_admin or \
///         user.name == 'Grant':
///     blah = 'yeahnah'
/// ```
///
/// [PEP 8]: https://www.python.org/dev/peps/pep-0008/#indentation
#[violation]
pub struct ContinuationLineIndentSameAsNextLogicalLine;

impl Violation for ContinuationLineIndentSameAsNextLogicalLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continuation line with same indent as next logical line.")
    }
}

/// ## What it does
/// Checks for continuation line over-indented for hanging indent.
///
/// ## Why is this bad?
/// This makes distinguishing continuation lines harder.
///
/// ## Example
/// ```python
/// print("Python", (
///         "Rules"))
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
pub struct ContinuationLineOverIndentedForHangingIndent;

impl Violation for ContinuationLineOverIndentedForHangingIndent {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continuation line over indented for hanging indent.")
    }
}

/// ## What it does
/// Checks for continuation line over-indented for visual indent.
///
/// ## Why is this bad?
/// This makes distinguishing continuation lines harder.
///
/// ## Example
/// ```python
/// print("Python", ("Hello",
///                    "World"))
/// ```
///
/// Use instead:
/// ```python
/// print("Python", ("Hello",
///                  "World"))
/// ```
///
/// [PEP 8]: https://www.python.org/dev/peps/pep-0008/#indentation
#[violation]
pub struct ContinuationLineOverIndentedForVisualIndent;

impl Violation for ContinuationLineOverIndentedForVisualIndent {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continuation line over indented for visual indent.")
    }
}

#[derive(Debug)]
struct TokenInfo<'a> {
    start_physical_line_idx: usize,
    end_physical_line_idx: usize,
    token_start_within_physical_line: i64,
    token_end_within_physical_line: i64,
    line: &'a str,
}

/// Compute the `TokenInfo` of each token.
fn get_token_infos<'a>(logical_line: &LogicalLine, locator: &'a Locator) -> Vec<TokenInfo<'a>> {
    let mut token_infos = Vec::new();
    let mut current_line_idx: usize = 0;
    // The first physical line occupied by the token, since a token can span multiple physical lines.
    let mut first_physical_line_start: usize = if let Some(first_token) = logical_line.first_token()
    {
        first_token.range.start().into()
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
            let trivia = locator.slice(TextRange::new(prev_token.range.end(), token.range.start()));
            for (index, _text) in trivia.match_indices('\n') {
                start_physical_line_idx += 1;
                current_line_idx = start_physical_line_idx;
                first_physical_line_start = usize::from(prev_token.range.end()) + index + 1;
                current_physical_line_start = first_physical_line_start;
            }
        }

        if !matches!(
            token.kind,
            TokenKind::NonLogicalNewline | TokenKind::Newline
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
            line: locator.full_lines(token.range),
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

/// E121 E122 E123 E124 E125 E126 E127 E128 E129 E133
pub(crate) fn continuation_lines(
    context: &mut LogicalLinesContext,
    logical_line: &LogicalLine,
    locator: &Locator,
    indent_char: char,
    indent_size: usize,
) {
    // The pycodestyle implementation makes use of negative values,
    // converting the indent_size type at the start avoids converting it multiple times later.
    let indent_size = i64::try_from(indent_size).expect("Indent size to be relatively small.");
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
    let mut indent_chances: Vec<i64> = Vec::new();
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
            rel_indent[row] = expand_indent(token_info.line) - start_indent_level;

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
                && indent_chances.contains(&token_info.token_start_within_physical_line);

            if is_closing_bracket && indent[depth] != 0 {
                // Closing bracket for visual indent.
                if token_info.token_start_within_physical_line != indent[depth] {
                    // E124.
                    let diagnostic = Diagnostic::new(
                        ClosingBracketNotMatchingOpeningBracketVisualIndentation,
                        token.range,
                    );
                    context.push_diagnostic(diagnostic);
                }
            } else if is_closing_bracket && hang == 0 {
                // Closing bracket matches indentation of opening bracket's line
                if hang_closing {
                    // TODO: Raise E133.
                }
            } else if indent[depth] != 0
                && token_info.token_start_within_physical_line < indent[depth]
            {
                // visual indent is broken
                if !visual_indent {
                    // TODO: Raise E128.
                }
            } else if hanging_indent || (indent_next && rel_indent[row] == (2 * indent_size)) {
                // hanging indent is verified
                if is_closing_bracket && !hang_closing {
                    // E123.
                    let diagnostic = Diagnostic::new(
                        ClosingBracketNotMatchingOpeningBracketIndentation,
                        token.range,
                    );
                    context.push_diagnostic(diagnostic);
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
                    context.push_diagnostic(diagnostic);
                } else if indent[depth] != 0 {
                    // E127
                    let diagnostic =
                        Diagnostic::new(ContinuationLineOverIndentedForVisualIndent, token.range);
                    context.push_diagnostic(diagnostic);
                } else if !is_closing_bracket && hangs[depth].is_some_and(|hang| hang > 0) {
                    // TODO: Raise 131.
                } else {
                    hangs[depth] = Some(hang);
                    if hang > indent_size {
                        // E126
                        let diagnostic = Diagnostic::new(
                            ContinuationLineOverIndentedForHangingIndent,
                            token.range,
                        );
                        context.push_diagnostic(diagnostic);
                    } else {
                        // E121.
                        let diagnostic = Diagnostic::new(UnderIndentedHangingIndent, token.range);
                        context.push_diagnostic(diagnostic);
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
        else if indent_chances.is_empty()
            && row == 0
            && depth == 0
            && matches!(token.kind, TokenKind::If)
        {
            indent_chances.push(token_info.token_end_within_physical_line + 1);
        } else if matches!(token.kind, TokenKind::Colon)
            && token_info.line[usize::try_from(token_info.token_end_within_physical_line)
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
                indent_chances.retain(|&ind| ind < prev_indent);
                open_rows.truncate(depth);
                depth -= 1;
                if depth > 0 {
                    indent_chances.push(indent[depth]);
                }
                for idx in (0..=row).rev() {
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
            rel_indent[token_info.end_physical_line_idx] = rel_indent[row];
        }

        if indent_next && expand_indent(token_info.line) == start_indent_level + indent_size {
            if visual_indent {
                // TODO: Raise 129.
            } else {
                // E125.
                let diagnostic =
                    Diagnostic::new(ContinuationLineIndentSameAsNextLogicalLine, token.range);
                context.push_diagnostic(diagnostic);
            }
        }
    }
}
