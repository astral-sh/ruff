/// Figure out whether we need `u8` (1 byte), `u16` (2 bytes) or `u32` (4 bytes) to store all
/// numbers. Returns the number of bytes
pub fn smallest_type<I: Iterator<Item = u32>>(x: I) -> usize {
    let n = x.max().unwrap_or(0);
    for (max, bytes) in [(u8::MAX as u32, 1), (u16::MAX as u32, 2)] {
        if n <= max {
            return bytes;
        }
    }
    4
}

pub fn smallest_u<I: Iterator<Item = u32>>(x: I) -> String {
    format!("u{}", 8 * smallest_type(x))
}
pub fn split<'a, 'b>(s: &'a str, splitters: &'b [u8]) -> Split<'a, 'b> {
    Split {
        s,
        splitters,
        pending: "",
        done: false,
    }
}

pub struct Split<'a, 'b> {
    s: &'a str,
    splitters: &'b [u8],
    pending: &'a str,
    done: bool,
}
impl<'a> Iterator for Split<'a, '_> {
    type Item = &'a str;
    fn next(&mut self) -> Option<&'a str> {
        if self.done {
            return None;
        }
        if self.s.is_empty() {
            self.done = true;
            return Some("");
        }
        if !self.pending.is_empty() {
            return Some(std::mem::take(&mut self.pending));
        }

        for (i, b) in self.s.bytes().enumerate() {
            if b == b' ' || self.splitters.contains(&b) {
                let ret = &self.s[..i];
                // dont include the space, but include everything else on the next step
                if b != b' ' {
                    self.pending = &self.s[i..i + 1]
                }
                self.s = &self.s[i + 1..];
                return Some(ret);
            }
        }
        // trailing data
        self.done = true;
        Some(self.s)
    }
}

#[test]
fn test_split() {
    let tests: &[(&str, &[&str])] = &[
        ("a", &["a"]),
        ("a b", &["a", "b"]),
        (" a b ", &["", "a", "b", ""]),
        ("a-b", &["a", "-", "b"]),
        ("a- b", &["a", "-", "", "b"]),
        ("a -b", &["a", "", "-", "b"]),
    ];

    for &(s, v) in tests.iter() {
        assert_eq!(split(s, b"-").collect::<Vec<&str>>(), v)
    }
}
