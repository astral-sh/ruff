#[cfg(test)]
mod tests {
    use crate::{format_module_source_range, LspRowColumn, PyFormatOptions};
    use indoc::indoc;
    use insta::assert_snapshot;

    fn format(source: &str, start: (usize, usize), end: (usize, usize)) -> String {
        format_module_source_range(
            source,
            PyFormatOptions::default(),
            Some(LspRowColumn {
                row: start.0,
                col: start.1,
            }),
            Some(LspRowColumn {
                row: end.0,
                col: end.1,
            }),
        )
        .unwrap()
    }

    #[test]
    fn test_top_level() {
        assert_snapshot!(format(indoc! {r#"
        a = [1,]
        b = [1,]
        c = [1,]
        d = [1,]
        "#}, (1, 3), (2, 5)), @r###"
        a = [1,]
        b = [
                1,
            ]
            c = [
                1,
            ]
        d = [1,]
        "###);
    }

    #[test]
    fn test_easy_nested() {
        assert_snapshot!(format(indoc! {r#"
        a = [1,]
        for i in range( 1 ):
            b = [1,]
            c = [1,]
            d = [1,]
        e = [1,]
        "#}, (3, 3), (3, 5)), @r###"
        a = [1,]
        for i in range(1):
                b = [
                    1,
                ]
                c = [
                    1,
                ]
                d = [
                    1,
                ]

        e = [1,]
        "###);
    }

    #[test]
    fn test_if() {
        let source = indoc! {r#"
        import     random
        if random.random()    <    0.5:
            a = [1,]
            b = [1,]
        elif random.random()    <    0.75:
            c = [1,]
            d = [1,]
        else:
            e = [1,]
            f = [1,]
        g = [1,]
        "#};

        assert_snapshot!(format(source, (3, 0), (3, 10)), @r###"
        import     random
        if random.random() < 0.5:
                a = [
                    1,
                ]
                b = [
                    1,
                ]
            elif random.random() < 0.75:
                c = [
                    1,
                ]
                d = [
                    1,
                ]
            else:
                e = [
                    1,
                ]
                f = [
                    1,
                ]

        g = [1,]
        "###);
        assert_snapshot!(format(source, (6, 0), (6, 10)), @r###"
        import     random
        if random.random() < 0.5:
                a = [
                    1,
                ]
                b = [
                    1,
                ]
            elif random.random() < 0.75:
                c = [
                    1,
                ]
                d = [
                    1,
                ]
            else:
                e = [
                    1,
                ]
                f = [
                    1,
                ]

        g = [1,]
        "###);
        assert_snapshot!(format(source, (9, 0), (9, 10)), @r###"
        import     random
        if random.random() < 0.5:
                a = [
                    1,
                ]
                b = [
                    1,
                ]
            elif random.random() < 0.75:
                c = [
                    1,
                ]
                d = [
                    1,
                ]
            else:
                e = [
                    1,
                ]
                f = [
                    1,
                ]

        g = [1,]
        "###);
        assert_snapshot!(format(source, (3, 0), (6, 10)), @r###"
        import     random
        if random.random() < 0.5:
                a = [
                    1,
                ]
                b = [
                    1,
                ]
            elif random.random() < 0.75:
                c = [
                    1,
                ]
                d = [
                    1,
                ]
            else:
                e = [
                    1,
                ]
                f = [
                    1,
                ]

        g = [1,]
        "###);
    }

    // TODO
    #[test]
    fn test_trailing_comment() {
        assert_snapshot!(format(indoc! {r#"
        if True:
            a = [1,]
            # trailing comment
        "#}, (1, 3), (2, 5)), @r###"
        if True:
                a = [
                    1,
                ]

            # trailing comment
        "###);
    }

    // TODO
    #[test]
    fn test_alternative_indent() {
        assert_snapshot!(format(indoc! {r#"
        if True:
          a = [1,]
          b = [1,]
          c = [1,]
        "#}, (1, 3), (2, 5)), @r###"
        if True:
          a = [
                1,
            ]
            b = [
                1,
            ]
          c = [1,]
        "###);
    }
}
