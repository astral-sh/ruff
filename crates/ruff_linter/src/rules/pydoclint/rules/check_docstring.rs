use itertools::Itertools;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, Stmt, visitor};
use ruff_python_semantic::analyze::{function_type, visibility};
use ruff_python_semantic::{Definition, SemanticModel};
use ruff_source_file::NewlineWithTrailingNewline;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::docstrings::sections::{SectionContext, SectionContexts, SectionKind};
use crate::docstrings::styles::SectionStyle;
use crate::registry::Rule;
use crate::rules::pydocstyle::settings::Convention;

/// ## What it does
/// Checks for functions with `return` statements that do not have "Returns"
/// sections in their docstrings.
///
/// ## Why is this bad?
/// A missing "Returns" section is a sign of incomplete documentation.
///
/// This rule is not enforced for abstract methods or functions that only return
/// `None`. It is also ignored for "stub functions": functions where the body only
/// consists of `pass`, `...`, `raise NotImplementedError`, or similar.
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
#[derive(ViolationMetadata)]
pub(crate) struct DocstringMissingReturns;

impl Violation for DocstringMissingReturns {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`return` is not documented in docstring".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add a \"Returns\" section to the docstring".to_string())
    }
}

/// ## What it does
/// Checks for function docstrings with unnecessary "Returns" sections.
///
/// ## Why is this bad?
/// A function without an explicit `return` statement should not have a
/// "Returns" section in its docstring.
///
/// This rule is not enforced for abstract methods. It is also ignored for
/// "stub functions": functions where the body only consists of `pass`, `...`,
/// `raise NotImplementedError`, or similar.
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
#[derive(ViolationMetadata)]
pub(crate) struct DocstringExtraneousReturns;

impl Violation for DocstringExtraneousReturns {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Docstring should not have a returns section because the function doesn't return anything"
            .to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove the \"Returns\" section".to_string())
    }
}

/// ## What it does
/// Checks for functions with `yield` statements that do not have "Yields" sections in
/// their docstrings.
///
/// ## Why is this bad?
/// A missing "Yields" section is a sign of incomplete documentation.
///
/// This rule is not enforced for abstract methods or functions that only yield `None`.
/// It is also ignored for "stub functions": functions where the body only consists
/// of `pass`, `...`, `raise NotImplementedError`, or similar.
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
#[derive(ViolationMetadata)]
pub(crate) struct DocstringMissingYields;

impl Violation for DocstringMissingYields {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`yield` is not documented in docstring".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add a \"Yields\" section to the docstring".to_string())
    }
}

/// ## What it does
/// Checks for function docstrings with unnecessary "Yields" sections.
///
/// ## Why is this bad?
/// A function that doesn't yield anything should not have a "Yields" section
/// in its docstring.
///
/// This rule is not enforced for abstract methods. It is also ignored for
/// "stub functions": functions where the body only consists of `pass`, `...`,
/// `raise NotImplementedError`, or similar.
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
#[derive(ViolationMetadata)]
pub(crate) struct DocstringExtraneousYields;

impl Violation for DocstringExtraneousYields {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Docstring has a \"Yields\" section but the function doesn't yield anything".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove the \"Yields\" section".to_string())
    }
}

