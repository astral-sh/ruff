use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    path::Path,
};

use ruff_source_file::LineColumn;
use serde::{Serialize, Serializer, ser::SerializeSeq};

use crate::diagnostic::{Diagnostic, Severity};

use super::FileResolver;

pub(super) struct GitlabRenderer<'a> {
    resolver: &'a dyn FileResolver,
}

impl<'a> GitlabRenderer<'a> {
    pub(super) fn new(resolver: &'a dyn FileResolver) -> Self {
        Self { resolver }
    }
}

impl GitlabRenderer<'_> {
    pub(super) fn render(
        &self,
        f: &mut std::fmt::Formatter,
        diagnostics: &[Diagnostic],
    ) -> std::fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_string_pretty(&SerializedMessages {
                diagnostics,
                resolver: self.resolver,
                #[expect(
                    clippy::disallowed_methods,
                    reason = "We don't have access to a `System` here, \
                              and this is only intended for use by GitLab CI, \
                              which runs on a real `System`."
                )]
                project_dir: std::env::var("CI_PROJECT_DIR").ok().as_deref(),
            })
            .unwrap()
        )
    }
}

struct SerializedMessages<'a> {
    diagnostics: &'a [Diagnostic],
    resolver: &'a dyn FileResolver,
    project_dir: Option<&'a str>,
}

impl Serialize for SerializedMessages<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.diagnostics.len()))?;
        let mut fingerprints = HashSet::<u64>::with_capacity(self.diagnostics.len());

        for diagnostic in self.diagnostics {
            let location = diagnostic
                .primary_span()
                .map(|span| {
                    let file = span.file();
                    let positions = if self.resolver.is_notebook(file) {
                        // We can't give a reasonable location for the structured formats,
                        // so we show one that's clearly a fallback
                        Default::default()
                    } else {
                        let diagnostic_source = file.diagnostic_source(self.resolver);
                        let source_code = diagnostic_source.as_source_code();
                        span.range()
                            .map(|range| Positions {
                                begin: source_code.line_column(range.start()),
                                end: source_code.line_column(range.end()),
                            })
                            .unwrap_or_default()
                    };

                    let path = self.project_dir.as_ref().map_or_else(
                        || file.relative_path(self.resolver).display().to_string(),
                        |project_dir| relativize_path_to(file.path(self.resolver), project_dir),
                    );

                    Location { path, positions }
                })
                .unwrap_or_default();

            let mut message_fingerprint = fingerprint(diagnostic, &location.path, 0);

            // Make sure that we do not get a fingerprint that is already in use
            // by adding in the previously generated one.
            while fingerprints.contains(&message_fingerprint) {
                message_fingerprint = fingerprint(diagnostic, &location.path, message_fingerprint);
            }
            fingerprints.insert(message_fingerprint);

            let description = diagnostic.concise_message();
            let check_name = diagnostic.secondary_code_or_id();
            let severity = match diagnostic.severity() {
                Severity::Info => "info",
                Severity::Warning => "minor",
                Severity::Error => "major",
                // Another option here is `blocker`
                Severity::Fatal => "critical",
            };

            let value = Message {
                check_name,
                // GitLab doesn't display the separate `check_name` field in a Code Quality report,
                // so prepend it to the description too.
                description: format!("{check_name}: {description}"),
                severity,
                fingerprint: format!("{:x}", message_fingerprint),
                location,
            };

            s.serialize_element(&value)?;
        }

        s.end()
    }
}

#[derive(Serialize)]
struct Message<'a> {
    check_name: &'a str,
    description: String,
    severity: &'static str,
    fingerprint: String,
    location: Location,
}

/// The place in the source code where the issue was discovered.
///
/// According to the CodeClimate report format [specification] linked from the GitLab [docs], this
/// field is required, so we fall back on a default `path` and position if the diagnostic doesn't
/// have a primary span.
///
/// [specification]: https://github.com/codeclimate/platform/blob/master/spec/analyzers/SPEC.md#data-types
/// [docs]: https://docs.gitlab.com/ci/testing/code_quality/#code-quality-report-format
#[derive(Default, Serialize)]
struct Location {
    path: String,
    positions: Positions,
}

#[derive(Default, Serialize)]
struct Positions {
    begin: LineColumn,
    end: LineColumn,
}

/// Generate a unique fingerprint to identify a violation.
fn fingerprint(diagnostic: &Diagnostic, project_path: &str, salt: u64) -> u64 {
    let mut hasher = DefaultHasher::new();

    salt.hash(&mut hasher);
    diagnostic.name().hash(&mut hasher);
    project_path.hash(&mut hasher);

    hasher.finish()
}

/// Convert an absolute path to be relative to the specified project root.
fn relativize_path_to<P: AsRef<Path>, R: AsRef<Path>>(path: P, project_root: R) -> String {
    format!(
        "{}",
        pathdiff::diff_paths(&path, project_root)
            .expect("Could not diff paths")
            .display()
    )
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{
        DiagnosticFormat,
        render::tests::{create_diagnostics, create_syntax_error_diagnostics},
    };

    const FINGERPRINT_FILTERS: [(&str, &str); 1] = [(
        r#""fingerprint": "[a-z0-9]+","#,
        r#""fingerprint": "<redacted>","#,
    )];

    #[test]
    fn output() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Gitlab);
        insta::with_settings!({filters => FINGERPRINT_FILTERS}, {
            insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
        });
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Gitlab);
        insta::with_settings!({filters => FINGERPRINT_FILTERS}, {
            insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
        });
    }
}
