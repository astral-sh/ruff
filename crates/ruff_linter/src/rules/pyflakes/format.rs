//! Implements helper functions for using vendored/format.rs
use ruff_python_ast::name::Name;
use ruff_python_literal::format::{
    FieldName, FieldType, FormatParseError, FormatPart, FormatString, FromTemplate,
};
use std::convert::TryFrom;

pub(crate) fn error_to_string(err: &FormatParseError) -> String {
    match err {
        FormatParseError::EmptyAttribute => "Empty attribute in format string",
        FormatParseError::InvalidCharacterAfterRightBracket => {
            "Only '.' or '[' may follow ']' in format field specifier"
        }
        FormatParseError::PlaceholderRecursionExceeded => {
            "Max format placeholder recursion exceeded"
        }
        FormatParseError::MissingStartBracket => "Single '}' encountered in format string",
        FormatParseError::MissingRightBracket => "Expected '}' before end of string",
        FormatParseError::UnmatchedBracket => "Single '{' encountered in format string",
        _ => "Unexpected error parsing format string",
    }
    .to_string()
}

#[derive(Debug)]
pub(crate) struct FormatSummary {
    pub(crate) autos: Vec<usize>,
    pub(crate) indices: Vec<usize>,
    pub(crate) keywords: Vec<Name>,
    pub(crate) has_nested_parts: bool,
}

impl TryFrom<&str> for FormatSummary {
    type Error = FormatParseError;

    fn try_from(literal: &str) -> Result<Self, Self::Error> {
        let format_string = FormatString::from_str(literal)?;

        let mut autos = Vec::new();
        let mut indices = Vec::new();
        let mut keywords = Vec::new();
        let mut has_nested_parts = false;

        for format_part in &format_string.format_parts {
            let FormatPart::Field {
                field_name,
                format_spec,
                ..
            } = format_part
            else {
                continue;
            };
            let parsed = FieldName::parse(field_name)?;
            match parsed.field_type {
                FieldType::Auto => autos.push(autos.len()),
                FieldType::Index(i) => indices.push(i),
                FieldType::Keyword(k) => keywords.push(Name::from(k)),
            }

            let nested = FormatString::from_str(format_spec)?;
            for nested_part in nested.format_parts {
                let FormatPart::Field { field_name, .. } = nested_part else {
                    continue;
                };
                let parsed = FieldName::parse(&field_name)?;
                match parsed.field_type {
                    FieldType::Auto => autos.push(autos.len()),
                    FieldType::Index(i) => indices.push(i),
                    FieldType::Keyword(k) => keywords.push(Name::from(k)),
                }
                has_nested_parts = true;
            }
        }

        Ok(FormatSummary {
            autos,
            indices,
            keywords,
            has_nested_parts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_summary() {
        let literal = "foo{foo}a{}b{2}c{2}d{1}{}{}e{bar}{foo}f{spam}";

        let expected_autos = [0usize, 1usize, 2usize].to_vec();
        let expected_indices = [2usize, 2usize, 1usize].to_vec();
        let expected_keywords: Vec<_> = ["foo", "bar", "foo", "spam"]
            .into_iter()
            .map(String::from)
            .collect();

        let format_summary = FormatSummary::try_from(literal).unwrap();

        assert_eq!(format_summary.autos, expected_autos);
        assert_eq!(format_summary.indices, expected_indices);
        assert_eq!(format_summary.keywords, expected_keywords);
        assert!(!format_summary.has_nested_parts);
    }

    #[test]
    fn test_format_summary_nested() {
        let literal = "foo{foo}a{:{}{}}b{2:{3}{4}}c{2}d{1}{}e{bar:{spam}{eggs}}";

        let expected_autos = [0usize, 1usize, 2usize, 3usize].to_vec();
        let expected_indices = [2usize, 3usize, 4usize, 2usize, 1usize].to_vec();
        let expected_keywords: Vec<_> = ["foo", "bar", "spam", "eggs"]
            .into_iter()
            .map(String::from)
            .collect();

        let format_summary = FormatSummary::try_from(literal).unwrap();

        assert_eq!(format_summary.autos, expected_autos);
        assert_eq!(format_summary.indices, expected_indices);
        assert_eq!(format_summary.keywords, expected_keywords);
        assert!(format_summary.has_nested_parts);
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
