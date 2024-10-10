use crate::checkers::logical_lines::LogicalLinesContext;
use crate::rules::pycodestyle::rules::logical_lines::TextRange;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::TokenKind;
use rustc_hash::FxHashSet;

use super::LogicalLine;

/// ## What it does
/// A line is less indented than it should be for hanging indents.
///
/// ## Why is this bad?
/// TODO
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
#[violation]
pub struct ContinuationUnderIndentedHanging;

impl Violation for ContinuationUnderIndentedHanging {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continuation line under-indented for hanging indent")
    }
}

/// ## What it does
/// A continuation line is not indented as far as it should be or is indented too far.
///
/// ## Why is this bad?
/// TODO
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
#[violation]
pub struct ContinuationOverIndentedOrMissing;

impl Violation for ContinuationOverIndentedOrMissing {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continuation line missing indentation or outdented")
    }
}

/// ## What it does
/// Closing brackets should match the same indentation level of the line that their opening bracket started on.
///
/// ## Why is this bad?
/// TODO
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
#[violation]
pub struct ClosingBracketMismatched;

impl Violation for ClosingBracketMismatched {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Closing bracket does not match indentation of opening bracket's line")
    }
}

/// ## What it does
/// Closing brackets should match the indentation of the opening bracket.
///
/// ## Why is this bad?
/// TODO
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
#[violation]
pub struct ClosingBracketMismatchedVisualIndent;

impl Violation for ClosingBracketMismatchedVisualIndent {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Closing bracket does not match visual indentation")
    }
}

/// ## What it does
/// Continuation lines should not be indented at the same level as the next logical line.
/// Instead, they should be indented to one more level so as to distinguish them from the next line.
///
/// ## Why is this bad?
/// TODO
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
#[violation]
pub struct ContinuationMatchesNextLine;

impl Violation for ContinuationMatchesNextLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continuation line with same indent as next logical line")
    }
}

/// ## What it does
/// A continuation line is indented farther than it should be for a hanging indent.
///
/// ## Why is this bad?
/// TODO
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
#[violation]
pub struct ContinuationOverIndentedHanging;

impl Violation for ContinuationOverIndentedHanging {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continuation line over-indented for hanging indent")
    }
}

/// ## What it does
/// A continuation line is indented farther than it should be for a visual indent.
///
/// ## Why is this bad?
/// TODO
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
#[violation]
pub struct ContinuationOverIndentedVisual;

impl Violation for ContinuationOverIndentedVisual {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continuation line over-indented for visual indent")
    }
}

/// ## What it does
/// A continuation line is under-indented for a visual indentation.
///
/// ## Why is this bad?
/// TODO
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
#[violation]
pub struct ContinuationUnderIndentedVisual;

impl Violation for ContinuationUnderIndentedVisual {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continuation line under-indented for visual indent")
    }
}

/// ## What it does
/// A visual indented line has the same indentation as the next logical line.
///
/// ## Why is this bad?
/// TODO
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
#[violation]
pub struct VisualIndentMatchesNextLine;

impl Violation for VisualIndentMatchesNextLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Visually indented line with same indent as next logical line")
    }
}

/// ## What it does
/// A continuation line is unaligned for hanging indent.
///
/// ## Why is this bad?
/// TODO
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
#[violation]
pub struct ContinuationUnalignedHanging;

impl Violation for ContinuationUnalignedHanging {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continuation line unaligned for hanging indent")
    }
}

/// ## What it does
/// A closing bracket is missing indentation.
/// This error only occurs if the --hang-closing flag is used, switching the default behavior of closing brackets so that they require hanging indents.
///
/// ## Why is this bad?
/// TODO
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
#[violation]
pub struct ClosingBracketMissingIndentation;

impl Violation for ClosingBracketMissingIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Closing bracket is missing indentation")
    }
}

