use indexmap::IndexSet;
use ruff_rowan::{Direction, Language, SyntaxNode, SyntaxToken, TextSize};

/// Tracks the ranges of the formatted (including replaced or tokens formatted as verbatim) tokens.
///
/// This implementation uses the fact that no two tokens can have an overlapping range to avoid the need for an interval tree.
/// Thus, testing if a token has already been formatted only requires testing if a token starting at the same offset has been formatted.
#[derive(Debug, Clone, Default)]
pub struct PrintedTokens {
    /// Key: Start of a token's range
    offsets: IndexSet<TextSize>,
    disabled: bool,
}

#[derive(Copy, Clone)]
pub struct PrintedTokensSnapshot {
    len: usize,
    disabled: bool,
}

impl PrintedTokens {
    /// Tracks a formatted token
    ///
    /// ## Panics
    /// If this token has been formatted before.
    pub fn track_token<L: Language>(&mut self, token: &SyntaxToken<L>) {
        if self.disabled {
            return;
        }

        let range = token.text_trimmed_range();

        if !self.offsets.insert(range.start()) {
            panic!("You tried to print the token '{token:?}' twice, and this is not valid.");
        }
    }

    /// Enables or disables the assertion tracking
    pub(crate) fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    pub(crate) fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub(crate) fn snapshot(&self) -> PrintedTokensSnapshot {
        PrintedTokensSnapshot {
            len: self.offsets.len(),
            disabled: self.disabled,
        }
    }

    pub(crate) fn restore(&mut self, snapshot: PrintedTokensSnapshot) {
        let PrintedTokensSnapshot { len, disabled } = snapshot;

        self.offsets.truncate(len);
        self.disabled = disabled
    }

    /// Asserts that all tokens of the passed in node have been tracked
    ///
    /// ## Panics
    /// If any descendant token of `root` hasn't been tracked
    pub fn assert_all_tracked<L: Language>(&self, root: &SyntaxNode<L>) {
        let mut offsets = self.offsets.clone();

        for token in root.descendants_tokens(Direction::Next) {
            if !offsets.remove(&token.text_trimmed_range().start()) {
                panic!("token has not been seen by the formatter: {token:#?}.\
                        \nUse `format_replaced` if you want to replace a token from the formatted output.\
                        \nUse `format_removed` if you want to remove a token from the formatted output.\n\
                        parent: {:#?}", token.parent())
            }
        }

        for offset in offsets {
            panic!("tracked offset {offset:?} doesn't match any token of {root:#?}. Have you passed a token from another tree?");
        }
    }
}
