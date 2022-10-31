use std::str::FromStr;

use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::checks_gen::CheckCodePrefix;
use crate::settings::types::StrCheckCodePair;

static COMMA_SEPARATED_LIST_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[,\s]").unwrap());

/// Parse a comma-separated list of `CheckCodePrefix` values (e.g., "F401,E501").
pub fn parse_prefix_codes(value: String) -> Vec<CheckCodePrefix> {
    let mut codes: Vec<CheckCodePrefix> = vec![];
    for code in COMMA_SEPARATED_LIST_RE.split(&value) {
        let code = code.trim();
        if let Ok(code) = CheckCodePrefix::from_str(code) {
            codes.push(code);
        } else {
            eprintln!("Unsupported prefix code: {code}");
        }
    }
    codes
}

/// Parse a comma-separated list of strings (e.g., "__init__.py,__main__.py").
pub fn parse_strings(value: String) -> Vec<String> {
    COMMA_SEPARATED_LIST_RE
        .split(&value)
        .map(|part| part.trim())
        .map(String::from)
        .collect()
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
    fn new() -> Self {
        Self {
            seen_sep: true,
            seen_colon: false,
            filenames: vec![],
            codes: vec![],
        }
    }

    /// Generate the list of `StrCheckCodePair` pairs for the current state.
    fn parse(&self) -> Vec<StrCheckCodePair> {
        let mut codes: Vec<StrCheckCodePair> = vec![];
        for code in &self.codes {
            match CheckCodePrefix::from_str(code) {
                Ok(code) => {
                    for filename in &self.filenames {
                        codes.push(StrCheckCodePair {
                            pattern: filename.clone(),
                            code: code.clone(),
                        });
                    }
                }
                Err(_) => eprintln!("Skipping unrecognized prefix: {}", code),
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
                        src: mat.as_str().to_string().trim().to_string(),
                    });
                    i += mat.end();
                    break;
                }
            }
        }
    }
    tokens.push(Token {
        token_name: TokenType::Eof,
        src: "".to_string(),
    });
    tokens
}

/// Parse a 'files-to-codes' mapping, mimicking Flake8's internal logic.
///
/// See: https://github.com/PyCQA/flake8/blob/7dfe99616fc2f07c0017df2ba5fa884158f3ea8a/src/flake8/utils.py#L45
pub fn parse_files_to_codes_mapping(value: String) -> Result<Vec<StrCheckCodePair>> {
    if value.trim().is_empty() {
        return Ok(vec![]);
    }
    let mut codes: Vec<StrCheckCodePair> = vec![];
    let mut state = State::new();
    for token in tokenize_files_to_codes_mapping(&value) {
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
                return Err(anyhow::anyhow!("Unexpected token: {:?}", token.token_name));
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
                return Err(anyhow::anyhow!("Unexpected token: {:?}", token.token_name));
            }
        }
    }
    Ok(codes)
}
