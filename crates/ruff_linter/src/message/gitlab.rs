use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write;

use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use serde_json::json;

use ruff_db::diagnostic::Diagnostic;

use crate::fs::{relativize_path, relativize_path_to};
use crate::message::{Emitter, EmitterContext};

/// Generate JSON with violations in GitLab CI format
//  https://docs.gitlab.com/ee/ci/testing/code_quality.html#implement-a-custom-tool
pub struct GitlabEmitter {
    project_dir: Option<String>,
}

impl Default for GitlabEmitter {
    fn default() -> Self {
        Self {
            project_dir: std::env::var("CI_PROJECT_DIR").ok(),
        }
    }
}

impl Emitter for GitlabEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[Diagnostic],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        serde_json::to_writer_pretty(
            writer,
            &SerializedMessages {
                diagnostics,
                context,
                project_dir: self.project_dir.as_deref(),
            },
        )?;

        Ok(())
    }
}

struct SerializedMessages<'a> {
    diagnostics: &'a [Diagnostic],
    context: &'a EmitterContext<'a>,
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
            let filename = diagnostic.expect_ruff_filename();

            let (start_location, end_location) = if self.context.is_notebook(&filename) {
                // We can't give a reasonable location for the structured formats,
                // so we show one that's clearly a fallback
                Default::default()
            } else {
                (
                    diagnostic.expect_ruff_start_location(),
                    diagnostic.expect_ruff_end_location(),
                )
            };

            let path = self.project_dir.as_ref().map_or_else(
                || relativize_path(&filename),
                |project_dir| relativize_path_to(&filename, project_dir),
            );

            let mut message_fingerprint = fingerprint(diagnostic, &path, 0);

            // Make sure that we do not get a fingerprint that is already in use
            // by adding in the previously generated one.
            while fingerprints.contains(&message_fingerprint) {
                message_fingerprint = fingerprint(diagnostic, &path, message_fingerprint);
            }
            fingerprints.insert(message_fingerprint);

            let description = diagnostic.body();
            let check_name = diagnostic.secondary_code_or_id();

            let value = json!({
                "check_name": check_name,
                // GitLab doesn't display the separate `check_name` field in a Code Quality report,
                // so prepend it to the description too.
                "description": format!("{check_name}: {description}"),
                "severity": "major",
                "fingerprint": format!("{:x}", message_fingerprint),
                "location": {
                    "path": path,
                    "positions": {
                        "begin": start_location,
                        "end": end_location,
                    },
                },
            });

            s.serialize_element(&value)?;
        }

        s.end()
    }
}

/// Generate a unique fingerprint to identify a violation.
fn fingerprint(message: &Diagnostic, project_path: &str, salt: u64) -> u64 {
    let mut hasher = DefaultHasher::new();

    salt.hash(&mut hasher);
    message.name().hash(&mut hasher);
    project_path.hash(&mut hasher);

    hasher.finish()
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::GitlabEmitter;
    use crate::message::tests::{
        capture_emitter_output, create_diagnostics, create_syntax_error_diagnostics,
    };

    #[test]
    fn output() {
        let mut emitter = GitlabEmitter::default();
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(redact_fingerprint(&content));
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = GitlabEmitter::default();
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_diagnostics());

        assert_snapshot!(redact_fingerprint(&content));
    }

    // Redact the fingerprint because the default hasher isn't stable across platforms.
    fn redact_fingerprint(content: &str) -> String {
        static FINGERPRINT_HAY_KEY: &str = r#""fingerprint": ""#;

        let mut output = String::with_capacity(content.len());
        let mut last = 0;

        for (start, _) in content.match_indices(FINGERPRINT_HAY_KEY) {
            let fingerprint_hash_start = start + FINGERPRINT_HAY_KEY.len();
            output.push_str(&content[last..fingerprint_hash_start]);
            output.push_str("<redacted>");
            last = fingerprint_hash_start
                + content[fingerprint_hash_start..]
                    .find('"')
                    .expect("Expected terminating quote");
        }

        output.push_str(&content[last..]);

        output
    }
}
