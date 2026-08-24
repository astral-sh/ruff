//! Run all code and documentation generation steps.

use anyhow::Result;
use similar::TextDiff;

use crate::{
    generate_cli_help, generate_docs, generate_json_schema, generate_ty_cli_reference,
    generate_ty_env_vars_reference, generate_ty_options, generate_ty_rules, generate_ty_schema,
};

pub(crate) const REGENERATE_ALL_COMMAND: &str = "cargo dev generate-all";

#[derive(clap::Args)]
pub(crate) struct Args {
    #[arg(long, default_value_t, value_enum)]
    mode: Mode,
}

#[derive(Copy, Clone, PartialEq, Eq, clap::ValueEnum, Default)]
pub(crate) enum Mode {
    /// Update the content in the `configuration.md`.
    #[default]
    Write,

    /// Don't write to the file, check if the file is up-to-date and error if not.
    Check,

    /// Write the generated help to stdout.
    DryRun,
}

impl Mode {
    pub(crate) const fn is_dry_run(self) -> bool {
        matches!(self, Mode::DryRun)
    }
}

pub(crate) fn main(args: &Args) -> Result<()> {
    generate_json_schema::main(&generate_json_schema::Args { mode: args.mode })?;
    generate_ty_schema::main(&generate_ty_schema::Args { mode: args.mode })?;
    generate_cli_help::main(&generate_cli_help::Args { mode: args.mode })?;
    generate_docs::main(&generate_docs::Args {
        dry_run: args.mode.is_dry_run(),
    })?;
    generate_ty_options::main(&generate_ty_options::Args { mode: args.mode })?;
    generate_ty_rules::main(&generate_ty_rules::Args { mode: args.mode })?;
    generate_ty_cli_reference::main(&generate_ty_cli_reference::Args { mode: args.mode })?;
    generate_ty_env_vars_reference::main(&generate_ty_env_vars_reference::Args {
        mode: args.mode,
    })?;
    Ok(())
}

/// Limit generated-file diffs so stale files do not overwhelm CI logs.
pub(crate) fn generated_file_diff(current: &str, generated: &str) -> String {
    const MAX_DIFF_LINES: usize = 100;

    let diff = TextDiff::from_lines(current, generated)
        .unified_diff()
        .to_string();
    let end: usize = diff
        .split_inclusive('\n')
        .take(MAX_DIFF_LINES)
        .map(str::len)
        .sum();

    if end < diff.len() {
        format!(
            "{}… diff truncated after {MAX_DIFF_LINES} lines\n",
            &diff[..end]
        )
    } else {
        diff
    }
}

#[cfg(test)]
mod tests {
    use super::generated_file_diff;

    #[test]
    fn short_diff() {
        assert_eq!(
            generated_file_diff("unchanged\nold\n", "unchanged\nnew\n"),
            "@@ -1,2 +1,2 @@\n unchanged\n-old\n+new\n"
        );
    }

    #[test]
    fn diff_within_limit() {
        for line_count in [98, 99] {
            let generated = "new\n".repeat(line_count);
            assert_eq!(
                generated_file_diff("", &generated),
                format!(
                    "@@ -0,0 +1,{line_count} @@\n{}",
                    "+new\n".repeat(line_count)
                )
            );
        }
    }

    #[test]
    fn truncated_diff() {
        for line_count in [100, 200] {
            let generated = "new\n".repeat(line_count);
            assert_eq!(
                generated_file_diff("", &generated),
                format!(
                    "@@ -0,0 +1,{line_count} @@\n{}… diff truncated after 100 lines\n",
                    "+new\n".repeat(99)
                )
            );
        }
    }

    #[test]
    fn diff_after_unchanged_prefix() {
        let prefix = "unchanged\n".repeat(150);
        assert_eq!(
            generated_file_diff(&format!("{prefix}old\n"), &format!("{prefix}new\n")),
            "@@ -148,4 +148,4 @@\n unchanged\n unchanged\n unchanged\n-old\n+new\n"
        );
    }
}
