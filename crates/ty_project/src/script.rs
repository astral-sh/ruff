use pep440_rs::VersionSpecifiers;
use ruff_db::Db as SourceDb;
use ruff_db::diagnostic::{
    Annotation, Diagnostic, DiagnosticId, Severity, Span, SubDiagnostic, SubDiagnosticSeverity,
};
use ruff_db::files::File;
use ruff_db::source::{is_notebook, source_text};
use ruff_python_ast::script::ScriptTag;
use ruff_ranged_value::{RangedValue, ValueSource, ValueSourceGuard};
use ruff_text_size::{Ranged, TextRange, TextSize};
use serde::Deserialize;
use ty_combine::Combine;
use ty_python_core::program::{FallibleStrategy, Program, ProgramSettings, UseDefaultStrategy};
use ty_python_semantic::PythonVersionWithSource;
use ty_python_semantic::dependency::DependencyMetadata;

use crate::metadata::options::{
    AnalysisOptionsContext, EnvironmentOptions, Options, OptionsContext,
};
use crate::metadata::pyproject::Tool;
use crate::metadata::value::RelativePathBuf;
use crate::uv::{DependencyMetadataError, UvMetadata, script_environment};
use crate::{Db, ProjectMetadata};

/// A standalone PEP 723 script and its resolved settings.
#[salsa::tracked(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct Script<'db> {
    #[returns(copy)]
    pub(crate) file: File,

    #[tracked]
    #[returns(copy)]
    pub(crate) program: Program<'db>,

    #[tracked]
    #[returns(ref)]
    pub(crate) python_version_with_source: PythonVersionWithSource,

    /// Whether the script's metadata, settings, and Python environment resolved without errors.
    ///
    /// For a script with invalid settings, `Program` is a best effort approximation
    /// of the script's configuration. It's, therefore, important that ty doesn't run any destructive
    /// operations or shows misleading diagnostics. That means, `--fix` should be a no-op and
    /// `check_file` (and similar operations) should bail and only show the setting related diagnostics.
    #[tracked]
    #[returns(copy)]
    pub(crate) has_valid_settings: bool,

    /// Diagnostics generated while parsing the script metadata and resolving its settings.
    #[tracked]
    #[returns(deref)]
    pub(crate) settings_diagnostics: Box<[Diagnostic]>,
}

#[salsa::tracked]
impl<'db> Script<'db> {
    /// Returns the script for `file` without creating a second Salsa memo for ordinary files.
    pub(crate) fn for_file(db: &'db dyn Db, file: File) -> Option<Self> {
        // Most files are not scripts. Check the existing tag query first so ordinary files
        // do not also allocate a tracked `script` memo just to cache another `None`.
        script_tag(db, file)?;
        script(db, file)
    }

    /// Cache dependency declarations separately from settings, which can remain unchanged after
    /// uv synchronizes an edit to the script's dependencies.
    #[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn dependency_metadata(
        self,
        db: &'db dyn Db,
    ) -> Result<Option<Box<DependencyMetadata>>, DependencyMetadataError> {
        if !self.has_valid_settings(db) {
            return Ok(None);
        }

        let Some(metadata) = script_environment(db, self.file(db))
            .and_then(|environment| environment.uv_metadata(db))
        else {
            return Ok(None);
        };
        metadata
            .dependency_metadata()
            .map(|metadata| Some(Box::new(metadata)))
    }
}

impl get_size2::GetSize for Script<'_> {}

