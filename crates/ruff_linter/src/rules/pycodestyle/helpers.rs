use ruff_python_parser::TokenKind;

/// Returns `true` if the name should be considered "ambiguous".
pub(super) fn is_ambiguous_name(name: &str) -> bool {
    name == "l" || name == "I" || name == "O"
}

/// Returns `true` if the given `token` is a non-logical token.
///
/// Unlike [`TokenKind::is_trivia`], this function also considers the indent, dedent and newline
/// tokens.
pub(super) const fn is_non_logical_token(token: TokenKind) -> bool {
    token.is_trivia()
        || matches!(
            token,
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
        )
}
