use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt::Display;
use std::io::Write;
use std::ops::{Deref, DerefMut};

use ruff_db::diagnostic::{
    self as db, Annotation, DiagnosticId, LintName, SecondaryCode, Severity, Span,
    ruff_create_syntax_error_diagnostic,
};
use rustc_hash::FxHashMap;

pub use azure::AzureEmitter;
pub use github::GithubEmitter;
pub use gitlab::GitlabEmitter;
pub use grouped::GroupedEmitter;
pub use json::JsonEmitter;
pub use json_lines::JsonLinesEmitter;
pub use junit::JunitEmitter;
pub use pylint::PylintEmitter;
pub use rdjson::RdjsonEmitter;
use ruff_notebook::NotebookIndex;
use ruff_python_parser::ParseError;
use ruff_source_file::{LineColumn, SourceFile};
use ruff_text_size::{Ranged, TextRange, TextSize};
pub use sarif::SarifEmitter;
pub use text::TextEmitter;

use crate::Fix;
use crate::Violation;
use crate::registry::Rule;

mod azure;
mod diff;
mod github;
mod gitlab;
mod grouped;
mod json;
mod json_lines;
mod junit;
mod pylint;
mod rdjson;
mod sarif;
mod text;

/// Create an [`OldDiagnostic`] from the given [`ParseError`].
pub fn create_parse_error_diagnostic(parse_error: &ParseError, file: SourceFile) -> OldDiagnostic {
    ruff_create_syntax_error_diagnostic(file, &parse_error.error, parse_error).into()
}

/// `OldDiagnostic` represents either a diagnostic message corresponding to a rule violation or a
/// syntax error message.
///
/// All of the information for syntax errors is captured in the underlying [`db::Diagnostic`], while
/// rule violations can have the additional optional fields like fixes, suggestions, and (parent)
/// `noqa` offsets.
///
/// For diagnostic messages, the [`db::Diagnostic`]'s primary message contains the
/// [`OldDiagnostic::body`], and the primary annotation optionally contains the suggestion
/// accompanying a fix. The `db::Diagnostic::id` field contains the kebab-case lint name derived
/// from the `Rule`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OldDiagnostic {
    pub diagnostic: db::Diagnostic,
}

impl OldDiagnostic {
    #[expect(clippy::too_many_arguments)]
    pub fn lint<B, S>(
        body: B,
        suggestion: Option<S>,
        range: TextRange,
        fix: Option<Fix>,
        parent: Option<TextSize>,
        file: SourceFile,
        noqa_offset: Option<TextSize>,
        rule: Rule,
    ) -> OldDiagnostic
    where
        B: Display,
        S: Display,
    {
        let mut diagnostic = db::Diagnostic::new(
            DiagnosticId::Lint(LintName::of(rule.into())),
            Severity::Error,
            body,
        );

        if let Some(fix) = fix {
            diagnostic.set_fix(fix);
        }

        if let Some(parent) = parent {
            diagnostic.set_parent(parent);
        }

        if let Some(noqa_offset) = noqa_offset {
            diagnostic.set_noqa_offset(noqa_offset);
        }

        let span = Span::from(file).with_range(range);
        let mut annotation = Annotation::primary(span);
        if let Some(suggestion) = suggestion {
            annotation = annotation.message(suggestion);
        }
        diagnostic.annotate(annotation);

        diagnostic.set_secondary_code(SecondaryCode::new(rule.noqa_code().to_string()));

        OldDiagnostic { diagnostic }
    }

    // TODO(brent) We temporarily allow this to avoid updating all of the call sites to add
    // references. I expect this method to go away or change significantly with the rest of the
    // diagnostic refactor, but if it still exists in this form at the end of the refactor, we
    // should just update the call sites.
    #[expect(clippy::needless_pass_by_value)]
    pub fn new<T: Violation>(kind: T, range: TextRange, file: &SourceFile) -> Self {
        Self::lint(
            Violation::message(&kind),
            Violation::fix_title(&kind),
            range,
            None,
            None,
            file.clone(),
            None,
            T::rule(),
        )
    }

