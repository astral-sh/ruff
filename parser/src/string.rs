use crate::{
    ast::{Constant, Expr, ExprKind, Location},
    error::{LexicalError, LexicalErrorType},
    string_parser::parse_string,
    token::StringKind,
};
use itertools::Itertools;

pub fn parse_strings(
    values: Vec<(Location, (String, StringKind, bool), Location)>,
) -> Result<Expr, LexicalError> {
    // Preserve the initial location and kind.
    let initial_start = values[0].0;
    let last_end = values.last().unwrap().2;
    let initial_kind = (values[0].1 .1 == StringKind::Unicode).then(|| "u".to_owned());
    let has_fstring = values.iter().any(|(_, (_, kind, ..), _)| kind.is_fstring());
    let num_bytes = values
        .iter()
        .filter(|(_, (_, kind, ..), _)| kind.is_bytes())
        .count();
    let has_bytes = num_bytes > 0;

    if has_bytes && num_bytes < values.len() {
        return Err(LexicalError {
            error: LexicalErrorType::OtherError(
                "cannot mix bytes and nonbytes literals".to_owned(),
            ),
            location: initial_start,
        });
    }

    if has_bytes {
        let mut content: Vec<u8> = vec![];
        for (start, (source, kind, triple_quoted), end) in values {
            for value in parse_string(&source, kind, triple_quoted, start, end)? {
                match value.node {
                    ExprKind::Constant {
                        value: Constant::Bytes(value),
                        ..
                    } => content.extend(value),
                    _ => unreachable!("Unexpected non-bytes expression."),
                }
            }
        }
        return Ok(Expr::new(
            initial_start,
            last_end,
            ExprKind::Constant {
                value: Constant::Bytes(content),
                kind: None,
            },
        ));
    }

    if !has_fstring {
        let mut content: Vec<String> = vec![];
        for (start, (source, kind, triple_quoted), end) in values {
            for value in parse_string(&source, kind, triple_quoted, start, end)? {
                match value.node {
                    ExprKind::Constant {
                        value: Constant::Str(value),
                        ..
                    } => content.push(value),
                    _ => unreachable!("Unexpected non-string expression."),
                }
            }
        }
        return Ok(Expr::new(
            initial_start,
            last_end,
            ExprKind::Constant {
                value: Constant::Str(content.join("")),
                kind: initial_kind,
            },
        ));
    }

    // De-duplicate adjacent constants.
    let mut deduped: Vec<Expr> = vec![];
    let mut current: Vec<String> = vec![];

    let take_current = |current: &mut Vec<String>| -> Expr {
        Expr::new(
            initial_start,
            last_end,
            ExprKind::Constant {
                value: Constant::Str(current.drain(..).join("")),
                kind: initial_kind.clone(),
            },
        )
    };

    for (start, (source, kind, triple_quoted), end) in values {
        for value in parse_string(&source, kind, triple_quoted, start, end)? {
            match value.node {
                ExprKind::FormattedValue { .. } => {
                    if !current.is_empty() {
                        deduped.push(take_current(&mut current));
                    }
                    deduped.push(value)
                }
                ExprKind::Constant {
                    value: Constant::Str(value),
                    ..
                } => current.push(value),
                _ => unreachable!("Unexpected non-string expression."),
            }
        }
    }
    if !current.is_empty() {
        deduped.push(take_current(&mut current));
    }

    Ok(if has_fstring {
        Expr::new(
            initial_start,
            last_end,
            ExprKind::JoinedStr { values: deduped },
        )
    } else {
        deduped
            .into_iter()
            .exactly_one()
            .expect("String must be concatenated to a single element.")
    })
}

#[cfg(test)]
mod tests {
    use crate::parser::parse_program;

