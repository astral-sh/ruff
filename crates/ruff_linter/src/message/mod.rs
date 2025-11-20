use std::backtrace::BacktraceStatus;
use std::fmt::Display;
use std::io::Write;
use std::path::Path;

use ruff_db::panic::PanicError;
use rustc_hash::FxHashMap;

use ruff_db::diagnostic::{
    Annotation, Diagnostic, DiagnosticFormat, DiagnosticId, DisplayDiagnosticConfig,
    DisplayDiagnostics, DisplayGithubDiagnostics, FileResolver, GithubRenderer, Input, LintName,
    SecondaryCode, Severity, Span, SubDiagnostic, SubDiagnosticSeverity, UnifiedFile,
};
use ruff_db::files::File;

pub use grouped::GroupedEmitter;
use ruff_notebook::NotebookIndex;
use ruff_source_file::{SourceFile, SourceFileBuilder};
use ruff_text_size::{TextRange, TextSize};
pub use sarif::SarifEmitter;

use crate::Fix;
use crate::registry::Rule;
use crate::settings::types::{OutputFormat, RuffOutputFormat};

mod grouped;
mod sarif;

/// Create a `Diagnostic` from a panic.
pub fn create_panic_diagnostic(error: &PanicError, path: Option<&Path>) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        DiagnosticId::Panic,
        Severity::Fatal,
        error.to_diagnostic_message(path.as_ref().map(|path| path.display())),
    );

    diagnostic.sub(SubDiagnostic::new(
        SubDiagnosticSeverity::Info,
        "This indicates a bug in Ruff.",
    ));
    let report_message = "If you could open an issue at \
                            https://github.com/astral-sh/ruff/issues/new?title=%5Bpanic%5D, \
                            we'd be very appreciative!";
    diagnostic.sub(SubDiagnostic::new(
        SubDiagnosticSeverity::Info,
        report_message,
    ));

    if let Some(backtrace) = &error.backtrace {
        match backtrace.status() {
            BacktraceStatus::Disabled => {
                diagnostic.sub(SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            "run with `RUST_BACKTRACE=1` environment variable to show the full backtrace information",
                        ));
            }
            BacktraceStatus::Captured => {
                diagnostic.sub(SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    format!("Backtrace:\n{backtrace}"),
                ));
            }
            _ => {}
        }
    }

    if let Some(path) = path {
        let file = SourceFileBuilder::new(path.to_string_lossy(), "").finish();
        let span = Span::from(file);
        let mut annotation = Annotation::primary(span);
        annotation.hide_snippet(true);
        diagnostic.annotate(annotation);
    }

    diagnostic
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
        annotation.hide_snippet(true);
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
    diagnostic.set_documentation_url(rule.url());

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

pub fn render_diagnostics(
    writer: &mut dyn Write,
    format: OutputFormat,
    config: DisplayDiagnosticConfig,
    context: &EmitterContext<'_>,
    diagnostics: &[Diagnostic],
) -> std::io::Result<()> {
    match DiagnosticFormat::try_from(format) {
        Ok(format) => {
            let config = config.format(format);
            let value = DisplayDiagnostics::new(context, &config, diagnostics);
            write!(writer, "{value}")?;
        }
        Err(RuffOutputFormat::Github) => {
            let renderer = GithubRenderer::new(context, "Ruff");
            let value = DisplayGithubDiagnostics::new(&renderer, diagnostics);
            write!(writer, "{value}")?;
        }
        Err(RuffOutputFormat::Grouped) => {
            GroupedEmitter::default()
                .with_show_fix_status(config.show_fix_status())
                .with_applicability(config.fix_applicability())
                .emit(writer, diagnostics, context)
                .map_err(std::io::Error::other)?;
        }
        Err(RuffOutputFormat::Sarif) => {
            SarifEmitter
                .emit(writer, diagnostics, context)
                .map_err(std::io::Error::other)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use rustc_hash::FxHashMap;

    use ruff_db::diagnostic::Diagnostic;
    use ruff_python_parser::{Mode, ParseOptions, parse_unchecked};
    use ruff_source_file::SourceFileBuilder;
    use ruff_text_size::{TextRange, TextSize};

    use crate::codes::Rule;
    use crate::message::{Emitter, EmitterContext, create_lint_diagnostic};
    use crate::{Edit, Fix};

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
                Diagnostic::invalid_syntax(source_file.clone(), &parse_error.error, parse_error)
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
}
