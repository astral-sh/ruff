use std::path::Path;

use anyhow::Result;
use ruff_linter::message::RenderContext;
use ruff_linter::settings::flags;
use ruff_workspace::resolver::{Resolver, RootProject, match_exclusion, project_file_at_path};
use ruff_workspace::script::script;

use crate::args::ConfigArguments;
use crate::diagnostics::{Diagnostics, lint_stdin};
use crate::stdin::{parrot_stdin, read_from_stdin};

/// Run the linter over a single file, read from `stdin`.
pub(crate) fn check_stdin(
    filename: Option<&Path>,
    root_project: &RootProject,
    overrides: &ConfigArguments,
    noqa: flags::Noqa,
    fix_mode: flags::FixMode,
) -> Result<Diagnostics> {
    let mut resolver = Resolver::new(root_project);

    if resolver.force_exclude() {
        if let Some(filename) = filename {
            if !project_file_at_path(filename, &mut resolver, overrides)? {
                if fix_mode.is_apply() {
                    parrot_stdin()?;
                }
                return Ok(Diagnostics::default());
            }

            if filename.file_name().is_some_and(|name| {
                match_exclusion(filename, name, &resolver.root_settings().linter.exclude)
            }) {
                if fix_mode.is_apply() {
                    parrot_stdin()?;
                }
                return Ok(Diagnostics::default());
            }
        }
    }
    let stdin = read_from_stdin()?;
    let settings = resolver.root_settings();
    let source_type =
        filename.map_or_default(|path| settings.linter.extension.get_source_type(path));
    let project = &root_project.project;
    let script = script(
        project,
        overrides,
        overrides.script_configuration(),
        filename,
        source_type,
        &stdin,
    )?;
    let linter = match &script {
        Some(script) => &script.settings.linter,
        None => &settings.linter,
    };
    let mut diagnostics = lint_stdin(
        filename,
        source_type,
        stdin,
        linter,
        noqa,
        fix_mode,
        settings.unsafe_fixes,
    )?;
    let context = RenderContext::new(&diagnostics.notebook_indexes);
    diagnostics.inner.sort_unstable_by(|a, b| {
        a.rendering_sort_key(&context)
            .cmp(&b.rendering_sort_key(&context))
    });
    Ok(diagnostics)
}
