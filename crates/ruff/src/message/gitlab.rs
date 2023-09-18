use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::io::Write;

use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use serde_json::json;

use crate::fs::{relativize_path, relativize_path_to};
use crate::message::{Emitter, EmitterContext, Message};
use crate::registry::AsRule;

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
        messages: &[Message],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        serde_json::to_writer_pretty(
            writer,
            &SerializedMessages {
                messages,
                context,
                project_dir: self.project_dir.as_deref(),
            },
        )?;

        Ok(())
    }
}

struct SerializedMessages<'a> {
    messages: &'a [Message],
    context: &'a EmitterContext<'a>,
    project_dir: Option<&'a str>,
}

impl Serialize for SerializedMessages<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.messages.len()))?;
        let mut fingerprints = HashSet::<u64>::with_capacity(self.messages.len());

        for message in self.messages {
            let start_location = message.compute_start_location();
            let end_location = message.compute_end_location();

            let lines = if self.context.is_notebook(message.filename()) {
                // We can't give a reasonable location for the structured formats,
                // so we show one that's clearly a fallback
                json!({
                    "begin": 1,
                    "end": 1
                })
            } else {
                json!({
                    "begin": start_location.row,
                    "end": end_location.row
                })
            };

            let path = self.project_dir.as_ref().map_or_else(
                || relativize_path(message.filename()),
                |project_dir| relativize_path_to(message.filename(), project_dir),
            );

            let mut message_fingerprint = fingerprint(message, 0);

            // Make sure that we do not get a fingerprint that is already in use
            // by adding in the previously generated one.
            while fingerprints.contains(&message_fingerprint) {
                message_fingerprint = fingerprint(message, message_fingerprint);
            }
            fingerprints.insert(message_fingerprint);

            let value = json!({
                "description": format!("({}) {}", message.kind.rule().noqa_code(), message.kind.body),
                "severity": "major",
                "fingerprint": format!("{:x}", message_fingerprint),
                "location": {
                    "path": path,
                    "lines": lines
                }
            });

            s.serialize_element(&value)?;
        }

        s.end()
    }
}

/// Generate a unique fingerprint to identify a violation.
fn fingerprint(message: &Message, salt: u64) -> u64 {
    let Message {
        kind,
        range: _,
        fix: _fix,
        file,
        noqa_offset: _,
    } = message;

    let mut hasher = DefaultHasher::new();

    salt.hash(&mut hasher);
    kind.name.hash(&mut hasher);
    file.name().hash(&mut hasher);

    hasher.finish()
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::tests::{capture_emitter_output, create_messages};
    use crate::message::GitlabEmitter;

    #[test]
    fn output() {
        let mut emitter = GitlabEmitter::default();
        let content = capture_emitter_output(&mut emitter, &create_messages());

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
