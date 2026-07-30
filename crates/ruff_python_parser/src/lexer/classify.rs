#![expect(unsafe_code, reason = "the SIMD classifier uses bounded vector loads")]

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::{
    uint8x16_t, vandq_u8, vdupq_n_u8, vgetq_lane_u64, vld1q_u8, vmaxvq_u8, vorrq_u8, vpaddq_u8,
    vqtbl1q_u8, vreinterpretq_u64_u8, vshrq_n_u8, vtstq_u8,
};

/// Structural starts for one source batch. Each set bit begins a word run, whitespace run, or
/// single structural byte; `ascii_source` lets the carver avoid per-identifier Unicode checks.
#[derive(Debug, Default)]
pub(super) struct Classified {
    pub(super) starts: Vec<u64>,
    pub(super) ascii_source: bool,
}

/// Intersecting the low- and high-nibble entries classifies a byte. Bits 0..=4 are ASCII word
/// ranges, bits 5..=6 are horizontal whitespace, and bit 7 marks non-ASCII word bytes.
#[cfg(target_arch = "aarch64")]
const LOW_NIBBLE_CLASSES: [u8; 16] = [
    213, 159, 159, 159, 159, 159, 159, 159, 159, 191, 158, 138, 170, 138, 138, 142,
];
#[cfg(target_arch = "aarch64")]
const HIGH_NIBBLE_CLASSES: [u8; 16] = [
    32, 0, 64, 1, 2, 4, 8, 16, 128, 128, 128, 128, 128, 128, 128, 128,
];

#[cfg(test)]
pub(super) fn classify(source: &[u8]) -> Classified {
    let mut classified = Classified::default();
    classify_into(source, &mut classified);
    classified
}

/// Reuses `classified` to identify token boundaries across the complete source batch.
pub(super) fn classify_into(source: &[u8], classified: &mut Classified) {
    let blocks = source.len().div_ceil(64);
    classified.starts.clear();
    classified.starts.reserve(blocks);
    classified.ascii_source = true;
    let mut previous_word = 0;
    let mut previous_whitespace = 0;

    let mut chunks = source.chunks_exact(64);
    let mut source_or = unsafe { vdupq_n_u8(0) };

    for chunk in &mut chunks {
        // SAFETY: `chunks_exact(64)` guarantees that all four 16-byte loads are in bounds.
        let (word, whitespace, chunk_or) = unsafe { classify_chunk(chunk.as_ptr()) };
        // SAFETY: All NEON operations are valid for arbitrary byte vectors.
        source_or = unsafe { vorrq_u8(source_or, chunk_or) };
        push_block(
            classified,
            word,
            whitespace,
            &mut previous_word,
            &mut previous_whitespace,
            u64::MAX,
        );
    }

    // SAFETY: All NEON operations are valid for arbitrary byte vectors.
    classified.ascii_source = unsafe { vmaxvq_u8(source_or) < 0x80 };

    let tail = chunks.remainder();
    if !tail.is_empty() {
        let mut word = 0;
        let mut whitespace = 0;
        for (bit, byte) in tail.iter().copied().enumerate() {
            let is_non_ascii = !byte.is_ascii();
            classified.ascii_source &= !is_non_ascii;
            if byte.is_ascii_alphanumeric() || byte == b'_' || is_non_ascii {
                word |= 1 << bit;
            } else if matches!(byte, b' ' | b'\t' | b'\x0c') {
                whitespace |= 1 << bit;
            }
        }
        let valid = (1 << tail.len()) - 1;
        push_block(
            classified,
            word,
            whitespace,
            &mut previous_word,
            &mut previous_whitespace,
            valid,
        );
    }
}

