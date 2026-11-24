use std::backtrace::BacktraceStatus;
use std::fmt::Display;
use std::io::Write;
use std::path::Path;

use ruff_db::panic::PanicError;
use rustc_hash::FxHashMap;
use strum::IntoEnumIterator;

use ruff_db::diagnostic::render::{
    AzureRenderer, ConciseRenderer, FullRenderer, GithubRenderer, GitlabRenderer, GroupedRenderer,
    JsonLinesRenderer, JsonRenderer, JunitRenderer, PylintRenderer, RdjsonRenderer, Renderer,
    SarifRenderer,
};
use ruff_db::diagnostic::{
    Annotation, Diagnostic, DiagnosticId, DisplayDiagnosticConfig, FileResolver, Input, LintName,
    RuleMetadata, SecondaryCode, Severity, Span, SubDiagnostic, SubDiagnosticSeverity, UnifiedFile,
};
use ruff_db::files::File;

use ruff_notebook::NotebookIndex;
use ruff_source_file::{SourceFile, SourceFileBuilder};
use ruff_text_size::{TextRange, TextSize};

use crate::Fix;
use crate::registry::{Linter, Rule, RuleNamespace, RuleSet};
use crate::rule_selector::PreviewOptions;
use crate::settings::DEFAULT_SELECTORS;
use crate::settings::types::OutputFormat;

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
                    "run with `RUST_BACKTRACE=1` environment variable \
                    to show the full backtrace information",
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
pub(crate) fn create_lint_diagnostic<B, S>(
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

    if let Some(code) = rule.noqa_code() {
        diagnostic.set_secondary_code(SecondaryCode::new(code.to_string()));
    }
    diagnostic.set_documentation_url(Some(rule.url()));

    diagnostic
}

impl FileResolver for RenderContext<'_> {
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

fn rule_metadata() -> FxHashMap<LintName, RuleMetadata> {
    // Describe the defaults before configuration overrides such as preview.
    let default_rules = DEFAULT_SELECTORS
        .iter()
        .flat_map(|selector| selector.rules(&PreviewOptions::default()))
        .collect::<RuleSet>();

    Rule::iter()
        .map(|rule| {
            let name = LintName::of(rule.into());
            let linter = rule.noqa_code().and_then(|code| {
                Linter::parse_code(&code.to_string()).map(|(linter, _)| linter.name())
            });

            (
                name,
                RuleMetadata {
                    name,
                    summary: rule.message_formats()[0],
                    description: rule.explanation().into(),
                    help_url: rule.url(),
                    linter,
                    default_severity: Some(Severity::Error),
                    enabled_by_default: default_rules.contains(rule),
                },
            )
        })
        .collect()
}

/// Context used while rendering diagnostics.
pub struct RenderContext<'a> {
    notebook_indexes: &'a FxHashMap<String, NotebookIndex>,
}

impl<'a> RenderContext<'a> {
    pub fn new(notebook_indexes: &'a FxHashMap<String, NotebookIndex>) -> Self {
        Self { notebook_indexes }
    }
}

pub fn render_diagnostics(
    writer: &mut dyn Write,
    format: OutputFormat,
    config: &DisplayDiagnosticConfig,
    context: &RenderContext<'_>,
    diagnostics: &[Diagnostic],
) -> std::io::Result<()> {
    match format {
        OutputFormat::Concise => ConciseRenderer::new(context, config).write(writer, diagnostics),
        OutputFormat::Full => FullRenderer::new(context, config).write(writer, diagnostics),
        OutputFormat::Json => JsonRenderer::new(context, config).write(writer, diagnostics),
        OutputFormat::JsonLines => {
            JsonLinesRenderer::new(context, config).write(writer, diagnostics)
        }
        OutputFormat::Junit => JunitRenderer::new(context, config).write(writer, diagnostics),
        OutputFormat::Github => GithubRenderer::new(context, config).write(writer, diagnostics),
        OutputFormat::Gitlab => GitlabRenderer::new(context, config).write(writer, diagnostics),
        OutputFormat::Pylint => PylintRenderer::new(context, config).write(writer, diagnostics),
        OutputFormat::Rdjson => RdjsonRenderer::new(context, config).write(writer, diagnostics),
        OutputFormat::Azure => AzureRenderer::new(context, config).write(writer, diagnostics),
        OutputFormat::Grouped => GroupedRenderer::new(context, config).write(writer, diagnostics),
        OutputFormat::Sarif => SarifRenderer::new(context, config, rule_metadata(), crate::VERSION)
            .write(writer, diagnostics),
    }
}
