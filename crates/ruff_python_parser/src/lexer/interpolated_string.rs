#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{
    _mm_and_si128, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_or_si128, _mm_set1_epi8,
};

use ruff_python_ast::StringFlags;

use crate::string::InterpolatedStringKind;

use super::TokenFlags;

/// The context representing the current f-string or t-string that the lexer is in.
#[derive(Clone, Debug)]
pub(crate) struct InterpolatedStringContext {
    flags: TokenFlags,

    /// The level of nesting for the lexer when it entered the current f/t-string.
    /// The nesting level includes all kinds of parentheses i.e., round, square,
    /// and curly.
    nesting: u32,

    /// The current depth of format spec for the current f/t-string. This is because
    /// there can be multiple format specs nested for the same f-string.
    /// For example, `{a:{b:{c}}}` has 3 format specs.
    format_spec_depth: u32,
}

impl InterpolatedStringContext {
    pub(crate) const fn new(flags: TokenFlags, nesting: u32) -> Option<Self> {
        if flags.is_interpolated_string() {
            Some(Self {
                flags,
                nesting,
                format_spec_depth: 0,
            })
        } else {
            None
        }
    }

    pub(crate) fn kind(&self) -> InterpolatedStringKind {
        if self.flags.is_f_string() {
            InterpolatedStringKind::FString
        } else if self.flags.is_t_string() {
            InterpolatedStringKind::TString
        } else {
            unreachable!("Can only be constructed when f-string or t-string flag is present")
        }
    }

    pub(crate) const fn flags(&self) -> TokenFlags {
        self.flags
    }

    pub(crate) const fn nesting(&self) -> u32 {
        self.nesting
    }

    /// Returns the quote character for the current f-string.
    pub(crate) fn quote_char(&self) -> char {
        self.flags.quote_style().as_char()
    }

    /// Returns the triple quotes for the current f-string if it is a triple-quoted
    /// f-string, `None` otherwise.
    pub(crate) fn triple_quotes(&self) -> Option<&'static str> {
        if self.is_triple_quoted() {
            Some(self.flags.quote_str())
        } else {
            None
        }
    }

    /// Returns `true` if the current f-string is a raw f-string.
    pub(crate) fn is_raw_string(&self) -> bool {
        self.flags.is_raw_string()
    }

    /// Returns `true` if the current f-string is a triple-quoted f-string.
    pub(crate) fn is_triple_quoted(&self) -> bool {
        self.flags.is_triple_quoted()
    }

    /// Calculates the number of open parentheses for the current f-string
    /// based on the current level of nesting for the lexer.
    const fn open_parentheses_count(&self, current_nesting: u32) -> u32 {
        current_nesting.saturating_sub(self.nesting)
    }

    /// Returns `true` if the lexer is in an f-string expression or t-string interpolation i.e., between
    /// two curly braces.
    pub(crate) const fn is_in_interpolation(&self, current_nesting: u32) -> bool {
        self.open_parentheses_count(current_nesting) > self.format_spec_depth
    }

    /// Returns `true` if the lexer is in a f-string format spec i.e., after a colon.
    pub(crate) const fn is_in_format_spec(&self, current_nesting: u32) -> bool {
        self.format_spec_depth > 0 && !self.is_in_interpolation(current_nesting)
    }

    /// Returns `true` if the context is in a valid position to start format spec
    /// i.e., at the same level of nesting as the opening parentheses token.
    /// Increments the format spec depth if it is.
    ///
    /// This assumes that the current character for the lexer is a colon (`:`).
    pub(crate) fn try_start_format_spec(&mut self, current_nesting: u32) -> bool {
        if self
            .open_parentheses_count(current_nesting)
            .saturating_sub(self.format_spec_depth)
            == 1
        {
            self.format_spec_depth += 1;
            true
        } else {
            false
        }
    }

    /// Decrements the format spec depth if the current f-string is in a format
    /// spec.
    pub(crate) fn try_end_format_spec(&mut self, current_nesting: u32) {
        if self.is_in_format_spec(current_nesting) {
            self.format_spec_depth = self.format_spec_depth.saturating_sub(1);
        }
    }
}

/// The interpolated strings stack is used to keep track of all the f-strings and t-strings that the
/// lexer encounters. This is necessary because f-strings and t-strings can be nested.
#[derive(Debug, Default)]
pub(crate) struct InterpolatedStrings {
    stack: Vec<InterpolatedStringContext>,
}

impl InterpolatedStrings {
    pub(crate) fn push(&mut self, context: InterpolatedStringContext) {
        self.stack.push(context);
    }

    pub(crate) fn pop(&mut self) -> Option<InterpolatedStringContext> {
        self.stack.pop()
    }

    pub(crate) fn current(&self) -> Option<&InterpolatedStringContext> {
        self.stack.last()
    }

    pub(crate) fn current_mut(&mut self) -> Option<&mut InterpolatedStringContext> {
        self.stack.last_mut()
    }

