use once_cell::sync::Lazy;
use regex::Regex;

static NO_QA_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?P<noqa># noqa(?::\s?(?P<codes>([A-Z]+[0-9]+(?:[,\s]+)?)+))?)")
        .expect("Invalid regex")
});
static SPLIT_COMMA_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"[,\s]").expect("Invalid regex"));

pub enum Directive<'a> {
    None,
    All(usize),
    Codes(usize, Vec<&'a str>),
}

pub fn extract_noqa_directive(line: &str) -> Directive {
    match NO_QA_REGEX.captures(line) {
        Some(caps) => match caps.name("noqa") {
            Some(noqa) => match caps.name("codes") {
                Some(codes) => Directive::Codes(
                    noqa.start(),
                    SPLIT_COMMA_REGEX
                        .split(codes.as_str())
                        .map(|code| code.trim())
                        .filter(|code| !code.is_empty())
                        .collect(),
                ),
                None => Directive::All(noqa.start()),
            },
            None => Directive::None,
        },
        None => Directive::None,
    }
}
