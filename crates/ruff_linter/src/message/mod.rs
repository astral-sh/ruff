use std::collections::BTreeMap;
use std::fmt::Display;
use std::io::Write;
use std::ops::Deref;

use rustc_hash::FxHashMap;

use ruff_db::diagnostic::{
    Annotation, Diagnostic, DiagnosticId, FileResolver, Input, LintName, SecondaryCode, Severity,
    Span, UnifiedFile,
};
use ruff_db::files::File;

pub use github::GithubEmitter;
pub use gitlab::GitlabEmitter;
pub use grouped::GroupedEmitter;
use ruff_notebook::NotebookIndex;
use ruff_source_file::{LineColumn, SourceFile};
use ruff_text_size::{Ranged, TextRange, TextSize};
pub use sarif::SarifEmitter;
pub use text::TextEmitter;

use crate::Fix;
use crate::registry::Rule;

mod diff;
mod github;
mod gitlab;
mod grouped;
mod sarif;
mod text;

/// Creates a `Diagnostic` from a syntax error, with the format expected by Ruff.
///
/// This is almost identical to `ruff_db::diagnostic::create_syntax_error_diagnostic`, except the
/// `message` is stored as the primary diagnostic message instead of on the primary annotation.
///
/// TODO(brent) These should be unified at some point, but we keep them separate for now to avoid a
/// ton of snapshot changes while combining ruff's diagnostic type with `Diagnostic`.
pub fn create_syntax_error_diagnostic(
    span: impl Into<Span>,
    message: impl std::fmt::Display,
    range: impl Ranged,
) -> Diagnostic {
    let mut diag = Diagnostic::new(DiagnosticId::InvalidSyntax, Severity::Error, message);
    let span = span.into().with_range(range.range());
    diag.annotate(Annotation::primary(span));
    diag
}

#[expect(clippy::too_many_arguments)]
pub fn create_lint_diagnostic<B, S>(
    body: B,
    suggestion: Option<S>,
    range: TextRange,
    fix: Option<Fix>,
    parent: Option<TextSize>,
    file: SourceFile,
    noqa_offset: Option<TextSize>,
    rule: Rule,
) -> Diagnostic
where
    B: Display,
    S: Display,
{
    let mut diagnostic = Diagnostic::new(
        DiagnosticId::Lint(LintName::of(rule.into())),
        Severity::Error,
        body,
    );

    let span = Span::from(file).with_range(range);
    let mut annotation = Annotation::primary(span);
    // The `0..0` range is used to highlight file-level diagnostics.
    //
    // TODO(brent) We should instead set this flag on annotations for individual lint rules that
    // actually need it, but we need to be able to cache the new diagnostic model first. See
    // https://github.com/astral-sh/ruff/issues/19688.
    if range == TextRange::default() {
        annotation.set_file_level(true);
    }
    diagnostic.annotate(annotation);

    if let Some(suggestion) = suggestion {
        diagnostic.help(suggestion);
    }

    if let Some(fix) = fix {
        diagnostic.set_fix(fix);
    }

    if let Some(parent) = parent {
        diagnostic.set_parent(parent);
    }

    if let Some(noqa_offset) = noqa_offset {
        diagnostic.set_noqa_offset(noqa_offset);
    }

    diagnostic.set_secondary_code(SecondaryCode::new(rule.noqa_code().to_string()));

    diagnostic
}

impl FileResolver for EmitterContext<'_> {
    fn path(&self, _file: File) -> &str {
        unimplemented!("Expected a Ruff file for rendering a Ruff diagnostic");
    }

    fn input(&self, _file: File) -> Input {
        unimplemented!("Expected a Ruff file for rendering a Ruff diagnostic");
    }

    fn notebook_index(&self, file: &UnifiedFile) -> Option<NotebookIndex> {
        match file {
            UnifiedFile::Ty(_) => {
                unimplemented!("Expected a Ruff file for rendering a Ruff diagnostic")
            }
            UnifiedFile::Ruff(file) => self.notebook_indexes.get(file.name()).cloned(),
        }
    }

    fn is_notebook(&self, file: &UnifiedFile) -> bool {
        match file {
            UnifiedFile::Ty(_) => {
                unimplemented!("Expected a Ruff file for rendering a Ruff diagnostic")
            }
            UnifiedFile::Ruff(file) => self.notebook_indexes.get(file.name()).is_some(),
        }
    }

    fn current_directory(&self) -> &std::path::Path {
        crate::fs::get_cwd()
    }
}