/// Resolve the `Script` for `file` if it has a PEP 723 metadata block or `None` otherwise.
#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn script(db: &dyn Db, file: File) -> Option<Script<'_>> {
    // Ordinary files must not depend on project membership.
    script_tag(db, file)?;
    if !crate::is_project_file(db, file) {
        return None;
    }

    let mut diagnostics = ScriptConfigurationDiagnostics::default();
    let fallback_metadata;
    let metadata = match script_metadata(db, file)? {
        Ok(metadata) => metadata,
        Err(error) => {
            diagnostics.report_invalid(error.clone());
            // Build a best-effort program for editor features using empty inline metadata.
            fallback_metadata = ScriptMetadata::default();
            &fallback_metadata
        }
    };

    let environment = script_environment(db, file);
    let uv_metadata = environment.and_then(|environment| environment.uv_metadata(db));

    if let Some(diagnostic) =
        environment.and_then(|environment| environment.initialization_error(db))
    {
        let mut diagnostic = diagnostic.clone();
        if let Some(tag) = script_tag(db, file) {
            let mut annotation = Annotation::primary(Span::from(file).with_range(tag.range()));
            annotation.hide_snippet(true);
            diagnostic.annotate(annotation);
        }
        diagnostics.report_invalid(diagnostic);
    }

    let project_metadata = db.project().metadata(db);
    let program_settings = if let Some(inline_options) = inline_script_options(db, file) {
        // A script with it's own `tool.ty` table uses its own inline settings over the project settings.
        // Merge and validate the settings (`file_settings` can't report diagnostics).
        let mut script_options = inline_options.clone();
        metadata.apply_requires_python(file, &mut script_options, &mut diagnostics);
        let options = merge_script_options(project_metadata, &script_options, uv_metadata);

        diagnostics.extend(
            inline_options
                .lint_unmerged(db)
                .into_iter()
                .map(|diagnostic| diagnostic.to_diagnostic()),
        );

        // Report diagnostics for other options that need resolving: e.g. invalid globs.
        if let Some(analysis) = &options.analysis {
            let mut errors = Vec::new();
            analysis.to_settings(db, AnalysisOptionsContext::TopLevel, &mut errors);
            if let Some(error) = errors.first() {
                diagnostics.report_invalid(error.to_diagnostic());
            }
        }

        // Future: Consider reporting a diagnostic for unused tables, e.g. `tool.src`
        let root = file
            .path(db)
            .as_system_path()
            .and_then(|path| path.parent())
            .unwrap_or_else(|| db.system().current_directory());

        resolve_script_program_settings(
            db,
            &options,
            OptionsContext::Script(root),
            project_metadata.name(),
            file,
            &mut diagnostics,
        )
    } else {
        // A script without its own `tool.ty` table. Use the project settings instead.
        // The settings were already validated by the project.
        let mut script_options = Options::default();
        let base_options = if project_metadata.config_file_override().is_some() {
            project_metadata.options()
        } else {
            // Respect a script's `requires-python` option
            metadata.apply_requires_python(file, &mut script_options, &mut diagnostics);
            if let Some(mut environment) = project_metadata.options().environment.clone() {
                // Each script is a standalone program with its own first-party roots.
                environment.root = None;
                script_options.environment.combine_with(Some(environment));
            }
            &script_options
        };

        let options = merge_script_options(project_metadata, base_options, uv_metadata);

        resolve_script_program_settings(
            db,
            &options,
            OptionsContext::Script(project_metadata.root()),
            project_metadata.name(),
            file,
            &mut diagnostics,
        )
    };

    program_settings.search_paths.try_register_static_roots(db);

    let program = Program::from_settings(db, &program_settings);

    Some(Script::new(
        db,
        file,
        program,
        program_settings.python_version,
        !diagnostics.has_invalid_settings,
        diagnostics.diagnostics.into_boxed_slice(),
    ))
}

/// Returns the script's inline `[tool.ty]` options unless `--config-file` takes precedence.
///
/// Callers can read these unmerged options before indexing the project.
/// User configuration, LSP, CLI etc. have not been applied.
pub(crate) fn inline_script_options(db: &dyn Db, file: File) -> Option<&Options> {
    // Check the tag first so ordinary files skip the project lookup and avoid caching
    // a second `None` in `script_metadata`.
    script_tag(db, file)?;

    if db.project().metadata(db).config_file_override().is_some() {
        return None;
    }
    let script = script_metadata(db, file)?.as_ref().ok()?;
    script.ty()
}

/// Returns the PEP 723 script tag embedded in `file`.
///
/// Most files have no script tag. Boxing keeps the cached result compact when it is `None`.
#[salsa::tracked(returns(as_deref))]
pub fn script_tag(db: &dyn SourceDb, file: File) -> Option<Box<ScriptTag>> {
    let path = file.path(db);
    if path.is_vendored_path() {
        return None;
    }

    // Notebook outputs can be large, so skip notebooks before reading their contents.
    if is_notebook(db, file) {
        return None;
    }

    let source = source_text(db, file);
    ScriptTag::parse(source.as_bytes()).map(Box::new)
}

