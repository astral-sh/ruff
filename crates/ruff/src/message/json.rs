use crate::message::{Emitter, EmitterContext, Message};
use crate::registry::AsRule;
use ruff_diagnostics::Edit;
use ruff_python_ast::source_code::{SourceCode, SourceLocation};
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use serde_json::{json, Value};
use std::io::Write;

#[derive(Default)]
pub struct JsonEmitter;

impl Emitter for JsonEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        messages: &[Message],
        _context: &EmitterContext,
    ) -> anyhow::Result<()> {
        serde_json::to_writer_pretty(writer, &ExpandedMessages { messages })?;

        Ok(())
    }
}

struct ExpandedMessages<'a> {
    messages: &'a [Message],
}

impl Serialize for ExpandedMessages<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.messages.len()))?;

        for message in self.messages {
            let source_code = message.file.to_source_code();

            let fix = if message.fix.is_empty() {
                None
            } else {
                Some(json!({
                    "message": message.kind.suggestion.as_deref(),
                    "edits": &ExpandedEdits { edits: message.fix.edits(), source_code: &source_code },
                }))
            };

            let start_location = source_code.source_location(message.start());
            let end_location = source_code.source_location(message.end());
            let noqa_location = source_code.source_location(message.noqa_offset);

            let value = json!({
                "code": message.kind.rule().noqa_code().to_string(),
                "message": message.kind.body,
                "fix": fix,
                "location": start_location,
                "end_location": end_location,
                "filename": message.filename(),
                "noqa_row": noqa_location.row
            });

            s.serialize_element(&value)?;
        }

        s.end()
    }
}

struct ExpandedEdits<'a> {
    edits: &'a [Edit],
    source_code: &'a SourceCode<'a, 'a>,
}

impl Serialize for ExpandedEdits<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.edits.len()))?;

        for edit in self.edits {
            let start_location = self.source_code.source_location(edit.start());
            let end_location = self.source_code.source_location(edit.end());
            let value = json!({
                "content": edit.content().unwrap_or_default(),
                "location": to_zero_indexed_column(&start_location),
                "end_location": to_zero_indexed_column(&end_location)
            });

            s.serialize_element(&value)?;
        }

        s.end()
    }
}

fn to_zero_indexed_column(location: &SourceLocation) -> Value {
    json!({
        "row": location.row,
        "column": location.column.to_zero_indexed()
    })
}

#[cfg(test)]
mod tests {
    use crate::message::tests::{capture_emitter_output, create_messages};
    use crate::message::JsonEmitter;
    use insta::assert_snapshot;

    #[test]
    fn output() {
        let mut emitter = JsonEmitter::default();
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }
}
