use ruff_db::{files::File, parsed::parsed_module, source::source_text};
use ruff_diagnostics::Edit;
use ruff_python_ast::StringFlags;
use ruff_text_size::{Ranged, TextSize};
use ty_project::Db;

/// Returns an edit to insert closing quotes after typing a triple-quoted string opener.
///
/// For example, typing `"""` in `value = """<cursor>` produces
/// `value = """<cursor>"""`.
pub fn on_type_formatting(
    db: &dyn Db,
    file: File,
    offset: TextSize,
    trigger_character: &str,
) -> Option<Edit> {
    let quotes = match trigger_character {
        "\"" => "\"\"\"",
        "'" => "'''",
        _ => return None,
    };

    let last_quote = offset.checked_sub(TextSize::new(1))?;
    let parsed = parsed_module(db, file).load(db);
    let token = parsed.tokens().at_offset(last_quote).next()?;
    let flags = token.string_flags()?;

    if flags.quote_str() != quotes
        || !flags.is_triple_quoted()
        || token.start() + flags.opener_len() != offset
        || source_text(db, file)
            .as_str()
            .get(offset.to_usize()..)?
            .starts_with(quotes)
    {
        return None;
    }

    Some(Edit::insertion(quotes.to_string(), offset))
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_text_size::Ranged;

    use super::on_type_formatting;
    use crate::tests::{CursorTest, cursor_test};

    impl CursorTest {
        fn format_on_type(&self, trigger_character: &str) -> String {
            let Some(edit) = on_type_formatting(
                &self.db,
                self.cursor.file,
                self.cursor.offset,
                trigger_character,
            ) else {
                return "<No edit>".to_string();
            };

            let mut source = self.cursor.source.to_string();
            source.replace_range(edit.range().to_std_range(), edit.content().unwrap_or(""));
            source
        }
    }

    #[test]
    fn closes_triple_quoted_docstring() {
        let test = cursor_test(
            r#"def foo():
    """<CURSOR>
    pass"#,
        );

        assert_snapshot!(test.format_on_type("\""), @r#"
        def foo():
            """"""
            pass
        "#);
    }

    #[test]
    fn closes_raw_triple_quoted_string() {
        let test = cursor_test(
            r#"value = r"""<CURSOR>
reveal_type(value)"#,
        );
        assert_snapshot!(test.format_on_type("\""), @r#"
        value = r""""""
        reveal_type(value)
        "#);
    }

    #[test]
    fn closes_f_string() {
        let test = cursor_test(
            r#"value = f"""<CURSOR>
reveal_type(value)"#,
        );
        assert_snapshot!(test.format_on_type("\""), @r#"
        value = f""""""
        reveal_type(value)
        "#);
    }

    #[test]
    fn closes_template_string() {
        let test = cursor_test(
            r#"value = t'''<CURSOR>
reveal_type(value)"#,
        );
        assert_snapshot!(test.format_on_type("'"), @"
        value = t''''''
        reveal_type(value)
        ");
    }

    #[test]
    fn closes_single_quoted_string() {
        let test = cursor_test(
            r#"value = '''<CURSOR>
reveal_type(value)"#,
        );
        assert_snapshot!(test.format_on_type("'"), @"
        value = ''''''
        reveal_type(value)
        ");
    }

    #[test]
    fn closes_before_existing_triple_quoted_string() {
        let test = cursor_test(
            r#"value = """<CURSOR>
other = """existing""""#,
        );
        assert_snapshot!(test.format_on_type("\""), @r#"
        value = """"""
        other = """existing"""
        "#);
    }

    #[test]
    fn closes_f_string_before_existing_triple_quoted_string() {
        let test = cursor_test(
            r#"value = f"""<CURSOR>
other = """existing""""#,
        );
        assert_snapshot!(test.format_on_type("\""), @r#"
        value = f""""""
        other = """existing"""
        "#);
    }

    #[test]
    fn does_not_close_already_empty_triple_quoted_string() {
        let test = cursor_test(r#"value = """<CURSOR>""""#);
        assert_snapshot!(test.format_on_type("\""), @"<No edit>");
    }

    #[test]
    fn does_not_close_already_empty_f_string() {
        let test = cursor_test(r#"value = f"""<CURSOR>""""#);
        assert_snapshot!(test.format_on_type("\""), @"<No edit>");
    }

    #[test]
    fn does_not_close_closing_triple_quotes() {
        let test = cursor_test(
            r#"
            value = """
            content
            """<CURSOR>
            "#,
        );
        assert_snapshot!(test.format_on_type("\""), @"<No edit>");
    }

    #[test]
    fn does_not_close_escaped_quote_sequence() {
        let test = cursor_test(r#"value = """\"""<CURSOR>"#);
        assert_snapshot!(test.format_on_type("\""), @"<No edit>");
    }

    #[test]
    fn ignores_other_trigger_characters() {
        let test = cursor_test(r#"value = """<CURSOR>"#);
        assert_snapshot!(test.format_on_type("."), @"<No edit>");
    }
}