/// Parses a script's inline metadata. Edits that move an option must invalidate its diagnostic
/// spans even when the option's value compares equal.
#[salsa::tracked(returns(as_deref), no_eq, heap_size=ruff_memory_usage::heap_size)]
fn script_metadata(db: &dyn Db, file: File) -> Option<Box<Result<ScriptMetadata, Diagnostic>>> {
    let tag = script_tag(db, file)?;

    let result = {
        let _guard = ValueSourceGuard::with_source_map(
            ValueSource::ScriptMetadata(file),
            tag.source_map().clone(),
        );
        toml::from_str::<ScriptMetadata>(tag.metadata())
    };

    let mut metadata = match result {
        Ok(metadata) => metadata,
        Err(error) => {
            let range = error.span().and_then(|span| {
                let start = TextSize::try_from(span.start).ok()?;
                let end = TextSize::try_from(span.end).ok()?;
                Some(tag.source_map().map_range(TextRange::new(start, end)))
            });
            return Some(Box::new(Err(invalid_script_metadata_diagnostic(
                file,
                error.message(),
                range,
            ))));
        }
    };

    if let Some(options) = metadata.tool.as_mut().and_then(|tool| tool.ty.as_mut()) {
        options.order_rule_selectors(db.lint_registry());
    }

    Some(Box::new(Ok(metadata)))
}

fn merge_script_options(
    project_metadata: &ProjectMetadata,
    base_options: &Options,
    uv_metadata: Option<&UvMetadata>,
) -> Options {
    let uv_options = uv_metadata.map(|metadata| Options {
        environment: Some(EnvironmentOptions {
            python: metadata
                .environment()
                .map(|path| RelativePathBuf::new(path, ValueSource::UvMetadata)),
            ..EnvironmentOptions::default()
        }),
        ..Options::default()
    });

    let mut options = Options::default();
    // Merge the options with CLI, LSP, user configuration, and fallback options
    for layer in project_metadata.options_in_precedence_order(base_options, uv_options.as_ref()) {
        options.combine_with(layer.clone());
    }

    // An explicit Python environment selects uv's interpreter, not the script's site-packages.
    if let Some(environment) = uv_metadata.and_then(UvMetadata::environment) {
        options.environment.get_or_insert_default().python =
            Some(RelativePathBuf::new(environment, ValueSource::UvMetadata));
    }

    // Unlike projects, standalone scripts default to `root = []`.
    options
        .environment
        .get_or_insert_default()
        .root
        .get_or_insert_default();

    options
}

fn resolve_script_program_settings(
    db: &dyn Db,
    options: &Options,
    context: OptionsContext<'_>,
    project_name: &str,
    file: File,
    diagnostics: &mut ScriptConfigurationDiagnostics,
) -> ProgramSettings {
    let (settings, settings_diagnostics) = match options.to_program_settings(
        context,
        project_name,
        db.system(),
        db.vendored(),
        &FallibleStrategy,
    ) {
        Ok(settings) => settings,
        Err(error) => {
            let (source_file, range) = error
                .setting_source(options)
                .and_then(|(source, range)| source.file(db).map(|file| (file, range)))
                .unwrap_or((file, None));

            let mut diagnostic =
                invalid_script_metadata_diagnostic(source_file, error.message(), range);
            if let Some(hint) = error.hint() {
                diagnostic.sub(SubDiagnostic::new(SubDiagnosticSeverity::Info, hint));
            }
            diagnostics.report_invalid(diagnostic);

            let Ok(settings) = options.to_program_settings(
                context,
                project_name,
                db.system(),
                db.vendored(),
                &UseDefaultStrategy,
            );
            settings
        }
    };
    diagnostics.extend(
        settings_diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.into_diagnostic(db).to_diagnostic()),
    );
    settings
}

/// PEP 723 metadata, whose Python requirement belongs at the top level rather than in `project`.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, get_size2::GetSize)]
#[serde(rename_all = "kebab-case")]
struct ScriptMetadata {
    // VersionSpecifiers does not expose its heap allocation.
    #[get_size(ignore)]
    requires_python: Option<RangedValue<VersionSpecifiers>>,
    tool: Option<Tool>,
}

impl ScriptMetadata {
    /// Returns the script's `[tool.ty]` options (if any).
    fn ty(&self) -> Option<&Options> {
        self.tool.as_ref()?.ty.as_ref()
    }

    /// Applies the `requires-python` constraint to `options`.
    fn apply_requires_python(
        &self,
        file: File,
        options: &mut Options,
        diagnostics: &mut ScriptConfigurationDiagnostics,
    ) {
        if let Err(error) = options.apply_requires_python(self.requires_python.as_ref()) {
            let range = self.requires_python.as_ref().and_then(RangedValue::range);
            let mut diagnostic = invalid_script_metadata_diagnostic(file, error.message(), range);
            if let Some(hint) = error.hint() {
                diagnostic.sub(SubDiagnostic::new(SubDiagnosticSeverity::Info, hint));
            }
            diagnostics.report_invalid(diagnostic);
        }
    }
}

