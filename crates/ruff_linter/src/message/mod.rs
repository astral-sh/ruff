use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::io::Write;
use std::ops::Deref;

use ruff_db::diagnostic::{self as db, Annotation, DiagnosticId, Severity, Span};
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
use crate::logging::DisplayParseErrorType;
use crate::registry::{AsRule, Rule};

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
/// error message raised by the parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Diagnostic(DiagnosticMessage),
    SyntaxError(db::Diagnostic),
}

/// A diagnostic message corresponding to a rule violation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticMessage {
    pub name: &'static str,
    pub body: String,
    pub suggestion: Option<String>,
    pub range: TextRange,
    pub fix: Option<Fix>,
    pub parent: Option<TextSize>,
    pub file: SourceFile,
    pub noqa_offset: TextSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessageKind {
    Diagnostic(Rule),
    SyntaxError,
}

impl MessageKind {
    pub fn as_str(&self) -> &str {
        match self {
            MessageKind::Diagnostic(rule) => rule.as_ref(),
            MessageKind::SyntaxError => "syntax-error",
        }
    }
}

impl Message {
    pub fn syntax_error(
        message: impl std::fmt::Display,
        range: TextRange,
        file: SourceFile,
    ) -> Message {
        let mut diag = db::Diagnostic::new(DiagnosticId::InvalidSyntax, Severity::Error, "");
        let span = Span::from(file).with_range(range);
        diag.annotate(Annotation::primary(span).message(message));
        Self::SyntaxError(diag)
    }

    /// Create a [`Message`] from the given [`Diagnostic`] corresponding to a rule violation.
    pub fn from_diagnostic(
        diagnostic: Diagnostic,
        file: SourceFile,
        noqa_offset: TextSize,
    ) -> Message {
        Message::Diagnostic(DiagnosticMessage {
            range: diagnostic.range(),
            name: diagnostic.name,
            body: diagnostic.body,
            suggestion: diagnostic.suggestion,
            fix: diagnostic.fix,
            parent: diagnostic.parent,
            file,
            noqa_offset,
        })
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

    pub const fn as_diagnostic_message(&self) -> Option<&DiagnosticMessage> {
        match self {
            Message::Diagnostic(m) => Some(m),
            Message::SyntaxError(_) => None,
        }
    }

    pub fn into_diagnostic_message(self) -> Option<DiagnosticMessage> {
        match self {
            Message::Diagnostic(m) => Some(m),
            Message::SyntaxError(_) => None,
        }
    }

    /// Returns `true` if `self` is a diagnostic message.
    pub const fn is_diagnostic_message(&self) -> bool {
        matches!(self, Message::Diagnostic(_))
    }

    /// Returns `true` if `self` is a syntax error message.
    pub fn is_syntax_error(&self) -> bool {
        match self {
            Message::Diagnostic(_) => false,
            Message::SyntaxError(diag) => diag.id().is_invalid_syntax(),
        }
    }

    /// Returns a message kind.
    pub fn kind(&self) -> MessageKind {
        match self {
            Message::Diagnostic(m) => MessageKind::Diagnostic(m.rule()),
            Message::SyntaxError(_) => MessageKind::SyntaxError,
        }
    }

    /// Returns the name used to represent the diagnostic.
    pub fn name(&self) -> &str {
        match self {
            Message::Diagnostic(m) => m.name,
            Message::SyntaxError(_) => "SyntaxError",
        }
    }

    /// Returns the message body to display to the user.
    pub fn body(&self) -> &str {
        match self {
            Message::Diagnostic(m) => &m.body,
            Message::SyntaxError(m) => m
                .primary_annotation()
                .expect("Expected a primary annotation for a ruff diagnostic")
                .get_message()
                .expect("Expected a message for a ruff diagnostic"),
        }
    }

    /// Returns the fix suggestion for the violation.
    pub fn suggestion(&self) -> Option<&str> {
        match self {
            Message::Diagnostic(m) => m.suggestion.as_deref(),
            Message::SyntaxError(_) => None,
        }
    }

    /// Returns the offset at which the `noqa` comment will be placed if it's a diagnostic message.
    pub fn noqa_offset(&self) -> Option<TextSize> {
        match self {
            Message::Diagnostic(m) => Some(m.noqa_offset),
            Message::SyntaxError(_) => None,
        }
    }

    /// Returns the [`Fix`] for the message, if there is any.
    pub fn fix(&self) -> Option<&Fix> {
        match self {
            Message::Diagnostic(m) => m.fix.as_ref(),
            Message::SyntaxError(_) => None,
        }
    }

    /// Returns `true` if the message contains a [`Fix`].
    pub fn fixable(&self) -> bool {
        self.fix().is_some()
    }

    /// Returns the [`Rule`] corresponding to the diagnostic message.
    pub fn rule(&self) -> Option<Rule> {
        match self {
            Message::Diagnostic(m) => Some(m.rule()),
            Message::SyntaxError(_) => None,
        }
    }

    /// Returns the filename for the message.
    pub fn filename(&self) -> Cow<'_, str> {
        match self {
            Message::Diagnostic(m) => Cow::Borrowed(m.file.name()),
            Message::SyntaxError(diag) => Cow::Owned(
                diag.expect_primary_span()
                    .expect_ruff_file()
                    .name()
                    .to_string(),
            ),
        }
    }

