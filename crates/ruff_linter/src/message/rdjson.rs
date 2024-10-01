use std::io::Write;

use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use serde_json::{json, Value};

use ruff_diagnostics::Edit;
use ruff_source_file::SourceCode;
use ruff_text_size::Ranged;

use crate::message::{Emitter, EmitterContext, Message, SourceLocation};

#[derive(Default)]
pub struct RdjsonEmitter;

impl Emitter for RdjsonEmitter {
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

fn message_to_rdjson_value(message: &Message) -> Value {
    let source_code = message.source_file().to_source_code();

    let start_location = source_code.source_location(message.start());
    let end_location = source_code.source_location(message.end());

    if let Some(fix) = message.fix() {
        json!({
            "message": message.body(),
            "location": {
                "path": message.filename(),
                "range": rdjson_range(&start_location, &end_location),
            },
            "code": {
                "value": message.rule().map(|rule| rule.noqa_code().to_string()),
                "url": message.rule().and_then(|rule| rule.url()),
            },
            "suggestions": rdjson_suggestions(fix.edits(), &source_code),
        })
    } else {
        json!({
            "message": message.body(),
            "location": {
                "path": message.filename(),
                "range": rdjson_range(&start_location, &end_location),
            },
            "code": {
                "value": message.rule().map(|rule| rule.noqa_code().to_string()),
                "url": message.rule().and_then(|rule| rule.url()),
            },
        })
    }
}

fn rdjson_suggestions(edits: &[Edit], source_code: &SourceCode) -> Value {
    Value::Array(
        edits
            .iter()
            .map(|edit| {
                let location = source_code.source_location(edit.start());
                let end_location = source_code.source_location(edit.end());

                json!({
                    "range": rdjson_range(&location, &end_location),
                    "text": edit.content().unwrap_or_default(),
                })
            })
            .collect(),
    )
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

    use crate::message::tests::{
        capture_emitter_output, create_messages, create_syntax_error_messages,
    };
    use crate::message::RdjsonEmitter;

    #[test]
    fn output() {
        let mut emitter = RdjsonEmitter;
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = RdjsonEmitter;
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_messages());

        assert_snapshot!(content);
    }
}
