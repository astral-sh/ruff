use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::{Definition, MemberKind, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::docstrings::sections::{SectionContexts, SectionKind};
use crate::docstrings::styles::SectionStyle;
use crate::registry::Rule;
use crate::rules::pydocstyle::settings::Convention;

/// ## What it does
/// Checks for function docstrings that do not include documentation for all
/// raised exceptions.
///
/// ## Why is this bad?
/// This rule helps prevent you from leaving docstrings unfinished or incomplete.
/// Some conventions require all explicit exceptions to be documented.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Args:
///         distance: Distance traveled.
///         time: Time spent traveling.
///
///     Returns:
///         Speed as distance divided by time.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Args:
///         distance: Distance traveled.
///         time: Time spent traveling.
///
///     Returns:
///         Speed as distance divided by time.
///
///     Raises:
///         FasterThanLightError: If speed is greater than the speed of light.
///     """
///     try:
///         return distance / time
///     except ZeroDivisionError as exc:
///         raise FasterThanLightError from exc
/// ```
#[violation]
pub struct DocstringMissingException {
    id: String,
}

impl Violation for DocstringMissingException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DocstringMissingException { id } = self;
        format!("Raised exception `{id}` missing from docstring")
    }
}

/// ## What it does'
/// Checks for function docstrings that include exceptions which are not
/// explicitly raised.
///
/// ## Why is this bad?
/// Some conventions prefer non-explicit exceptions be left out of the docstring.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Args:
///         distance: Distance traveled.
///         time: Time spent traveling.
///
///     Returns:
///         Speed as distance divided by time.
///
///     Raises:
///         ZeroDivisionError: Divided by zero.
///     """
///     return distance / time
/// ```
///
/// Use instead:
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Args:
///         distance: Distance traveled.
///         time: Time spent traveling.
///
///     Returns:
///         Speed as distance divided by time.
///     """
///     return distance / time
/// ```
#[violation]
pub struct DocstringExtraneousException {
    ids: Vec<String>,
}

impl Violation for DocstringExtraneousException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DocstringExtraneousException { ids } = self;
        format!("{} not explicitly raised.", ids.join(", "))
    }
}

// Parse docstring
#[derive(Debug)]
struct DocstringEntries<'a> {
    raised_exceptions: Vec<QualifiedName<'a>>,
    raised_exceptions_range: Option<TextRange>,
}

impl<'a> DocstringEntries<'a> {
    fn new(sections: &'a SectionContexts, style: SectionStyle) -> Self {
        let mut raised_exceptions: Vec<QualifiedName> = Vec::new();
        let mut raised_exceptions_range = None;

        for section in sections.iter() {
            if section.kind() == SectionKind::Raises {
                raised_exceptions = parse_entries(section.following_lines_str(), style);
                raised_exceptions_range = Some(section.range());
            }
        }

        Self {
            raised_exceptions,
            raised_exceptions_range,
        }
    }
}

// Parses docstring sections of supported styles.
fn parse_entries(content: &str, style: SectionStyle) -> Vec<QualifiedName> {
    match style {
        SectionStyle::Google => parse_entries_google(content),
        SectionStyle::Numpy => parse_entries_numpy(content),
    }
}

// Parses google style docstring sections of the form:
//
//     Raises:
//         FasterThanLightError: If speed is greater than the speed of light.
//         DivisionByZero: If attempting to divide by zero.
//
fn parse_entries_google(content: &str) -> Vec<QualifiedName> {
    let mut entries: Vec<QualifiedName> = Vec::new();
    for potential in content.split('\n') {
        let Some(colon_idx) = potential.find(':') else {
            continue;
        };
        let entry = potential[..colon_idx].trim();
        entries.push(QualifiedName::user_defined(entry));
    }
    entries
}

// Parses numpy style docstring sections of the form:
//
//    Raises
//    ------
//    FasterThanLightError
//        If speed is greater than the speed of light.
//    DivisionByZero
//        If attempting to divide by zero.
//
fn parse_entries_numpy(content: &str) -> Vec<QualifiedName> {
    let mut entries: Vec<QualifiedName> = Vec::new();
    let mut split = content.split('\n');
    let Some(dashes) = split.next() else {
        return entries;
    };
    let indentation = dashes.len() - dashes.trim_start().len();
    for potential in split {
        if let Some(first_char) = potential.chars().nth(indentation) {
            if !first_char.is_whitespace() {
                let entry = potential[indentation..].trim();
                entries.push(QualifiedName::user_defined(entry));
            }
        }
    }
    entries
}

