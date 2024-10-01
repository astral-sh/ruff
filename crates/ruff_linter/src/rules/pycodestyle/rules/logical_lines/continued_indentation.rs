use crate::checkers::logical_lines::{expand_indent, LogicalLinesContext};
use crate::line_width::IndentWidth;
use crate::rules::pycodestyle::rules::logical_lines::{TextRange, TextSize};
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_parser::TokenKind;
use ruff_source_file::LineRanges;
use rustc_hash::FxHashMap;

use super::LogicalLine;

/// ## What it does
/// Checks that a line is less indented than it should be for hanging indents.
///
/// ## Why is this bad?
/// According to [PEP8], continued lines with hanging indents should be aligned
/// to the next indentation level.
///
/// ## Example
/// ```python
/// print("Python", (
///   "Rules"))
/// ```
///
/// Use instead:
/// ```python
/// print("Python", (
///     "Rules"))
/// ```
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[derive(ViolationMetadata)]
pub(crate) struct ContinuationUnderIndentedHanging;

impl Violation for ContinuationUnderIndentedHanging {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Continuation line under-indented for hanging indent".to_string()
    }
}

/// ## What it does
/// Checks that a continuation line is not indented as far as it should be or
/// is indented too far.
///
/// ## Why is this bad?
/// According to [PEP8], continued lines with hanging indents should be aligned
/// to the next indentation level.
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
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[derive(ViolationMetadata)]
pub(crate) struct ContinuationOverIndentedOrMissing;

impl Violation for ContinuationOverIndentedOrMissing {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Continuation line missing indentation or outdented".to_string()
    }
}

/// ## What it does
/// Checks that closing brackets match the same indentation level of the line
/// that their opening bracket started on.
///
/// ## Why is this bad?
/// According to [PEP8], closing brackets on their own line should match the
/// same indentation level as the opening bracket.
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
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[derive(ViolationMetadata)]
pub(crate) struct ClosingBracketMismatched;

impl Violation for ClosingBracketMismatched {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Closing bracket does not match indentation of opening bracket's line".to_string()
    }
}

/// ## What it does
/// Checks if a closing bracket does not match visual indentation.
///
/// ## Why is this bad?
/// According to [PEP8], closing brackets should match the indentation of the
/// opening bracket.
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
///                                        )
/// ```
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[derive(ViolationMetadata)]
pub(crate) struct ClosingBracketMismatchedVisualIndent;

impl Violation for ClosingBracketMismatchedVisualIndent {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Closing bracket does not match visual indentation".to_string()
    }
}

/// ## What it does
/// Checks for a continuation line with same indent as next logical line.
///
/// ## Why is this bad?
/// According to [PEP8], continuation lines should not be indented at the same
/// level as the next logical line. Instead, they should be indented to one
/// more level so as to distinguish them from the next line.
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
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[derive(ViolationMetadata)]
pub(crate) struct ContinuationMatchesNextLine;

impl Violation for ContinuationMatchesNextLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Continuation line with same indent as next logical line".to_string()
    }
}

/// ## What it does
/// Checks for a continuation line over-indented for hanging indent.
///
/// ## Why is this bad?
/// According to [PEP8], continuation lines should not be indented any more
/// than necessary.
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
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[derive(ViolationMetadata)]
pub(crate) struct ContinuationOverIndentedHanging;

impl Violation for ContinuationOverIndentedHanging {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Continuation line over-indented for hanging indent".to_string()
    }
}

/// ## What it does
/// Checks if a continuation line is over-indented for visual indent.
///
/// ## Why is this bad?
/// When using visual indentation to break a statement over multiple lines it
/// should be aligned with a token in the line above.
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
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[derive(ViolationMetadata)]
pub(crate) struct ContinuationOverIndentedVisual;

impl Violation for ContinuationOverIndentedVisual {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Continuation line over-indented for visual indent".to_string()
    }
}

