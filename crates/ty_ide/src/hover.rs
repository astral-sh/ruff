use crate::goto::{find_goto_target, GotoTarget};
use crate::{Db, MarkupKind, RangedValue};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextSize};
use std::fmt;
use std::fmt::Formatter;
use ty_python_semantic::types::Type;
use ty_python_semantic::SemanticModel;

pub fn hover(db: &dyn Db, file: File, offset: TextSize) -> Option<RangedValue<Hover>> {
    let parsed = parsed_module(db.upcast(), file);
    let goto_target = find_goto_target(parsed, offset)?;

    if let GotoTarget::Expression(expr) = goto_target {
        if expr.is_literal_expr() {
            return None;
        }
    }

    let model = SemanticModel::new(db.upcast(), file);
    let ty = goto_target.inferred_type(&model)?;

    tracing::debug!(
        "Inferred type of covering node is {}",
        ty.display(db.upcast())
    );

    // TODO: Add documentation of the symbol (not the type's definition).
    // TODO: Render the symbol's signature instead of just its type.
    let contents = vec![HoverContent::Type(ty)];

    Some(RangedValue {
        range: FileRange::new(file, goto_target.range()),
        value: Hover { contents },
    })
}

pub struct Hover<'db> {
    contents: Vec<HoverContent<'db>>,
}

impl<'db> Hover<'db> {
    /// Renders the hover to a string using the specified markup kind.
    pub const fn display<'a>(&'a self, db: &'a dyn Db, kind: MarkupKind) -> DisplayHover<'a> {
        DisplayHover {
            db,
            hover: self,
            kind,
        }
    }

    fn iter(&self) -> std::slice::Iter<'_, HoverContent<'db>> {
        self.contents.iter()
    }
}

impl<'db> IntoIterator for Hover<'db> {
    type Item = HoverContent<'db>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.contents.into_iter()
    }
}

impl<'a, 'db> IntoIterator for &'a Hover<'db> {
    type Item = &'a HoverContent<'db>;
    type IntoIter = std::slice::Iter<'a, HoverContent<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct DisplayHover<'a> {
    db: &'a dyn Db,
    hover: &'a Hover<'a>,
    kind: MarkupKind,
}

impl fmt::Display for DisplayHover<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for content in &self.hover.contents {
            if !first {
                self.kind.horizontal_line().fmt(f)?;
            }

            content.display(self.db, self.kind).fmt(f)?;
            first = false;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum HoverContent<'db> {
    Type(Type<'db>),
}

impl<'db> HoverContent<'db> {
    fn display(&self, db: &'db dyn Db, kind: MarkupKind) -> DisplayHoverContent<'_, 'db> {
        DisplayHoverContent {
            db,
            content: self,
            kind,
        }
    }
}

pub(crate) struct DisplayHoverContent<'a, 'db> {
    db: &'db dyn Db,
    content: &'a HoverContent<'db>,
    kind: MarkupKind,
}

impl fmt::Display for DisplayHoverContent<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.content {
            HoverContent::Type(ty) => self
                .kind
                .fenced_code_block(ty.display(self.db.upcast()), "text")
                .fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::{cursor_test, CursorTest};
    use crate::{hover, MarkupKind};
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticFormat, DiagnosticId, DisplayDiagnosticConfig, LintName,
        Severity, Span,
    };
    use ruff_db::Upcast;
    use ruff_text_size::{Ranged, TextRange};

