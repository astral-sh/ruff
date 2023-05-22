use std::collections::BTreeSet;

use imperative::Mood;
use once_cell::sync::Lazy;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::{from_qualified_name, CallPath};
use ruff_python_ast::cast;
use ruff_python_ast::newlines::StrExt;
use ruff_python_semantic::analyze::visibility::{is_property, is_test};
use ruff_python_semantic::definition::{Definition, Member, MemberKind};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::rules::pydocstyle::helpers::normalize_word;

static MOOD: Lazy<Mood> = Lazy::new(Mood::new);

/// D401
pub(crate) fn non_imperative_mood(
    checker: &mut Checker,
    docstring: &Docstring,
    property_decorators: &BTreeSet<String>,
) {
    let Definition::Member(Member { kind, stmt, .. }) = &docstring.definition else {
        return;
    };

    if !matches!(
        kind,
        MemberKind::Function | MemberKind::NestedFunction | MemberKind::Method,
    ) {
        return;
    }

    let property_decorators = property_decorators
        .iter()
        .map(|decorator| from_qualified_name(decorator))
        .collect::<Vec<CallPath>>();

    if is_test(cast::name(stmt))
        || is_property(
            checker.semantic_model(),
            cast::decorator_list(stmt),
            &property_decorators,
        )
    {
        return;
    }

    let body = docstring.body();

    // Find first line, disregarding whitespace.
    let line = match body.trim().universal_newlines().next() {
        Some(line) => line.as_str().trim(),
        None => return,
    };
    // Find the first word on that line and normalize it to lower-case.
    let first_word_norm = match line.split_whitespace().next() {
        Some(word) => normalize_word(word),
        None => return,
    };
    if first_word_norm.is_empty() {
        return;
    }
    if let Some(false) = MOOD.is_imperative(&first_word_norm) {
        let diagnostic = Diagnostic::new(NonImperativeMood(line.to_string()), docstring.range());
        checker.diagnostics.push(diagnostic);
    }
}

#[violation]
pub struct NonImperativeMood(pub String);

impl Violation for NonImperativeMood {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonImperativeMood(first_line) = self;
        format!("First line of docstring should be in imperative mood: \"{first_line}\"")
    }
}