    #[test]
    fn test_parse_string_concat() {
        let source = "'Hello ' 'world'";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_string_concat_1() {
        let source = "'Hello ' u'world'";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_string_concat_2() {
        let source = "u'Hello ' 'world'";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string_concat_1() {
        let source = "'Hello ' f'world'";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string_concat_2() {
        let source = "'Hello ' f'world'";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string_concat_3() {
        let source = "'Hello ' f'world{\"!\"}'";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_f_string_concat_1() {
        let source = "u'Hello ' f'world'";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_f_string_concat_2() {
        let source = "u'Hello ' f'world' '!'";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_string_triple_quotes_with_kind() {
        let source = "u'''Hello, world!'''";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_single_quoted_byte() {
        // single quote
        let source = r##"b'\x00\x01\x02\x03\x04\x05\x06\x07\x08\t\n\x0b\x0c\r\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f !"#$%&\'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\x7f\x80\x81\x82\x83\x84\x85\x86\x87\x88\x89\x8a\x8b\x8c\x8d\x8e\x8f\x90\x91\x92\x93\x94\x95\x96\x97\x98\x99\x9a\x9b\x9c\x9d\x9e\x9f\xa0\xa1\xa2\xa3\xa4\xa5\xa6\xa7\xa8\xa9\xaa\xab\xac\xad\xae\xaf\xb0\xb1\xb2\xb3\xb4\xb5\xb6\xb7\xb8\xb9\xba\xbb\xbc\xbd\xbe\xbf\xc0\xc1\xc2\xc3\xc4\xc5\xc6\xc7\xc8\xc9\xca\xcb\xcc\xcd\xce\xcf\xd0\xd1\xd2\xd3\xd4\xd5\xd6\xd7\xd8\xd9\xda\xdb\xdc\xdd\xde\xdf\xe0\xe1\xe2\xe3\xe4\xe5\xe6\xe7\xe8\xe9\xea\xeb\xec\xed\xee\xef\xf0\xf1\xf2\xf3\xf4\xf5\xf6\xf7\xf8\xf9\xfa\xfb\xfc\xfd\xfe\xff'"##;
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_double_quoted_byte() {
        // double quote
        let source = r##"b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\t\n\x0b\x0c\r\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\x7f\x80\x81\x82\x83\x84\x85\x86\x87\x88\x89\x8a\x8b\x8c\x8d\x8e\x8f\x90\x91\x92\x93\x94\x95\x96\x97\x98\x99\x9a\x9b\x9c\x9d\x9e\x9f\xa0\xa1\xa2\xa3\xa4\xa5\xa6\xa7\xa8\xa9\xaa\xab\xac\xad\xae\xaf\xb0\xb1\xb2\xb3\xb4\xb5\xb6\xb7\xb8\xb9\xba\xbb\xbc\xbd\xbe\xbf\xc0\xc1\xc2\xc3\xc4\xc5\xc6\xc7\xc8\xc9\xca\xcb\xcc\xcd\xce\xcf\xd0\xd1\xd2\xd3\xd4\xd5\xd6\xd7\xd8\xd9\xda\xdb\xdc\xdd\xde\xdf\xe0\xe1\xe2\xe3\xe4\xe5\xe6\xe7\xe8\xe9\xea\xeb\xec\xed\xee\xef\xf0\xf1\xf2\xf3\xf4\xf5\xf6\xf7\xf8\xf9\xfa\xfb\xfc\xfd\xfe\xff""##;
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_escape_char_in_byte_literal() {
        // backslash does not escape
        let source = r##"b"omkmok\Xaa""##;
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_raw_byte_literal_1() {
        let source = r"rb'\x1z'";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_raw_byte_literal_2() {
        let source = r"rb'\\'";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_escape_octet() {
        let source = r##"b'\43a\4\1234'"##;
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_escaped_newline() {
        let source = r#"f"\n{x}""#;
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_unescaped_newline() {
        let source = r#"f"""
{x}""""#;
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_escaped_character() {
        let source = r#"f"\\{x}""#;
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_raw_fstring() {
        let source = r#"rf"{x}""#;
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_triple_quoted_raw_fstring() {
        let source = r#"rf"""{x}""""#;
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_line_continuation() {
        let source = r#"rf"\
{x}""#;
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }
}
