//! Algorithmic mapping for hangul syllables.

// derived from Jamo.txt
pub static CHOSEONG: &[&str] = &[
    "G", "GG", "N", "D", "DD", "R", "M", "B", "BB", "S", "SS", "", "J", "JJ", "C", "K", "T", "P",
    "H",
];
pub static JUNGSEONG: &[&str] = &[
    "A", "AE", "YA", "YAE", "EO", "E", "YEO", "YE", "O", "WA", "WAE", "OE", "YO", "U", "WEO", "WE",
    "WI", "YU", "EU", "YI", "I",
];
pub static JONGSEONG: &[&str] = &[
    "", "G", "GG", "GS", "N", "NJ", "NH", "D", "L", "LG", "LM", "LB", "LS", "LT", "LP", "LH", "M",
    "B", "BS", "S", "SS", "NG", "J", "C", "K", "T", "P", "H",
];

pub fn is_hangul_syllable(c: char) -> bool {
    ('\u{AC00}'..='\u{D7A3}').contains(&c)
}

pub fn syllable_decomposition(c: char) -> Option<(u8, u8, u8)> {
    if is_hangul_syllable(c) {
        let n = c as u32 - 0xAC00;
        // break this into the various parts.
        let jongseong = n % 28;
        let jungseong = (n / 28) % 21;
        let choseong = n / (28 * 21);
        Some((choseong as u8, jungseong as u8, jongseong as u8))
    } else {
        // outside the range
        None
    }
}

fn slice_shift_byte(a: &[u8]) -> (Option<u8>, &[u8]) {
    if a.is_empty() {
        (None, a)
    } else {
        (Some(a[0]), &a[1..])
    }
}

pub fn slice_shift_choseong(name: &[u8]) -> (Option<u32>, &[u8]) {
    match slice_shift_byte(name) {
        (Some(b'G'), name) => match slice_shift_byte(name) {
            (Some(b'G'), name) => (Some(1), name),
            (_, _) => (Some(0), name),
        },
        (Some(b'N'), name) => (Some(2), name),
        (Some(b'D'), name) => match slice_shift_byte(name) {
            (Some(b'D'), name) => (Some(4), name),
            (_, _) => (Some(3), name),
        },
        (Some(b'R'), name) => (Some(5), name),
        (Some(b'M'), name) => (Some(6), name),
        (Some(b'B'), name) => match slice_shift_byte(name) {
            (Some(b'B'), name) => (Some(8), name),
            (_, _) => (Some(7), name),
        },
        (Some(b'S'), name) => match slice_shift_byte(name) {
            (Some(b'S'), name) => (Some(10), name),
            (_, _) => (Some(9), name),
        },
        (Some(b'J'), name) => match slice_shift_byte(name) {
            (Some(b'J'), name) => (Some(13), name),
            (_, _) => (Some(12), name),
        },
        (Some(b'C'), name) => (Some(14), name),
        (Some(b'K'), name) => (Some(15), name),
        (Some(b'T'), name) => (Some(16), name),
        (Some(b'P'), name) => (Some(17), name),
        (Some(b'H'), name) => (Some(18), name),
        (_, _) => (Some(11), name),
    }
}

