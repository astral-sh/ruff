use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::io::Write;
use std::ops::Deref;

use rustc_hash::FxHashMap;

pub use azure::AzureEmitter;
pub use github::GithubEmitter;
pub use gitlab::GitlabEmitter;
pub use grouped::GroupedEmitter;
pub use json::JsonEmitter;
pub use json_lines::JsonLinesEmitter;
pub use junit::JunitEmitter;
pub use pylint::PylintEmitter;
use ruff_diagnostics::{Diagnostic, DiagnosticKind, Fix};
use ruff_notebook::NotebookIndex;
use ruff_source_file::{SourceFile, SourceLocation};
use ruff_text_size::{Ranged, TextRange, TextSize};
pub use text::TextEmitter;

mod azure;
mod diff;
mod github;
mod gitlab;
mod grouped;
mod json;
mod json_lines;
mod junit;
mod pylint;
mod text;

#[derive(Debug, PartialEq, Eq)]
pub struct Message {
    pub kind: DiagnosticKind,
    pub range: TextRange,
    pub fix: Option<Fix>,
    pub file: SourceFile,
    pub noqa_offset: TextSize,
}

impl Message {
    pub fn from_diagnostic(
        diagnostic: Diagnostic,
        file: SourceFile,
        noqa_offset: TextSize,
    ) -> Self {
        Self {
            range: diagnostic.range(),
            kind: diagnostic.kind,
            fix: diagnostic.fix,
            file,
            noqa_offset,
        }
    }

    pub fn filename(&self) -> &str {
        self.file.name()
    }

    pub fn compute_start_location(&self) -> SourceLocation {
        self.file.to_source_code().source_location(self.start())
    }

    pub fn compute_end_location(&self) -> SourceLocation {
        self.file.to_source_code().source_location(self.end())
    }
}

impl Ord for Message {
    fn cmp(&self, other: &Self) -> Ordering {
        (&self.file, self.start()).cmp(&(&other.file, other.start()))
    }
}

impl PartialOrd for Message {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ranged for Message {
    fn range(&self) -> TextRange {
        self.range
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
    use ruff_source_file::SourceFileBuilder;
    use ruff_text_size::{Ranged, TextRange, TextSize};

    use crate::message::{Emitter, EmitterContext, Message};

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
        .with_fix(Fix::suggested(Edit::range_deletion(TextRange::new(
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
        .with_fix(Fix::suggested(Edit::deletion(
            TextSize::from(94),
            TextSize::from(99),
        )));

        let file_2 = r#"if a == 1: pass"#;

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
}
