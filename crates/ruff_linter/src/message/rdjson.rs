use std::io::Write;

use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use serde_json::{json, Value};

use ruff_diagnostics::Edit;
use ruff_source_file::SourceCode;
use ruff_text_size::Ranged;

use crate::message::{Emitter, EmitterContext, Message, SourceLocation};
use crate::registry::AsRule;

#[derive(Default)]
pub struct RDJsonEmitter;

impl Emitter for RDJsonEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        messages: &[Message],
        _context: &EmitterContext,
    ) -> anyhow::Result<()> {
        serde_json::to_writer_pretty(
            writer,
            &json!({
                "source": {
                    "name": "ruff",
                    "url": "https://docs.astral.sh/ruff",
                },
                "severity": "warning",
                "diagnostics": &ExpandedMessages{ messages }
            }),
        )?;

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
            let value = message_to_rdjson_value(message);
            s.serialize_element(&value)?;
        }

        s.end()
    }
}

pub(crate) fn message_to_rdjson_value(message: &Message) -> Value {
    let source_code = message.file.to_source_code();

    let start_location = source_code.source_location(message.start());
    let end_location = source_code.source_location(message.end());

    let mut result = json!({
        "message": message.kind.body,
        "location": {
            "path": message.filename(),
            "range": rdjson_range(&start_location, &end_location),
        },
        "code": {
            "value": message.kind.rule().noqa_code().to_string(),
            "url": message.kind.rule().url(),
        },
    })
    .as_object()
    .unwrap()
    .clone();

    if let Some(fix) = message.fix.as_ref() {
        result.insert(
            "suggestions".into(),
            rdjson_suggestions(fix.edits(), &source_code),
        );
    };

    Value::Object(result)
}

fn rdjson_suggestions(edits: &[Edit], source_code: &SourceCode) -> Value {
    let mut suggestions: Vec<Value> = vec![];

    for edit in edits {
        let location = source_code.source_location(edit.start());
        let end_location = source_code.source_location(edit.end());

        suggestions.push(json!({"range": rdjson_range(&location, &end_location), "text": edit.content().unwrap_or_default()}));
    }

    Value::Array(suggestions)
}

fn rdjson_range(start: &SourceLocation, end: &SourceLocation) -> Value {
    json!({
        "start": {
            "line": start.row,
            "column": start.column,
        },
        "end": {
            "line": end.row,
            "column": end.column,
        },
    })
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::tests::{capture_emitter_output, create_messages};
    use crate::message::RDJsonEmitter;

    #[test]
    fn output() {
        let mut emitter = RDJsonEmitter;
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }
}
