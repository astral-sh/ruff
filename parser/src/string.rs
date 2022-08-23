use crate::{
    ast::{Constant, Expr, ExprKind, Location},
    error::{LexicalError, LexicalErrorType},
    fstring::parse_located_fstring,
    token::StringKind,
};
use itertools::Itertools;

pub fn parse_strings(values: Vec<(Location, (String, StringKind))>) -> Result<Expr, LexicalError> {
    // Preserve the initial location and kind.
    let initial_location = values[0].0;
    let initial_kind = (values[0].1 .1 == StringKind::U).then(|| "u".to_owned());

    // Determine whether the list of values contains any f-strings. (If not, we can return a
    // single Constant at the end, rather than a JoinedStr.)
    let mut has_fstring = false;

    // De-duplicate adjacent constants.
    let mut deduped: Vec<Expr> = vec![];
    let mut current: Vec<String> = vec![];

    let take_current = |current: &mut Vec<String>| -> Expr {
        Expr::new(
            initial_location,
            ExprKind::Constant {
                value: Constant::Str(current.drain(..).join("")),
                kind: initial_kind.clone(),
            },
        )
    };

    for (location, (string, string_kind)) in values {
        match string_kind {
            StringKind::Normal | StringKind::U => current.push(string),
            StringKind::F => {
                has_fstring = true;
                for value in parse_located_fstring(&string, location).map_err(|e| LexicalError {
                    location,
                    error: LexicalErrorType::FStringError(e.error),
                })? {
                    match value.node {
                        ExprKind::FormattedValue { .. } => {
                            if !current.is_empty() {
                                deduped.push(take_current(&mut current));
                            }
                            deduped.push(value)
                        }
                        ExprKind::Constant { value, .. } => {
                            if let Constant::Str(value) = value {
                                current.push(value);
                            } else {
                                unreachable!("Unexpected non-string constant.");
                            }
                        }
                        _ => unreachable!("Unexpected non-string expression."),
                    }
                }
            }
        }
    }
    if !current.is_empty() {
        deduped.push(take_current(&mut current));
    }

    Ok(if has_fstring {
        Expr::new(initial_location, ExprKind::JoinedStr { values: deduped })
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
        let source = String::from("'Hello ' 'world'");
        let parse_ast = parse_program(&source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_string_concat_1() {
        let source = String::from("'Hello ' u'world'");
        let parse_ast = parse_program(&source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_string_concat_2() {
        let source = String::from("u'Hello ' 'world'");
        let parse_ast = parse_program(&source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string_concat_1() {
        let source = String::from("'Hello ' f'world'");
        let parse_ast = parse_program(&source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string_concat_2() {
        let source = String::from("'Hello ' f'world'");
        let parse_ast = parse_program(&source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string_concat_3() {
        let source = String::from("'Hello ' f'world{\"!\"}'");
        let parse_ast = parse_program(&source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_f_string_concat_1() {
        let source = String::from("u'Hello ' f'world'");
        let parse_ast = parse_program(&source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_f_string_concat_2() {
        let source = String::from("u'Hello ' f'world' '!'");
        let parse_ast = parse_program(&source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }
}
