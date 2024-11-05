/// See: [eradicate.py](https://github.com/myint/eradicate/blob/98f199940979c94447a461d50d27862b118b282d/eradicate.py)
use aho_corasick::AhoCorasick;
use itertools::Itertools;
use regex::{Regex, RegexSet};
use ruff_python_parser::parse_module;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::TextSize;
use std::sync::LazyLock;

static CODE_INDICATORS: LazyLock<AhoCorasick> = LazyLock::new(|| {
    AhoCorasick::new([
        "(", ")", "[", "]", "{", "}", ":", "=", "%", "return", "break", "continue", "import",
    ])
    .unwrap()
});

static ALLOWLIST_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        ^
        (?:
            # Case-sensitive
            pyright
        |   mypy:
        |   type:\s*ignore
        |   SPDX-License-Identifier:
        |   fmt:\s*(on|off|skip)
        |   region|endregion

            # Case-insensitive
        |   (?i:
                noqa
            )

            # Unknown case sensitivity
        |   (?i:
                pylint
            |   nosec
            |   isort:\s*(on|off|skip|skip_file|split|dont-add-imports(:\s*\[.*?])?)
            |   (?:en)?coding[:=][\x20\t]*([-_.A-Z0-9]+)
            )

            # IntelliJ language injection comments:
            # * `language` must be lowercase.
            # * No spaces around `=`.
            # * Language IDs as used in comments must have no spaces,
            #   though to IntelliJ they can be anything.
            # * May optionally contain `prefix=` and/or `suffix=`,
            #   not declared here since we use `.is_match()`.
        |   language=[-_.a-zA-Z0-9]+

        )
        ",
    )
    .unwrap()
});

static HASH_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#\d").unwrap());

static POSITIVE_CASES: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new([
        // Keywords
        r"^(?:elif\s+.*\s*:.*|else\s*:.*|try\s*:.*|finally\s*:.*|except.*:.*|case\s+.*\s*:.*)$",
        // Partial dictionary
        r#"^['"]\w+['"]\s*:.+[,{]\s*(#.*)?$"#,
        // Multiline assignment
        r"^(?:[(\[]\s*)?(?:\w+\s*,\s*)*\w+\s*([)\]]\s*)?=.*[(\[{]$",
        // Brackets,
        r"^[()\[\]{}\s]+$",
    ])
    .unwrap()
});

/// Returns `true` if a comment contains Python code.
pub(crate) fn comment_contains_code(line: &str, task_tags: &[String]) -> bool {
    let line = line.trim_start_matches([' ', '#']).trim_end();

    // Fast path: if none of the indicators are present, the line is not code.
    if !CODE_INDICATORS.is_match(line) {
        return false;
    }

    // Fast path: if the comment contains consecutive identifiers, we know it won't parse.
    let tokenizer = SimpleTokenizer::starts_at(TextSize::default(), line).skip_trivia();
    if tokenizer.tuple_windows().any(|(first, second)| {
        first.kind == SimpleTokenKind::Name && second.kind == SimpleTokenKind::Name
    }) {
        return false;
    }

    // Ignore task tag comments (e.g., "# TODO(tom): Refactor").
    if line
        .split(&[' ', ':', '('])
        .next()
        .is_some_and(|first| task_tags.iter().any(|tag| tag == first))
    {
        return false;
    }

    // Ignore whitelisted comments.
    if ALLOWLIST_REGEX.is_match(line) {
        return false;
    }

    // Ignore non-comment related hashes (e.g., "# Issue #999").
    if HASH_NUMBER.is_match(line) {
        return false;
    }

    // If the comment made it this far, and ends in a continuation, assume it's code.
    if line.ends_with('\\') {
        return true;
    }

    // If the comment matches any of the specified positive cases, assume it's code.
    if POSITIVE_CASES.is_match(line) {
        return true;
    }

    // Finally, compile the source code.
    parse_module(line).is_ok()
}

#[cfg(test)]
mod tests {
    use crate::settings::TASK_TAGS;

    use super::comment_contains_code;

