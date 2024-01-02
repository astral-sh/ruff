use crate::TokenKind;

/// A bit-set of `TokenKind`s
#[derive(Clone, Copy)]
pub(crate) struct TokenSet(u128);

impl TokenSet {
    pub(crate) const EMPTY: TokenSet = TokenSet(0);

    pub(crate) const fn new(kinds: &[TokenKind]) -> TokenSet {
        let mut res = 0u128;
        let mut i = 0;
        while i < kinds.len() {
            res |= mask(kinds[i]);
            i += 1;
        }
        TokenSet(res)
    }

    pub(crate) const fn union(self, other: TokenSet) -> TokenSet {
        TokenSet(self.0 | other.0)
    }

    pub(crate) const fn remove(self, kind: TokenKind) -> TokenSet {
        TokenSet(self.0 & !mask(kind))
    }

    pub(crate) const fn contains(&self, kind: TokenKind) -> bool {
        self.0 & mask(kind) != 0
    }
}

const fn mask(kind: TokenKind) -> u128 {
    1u128 << (kind as usize)
}

impl From<&[TokenKind]> for TokenSet {
    fn from(value: &[TokenKind]) -> Self {
        TokenSet::new(value)
    }
}

#[test]
fn token_set_works_for_tokens() {
    use crate::TokenKind::*;
    let mut ts = TokenSet::new(&[EndOfFile, Name]);
    assert!(ts.contains(EndOfFile));
    assert!(ts.contains(Name));
    assert!(!ts.contains(Plus));
    ts = ts.remove(Name);
    assert!(!ts.contains(Name));
}
