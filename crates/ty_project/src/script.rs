use pep440_rs::VersionSpecifiers;
use ruff_db::Db as SourceDb;
use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::File;
use ruff_db::source::source_text;
use ruff_python_ast::script::ScriptTag;
use ruff_ranged_value::{RangedValue, ValueSource, ValueSourceGuard};
use serde::Deserialize;
use ty_combine::Combine;
use ty_python_core::program::{FallibleStrategy, Program, ProgramSettings};
use ty_python_semantic::PythonVersionWithSource;

use crate::metadata::options::{Options, OptionsContext};
use crate::metadata::pyproject::Tool;
use crate::metadata::settings::Settings;
use crate::{Db, ProjectMetadata};

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

    #[tracked]
    #[returns(deref)]
    pub(crate) diagnostics: Box<[Diagnostic]>,
}

impl<'db> Script<'db> {
    /// Returns the script for `file` without creating a second Salsa memo for ordinary files.
    pub(crate) fn for_file(db: &'db dyn Db, file: File) -> Option<Self> {
        // Most files are not scripts. Check the existing metadata query first so ordinary files
        // do not also allocate a tracked `script` memo just to cache another `None`.
        script_metadata(db, file)?;
        script(db, file)
    }
}

impl get_size2::GetSize for Script<'_> {}

/// Resolve the `Script` for `file` if it has a PEP 723 metadata block or `None` otherwise.
#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn script(db: &dyn Db, file: File) -> Option<Script<'_>> {
    // Files without script metadata must not depend on the low-durability open-file set.
    let metadata = script_metadata(db, file)?;

    // Never treat third-party files as scripts.
    if !crate::should_check_file(db, file) {
        return None;
    }

    let configuration_root = file
        .path(db)
        .as_system_path()
        .and_then(|path| path.parent())
        .unwrap_or_else(|| db.system().current_directory());
    let context = OptionsContext::Script(configuration_root);

    let project_metadata = db.project().metadata(db);

    let mut diagnostics = Vec::new();
    // FIXME: Report configuration errors as diagnostics and skip checking the script entirely so
    // that fixes cannot be applied using the enclosing project's configuration.
    let options = resolve_script_options(project_metadata, metadata)?;
    let settings = resolve_script_settings(db, &options, context, &mut diagnostics)?;
    let program_settings = resolve_script_program_settings(
        db,
        &options,
        context,
        project_metadata.name(),
        &mut diagnostics,
    )?;

    program_settings.search_paths.try_register_static_roots(db);

    let program = Program::from_settings(db, &program_settings);

    Some(Script::new(
        db,
        file,
        settings,
        program,
        program_settings.python_version,
        diagnostics.into_boxed_slice(),
    ))
}

fn resolve_script_options(
    project_metadata: &ProjectMetadata,
    metadata: &ScriptMetadata,
) -> Option<Options> {
    // When using `--config-file <FILE>`, use the settings from `<FILE>`
    let inline = if project_metadata.config_file_override().is_some() {
        project_metadata.options().clone()
    } else {
        // Otherwise use the script's settings.
        metadata.to_options()?
    };

    let mut options = Options::default();
    // Merge the options with CLI, LSP, user configuration, and fallback options
    for layer in project_metadata.script_options_in_precedence_order(&inline) {
        options.combine_with(layer.clone());
    }

    // Unlike Project's, default to `[]` for scripts (unless explicitly specified).
    options
        .environment
        .get_or_insert_default()
        .root
        .get_or_insert_default();

    Some(options)
}

fn resolve_script_settings(
    db: &dyn Db,
    options: &Options,
    context: OptionsContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Settings> {
    let (settings, settings_diagnostics) =
        options.to_settings(db, context, &FallibleStrategy).ok()?;
    diagnostics.extend(
        settings_diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.to_diagnostic()),
    );
    Some(settings)
}

fn resolve_script_program_settings(
    db: &dyn Db,
    options: &Options,
    context: OptionsContext<'_>,
    project_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ProgramSettings> {
    let (settings, settings_diagnostics) = options
        .to_program_settings(
            context,
            project_name,
            db.system(),
            db.vendored(),
            &FallibleStrategy,
        )
        .ok()?;
    diagnostics.extend(
        settings_diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.into_diagnostic(db).to_diagnostic()),
    );
    Some(settings)
}

/// Returns the PEP 723 metadata embedded in `file`.
///
/// Most files have no script metadata. Boxing keeps the cached result compact when it is `None`.
#[salsa::tracked(returns(as_deref))]
pub(crate) fn script_metadata(db: &dyn SourceDb, file: File) -> Option<Box<ScriptMetadata>> {
    let path = file.path(db);
    if path.is_vendored_path() {
        return None;
    }

    let source = source_text(db, file);
    if source.is_notebook() {
        return None;
    }

    let (content, source_map) = ScriptTag::parse(source.as_bytes())?.into_metadata_and_source_map();
    let _guard = ValueSourceGuard::with_source_map(ValueSource::ScriptMetadata(file), source_map);
    // FIXME: Report invalid TOML in script metadata instead of silently ignoring it.
    let mut metadata: ScriptMetadata = toml::from_str(&content).ok()?;

    if let Some(options) = metadata.tool.as_mut().and_then(|tool| tool.ty.as_mut()) {
        options.prioritize_all_selectors();
    }

    Some(Box::new(metadata))
}

/// PEP 723 metadata, whose Python requirement belongs at the top level rather than in `project`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ScriptMetadata {
    requires_python: Option<RangedValue<VersionSpecifiers>>,
    tool: Option<Tool>,
}

impl ScriptMetadata {
    fn to_options(&self) -> Option<Options> {
        let mut options = self.ty().cloned().unwrap_or_default();
        options
            .apply_requires_python(self.requires_python.as_ref())
            .ok()?;
        Some(options)
    }

    fn ty(&self) -> Option<&Options> {
        self.tool.as_ref().and_then(|tool| tool.ty.as_ref())
    }
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
