use crate::generated::{
    LEXICON, LEXICON_OFFSETS, LEXICON_ORDERED_LENGTHS, LEXICON_ORDERED_LENGTHS_LEN,
    LEXICON_SHORT_LENGTHS, PHRASEBOOK, PHRASEBOOK_SHORT,
};

#[derive(Clone)]
struct PhrasebookIter {
    index: u32,
}

impl PhrasebookIter {
    fn empty() -> Self {
        Self {
            index: PHRASEBOOK.len() as u32,
        }
    }
}

impl Iterator for PhrasebookIter {
    type Item = u8;
    fn next(&mut self) -> Option<Self::Item> {
        let b = *PHRASEBOOK.get(self.index as usize)?;
        self.index += 1;
        Some(b)
    }
}

#[derive(Clone)]
pub struct IterStr {
    phrasebook: PhrasebookIter,
    last_was_word: bool,
}

impl IterStr {
    pub fn new(start_index: u32) -> IterStr {
        IterStr {
            phrasebook: PhrasebookIter { index: start_index },
            last_was_word: false,
        }
    }
}

const HYPHEN: u8 = 127;

/// An array where `arr[i]` holds the largest lexicon index with length `i`.
static LEXICON_ORDERED_LENGTH_INDICES: [u16; LEXICON_ORDERED_LENGTHS_LEN] = {
    let mut arr = [0u16; LEXICON_ORDERED_LENGTHS_LEN];

    let mut prev_len = None;
    let mut i = 0;
    while i < LEXICON_ORDERED_LENGTHS.len() {
        let (end_idx, length) = LEXICON_ORDERED_LENGTHS[i];

        // make sure this is contiguous - that there are no gaps where e.g. there
        // are words of length 15 and of length 17 but none of length 16.
        if let Some(prev_len) = prev_len {
            assert!(length == prev_len + 1);
        }
        prev_len = Some(length);

        assert!(end_idx <= u16::MAX as usize);
        arr[i] = end_idx as u16 - 1;
        i += 1;
    }

    arr
};

impl Iterator for IterStr {
    type Item = &'static str;
    fn next(&mut self) -> Option<&'static str> {
        let mut tmp = self.phrasebook.clone();
        let raw_b = tmp.next()?;
        // the first byte includes if it is the last in this name
        // in the high bit.
        let (is_end, b) = (raw_b & 0b1000_0000 != 0, raw_b & 0b0111_1111);

        let ret = if b == HYPHEN {
            // have to handle this before the case below, because a -
            // replaces the space entirely.
            self.last_was_word = false;
            "-"
        } else if self.last_was_word {
            self.last_was_word = false;
            // early return, we don't want to update the
            // phrasebook (i.e. we're pretending we didn't touch
            // this byte).
            return Some(" ");
        } else {
            self.last_was_word = true;

            let (length, idx) = if b < PHRASEBOOK_SHORT {
                let idx = b as usize;
                // these lengths are hard-coded
                (LEXICON_SHORT_LENGTHS[idx] as usize, idx)
            } else {
                let idx = u16::from_be_bytes([b - PHRASEBOOK_SHORT, tmp.next().unwrap()]);

                // The value at each index `i` in the array `LEXICON_ORDERED_LENGTH_INDICES`
                // (herein referred to as `arr`) is the largest lexicon index with length `i`.
                let length = match LEXICON_ORDERED_LENGTH_INDICES.binary_search(&idx) {
                    // In this case, `idx` is equal to the index at `arr[i]`,
                    // so `i` is the correct length.
                    Ok(i) => i,
                    // `binary_search(idx)` returning `Err(i)` means that `arr[i-1] < idx < arr[i]`.
                    // Therefore, `idx` is larger than the largest index with length `i - 1`, but
                    // smaller than the largest index with length `i`, meaning its length is `i`.
                    Err(i) => i,
                };

                (length, idx as usize)
            };
            let offset = LEXICON_OFFSETS[idx] as usize;
            &LEXICON[offset..offset + length]
        };
        self.phrasebook = if is_end { PhrasebookIter::empty() } else { tmp };
        Some(ret)
    }
}
