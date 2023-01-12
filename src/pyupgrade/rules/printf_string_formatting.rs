use crate::checkers::ast::Checker;
use once_cell::sync::Lazy;
use regex::{Match, Regex};
use rustpython_ast::Expr;
use std::default::Default;

// Tests: https://github.com/asottile/pyupgrade/blob/main/tests/features/percent_format_test.py
// Code: https://github.com/asottile/pyupgrade/blob/97ed6fb3cf2e650d4f762ba231c3f04c41797710/pyupgrade/_plugins/percent_format.py#L48

static MAPPING_KEY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\(([^()]*)\)").unwrap());
static CONVERSION_FLAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[#0+ -]*").unwrap());
static WIDTH_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:\*|\d*)").unwrap());
static PRECISION_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:\.(?:\*|\d*))?").unwrap());
static LENGTH_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[hlL]?").unwrap());

#[derive(Debug, PartialEq, Clone)]
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

#[derive(Debug, PartialEq, Clone)]
struct PercentFormat {
    item: String,
    parts: Option<PercentFormatPart>,
}

impl Default for PercentFormat {
    fn default() -> Self {
        Self {
            item: "\"".to_string(),
            parts: None,
        }
    }
}

impl PercentFormat {
    fn new(item: String, parts: Option<PercentFormatPart>) -> Self {
        Self { item, parts }
    }
}

/// Gets the match from a regex and potentiall updated the value of a given integer
fn get_flag<'a>(regex: &'a Lazy<Regex>, string: &'a str, position: &mut usize) -> Option<String> {
    let flag_match = regex.find_at(string, *position);
    if let Some(flag_match) = flag_match {
        *position = flag_match.end();
        let the_string = flag_match.as_str().to_string();
        if the_string.is_empty() {
            None
        } else {
            Some(the_string)
        }
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
                }
                // Since we cut off the part of the string before `i` in the beginning, we need to
                // add it back to get the proper index
                Some(item) => item + i,
            };
            string_end = i;
            i += 1;
            in_fmt = true;
        } else {
            let mut key: Option<String> = None;
            if let Some(key_item) = MAPPING_KEY_RE.captures(&string[i..]) {
                if let Some(match_item) = key_item.get(1) {
                    key = Some(match_item.as_str().to_string());
                    // Have to use another regex because the rust Capture object does not have an
                    // end() method
                    i = MAPPING_KEY_RE.find_at(string, i).unwrap().end();
                }
            };

            let conversion_flag = get_flag(&CONVERSION_FLAG_RE, string, &mut i);
            let width = get_flag(&WIDTH_RE, string, &mut i);
            let precision = get_flag(&PRECISION_RE, string, &mut i);

            // length modifier is ignored
            i = LENGTH_RE.find_at(string, i).unwrap().end();
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

    #[test]
    fn test_parse_percent_format_double_percent() {
        let sample = "\"%%\"";
        let sube1 = PercentFormatPart::new(None, None, None, None, "%".to_string());
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e2 = PercentFormat::default();
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_double_s() {
        let sample = "\"%s\"";
        let sube1 = PercentFormatPart::new(None, None, None, None, "s".to_string());
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e2 = PercentFormat::default();
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_double_two() {
        let sample = "\"%s two! %s\"";
        let sube1 = PercentFormatPart::new(None, None, None, None, "s".to_string());
        let e1 = PercentFormat::new(" two! ".to_string(), Some(sube1.clone()));
        let e2 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e3 = PercentFormat::default();
        let expected = vec![e2, e1, e3];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_word_in_paren() {
        let sample = "\"%(hi)s\"";
        let sube1 =
            PercentFormatPart::new(Some("hi".to_string()), None, None, None, "s".to_string());
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e2 = PercentFormat::default();
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_empty_paren() {
        let sample = "\"%()s\"";
        let sube1 = PercentFormatPart::new(Some("".to_string()), None, None, None, "s".to_string());
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e2 = PercentFormat::default();
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_hashtag() {
        let sample = "\"%#o\"";
        let sube1 =
            PercentFormatPart::new(None, Some("#".to_string()), None, None, "o".to_string());
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e2 = PercentFormat::default();
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_hashtag_and_symbols() {
        let sample = "\"% #0-+d\"";
        let sube1 =
            PercentFormatPart::new(None, Some(" #0-+".to_string()), None, None, "d".to_string());
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e2 = PercentFormat::default();
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_numbers_letters() {
        let sample = "\"%5d\"";
        let sube1 =
            PercentFormatPart::new(None, None, Some("5".to_string()), None, "d".to_string());
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e2 = PercentFormat::default();
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_star_d() {
        let sample = "\"%*d\"";
        let sube1 =
            PercentFormatPart::new(None, None, Some("*".to_string()), None, "d".to_string());
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e2 = PercentFormat::default();
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_dot_letter() {
        let sample = "\"%.f\"";
        let sube1 =
            PercentFormatPart::new(None, None, None, Some(".".to_string()), "f".to_string());
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e2 = PercentFormat::default();
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_dot_number_letter() {
        let sample = "\"%.5f\"";
        let sube1 =
            PercentFormatPart::new(None, None, None, Some(".5".to_string()), "f".to_string());
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e2 = PercentFormat::default();
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_dot_star_letter() {
        let sample = "\"%.*f\"";
        let sube1 =
            PercentFormatPart::new(None, None, None, Some(".*".to_string()), "f".to_string());
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e2 = PercentFormat::default();
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_two_letter() {
        let sample = "\"%ld\"";
        let sube1 = PercentFormatPart::new(None, None, None, None, "d".to_string());
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e2 = PercentFormat::default();
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_everything() {
        let sample = "\"%(complete)#4.4f\"";
        let sube1 = PercentFormatPart::new(
            Some("complete".to_string()),
            Some("#".to_string()),
            Some("4".to_string()),
            Some(".4".to_string()),
            "f".to_string(),
        );
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e2 = PercentFormat::default();
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }
}
