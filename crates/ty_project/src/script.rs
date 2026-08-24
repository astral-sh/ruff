use pep440_rs::VersionSpecifiers;
use ruff_db::Db as SourceDb;
use ruff_db::diagnostic::{
    Annotation, Diagnostic, DiagnosticId, Severity, Span, SubDiagnostic, SubDiagnosticSeverity,
};
use ruff_db::files::File;
use ruff_db::source::source_text;
use ruff_python_ast::script::ScriptTag;
use ruff_ranged_value::{RangedValue, ValueSource, ValueSourceGuard};
use ruff_text_size::{Ranged, TextRange, TextSize};
use serde::Deserialize;
use ty_combine::Combine;
use ty_python_core::program::{FallibleStrategy, Program, ProgramSettings, UseDefaultStrategy};
use ty_python_semantic::PythonVersionWithSource;

use crate::metadata::options::{EnvironmentOptions, Options, OptionsContext};
use crate::metadata::pyproject::Tool;
use crate::metadata::settings::Settings;
use crate::metadata::value::RelativePathBuf;
use crate::uv::UvMetadata;
use crate::{Db, ProjectMetadata};

mod environment;

pub(crate) use environment::ScriptEnvironmentCacheKey;
use environment::script_environment;
pub use environment::{ScriptEnvironmentAvailability, ScriptEnvironments};

/// A standalone PEP 723 script and its resolved settings.
#[salsa::tracked(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct Script<'db> {
    #[returns(copy)]
    pub(crate) file: File,

    #[tracked]
    #[returns(ref)]
    pub(crate) settings: Settings,

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

impl<'db> Script<'db> {
    /// Returns the script for `file` without creating a second Salsa memo for ordinary files.
    pub(crate) fn for_file(db: &'db dyn Db, file: File) -> Option<Self> {
        // Most files are not scripts. Check the existing tag query first so ordinary files
        // do not also allocate a tracked `script` memo just to cache another `None`.
        script_tag(db, file)?;
        script(db, file)
    }
}

impl get_size2::GetSize for Script<'_> {}

/// Resolve the `Script` for `file` if it has a PEP 723 metadata block or `None` otherwise.
#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn script(db: &dyn Db, file: File) -> Option<Script<'_>> {
    // Files without script metadata must not depend on the low-durability open-file set.
    let tag = script_tag(db, file)?;

    // Never treat third-party files as scripts.
    if !crate::should_check_file(db, file) {
        return None;
    }

    let mut diagnostics = ScriptConfigurationDiagnostics::default();
    let metadata = parse_script_metadata(file, tag, &mut diagnostics);
    let environment = script_environment(db, file);
    let uv_metadata = environment.and_then(|environment| environment.uv_metadata(db));

    if let Some(error) = environment.and_then(|environment| environment.initialization_error(db)) {
        diagnostics.report_invalid(uv_metadata_diagnostic(file, tag, error));
    }

    let configuration_root = file
        .path(db)
        .as_system_path()
        .and_then(|path| path.parent())
        .unwrap_or_else(|| db.system().current_directory());
    let context = OptionsContext::Script(configuration_root);

    let project_metadata = db.project().metadata(db);

    let options = resolve_script_options(
        project_metadata,
        &metadata,
        uv_metadata,
        file,
        &mut diagnostics,
    );
    let settings = resolve_script_settings(db, &options, context, &mut diagnostics);
    let program_settings = resolve_script_program_settings(
        db,
        &options,
        context,
        project_metadata.name(),
        file,
        &mut diagnostics,
    );

    program_settings.search_paths.try_register_static_roots(db);

    let program = Program::from_settings(db, &program_settings);

    Some(Script::new(
        db,
        file,
        settings,
        program,
        program_settings.python_version,
        !diagnostics.has_invalid_settings,
        diagnostics.diagnostics.into_boxed_slice(),
    ))
}

/// Returns the PEP 723 script tag embedded in `file`.
///
/// Most files have no script tag. Boxing keeps the cached result compact when it is `None`.
#[salsa::tracked(returns(as_deref))]
pub(crate) fn script_tag(db: &dyn SourceDb, file: File) -> Option<Box<ScriptTag>> {
    let path = file.path(db);
    if path.is_vendored_path() {
        return None;
    }

    let source = source_text(db, file);
    if source.is_notebook() {
        return None;
    }

    ScriptTag::parse(source.as_bytes()).map(Box::new)
}