// Parse body
#[derive(Debug)]
struct Entry<'a> {
    qualified_name: QualifiedName<'a>,
    range: TextRange,
}

struct BodyEntries<'a> {
    raised_exceptions: Vec<Entry<'a>>,
}

struct BodyVisitor<'a> {
    raised_exceptions: Vec<Entry<'a>>,
    semantic: &'a SemanticModel<'a>,
}

impl<'a> BodyVisitor<'a> {
    fn new(semantic: &'a SemanticModel) -> Self {
        Self {
            raised_exceptions: Vec::new(),
            semantic,
        }
    }

    fn finish(self) -> BodyEntries<'a> {
        BodyEntries {
            raised_exceptions: self.raised_exceptions,
        }
    }
}

impl<'a> Visitor<'a> for BodyVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::Raise(ast::StmtRaise { exc: Some(exc), .. }) = stmt {
            if let Some(qualified_name) = extract_raised_exception(self.semantic, exc.as_ref()) {
                self.raised_exceptions.push(Entry {
                    qualified_name,
                    range: exc.as_ref().range(),
                });
            }
        }
        visitor::walk_stmt(self, stmt);
    }
}

fn extract_raised_exception<'a>(
    semantic: &SemanticModel<'a>,
    exc: &'a Expr,
) -> Option<QualifiedName<'a>> {
    if let Some(qualified_name) = semantic.resolve_qualified_name(exc) {
        return Some(qualified_name);
    }
    if let Expr::Call(ast::ExprCall { func, .. }) = exc {
        return extract_raised_exception(semantic, func.as_ref());
    }
    None
}

/// DOC501, DOC502
pub(crate) fn check_docstring(
    checker: &mut Checker,
    definition: &Definition,
    section_contexts: &SectionContexts,
    convention: Option<&Convention>,
) {
    let mut diagnostics = Vec::new();
    let Definition::Member(member) = definition else {
        return;
    };

    // Only check function docstrings.
    if matches!(
        member.kind,
        MemberKind::Class(_) | MemberKind::NestedClass(_)
    ) {
        return;
    }

    // Prioritize the specified convention over the determined style.
    let docstring_entries = match convention {
        Some(Convention::Google) => DocstringEntries::new(section_contexts, SectionStyle::Google),
        Some(Convention::Numpy) => DocstringEntries::new(section_contexts, SectionStyle::Numpy),
        _ => DocstringEntries::new(section_contexts, section_contexts.style()),
    };

    let mut visitor = BodyVisitor::new(checker.semantic());
    visitor::walk_body(&mut visitor, member.body());
    let body_entries = visitor.finish();

    // DOC501
    if checker.enabled(Rule::DocstringMissingException) {
        for body_raise in &body_entries.raised_exceptions {
            if *body_raise.qualified_name.segments().last().unwrap() == "NotImplementedError" {
                continue;
            }

            if !docstring_entries
                .raised_exceptions
                .iter()
                .any(|r| body_raise.qualified_name.segments().ends_with(r.segments()))
            {
                let diagnostic = Diagnostic::new(
                    DocstringMissingException {
                        id: (*body_raise.qualified_name.segments().last().unwrap()).to_string(),
                    },
                    body_raise.range,
                );
                diagnostics.push(diagnostic);
            }
        }
    }

    // DOC502
    if checker.enabled(Rule::DocstringExtraneousException) {
        let mut extraneous_exceptions = Vec::new();
        for docstring_raise in &docstring_entries.raised_exceptions {
            if !body_entries.raised_exceptions.iter().any(|r| {
                r.qualified_name
                    .segments()
                    .ends_with(docstring_raise.segments())
            }) {
                extraneous_exceptions.push(docstring_raise.to_string());
            }
        }
        if !extraneous_exceptions.is_empty() {
            let diagnostic = Diagnostic::new(
                DocstringExtraneousException {
                    ids: extraneous_exceptions,
                },
                docstring_entries.raised_exceptions_range.unwrap(),
            );
            diagnostics.push(diagnostic);
        }
    }

    checker.diagnostics.extend(diagnostics);
}
