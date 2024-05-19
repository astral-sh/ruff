use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::Definition;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::docstrings::sections::{SectionContexts, SectionKind};
use crate::docstrings::styles::SectionStyle;
use crate::docstrings::Docstring;
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
///         return distance / time
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
///         return distance / time
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
            match section.kind() {
                SectionKind::Raises => {
                    raised_exceptions = parse_entries(section.following_lines_str(), style);
                    raised_exceptions_range = Some(section.range());
                }
                _ => {}
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
        if let Some(first_char) = potential[indentation..].chars().next() {
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

#[derive(Debug)]
struct BodyEntries {
    raised_exceptions: Vec<Entry>,
}

impl BodyEntries {
    fn new() -> Self {
        Self {
            raised_exceptions: Vec::new(),
        }
    }
}

impl Visitor<'_> for BodyEntries {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        if let Stmt::Raise(ast::StmtRaise { exc, .. }) = stmt {
            if let Some(exc) = exc {
                if let Expr::Name(ast::ExprName { id, range, .. }) = exc.as_ref() {
                    self.raised_exceptions.push(Entry {
                        id: id.to_string(),
                        range: *range,
                    });
                }
            }
        }
        visitor::walk_stmt(self, stmt);
    }
}

/// DAR401, DAR402
pub(crate) fn check_docstring(
    checker: &mut Checker,
    definition: &Definition,
    docstring: &Docstring,
    convention: Option<&Convention>,
) {
    let Definition::Member(member) = definition else {
        return;
    };

    let docstring_entries;
    match convention {
        Some(Convention::Google) => {
            let sections = SectionContexts::from_docstring(docstring, SectionStyle::Google);
            docstring_entries = DocstringEntries::new(&sections, SectionStyle::Google)
        }

        Some(Convention::Numpy) => {
            let sections = SectionContexts::from_docstring(docstring, SectionStyle::Numpy);
            docstring_entries = DocstringEntries::new(&sections, SectionStyle::Numpy)
        }
        _ => 'unspecified: {
            // There are some overlapping section names, between the Google and NumPy conventions
            // (e.g., "Returns", "Raises"). Break ties by checking for the presence of some of the
            // section names that are unique to each convention.

            // If the docstring contains any argument specifier, use the Google convention.
            let google_sections = SectionContexts::from_docstring(docstring, SectionStyle::Google);
            if google_sections.iter().any(|context| {
                matches!(
                    context.kind(),
                    SectionKind::Args
                        | SectionKind::Arguments
                        | SectionKind::KeywordArgs
                        | SectionKind::KeywordArguments
                        | SectionKind::OtherArgs
                        | SectionKind::OtherArguments
                )
            }) {
                docstring_entries = DocstringEntries::new(&google_sections, SectionStyle::Google);
                break 'unspecified;
            }

            // If the docstring contains `Parameters:` or `Other Parameters:`, use the NumPy
            // convention.
            let numpy_sections = SectionContexts::from_docstring(docstring, SectionStyle::Numpy);
            if numpy_sections.iter().any(|context| {
                matches!(
                    context.kind(),
                    SectionKind::Parameters
                        | SectionKind::OtherParams
                        | SectionKind::OtherParameters
                )
            }) {
                docstring_entries = DocstringEntries::new(&numpy_sections, SectionStyle::Numpy);
                break 'unspecified;
            }

            // Otherwise, use whichever convention matched more sections.
            if google_sections.len() > numpy_sections.len() {
                docstring_entries = DocstringEntries::new(&google_sections, SectionStyle::Google);
            } else {
                docstring_entries = DocstringEntries::new(&numpy_sections, SectionStyle::Numpy);
            }
        }
    };

    let mut body_entries = BodyEntries::new();
    visitor::walk_body(&mut body_entries, &member.body());

    // DAR401
    if checker.enabled(Rule::DocstringMissingException) {
        for body_raise in body_entries.raised_exceptions.iter() {
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

    // DAR402
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
