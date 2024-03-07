use crate::edit::ToRangeExt;
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
use lsp_types::{self as types, request as req};
use ruff_source_file::LineIndex;
use ruff_text_size::{TextLen, TextRange, TextSize};
use types::TextEdit;

pub(crate) struct Format;

impl super::Request for Format {
    type RequestType = req::Formatting;
}

impl super::BackgroundDocumentRequest for Format {
    super::define_document_url!(params: &types::DocumentFormattingParams);
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        _params: types::DocumentFormattingParams,
    ) -> Result<super::FormatResponse> {
        let doc = snapshot.document();
        let formatted = crate::format::format(doc, &snapshot.configuration().formatter)
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;
        let formatted_index: LineIndex = LineIndex::from_source_text(&formatted);

        let unformatted = doc.contents();
        let unformatted_index = doc.index();

        let Replacement {
            replace_range,
            replacement_text_range,
        } = find_replacement_range(
            unformatted,
            unformatted_index.line_starts(),
            &formatted,
            formatted_index.line_starts(),
        );

        Ok(Some(vec![TextEdit {
            range: replace_range.to_range(unformatted, unformatted_index, snapshot.encoding()),
            new_text: formatted[replacement_text_range].to_owned(),
        }]))
    }
}

struct Replacement {
    replace_range: TextRange,
    replacement_text_range: TextRange,
}

/// Returns a [`Replacement`] that describes the `replace_range` of `old_text` to replace
/// with `new_text` sliced by `replacement_text_range`.
fn find_replacement_range(
    old_text: &str,
    old_text_line_starts: &[TextSize],
    new_text: &str,
    new_text_line_starts: &[TextSize],
) -> Replacement {
    let mut old_start = TextSize::default();
    let mut new_start = TextSize::default();
    let mut old_end = old_text.text_len();
    let mut new_end = new_text.text_len();
    for (old_line_start, new_line_start) in old_text_line_starts
        .iter()
        .copied()
        .zip(new_text_line_starts.iter().copied())
    {
        if old_line_start != new_line_start
            || old_text[TextRange::new(old_start, old_line_start)]
                != new_text[TextRange::new(new_start, new_line_start)]
        {
            break;
        }
        old_start = old_line_start;
        new_start = new_line_start;
    }

    for (old_line_start, new_line_start) in old_text_line_starts
        .iter()
        .rev()
        .copied()
        .zip(new_text_line_starts.iter().rev().copied())
    {
        if old_line_start <= old_start
            || new_line_start <= new_start
            || old_text[TextRange::new(old_line_start, old_end)]
                != new_text[TextRange::new(new_line_start, new_end)]
        {
            break;
        }
        old_end = old_line_start;
        new_end = new_line_start;
    }

    Replacement {
        replace_range: TextRange::new(old_start, old_end),
        replacement_text_range: TextRange::new(new_start, new_end),
    }
}

#[cfg(test)]
mod tests {
    use ruff_source_file::LineIndex;

    use crate::server::api::requests::format::find_replacement_range;

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
        let replacement = find_replacement_range(
            original,
            original_index.line_starts(),
            new,
            new_index.line_starts(),
        );
        let mut test = original.to_string();
        test.replace_range(
            replacement.replace_range.start().to_usize()
                ..replacement.replace_range.end().to_usize(),
            &new[replacement.replacement_text_range],
        );
        assert_eq!(expected, &test);
    }
}
