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

    let fix = message.fix.as_ref().map(|fix| ExpandedEdits {
        edits: fix.edits(),
        source_code: &source_code,
    });

    let start_location = source_code.source_location(message.start());
    let end_location = source_code.source_location(message.end());

    json!({
        "message": message.kind.body,
        "location": {
            "path": message.filename(),
            "range": rdjson_range(start_location, end_location),
        },
        "code": {
            "value": message.kind.rule().noqa_code().to_string(),
            "url": message.kind.rule().url(),
        },
        "suggestions": fix,
    })
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
            let location = self.source_code.source_location(edit.start());
            let end_location = self.source_code.source_location(edit.end());

            s.serialize_element(&json!({
                "range": rdjson_range(location, end_location),
                "text": edit.content().unwrap_or_default(),
            }))?;
        }

        s.end()
    }
}

fn rdjson_range(start: SourceLocation, end: SourceLocation) -> Value {
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
