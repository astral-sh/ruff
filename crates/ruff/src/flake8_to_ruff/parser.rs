use std::str::FromStr;

use anyhow::{bail, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use rustc_hash::FxHashMap;

use crate::rule_selector::RuleSelector;
use crate::settings::types::PatternPrefixPair;
use crate::warn_user;

static COMMA_SEPARATED_LIST_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[,\s]").unwrap());

/// Parse a comma-separated list of `RuleSelector` values (e.g.,
/// "F401,E501").
pub fn parse_prefix_codes(value: &str) -> Vec<RuleSelector> {
    let mut codes: Vec<RuleSelector> = vec![];
    for code in COMMA_SEPARATED_LIST_RE.split(value) {
        let code = code.trim();
        if code.is_empty() {
            continue;
        }
        if let Ok(code) = RuleSelector::from_str(code) {
            codes.push(code);
        } else {
            warn_user!("Unsupported prefix code: {code}");
        }
    }
    codes
}

/// Parse a comma-separated list of strings (e.g., "__init__.py,__main__.py").
pub fn parse_strings(value: &str) -> Vec<String> {
    COMMA_SEPARATED_LIST_RE
        .split(value)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(String::from)
        .collect()
}

/// Parse a boolean.
pub fn parse_bool(value: &str) -> Result<bool> {
    match value.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => bail!("Unexpected boolean value: {value}"),
    }
}

#[derive(Debug)]
struct Token {
    token_name: TokenType,
    src: String,
}

#[derive(Debug)]
enum TokenType {
    Code,
    File,
    Colon,
    Comma,
    Ws,
    Eof,
}

struct State {
    seen_sep: bool,
    seen_colon: bool,
    filenames: Vec<String>,
    codes: Vec<String>,
}

impl State {
    const fn new() -> Self {
        Self {
            seen_sep: true,
            seen_colon: false,
            filenames: vec![],
            codes: vec![],
        }
    }

    /// Generate the list of `StrRuleCodePair` pairs for the current
    /// state.
    fn parse(&self) -> Vec<PatternPrefixPair> {
        let mut codes: Vec<PatternPrefixPair> = vec![];
        for code in &self.codes {
            if let Ok(code) = RuleSelector::from_str(code) {
                for filename in &self.filenames {
                    codes.push(PatternPrefixPair {
                        pattern: filename.clone(),
                        prefix: code.clone(),
                    });
                }
            } else {
                warn_user!("Unsupported prefix code: {code}");
            }
        }
        codes
    }
}

/// Tokenize the raw 'files-to-codes' mapping.
fn tokenize_files_to_codes_mapping(value: &str) -> Vec<Token> {
    let mut tokens = vec![];
    let mut i = 0;
    while i < value.len() {
        for (token_re, token_name) in [
            (
                Regex::new(r"([A-Z]+[0-9]*)(?:$|\s|,)").unwrap(),
                TokenType::Code,
            ),
            (Regex::new(r"([^\s:,]+)").unwrap(), TokenType::File),
            (Regex::new(r"(\s*:\s*)").unwrap(), TokenType::Colon),
            (Regex::new(r"(\s*,\s*)").unwrap(), TokenType::Comma),
            (Regex::new(r"(\s+)").unwrap(), TokenType::Ws),
        ] {
            if let Some(cap) = token_re.captures(&value[i..]) {
                let mat = cap.get(1).unwrap();
                if mat.start() == 0 {
                    tokens.push(Token {
                        token_name,
                        src: mat.as_str().trim().to_string(),
                    });
                    i += mat.end();
                    break;
                }
            }
        }
    }
    tokens.push(Token {
        token_name: TokenType::Eof,
        src: String::new(),
    });
    tokens
}

