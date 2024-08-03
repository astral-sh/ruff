use itertools::Itertools;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, visitor, Expr, Stmt};
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::{Definition, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::docstrings::sections::{SectionContext, SectionContexts, SectionKind};
use crate::docstrings::styles::SectionStyle;
use crate::registry::Rule;
use crate::rules::pydocstyle::settings::Convention;

/// ## What it does
/// Checks for functions with explicit returns missing a "returns" section in
/// their docstring.
///
/// ## Why is this bad?
/// Docstrings missing return sections are a sign of incomplete documentation
/// or refactors.
///
/// ## Example
/// ```python
/// def calculate_speed(distance: float, time: float) -> float:
///     """Calculate speed as distance divided by time.
///
///     Args:
///         distance: Distance traveled.
///         time: Time spent traveling.
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
pub struct DocstringMissingReturns;

impl Violation for DocstringMissingReturns {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`return` is not documented in docstring")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Add a \"Returns\" section to the docstring"))
    }
}

/// ## What it does
/// Checks for function docstrings that have a "returns" section without
/// needing one.
///
/// ## Why is this bad?
/// Functions without an explicit return should not have a returns section
/// in their docstrings.
///
/// ## Example
/// ```python
/// def say_hello(n: int) -> None:
///     """Says hello to the user.
///
///     Args:
///         n: Number of times to say hello.
///
///     Returns:
///         Doesn't return anything.
///     """
///     for _ in range(n):
///         print("Hello!")
/// ```
///
/// Use instead:
/// ```python
/// def say_hello(n: int) -> None:
///     """Says hello to the user.
///
///     Args:
///         n: Number of times to say hello.
///     """
///     for _ in range(n):
///         print("Hello!")
/// ```
#[violation]
pub struct DocstringExtraneousReturns;

impl Violation for DocstringExtraneousReturns {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring should not have a returns section because the function doesn't return anything")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Remove the \"Returns\" section"))
    }
}

/// ## What it does
/// Checks for functions with yield statements missing a "yields" section in
/// their docstring.
///
/// ## Why is this bad?
/// Docstrings missing yields sections are a sign of incomplete documentation
/// or refactors.
///
/// ## Example
/// ```python
/// def count_to_n(n: int) -> int:
///     """Generate integers up to *n*.
///
///     Args:
///         n: The number at which to stop counting.
///     """
///     for i in range(1, n + 1):
///         yield i
/// ```
///
/// Use instead:
/// ```python
/// def count_to_n(n: int) -> int:
///     """Generate integers up to *n*.
///
///     Args:
///         n: The number at which to stop counting.
///
///     Yields:
///         int: The number we're at in the count.
///     """
///     for i in range(1, n + 1):
///         yield i
/// ```
#[violation]
pub struct DocstringMissingYields;

impl Violation for DocstringMissingYields {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`yield` is not documented in docstring")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Add a \"Yields\" section to the docstring"))
    }
}

/// ## What it does
/// Checks for function docstrings that have a "yields" section without
/// needing one.
///
/// ## Why is this bad?
/// Functions which don't yield anything should not have a yields section
/// in their docstrings.
///
/// ## Example
/// ```python
/// def say_hello(n: int) -> None:
///     """Says hello to the user.
///
///     Args:
///         n: Number of times to say hello.
///
///     Yields:
///         Doesn't yield anything.
///     """
///     for _ in range(n):
///         print("Hello!")
/// ```
///
/// Use instead:
/// ```python
/// def say_hello(n: int) -> None:
///     """Says hello to the user.
///
///     Args:
///         n: Number of times to say hello.
///     """
///     for _ in range(n):
///         print("Hello!")
/// ```
#[violation]
pub struct DocstringExtraneousYields;

impl Violation for DocstringExtraneousYields {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring has a \"Yields\" section but the function doesn't yield anything")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Remove the \"Yields\" section"))
    }
}

/// ## What it does
/// Checks for function docstrings that do not include documentation for all
/// explicitly raised exceptions.
///
/// ## Why is this bad?
/// If a function raises an exception without documenting it in its docstring,
/// it can be misleading to users and/or a sign of incomplete documentation or
/// refactors.
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

    fn fix_title(&self) -> Option<String> {
        let DocstringMissingException { id } = self;
        Some(format!("Add `{id}` to the docstring"))
    }
}