    /// Computes the start source location for the message.
    pub fn compute_start_location(&self) -> LineColumn {
        match self {
            Message::Diagnostic(m) => m.file.to_source_code().line_column(m.range.start()),
            Message::SyntaxError(diag) => diag
                .expect_primary_span()
                .expect_ruff_file()
                .to_source_code()
                .line_column(self.start()),
        }
    }

    /// Computes the end source location for the message.
    pub fn compute_end_location(&self) -> LineColumn {
        match self {
            Message::Diagnostic(m) => m.file.to_source_code().line_column(m.range.end()),
            Message::SyntaxError(diag) => diag
                .expect_primary_span()
                .expect_ruff_file()
                .to_source_code()
                .line_column(self.end()),
        }
    }

    /// Returns the [`SourceFile`] which the message belongs to.
    pub fn source_file(&self) -> SourceFile {
        match self {
            Message::Diagnostic(m) => m.file.clone(),
            Message::SyntaxError(m) => m.expect_primary_span().expect_ruff_file().clone(),
        }
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
        match self {
            Message::Diagnostic(m) => m.range,
            Message::SyntaxError(m) => m
                .expect_primary_span()
                .range()
                .expect("Expected range for ruff span"),
        }
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
    use crate::message::{DiagnosticMessage, Emitter, EmitterContext, Message};

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
        let unused_import = DiagnosticMessage {
            name: "unused-import",
            body: "`os` imported but unused".to_string(),
            suggestion: Some("Remove unused import: `os`".to_string()),
            range: TextRange::new(unused_import_start, TextSize::from(9)),
            fix: Some(Fix::unsafe_edit(Edit::range_deletion(TextRange::new(
                TextSize::from(0),
                TextSize::from(10),
            )))),
            parent: None,
            noqa_offset: unused_import_start,
            file: fib_source.clone(),
        };

        let unused_variable_start = TextSize::from(94);
        let unused_variable = DiagnosticMessage {
            name: "unused-variable",
            body: "Local variable `x` is assigned to but never used".to_string(),
            suggestion: Some("Remove assignment to unused variable `x`".to_string()),
            range: TextRange::new(unused_variable_start, TextSize::from(95)),
            fix: Some(Fix::unsafe_edit(Edit::deletion(
                TextSize::from(94),
                TextSize::from(99),
            ))),
            parent: None,
            noqa_offset: unused_variable_start,
            file: fib_source,
        };

        let file_2 = r"if a == 1: pass";

        let undefined_name_start = TextSize::from(3);
        let undefined_name = DiagnosticMessage {
            name: "undefined-name",
            body: "Undefined name `a`".to_string(),
            suggestion: None,
            range: TextRange::new(undefined_name_start, TextSize::from(4)),
            fix: None,
            parent: None,
            noqa_offset: undefined_name_start,
            file: SourceFileBuilder::new("undef.py", file_2).finish(),
        };

        vec![
            Message::Diagnostic(unused_import),
            Message::Diagnostic(unused_variable),
            Message::Diagnostic(undefined_name),
        ]
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
        let unused_import_os = DiagnosticMessage {
            name: "unused-import",
            body: "`os` imported but unused".to_string(),
            suggestion: Some("Remove unused import: `os`".to_string()),
            range: TextRange::new(unused_import_os_start, TextSize::from(18)),
            fix: Some(Fix::safe_edit(Edit::range_deletion(TextRange::new(
                TextSize::from(9),
                TextSize::from(19),
            )))),
            parent: None,
            file: notebook_source.clone(),
            noqa_offset: unused_import_os_start,
        };

        let unused_import_math_start = TextSize::from(35);
        let unused_import_math = DiagnosticMessage {
            name: "unused-import",
            body: "`math` imported but unused".to_string(),
            suggestion: Some("Remove unused import: `math`".to_string()),
            range: TextRange::new(unused_import_math_start, TextSize::from(39)),
            fix: Some(Fix::safe_edit(Edit::range_deletion(TextRange::new(
                TextSize::from(28),
                TextSize::from(40),
            )))),
            parent: None,
            file: notebook_source.clone(),
            noqa_offset: unused_import_math_start,
        };

        let unused_variable_start = TextSize::from(98);
        let unused_variable = DiagnosticMessage {
            name: "unused-variable",
            body: "Local variable `x` is assigned to but never used".to_string(),
            suggestion: Some("Remove assignment to unused variable `x`".to_string()),
            range: TextRange::new(unused_variable_start, TextSize::from(99)),
            fix: Some(Fix::unsafe_edit(Edit::deletion(
                TextSize::from(94),
                TextSize::from(104),
            ))),
            parent: None,
            file: notebook_source,
            noqa_offset: unused_variable_start,
        };

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
            vec![
                Message::Diagnostic(unused_import_os),
                Message::Diagnostic(unused_import_math),
                Message::Diagnostic(unused_variable),
            ],
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
