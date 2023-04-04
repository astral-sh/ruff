mod azure;
mod github;
mod gitlab;
mod grouped;
mod json;
mod junit;
mod pylint;
mod text;

use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::io::Write;

pub use azure::AzureEmitter;
pub use github::GithubEmitter;
pub use gitlab::GitlabEmitter;
pub use grouped::GroupedEmitter;
pub use json::JsonEmitter;
pub use junit::JunitEmitter;
pub use pylint::PylintEmitter;
pub use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};
pub use text::TextEmitter;

use crate::jupyter::JupyterIndex;
use ruff_diagnostics::{Diagnostic, DiagnosticKind, Fix};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub kind: DiagnosticKind,
    pub location: Location,
    pub end_location: Location,
    pub fix: Fix,
    pub filename: String,
    pub source: Option<Source>,
    pub noqa_row: usize,
}

impl Message {
    pub fn from_diagnostic(
        diagnostic: Diagnostic,
        filename: String,
        source: Option<Source>,
        noqa_row: usize,
    ) -> Self {
        Self {
            kind: diagnostic.kind,
            location: Location::new(diagnostic.location.row(), diagnostic.location.column() + 1),
            end_location: Location::new(
                diagnostic.end_location.row(),
                diagnostic.end_location.column() + 1,
            ),
            fix: diagnostic.fix,
            filename,
            source,
            noqa_row,
        }
    }
}

impl Ord for Message {
    fn cmp(&self, other: &Self) -> Ordering {
        (&self.filename, self.location.row(), self.location.column()).cmp(&(
            &other.filename,
            other.location.row(),
            other.location.column(),
        ))
    }
}

impl PartialOrd for Message {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct Source {
    pub contents: String,
    pub range: (usize, usize),
}

impl Source {
    pub fn from_diagnostic(diagnostic: &Diagnostic, locator: &Locator) -> Self {
        let location = Location::new(diagnostic.location.row(), 0);
        // Diagnostics can already extend one-past-the-end. If they do, though, then
        // they'll end at the start of a line. We need to avoid extending by yet another
        // line past-the-end.
        let end_location = if diagnostic.end_location.column() == 0 {
            diagnostic.end_location
        } else {
            Location::new(diagnostic.end_location.row() + 1, 0)
        };
        let source = locator.slice(Range::new(location, end_location));
        let num_chars_in_range = locator
            .slice(Range::new(diagnostic.location, diagnostic.end_location))
            .chars()
            .count();
        Source {
            contents: source.to_string(),
            range: (
                diagnostic.location.column(),
                diagnostic.location.column() + num_chars_in_range,
            ),
        }
    }
}

fn group_messages_by_filename(messages: &[Message]) -> BTreeMap<&str, Vec<&Message>> {
    let mut grouped_messages = BTreeMap::default();
    for message in messages {
        grouped_messages
            .entry(message.filename.as_str())
            .or_insert_with(Vec::new)
            .push(message);
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
    jupyter_indices: &'a FxHashMap<String, JupyterIndex>,
}

impl<'a> EmitterContext<'a> {
    pub fn new(jupyter_indices: &'a FxHashMap<String, JupyterIndex>) -> Self {
        Self { jupyter_indices }
    }

    /// Tests if the file with `name` is a jupyter notebook.
    pub fn is_jupyter_notebook(&self, name: &str) -> bool {
        self.jupyter_indices.contains_key(name)
    }

    /// Returns the file's [`JupyterIndex`] if the file `name` is a jupyter notebook.
    pub fn jupyter_index(&self, name: &str) -> Option<&JupyterIndex> {
        self.jupyter_indices.get(name)
    }
}

#[cfg(test)]
mod tests {
    use crate::message::{Emitter, EmitterContext, Location, Message, Source};
    use crate::rules::pyflakes::rules::{UndefinedName, UnusedImport, UnusedVariable};
    use ruff_diagnostics::{Diagnostic, Edit, Fix};
    use ruff_python_ast::source_code::Locator;
    use ruff_python_ast::types::Range;
    use rustc_hash::FxHashMap;

    pub(super) fn create_messages() -> Vec<Message> {
        let file_1 = r#"import os

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

        let file_1_locator = Locator::new(file_1);

        let unused_import = Diagnostic::new(
            UnusedImport {
                name: "os".to_string(),
                context: None,
                multiple: false,
            },
            Range::new(Location::new(1, 7), Location::new(1, 9)),
        );

        let unused_source = Source::from_diagnostic(&unused_import, &file_1_locator);

        let unused_variable = Diagnostic::new(
            UnusedVariable {
                name: "x".to_string(),
            },
            Range::new(Location::new(5, 4), Location::new(5, 5)),
        )
        .with_fix(Fix::new(vec![Edit::deletion(
            Location::new(5, 4),
            Location::new(5, 9),
        )]));

        let unused_variable_source = Source::from_diagnostic(&unused_variable, &file_1_locator);

        let file_2 = r#"if a == 1: pass"#;

        let undefined_name = Diagnostic::new(
            UndefinedName {
                name: "a".to_string(),
            },
            Range::new(Location::new(1, 3), Location::new(1, 4)),
        );

        let undefined_source = Source::from_diagnostic(&undefined_name, &Locator::new(file_2));

        vec![
            Message::from_diagnostic(unused_import, "fib.py".to_string(), Some(unused_source), 1),
            Message::from_diagnostic(
                unused_variable,
                "fib.py".to_string(),
                Some(unused_variable_source),
                1,
            ),
            Message::from_diagnostic(
                undefined_name,
                "undef.py".to_string(),
                Some(undefined_source),
                1,
            ),
        ]
    }

    pub(super) fn capture_emitter_output(
        emitter: &mut dyn Emitter,
        messages: &[Message],
    ) -> String {
        let indices = FxHashMap::default();
        let context = EmitterContext::new(&indices);
        let mut output: Vec<u8> = Vec::new();
        emitter.emit(&mut output, messages, &context).unwrap();

        String::from_utf8(output).expect("Output to be valid UTF-8")
    }
}