/// ## What it does
/// Checks for function docstrings that include exceptions which are not
/// explicitly raised.
///
/// ## Why is this bad?
/// Some conventions prefer non-explicit exceptions be omitted from the
/// docstring.
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

        if let [id] = ids.as_slice() {
            format!("Raised exception is not explicitly raised: `{id}`")
        } else {
            format!(
                "Raised exceptions are not explicitly raised: {}",
                ids.iter().map(|id| format!("`{id}`")).join(", ")
            )
        }
    }

    fn fix_title(&self) -> Option<String> {
        let DocstringExtraneousException { ids } = self;
        Some(format!(
            "Remove {} from the docstring",
            ids.iter().map(|id| format!("`{id}`")).join(", ")
        ))
    }
}

// A generic docstring section.
#[derive(Debug)]
struct GenericSection {
    range: TextRange,
}

impl Ranged for GenericSection {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl GenericSection {
    fn from_section(section: &SectionContext) -> Self {
        Self {
            range: section.range(),
        }
    }
}

// A Raises docstring section.
#[derive(Debug)]
struct RaisesSection<'a> {
    raised_exceptions: Vec<QualifiedName<'a>>,
    range: TextRange,
}

impl Ranged for RaisesSection<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl<'a> RaisesSection<'a> {
    /// Return the raised exceptions for the docstring, or `None` if the docstring does not contain
    /// a `Raises` section.
    fn from_section(section: &SectionContext<'a>, style: SectionStyle) -> Self {
        Self {
            raised_exceptions: parse_entries(section.following_lines_str(), style),
            range: section.range(),
        }
    }
}

#[derive(Debug, Default)]
struct DocstringSections<'a> {
    returns: Option<GenericSection>,
    yields: Option<GenericSection>,
    raises: Option<RaisesSection<'a>>,
}

impl<'a> DocstringSections<'a> {
    fn from_sections(sections: &'a SectionContexts, style: SectionStyle) -> Self {
        let mut docstring_sections = Self::default();
        for section in sections {
            match section.kind() {
                SectionKind::Raises => {
                    docstring_sections.raises = Some(RaisesSection::from_section(&section, style));
                }
                SectionKind::Returns => {
                    docstring_sections.returns = Some(GenericSection::from_section(&section));
                }
                SectionKind::Yields => {
                    docstring_sections.yields = Some(GenericSection::from_section(&section));
                }
                _ => continue,
            }
        }
        docstring_sections
    }
}

/// Parse the entries in a `Raises` section of a docstring.
fn parse_entries(content: &str, style: SectionStyle) -> Vec<QualifiedName> {
    match style {
        SectionStyle::Google => parse_entries_google(content),
        SectionStyle::Numpy => parse_entries_numpy(content),
    }
}

/// Parses Google-style docstring sections of the form:
///
/// ```python
/// Raises:
///     FasterThanLightError: If speed is greater than the speed of light.
///     DivisionByZero: If attempting to divide by zero.
/// ```
fn parse_entries_google(content: &str) -> Vec<QualifiedName> {
    let mut entries: Vec<QualifiedName> = Vec::new();
    for potential in content.lines() {
        let Some(colon_idx) = potential.find(':') else {
            continue;
        };
        let entry = potential[..colon_idx].trim();
        entries.push(QualifiedName::user_defined(entry));
    }
    entries
}

/// Parses NumPy-style docstring sections of the form:
///
/// ```python
/// Raises
/// ------
/// FasterThanLightError
///     If speed is greater than the speed of light.
/// DivisionByZero
///     If attempting to divide by zero.
/// ```
fn parse_entries_numpy(content: &str) -> Vec<QualifiedName> {
    let mut entries: Vec<QualifiedName> = Vec::new();
    let mut lines = content.lines();
    let Some(dashes) = lines.next() else {
        return entries;
    };
    let indentation = &dashes[..dashes.len() - dashes.trim_start().len()];
    for potential in lines {
        if let Some(entry) = potential.strip_prefix(indentation) {
            if let Some(first_char) = entry.chars().next() {
                if !first_char.is_whitespace() {
                    entries.push(QualifiedName::user_defined(entry.trim_end()));
                }
            }
        }
    }
    entries
}

