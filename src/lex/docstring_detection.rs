//! Extract docstrings via tokenization.
//!
//! See: <https://github.com/zheller/flake8-quotes/blob/ef0d9a90249a080e460b70ab62bf4b65e5aa5816/flake8_quotes/docstring_detection.py#L29>
//!
//! TODO(charlie): Consolidate with the existing AST-based docstring extraction.

use rustpython_parser::lexer::Tok;

#[derive(Debug)]
enum State {
    // Start of the module: first string gets marked as a docstring.
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

pub struct StateMachine {
    state: State,
    bracket_count: usize,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl StateMachine {
    pub const fn new() -> Self {
        Self {
            state: State::ExpectModuleDocstring,
            bracket_count: 0,
        }
    }

    pub fn consume(&mut self, tok: &Tok) -> bool {
        if matches!(
            tok,
            Tok::NonLogicalNewline | Tok::Newline | Tok::Indent | Tok::Dedent | Tok::Comment(..)
        ) {
            return false;
        }

        if matches!(tok, Tok::String { .. }) {
            return if matches!(
                self.state,
                State::ExpectModuleDocstring
                    | State::ExpectClassDocstring
                    | State::ExpectFunctionDocstring
            ) {
                self.state = State::Other;
                true
            } else {
                false
            };
        }

        if matches!(tok, Tok::Class) {
            self.state = State::ExpectClassColon;
            self.bracket_count = 0;
            return false;
        }

        if matches!(tok, Tok::Def) {
            self.state = State::ExpectFunctionColon;
            self.bracket_count = 0;
            return false;
        }

        if matches!(tok, Tok::Colon) {
            if self.bracket_count == 0 {
                if matches!(self.state, State::ExpectClassColon) {
                    self.state = State::ExpectClassDocstring;
                } else if matches!(self.state, State::ExpectFunctionColon) {
                    self.state = State::ExpectFunctionDocstring;
                }
            }
            return false;
        }

        if matches!(tok, Tok::Lpar | Tok::Lbrace | Tok::Lsqb) {
            self.bracket_count += 1;
            if matches!(
                self.state,
                State::ExpectModuleDocstring
                    | State::ExpectClassDocstring
                    | State::ExpectFunctionDocstring
            ) {
                self.state = State::Other;
            }
            return false;
        }

        if matches!(tok, Tok::Rpar | Tok::Rbrace | Tok::Rsqb) {
            self.bracket_count -= 1;
            if matches!(
                self.state,
                State::ExpectModuleDocstring
                    | State::ExpectClassDocstring
                    | State::ExpectFunctionDocstring
            ) {
                self.state = State::Other;
            }
            return false;
        }

        if matches!(
            self.state,
            State::ExpectModuleDocstring
                | State::ExpectClassDocstring
                | State::ExpectFunctionDocstring
        ) {
            self.state = State::Other;
            return false;
        }

        false
    }
}
