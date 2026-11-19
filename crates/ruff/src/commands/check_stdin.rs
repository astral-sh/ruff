use std::path::Path;

use anyhow::Result;
use ruff_linter::message::RenderContext;
use ruff_linter::settings::flags;
use ruff_workspace::resolver::{Resolver, RootSettings, match_exclusion, project_file_at_path};

use crate::args::ConfigArguments;
use crate::diagnostics::{Diagnostics, lint_stdin};
use crate::stdin::{parrot_stdin, read_from_stdin};

/// Run the linter over a single file, read from `stdin`.
pub(crate) fn check_stdin(
    filename: Option<&Path>,
    root_settings: &RootSettings,
    overrides: &ConfigArguments,
    noqa: flags::Noqa,
    fix_mode: flags::FixMode,
) -> Result<Diagnostics> {
    let mut resolver = Resolver::new(root_settings);

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
    let mut diagnostics = lint_stdin(
        filename,
        stdin,
        &settings.linter,
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
