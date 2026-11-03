use ruff_python_ast::token::TokenKind;

use super::{ASCII_IDENTIFIER_CONTINUE, KeywordTable, is_ascii_identifier_start};

pub(super) static KEYWORD_TABLE: KeywordTable = KeywordTable::new(KEYWORDS);
pub(super) const KEYWORD_HASH_MULTIPLIER: u64 = find_multiplier();

const KEYWORDS: &[(&[u8], TokenKind)] = &[
    (b"False", TokenKind::False),
    (b"None", TokenKind::None),
    (b"True", TokenKind::True),
    (b"and", TokenKind::And),
    (b"as", TokenKind::As),
    (b"assert", TokenKind::Assert),
    (b"async", TokenKind::Async),
    (b"await", TokenKind::Await),
    (b"break", TokenKind::Break),
    (b"case", TokenKind::Case),
    (b"class", TokenKind::Class),
    (b"continue", TokenKind::Continue),
    (b"def", TokenKind::Def),
    (b"del", TokenKind::Del),
    (b"elif", TokenKind::Elif),
    (b"else", TokenKind::Else),
    (b"except", TokenKind::Except),
    (b"finally", TokenKind::Finally),
    (b"for", TokenKind::For),
    (b"from", TokenKind::From),
    (b"global", TokenKind::Global),
    (b"if", TokenKind::If),
    (b"import", TokenKind::Import),
    (b"in", TokenKind::In),
    (b"is", TokenKind::Is),
    (b"lazy", TokenKind::Lazy),
    (b"lambda", TokenKind::Lambda),
    (b"match", TokenKind::Match),
    (b"nonlocal", TokenKind::Nonlocal),
    (b"not", TokenKind::Not),
    (b"or", TokenKind::Or),
    (b"pass", TokenKind::Pass),
    (b"raise", TokenKind::Raise),
    (b"return", TokenKind::Return),
    (b"try", TokenKind::Try),
    (b"type", TokenKind::Type),
    (b"while", TokenKind::While),
    (b"with", TokenKind::With),
    (b"yield", TokenKind::Yield),
];

/// Packs a keyword in little-endian order at the high end of a word.
pub(super) const fn pack(keyword: &[u8]) -> u64 {
    assert!(
        keyword.len() >= 2 && keyword.len() <= 8,
        "keywords must contain two to eight ASCII bytes"
    );
    assert!(
        is_ascii_identifier_start(keyword[0] as char),
        "keywords must be ASCII identifiers"
    );
    let mut word = 0;
    let mut i = 0;
    while i < keyword.len() {
        let byte = keyword[i];
        assert!(
            ASCII_IDENTIFIER_CONTINUE[byte as usize],
            "keywords must be ASCII identifiers"
        );
        word = (word >> 8) | ((byte as u64) << 56);
        i += 1;
    }
    word
}

/// Uses the high six bits of the product to select one of 64 slots.
pub(super) const fn slot(word: u64, multiplier: u64) -> usize {
    (word.wrapping_mul(multiplier) >> 58) as usize
}

/// An `n`-byte keyword ends in `8 - n` zero bytes once packed, so its slot
/// depends only on the low `n` bytes of the multiplier. Choose those bytes
/// from low to high. Each new byte leaves all shorter keywords in their slots.
const fn find_multiplier() -> u64 {
    assert!(
        KEYWORDS.len() <= 64,
        "too many keywords for a 64-slot table"
    );
    // Zero pads each group because an ASCII identifier packs to a nonzero word.
    let mut by_length = [[0; KEYWORDS.len()]; 7];
    let mut keywords = KEYWORDS;
    while let Some((&(keyword, _), rest)) = keywords.split_first() {
        let word = pack(keyword);
        let group = &mut by_length[keyword.len() - 2];
        let mut i = 0;
        while group[i] != 0 {
            assert!(group[i] != word, "duplicate keyword spelling");
            i += 1;
        }
        group[i] = word;
        keywords = rest;
    }

    // If no byte places the whole group, start over with the next odd low byte.
    let mut seed = 1;
    'seeds: while seed < 256 {
        let mut multiplier = seed;
        seed += 2;
        let mut occupied = 0;
        let mut len = 2;
        while len <= 8 {
            let mut byte = 0;
            loop {
                if byte == 256 {
                    continue 'seeds;
                }
                let candidate = multiplier | (byte << (8 * (len - 1)));
                if let Some(slots) = place_keywords(&by_length[len - 2], candidate, occupied) {
                    multiplier = candidate;
                    occupied = slots;
                    break;
                }
                byte += 1;
            }
            len += 1;
        }
        return multiplier;
    }
    panic!("no multiplier found for the 64-slot keyword table");
}

/// Tries to place every keyword of this length alongside the shorter keywords.
const fn place_keywords(mut words: &[u64], multiplier: u64, mut occupied: u64) -> Option<u64> {
    while let Some((&word, rest)) = words.split_first()
        && word != 0
    {
        let bit = 1 << slot(word, multiplier);
        if occupied & bit != 0 {
            return None;
        }
        occupied |= bit;
        words = rest;
    }
    Some(occupied)
}
