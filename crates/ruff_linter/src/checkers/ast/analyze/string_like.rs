use ruff_python_ast::StringLike;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_bandit, flake8_pyi, flake8_quotes, ruff};

/// Run lint rules over a [`StringLike`] syntax nodes.
pub(crate) fn string_like(string_like: StringLike, checker: &mut Checker) {
    if checker.any_enabled(&[
        Rule::AmbiguousUnicodeCharacterString,
        Rule::AmbiguousUnicodeCharacterDocstring,
    ]) {
        ruff::rules::ambiguous_unicode_character_string(checker, string_like);
    }
    if checker.enabled(Rule::HardcodedBindAllInterfaces) {
        flake8_bandit::rules::hardcoded_bind_all_interfaces(checker, string_like);
    }
    if checker.enabled(Rule::HardcodedTempFile) {
        flake8_bandit::rules::hardcoded_tmp_directory(checker, string_like);
    }
    if checker.source_type.is_stub() {
        if checker.enabled(Rule::StringOrBytesTooLong) {
            flake8_pyi::rules::string_or_bytes_too_long(checker, string_like);
        }
    }
    if checker.any_enabled(&[
        Rule::BadQuotesInlineString,
        Rule::BadQuotesMultilineString,
        Rule::BadQuotesDocstring,
    ]) {
        flake8_quotes::rules::check_string_quotes(checker, string_like);
    }
}
