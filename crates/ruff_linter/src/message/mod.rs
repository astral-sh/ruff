use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::io::Write;
use std::ops::Deref;

use ruff_db::diagnostic::{self as db, Annotation, DiagnosticId, LintName, Severity, Span};
use ruff_python_parser::semantic_errors::SemanticSyntaxError;
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
use ruff_diagnostics::{Diagnostic, Fix};
use ruff_notebook::NotebookIndex;
use ruff_python_parser::{ParseError, UnsupportedSyntaxError};
use ruff_source_file::{LineColumn, SourceFile};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
pub use sarif::SarifEmitter;
pub use text::TextEmitter;

use crate::Locator;
use crate::codes::NoqaCode;
use crate::logging::DisplayParseErrorType;
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

/// Message represents either a diagnostic message corresponding to a rule violation or a syntax
/// error message.
///
/// All of the information for syntax errors is captured in the underlying [`db::Diagnostic`], while
/// rule violations can have the additional optional fields like fixes, suggestions, and (parent)
/// `noqa` offsets.
///
/// For diagnostic messages, the [`db::Diagnostic`]'s primary message contains the
/// [`Diagnostic::body`], and the primary annotation optionally contains the suggestion accompanying
/// a fix. The `db::Diagnostic::id` field contains the kebab-case lint name derived from the `Rule`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub diagnostic: db::Diagnostic,

    // these fields are specific to rule violations
    pub fix: Option<Fix>,
    pub parent: Option<TextSize>,
    pub(crate) noqa_offset: Option<TextSize>,
}

impl Message {
    pub fn syntax_error(
        message: impl std::fmt::Display,
        range: TextRange,
        file: SourceFile,
    ) -> Message {
        let mut diag = db::Diagnostic::new(DiagnosticId::InvalidSyntax, Severity::Error, message);
        let span = Span::from(file).with_range(range);
        diag.annotate(Annotation::primary(span));
        Self {
            diagnostic: diag,
            fix: None,
            parent: None,
            noqa_offset: None,
        }
    }

    #[expect(clippy::too_many_arguments)]
    pub fn diagnostic(
        name: &'static str,
        body: String,
        suggestion: Option<String>,
        range: TextRange,
        fix: Option<Fix>,
        parent: Option<TextSize>,
        file: SourceFile,
        noqa_offset: Option<TextSize>,
    ) -> Message {
        let mut diagnostic = db::Diagnostic::new(
            DiagnosticId::Lint(LintName::of(name)),
            Severity::Error,
            body,
        );
        let span = Span::from(file).with_range(range);
        let mut annotation = Annotation::primary(span);
        if let Some(suggestion) = suggestion {
            annotation = annotation.message(suggestion);
        }
        diagnostic.annotate(annotation);

        Message {
            diagnostic,
            fix,
            parent,
            noqa_offset,
        }
    }

    /// Create a [`Message`] from the given [`Diagnostic`] corresponding to a rule violation.
    pub fn from_diagnostic(
        diagnostic: Diagnostic,
        file: SourceFile,
        noqa_offset: Option<TextSize>,
    ) -> Message {
        let Diagnostic {
            name,
            body,
            suggestion,
            range,
            fix,
            parent,
        } = diagnostic;
        Self::diagnostic(
            name,
            body,
            suggestion,
            range,
            fix,
            parent,
            file,
            noqa_offset,
        )
    }

    /// Create a [`Message`] from the given [`ParseError`].
    pub fn from_parse_error(
        parse_error: &ParseError,
        locator: &Locator,
        file: SourceFile,
    ) -> Message {
        // Try to create a non-empty range so that the diagnostic can print a caret at the right
        // position. This requires that we retrieve the next character, if any, and take its length
        // to maintain char-boundaries.
        let len = locator
            .after(parse_error.location.start())
            .chars()
            .next()
            .map_or(TextSize::new(0), TextLen::text_len);

        Message::syntax_error(
            format_args!(
                "SyntaxError: {}",
                DisplayParseErrorType::new(&parse_error.error)
            ),
            TextRange::at(parse_error.location.start(), len),
            file,
        )
    }