    pub(crate) fn checkpoint(&self) -> InterpolatedStringsCheckpoint {
        InterpolatedStringsCheckpoint(self.stack.clone())
    }

    pub(crate) fn rewind(&mut self, checkpoint: InterpolatedStringsCheckpoint) {
        self.stack = checkpoint.0;
    }
}

#[derive(Debug, Clone)]
pub(crate) struct InterpolatedStringsCheckpoint(Vec<InterpolatedStringContext>);

/// Finds the next byte that requires the interpolated-string state machine.
/// Only ASCII bytes stop the scan, including when a UTF-8 codepoint spans two chunks.
#[cfg(target_arch = "x86_64")]
// Keep the vector loop out of the hot `Lexer::next_token` function.
#[inline(never)]
#[expect(
    unsafe_code,
    reason = "SSE2 is guaranteed on x86-64 and every load is bounded by a 16-byte chunk"
)]
pub(super) fn literal_text_length(source: &[u8], quote: u8, triple_quoted: bool) -> usize {
    // SAFETY: SSE2 is part of the x86-64 baseline.
    unsafe { literal_text_length_sse2(source, quote, triple_quoted) }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[expect(
    unsafe_code,
    reason = "every vector load is bounded by a 16-byte chunk"
)]
unsafe fn literal_text_length_sse2(source: &[u8], quote: u8, triple_quoted: bool) -> usize {
    let quote_vector = _mm_set1_epi8(quote.cast_signed());
    let open = _mm_set1_epi8(b'{'.cast_signed());
    let close = _mm_set1_epi8(b'}'.cast_signed());
    let escape = _mm_set1_epi8(b'\\'.cast_signed());
    let cr = _mm_set1_epi8(b'\r'.cast_signed());
    let lf = _mm_set1_epi8(b'\n'.cast_signed());
    let newline_mask = _mm_set1_epi8(if triple_quoted { 0 } else { -1 });
    let (chunks, tail) = source.as_chunks::<16>();
    for (index, chunk) in chunks.iter().enumerate() {
        // SAFETY: The chunk contains all sixteen bytes read by this unaligned load.
        let bytes = unsafe { _mm_loadu_si128(chunk.as_ptr().cast()) };
        let quote_or_escape = _mm_or_si128(
            _mm_cmpeq_epi8(bytes, quote_vector),
            _mm_cmpeq_epi8(bytes, escape),
        );
        let braces = _mm_or_si128(_mm_cmpeq_epi8(bytes, open), _mm_cmpeq_epi8(bytes, close));
        let newlines = _mm_and_si128(
            newline_mask,
            _mm_or_si128(_mm_cmpeq_epi8(bytes, cr), _mm_cmpeq_epi8(bytes, lf)),
        );
        let mask = _mm_movemask_epi8(_mm_or_si128(
            _mm_or_si128(quote_or_escape, braces),
            newlines,
        ));
        if mask != 0 {
            return index * 16 + mask.trailing_zeros() as usize;
        }
    }
    tail.iter()
        .position(|byte| {
            *byte == quote
                || matches!(byte, b'{' | b'}' | b'\\')
                || (!triple_quoted && matches!(byte, b'\r' | b'\n'))
        })
        .map_or(source.len(), |index| chunks.len() * 16 + index)
}

/// Finds the next delimiter with bounded searches on other targets.
#[cfg(not(target_arch = "x86_64"))]
#[inline(never)]
pub(super) fn literal_text_length(source: &[u8], quote: u8, triple_quoted: bool) -> usize {
    let mut offset = 0;
    for chunk in source.chunks(64) {
        let special = memchr::memchr3(b'{', b'}', b'\\', chunk).unwrap_or(chunk.len());
        let delimiter = if triple_quoted {
            memchr::memchr(quote, &chunk[..special])
        } else {
            memchr::memchr3(quote, b'\r', b'\n', &chunk[..special])
        };
        if let Some(delimiter) = delimiter {
            return offset + delimiter;
        }
        if special != chunk.len() {
            return offset + special;
        }
        offset += chunk.len();
    }
    source.len()
}

#[cfg(test)]
mod literal_text_tests {
    use super::literal_text_length;

    #[test]
    fn bulk_scan_matches_scalar_at_chunk_boundaries() {
        for &quote in b"'\"" {
            for triple in [false, true] {
                for length in 0..130 {
                    for special in [quote, b'{', b'}', b'\\', b'\n', b'\r', 0, 0xc3, 0xa9] {
                        let mut source = vec![b'a'; length];
                        source.push(special);
                        source.extend_from_slice(b"ordinary tail");
                        let expected = source
                            .iter()
                            .position(|byte| {
                                *byte == quote
                                    || matches!(byte, b'{' | b'}' | b'\\')
                                    || (!triple && matches!(byte, b'\n' | b'\r'))
                            })
                            .unwrap_or(source.len());
                        assert_eq!(literal_text_length(&source, quote, triple), expected);
                    }
                }
            }
        }
    }
}