    /// Returns `true` if `self` is a syntax error message.
    pub fn is_syntax_error(&self) -> bool {
        self.diagnostic.id().is_invalid_syntax()
    }

    /// Returns the name used to represent the diagnostic.
    pub fn name(&self) -> &'static str {
        if self.is_syntax_error() {
            "syntax-error"
        } else {
            self.diagnostic.id().as_str()
        }
    }

    /// Returns the message body to display to the user.
    pub fn body(&self) -> &str {
        self.diagnostic.primary_message()
    }

    /// Returns the fix suggestion for the violation.
    pub fn suggestion(&self) -> Option<&str> {
        self.diagnostic.primary_annotation()?.get_message()
    }

    /// Returns `true` if the diagnostic contains a [`Fix`].
    pub fn fixable(&self) -> bool {
        self.fix().is_some()
    }

    /// Returns the URL for the rule documentation, if it exists.
    pub fn to_url(&self) -> Option<String> {
        if self.is_syntax_error() {
            None
        } else {
            Some(format!(
                "{}/rules/{}",
                env!("CARGO_PKG_HOMEPAGE"),
                self.name()
            ))
        }
    }

    /// Returns the filename for the message.
    pub fn filename(&self) -> String {
        self.diagnostic
            .expect_primary_span()
            .expect_ruff_file()
            .name()
            .to_string()
    }

    /// Computes the start source location for the message.
    pub fn compute_start_location(&self) -> LineColumn {
        self.diagnostic
            .expect_primary_span()
            .expect_ruff_file()
            .to_source_code()
            .line_column(self.start())
    }

    /// Computes the end source location for the message.
    pub fn compute_end_location(&self) -> LineColumn {
        self.diagnostic
            .expect_primary_span()
            .expect_ruff_file()
            .to_source_code()
            .line_column(self.end())
    }

    /// Returns the [`SourceFile`] which the message belongs to.
    pub fn source_file(&self) -> SourceFile {
        self.diagnostic
            .expect_primary_span()
            .expect_ruff_file()
            .clone()
    }
}

impl From<db::Diagnostic> for OldDiagnostic {
    fn from(diagnostic: db::Diagnostic) -> Self {
        Self { diagnostic }
    }
}

impl Deref for OldDiagnostic {
    type Target = db::Diagnostic;

    fn deref(&self) -> &Self::Target {
        &self.diagnostic
    }
}

impl DerefMut for OldDiagnostic {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.diagnostic
    }
}

impl Ord for OldDiagnostic {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.source_file(), self.start()).cmp(&(other.source_file(), other.start()))
    }
}

impl PartialOrd for OldDiagnostic {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ranged for OldDiagnostic {
    fn range(&self) -> TextRange {
        self.diagnostic
            .expect_primary_span()
            .range()
            .expect("Expected range for ruff span")
    }
}

struct MessageWithLocation<'a> {
    message: &'a OldDiagnostic,
    start_location: LineColumn,
}

impl Deref for MessageWithLocation<'_> {
    type Target = OldDiagnostic;

    fn deref(&self) -> &Self::Target {
        self.message
    }
}

fn group_diagnostics_by_filename(
    diagnostics: &[OldDiagnostic],
) -> BTreeMap<String, Vec<MessageWithLocation>> {
    let mut grouped_messages = BTreeMap::default();
    for diagnostic in diagnostics {
        grouped_messages
            .entry(diagnostic.filename().to_string())
            .or_insert_with(Vec::new)
            .push(MessageWithLocation {
                message: diagnostic,
                start_location: diagnostic.compute_start_location(),
            });
    }
    grouped_messages
}

/// Display format for [`OldDiagnostic`]s.
///
/// The emitter serializes a slice of [`OldDiagnostic`]s and writes them to a [`Write`].
pub trait Emitter {
    /// Serializes the `diagnostics` and writes the output to `writer`.
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[OldDiagnostic],
        context: &EmitterContext,
    ) -> anyhow::Result<()>;
}