fn parse_script_metadata(
    file: File,
    tag: &ScriptTag,
    diagnostics: &mut ScriptConfigurationDiagnostics,
) -> ScriptMetadata {
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

            diagnostics.report_invalid(invalid_script_metadata_diagnostic(
                file,
                error.message(),
                range,
            ));
            return ScriptMetadata::default();
        }
    };

    if let Some(options) = metadata.tool.as_mut().and_then(|tool| tool.ty.as_mut()) {
        options.prioritize_all_selectors();
    }

    metadata
}

fn resolve_script_options(
    project_metadata: &ProjectMetadata,
    metadata: &ScriptMetadata,
    uv_metadata: Option<&UvMetadata>,
    file: File,
    diagnostics: &mut ScriptConfigurationDiagnostics,
) -> Options {
    // When using `--config-file <FILE>`, use the settings from `<FILE>`
    let inline = if project_metadata.config_file_override().is_some() {
        project_metadata.options().clone()
    } else {
        // Otherwise use the script's settings.
        metadata.to_options(file, diagnostics)
    };

    let uv_options = uv_metadata.map(|metadata| Options {
        environment: Some(EnvironmentOptions {
            python_version: metadata.python_version().cloned(),
            python: metadata
                .environment()
                .map(|path| RelativePathBuf::new(path, ValueSource::UvMetadata)),
            ..EnvironmentOptions::default()
        }),
        ..Options::default()
    });

    let mut options = Options::default();
    // Merge the options with CLI, LSP, user configuration, and fallback options
    for layer in project_metadata.options_in_precedence_order(&inline, uv_options.as_ref()) {
        options.combine_with(layer.clone());
    }

    // An explicit Python environment selects uv's interpreter, not the script's site-packages.
    if let Some(environment) = uv_metadata.and_then(UvMetadata::environment) {
        options.environment.get_or_insert_default().python =
            Some(RelativePathBuf::new(environment, ValueSource::UvMetadata));
    }

    // Unlike Project's, default to `[]` for scripts (unless explicitly specified).
    options
        .environment
        .get_or_insert_default()
        .root
        .get_or_insert_default();

    options
}

fn resolve_script_settings(
    db: &dyn Db,
    options: &Options,
    context: OptionsContext<'_>,
    diagnostics: &mut ScriptConfigurationDiagnostics,
) -> Settings {
    let (settings, settings_diagnostics) = match options.to_settings(db, context, &FallibleStrategy)
    {
        Ok(settings) => settings,
        Err(error) => {
            diagnostics.report_invalid(error.into_diagnostic().to_diagnostic());
            let Ok(settings) = options.to_settings(db, context, &UseDefaultStrategy);
            settings
        }
    };
    diagnostics.extend(
        settings_diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.to_diagnostic()),
    );
    settings
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
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ScriptMetadata {
    requires_python: Option<RangedValue<VersionSpecifiers>>,
    tool: Option<Tool>,
}

impl ScriptMetadata {
    fn to_options(&self, file: File, diagnostics: &mut ScriptConfigurationDiagnostics) -> Options {
        let mut options = self.ty().cloned().unwrap_or_default();
        if let Err(error) = options.apply_requires_python(self.requires_python.as_ref()) {
            let range = self.requires_python.as_ref().and_then(RangedValue::range);
            let mut diagnostic = invalid_script_metadata_diagnostic(file, error.message(), range);
            if let Some(hint) = error.hint() {
                diagnostic.sub(SubDiagnostic::new(SubDiagnosticSeverity::Info, hint));
            }
            diagnostics.report_invalid(diagnostic);
        }
        options
    }

    fn ty(&self) -> Option<&Options> {
        self.tool.as_ref().and_then(|tool| tool.ty.as_ref())
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

fn uv_metadata_diagnostic(file: File, tag: &ScriptTag, message: &str) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(DiagnosticId::UvMetadata, Severity::Error, message);
    let mut annotation = Annotation::primary(Span::from(file).with_range(tag.range()));
    annotation.hide_snippet(true);
    diagnostic.annotate(annotation);
    diagnostic
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
    use ruff_db::system::{DbWithWritableSystem as _, SystemPath, SystemPathBuf};
    use ruff_db::testing::assert_function_query_was_not_run;
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
