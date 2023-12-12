use ruff_python_ast::StringLike;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_bandit, flake8_pyi};

/// Run lint rules over a [`StringLike`] syntax nodes.
pub(crate) fn string_like(string_like: StringLike, checker: &mut Checker) {
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
}
