use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::io::Write;
use std::ops::Deref;

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
use ruff_diagnostics::{Diagnostic, DiagnosticKind, Fix};
use ruff_notebook::NotebookIndex;
use ruff_python_parser::{ParseError, UnsupportedSyntaxError};
use ruff_source_file::{SourceFile, SourceLocation};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
pub use sarif::SarifEmitter;
pub use text::TextEmitter;

use crate::logging::DisplayParseErrorType;
use crate::registry::{AsRule, Rule};
use crate::Locator;

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
    SyntaxError(SyntaxErrorMessage),
}

/// A diagnostic message corresponding to a rule violation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticMessage {
    pub kind: DiagnosticKind,
    pub range: TextRange,
    pub fix: Option<Fix>,
    pub parent: Option<TextSize>,
    pub file: SourceFile,
    pub noqa_offset: TextSize,
}

/// A syntax error message raised by the parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxErrorMessage {
    pub message: String,
    pub range: TextRange,
    pub file: SourceFile,
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
    /// Create a [`Message`] from the given [`Diagnostic`] corresponding to a rule violation.
    pub fn from_diagnostic(
        diagnostic: Diagnostic,
        file: SourceFile,
        noqa_offset: TextSize,
    ) -> Message {
        Message::Diagnostic(DiagnosticMessage {
            range: diagnostic.range(),
            kind: diagnostic.kind,
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

        Message::SyntaxError(SyntaxErrorMessage {
            message: format!(
                "SyntaxError: {}",
                DisplayParseErrorType::new(&parse_error.error)
            ),
            range: TextRange::at(parse_error.location.start(), len),
            file,
        })
    }

    /// Create a [`Message`] from the given [`UnsupportedSyntaxError`].
    pub fn from_unsupported_syntax_error(
        unsupported_syntax_error: &UnsupportedSyntaxError,
        file: SourceFile,
    ) -> Message {
        Message::SyntaxError(SyntaxErrorMessage {
            message: format!("SyntaxError: {unsupported_syntax_error}"),
            range: unsupported_syntax_error.range,
            file,
        })
    }

    /// Create a [`Message`] from the given [`SemanticSyntaxError`].
    pub fn from_semantic_syntax_error(
        semantic_syntax_error: &SemanticSyntaxError,
        file: SourceFile,
    ) -> Message {
        Message::SyntaxError(SyntaxErrorMessage {
            message: format!("SyntaxError: {semantic_syntax_error}"),
            range: semantic_syntax_error.range,
            file,
        })
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
    pub const fn is_syntax_error(&self) -> bool {
        matches!(self, Message::SyntaxError(_))
    }

    /// Returns a message kind.
    pub fn kind(&self) -> MessageKind {
        match self {
            Message::Diagnostic(m) => MessageKind::Diagnostic(m.kind.rule()),
            Message::SyntaxError(_) => MessageKind::SyntaxError,
        }
    }

    /// Returns the name used to represent the diagnostic.
    pub fn name(&self) -> &str {
        match self {
            Message::Diagnostic(m) => &m.kind.name,
            Message::SyntaxError(_) => "SyntaxError",
        }
    }

    /// Returns the message body to display to the user.
    pub fn body(&self) -> &str {
        match self {
            Message::Diagnostic(m) => &m.kind.body,
            Message::SyntaxError(m) => &m.message,
        }
    }

    /// Returns the fix suggestion for the violation.
    pub fn suggestion(&self) -> Option<&str> {
        match self {
            Message::Diagnostic(m) => m.kind.suggestion.as_deref(),
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
            Message::Diagnostic(m) => Some(m.kind.rule()),
            Message::SyntaxError(_) => None,
        }
    }

    /// Returns the filename for the message.
    pub fn filename(&self) -> &str {
        self.source_file().name()
    }

    /// Computes the start source location for the message.
    pub fn compute_start_location(&self) -> SourceLocation {
        self.source_file()
            .to_source_code()
            .source_location(self.start())
    }

    /// Computes the end source location for the message.
    pub fn compute_end_location(&self) -> SourceLocation {
        self.source_file()
            .to_source_code()
            .source_location(self.end())
    }

    /// Returns the [`SourceFile`] which the message belongs to.
    pub fn source_file(&self) -> &SourceFile {
        match self {
            Message::Diagnostic(m) => &m.file,
            Message::SyntaxError(m) => &m.file,
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
            Message::SyntaxError(m) => m.range,
        }
    }
}

struct MessageWithLocation<'a> {
    message: &'a Message,
    start_location: SourceLocation,
}

impl Deref for MessageWithLocation<'_> {
    type Target = Message;

    fn deref(&self) -> &Self::Target {
        self.message
    }
}

