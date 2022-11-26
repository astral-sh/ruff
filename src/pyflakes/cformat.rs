//! Implements helper functions for using vendored/cformat.rs
use std::convert::TryFrom;
use std::str::FromStr;

use rustc_hash::FxHashSet;

use crate::vendored::cformat::{CFormatError, CFormatPart, CFormatSpec, CFormatString};

pub(crate) struct CFormatSummary {
    pub positional: usize,
    pub keywords: FxHashSet<String>,
}

impl TryFrom<&str> for CFormatSummary {
    type Error = CFormatError;

    fn try_from(literal: &str) -> Result<Self, Self::Error> {
        let format_string = CFormatString::from_str(literal)?;

        let mut positional = 0;
        let mut keywords = FxHashSet::default();

        for format_part in format_string.parts {
            if let CFormatPart::Spec(CFormatSpec { mapping_key, .. }) = format_part.1 {
                match mapping_key {
                    Some(k) => {
                        keywords.insert(k);
                    }
                    None => positional += 1,
                };
            }
        }

        Ok(CFormatSummary {
            positional,
            keywords,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cformat_summary() {
        let literal = "%(foo)s %s %d %(bar)x";

        let expected_positional = 2;
        let expected_keywords = ["foo", "bar"].into_iter().map(String::from).collect();

        let format_summary = CFormatSummary::try_from(literal).unwrap();
        assert_eq!(format_summary.positional, expected_positional);
        assert_eq!(format_summary.keywords, expected_keywords);
    }

    #[test]
    fn test_cformat_summary_invalid() {
        assert!(CFormatSummary::try_from("%").is_err());
        assert!(CFormatSummary::try_from("%(foo).").is_err());
    }
}
