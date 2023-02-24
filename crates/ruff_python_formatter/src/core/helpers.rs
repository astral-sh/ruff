use crate::core::locator::Locator;
use crate::core::types::Range;
use rustpython_parser::ast::Location;

/// Return the leading quote for a string or byte literal (e.g., `"""`).
pub fn leading_quote(content: &str) -> Option<&str> {
    if let Some(first_line) = content.lines().next() {
        for pattern in ruff_python::str::TRIPLE_QUOTE_PREFIXES
            .iter()
            .chain(ruff_python::bytes::TRIPLE_QUOTE_PREFIXES)
            .chain(ruff_python::str::SINGLE_QUOTE_PREFIXES)
            .chain(ruff_python::bytes::SINGLE_QUOTE_PREFIXES)
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
    ruff_python::str::TRIPLE_QUOTE_SUFFIXES
        .iter()
        .chain(ruff_python::str::SINGLE_QUOTE_SUFFIXES)
        .find(|&pattern| content.ends_with(pattern))
}

pub fn is_radix_literal(content: &str) -> bool {
    content.starts_with("0b")
        || content.starts_with("0o")
        || content.starts_with("0x")
        || content.starts_with("0B")
        || content.starts_with("0O")
        || content.starts_with("0X")
}

pub fn expand_indented_block(
    location: Location,
    end_location: Location,
    locator: &Locator,
) -> Location {
    let contents = locator.contents();
    let index = locator.index(end_location);
    let offset = contents[index..]
        .lines()
        .skip(1)
        .take_while(|line| {
            line.chars()
                .take(location.column())
                .all(char::is_whitespace)
        })
        .count();
    Location::new(end_location.row() + 1 + offset, 0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_prefixes() {
        let prefixes = ruff_python::str::TRIPLE_QUOTE_PREFIXES
            .iter()
            .chain(ruff_python::bytes::TRIPLE_QUOTE_PREFIXES)
            .chain(ruff_python::str::SINGLE_QUOTE_PREFIXES)
            .chain(ruff_python::bytes::SINGLE_QUOTE_PREFIXES)
            .collect::<Vec<_>>();
        for i in 1..prefixes.len() {
            for j in 0..i - 1 {
                if i != j {
                    if prefixes[i].starts_with(prefixes[j]) {
                        assert!(
                            !prefixes[i].starts_with(prefixes[j]),
                            "Prefixes are not unique: {} starts with {}",
                            prefixes[i],
                            prefixes[j]
                        );
                    }
                }
            }
        }
    }
}
