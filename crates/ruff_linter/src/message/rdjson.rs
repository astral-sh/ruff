use std::io::Write;

use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use serde_json::{Value, json};

use ruff_source_file::SourceCode;
use ruff_text_size::Ranged;

use crate::Edit;
use crate::message::{Emitter, EmitterContext, LineColumn, OldDiagnostic};

#[derive(Default)]
pub struct RdjsonEmitter;

impl Emitter for RdjsonEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[OldDiagnostic],
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
                "diagnostics": &ExpandedMessages{ diagnostics }
            }),
        )?;

        Ok(())
    }
}

struct ExpandedMessages<'a> {
    diagnostics: &'a [OldDiagnostic],
}

impl Serialize for ExpandedMessages<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.diagnostics.len()))?;

        for message in self.diagnostics {
            let value = message_to_rdjson_value(message);
            s.serialize_element(&value)?;
        }

        s.end()
    }
}

fn message_to_rdjson_value(message: &OldDiagnostic) -> Value {
    let source_file = message.source_file();
    let source_code = source_file.to_source_code();

    let start_location = source_code.line_column(message.start());
    let end_location = source_code.line_column(message.end());

    if let Some(fix) = message.fix() {
        json!({
            "message": message.body(),
            "location": {
                "path": message.filename(),
                "range": rdjson_range(start_location, end_location),
            },
            "code": {
                "value": message.secondary_code(),
                "url": message.to_url(),
            },
            "suggestions": rdjson_suggestions(fix.edits(), &source_code),
        })
    } else {
        json!({
            "message": message.body(),
            "location": {
                "path": message.filename(),
                "range": rdjson_range(start_location, end_location),
            },
            "code": {
                "value": message.secondary_code(),
                "url": message.to_url(),
            },
        })
    }
}

fn rdjson_suggestions(edits: &[Edit], source_code: &SourceCode) -> Value {
    Value::Array(
        edits
            .iter()
            .map(|edit| {
                let location = source_code.line_column(edit.start());
                let end_location = source_code.line_column(edit.end());

                json!({
                    "range": rdjson_range(location, end_location),
                    "text": edit.content().unwrap_or_default(),
                })
            })
            .collect(),
    )
}

fn rdjson_range(start: LineColumn, end: LineColumn) -> Value {
    json!({
        "start": start,
        "end": end,
    })
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::RdjsonEmitter;
    use crate::message::tests::{
        capture_emitter_output, create_diagnostics, create_syntax_error_diagnostics,
    };

    #[test]
    fn output() {
        let mut emitter = RdjsonEmitter;
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = RdjsonEmitter;
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_diagnostics());

        assert_snapshot!(content);
    }
}
