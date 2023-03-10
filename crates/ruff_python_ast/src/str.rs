// use std::str::Chars;

/// See: <https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals>
const TRIPLE_QUOTE_STR_PREFIXES: &[&str] = &[
    "u\"\"\"", "u'''", "r\"\"\"", "r'''", "U\"\"\"", "U'''", "R\"\"\"", "R'''", "\"\"\"", "'''",
];
const SINGLE_QUOTE_STR_PREFIXES: &[&str] = &[
    "u\"", "u'", "r\"", "r'", "U\"", "U'", "R\"", "R'", "\"", "'",
];
pub const TRIPLE_QUOTE_BYTE_PREFIXES: &[&str] = &[
    "br'''", "rb'''", "bR'''", "Rb'''", "Br'''", "rB'''", "RB'''", "BR'''", "b'''", "br\"\"\"",
    "rb\"\"\"", "bR\"\"\"", "Rb\"\"\"", "Br\"\"\"", "rB\"\"\"", "RB\"\"\"", "BR\"\"\"", "b\"\"\"",
    "B\"\"\"",
];
pub const SINGLE_QUOTE_BYTE_PREFIXES: &[&str] = &[
    "br'", "rb'", "bR'", "Rb'", "Br'", "rB'", "RB'", "BR'", "b'", "br\"", "rb\"", "bR\"", "Rb\"",
    "Br\"", "rB\"", "RB\"", "BR\"", "b\"", "B\"",
];
const TRIPLE_QUOTE_SUFFIXES: &[&str] = &["\"\"\"", "'''"];
const SINGLE_QUOTE_SUFFIXES: &[&str] = &["\"", "'"];

/// Strip the leading and trailing quotes from a docstring.
pub fn raw_contents(contents: &str) -> &str {
    for pattern in TRIPLE_QUOTE_STR_PREFIXES
        .iter()
        .chain(TRIPLE_QUOTE_BYTE_PREFIXES)
    {
        if contents.starts_with(pattern) {
            return &contents[pattern.len()..contents.len() - 3];
        }
    }
    for pattern in SINGLE_QUOTE_STR_PREFIXES
        .iter()
        .chain(SINGLE_QUOTE_BYTE_PREFIXES)
    {
        if contents.starts_with(pattern) {
            return &contents[pattern.len()..contents.len() - 1];
        }
    }
    unreachable!("Expected docstring to start with a valid triple- or single-quote prefix")
}

/// Return the leading quote for a string or byte literal (e.g., `"""`).
pub fn leading_quote(content: &str) -> Option<&str> {
    if let Some(first_line) = content.lines().next() {
        for pattern in TRIPLE_QUOTE_STR_PREFIXES
            .iter()
            .chain(TRIPLE_QUOTE_BYTE_PREFIXES)
            .chain(SINGLE_QUOTE_STR_PREFIXES)
            .chain(SINGLE_QUOTE_BYTE_PREFIXES)
        {
            if first_line.starts_with(pattern) {
                return Some(pattern);
            }
        }
    }
    None
}

/// Return the trailing quote string for a string or byte literal (e.g., `"""`).
pub fn trailing_quote(content: &str) -> Option<&&str> {
    TRIPLE_QUOTE_SUFFIXES
        .iter()
        .chain(SINGLE_QUOTE_SUFFIXES)
        .find(|&pattern| content.ends_with(pattern))
}

/// Return `true` if the string is a triple-quote string or byte prefix.
pub fn is_triple_quote(content: &str) -> bool {
    TRIPLE_QUOTE_STR_PREFIXES.contains(&content) || TRIPLE_QUOTE_BYTE_PREFIXES.contains(&content)
}

// pub struct UnescapedDocStringChar {
//     value: Option<char>,      // the char after unescaped
//     consumed: u8,             // the number of characters consumed in the original string
//     encounter_new_line: bool, // whether process a new line character in the unescaping process
// }

// /// Return an unescaped char
// pub fn unescaped_docstring_char(chars: &mut Chars<'_>) -> Option<UnescapedDocStringChar> {
//     let c = chars.next()?;
//     let mut consumed = 1_u8;
//     let mut encounter_new_line = false;

//     let res = match c {
//         '\\' => {
//             // must have at least one character after it
//             // otherwise, it will be rejected by the parser
//             let res = match chars.next().unwrap() {
//                 // placeholder ignore new lines
//                 '\n' => None,
//                 '\\' => Some('\\'),
//                 '\'' => Some('\''),
//                 '"' => Some('"'),
//                 'a' => Some(0x07 as char),
//                 'b' => Some(0x08 as char),
//                 'f' => Some(0x0c as char),
//                 'n' => Some('\n'),
//                 'r' => Some('\r'),
//                 't' => Some('\t'),
//                 'v' => Some(0x0b as char),
//                 'x' => {
//                     // must have one valid hex number with 2 characters after it
//                     // otherwise, it will be rejected by the parser
//                     let hi = chars.next().unwrap();
//                     let hi = hi.to_digit(16)?;
//                     let lo = chars.next().unwrap();
//                     let lo = lo.to_digit(16).unwrap();
//                     let value = hi * 16 + lo;
//                     Some(value as u8 as char)
//                 }

//                 // ignore u and U because they can cause problems with utf-8 encoding
//                 // as python allows you to use surrogate inside docstring
//                 // (as soon as you don't print it)
//                 // so ignore them for now
//                 // 'u' => {
//                 //     let hex_str: String = chars.take(4).collect();
//                 //     let value = u32::from_str_radix(hex_str.as_str(), 16).unwrap();
//                 // }

//                 c => Some(c),
//             };
//             res
//         }
//         _ => Some(c),
//     };

//     Some(UnescapedDocStringChar {
//         value: res,
//         consumed: consumed,
//         encounter_new_line: encounter_new_line,
//     })
// }

#[cfg(test)]
mod tests {
    use super::{
        SINGLE_QUOTE_BYTE_PREFIXES, SINGLE_QUOTE_STR_PREFIXES, TRIPLE_QUOTE_BYTE_PREFIXES,
        TRIPLE_QUOTE_STR_PREFIXES,
    };

    #[test]
    fn test_prefixes() {
        let prefixes = TRIPLE_QUOTE_STR_PREFIXES
            .iter()
            .chain(TRIPLE_QUOTE_BYTE_PREFIXES)
            .chain(SINGLE_QUOTE_STR_PREFIXES)
            .chain(SINGLE_QUOTE_BYTE_PREFIXES)
            .collect::<Vec<_>>();
        for (i, prefix_i) in prefixes.iter().enumerate() {
            for (j, prefix_j) in prefixes.iter().enumerate() {
                if i > j {
                    assert!(
                        !prefix_i.starts_with(*prefix_j),
                        "Prefixes are not unique: {prefix_i} starts with {prefix_j}",
                    );
                }
            }
        }
    }
}