    /// Create a [`Message`] from the given [`UnsupportedSyntaxError`].
    pub fn from_unsupported_syntax_error(
        unsupported_syntax_error: &UnsupportedSyntaxError,
        file: SourceFile,
    ) -> Message {
        Message::syntax_error(
            format_args!("SyntaxError: {unsupported_syntax_error}"),
            unsupported_syntax_error.range,
            file,
        )
    }

    /// Create a [`Message`] from the given [`SemanticSyntaxError`].
    pub fn from_semantic_syntax_error(
        semantic_syntax_error: &SemanticSyntaxError,
        file: SourceFile,
    ) -> Message {
        Message::syntax_error(
            format_args!("SyntaxError: {semantic_syntax_error}"),
            semantic_syntax_error.range,
            file,
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

    /// Returns the offset at which the `noqa` comment will be placed if it's a diagnostic message.
    pub fn noqa_offset(&self) -> Option<TextSize> {
        self.noqa_offset
    }

    /// Returns the [`Fix`] for the message, if there is any.
    pub fn fix(&self) -> Option<&Fix> {
        self.fix.as_ref()
    }

    /// Returns `true` if the message contains a [`Fix`].
    pub fn fixable(&self) -> bool {
        self.fix().is_some()
    }

    /// Returns the [`Rule`] corresponding to the diagnostic message.
    pub fn to_rule(&self) -> Option<Rule> {
        if self.is_syntax_error() {
            None
        } else {
            Some(self.name().parse().expect("Expected a valid rule name"))
        }
    }

    /// Returns the [`NoqaCode`] corresponding to the diagnostic message.
    pub fn to_noqa_code(&self) -> Option<NoqaCode> {
        self.to_rule().map(|rule| rule.noqa_code())
    }

    /// Returns the URL for the rule documentation, if it exists.
    pub fn to_url(&self) -> Option<String> {
        // TODO(brent) Rule::url calls Rule::explanation, which calls ViolationMetadata::explain,
        // which when derived (seems always to be the case?) is always `Some`, so I think it's
        // pretty safe to inline the Rule::url implementation here, using `self.name()`:
        //
        // format!("{}/rules/{}", env!("CARGO_PKG_HOMEPAGE"), self.name())
        //
        // at least in the case of diagnostics, I guess syntax errors will return None
        self.to_rule().and_then(|rule| rule.url())
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

impl Ord for Message {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.source_file(), self.start()).cmp(&(other.source_file(), other.start()))
    }
}

impl PartialOrd for Message {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ranged for Message {
    fn range(&self) -> TextRange {
        self.diagnostic
            .expect_primary_span()
            .range()
            .expect("Expected range for ruff span")
    }
}

struct MessageWithLocation<'a> {
    message: &'a Message,
    start_location: LineColumn,
}

impl Deref for MessageWithLocation<'_> {
    type Target = Message;

    fn deref(&self) -> &Self::Target {
        self.message
    }
}

fn group_messages_by_filename(messages: &[Message]) -> BTreeMap<String, Vec<MessageWithLocation>> {
    let mut grouped_messages = BTreeMap::default();
    for message in messages {
        grouped_messages
            .entry(message.filename().to_string())
            .or_insert_with(Vec::new)
            .push(MessageWithLocation {
                message,
                start_location: message.compute_start_location(),
            });
    }
    grouped_messages
}

/// Display format for a [`Message`]s.
///
/// The emitter serializes a slice of [`Message`]'s and writes them to a [`Write`].
pub trait Emitter {
    /// Serializes the `messages` and writes the output to `writer`.
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        messages: &[Message],
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

    use ruff_diagnostics::{Edit, Fix};
    use ruff_notebook::NotebookIndex;
    use ruff_python_parser::{Mode, ParseOptions, parse_unchecked};
    use ruff_source_file::{OneIndexed, SourceFileBuilder};
    use ruff_text_size::{TextRange, TextSize};

    use crate::Locator;
    use crate::message::{Emitter, EmitterContext, Message};