/// ## What it does
/// Checks if a continuation line is over-indented for visual indent.
///
/// ## Why is this bad?
/// When using visual indentation to break a statement over multiple lines it
/// should be aligned with a token in the line above.
///
/// ## Example
/// ```python
/// print("Python", ("Hello",
///                "World"))
/// ```
///
/// Use instead:
/// ```python
/// print("Python", ("Hello",
///                  "World"))
/// ```
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[derive(ViolationMetadata)]
pub(crate) struct ContinuationUnderIndentedVisual;

impl Violation for ContinuationUnderIndentedVisual {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Continuation line under-indented for visual indent".to_string()
    }
}

/// ## What it does
/// Checks for a visually indented line with same indent as next logical line.
///
/// ## Why is this bad?
/// A visual indented line has the same indentation as the next logical line.
/// This can make it hard to read.
///
/// ## Example
/// ```python
/// if (row < 0 or module_count <= row or
///     col < 0 or module_count <= col):
///     raise Exception("%s,%s - %s" % (row, col, self.moduleCount))
/// ```
///
/// Use instead:
/// ```python
/// if (row < 0 or module_count <= row or
///         col < 0 or module_count <= col):
///     raise Exception("%s,%s - %s" % (row, col, self.moduleCount))
/// ```
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[derive(ViolationMetadata)]
pub(crate) struct VisualIndentMatchesNextLine;

impl Violation for VisualIndentMatchesNextLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Visually indented line with same indent as next logical line".to_string()
    }
}

/// ## What it does
/// Checks if a continuation line unaligned for hanging indent.
///
/// ## Why is this bad?
/// When using visual indentation to break a statement over multiple lines it
/// should be aligned with a token in the line above.
///
/// ## Example
/// ```python
/// my_dict = {
///     "key": "value",
///     "long": "the quick brown fox jumps over the "
///         "lazy dog",
/// }
/// ```
///
/// Use instead:
/// ```python
/// my_dict = {
///     "key": "value",
///     "long": "the quick brown fox jumps over the "
///             "lazy dog",
/// }
/// ```
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[derive(ViolationMetadata)]
pub(crate) struct ContinuationUnalignedHanging;

impl Violation for ContinuationUnalignedHanging {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Continuation line unaligned for hanging indent".to_string()
    }
}

/// ## What it does
/// Checks if a closing bracket is missing indentation.
///
/// ## Why is this bad?
/// If `hang-closing` is enabled, the closing bracket should be indented along
/// with the other items in the list.
/// This error only occurs if the `hang-closing` is used, switching the default
/// behavior of closing brackets so that they require hanging indents.
///
/// ## Example
/// ```python
/// my_list = [
///     1, 2, 3,
///     4, 5, 6,
/// ]
/// ```
///
/// Use instead:
/// ```python
/// my_list = [
///     1, 2, 3,
///     4, 5, 6,
///     ]
/// ```
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// ## Options
/// - `lint.pycodestyle.hang-closing`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[derive(ViolationMetadata)]
pub(crate) struct ClosingBracketMissingIndentation;

impl Violation for ClosingBracketMissingIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Closing bracket is missing indentation".to_string()
    }
}

#[derive(Clone, Debug, PartialEq)]
enum VisualIndentType {
    Normal,
    String,
    Token(TokenKind),
}

fn valid_hang(hang: isize, indent_width: IndentWidth, indent_char: char) -> bool {
    // SAFETY: Never indent width is never going to be huge
    let indent_size = isize::try_from(indent_width.as_usize()).unwrap();
    hang == indent_size || (indent_char == '\t' && hang == 2 * indent_size)
}