/// ## What it does
/// Checks for function docstrings that do not document all explicitly raised
/// exceptions.
///
/// ## Why is this bad?
/// A function should document all exceptions that are directly raised in some
/// circumstances. Failing to document an exception that could be raised
/// can be misleading to users and/or a sign of incomplete documentation.
///
/// This rule is not enforced for abstract methods. It is also ignored for
/// "stub functions": functions where the body only consists of `pass`, `...`,
/// `raise NotImplementedError`, or similar.
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
#[derive(ViolationMetadata)]
pub(crate) struct DocstringMissingException {
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
/// Checks for function docstrings that state that exceptions could be raised
/// even though they are not directly raised in the function body.
///
/// ## Why is this bad?
/// Some conventions prefer non-explicit exceptions be omitted from the
/// docstring.
///
/// This rule is not enforced for abstract methods. It is also ignored for
/// "stub functions": functions where the body only consists of `pass`, `...`,
/// `raise NotImplementedError`, or similar.
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
///
/// ## Known issues
/// It may often be desirable to document *all* exceptions that a function
/// could possibly raise, even those which are not explicitly raised using
/// `raise` statements in the function body.
#[derive(ViolationMetadata)]
pub(crate) struct DocstringExtraneousException {
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

/// A generic docstring section.
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

/// A "Raises" section in a docstring.
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
    /// a "Raises" section.
    fn from_section(section: &SectionContext<'a>, style: Option<SectionStyle>) -> Self {
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
    fn from_sections(sections: &'a SectionContexts, style: Option<SectionStyle>) -> Self {
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

/// Parse the entries in a "Raises" section of a docstring.
///
/// Attempts to parse using the specified [`SectionStyle`], falling back to the other style if no
/// entries are found.
fn parse_entries(content: &str, style: Option<SectionStyle>) -> Vec<QualifiedName> {
    match style {
        Some(SectionStyle::Google) => parse_entries_google(content),
        Some(SectionStyle::Numpy) => parse_entries_numpy(content),
        None => {
            let entries = parse_entries_google(content);
            if entries.is_empty() {
                parse_entries_numpy(content)
            } else {
                entries
            }
        }
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

/// An individual `yield` expression in a function body.
#[derive(Debug)]
struct YieldEntry {
    range: TextRange,
    is_none_yield: bool,
}

impl Ranged for YieldEntry {
    fn range(&self) -> TextRange {
        self.range
    }
}

#[expect(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReturnEntryKind {
    NotNone,
    ImplicitNone,
    ExplicitNone,
}

/// An individual `return` statement in a function body.
#[derive(Debug)]
struct ReturnEntry {
    range: TextRange,
    kind: ReturnEntryKind,
}

impl ReturnEntry {
    const fn is_none_return(&self) -> bool {
        matches!(
            &self.kind,
            ReturnEntryKind::ExplicitNone | ReturnEntryKind::ImplicitNone
        )
    }

    const fn is_implicit(&self) -> bool {
        matches!(&self.kind, ReturnEntryKind::ImplicitNone)
    }
}

impl Ranged for ReturnEntry {
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
    returns: Vec<ReturnEntry>,
    yields: Vec<YieldEntry>,
    raised_exceptions: Vec<ExceptionEntry<'a>>,
}

/// An AST visitor to extract a summary of documentable statements from a function body.
struct BodyVisitor<'a> {
    returns: Vec<ReturnEntry>,
    yields: Vec<YieldEntry>,
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
                        for exception in tuple {
                            maybe_store_exception(exception);
                        }
                    } else {
                        maybe_store_exception(exceptions);
                    }
                }
            }
            Stmt::Return(ast::StmtReturn {
                range,
                value: Some(value),
            }) => {
                self.returns.push(ReturnEntry {
                    range: *range,
                    kind: if value.is_none_literal_expr() {
                        ReturnEntryKind::ExplicitNone
                    } else {
                        ReturnEntryKind::NotNone
                    },
                });
            }
            Stmt::Return(ast::StmtReturn { range, value: None }) => {
                self.returns.push(ReturnEntry {
                    range: *range,
                    kind: ReturnEntryKind::ImplicitNone,
                });
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
                value: Some(value),
            }) => {
                self.yields.push(YieldEntry {
                    range: *range,
                    is_none_yield: value.is_none_literal_expr(),
                });
            }
            Expr::Yield(ast::ExprYield { range, value: None }) => {
                self.yields.push(YieldEntry {
                    range: *range,
                    is_none_yield: true,
                });
            }
            Expr::YieldFrom(ast::ExprYieldFrom { range, .. }) => {
                self.yields.push(YieldEntry {
                    range: *range,
                    is_none_yield: false,
                });
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

fn starts_with_returns(docstring: &Docstring) -> bool {
    if let Some(first_word) = docstring.body().as_str().split(' ').next() {
        return matches!(first_word, "Return" | "Returns");
    }
    false
}

fn returns_documented(
    docstring: &Docstring,
    docstring_sections: &DocstringSections,
    convention: Option<Convention>,
) -> bool {
    docstring_sections.returns.is_some()
        || (matches!(convention, Some(Convention::Google)) && starts_with_returns(docstring))
}

fn should_document_returns(function_def: &ast::StmtFunctionDef) -> bool {
    !matches!(function_def.name.as_str(), "__new__")
}

fn starts_with_yields(docstring: &Docstring) -> bool {
    if let Some(first_word) = docstring.body().as_str().split(' ').next() {
        return matches!(first_word, "Yield" | "Yields");
    }
    false
}

fn yields_documented(
    docstring: &Docstring,
    docstring_sections: &DocstringSections,
    convention: Option<Convention>,
) -> bool {
    docstring_sections.yields.is_some()
        || (matches!(convention, Some(Convention::Google)) && starts_with_yields(docstring))
}

#[derive(Debug, Copy, Clone)]
enum GeneratorOrIteratorArguments<'a> {
    Unparameterized,
    Single(&'a Expr),
    Several(&'a [Expr]),
}

impl<'a> GeneratorOrIteratorArguments<'a> {
    fn first(self) -> Option<&'a Expr> {
        match self {
            Self::Unparameterized => None,
            Self::Single(element) => Some(element),
            Self::Several(elements) => elements.first(),
        }
    }

    fn indicates_none_returned(self) -> bool {
        match self {
            Self::Unparameterized => true,
            Self::Single(_) => true,
            Self::Several(elements) => elements.get(2).is_none_or(Expr::is_none_literal_expr),
        }
    }
}

/// Returns the arguments to a generator annotation, if it exists.
fn generator_annotation_arguments<'a>(
    expr: &'a Expr,
    semantic: &'a SemanticModel,
) -> Option<GeneratorOrIteratorArguments<'a>> {
    let qualified_name = semantic.resolve_qualified_name(map_subscript(expr))?;
    match qualified_name.segments() {
        [
            "typing" | "typing_extensions",
            "Iterable" | "AsyncIterable" | "Iterator" | "AsyncIterator",
        ]
        | [
            "collections",
            "abc",
            "Iterable" | "AsyncIterable" | "Iterator" | "AsyncIterator",
        ] => match expr {
            Expr::Subscript(ast::ExprSubscript { slice, .. }) => {
                Some(GeneratorOrIteratorArguments::Single(slice))
            }
            _ => Some(GeneratorOrIteratorArguments::Unparameterized),
        },
        [
            "typing" | "typing_extensions",
            "Generator" | "AsyncGenerator",
        ]
        | ["collections", "abc", "Generator" | "AsyncGenerator"] => match expr {
            Expr::Subscript(ast::ExprSubscript { slice, .. }) => {
                if let Expr::Tuple(tuple) = &**slice {
                    Some(GeneratorOrIteratorArguments::Several(tuple.elts.as_slice()))
                } else {
                    // `Generator[int]` implies `Generator[int, None, None]`
                    // as it uses a PEP-696 TypeVar with default values
                    Some(GeneratorOrIteratorArguments::Single(slice))
                }
            }
            _ => Some(GeneratorOrIteratorArguments::Unparameterized),
        },
        _ => None,
    }
}