/// An individual documentable statement in a function body.
#[derive(Debug)]
struct Entry {
    range: TextRange,
}

impl Ranged for Entry {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// An individual exception raised in a function body.
#[derive(Debug)]
struct ExceptionEntry<'a> {
    qualified_name: QualifiedName<'a>,
    range: TextRange,
}

impl Ranged for ExceptionEntry<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// A summary of documentable statements from the function body
#[derive(Debug)]
struct BodyEntries<'a> {
    returns: Vec<Entry>,
    yields: Vec<Entry>,
    raised_exceptions: Vec<ExceptionEntry<'a>>,
}

/// An AST visitor to extract a summary of documentable statements from a function body.
struct BodyVisitor<'a> {
    returns: Vec<Entry>,
    yields: Vec<Entry>,
    currently_suspended_exceptions: Option<&'a ast::Expr>,
    raised_exceptions: Vec<ExceptionEntry<'a>>,
    semantic: &'a SemanticModel<'a>,
}

impl<'a> BodyVisitor<'a> {
    fn new(semantic: &'a SemanticModel) -> Self {
        Self {
            returns: Vec::new(),
            yields: Vec::new(),
            currently_suspended_exceptions: None,
            raised_exceptions: Vec::new(),
            semantic,
        }
    }

    fn finish(self) -> BodyEntries<'a> {
        let BodyVisitor {
            returns,
            yields,
            mut raised_exceptions,
            ..
        } = self;

        // Deduplicate exceptions collected:
        // no need to complain twice about `raise TypeError` not being documented
        // just because there are two separate `raise TypeError` statements in the function
        raised_exceptions.sort_unstable_by(|left, right| {
            left.qualified_name
                .segments()
                .cmp(right.qualified_name.segments())
                .then_with(|| left.start().cmp(&right.start()))
                .then_with(|| left.end().cmp(&right.end()))
        });
        raised_exceptions.dedup_by(|left, right| {
            left.qualified_name.segments() == right.qualified_name.segments()
        });

        BodyEntries {
            returns,
            yields,
            raised_exceptions,
        }
    }
}

impl<'a> Visitor<'a> for BodyVisitor<'a> {
    fn visit_except_handler(&mut self, handler: &'a ast::ExceptHandler) {
        let ast::ExceptHandler::ExceptHandler(handler_inner) = handler;
        self.currently_suspended_exceptions = handler_inner.type_.as_deref();
        visitor::walk_except_handler(self, handler);
        self.currently_suspended_exceptions = None;
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Raise(ast::StmtRaise { exc, .. }) => {
                if let Some(exc) = exc.as_ref() {
                    if let Some(qualified_name) =
                        self.semantic.resolve_qualified_name(map_callable(exc))
                    {
                        self.raised_exceptions.push(ExceptionEntry {
                            qualified_name,
                            range: exc.range(),
                        });
                    }
                } else if let Some(exceptions) = self.currently_suspended_exceptions {
                    let mut maybe_store_exception = |exception| {
                        let Some(qualified_name) = self.semantic.resolve_qualified_name(exception)
                        else {
                            return;
                        };
                        if is_exception_or_base_exception(&qualified_name) {
                            return;
                        }
                        self.raised_exceptions.push(ExceptionEntry {
                            qualified_name,
                            range: stmt.range(),
                        });
                    };

                    if let ast::Expr::Tuple(tuple) = exceptions {
                        for exception in &tuple.elts {
                            maybe_store_exception(exception);
                        }
                    } else {
                        maybe_store_exception(exceptions);
                    }
                }
            }
            Stmt::Return(ast::StmtReturn {
                range,
                value: Some(_),
            }) => {
                self.returns.push(Entry { range: *range });
            }
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => return,
            _ => {}
        }

        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Yield(ast::ExprYield {
                range,
                value: Some(_),
            }) => {
                self.yields.push(Entry { range: *range });
            }
            Expr::YieldFrom(ast::ExprYieldFrom { range, .. }) => {
                self.yields.push(Entry { range: *range });
            }
            Expr::Lambda(_) => return,
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}