/// Context passed to [`Emitter`].
pub struct EmitterContext<'a> {
    notebook_indexes: &'a FxHashMap<String, NotebookIndex>,
}

impl<'a> EmitterContext<'a> {
    pub fn new(notebook_indexes: &'a FxHashMap<String, NotebookIndex>) -> Self {
        Self { notebook_indexes }
    }

    /// Tests if the file with `name` is a jupyter notebook.
    pub fn is_notebook(&self, name: &str) -> bool {
        self.notebook_indexes.contains_key(name)
    }

    pub fn notebook_index(&self, name: &str) -> Option<&NotebookIndex> {
        self.notebook_indexes.get(name)
    }
}

#[cfg(test)]
mod tests {
    use rustc_hash::FxHashMap;

    use crate::codes::Rule;
    use crate::{Edit, Fix};
    use ruff_notebook::NotebookIndex;
    use ruff_python_parser::{Mode, ParseOptions, parse_unchecked};
    use ruff_source_file::{OneIndexed, SourceFileBuilder};
    use ruff_text_size::{TextRange, TextSize};

    use crate::message::{Emitter, EmitterContext, OldDiagnostic, create_parse_error_diagnostic};

    pub(super) fn create_syntax_error_diagnostics() -> Vec<OldDiagnostic> {
        let source = r"from os import

if call(foo
    def bar():
        pass
";
        let source_file = SourceFileBuilder::new("syntax_errors.py", source).finish();
        parse_unchecked(source, ParseOptions::from(Mode::Module))
            .errors()
            .iter()
            .map(|parse_error| create_parse_error_diagnostic(parse_error, source_file.clone()))
            .collect()
    }

    pub(super) fn create_diagnostics() -> Vec<OldDiagnostic> {
        let fib = r#"import os


def fibonacci(n):
    """Compute the nth number in the Fibonacci sequence."""
    x = 1
    if n == 0:
        return 0
    elif n == 1:
        return 1
    else:
        return fibonacci(n - 1) + fibonacci(n - 2)
"#;

        let fib_source = SourceFileBuilder::new("fib.py", fib).finish();

        let unused_import_start = TextSize::from(7);
        let unused_import = OldDiagnostic::lint(
            "`os` imported but unused",
            Some("Remove unused import: `os`"),
            TextRange::new(unused_import_start, TextSize::from(9)),
            Some(Fix::unsafe_edit(Edit::range_deletion(TextRange::new(
                TextSize::from(0),
                TextSize::from(10),
            )))),
            None,
            fib_source.clone(),
            Some(unused_import_start),
            Rule::UnusedImport,
        );

        let unused_variable_start = TextSize::from(94);
        let unused_variable = OldDiagnostic::lint(
            "Local variable `x` is assigned to but never used",
            Some("Remove assignment to unused variable `x`"),
            TextRange::new(unused_variable_start, TextSize::from(95)),
            Some(Fix::unsafe_edit(Edit::deletion(
                TextSize::from(94),
                TextSize::from(99),
            ))),
            None,
            fib_source,
            Some(unused_variable_start),
            Rule::UnusedVariable,
        );

        let file_2 = r"if a == 1: pass";

        let undefined_name_start = TextSize::from(3);
        let undefined_name = OldDiagnostic::lint(
            "Undefined name `a`",
            Option::<&'static str>::None,
            TextRange::new(undefined_name_start, TextSize::from(4)),
            None,
            None,
            SourceFileBuilder::new("undef.py", file_2).finish(),
            Some(undefined_name_start),
            Rule::UndefinedName,
        );

        vec![unused_import, unused_variable, undefined_name]
    }

    pub(super) fn create_notebook_diagnostics()
    -> (Vec<OldDiagnostic>, FxHashMap<String, NotebookIndex>) {
        let notebook = r"# cell 1
import os
# cell 2
import math

print('hello world')
# cell 3
def foo():
    print()
    x = 1
";

        let notebook_source = SourceFileBuilder::new("notebook.ipynb", notebook).finish();

        let unused_import_os_start = TextSize::from(16);
        let unused_import_os = OldDiagnostic::lint(
            "`os` imported but unused",
            Some("Remove unused import: `os`"),
            TextRange::new(unused_import_os_start, TextSize::from(18)),
            Some(Fix::safe_edit(Edit::range_deletion(TextRange::new(
                TextSize::from(9),
                TextSize::from(19),
            )))),
            None,
            notebook_source.clone(),
            Some(unused_import_os_start),
            Rule::UnusedImport,
        );

        let unused_import_math_start = TextSize::from(35);
        let unused_import_math = OldDiagnostic::lint(
            "`math` imported but unused",
            Some("Remove unused import: `math`"),
            TextRange::new(unused_import_math_start, TextSize::from(39)),
            Some(Fix::safe_edit(Edit::range_deletion(TextRange::new(
                TextSize::from(28),
                TextSize::from(40),
            )))),
            None,
            notebook_source.clone(),
            Some(unused_import_math_start),
            Rule::UnusedImport,
        );

        let unused_variable_start = TextSize::from(98);
        let unused_variable = OldDiagnostic::lint(
            "Local variable `x` is assigned to but never used",
            Some("Remove assignment to unused variable `x`"),
            TextRange::new(unused_variable_start, TextSize::from(99)),
            Some(Fix::unsafe_edit(Edit::deletion(
                TextSize::from(94),
                TextSize::from(104),
            ))),
            None,
            notebook_source,
            Some(unused_variable_start),
            Rule::UnusedVariable,
        );

        let mut notebook_indexes = FxHashMap::default();
        notebook_indexes.insert(
            "notebook.ipynb".to_string(),
            NotebookIndex::new(
                vec![
                    OneIndexed::from_zero_indexed(0),
                    OneIndexed::from_zero_indexed(0),
                    OneIndexed::from_zero_indexed(1),
                    OneIndexed::from_zero_indexed(1),
                    OneIndexed::from_zero_indexed(1),
                    OneIndexed::from_zero_indexed(1),
                    OneIndexed::from_zero_indexed(2),
                    OneIndexed::from_zero_indexed(2),
                    OneIndexed::from_zero_indexed(2),
                    OneIndexed::from_zero_indexed(2),
                ],
                vec![
                    OneIndexed::from_zero_indexed(0),
                    OneIndexed::from_zero_indexed(1),
                    OneIndexed::from_zero_indexed(0),
                    OneIndexed::from_zero_indexed(1),
                    OneIndexed::from_zero_indexed(2),
                    OneIndexed::from_zero_indexed(3),
                    OneIndexed::from_zero_indexed(0),
                    OneIndexed::from_zero_indexed(1),
                    OneIndexed::from_zero_indexed(2),
                    OneIndexed::from_zero_indexed(3),
                ],
            ),
        );

        (
            vec![unused_import_os, unused_import_math, unused_variable],
            notebook_indexes,
        )
    }

    pub(super) fn capture_emitter_output(
        emitter: &mut dyn Emitter,
        diagnostics: &[OldDiagnostic],
    ) -> String {
        let notebook_indexes = FxHashMap::default();
        let context = EmitterContext::new(&notebook_indexes);
        let mut output: Vec<u8> = Vec::new();
        emitter.emit(&mut output, diagnostics, &context).unwrap();

        String::from_utf8(output).expect("Output to be valid UTF-8")
    }

    pub(super) fn capture_emitter_notebook_output(
        emitter: &mut dyn Emitter,
        diagnostics: &[OldDiagnostic],
        notebook_indexes: &FxHashMap<String, NotebookIndex>,
    ) -> String {
        let context = EmitterContext::new(notebook_indexes);
        let mut output: Vec<u8> = Vec::new();
        emitter.emit(&mut output, diagnostics, &context).unwrap();

        String::from_utf8(output).expect("Output to be valid UTF-8")
    }
}
