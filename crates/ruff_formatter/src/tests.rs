use std::cell::Cell;

use ruff_text_size::TextSize;

use crate::prelude::*;
use crate::{
    FormatState, Formatted, IndentStyle, RemoveSoftLinesBuffer, SimpleFormatOptions, SourceMarker,
    VecBuffer, format, format_args, write,
};

fn print_at(width: u16, content: &dyn Format<SimpleFormatContext>) -> String {
    let context = SimpleFormatContext::new(SimpleFormatOptions {
        line_width: width.try_into().unwrap(),
        ..SimpleFormatOptions::default()
    });
    format!(context, [content])
        .unwrap()
        .print()
        .unwrap()
        .into_code()
}

fn assert_layouts(content: &dyn Format<SimpleFormatContext>, cases: &[(u16, &str)]) {
    for &(width, expected) in cases {
        assert_eq!(print_at(width, content), expected, "line width {width}");
    }
}

#[test]
fn writing_to_a_buffer() {
    let mut state = FormatState::new(SimpleFormatContext::default());
    let mut buffer = VecBuffer::new(&mut state);
    buffer.write_element(FormatElement::Token { text: "a" });
    buffer.write_fmt(format_args!(space(), token("b"))).unwrap();
    write!(buffer, [format_args!(space(), token("c"))]).unwrap();
    dbg_write!(buffer, [token("!")]).unwrap();

    assert_eq!(
        buffer.elements(),
        [
            FormatElement::Token { text: "a" },
            FormatElement::Space,
            FormatElement::Token { text: "b" },
            FormatElement::Space,
            FormatElement::Token { text: "c" },
            FormatElement::Token { text: "!" },
        ]
    );
    let formatted = Formatted::new(
        Document::from(buffer.into_vec()),
        SimpleFormatContext::default(),
    );
    assert_eq!(formatted.print().unwrap().as_code(), "a b c!");
}

#[test]
fn record_nested_writes() {
    let content = format_with(|f| {
        let mut recording = f.start_recording();
        write!(recording, [token("A")])?;
        write!(
            recording,
            [format_with(|f| write!(f, [token("B"), token("C")]))]
        )?;
        assert_eq!(
            &*recording.stop(),
            [
                FormatElement::Token { text: "A" },
                FormatElement::Token { text: "B" },
                FormatElement::Token { text: "C" },
            ]
        );
        Ok(())
    });
    assert_eq!(print_at(80, &content), "ABC");
}

#[test]
fn remove_soft_lines_from_buffer() {
    let formatted = format!(
        SimpleFormatContext::default(),
        [format_with(|f| {
            let mut buffer = RemoveSoftLinesBuffer::new(f);
            write!(
                buffer,
                [
                    token("a"),
                    soft_line_break_or_space(),
                    token("b"),
                    soft_line_break(),
                    token("c")
                ]
            )
        })]
    )
    .unwrap();
    assert_eq!(
        formatted.into_document(),
        Document::from(vec![
            FormatElement::Token { text: "a" },
            FormatElement::Space,
            FormatElement::Token { text: "b" },
            FormatElement::Token { text: "c" },
        ])
    );
}

