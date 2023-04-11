use crate::fs::{relativize_path, relativize_path_to};
use crate::message::{Emitter, EmitterContext, Message};
use crate::registry::AsRule;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use serde_json::json;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write;

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

        for message in self.messages {
            let lines = if self.context.is_jupyter_notebook(message.filename()) {
                // We can't give a reasonable location for the structured formats,
                // so we show one that's clearly a fallback
                json!({
                    "begin": 1,
                    "end": 1
                })
            } else {
                json!({
                    "begin": message.location.row(),
                    "end": message.end_location.row()
                })
            };

            let path = self.project_dir.as_ref().map_or_else(
                || relativize_path(message.filename()),
                |project_dir| relativize_path_to(message.filename(), project_dir),
            );

            let value = json!({
                "description": format!("({}) {}", message.kind.rule().noqa_code(), message.kind.body),
                "severity": "major",
                "fingerprint": fingerprint(message),
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
fn fingerprint(message: &Message) -> String {
    let Message {
        kind,
        location,
        end_location,
        fix: _fix,
        file,
        noqa_row: _noqa_row,
    } = message;

    let mut hasher = DefaultHasher::new();

    kind.rule().hash(&mut hasher);
    location.row().hash(&mut hasher);
    location.column().hash(&mut hasher);
    end_location.row().hash(&mut hasher);
    end_location.column().hash(&mut hasher);
    file.name().hash(&mut hasher);

    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use crate::message::tests::{capture_emitter_output, create_messages};
    use crate::message::GitlabEmitter;
    use insta::assert_snapshot;

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