pub fn slice_shift_jungseong(name: &[u8]) -> (Option<u32>, &[u8]) {
    match slice_shift_byte(name) {
        (Some(b'A'), name) => match slice_shift_byte(name) {
            (Some(b'E'), name) => (Some(1), name),
            (_, _) => (Some(0), name),
        },
        (Some(b'Y'), name) => match slice_shift_byte(name) {
            (Some(b'A'), name) => match slice_shift_byte(name) {
                (Some(b'E'), name) => (Some(3), name),
                (_, _) => (Some(2), name),
            },
            (Some(b'E'), name) => match slice_shift_byte(name) {
                (Some(b'O'), name) => (Some(6), name),
                (_, _) => (Some(7), name),
            },
            (Some(b'O'), name) => (Some(12), name),
            (Some(b'U'), name) => (Some(17), name),
            (Some(b'I'), name) => (Some(19), name),
            (_, _) => (None, name),
        },
        (Some(b'E'), name) => match slice_shift_byte(name) {
            (Some(b'O'), name) => (Some(4), name),
            (Some(b'U'), name) => (Some(18), name),
            (_, _) => (Some(5), name),
        },
        (Some(b'O'), name) => match slice_shift_byte(name) {
            (Some(b'E'), name) => (Some(11), name),
            (_, _) => (Some(8), name),
        },
        (Some(b'W'), name) => match slice_shift_byte(name) {
            (Some(b'A'), name) => match slice_shift_byte(name) {
                (Some(b'E'), name) => (Some(10), name),
                (_, _) => (Some(9), name),
            },
            (Some(b'E'), name) => match slice_shift_byte(name) {
                (Some(b'O'), name) => (Some(14), name),
                (_, _) => (Some(15), name),
            },
            (Some(b'I'), name) => (Some(16), name),
            (_, _) => (None, name),
        },
        (Some(b'U'), name) => (Some(13), name),
        (Some(b'I'), name) => (Some(20), name),
        (_, _) => (None, name),
    }
}

pub fn slice_shift_jongseong(name: &[u8]) -> (Option<u32>, &[u8]) {
    match slice_shift_byte(name) {
        (Some(b'G'), name) => match slice_shift_byte(name) {
            (Some(b'G'), name) => (Some(2), name),
            (Some(b'S'), name) => (Some(3), name),
            (_, _) => (Some(1), name),
        },
        (Some(b'N'), name) => match slice_shift_byte(name) {
            (Some(b'J'), name) => (Some(5), name),
            (Some(b'H'), name) => (Some(6), name),
            (Some(b'G'), name) => (Some(21), name),
            (_, _) => (Some(4), name),
        },
        (Some(b'D'), name) => (Some(7), name),
        (Some(b'L'), name) => match slice_shift_byte(name) {
            (Some(b'G'), name) => (Some(9), name),
            (Some(b'M'), name) => (Some(10), name),
            (Some(b'B'), name) => (Some(11), name),
            (Some(b'S'), name) => (Some(12), name),
            (Some(b'T'), name) => (Some(13), name),
            (Some(b'P'), name) => (Some(14), name),
            (Some(b'H'), name) => (Some(15), name),
            (_, _) => (Some(8), name),
        },
        (Some(b'M'), name) => (Some(16), name),
        (Some(b'B'), name) => match slice_shift_byte(name) {
            (Some(b'S'), name) => (Some(18), name),
            (_, _) => (Some(17), name),
        },
        (Some(b'S'), name) => match slice_shift_byte(name) {
            (Some(b'S'), name) => (Some(20), name),
            (_, _) => (Some(19), name),
        },
        (Some(b'J'), name) => (Some(22), name),
        (Some(b'C'), name) => (Some(23), name),
        (Some(b'K'), name) => (Some(24), name),
        (Some(b'T'), name) => (Some(25), name),
        (Some(b'P'), name) => (Some(26), name),
        (Some(b'H'), name) => (Some(27), name),
        (_, _) => (Some(0), name),
    }
}

#[cfg(test)]
mod tests {
    use std::prelude::v1::*;

    #[test]
    fn correct_slice_shift_choseong() {
        for (i, &choseong) in super::CHOSEONG.iter().enumerate() {
            assert_eq!(
                super::slice_shift_choseong(choseong.as_bytes()),
                (Some(i as u32), b"" as &[u8])
            );
        }
    }

    #[test]
    fn correct_slice_shift_jungseong() {
        for (i, &jungseong) in super::JUNGSEONG.iter().enumerate() {
            assert_eq!(
                super::slice_shift_jungseong(jungseong.as_bytes()),
                (Some(i as u32), b"" as &[u8])
            );
        }

        // there is no empty jungseong
        assert_eq!(super::slice_shift_jungseong(b""), (None, b"" as &[u8]));
    }

    #[test]
    fn correct_slice_shift_jongseong() {
        for (i, &jongseong) in super::JONGSEONG.iter().enumerate() {
            assert_eq!(
                super::slice_shift_jongseong(jongseong.as_bytes()),
                (Some(i as u32), b"" as &[u8])
            );
        }
    }
}
