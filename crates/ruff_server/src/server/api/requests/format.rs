use crate::edit::ToRangeExt;
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
use lsp_types::{self as types, request as req};
use ruff_source_file::LineIndex;
use ruff_text_size::{TextLen, TextRange, TextSize};
use types::TextEdit;

pub(crate) struct Format;

impl super::RequestHandler for Format {
    type RequestType = req::Formatting;
}

impl super::BackgroundDocumentRequestHandler for Format {
    super::define_document_url!(params: &types::DocumentFormattingParams);
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        _params: types::DocumentFormattingParams,
    ) -> Result<super::FormatResponse> {
        let doc = snapshot.document();
        let source = doc.contents();
        let formatted = crate::format::format(doc, &snapshot.configuration().formatter)
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;
        // fast path - if the code is the same, return early
        if formatted == source {
            return Ok(None);
        }
        let formatted_index: LineIndex = LineIndex::from_source_text(&formatted);

        let unformatted_index = doc.index();

        let Replacement {
            source_range: replace_range,
            formatted_range: replacement_text_range,
        } = Replacement::between(
            source,
            unformatted_index.line_starts(),
            &formatted,
            formatted_index.line_starts(),
        );

        Ok(Some(vec![TextEdit {
            range: replace_range.to_range(source, unformatted_index, snapshot.encoding()),
            new_text: formatted[replacement_text_range].to_owned(),
        }]))
    }
}

struct Replacement {
    source_range: TextRange,
    formatted_range: TextRange,
}

impl Replacement {
    /// Creates a [`Replacement`] that describes the `replace_range` of `old_text` to replace
    /// with `new_text` sliced by `replacement_text_range`.
    fn between(
        source: &str,
        source_line_starts: &[TextSize],
        formatted: &str,
        formatted_line_starts: &[TextSize],
    ) -> Self {
        let mut source_start = TextSize::default();
        let mut formatted_start = TextSize::default();
        let mut source_end = source.text_len();
        let mut formatted_end = formatted.text_len();
        let mut line_iter = source_line_starts
            .iter()
            .copied()
            .zip(formatted_line_starts.iter().copied());
        for (source_line_start, formatted_line_start) in line_iter.by_ref() {
            if source_line_start != formatted_line_start
                || source[TextRange::new(source_start, source_line_start)]
                    != formatted[TextRange::new(formatted_start, formatted_line_start)]
            {
                break;
            }
            source_start = source_line_start;
            formatted_start = formatted_line_start;
        }

        let mut line_iter = line_iter.rev();

        for (old_line_start, new_line_start) in line_iter.by_ref() {
            if old_line_start <= source_start
                || new_line_start <= formatted_start
                || source[TextRange::new(old_line_start, source_end)]
                    != formatted[TextRange::new(new_line_start, formatted_end)]
            {
                break;
            }
            source_end = old_line_start;
            formatted_end = new_line_start;
        }

        Replacement {
            source_range: TextRange::new(source_start, source_end),
            formatted_range: TextRange::new(formatted_start, formatted_end),
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_source_file::LineIndex;

    use crate::server::api::requests::format::Replacement;

    #[test]
    fn find_replacement_range_works() {
        let original = r#"
         aaaa
         bbbb
         cccc
         dddd
         eeee
         "#;
        let original_index = LineIndex::from_source_text(original);
        let new = r#"
         bb
         cccc
         dd
         "#;
        let new_index = LineIndex::from_source_text(new);
        let expected = r#"
         bb
         cccc
         dd
         "#;
        let replacement = Replacement::between(
            original,
            original_index.line_starts(),
            new,
            new_index.line_starts(),
        );
        let mut test = original.to_string();
        test.replace_range(
            replacement.source_range.start().to_usize()..replacement.source_range.end().to_usize(),
            &new[replacement.formatted_range],
        );

        assert_eq!(expected, &test);
    }
}