    #[test]
    fn hover_basic() {
        let test = cursor_test(
            r#"
        a = 10

        a<CURSOR>
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        Literal[10]
        ---------------------------------------------
        ```text
        Literal[10]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:9
          |
        2 |         a = 10
        3 |
        4 |         a
          |         ^- Cursor offset
          |         |
          |         source
          |
        ");
    }

    #[test]
    fn hover_member() {
        let test = cursor_test(
            r#"
        class Foo:
            a: int = 10

            def __init__(a: int, b: str):
                self.a = a
                self.b: str = b

        foo = Foo()
        foo.<CURSOR>a
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        int
        ---------------------------------------------
        ```text
        int
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:10:9
           |
         9 |         foo = Foo()
        10 |         foo.a
           |         ^^^^-
           |         |   |
           |         |   Cursor offset
           |         source
           |
        ");
    }

    #[test]
    fn hover_function_typed_variable() {
        let test = cursor_test(
            r#"
            def foo(a, b): ...

            foo<CURSOR>
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        def foo(a, b) -> Unknown
        ---------------------------------------------
        ```text
        def foo(a, b) -> Unknown
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:13
          |
        2 |             def foo(a, b): ...
        3 |
        4 |             foo
          |             ^^^- Cursor offset
          |             |
          |             source
          |
        ");
    }

    #[test]
    fn hover_binary_expression() {
        let test = cursor_test(
            r#"
            def foo(a: int, b: int, c: int):
                a + b ==<CURSOR> c
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        bool
        ---------------------------------------------
        ```text
        bool
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:3:17
          |
        2 |             def foo(a: int, b: int, c: int):
        3 |                 a + b == c
          |                 ^^^^^^^^-^
          |                 |       |
          |                 |       Cursor offset
          |                 source
          |
        ");
    }

    #[test]
    fn hover_keyword_parameter() {
        let test = cursor_test(
            r#"
            def test(a: int): ...

            test(a<CURSOR>= 123)
            "#,
        );

        // TODO: This should reveal `int` because the user hovers over the parameter and not the value.
        assert_snapshot!(test.hover(), @r"
        Literal[123]
        ---------------------------------------------
        ```text
        Literal[123]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:18
          |
        2 |             def test(a: int): ...
        3 |
        4 |             test(a= 123)
          |                  ^- Cursor offset
          |                  |
          |                  source
          |
        ");
    }

    #[test]
    fn hover_union() {
        let test = cursor_test(
            r#"

            def foo(a, b): ...

            def bar(a, b): ...

            if random.choice([True, False]):
                a = foo
            else:
                a = bar

            a<CURSOR>
            "#,
        );

        assert_snapshot!(test.hover(), @r"
        (def foo(a, b) -> Unknown) | (def bar(a, b) -> Unknown)
        ---------------------------------------------
        ```text
        (def foo(a, b) -> Unknown) | (def bar(a, b) -> Unknown)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:12:13
           |
        10 |                 a = bar
        11 |
        12 |             a
           |             ^- Cursor offset
           |             |
           |             source
           |
        ");
    }

    #[test]
    fn hover_module() {
        let mut test = cursor_test(
            r#"
            import lib

            li<CURSOR>b
            "#,
        );

        test.write_file("lib.py", "a = 10").unwrap();

        assert_snapshot!(test.hover(), @r"
        <module 'lib'>
        ---------------------------------------------
        ```text
        <module 'lib'>
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:13
          |
        2 |             import lib
        3 |
        4 |             lib
          |             ^^-
          |             | |
          |             | Cursor offset
          |             source
          |
        ");
    }

    #[test]
    fn hover_type_of_expression_with_type_var_type() {
        let test = cursor_test(
            r#"
            type Alias[T: int = bool] = list[T<CURSOR>]
            "#,
        );

        assert_snapshot!(test.hover(), @r"
        T
        ---------------------------------------------
        ```text
        T
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:46
          |
        2 |             type Alias[T: int = bool] = list[T]
          |                                              ^- Cursor offset
          |                                              |
          |                                              source
          |
        ");
    }

    #[test]
    fn hover_type_of_expression_with_type_param_spec() {
        let test = cursor_test(
            r#"
            type Alias[**P = [int, str]] = Callable[P<CURSOR>, int]
            "#,
        );

        assert_snapshot!(test.hover(), @r"
        @Todo
        ---------------------------------------------
        ```text
        @Todo
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:53
          |
        2 |             type Alias[**P = [int, str]] = Callable[P, int]
          |                                                     ^- Cursor offset
          |                                                     |
          |                                                     source
          |
        ");
    }

    #[test]
    fn hover_type_of_expression_with_type_var_tuple() {
        let test = cursor_test(
            r#"
            type Alias[*Ts = ()] = tuple[*Ts<CURSOR>]
            "#,
        );

        assert_snapshot!(test.hover(), @r"
        @Todo
        ---------------------------------------------
        ```text
        @Todo
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:43
          |
        2 |             type Alias[*Ts = ()] = tuple[*Ts]
          |                                           ^^- Cursor offset
          |                                           |
          |                                           source
          |
        ");
    }

    #[test]
    fn hover_variable_assignment() {
        let test = cursor_test(
            r#"
            value<CURSOR> = 1
            "#,
        );

        assert_snapshot!(test.hover(), @r"
        Literal[1]
        ---------------------------------------------
        ```text
        Literal[1]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:13
          |
        2 |             value = 1
          |             ^^^^^- Cursor offset
          |             |
          |             source
          |
        ");
    }

    #[test]
    fn hover_augmented_assignment() {
        let test = cursor_test(
            r#"
            value = 1
            value<CURSOR> += 2
            "#,
        );

        // We currently show the *previous* value of the variable (1), not the new one (3).
        // Showing the new value might be more intuitive for some users, but the actual 'use'
        // of the `value` symbol here in read-context is `1`. This comment mainly exists to
        // signal that it might be okay to revisit this in the future and reveal 3 instead.
        assert_snapshot!(test.hover(), @r"
        Literal[1]
        ---------------------------------------------
        ```text
        Literal[1]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:3:13
          |
        2 |             value = 1
        3 |             value += 2
          |             ^^^^^- Cursor offset
          |             |
          |             source
          |
        ");
    }

    #[test]
    fn hover_attribute_assignment() {
        let test = cursor_test(
            r#"
            class C:
                attr: int = 1

            C.attr<CURSOR> = 2
            "#,
        );

        assert_snapshot!(test.hover(), @r"
        Literal[2]
        ---------------------------------------------
        ```text
        Literal[2]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:5:13
          |
        3 |                 attr: int = 1
        4 |
        5 |             C.attr = 2
          |             ^^^^^^- Cursor offset
          |             |
          |             source
          |
        ");
    }

    #[test]
    fn hover_augmented_attribute_assignment() {
        let test = cursor_test(
            r#"
            class C:
                attr = 1

            C.attr<CURSOR> += 2
            "#,
        );

        // See the comment in the `hover_augmented_assignment` test above. The same
        // reasoning applies here.
        assert_snapshot!(test.hover(), @r"
        Unknown | Literal[1]
        ---------------------------------------------
        ```text
        Unknown | Literal[1]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:5:13
          |
        3 |                 attr = 1
        4 |
        5 |             C.attr += 2
          |             ^^^^^^- Cursor offset
          |             |
          |             source
          |
        ");
    }

    #[test]
    fn hover_annotated_assignment() {
        let test = cursor_test(
            r#"
        class Foo:
            a<CURSOR>: int
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        int
        ---------------------------------------------
        ```text
        int
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:3:13
          |
        2 |         class Foo:
        3 |             a: int
          |             ^- Cursor offset
          |             |
          |             source
          |
        ");
    }

    #[test]
    fn hover_annotated_assignment_with_rhs() {
        let test = cursor_test(
            r#"
        class Foo:
            a<CURSOR>: int = 1
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        Literal[1]
        ---------------------------------------------
        ```text
        Literal[1]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:3:13
          |
        2 |         class Foo:
        3 |             a: int = 1
          |             ^- Cursor offset
          |             |
          |             source
          |
        ");
    }

    #[test]
    fn hover_annotated_attribute_assignment() {
        let test = cursor_test(
            r#"
        class Foo:
            def __init__(self, a: int):
                self.a<CURSOR>: int = a
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        int
        ---------------------------------------------
        ```text
        int
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:17
          |
        2 |         class Foo:
        3 |             def __init__(self, a: int):
        4 |                 self.a: int = a
          |                 ^^^^^^- Cursor offset
          |                 |
          |                 source
          |
        ");
    }

    #[test]
    fn hover_type_narrowing() {
        let test = cursor_test(
            r#"
            def foo(a: str | None, b):
                if a is not None:
                    print(a<CURSOR>)
            "#,
        );

        assert_snapshot!(test.hover(), @r"
        str
        ---------------------------------------------
        ```text
        str
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:27
          |
        2 |             def foo(a: str | None, b):
        3 |                 if a is not None:
        4 |                     print(a)
          |                           ^- Cursor offset
          |                           |
          |                           source
          |
        ");
    }

    #[test]
    fn hover_whitespace() {
        let test = cursor_test(
            r#"
        class C:
            <CURSOR>
            foo: str = 'bar'
        "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_literal_int() {
        let test = cursor_test(
            r#"
        print(
            0 + 1<CURSOR>
        )
        "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_literal_ellipsis() {
        let test = cursor_test(
            r#"
        print(
            .<CURSOR>..
        )
        "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_docstring() {
        let test = cursor_test(
            r#"
        def f():
            """Lorem ipsum dolor sit amet.<CURSOR>"""
        "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    impl CursorTest {
        fn hover(&self) -> String {
            use std::fmt::Write;

            let Some(hover) = hover(&self.db, self.file, self.cursor_offset) else {
                return "Hover provided no content".to_string();
            };

            let source = hover.range;

            let mut buf = String::new();

            write!(
                &mut buf,
                "{plaintext}{line}{markdown}{line}",
                plaintext = hover.display(&self.db, MarkupKind::PlainText),
                line = MarkupKind::PlainText.horizontal_line(),
                markdown = hover.display(&self.db, MarkupKind::Markdown),
            )
            .unwrap();

            let config = DisplayDiagnosticConfig::default()
                .color(false)
                .format(DiagnosticFormat::Full);

            let mut diagnostic = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("hover")),
                Severity::Info,
                "Hovered content is",
            );
            diagnostic.annotate(
                Annotation::primary(Span::from(source.file()).with_range(source.range()))
                    .message("source"),
            );
            diagnostic.annotate(
                Annotation::secondary(
                    Span::from(source.file()).with_range(TextRange::empty(self.cursor_offset)),
                )
                .message("Cursor offset"),
            );

            write!(buf, "{}", diagnostic.display(&self.db.upcast(), &config)).unwrap();

            buf
        }
    }
}