#[test]
fn tokens_and_source_positions() {
    let formatted = format!(
        SimpleFormatContext::default(),
        [
            source_position(TextSize::new(0)),
            token("\"Hello "),
            source_position(TextSize::new(8)),
            token("'Ruff'"),
            source_position(TextSize::new(14)),
            token("\""),
            source_position(TextSize::new(20)),
        ]
    )
    .unwrap();
    let printed = formatted.print().unwrap();
    assert_eq!(printed.as_code(), r#""Hello 'Ruff'""#);
    assert_eq!(
        printed.sourcemap(),
        [(0, 0), (8, 7), (14, 13), (20, 14)].map(|(source, dest)| SourceMarker {
            source: TextSize::new(source),
            dest: TextSize::new(dest)
        })
    );
    assert_eq!(
        print_at(80, &token(r#""Hello\tWorld""#)),
        r#""Hello\tWorld""#
    );
}

#[test]
fn group_line_breaks() {
    for (line, flat) in [
        (soft_line_break(), "a,b"),
        (soft_line_break_or_space(), "a, b"),
    ] {
        let content = format_with(|f| group(&format_args![token("a,"), line, token("b")]).fmt(f));
        assert_eq!(print_at(10, &content), flat);
        assert_eq!(print_at(2, &content), "a,\nb");
    }
    for (line, expected) in [
        (hard_line_break(), "a,\nb\n"),
        (empty_line(), "a,\n\nb\n\n"),
    ] {
        assert_eq!(
            print_at(
                80,
                &group(&format_args![token("a,"), line, token("b"), line])
            ),
            expected
        );
    }
}

#[test]
fn line_suffix_boundary_flushes_pending_suffix() {
    assert_eq!(
        print_at(
            80,
            &format_args![token("a"), line_suffix(&token("c"), 0), token("b")]
        ),
        "abc"
    );
    assert_eq!(
        print_at(
            80,
            &format_args![
                token("a"),
                line_suffix(&token("c"), 0),
                token("b"),
                line_suffix_boundary(),
                token("d")
            ]
        ),
        "abc\nd"
    );
}

#[test]
fn labels_survive_recording() {
    struct Main;
    impl tag::LabelDefinition for Main {
        fn value(&self) -> u64 {
            0
        }
        fn name(&self) -> &'static str {
            "Main"
        }
    }
    let content = format_with(|f| {
        let mut recording = f.start_recording();
        write!(recording, [labelled(LabelId::of(Main), &token("content"))])?;
        assert!(recording.stop()[0].has_label(LabelId::of(Main)));
        Ok(())
    });
    assert_eq!(print_at(80, &content), "content");
}

#[test]
fn block_indent_and_nested_indent() {
    let content = format_with(|f| {
        format_args![
            token("{"),
            block_indent(&format_args![
                token("case:"),
                indent(&format_args![hard_line_break(), token("break;")]),
                hard_line_break(),
                token("next"),
            ]),
            token("}")
        ]
        .fmt(f)
    });
    assert_eq!(print_at(80, &content), "{\n\tcase:\n\t\tbreak;\n\tnext\n}");
}

#[test]
fn dedent_and_restore_indent() {
    let line = |word| format_with(move |f| write!(f, [hard_line_break(), token(word)]));
    let content = format_with(|f| {
        format_args![
            token("root"),
            align(
                2,
                &format_args![
                    line("aligned"),
                    dedent(&line("not aligned")),
                    dedent(&indent(&line("indented"))),
                ]
            ),
            dedent(&line("still root")),
            indent(&format_args![
                line("first"),
                indent(&format_args![
                    line("second"),
                    align(
                        2,
                        &format_args![line("aligned"), dedent_to_root(&line("root"))]
                    ),
                    line("second"),
                ])
            ])
        ]
        .fmt(f)
    });
    assert_eq!(
        print_at(80, &content),
        "root\n  aligned\nnot aligned\n\tindented\nstill root\n\tfirst\n\t\tsecond\n\t\t  aligned\nroot\n\t\tsecond"
    );
}

#[test]
fn align_inside_tab_and_space_indentation() {
    for (style, expected) in [
        (IndentStyle::Tab, "? {\n\t\tbody\n  }"),
        (IndentStyle::Space, "? {\n      body\n  }"),
    ] {
        let context = SimpleFormatContext::new(SimpleFormatOptions {
            indent_style: style,
            indent_width: 4.try_into().unwrap(),
            ..SimpleFormatOptions::default()
        });
        let formatted = format!(
            context,
            [
                token("? "),
                align(
                    2,
                    &format_args![token("{"), block_indent(&token("body")), token("}"),]
                )
            ]
        )
        .unwrap();
        assert_eq!(formatted.print().unwrap().as_code(), expected);
    }
}

#[test]
fn soft_indentation_when_a_group_fits_or_breaks() {
    assert_layouts(
        &group(&format_args![
            token("["),
            soft_block_indent(&format_args![
                token("1,"),
                soft_line_break_or_space(),
                token("2")
            ]),
            token("]"),
        ]),
        &[(80, "[1, 2]"), (4, "[\n\t1,\n\t2\n]")],
    );
    assert_layouts(
        &group(&format_args![
            token("name ="),
            soft_line_indent_or_space(&token("value"))
        ]),
        &[(80, "name = value"), (4, "name =\n\tvalue")],
    );
    assert_layouts(
        &group(&format_args![
            token("{"),
            soft_space_or_block_indent(&token("a: 5")),
            token("}")
        ]),
        &[(80, "{ a: 5 }"), (4, "{\n\ta: 5\n}")],
    );
}

#[test]
fn contents_depend_on_enclosing_group() {
    assert_layouts(
        &group(&format_args![
            token("["),
            soft_block_indent(&format_args![
                token("1,"),
                soft_line_break_or_space(),
                token("2"),
                if_group_breaks(&token(",")),
                if_group_fits_on_line(&token(";")),
            ]),
            token("]"),
        ]),
        &[(80, "[1, 2;]"), (4, "[\n\t1,\n\t2,\n]")],
    );
}

#[test]
fn indent_depends_on_referenced_group() {
    assert_layouts(
        &format_with(|f| {
            let id = f.group_id("header");
            write!(
                f,
                [
                    group(&token("header")).with_id(Some(id)),
                    indent_if_group_breaks(&format_args![hard_line_break(), token("body")], id)
                ]
            )
        }),
        &[(4, "header\n\tbody"), (80, "header\nbody")],
    );
}

#[test]
fn fill_trailing_comma_depends_on_outer_group() {
    let content = format_with(|f| {
        let id = f.group_id("array");
        group(&format_args![
            token("["),
            soft_block_indent(&format_with(|f| {
                f.fill()
                    .entry(&soft_line_break_or_space(), &token("1,"))
                    .entry(&soft_line_break_or_space(), &token("234568789,"))
                    .entry(&soft_line_break_or_space(), &token("3456789,"))
                    .entry(
                        &soft_line_break_or_space(),
                        &format_args![
                            token("["),
                            soft_block_indent(&token("4")),
                            token("]"),
                            if_group_breaks(&token(",")).with_group_id(Some(id))
                        ],
                    )
                    .finish()
            })),
            token("]")
        ])
        .with_id(Some(id))
        .fmt(f)
    });
    assert_eq!(
        print_at(20, &content),
        "[\n\t1, 234568789,\n\t3456789, [4],\n]"
    );
}

#[test]
fn parenthesize_only_if_it_helps_fit() {
    let value = token("value");
    let specialized =
        format_with(|f| format_args![token("name = "), best_fit_parenthesize(&value)].fmt(f));
    let alternatives = format_with(|f| {
        format_args![
            token("name = "),
            best_fitting![
                group(&value),
                group(&format_args![
                    token("("),
                    soft_block_indent(&value),
                    token(")")
                ])
                .should_expand(true),
                group(&value).should_expand(true),
            ]
            .with_mode(BestFittingMode::AllLines)
        ]
        .fmt(f)
    });
    for (width, expected) in [
        (15, "name = value"),
        (9, "name = (\n\tvalue\n)"),
        (5, "name = value"),
    ] {
        assert_eq!(print_at(width, &specialized), expected);
        assert_eq!(print_at(width, &alternatives), expected);
    }
}

#[test]
fn fits_expanded_allows_an_expanded_child_in_a_flat_parent() {
    let content = format_with(|f| {
        group(&format_args![
            token("a"),
            soft_line_break_or_space(),
            token("+ "),
            fits_expanded(&group(&format_args![
                token("["),
                soft_block_indent(&format_args![
                    token("b,"),
                    expand_parent(),
                    soft_line_break_or_space(),
                    token("'a long string'"),
                ]),
                token("]")
            ]))
        ])
        .fmt(f)
    });
    assert_eq!(print_at(12, &content), "a + [\n\tb,\n\t'a long string'\n]");
}

#[test]
fn conditional_groups_expand_only_when_the_parent_breaks() {
    let content = format_with(|f| {
        let id = f.group_id("parentheses");
        let list = format_with(|f| {
            write!(
                f,
                [
                    token("["),
                    soft_block_indent(&format_args![
                        token("'Good morning!',"),
                        soft_line_break_or_space(),
                        token("'How are you?'")
                    ]),
                    token("]")
                ]
            )
        });
        let expression = format_with(|f| {
            conditional_group(
                &format_args![
                    token("'aaaaaaa'"),
                    soft_line_break_or_space(),
                    token("+ "),
                    fits_expanded(&conditional_group(
                        &list,
                        tag::Condition::if_group_fits_on_line(id)
                    )),
                    soft_line_break_or_space(),
                    token("+ "),
                    conditional_group(
                        &format_args![
                            token("'bbbb'"),
                            soft_line_break_or_space(),
                            token("and 'c'")
                        ],
                        tag::Condition::if_group_fits_on_line(id),
                    ),
                ],
                tag::Condition::if_breaks(),
            )
            .fmt(f)
        });
        group(&format_args![
            if_group_breaks(&token("(")),
            indent_if_group_breaks(&format_args![soft_line_break(), expression], id),
            soft_line_break(),
            if_group_breaks(&token(")")),
        ])
        .with_id(Some(id))
        .fmt(f)
    });
    assert_layouts(
        &content,
        &[
            (
                65,
                "'aaaaaaa' + ['Good morning!', 'How are you?'] + 'bbbb' and 'c'",
            ),
            (
                21,
                "'aaaaaaa' + [\n\t'Good morning!',\n\t'How are you?'\n] + 'bbbb' and 'c'",
            ),
            (
                11,
                "(\n\t'aaaaaaa'\n\t+ [\n\t\t'Good morning!',\n\t\t'How are you?'\n\t]\n\t+ 'bbbb'\n\tand 'c'\n)",
            ),
        ],
    );
}

#[test]
fn best_fitting_tries_alternatives_in_order() {
    assert_layouts(
        &format_args![
            token("call"),
            best_fitting![
                token("([1, 2])"),
                format_args![token("(["), block_indent(&token("1, 2")), token("])")],
                format_args![token("("), block_indent(&token("[1, 2]")), token(")")],
            ],
        ],
        &[
            (20, "call([1, 2])"),
            (6, "call([\n\t1, 2\n])"),
            (5, "call(\n\t[1, 2]\n)"),
        ],
    );
}

#[test]
fn best_fitting_can_select_an_expanded_group() {
    let code = print_at(
        80,
        &best_fitting![
            format_args![
                token("call("),
                group(&format_args![
                    token("["),
                    soft_block_indent(&token("1")),
                    token("]")
                ])
                .should_expand(true),
                token(")")
            ],
            token("fallback"),
        ],
    );
    assert_eq!(code, "call([\n\t1\n])");
}

#[test]
fn best_fitting_can_measure_every_line() {
    assert_layouts(
        &best_fitting![
            token("[1, 2] + name"),
            format_args![
                token("["),
                group(&block_indent(&token("1, 2"))).should_expand(true),
                token("] + name"),
            ],
            format_args![
                token("("),
                block_indent(&format_args![
                    token("[1, 2]"),
                    hard_line_break(),
                    token("+ name")
                ]),
                token(")")
            ],
        ]
        .with_mode(BestFittingMode::AllLines),
        &[
            (15, "[1, 2] + name"),
            (8, "[\n\t1, 2\n] + name"),
            (7, "(\n\t[1, 2]\n\t+ name\n)"),
        ],
    );
}

#[test]
fn join_entries_with_and_without_separators() {
    struct Words<'a>(&'a [&'a str]);
    impl Format<SimpleFormatContext> for Words<'_> {
        fn fmt(&self, f: &mut Formatter<SimpleFormatContext>) -> FormatResult<()> {
            f.join_with(&format_args![token(","), space()])
                .entries(self.0.iter().map(|word| text(word)))
                .finish()
        }
    }
    let content = format_with(|f| {
        f.join()
            .entry(&token("("))
            .entry(&Words(&["a", "b", "c"]))
            .entry(&token(")"))
            .finish()
    });
    assert_eq!(print_at(80, &content), "(a, b, c)");
}

#[test]
fn fill_entries_can_have_an_empty_separator() {
    let entries = [
        token("<b>"),
        token("unbreakable text"),
        token("<em>"),
        token("!"),
    ];
    let content = format_with(|f| f.fill().entries(&soft_line_break(), &entries).finish());
    assert_eq!(print_at(4, &content), "<b>\nunbreakable text\n<em>\n!");
}

#[test]
fn format_once_can_consume_an_iterator() {
    let mut values = ["1", "2", "3", "4"].into_iter().map(token);
    let first = values.next();
    let rest = format_once(|f| f.join_with(&hard_line_break()).entries(values).finish());
    assert_eq!(
        print_at(80, &format_args![first, block_indent(&rest)]),
        "1\n\t2\n\t3\n\t4\n"
    );
}

#[test]
#[should_panic(expected = "Tried to format a `format_once` at least twice")]
fn format_once_cannot_be_reused() {
    let value = format_once(|f| write!(f, [token("value")]));
    assert_eq!(print_at(80, &value), "value");
    print_at(80, &value);
}

#[test]
fn memoizing_and_inspecting_format_only_once() {
    let count = Cell::new(0);
    let counter = format_with(|f| {
        let current = count.get();
        count.set(current + 1);
        write!(f, [text(&current.to_string()), hard_line_break()])
    });
    assert_eq!(print_at(80, &format_args![counter, counter]), "0\n1\n");
    let memoized = counter.memoized();
    let content = format_with(|f| {
        assert!(memoized.inspect(f)?.will_break());
        write!(f, [token("Counter:"), block_indent(&memoized), memoized])
    });
    assert_eq!(print_at(80, &content), "Counter:\n\t2\n2\n");
    assert_eq!(count.get(), 3);
}