fn is_exception_or_base_exception(qualified_name: &QualifiedName) -> bool {
    matches!(
        qualified_name.segments(),
        [
            "" | "builtins",
            "BaseException" | "Exception" | "BaseExceptionGroup" | "ExceptionGroup"
        ]
    )
}

/// DOC201, DOC202, DOC402, DOC403, DOC501, DOC502
pub(crate) fn check_docstring(
    checker: &mut Checker,
    definition: &Definition,
    section_contexts: &SectionContexts,
    convention: Option<Convention>,
) {
    let mut diagnostics = Vec::new();

    // Only check function docstrings.
    let Some(function_def) = definition.as_function_def() else {
        return;
    };

    // Ignore stubs.
    if function_type::is_stub(function_def, checker.semantic()) {
        return;
    }

    // Prioritize the specified convention over the determined style.
    let docstring_sections = match convention {
        Some(Convention::Google) => {
            DocstringSections::from_sections(section_contexts, SectionStyle::Google)
        }
        Some(Convention::Numpy) => {
            DocstringSections::from_sections(section_contexts, SectionStyle::Numpy)
        }
        _ => DocstringSections::from_sections(section_contexts, section_contexts.style()),
    };

    let body_entries = {
        let mut visitor = BodyVisitor::new(checker.semantic());
        visitor.visit_body(&function_def.body);
        visitor.finish()
    };

    // DOC201
    if checker.enabled(Rule::DocstringMissingReturns) {
        if docstring_sections.returns.is_none() {
            let extra_property_decorators = checker.settings.pydocstyle.property_decorators();
            if !definition.is_property(extra_property_decorators, checker.semantic()) {
                if let Some(body_return) = body_entries.returns.first() {
                    let diagnostic = Diagnostic::new(DocstringMissingReturns, body_return.range());
                    diagnostics.push(diagnostic);
                }
            }
        }
    }

    // DOC202
    if checker.enabled(Rule::DocstringExtraneousReturns) {
        if let Some(docstring_returns) = docstring_sections.returns {
            if body_entries.returns.is_empty() {
                let diagnostic =
                    Diagnostic::new(DocstringExtraneousReturns, docstring_returns.range());
                diagnostics.push(diagnostic);
            }
        }
    }

    // DOC402
    if checker.enabled(Rule::DocstringMissingYields) {
        if docstring_sections.yields.is_none() {
            if let Some(body_yield) = body_entries.yields.first() {
                let diagnostic = Diagnostic::new(DocstringMissingYields, body_yield.range());
                diagnostics.push(diagnostic);
            }
        }
    }

    // DOC403
    if checker.enabled(Rule::DocstringExtraneousYields) {
        if let Some(docstring_yields) = docstring_sections.yields {
            if body_entries.yields.is_empty() {
                let diagnostic =
                    Diagnostic::new(DocstringExtraneousYields, docstring_yields.range());
                diagnostics.push(diagnostic);
            }
        }
    }

    // DOC501
    if checker.enabled(Rule::DocstringMissingException) {
        for body_raise in &body_entries.raised_exceptions {
            let Some(name) = body_raise.qualified_name.segments().last() else {
                continue;
            };

            if *name == "NotImplementedError" {
                continue;
            }

            if !docstring_sections.raises.as_ref().is_some_and(|section| {
                section.raised_exceptions.iter().any(|exception| {
                    body_raise
                        .qualified_name
                        .segments()
                        .ends_with(exception.segments())
                })
            }) {
                let diagnostic = Diagnostic::new(
                    DocstringMissingException {
                        id: (*name).to_string(),
                    },
                    body_raise.range(),
                );
                diagnostics.push(diagnostic);
            }
        }
    }

    // DOC502
    if checker.enabled(Rule::DocstringExtraneousException) {
        if let Some(docstring_raises) = docstring_sections.raises {
            let mut extraneous_exceptions = Vec::new();
            for docstring_raise in &docstring_raises.raised_exceptions {
                if !body_entries.raised_exceptions.iter().any(|exception| {
                    exception
                        .qualified_name
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
                    docstring_raises.range(),
                );
                diagnostics.push(diagnostic);
            }
        }
    }

    checker.diagnostics.extend(diagnostics);
}
