use std::collections::BTreeSet;

use imperative::Mood;
use once_cell::sync::Lazy;
use ruff_macros::{derive_message_formats, violation};

use crate::ast::cast;
use crate::ast::helpers::to_call_path;
use crate::ast::types::{CallPath, Range};
use crate::ast::visibility::{is_property, is_test};
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::registry::Diagnostic;
use crate::rules::pydocstyle::helpers::normalize_word;
use crate::violation::Violation;

static MOOD: Lazy<Mood> = Lazy::new(Mood::new);

/// D401
pub fn non_imperative_mood(
    checker: &mut Checker,
    docstring: &Docstring,
    property_decorators: &BTreeSet<String>,
) {
    let (
        DefinitionKind::Function(parent)
        | DefinitionKind::NestedFunction(parent)
        | DefinitionKind::Method(parent)
    ) = &docstring.kind else {
        return;
    };

    let property_decorators = property_decorators
        .iter()
        .map(|decorator| to_call_path(decorator))
        .collect::<Vec<CallPath>>();

    if is_test(cast::name(parent))
        || is_property(
            &checker.ctx,
            cast::decorator_list(parent),
            &property_decorators,
        )
    {
        return;
    }

    let body = docstring.body;

    // Find first line, disregarding whitespace.
    let line = match body.trim().lines().next() {
        Some(line) => line.trim(),
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
        let diagnostic = Diagnostic::new(
            NonImperativeMood(line.to_string()),
            Range::from_located(docstring.expr),
        );
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