/// Parse a 'files-to-codes' mapping, mimicking Flake8's internal logic.
/// See: <https://github.com/PyCQA/flake8/blob/7dfe99616fc2f07c0017df2ba5fa884158f3ea8a/src/flake8/utils.py#L45>
pub fn parse_files_to_codes_mapping(value: &str) -> Result<Vec<PatternPrefixPair>> {
    if value.trim().is_empty() {
        return Ok(vec![]);
    }
    let mut codes: Vec<PatternPrefixPair> = vec![];
    let mut state = State::new();
    for token in tokenize_files_to_codes_mapping(value) {
        if matches!(token.token_name, TokenType::Comma | TokenType::Ws) {
            state.seen_sep = true;
        } else if !state.seen_colon {
            if matches!(token.token_name, TokenType::Colon) {
                state.seen_colon = true;
                state.seen_sep = true;
            } else if state.seen_sep && matches!(token.token_name, TokenType::File) {
                state.filenames.push(token.src);
                state.seen_sep = false;
            } else {
                bail!("Unexpected token: {:?}", token.token_name);
            }
        } else {
            if matches!(token.token_name, TokenType::Eof) {
                codes.extend(state.parse());
                state = State::new();
            } else if state.seen_sep && matches!(token.token_name, TokenType::Code) {
                state.codes.push(token.src);
                state.seen_sep = false;
            } else if state.seen_sep && matches!(token.token_name, TokenType::File) {
                codes.extend(state.parse());
                state = State::new();
                state.filenames.push(token.src);
                state.seen_sep = false;
            } else {
                bail!("Unexpected token: {:?}", token.token_name);
            }
        }
    }
    Ok(codes)
}

