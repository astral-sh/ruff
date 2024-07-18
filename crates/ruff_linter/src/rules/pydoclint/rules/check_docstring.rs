use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::{Definition, MemberKind};
use ruff_text_size::TextRange;

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
struct DocstringEntries {
    raised_exceptions: Vec<String>,
    raised_exceptions_range: Option<TextRange>,
}

impl DocstringEntries {
    fn new(sections: &SectionContexts, style: SectionStyle) -> Self {
        let mut raised_exceptions: Vec<String> = Vec::new();
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

fn parse_entries(content: &str, style: SectionStyle) -> Vec<String> {
    match style {
        SectionStyle::Google => parse_entries_google(content),
        SectionStyle::Numpy => parse_entries_numpy(content),
    }
}

fn parse_entries_google(content: &str) -> Vec<String> {
    let mut entries: Vec<String> = Vec::new();
    for potential in content.split('\n') {
        let Some(colon_idx) = potential.find(':') else {
            continue;
        };
        let entry = potential[..colon_idx].trim().to_string();
        entries.push(entry);
    }
    entries
}

fn parse_entries_numpy(content: &str) -> Vec<String> {
    let mut entries: Vec<String> = Vec::new();
    let mut split = content.split('\n');
    let Some(dashes) = split.next() else {
        return entries;
    };
    let indentation = dashes.len() - dashes.trim_start().len();
    for potential in split {
        if let Some(first_char) = potential.chars().nth(indentation) {
            if !first_char.is_whitespace() {
                let entry = potential[indentation..].trim().to_string();
                entries.push(entry);
            }
        }
    }
    entries
}

// Parse body
#[derive(Debug)]
struct Entry {
    id: String,
    range: TextRange,
}

struct BodyEntries {
    raised_exceptions: Vec<Entry>,
}

struct BodyVisitor<'a> {
    raised_exceptions: Vec<Entry>,
    semantic: &'a SemanticModel<'a>,
}

impl<'a> BodyVisitor<'a> {
    fn new(semantic: &'a SemanticModel<'a>) -> Self {
        Self {
            raised_exceptions: Vec::new(),
            semantic,
        }
    }

    fn finish(self) -> BodyEntries {
        BodyEntries {
            raised_exceptions: self.raised_exceptions,
        }
    }
}

impl Visitor<'_> for BodyVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        if let Stmt::Raise(ast::StmtRaise { exc: Some(exc), .. }) = stmt {
            match exc.as_ref() {
                Expr::Name(ast::ExprName { id, range, .. }) => {
                    // SemanticModel will resolve qualified_name for local Class definitions,
                    // or imported definitions, but not variables which we want to ignore.
                    if self.semantic.resolve_qualified_name(exc.as_ref()).is_some() {
                        self.raised_exceptions.push(Entry {
                            id: id.to_string(),
                            range: *range,
                        });
                    }
                }
                Expr::Call(ast::ExprCall { func, range, .. }) => {
                    if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
                        // SemanticModel will resolve qualified_name for local Class definitions,
                        // or imported definitions, but not variables which we want to ignore.
                        if self
                            .semantic
                            .resolve_qualified_name(func.as_ref())
                            .is_some()
                        {
                            self.raised_exceptions.push(Entry {
                                id: id.to_string(),
                                range: *range,
                            });
                        }
                    }
                }
                _ => {}
            };
        }
        visitor::walk_stmt(self, stmt);
    }
}

/// DOC501, DOC502
pub(crate) fn check_docstring(
    checker: &mut Checker,
    definition: &Definition,
    section_contexts: &SectionContexts,
    convention: Option<&Convention>,
) {
    let Definition::Member(member) = definition else {
        return;
    };

    if matches!(
        member.kind,
        MemberKind::Class(_) | MemberKind::NestedClass(_)
    ) {
        return;
    }

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
            if body_raise.id == "NotImplementedError" {
                continue;
            }

            if !docstring_entries.raised_exceptions.contains(&body_raise.id) {
                let diagnostic = Diagnostic::new(
                    DocstringMissingException {
                        id: body_raise.id.clone(),
                    },
                    body_raise.range,
                );
                checker.diagnostics.push(diagnostic);
            }
        }
    }

    // DOC502
    if checker.enabled(Rule::DocstringExtraneousException) {
        let mut extraneous_exceptions = Vec::new();
        for docstring_raise in docstring_entries.raised_exceptions {
            if !body_entries
                .raised_exceptions
                .iter()
                .any(|r| r.id == docstring_raise)
            {
                extraneous_exceptions.push(docstring_raise);
            }
        }
        if !extraneous_exceptions.is_empty() {
            let diagnostic = Diagnostic::new(
                DocstringExtraneousException {
                    ids: extraneous_exceptions,
                },
                docstring_entries.raised_exceptions_range.unwrap(),
            );
            checker.diagnostics.push(diagnostic);
        }
    }
}
