use once_cell::sync::Lazy;
use regex::Regex;

static URL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^https?://\S+$").unwrap());

pub fn is_overlong(
    line: &str,
    line_length: usize,
    limit: usize,
    ignore_overlong_task_comments: bool,
    task_tags: &[String],
) -> bool {
    if line_length <= limit {
        return false;
    }

    let mut chunks = line.split_whitespace();
    let (Some(first), Some(second)) = (chunks.next(), chunks.next()) else {
        // Single word / no printable chars - no way to make the line shorter
        return false;
    };

    if first == "#" {
        if ignore_overlong_task_comments {
            let second = second.trim_end_matches(':');
            if task_tags.iter().any(|tag| tag == second) {
                return false;
            }
        }

        // Do not enforce the line length for commented lines that end with a URL
        // or contain only a single word.
        if chunks.last().map_or(true, |c| URL_REGEX.is_match(c)) {
            return false;
        }
    }

    true
}
