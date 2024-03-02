//! Extract docstrings via tokenization.
//!
//! See: <https://github.com/zheller/flake8-quotes/blob/ef0d9a90249a080e460b70ab62bf4b65e5aa5816/flake8_quotes/docstring_detection.py#L29>
//!
//! TODO(charlie): Consolidate with the existing AST-based docstring extraction.

use ruff_python_parser::TokenKind;

#[derive(Default, Copy, Clone)]
enum State {
    // Start of the module: first string gets marked as a docstring.
    #[default]
    ExpectModuleDocstring,
    // After seeing a class definition, we're waiting for the block colon (and do bracket
    // counting).
    ExpectClassColon,
    // After seeing the block colon in a class definition, we expect a docstring.
    ExpectClassDocstring,
    // Same as ExpectClassColon, but for function definitions.
    ExpectFunctionColon,
    // Same as ExpectClassDocstring, but for function definitions.
    ExpectFunctionDocstring,
    // Skip tokens until we observe a `class` or `def`.
    Other,
}

#[derive(Default)]
pub(crate) struct StateMachine {
    state: State,
    bracket_count: usize,
}

impl StateMachine {
    pub(crate) fn consume(&mut self, tok: TokenKind) -> bool {
        match tok {
            TokenKind::NonLogicalNewline
            | TokenKind::Newline
            | TokenKind::Indent
            | TokenKind::Dedent
            | TokenKind::Comment => false,

            TokenKind::String => {
                if matches!(
                    self.state,
                    State::ExpectModuleDocstring
                        | State::ExpectClassDocstring
                        | State::ExpectFunctionDocstring
                ) {
                    self.state = State::Other;
                    true
                } else {
                    false
                }
            }
            TokenKind::Class => {
                self.state = State::ExpectClassColon;
                self.bracket_count = 0;

                false
            }

            TokenKind::Def => {
                self.state = State::ExpectFunctionColon;
                self.bracket_count = 0;

                false
            }

            TokenKind::Colon => {
                if self.bracket_count == 0 {
                    if matches!(self.state, State::ExpectClassColon) {
                        self.state = State::ExpectClassDocstring;
                    } else if matches!(self.state, State::ExpectFunctionColon) {
                        self.state = State::ExpectFunctionDocstring;
                    }
                }

                false
            }

            TokenKind::Lpar | TokenKind::Lbrace | TokenKind::Lsqb => {
                self.bracket_count = self.bracket_count.saturating_add(1);
                if matches!(
                    self.state,
                    State::ExpectModuleDocstring
                        | State::ExpectClassDocstring
                        | State::ExpectFunctionDocstring
                ) {
                    self.state = State::Other;
                }
                false
            }

            TokenKind::Rpar | TokenKind::Rbrace | TokenKind::Rsqb => {
                self.bracket_count = self.bracket_count.saturating_sub(1);
                if matches!(
                    self.state,
                    State::ExpectModuleDocstring
                        | State::ExpectClassDocstring
                        | State::ExpectFunctionDocstring
                ) {
                    self.state = State::Other;
                }

                false
            }

            _ => {
                if matches!(
                    self.state,
                    State::ExpectModuleDocstring
                        | State::ExpectClassDocstring
                        | State::ExpectFunctionDocstring
                ) {
                    self.state = State::Other;
                }

                false
            }
        }
    }
}