#[derive(Default)]
struct ScriptConfigurationDiagnostics {
    diagnostics: Vec<Diagnostic>,
    has_invalid_settings: bool,
}

impl ScriptConfigurationDiagnostics {
    fn report_invalid(&mut self, diagnostic: Diagnostic) {
        self.has_invalid_settings = true;
        self.diagnostics.push(diagnostic);
    }

    fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(diagnostics);
    }
}

fn invalid_script_metadata_diagnostic(
    file: File,
    message: impl std::fmt::Display,
    range: Option<TextRange>,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        DiagnosticId::InvalidScriptMetadata,
        Severity::Error,
        message,
    );
    diagnostic.annotate(Annotation::primary(
        Span::from(file).with_optional_range(range),
    ));
    diagnostic
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::source::source_text;
    use ruff_db::system::{DbWithWritableSystem as _, SystemPath, SystemPathBuf};
    use ruff_db::testing::{assert_function_query_was_not_run, assert_function_query_was_run};
    use ty_python_semantic::Db as _;

    use crate::db::testing::TestDb;
    use crate::{Db as _, ProjectMetadata};

    use super::{Script, script};

    #[test]
    fn ordinary_files_do_not_depend_on_open_files() -> anyhow::Result<()> {
        let mut db = TestDb::new(ProjectMetadata::new(
            "test",
            SystemPathBuf::from("/project"),
        ));
        db.write_files([
            ("/project/ordinary.py", "value = 1\n"),
            ("/project/opened.py", "value = 2\n"),
        ])?;
        let ordinary = system_path_to_file(&db, SystemPath::new("/project/ordinary.py"))?;
        let opened = system_path_to_file(&db, SystemPath::new("/project/opened.py"))?;

        assert!(Script::for_file(&db, ordinary).is_none());
        let events = db.take_salsa_events();
        assert_function_query_was_not_run(&db, script, ordinary, &events);

        assert!(script(&db, ordinary).is_none());
        db.take_salsa_events();

        db.project().open_file(&mut db, opened);
        db.take_salsa_events();

        assert!(script(&db, ordinary).is_none());
        let events = db.take_salsa_events();
        assert_function_query_was_not_run(&db, crate::should_check_file, ordinary, &events);
        assert_function_query_was_not_run(&db, script, ordinary, &events);

        Ok(())
    }

    #[test]
    fn script_tag_does_not_read_notebook_source() -> anyhow::Result<()> {
        let mut db = TestDb::new(ProjectMetadata::new(
            "test",
            SystemPathBuf::from("/project"),
        ));
        db.write_file(
            "/project/notebook.ipynb",
            r#"{"cells": [], "metadata": {}, "nbformat": 4, "nbformat_minor": 5}"#,
        )?;
        let notebook = system_path_to_file(&db, SystemPath::new("/project/notebook.ipynb"))?;

        assert_eq!(db.project().script_files(&db).iter().count(), 0);
        assert!(db.project().files(&db).contains(notebook));

        db.take_salsa_events();
        source_text(&db, notebook);
        let events = db.take_salsa_events();
        // This assertion would fail if index already read the full text of notebook.
        assert_function_query_was_run(&db, source_text, notebook, &events);

        Ok(())
    }

    #[test]
    fn equivalent_script_settings_share_programs() -> anyhow::Result<()> {
        let mut db = TestDb::new(ProjectMetadata::new(
            "test",
            SystemPathBuf::from("/project"),
        ));
        db.write_dedented(
            "/project/requirement.py",
            r#"
            # /// script
            # requires-python = ">=3.12"
            # ///
            "#,
        )?;
        db.write_dedented(
            "/project/nested/configured.py",
            r#"
            # /// script
            # [tool.ty.environment]
            # python-version = "3.12"
            # ///
            "#,
        )?;

        let requirement = system_path_to_file(&db, SystemPath::new("/project/requirement.py"))?;
        let configured =
            system_path_to_file(&db, SystemPath::new("/project/nested/configured.py"))?;

        assert_eq!(
            db.program_file(requirement).program(&db),
            db.program_file(configured).program(&db)
        );
        assert_ne!(
            db.python_version_with_source(requirement),
            db.python_version_with_source(configured)
        );

        Ok(())
    }
}
