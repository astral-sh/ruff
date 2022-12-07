//! Implements helper functions for using vendored/format.rs
use std::convert::TryFrom;
use std::fmt;

use rustc_hash::FxHashSet;

use crate::vendored::format::{
    FieldName, FieldType, FormatParseError, FormatPart, FormatString, FromTemplate,
};

impl fmt::Display for FormatParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            FormatParseError::EmptyAttribute => "Empty attribute in format string",
            FormatParseError::InvalidCharacterAfterRightBracket => {
                "Only '.' or '[' may follow ']' in format field specifier"
            }
            FormatParseError::InvalidFormatSpecifier => "Max string recursion exceeded",
            FormatParseError::MissingStartBracket => "Single '}' encountered in format string",
            FormatParseError::MissingRightBracket => "Expected '}' before end of string",
            FormatParseError::UnmatchedBracket => "Single '{' encountered in format string",
            _ => "Unexpected error parsing format string",
        };

        write!(f, "{message}")
    }
}

pub(crate) struct FormatSummary {
    pub autos: FxHashSet<usize>,
    pub indexes: FxHashSet<usize>,
    pub keywords: FxHashSet<String>,
}

impl TryFrom<&str> for FormatSummary {
    type Error = FormatParseError;

    fn try_from(literal: &str) -> Result<Self, Self::Error> {
        let format_string = FormatString::from_str(literal)?;

        let mut autos = FxHashSet::default();
        let mut indexes = FxHashSet::default();
        let mut keywords = FxHashSet::default();

        for format_part in format_string.format_parts {
            let FormatPart::Field {
                field_name,
                format_spec,
                ..
            } = format_part else {
                continue;
            };
            let parsed = FieldName::parse(&field_name)?;
            match parsed.field_type {
                FieldType::Auto => autos.insert(autos.len()),
                FieldType::Index(i) => indexes.insert(i),
                FieldType::Keyword(k) => keywords.insert(k),
            };

            let nested = FormatString::from_str(&format_spec)?;
            for nested_part in nested.format_parts {
                let FormatPart::Field { field_name, .. } = nested_part else {
                    continue;
                };
                let parsed = FieldName::parse(&field_name)?;
                match parsed.field_type {
                    FieldType::Auto => autos.insert(autos.len()),
                    FieldType::Index(i) => indexes.insert(i),
                    FieldType::Keyword(k) => keywords.insert(k),
                };
            }
        }

        Ok(FormatSummary {
            autos,
            indexes,
            keywords,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vendored::format::FromTemplate;

    #[test]
    fn test_format_summary() {
        let literal = "foo{foo}a{}b{2}c{2}d{1}{}{}e{bar}{foo}f{spam}";

        let expected_autos = [0usize, 1usize, 2usize].into_iter().collect();
        let expected_indexes = [1usize, 2usize].into_iter().collect();
        let expected_keywords = ["foo", "bar", "spam"]
            .into_iter()
            .map(String::from)
            .collect();

        let format_summary = FormatSummary::try_from(literal).unwrap();

        assert_eq!(format_summary.autos, expected_autos);
        assert_eq!(format_summary.indexes, expected_indexes);
        assert_eq!(format_summary.keywords, expected_keywords);
    }

    #[test]
    fn test_format_summary_nested() {
        let literal = "foo{foo}a{:{}{}}b{2:{3}{4}}c{2}d{1}{}e{bar:{spam}{eggs}}";

        let expected_autos = [0usize, 1usize, 2usize, 3usize].into_iter().collect();
        let expected_indexes = [1usize, 2usize, 3usize, 4usize].into_iter().collect();
        let expected_keywords = ["foo", "bar", "spam", "eggs"]
            .into_iter()
            .map(String::from)
            .collect();

        let format_summary = FormatSummary::try_from(literal).unwrap();

        assert_eq!(format_summary.autos, expected_autos);
        assert_eq!(format_summary.indexes, expected_indexes);
        assert_eq!(format_summary.keywords, expected_keywords);
    }

    #[test]
    fn test_format_summary_invalid() {
        assert!(FormatSummary::try_from("{").is_err());

        let literal = "{foo}a{}b{bar..}";
        assert!(FormatString::from_str(literal).is_ok());
        assert!(FormatSummary::try_from(literal).is_err());

        let literal_nested = "{foo}a{}b{bar:{spam..}}";
        assert!(FormatString::from_str(literal_nested).is_ok());
        assert!(FormatSummary::try_from(literal_nested).is_err());
    }
}