// E121, E122, E123, E124, E125, E126, E127, E128, E129, E131, E133
pub(crate) fn continued_indentation(
    logical_line: &LogicalLine,
    indent_level: usize,
    indent_char: char,
    indent_width: IndentWidth,
    hang_closing: bool,
    context: &mut LogicalLinesContext,
) {
    let total_lines = logical_line.text().chars().filter(|c| *c == '\n').count();

    assert!(total_lines > 0);
    if total_lines == 1 {
        return;
    }

    // indent_next tells us whether the next block is indented; assuming
    // that it is indented by 4 spaces, then we should not allow 4-space
    // indents on the final continuation line; in turn, some other
    // indents are allowed to have an extra 4 spaces.
    let indent_next = logical_line
        .tokens_trimmed()
        .last()
        .is_some_and(|t| t.kind() == TokenKind::Colon);

    let mut line_no = 0;
    let mut bracket_depth = 0;

    // remember how many brackets were opened on each line
    let mut parens = vec![0; total_lines];

    // relative indents of physical lines
    let mut relative_line_indent = vec![0; total_lines];

    // for each bracket_depth collect a list of opening rows
    let mut open_lines = vec![vec![0]];

    // for each bracket_depth, memorize the hanging indentation
    let mut hangs = vec![None];

    // visual indents
    let mut indent_chances: FxHashMap<usize, VisualIndentType> = FxHashMap::default();
    let mut visual_indent: Option<VisualIndentType> = None;
    let mut prev_newline_token = None;
    // for each bracket_depth, memorize the visual indent column
    let mut bracket_indent_level = vec![];
    {
        // SAFETY: There is always an initial token
        let first_token = logical_line.tokens().first().unwrap();
        let initial_token_indent: usize = (first_token.range.start()
            - logical_line
                .lines
                .locator
                .line_start(first_token.range.start()))
        .into();
        bracket_indent_level.push(initial_token_indent);
    }

    for tok_idx in 0..logical_line.tokens().len() {
        let maybe_prev_token = if tok_idx > 0 {
            Some(&logical_line.tokens()[tok_idx - 1])
        } else {
            None
        };
        let tok = &logical_line.tokens()[tok_idx];

        // Total lines in this token including any previous whitespace & continuation lines
        let num_newlines_in_whitespace = if let Some(prev_tok) = maybe_prev_token {
            logical_line
                .lines
                .locator
                .slice(TextRange::new(prev_tok.range.end(), tok.range.start()))
                .match_indices('\n')
                .count()
        } else {
            0
        };

        let num_token_newlines = logical_line
            .lines
            .locator
            .slice(TextRange::new(tok.range.start(), tok.range.end()))
            .match_indices('\n')
            .count();
        line_no += num_newlines_in_whitespace + num_token_newlines;

        if matches!(
            tok.kind(),
            TokenKind::Newline | TokenKind::NonLogicalNewline
        ) {
            prev_newline_token = Some(tok);
            // No need to do any further processing on newline tokens
            continue;
        }

        let is_start_of_physical_newline =
            num_newlines_in_whitespace > 0 || prev_newline_token.is_some();

        let token_indent = (tok.range.start()
            - logical_line.lines.locator.line_start(tok.range.start()))
        .to_usize();

        if is_start_of_physical_newline {
            // this is the beginning of a continuation line

            // record initial indent
            // TODO: A better way of doing this?
            relative_line_indent[line_no] =
                isize::try_from(token_indent).unwrap() - isize::try_from(indent_level).unwrap();

            // identify closing bracket
            let close_bracket = matches!(
                tok.kind(),
                TokenKind::Rpar | TokenKind::Rbrace | TokenKind::Rsqb
            );

            // is the indent relative to an opening bracket line?
            let mut hang = 0;
            let mut hanging_indent = false;
            for open_row in open_lines[bracket_depth].iter().rev() {
                hang = relative_line_indent[line_no] - relative_line_indent[*open_row];
                if valid_hang(hang, indent_width, indent_char) {
                    hanging_indent = true;
                    break;
                }
            }
            if hangs[bracket_depth].is_some() {
                hanging_indent = hangs[bracket_depth].is_some_and(|h| h == hang);
            }

            // is there any chance of visual indent?
            visual_indent = if !close_bracket && hang > 0 {
                indent_chances.get(&token_indent).cloned()
            } else {
                None
            };

            if close_bracket && bracket_indent_level[bracket_depth] > 0 {
                // closing bracket for visual indent
                if token_indent != bracket_indent_level[bracket_depth] {
                    let diagnostic = Diagnostic::new(
                        ClosingBracketMismatchedVisualIndent, // E124
                        tok.range,
                    );
                    context.push_diagnostic(diagnostic);
                }
            } else if close_bracket && hang == 0 {
                // closing bracket matches indentation of opening bracket's line
                if hang_closing {
                    let diagnostic = Diagnostic::new(
                        ClosingBracketMissingIndentation, // E133
                        tok.range,
                    );
                    context.push_diagnostic(diagnostic);
                }
            } else if bracket_indent_level[bracket_depth] > 0
                && token_indent < bracket_indent_level[bracket_depth]
            {
                if visual_indent != Some(VisualIndentType::Normal) {
                    let diagnostic = Diagnostic::new(
                        ContinuationUnderIndentedVisual, // E128
                        tok.range,
                    );
                    context.push_diagnostic(diagnostic);
                }
            } else if hanging_indent
                || (indent_next
                    // SAFETY: Lines are not that long
                    && relative_line_indent[line_no] == isize::try_from(indent_width.as_usize()).unwrap() * 2)
            {
                // hanging indent is verified
                if close_bracket && !hang_closing {
                    let diagnostic = Diagnostic::new(
                        ClosingBracketMismatched, // E123
                        tok.range,
                    );
                    context.push_diagnostic(diagnostic);
                }
                hangs[bracket_depth] = Some(hang);
            } else if visual_indent == Some(VisualIndentType::Normal) {
                // visual indent is verified
                bracket_indent_level[bracket_depth] = token_indent;
            } else if visual_indent == Some(VisualIndentType::String)
                || visual_indent == Some(VisualIndentType::Token(tok.kind()))
            {
                // ignore token lined up with string or matching token from a previous line
            } else {
                // indent is broken
                if hang <= 0 {
                    let diagnostic = Diagnostic::new(
                        ContinuationOverIndentedOrMissing, // E122
                        tok.range,
                    );
                    context.push_diagnostic(diagnostic);
                } else if bracket_indent_level[bracket_depth] > 0 {
                    let diagnostic = Diagnostic::new(
                        ContinuationOverIndentedVisual, // E127
                        tok.range,
                    );
                    context.push_diagnostic(diagnostic);
                } else if !close_bracket && hangs[bracket_depth].is_some() {
                    let diagnostic = Diagnostic::new(
                        ContinuationUnalignedHanging, // E131
                        tok.range,
                    );
                    context.push_diagnostic(diagnostic);
                } else {
                    hangs[bracket_depth] = Some(hang);
                    // SAFETY: Lines are short, not likely to overflow
                    if hang > isize::try_from(indent_width.as_usize()).unwrap() {
                        let diagnostic = Diagnostic::new(
                            ContinuationOverIndentedHanging, // E126
                            tok.range,
                        );
                        context.push_diagnostic(diagnostic);
                    } else {
                        let diagnostic = Diagnostic::new(
                            ContinuationUnderIndentedHanging, // E121
                            tok.range,
                        );
                        context.push_diagnostic(diagnostic);
                    }
                }
            }
            prev_newline_token = None;
        }

        if parens[line_no] > 0
            && tok.kind() != TokenKind::NonLogicalNewline
            && tok.kind() != TokenKind::Comment
            && bracket_indent_level[bracket_depth] == 0
        {
            // look for visual indenting
            bracket_indent_level[bracket_depth] = token_indent;
            indent_chances.insert(token_indent, VisualIndentType::Normal);
        } else if matches!(
            tok.kind(),
            TokenKind::String | TokenKind::Comment | TokenKind::FStringStart
        ) {
            // deal with implicit string concatenation
            indent_chances.insert(token_indent, VisualIndentType::String);
        } else if line_no == 0
            && bracket_depth == 0
            && matches!(
                tok.kind(),
                TokenKind::Assert | TokenKind::Raise | TokenKind::With
            )
        {
            indent_chances.insert(
                token_indent + tok.range.len().to_usize() + 1,
                VisualIndentType::Normal,
            );
        } else if indent_chances.is_empty()
            && line_no == 0
            && bracket_depth == 0
            && tok.kind() == TokenKind::If
        {
            // special case for the 'if' statement because len("if (") == 4
            indent_chances.insert(
                token_indent + tok.range.len().to_usize() + 1,
                VisualIndentType::Normal,
            );
        } else if tok.kind() == TokenKind::Colon
            && logical_line
                .text_after(tok)
                .chars()
                .all(char::is_whitespace)
        {
            // If there's only whitespace following this token, it can't be a dictionary construction
            // TODO: Is this always correct?
            open_lines[bracket_depth].push(line_no);
        }

        let open_bracket = matches!(
            tok.kind(),
            TokenKind::Lpar | TokenKind::Lbrace | TokenKind::Lsqb
        );
        let close_bracket = matches!(
            tok.kind(),
            TokenKind::Rpar | TokenKind::Rbrace | TokenKind::Rsqb
        );
        if open_bracket {
            bracket_depth += 1;
            bracket_indent_level.push(0);
            hangs.push(None);
            if open_lines.len() == bracket_depth {
                open_lines.push(vec![]);
            }
            open_lines[bracket_depth].push(line_no);
            parens[line_no] += 1;
        } else if close_bracket && bracket_depth > 0 {
            // parent indents should not be more than this one
            let line_indent_level = expand_indent(
                logical_line.lines.locator.full_line_str(tok.range.start()),
                indent_width,
            );
            let prev_indent = bracket_indent_level
                .pop()
                .filter(|n| *n > 0)
                .unwrap_or(line_indent_level);
            hangs.pop();
            bracket_indent_level
                .iter_mut()
                .take(bracket_depth)
                .filter(|ind| **ind > prev_indent)
                .for_each(|ind| *ind = 0);
            indent_chances.retain(|&ind, _| ind < prev_indent);
            open_lines.truncate(bracket_depth);
            bracket_depth -= 1;
            if bracket_depth > 0 {
                indent_chances.insert(
                    bracket_indent_level[bracket_depth],
                    VisualIndentType::Normal,
                );
            }
            for idx in (0..=line_no).rev() {
                if parens[idx] > 0 {
                    parens[idx] -= 1;
                    break;
                }
            }
        }
        if open_bracket || close_bracket {
            assert!(bracket_indent_level.len() == bracket_depth + 1);
            // allow lining up tokens if nothing else takes precedence
            indent_chances
                .entry(token_indent)
                .or_insert_with(|| VisualIndentType::Token(tok.kind()));
        }
    }

    {
        // SAFETY: There has to be a last token
        let last_token = logical_line.tokens_trimmed().last().unwrap();
        let last_phys_line = logical_line
            .lines
            .locator
            .full_line_str(last_token.range.start());
        let last_token_indent = expand_indent(last_phys_line, indent_width);

        if indent_next
            && last_token_indent == indent_level + (if indent_char == '\t' { 8 } else { 4 })
        {
            // Get the index of the first non-whitespace character
            let phys_line_nonwhitespace_start = logical_line
                .lines
                .locator
                .line_start(last_token.range.start())
                // SAFETY: Line length is not going to exceed u32 limits
                + TextSize::try_from(last_token_indent).unwrap();
            if visual_indent.is_some() {
                let diagnostic = Diagnostic::new(
                    VisualIndentMatchesNextLine, // E129
                    TextRange::new(phys_line_nonwhitespace_start, last_token.range.end()),
                );
                context.push_diagnostic(diagnostic);
            } else {
                let diagnostic = Diagnostic::new(
                    ContinuationMatchesNextLine, // E125
                    TextRange::new(phys_line_nonwhitespace_start, last_token.range.end()),
                );
                context.push_diagnostic(diagnostic);
            }
        }
    }
}