struct MessageWithLocation<'a> {
    message: &'a Diagnostic,
    start_location: LineColumn,
}

impl Deref for MessageWithLocation<'_> {
    type Target = Diagnostic;

    fn deref(&self) -> &Self::Target {
        self.message
    }
}

fn group_diagnostics_by_filename(
    diagnostics: &[Diagnostic],
) -> BTreeMap<String, Vec<MessageWithLocation<'_>>> {
    let mut grouped_messages = BTreeMap::default();
    for diagnostic in diagnostics {
        grouped_messages
            .entry(diagnostic.expect_ruff_filename())
            .or_insert_with(Vec::new)
            .push(MessageWithLocation {
                message: diagnostic,
                start_location: diagnostic.expect_ruff_start_location(),
            });
    }
    grouped_messages
}

/// Display format for [`Diagnostic`]s.
///
/// The emitter serializes a slice of [`Diagnostic`]s and writes them to a [`Write`].
pub trait Emitter {
    /// Serializes the `diagnostics` and writes the output to `writer`.
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[Diagnostic],
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

    use ruff_db::diagnostic::Diagnostic;
    use ruff_notebook::NotebookIndex;
    use ruff_python_parser::{Mode, ParseOptions, parse_unchecked};
    use ruff_source_file::{OneIndexed, SourceFileBuilder};
    use ruff_text_size::{TextRange, TextSize};

    use crate::codes::Rule;
    use crate::message::{Emitter, EmitterContext, create_lint_diagnostic};
    use crate::{Edit, Fix};

    use super::create_syntax_error_diagnostic;

    pub(super) fn create_syntax_error_diagnostics() -> Vec<Diagnostic> {
        let source = r"from os import

if call(foo
    def bar():
        pass
";
        let source_file = SourceFileBuilder::new("syntax_errors.py", source).finish();
        parse_unchecked(source, ParseOptions::from(Mode::Module))
            .errors()
            .iter()
            .map(|parse_error| {
                create_syntax_error_diagnostic(source_file.clone(), &parse_error.error, parse_error)
            })
            .collect()
    }

    pub(super) fn create_diagnostics() -> Vec<Diagnostic> {
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
        let unused_import = create_lint_diagnostic(
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
        let unused_variable = create_lint_diagnostic(
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
        let undefined_name = create_lint_diagnostic(
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
    -> (Vec<Diagnostic>, FxHashMap<String, NotebookIndex>) {
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
        let unused_import_os = create_lint_diagnostic(
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
        let unused_import_math = create_lint_diagnostic(
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
        let unused_variable = create_lint_diagnostic(
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
        diagnostics: &[Diagnostic],
    ) -> String {
        let notebook_indexes = FxHashMap::default();
        let context = EmitterContext::new(&notebook_indexes);
        let mut output: Vec<u8> = Vec::new();
        emitter.emit(&mut output, diagnostics, &context).unwrap();

        String::from_utf8(output).expect("Output to be valid UTF-8")
    }

    pub(super) fn capture_emitter_notebook_output(
        emitter: &mut dyn Emitter,
        diagnostics: &[Diagnostic],
        notebook_indexes: &FxHashMap<String, NotebookIndex>,
    ) -> String {
        let context = EmitterContext::new(notebook_indexes);
        let mut output: Vec<u8> = Vec::new();
        emitter.emit(&mut output, diagnostics, &context).unwrap();

        String::from_utf8(output).expect("Output to be valid UTF-8")
    }
}