    #[test]
    fn comment_contains_code_basic() {
        assert!(comment_contains_code("# x = 1", &[]));
        assert!(comment_contains_code("# # x = 1", &[]));
        assert!(comment_contains_code("#from foo import eradicate", &[]));
        assert!(comment_contains_code("#import eradicate", &[]));
        assert!(comment_contains_code(r#"#"key": value,"#, &[]));
        assert!(comment_contains_code(r#"#"key": "value","#, &[]));
        assert!(comment_contains_code(r#"#"key": 1 + 1,"#, &[]));
        assert!(comment_contains_code("#'key': 1 + 1,", &[]));
        assert!(comment_contains_code(r#"#"key": {"#, &[]));
        assert!(comment_contains_code("#}", &[]));
        assert!(comment_contains_code("#} )]", &[]));

        assert!(!comment_contains_code("#", &[]));
        assert!(!comment_contains_code("# This is a (real) comment.", &[]));
        assert!(!comment_contains_code("# # A (nested) comment.", &[]));
        assert!(!comment_contains_code("# 123", &[]));
        assert!(!comment_contains_code("# 123.1", &[]));
        assert!(!comment_contains_code("# 1, 2, 3", &[]));
        assert!(!comment_contains_code(
            "# pylint: disable=redefined-outer-name",
            &[]
        ));
        assert!(!comment_contains_code(
            "# Issue #999: This is not code",
            &[]
        ));
        assert!(!comment_contains_code("# mypy: allow-untyped-calls", &[]));
        assert!(!comment_contains_code(
            "# SPDX-License-Identifier: MIT",
            &[]
        ));

        // TODO(charlie): This should be `true` under aggressive mode.
        assert!(!comment_contains_code("#},", &[]));
    }

    #[test]
    fn comment_contains_code_with_print() {
        assert!(comment_contains_code("#print(1)", &[]));

        assert!(!comment_contains_code("#print", &[]));
        assert!(!comment_contains_code("#print 1", &[]));
        assert!(!comment_contains_code("#to print", &[]));
    }

    #[test]
    fn comment_contains_code_with_return() {
        assert!(comment_contains_code("#return x", &[]));

        assert!(!comment_contains_code("#to print", &[]));
    }

    #[test]
    fn comment_contains_code_single_line() {
        assert!(comment_contains_code("# case 1: print()", &[]));
        assert!(comment_contains_code("# try: get(1, 2, 3)", &[]));
        assert!(comment_contains_code("# else: print()", &[]));
        assert!(comment_contains_code("# elif x == 10: print()", &[]));
        assert!(comment_contains_code(
            "# except Exception as e: print(e)",
            &[]
        ));
        assert!(comment_contains_code("# except: print()", &[]));
        assert!(comment_contains_code("# finally: close_handle()", &[]));

        assert!(!comment_contains_code("# try: use cache", &[]));
        assert!(!comment_contains_code("# else: we should return", &[]));
        assert!(!comment_contains_code(
            "# call function except: without cache",
            &[]
        ));
    }

    #[test]
    fn comment_contains_code_with_multiline() {
        assert!(comment_contains_code("#else:", &[]));
        assert!(comment_contains_code("#  else  :  ", &[]));
        assert!(comment_contains_code(r#"# "foo %d" % \\"#, &[]));
        assert!(comment_contains_code("#elif True:", &[]));
        assert!(comment_contains_code("#x = foo(", &[]));
        assert!(comment_contains_code("#except Exception:", &[]));
        assert!(comment_contains_code("# case 1:", &[]));
        assert!(comment_contains_code("#case 1:", &[]));
        assert!(comment_contains_code("# try:", &[]));

        assert!(!comment_contains_code("# this is = to that :(", &[]));
        assert!(!comment_contains_code("#else", &[]));
        assert!(!comment_contains_code("#or else:", &[]));
        assert!(!comment_contains_code("#else True:", &[]));
        assert!(!comment_contains_code("# in that case:", &[]));

        // Unpacking assignments
        assert!(comment_contains_code(
            "# user_content_type, _ = TimelineEvent.objects.using(db_alias).get_or_create(",
            &[]
        ));
        assert!(comment_contains_code(
            "# (user_content_type, _) = TimelineEvent.objects.using(db_alias).get_or_create(",
            &[]
        ));
        assert!(comment_contains_code(
            "# ( user_content_type , _ )= TimelineEvent.objects.using(db_alias).get_or_create(",
            &[]
        ));
        assert!(comment_contains_code("# )", &[]));

        // This used to return true, but our parser has gotten a bit better
        // at rejecting invalid Python syntax. And indeed, this is not valid
        // Python code.
        assert!(!comment_contains_code(
            "#     app_label=\"core\", model=\"user\"",
            &[]
        ));

        // TODO(charlie): This should be `true` under aggressive mode.
        assert!(!comment_contains_code("#def foo():", &[]));
    }

    #[test]
    fn comment_contains_code_with_sentences() {
        assert!(!comment_contains_code("#code is good", &[]));
    }

    #[test]
    fn comment_contains_code_with_encoding() {
        assert!(comment_contains_code("# codings=utf-8", &[]));

        assert!(!comment_contains_code("# coding=utf-8", &[]));
        assert!(!comment_contains_code("#coding= utf-8", &[]));
        assert!(!comment_contains_code("# coding: utf-8", &[]));
        assert!(!comment_contains_code("# encoding: utf8", &[]));
    }

    #[test]
    fn comment_contains_code_with_default_allowlist() {
        assert!(!comment_contains_code("# pylint: disable=A0123", &[]));
        assert!(!comment_contains_code("# pylint:disable=A0123", &[]));
        assert!(!comment_contains_code("# pylint: disable = A0123", &[]));
        assert!(!comment_contains_code("# pylint:disable = A0123", &[]));
        assert!(!comment_contains_code(
            "# pyright: reportErrorName=true",
            &[]
        ));
        assert!(!comment_contains_code("# noqa", &[]));
        assert!(!comment_contains_code("# NOQA", &[]));
        assert!(!comment_contains_code("# noqa: A123", &[]));
        assert!(!comment_contains_code("# noqa:A123", &[]));
        assert!(!comment_contains_code("# nosec", &[]));
        assert!(!comment_contains_code("# region", &[]));
        assert!(!comment_contains_code("# endregion", &[]));
        assert!(!comment_contains_code("# region.name", &[]));
        assert!(!comment_contains_code("# region name", &[]));
        assert!(!comment_contains_code("# region: name", &[]));
        assert!(!comment_contains_code("# fmt: on", &[]));
        assert!(!comment_contains_code("# fmt: off", &[]));
        assert!(!comment_contains_code("# fmt:on", &[]));
        assert!(!comment_contains_code("# fmt:off", &[]));
        assert!(!comment_contains_code("# isort: on", &[]));
        assert!(!comment_contains_code("# isort:on", &[]));
        assert!(!comment_contains_code("# isort: off", &[]));
        assert!(!comment_contains_code("# isort:off", &[]));
        assert!(!comment_contains_code("# isort: skip", &[]));
        assert!(!comment_contains_code("# isort:skip", &[]));
        assert!(!comment_contains_code("# isort: skip_file", &[]));
        assert!(!comment_contains_code("# isort:skip_file", &[]));
        assert!(!comment_contains_code("# isort: split", &[]));
        assert!(!comment_contains_code("# isort:split", &[]));
        assert!(!comment_contains_code("# isort: dont-add-imports", &[]));
        assert!(!comment_contains_code("# isort:dont-add-imports", &[]));
        assert!(!comment_contains_code(
            "# isort: dont-add-imports: [\"import os\"]",
            &[]
        ));
        assert!(!comment_contains_code(
            "# isort:dont-add-imports: [\"import os\"]",
            &[]
        ));
        assert!(!comment_contains_code(
            "# isort: dont-add-imports:[\"import os\"]",
            &[]
        ));
        assert!(!comment_contains_code(
            "# isort:dont-add-imports:[\"import os\"]",
            &[]
        ));
        assert!(!comment_contains_code("# type: ignore", &[]));
        assert!(!comment_contains_code("# type:ignore", &[]));
        assert!(!comment_contains_code("# type: ignore[import]", &[]));
        assert!(!comment_contains_code("# type:ignore[import]", &[]));
        assert!(!comment_contains_code(
            "# TODO: Do that",
            &["TODO".to_string()]
        ));
        assert!(!comment_contains_code(
            "# FIXME: Fix that",
            &["FIXME".to_string()]
        ));
        assert!(!comment_contains_code(
            "# XXX: What ever",
            &["XXX".to_string()]
        ));
    }

    #[test]
    fn comment_contains_language_injection() {
        // `language` with bad casing
        assert!(comment_contains_code("# Language=C#", &[]));
        assert!(comment_contains_code("# lAngUAgE=inI", &[]));

        // Unreasonable language IDs, possibly literals
        assert!(comment_contains_code("# language=\"pt\"", &[]));
        assert!(comment_contains_code("# language='en'", &[]));

        // Spaces around equal sign
        assert!(comment_contains_code("# language =xml", &[]));
        assert!(comment_contains_code("# language= html", &[]));
        assert!(comment_contains_code("# language = RegExp", &[]));

        // Leading whitespace
        assert!(!comment_contains_code("#language=CSS", &[]));
        assert!(!comment_contains_code("#   \t language=C++", &[]));

        // Human language false negatives
        assert!(!comment_contains_code("# language=en", &[]));
        assert!(!comment_contains_code("# language=en-US", &[]));

        // Casing (fine because such IDs cannot be validated)
        assert!(!comment_contains_code("# language=PytHoN", &[]));
        assert!(!comment_contains_code("# language=jaVaScrIpt", &[]));

        // Space within ID (fine because `Shell` is considered the ID)
        assert!(!comment_contains_code("#    language=Shell Script", &[]));

        // With prefix and/or suffix
        assert!(!comment_contains_code("# language=HTML prefix=<body>", &[]));
        assert!(!comment_contains_code(
            r"# language=Requirements suffix=\n",
            &[]
        ));
        assert!(!comment_contains_code(
            "language=javascript prefix=(function(){ suffix=})()",
            &[]
        ));
    }

    #[test]
    fn comment_contains_todo() {
        let task_tags = TASK_TAGS
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        assert!(!comment_contains_code(
            "# TODO(tom): Rewrite in Rust",
            &task_tags
        ));
        assert!(!comment_contains_code(
            "# TODO: Rewrite in Rust",
            &task_tags
        ));
        assert!(!comment_contains_code("# TODO:Rewrite in Rust", &task_tags));

        assert!(!comment_contains_code(
            "# FIXME(tom): Rewrite in Rust",
            &task_tags
        ));
        assert!(!comment_contains_code(
            "# FIXME: Rewrite in Rust",
            &task_tags
        ));
        assert!(!comment_contains_code(
            "# FIXME:Rewrite in Rust",
            &task_tags
        ));

        assert!(!comment_contains_code(
            "# XXX(tom): Rewrite in Rust",
            &task_tags
        ));
        assert!(!comment_contains_code("# XXX: Rewrite in Rust", &task_tags));
        assert!(!comment_contains_code("# XXX:Rewrite in Rust", &task_tags));
    }
}
