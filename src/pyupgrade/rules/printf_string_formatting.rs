use crate::checkers::ast::Checker;
use once_cell::sync::Lazy;
use regex::{Match, Regex};
use rustpython_ast::Expr;

static MAPPING_KEY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\(([^()]*)\)").unwrap());
static CONVERSION_FLAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[#0+ -]*").unwrap());
static WIDTH_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:\*|\d*)").unwrap());
static PRECISION_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:\.(?:\*|\d*))?").unwrap());
static LENGTH_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[hlL]?").unwrap());

#[derive(Debug, PartialEq)]
struct PercentFormatPart {
    key: Option<String>,
    conversion_flag: Option<String>,
    width: Option<String>,
    precision: Option<String>,
    conversion: String,
}

impl PercentFormatPart {
    fn new(
        key: Option<String>,
        conversion_flag: Option<String>,
        width: Option<String>,
        precision: Option<String>,
        conversion: String,
    ) -> Self {
        Self {
            key,
            conversion_flag,
            width,
            precision,
            conversion,
        }
    }
}

#[derive(Debug, PartialEq)]
struct PercentFormat {
    item: String,
    parts: Option<PercentFormatPart>,
}

impl PercentFormat {
    fn new(item: String, parts: Option<PercentFormatPart>) -> Self {
        Self { item, parts }
    }
}

fn must_match<'a>(regex: &'a Lazy<Regex>, string: &'a str, position: usize) -> Option<Match<'a>> {
    regex.find_at(string, position)
}

/// Gets the match from a regex and potentiall updated the value of a given integer
fn get_flag<'a>(regex: &'a Lazy<Regex>, string: &'a str, position: &mut usize) -> Option<String> {
    let flag_match = must_match(regex, string, *position);
    if let Some(flag_match) = flag_match {
        *position = flag_match.end();
        Some(flag_match.as_str().to_string())
    } else {
        None
    }
}

fn parse_percent_format(string: &str) -> Vec<PercentFormat> {
    let mut string_start = 0;
    let mut string_end = 0;
    let mut in_fmt = false;
    let mut formats: Vec<PercentFormat> = vec![];

    let mut i = 0;
    while i < string.len() {
        if !in_fmt {
            i = match string[i..].find('%') {
                None => {
                    let fmt_full = PercentFormat::new(string[string_start..].to_string(), None);
                    formats.push(fmt_full);
                    return formats;
                },
                Some(item) => item,
            };
            string_end = i;
            i += 1;
            in_fmt = true;
        } else {
            let mut key: Option<String> = None;
            if let Some(key_item) = MAPPING_KEY_RE.captures(&string[i..]) {
                if let Some(match_item) = key_item.get(1) {
                    key = Some(match_item.as_str().to_string());
                    i += match_item.end();
                }
            };

            let conversion_flag = get_flag(&CONVERSION_FLAG_RE, string, &mut i);
            let width = get_flag(&WIDTH_RE, string, &mut i);
            let precision = get_flag(&PRECISION_RE, string, &mut i);

            // length modifier is ignored
            i = must_match(&LENGTH_RE, string, i).unwrap().end();
            // I use clone because nth consumes characters before position n
            let conversion = match string.clone().chars().nth(i) {
                None => panic!("end-of-string while parsing format"),
                Some(conv_item) => conv_item,
            };
            i += 1;

            let fmt = PercentFormatPart::new(
                key,
                conversion_flag,
                width,
                precision,
                conversion.to_string(),
            );
            let fmt_full =
                PercentFormat::new(string[string_start..string_end].to_string(), Some(fmt));
            formats.push(fmt_full);

            in_fmt = false;
            string_start = i;
        }
    }

    if in_fmt {
        panic!("end-of-string while parsing format");
    }
    formats
}

/// UP031
pub fn printf_string_formatting(checker: &mut Checker, left: &Expr, right: &Expr) {
    println!("{:?}", left);
    println!("==========");
    println!("{:?}", right);
    //This is just to get rid of the 10,000 lint errors
    let answer = parse_percent_format("test");
}

//Pyupgrade has a bunch of tests specific to `parse_percent_format`, I figured it wouldn't hurt to
//add them
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_percent_format_none() {
        let sample = "\"\"";
        let e1 = PercentFormat::new("\"\"".to_string(), None);
        let expected = vec![e1];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }
}
