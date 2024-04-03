use ruff_text_size::{TextLen, TextRange, TextSize};

pub(crate) struct Replacement {
    pub(crate) source_range: TextRange,
    pub(crate) modified_range: TextRange,
}

impl Replacement {
    /// Creates a [`Replacement`] that describes the `source_range` of `source` to replace
    /// with `modified` sliced by `modified_range`.
    pub(crate) fn between(
        source: &str,
        source_line_starts: &[TextSize],
        modified: &str,
        modified_line_starts: &[TextSize],
    ) -> Self {
        let mut source_start = TextSize::default();
        let mut replaced_start = TextSize::default();
        let mut source_end = source.text_len();
        let mut replaced_end = modified.text_len();
        let mut line_iter = source_line_starts
            .iter()
            .copied()
            .zip(modified_line_starts.iter().copied());
        for (source_line_start, modified_line_start) in line_iter.by_ref() {
            if source_line_start != modified_line_start
                || source[TextRange::new(source_start, source_line_start)]
                    != modified[TextRange::new(replaced_start, modified_line_start)]
            {
                break;
            }
            source_start = source_line_start;
            replaced_start = modified_line_start;
        }

        let mut line_iter = line_iter.rev();

        for (old_line_start, new_line_start) in line_iter.by_ref() {
            if old_line_start <= source_start
                || new_line_start <= replaced_start
                || source[TextRange::new(old_line_start, source_end)]
                    != modified[TextRange::new(new_line_start, replaced_end)]
            {
                break;
            }
            source_end = old_line_start;
            replaced_end = new_line_start;
        }

        Replacement {
            source_range: TextRange::new(source_start, source_end),
            modified_range: TextRange::new(replaced_start, replaced_end),
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_source_file::LineIndex;

    use super::Replacement;

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
            &new[replacement.modified_range],
        );

        assert_eq!(expected, &test);
    }
}