fn group_messages_by_filename(messages: &[Message]) -> BTreeMap<&str, Vec<MessageWithLocation>> {
    let mut grouped_messages = BTreeMap::default();
    for message in messages {
        grouped_messages
            .entry(message.filename())
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

    use ruff_diagnostics::{Diagnostic, DiagnosticKind, Edit, Fix};
    use ruff_notebook::NotebookIndex;
    use ruff_python_parser::{parse_unchecked, Mode, ParseOptions};
    use ruff_source_file::{OneIndexed, SourceFileBuilder};
    use ruff_text_size::{Ranged, TextRange, TextSize};

    use crate::message::{Emitter, EmitterContext, Message};
    use crate::Locator;

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

        let unused_import = Diagnostic::new(
            DiagnosticKind {
                name: "UnusedImport".to_string(),
                body: "`os` imported but unused".to_string(),
                suggestion: Some("Remove unused import: `os`".to_string()),
            },
            TextRange::new(TextSize::from(7), TextSize::from(9)),
        )
        .with_fix(Fix::unsafe_edit(Edit::range_deletion(TextRange::new(
            TextSize::from(0),
            TextSize::from(10),
        ))));

        let fib_source = SourceFileBuilder::new("fib.py", fib).finish();

        let unused_variable = Diagnostic::new(
            DiagnosticKind {
                name: "UnusedVariable".to_string(),
                body: "Local variable `x` is assigned to but never used".to_string(),
                suggestion: Some("Remove assignment to unused variable `x`".to_string()),
            },
            TextRange::new(TextSize::from(94), TextSize::from(95)),
        )
        .with_fix(Fix::unsafe_edit(Edit::deletion(
            TextSize::from(94),
            TextSize::from(99),
        )));

        let file_2 = r"if a == 1: pass";

        let undefined_name = Diagnostic::new(
            DiagnosticKind {
                name: "UndefinedName".to_string(),
                body: "Undefined name `a`".to_string(),
                suggestion: None,
            },
            TextRange::new(TextSize::from(3), TextSize::from(4)),
        );

        let file_2_source = SourceFileBuilder::new("undef.py", file_2).finish();

        let unused_import_start = unused_import.start();
        let unused_variable_start = unused_variable.start();
        let undefined_name_start = undefined_name.start();
        vec![
            Message::from_diagnostic(unused_import, fib_source.clone(), unused_import_start),
            Message::from_diagnostic(unused_variable, fib_source, unused_variable_start),
            Message::from_diagnostic(undefined_name, file_2_source, undefined_name_start),
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

        let unused_import_os = Diagnostic::new(
            DiagnosticKind {
                name: "UnusedImport".to_string(),
                body: "`os` imported but unused".to_string(),
                suggestion: Some("Remove unused import: `os`".to_string()),
            },
            TextRange::new(TextSize::from(16), TextSize::from(18)),
        )
        .with_fix(Fix::safe_edit(Edit::range_deletion(TextRange::new(
            TextSize::from(9),
            TextSize::from(19),
        ))));

        let unused_import_math = Diagnostic::new(
            DiagnosticKind {
                name: "UnusedImport".to_string(),
                body: "`math` imported but unused".to_string(),
                suggestion: Some("Remove unused import: `math`".to_string()),
            },
            TextRange::new(TextSize::from(35), TextSize::from(39)),
        )
        .with_fix(Fix::safe_edit(Edit::range_deletion(TextRange::new(
            TextSize::from(28),
            TextSize::from(40),
        ))));

        let unused_variable = Diagnostic::new(
            DiagnosticKind {
                name: "UnusedVariable".to_string(),
                body: "Local variable `x` is assigned to but never used".to_string(),
                suggestion: Some("Remove assignment to unused variable `x`".to_string()),
            },
            TextRange::new(TextSize::from(98), TextSize::from(99)),
        )
        .with_fix(Fix::unsafe_edit(Edit::deletion(
            TextSize::from(94),
            TextSize::from(104),
        )));

        let notebook_source = SourceFileBuilder::new("notebook.ipynb", notebook).finish();

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

        let unused_import_os_start = unused_import_os.start();
        let unused_import_math_start = unused_import_math.start();
        let unused_variable_start = unused_variable.start();

        (
            vec![
                Message::from_diagnostic(
                    unused_import_os,
                    notebook_source.clone(),
                    unused_import_os_start,
                ),
                Message::from_diagnostic(
                    unused_import_math,
                    notebook_source.clone(),
                    unused_import_math_start,
                ),
                Message::from_diagnostic(unused_variable, notebook_source, unused_variable_start),
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