fn is_generator_function_annotated_as_returning_none(
    entries: &BodyEntries,
    return_annotations: &Expr,
    semantic: &SemanticModel,
) -> bool {
    if entries.yields.is_empty() {
        return false;
    }
    generator_annotation_arguments(return_annotations, semantic)
        .is_some_and(GeneratorOrIteratorArguments::indicates_none_returned)
}

fn is_one_line(docstring: &Docstring) -> bool {
    let mut non_empty_line_count = 0;
    for line in NewlineWithTrailingNewline::from(docstring.body().as_str()) {
        if !line.trim().is_empty() {
            non_empty_line_count += 1;
        }
        if non_empty_line_count > 1 {
            return false;
        }
    }
    true
}

/// DOC201, DOC202, DOC402, DOC403, DOC501, DOC502
pub(crate) fn check_docstring(
    checker: &Checker,
    definition: &Definition,
    docstring: &Docstring,
    section_contexts: &SectionContexts,
    convention: Option<Convention>,
) {
    // Only check function docstrings.
    let Some(function_def) = definition.as_function_def() else {
        return;
    };

    if checker.settings.pydoclint.ignore_one_line_docstrings && is_one_line(docstring) {
        return;
    }

    let semantic = checker.semantic();

    if function_type::is_stub(function_def, semantic) {
        return;
    }

    // Prioritize the specified convention over the determined style.
    let docstring_sections = match convention {
        Some(Convention::Google) => {
            DocstringSections::from_sections(section_contexts, Some(SectionStyle::Google))
        }
        Some(Convention::Numpy) => {
            DocstringSections::from_sections(section_contexts, Some(SectionStyle::Numpy))
        }
        Some(Convention::Pep257) | None => DocstringSections::from_sections(section_contexts, None),
    };

    let body_entries = {
        let mut visitor = BodyVisitor::new(semantic);
        visitor.visit_body(&function_def.body);
        visitor.finish()
    };

    // DOC201
    if checker.enabled(Rule::DocstringMissingReturns) {
        if should_document_returns(function_def)
            && !returns_documented(docstring, &docstring_sections, convention)
        {
            let extra_property_decorators = checker.settings.pydocstyle.property_decorators();
            if !definition.is_property(extra_property_decorators, semantic) {
                if !body_entries.returns.is_empty() {
                    match function_def.returns.as_deref() {
                        Some(returns) => {
                            // Ignore it if it's annotated as returning `None`
                            // or it's a generator function annotated as returning `None`,
                            // i.e. any of `-> None`, `-> Iterator[...]` or `-> Generator[..., ..., None]`
                            if !returns.is_none_literal_expr()
                                && !is_generator_function_annotated_as_returning_none(
                                    &body_entries,
                                    returns,
                                    semantic,
                                )
                            {
                                checker.report_diagnostic(Diagnostic::new(
                                    DocstringMissingReturns,
                                    docstring.range(),
                                ));
                            }
                        }
                        None if body_entries
                            .returns
                            .iter()
                            .any(|entry| !entry.is_none_return()) =>
                        {
                            checker.report_diagnostic(Diagnostic::new(
                                DocstringMissingReturns,
                                docstring.range(),
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // DOC402
    if checker.enabled(Rule::DocstringMissingYields) {
        if !yields_documented(docstring, &docstring_sections, convention) {
            if !body_entries.yields.is_empty() {
                match function_def.returns.as_deref() {
                    Some(returns)
                        if !generator_annotation_arguments(returns, semantic).is_some_and(
                            |arguments| arguments.first().is_none_or(Expr::is_none_literal_expr),
                        ) =>
                    {
                        checker.report_diagnostic(Diagnostic::new(
                            DocstringMissingYields,
                            docstring.range(),
                        ));
                    }
                    None if body_entries.yields.iter().any(|entry| !entry.is_none_yield) => {
                        checker.report_diagnostic(Diagnostic::new(
                            DocstringMissingYields,
                            docstring.range(),
                        ));
                    }
                    _ => {}
                }
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
                    docstring.range(),
                );
                checker.report_diagnostic(diagnostic);
            }
        }
    }

    // Avoid applying "extraneous" rules to abstract methods. An abstract method's docstring _could_
    // document that it raises an exception without including the exception in the implementation.
    if !visibility::is_abstract(&function_def.decorator_list, semantic) {
        // DOC202
        if checker.enabled(Rule::DocstringExtraneousReturns) {
            if docstring_sections.returns.is_some() {
                if body_entries.returns.is_empty()
                    || body_entries.returns.iter().all(ReturnEntry::is_implicit)
                {
                    let diagnostic = Diagnostic::new(DocstringExtraneousReturns, docstring.range());
                    checker.report_diagnostic(diagnostic);
                }
            }
        }

        // DOC403
        if checker.enabled(Rule::DocstringExtraneousYields) {
            if docstring_sections.yields.is_some() {
                if body_entries.yields.is_empty() {
                    let diagnostic = Diagnostic::new(DocstringExtraneousYields, docstring.range());
                    checker.report_diagnostic(diagnostic);
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
                        docstring.range(),
                    );
                    checker.report_diagnostic(diagnostic);
                }
            }
        }
    }
}