/// Converts word and whitespace masks into structural starts while carrying runs across blocks.
fn push_block(
    classified: &mut Classified,
    word: u64,
    whitespace: u64,
    previous_word: &mut u64,
    previous_whitespace: &mut u64,
    valid: u64,
) {
    let word_starts = word & !((word << 1) | *previous_word);
    let whitespace_starts = whitespace & !((whitespace << 1) | *previous_whitespace);
    let other = !(word | whitespace) & valid;

    classified
        .starts
        .push(word_starts | whitespace_starts | other);

    *previous_word = word >> 63;
    *previous_whitespace = whitespace >> 63;
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn classify_chunk(source: *const u8) -> (u64, u64, uint8x16_t) {
    // SAFETY: The caller guarantees that `source..source + 64` is valid.
    unsafe {
        let first = vld1q_u8(source);
        let second = vld1q_u8(source.add(16));
        let third = vld1q_u8(source.add(32));
        let fourth = vld1q_u8(source.add(48));
        let bits = vld1q_u8([1_u8, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128].as_ptr());

        let low = vld1q_u8(LOW_NIBBLE_CLASSES.as_ptr());
        let high = vld1q_u8(HIGH_NIBBLE_CLASSES.as_ptr());
        let (word_first, whitespace_first) = classify_bytes(first, low, high);
        let (word_second, whitespace_second) = classify_bytes(second, low, high);
        let (word_third, whitespace_third) = classify_bytes(third, low, high);
        let (word_fourth, whitespace_fourth) = classify_bytes(fourth, low, high);

        let word = mask64(bits, word_first, word_second, word_third, word_fourth);
        let whitespace = mask64(
            bits,
            whitespace_first,
            whitespace_second,
            whitespace_third,
            whitespace_fourth,
        );
        let source_or = vorrq_u8(vorrq_u8(first, second), vorrq_u8(third, fourth));
        (word, whitespace, source_or)
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn classify_bytes(
    bytes: uint8x16_t,
    low: uint8x16_t,
    high: uint8x16_t,
) -> (uint8x16_t, uint8x16_t) {
    // SAFETY: All NEON operations are valid for arbitrary byte vectors.
    unsafe {
        let low = vqtbl1q_u8(low, vandq_u8(bytes, vdupq_n_u8(0x0f)));
        let high = vqtbl1q_u8(high, vshrq_n_u8::<4>(bytes));
        let classes = vandq_u8(low, high);
        let word = vtstq_u8(classes, vdupq_n_u8(0x9f));
        let whitespace = vtstq_u8(classes, vdupq_n_u8(0x60));
        (word, whitespace)
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn mask64(
    bits: uint8x16_t,
    first: uint8x16_t,
    second: uint8x16_t,
    third: uint8x16_t,
    fourth: uint8x16_t,
) -> u64 {
    // SAFETY: All NEON operations are valid for arbitrary predicate vectors.
    unsafe {
        let first = vandq_u8(first, bits);
        let second = vandq_u8(second, bits);
        let third = vandq_u8(third, bits);
        let fourth = vandq_u8(fourth, bits);
        let first = vpaddq_u8(first, second);
        let second = vpaddq_u8(third, fourth);
        let result = vpaddq_u8(first, second);
        vgetq_lane_u64::<0>(vreinterpretq_u64_u8(vpaddq_u8(result, result)))
    }
}

#[cfg(test)]
mod tests {
    use super::classify;

    fn assert_matches_scalar(source: &[u8]) {
        let classified = classify(source);
        let mut expected_starts = vec![0; source.len().div_ceil(64)];
        let mut previous_word = false;
        let mut previous_whitespace = false;

        for (offset, byte) in source.iter().copied().enumerate() {
            let word = byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii();
            let whitespace = matches!(byte, b' ' | b'\t' | b'\x0c');
            let block = offset / 64;
            let bit = 1 << (offset % 64);

            if (!word && !whitespace)
                || (word && !previous_word)
                || (whitespace && !previous_whitespace)
            {
                expected_starts[block] |= bit;
            }

            previous_word = word;
            previous_whitespace = whitespace;
        }

        assert_eq!(classified.starts, expected_starts);
        assert_eq!(classified.ascii_source, source.is_ascii());
    }

    #[test]
    fn every_boundary_bit() {
        for bit in 0..64 {
            let mut source = [b' '; 64];
            source[bit] = b'a';
            assert_matches_scalar(&source);

            let mut source = [b'a'; 64];
            source[bit] = b'+';
            assert_matches_scalar(&source);
        }
    }

    #[test]
    fn runs_cross_chunk_boundary() {
        for offset in [63, 64, 65] {
            for byte in *b"a \t\x0c" {
                let mut source = vec![b'+'; offset];
                source.extend(std::iter::repeat_n(byte, 65));
                source.push(b'+');
                assert_matches_scalar(&source);
            }
        }

        for length in [63, 64, 65] {
            assert_matches_scalar(&vec![b'a'; length]);
            assert_matches_scalar(&vec![b' '; length]);
        }
    }

    #[test]
    fn punctuation_and_newlines_start_tokens() {
        assert_matches_scalar(b"a+-*/=()[]{},.:;@!<>\r\nb");
    }

    #[test]
    fn non_ascii_is_part_of_a_word_run() {
        let source = "before \u{03bb}\u{53d8}\u{91cf}_2 + after".as_bytes();
        assert_matches_scalar(source);
    }

    #[test]
    fn non_ascii_is_detected_at_chunk_boundaries() {
        for offset in [0, 15, 16, 31, 32, 47, 48, 63, 64, 65, 127, 128, 129] {
            let mut source = vec![b'a'; offset];
            source.push(0x80);
            source.extend(std::iter::repeat_n(b'a', 65));
            assert_matches_scalar(&source);
        }
    }

    #[test]
    fn all_byte_values_match_scalar() {
        let bytes: Vec<_> = (0..=u8::MAX).collect();
        for offset in 0..64 {
            let mut source = vec![b'+'; offset];
            source.extend(&bytes);
            assert_matches_scalar(&source);
        }
    }
}