// E121, E122, E123, E124, E125, E126, E127, E128, E129, E131, E133
pub(crate) fn continued_indentation(
    logical_line: &LogicalLine,
    indent_level: usize,
    indent_char: char,
    hang_closing: bool,
    context: &mut LogicalLinesContext,
) {
    //println!("LOGICAL_LINE {}", logical_line.text()); // TODO
    let num_lines = logical_line
        .tokens()
        .iter()
        .map(|t| {
            logical_line
                .lines
                .locator
                .slice(t.range)
                .chars()
                .filter(|c| *c == '\n')
                .count()
        })
        .sum::<usize>()
        + 1;

    if num_lines == 1 {
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
    let allow_double_hang_indent = indent_char == '\t';

    // remember how many brackets were opened on each line
    let mut parens = vec![0; num_lines];

    // relative indents of physical lines
    let mut rel_indent = vec![0; num_lines];

    // for each bracket_depth collect a list of opening rows
    let mut open_lines = vec![vec![0]];

    // for each bracket_depth, memorize the hanging indentation
    let mut hangs = vec![None];

    // visual indents
    let mut indent_chances: FxHashSet<usize> = FxHashSet::default();
    let mut visual_indent = false;
    let mut prev_newline_token = None;
    // for each bracket_depth, memorize the visual indent column
    let mut indent = vec![0];

    for tok in logical_line.tokens() {
        if tok.kind() == TokenKind::NonLogicalNewline || tok.kind() == TokenKind::Newline {
            prev_newline_token = Some(tok);
            line_no += 1;
            continue;
        }

        // Strings can contain newlines too
        let num_lines = logical_line
            .lines
            .locator
            .slice(tok.range)
            .chars()
            .filter(|c| *c == '\n')
            .count();
        line_no += num_lines;

        let is_start_of_newline = prev_newline_token.is_some();

        // Newlines have width, so use end to make sure indents are 1-based
        let token_indent = (tok.range.start()
            - logical_line.lines.locator.line_start(tok.range.start()))
        .to_usize();
        //dbg!(tok, token_indent); //TODO

        if is_start_of_newline {
            // this is the beginning of a continuation line

            // record initial indent
            // TODO: A better way of doing this?
            rel_indent[line_no] =
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
                hang = rel_indent[line_no] - rel_indent[*open_row];
                if hang == 4 || (allow_double_hang_indent && hang == 8) {
                    hanging_indent = true;
                    break;
                }
            }
            if hangs[bracket_depth].is_some() {
                hanging_indent = hangs[bracket_depth].is_some_and(|h| h == hang);
            }

            // is there any chance of visual indent?
            visual_indent = !close_bracket && hang > 0 && indent_chances.contains(&token_indent);
            //dbg!(hanging_indent, visual_indent); // TODO

            if close_bracket && indent[bracket_depth] > 0 {
                // closing bracket for visual indent
                if token_indent != indent[bracket_depth] {
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
            } else if indent[bracket_depth] > 0 && token_indent < indent[bracket_depth] {
                if !visual_indent {
                    let diagnostic = Diagnostic::new(
                        ContinuationUnderIndentedVisual, // E128
                        tok.range,
                    );
                    context.push_diagnostic(diagnostic);
                }
            } else if hanging_indent || (indent_next && rel_indent[line_no] == 8) {
                // hanging indent is verified
                if close_bracket && !hang_closing {
                    let diagnostic = Diagnostic::new(
                        ClosingBracketMismatched, // E123
                        tok.range,
                    );
                    context.push_diagnostic(diagnostic);
                }
                hangs[bracket_depth] = Some(hang);
            } else if visual_indent {
                // visual indent is verified
                indent[bracket_depth] = token_indent;
            }
            //else if visual_indent == token_indent {
            //    // ignore token lined up with matching one from a previous line
            //}
            else {
                //dbg!(hang); // TODO
                // indent is broken
                if hang <= 0 {
                    let diagnostic = Diagnostic::new(
                        ContinuationOverIndentedOrMissing, // E122
                        tok.range,
                    );
                    context.push_diagnostic(diagnostic);
                } else if indent[bracket_depth] > 0 {
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
                    if hang > 4 {
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
        }

        if parens[line_no] > 0
            && tok.kind() != TokenKind::NonLogicalNewline
            && tok.kind() != TokenKind::Comment
            && indent[bracket_depth] == 0
        {
            // look for visual indenting
            indent[bracket_depth] = token_indent;
            indent_chances.insert(token_indent);
        } else if matches!(
            tok.kind(),
            TokenKind::String | TokenKind::Comment | TokenKind::FStringStart
        ) {
            // deal with implicit string concatenation
            indent_chances.insert(token_indent);
        } else if line_no == 0
            && bracket_depth == 0
            && matches!(
                tok.kind(),
                TokenKind::Assert | TokenKind::Raise | TokenKind::With
            )
        {
            indent_chances.insert(token_indent); // TODO
        } else if indent_chances.is_empty()
            && line_no == 0
            && bracket_depth == 0
            && tok.kind() == TokenKind::If
        {
            // special case for the 'if' statement because len("if (") == 4
            indent_chances.insert(token_indent); // TODO
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
            indent.push(0);
            hangs.push(None);
            if open_lines.len() == bracket_depth {
                open_lines.push(vec![]);
            }
            open_lines[bracket_depth].push(line_no);
            parens[line_no] += 1;
        } else if close_bracket && bracket_depth > 0 {
            // parent indents should not be more than this one
            let prev_indent = indent.pop().unwrap_or(token_indent);
            hangs.pop();
            for ind in &mut indent {
                if *ind > prev_indent {
                    *ind = 0;
                }
            }
            indent_chances.retain(|ind| *ind < prev_indent);
            open_lines.truncate(bracket_depth);
            bracket_depth -= 1;
            if bracket_depth > 0 {
                indent_chances.insert(indent[bracket_depth]);
            }
            for idx in (0..=line_no).rev() {
                if parens[idx] > 0 {
                    parens[idx] -= 1;
                    break;
                }
            }
        }
        if open_bracket || close_bracket {
            assert!(indent.len() == bracket_depth + 1);
            if !indent_chances.contains(&token_indent) {
                // allow lining up tokens
                indent_chances.insert(token_indent);
            }
        }
        prev_newline_token = None;
    }

    if indent_next
        && prev_newline_token.is_some_and(|t| {
            logical_line.trailing_whitespace(t).1.to_usize()
                == indent_level + (if indent_char == '\t' { 8 } else { 4 })
        })
    {
        if visual_indent {
            let diagnostic = Diagnostic::new(
                VisualIndentMatchesNextLine, // E129
                TextRange::new(
                    prev_newline_token.unwrap().range.end(),
                    logical_line.last_token().unwrap().range.start(),
                ),
            );
            context.push_diagnostic(diagnostic);
        } else {
            let diagnostic = Diagnostic::new(
                ContinuationMatchesNextLine, // E125
                TextRange::new(
                    prev_newline_token.unwrap().range.end(),
                    logical_line.last_token().unwrap().range.start(),
                ),
            );
            context.push_diagnostic(diagnostic);
        }
    }
}
