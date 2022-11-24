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

impl TryFrom<FormatString> for FormatSummary {
    type Error = FormatParseError;

    fn try_from(format_string: FormatString) -> Result<Self, Self::Error> {
        let mut autos = FxHashSet::default();
        let mut indexes = FxHashSet::default();
        let mut keywords = FxHashSet::default();

        for format_part in format_string.format_parts {
            if let FormatPart::Field {
                field_name,
                format_spec,
                ..
            } = format_part
            {
                match FieldName::parse(&field_name) {
                    Ok(parsed) => {
                        match parsed.field_type {
                            FieldType::Auto => autos.insert(autos.len()),
                            FieldType::Index(i) => indexes.insert(i),
                            FieldType::Keyword(k) => keywords.insert(k),
                        };
                    }
                    Err(e) => return Err(e),
                }

                match FormatString::from_str(&format_spec) {
                    Ok(nested) => {
                        for nested_part in nested.format_parts {
                            if let FormatPart::Field { field_name, .. } = nested_part {
                                match FieldName::parse(&field_name) {
                                    Ok(parsed) => {
                                        match parsed.field_type {
                                            FieldType::Auto => autos.insert(autos.len()),
                                            FieldType::Index(i) => indexes.insert(i),
                                            FieldType::Keyword(k) => keywords.insert(k),
                                        };
                                    }
                                    Err(e) => return Err(e),
                                }
                            }
                        }
                    }
                    Err(e) => return Err(e),
                }
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

        let format_string = FormatString::from_str(literal).unwrap();
        let format_summary = FormatSummary::try_from(format_string).unwrap();

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

        let format_string = FormatString::from_str(literal).unwrap();
        let format_summary = FormatSummary::try_from(format_string).unwrap();

        assert_eq!(format_summary.autos, expected_autos);
        assert_eq!(format_summary.indexes, expected_indexes);
        assert_eq!(format_summary.keywords, expected_keywords);
    }

    #[test]
    fn test_format_summary_invalid() {
        let literal = "{foo}a{}b{bar..}";
        let format_string = FormatString::from_str(literal).unwrap();
        assert!(FormatSummary::try_from(format_string).is_err());

        let literal_nested = "{foo}a{}b{bar:{spam..}}";
        let format_string_nested = FormatString::from_str(literal_nested).unwrap();
        assert!(FormatSummary::try_from(format_string_nested).is_err());
    }
}