    pub(super) fn create_syntax_error_messages() -> Vec<Message> {
        let source = r"from os import

if call(foo
    def bar():
        pass
";
        let locator = Locator::new(source);
        let source_file = SourceFileBuilder::new("syntax_errors.py", source).finish();
        parse_unchecked(source, ParseOptions::from(Mode::Module))
            .errors()
            .iter()
            .map(|parse_error| {
                Message::from_parse_error(parse_error, &locator, source_file.clone())
            })
            .collect()
    }

    pub(super) fn create_messages() -> Vec<Message> {
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
        let unused_import = Message::diagnostic(
            "unused-import",
            "`os` imported but unused".to_string(),
            Some("Remove unused import: `os`".to_string()),
            TextRange::new(unused_import_start, TextSize::from(9)),
            Some(Fix::unsafe_edit(Edit::range_deletion(TextRange::new(
                TextSize::from(0),
                TextSize::from(10),
            )))),
            None,
            fib_source.clone(),
            Some(unused_import_start),
        );

        let unused_variable_start = TextSize::from(94);
        let unused_variable = Message::diagnostic(
            "unused-variable",
            "Local variable `x` is assigned to but never used".to_string(),
            Some("Remove assignment to unused variable `x`".to_string()),
            TextRange::new(unused_variable_start, TextSize::from(95)),
            Some(Fix::unsafe_edit(Edit::deletion(
                TextSize::from(94),
                TextSize::from(99),
            ))),
            None,
            fib_source,
            Some(unused_variable_start),
        );

        let file_2 = r"if a == 1: pass";

        let undefined_name_start = TextSize::from(3);
        let undefined_name = Message::diagnostic(
            "undefined-name",
            "Undefined name `a`".to_string(),
            None,
            TextRange::new(undefined_name_start, TextSize::from(4)),
            None,
            None,
            SourceFileBuilder::new("undef.py", file_2).finish(),
            Some(undefined_name_start),
        );

        vec![unused_import, unused_variable, undefined_name]
    }

    pub(super) fn create_notebook_messages() -> (Vec<Message>, FxHashMap<String, NotebookIndex>) {
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
        let unused_import_os = Message::diagnostic(
            "unused-import",
            "`os` imported but unused".to_string(),
            Some("Remove unused import: `os`".to_string()),
            TextRange::new(unused_import_os_start, TextSize::from(18)),
            Some(Fix::safe_edit(Edit::range_deletion(TextRange::new(
                TextSize::from(9),
                TextSize::from(19),
            )))),
            None,
            notebook_source.clone(),
            Some(unused_import_os_start),
        );

        let unused_import_math_start = TextSize::from(35);
        let unused_import_math = Message::diagnostic(
            "unused-import",
            "`math` imported but unused".to_string(),
            Some("Remove unused import: `math`".to_string()),
            TextRange::new(unused_import_math_start, TextSize::from(39)),
            Some(Fix::safe_edit(Edit::range_deletion(TextRange::new(
                TextSize::from(28),
                TextSize::from(40),
            )))),
            None,
            notebook_source.clone(),
            Some(unused_import_math_start),
        );

        let unused_variable_start = TextSize::from(98);
        let unused_variable = Message::diagnostic(
            "unused-variable",
            "Local variable `x` is assigned to but never used".to_string(),
            Some("Remove assignment to unused variable `x`".to_string()),
            TextRange::new(unused_variable_start, TextSize::from(99)),
            Some(Fix::unsafe_edit(Edit::deletion(
                TextSize::from(94),
                TextSize::from(104),
            ))),
            None,
            notebook_source,
            Some(unused_variable_start),
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
        messages: &[Message],
    ) -> String {
        let notebook_indexes = FxHashMap::default();
        let context = EmitterContext::new(&notebook_indexes);
        let mut output: Vec<u8> = Vec::new();
        emitter.emit(&mut output, messages, &context).unwrap();

        String::from_utf8(output).expect("Output to be valid UTF-8")
    }

    pub(super) fn capture_emitter_notebook_output(
        emitter: &mut dyn Emitter,
        messages: &[Message],
        notebook_indexes: &FxHashMap<String, NotebookIndex>,
    ) -> String {
        let context = EmitterContext::new(notebook_indexes);
        let mut output: Vec<u8> = Vec::new();
        emitter.emit(&mut output, messages, &context).unwrap();

        String::from_utf8(output).expect("Output to be valid UTF-8")
    }
}