/// Collect a list of `PatternPrefixPair` structs as a `BTreeMap`.
pub fn collect_per_file_ignores(
    pairs: Vec<PatternPrefixPair>,
) -> FxHashMap<String, Vec<RuleSelector>> {
    let mut per_file_ignores: FxHashMap<String, Vec<RuleSelector>> = FxHashMap::default();
    for pair in pairs {
        per_file_ignores
            .entry(pair.pattern)
            .or_insert_with(Vec::new)
            .push(pair.prefix);
    }
    per_file_ignores
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::{parse_files_to_codes_mapping, parse_prefix_codes, parse_strings};
    use crate::registry::RuleCodePrefix;
    use crate::rule_selector::RuleSelector;
    use crate::settings::types::PatternPrefixPair;

    #[test]
    fn it_parses_prefix_codes() {
        let actual = parse_prefix_codes("");
        let expected: Vec<RuleSelector> = vec![];
        assert_eq!(actual, expected);

        let actual = parse_prefix_codes(" ");
        let expected: Vec<RuleSelector> = vec![];
        assert_eq!(actual, expected);

        let actual = parse_prefix_codes("F401");
        let expected = vec![RuleCodePrefix::F401.into()];
        assert_eq!(actual, expected);

        let actual = parse_prefix_codes("F401,");
        let expected = vec![RuleCodePrefix::F401.into()];
        assert_eq!(actual, expected);

        let actual = parse_prefix_codes("F401,E501");
        let expected = vec![RuleCodePrefix::F401.into(), RuleCodePrefix::E501.into()];
        assert_eq!(actual, expected);

        let actual = parse_prefix_codes("F401, E501");
        let expected = vec![RuleCodePrefix::F401.into(), RuleCodePrefix::E501.into()];
        assert_eq!(actual, expected);
    }

    #[test]
    fn it_parses_strings() {
        let actual = parse_strings("");
        let expected: Vec<String> = vec![];
        assert_eq!(actual, expected);

        let actual = parse_strings(" ");
        let expected: Vec<String> = vec![];
        assert_eq!(actual, expected);

        let actual = parse_strings("__init__.py");
        let expected = vec!["__init__.py".to_string()];
        assert_eq!(actual, expected);

        let actual = parse_strings("__init__.py,");
        let expected = vec!["__init__.py".to_string()];
        assert_eq!(actual, expected);

        let actual = parse_strings("__init__.py,__main__.py");
        let expected = vec!["__init__.py".to_string(), "__main__.py".to_string()];
        assert_eq!(actual, expected);

        let actual = parse_strings("__init__.py, __main__.py");
        let expected = vec!["__init__.py".to_string(), "__main__.py".to_string()];
        assert_eq!(actual, expected);
    }

    #[test]
    fn it_parse_files_to_codes_mapping() -> Result<()> {
        let actual = parse_files_to_codes_mapping("")?;
        let expected: Vec<PatternPrefixPair> = vec![];
        assert_eq!(actual, expected);

        let actual = parse_files_to_codes_mapping(" ")?;
        let expected: Vec<PatternPrefixPair> = vec![];
        assert_eq!(actual, expected);

        // Ex) locust
        let actual = parse_files_to_codes_mapping(
            "per-file-ignores =
    locust/test/*: F841
    examples/*: F841
    *.pyi: E302,E704"
                .strip_prefix("per-file-ignores =")
                .unwrap(),
        )?;
        let expected: Vec<PatternPrefixPair> = vec![
            PatternPrefixPair {
                pattern: "locust/test/*".to_string(),
                prefix: RuleCodePrefix::F841.into(),
            },
            PatternPrefixPair {
                pattern: "examples/*".to_string(),
                prefix: RuleCodePrefix::F841.into(),
            },
        ];
        assert_eq!(actual, expected);

        // Ex) celery
        let actual = parse_files_to_codes_mapping(
            "per-file-ignores =
   t/*,setup.py,examples/*,docs/*,extra/*:
       D,"
            .strip_prefix("per-file-ignores =")
            .unwrap(),
        )?;
        let expected: Vec<PatternPrefixPair> = vec![
            PatternPrefixPair {
                pattern: "t/*".to_string(),
                prefix: RuleCodePrefix::D.into(),
            },
            PatternPrefixPair {
                pattern: "setup.py".to_string(),
                prefix: RuleCodePrefix::D.into(),
            },
            PatternPrefixPair {
                pattern: "examples/*".to_string(),
                prefix: RuleCodePrefix::D.into(),
            },
            PatternPrefixPair {
                pattern: "docs/*".to_string(),
                prefix: RuleCodePrefix::D.into(),
            },
            PatternPrefixPair {
                pattern: "extra/*".to_string(),
                prefix: RuleCodePrefix::D.into(),
            },
        ];
        assert_eq!(actual, expected);

        // Ex) scrapy
        let actual = parse_files_to_codes_mapping(
            "per-file-ignores =
    scrapy/__init__.py:E402
    scrapy/core/downloader/handlers/http.py:F401
    scrapy/http/__init__.py:F401
    scrapy/linkextractors/__init__.py:E402,F401
    scrapy/selector/__init__.py:F401
    scrapy/spiders/__init__.py:E402,F401
    scrapy/utils/url.py:F403,F405
    tests/test_loader.py:E741"
                .strip_prefix("per-file-ignores =")
                .unwrap(),
        )?;
        let expected: Vec<PatternPrefixPair> = vec![
            PatternPrefixPair {
                pattern: "scrapy/__init__.py".to_string(),
                prefix: RuleCodePrefix::E402.into(),
            },
            PatternPrefixPair {
                pattern: "scrapy/core/downloader/handlers/http.py".to_string(),
                prefix: RuleCodePrefix::F401.into(),
            },
            PatternPrefixPair {
                pattern: "scrapy/http/__init__.py".to_string(),
                prefix: RuleCodePrefix::F401.into(),
            },
            PatternPrefixPair {
                pattern: "scrapy/linkextractors/__init__.py".to_string(),
                prefix: RuleCodePrefix::E402.into(),
            },
            PatternPrefixPair {
                pattern: "scrapy/linkextractors/__init__.py".to_string(),
                prefix: RuleCodePrefix::F401.into(),
            },
            PatternPrefixPair {
                pattern: "scrapy/selector/__init__.py".to_string(),
                prefix: RuleCodePrefix::F401.into(),
            },
            PatternPrefixPair {
                pattern: "scrapy/spiders/__init__.py".to_string(),
                prefix: RuleCodePrefix::E402.into(),
            },
            PatternPrefixPair {
                pattern: "scrapy/spiders/__init__.py".to_string(),
                prefix: RuleCodePrefix::F401.into(),
            },
            PatternPrefixPair {
                pattern: "scrapy/utils/url.py".to_string(),
                prefix: RuleCodePrefix::F403.into(),
            },
            PatternPrefixPair {
                pattern: "scrapy/utils/url.py".to_string(),
                prefix: RuleCodePrefix::F405.into(),
            },
            PatternPrefixPair {
                pattern: "tests/test_loader.py".to_string(),
                prefix: RuleCodePrefix::E741.into(),
            },
        ];
        assert_eq!(actual, expected);

        Ok(())
    }
}
