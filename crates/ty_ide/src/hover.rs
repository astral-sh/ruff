use crate::docstring::Docstring;
use crate::goto::{GotoTarget, find_goto_target};
use crate::{Db, MarkupKind, RangedValue};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextSize};
use std::fmt;
use std::fmt::Formatter;
use ty_python_semantic::SemanticModel;
use ty_python_semantic::types::Type;

pub fn hover(db: &dyn Db, file: File, offset: TextSize) -> Option<RangedValue<Hover<'_>>> {
    let parsed = parsed_module(db, file).load(db);
    let goto_target = find_goto_target(&parsed, offset)?;

    if let GotoTarget::Expression(expr) = goto_target {
        if expr.is_literal_expr() {
            return None;
        }
    }

    let model = SemanticModel::new(db, file);
    let ty = goto_target.inferred_type(&model)?;
    let docs = goto_target
        .get_definition_targets(
            file,
            db,
            ty_python_semantic::ImportAliasResolution::ResolveAliases,
        )
        .and_then(|definitions| definitions.docstring(db));
    tracing::debug!("Inferred type of covering node is {}", ty.display(db));

    // TODO: Render the symbol's signature instead of just its type.
    let contents = vec![HoverContent::Type(ty, docs)];

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
    Type(Type<'db>, Option<Docstring>),
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
            HoverContent::Type(ty, docstring) => {
                self.kind
                    .fenced_code_block(ty.display(self.db), "python")
                    .fmt(f)?;

                if let Some(docstring) = docstring {
                    self.kind.horizontal_line().fmt(f)?;

                    match self.kind {
                        MarkupKind::PlainText => docstring.render_plaintext().fmt(f)?,
                        MarkupKind::Markdown => docstring.render_markdown().fmt(f)?,
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::{CursorTest, cursor_test};
    use crate::{MarkupKind, hover};
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticFormat, DiagnosticId, DisplayDiagnosticConfig, LintName,
        Severity, Span,
    };
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
        ```python
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
    fn hover_function() {
        let test = cursor_test(
            r#"
        def my_func(a, b):
            '''This is such a great func!!

            Args:
                a: first for a reason
                b: coming for `a`'s title
            '''
            return 0

        my_fu<CURSOR>nc(1, 2)
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        def my_func(a, b) -> Unknown
        ---------------------------------------------
        This is such a great func!!

        Args:
            a: first for a reason
            b: coming for `a`'s title

        ---------------------------------------------
        ```python
        def my_func(a, b) -> Unknown
        ```
        ---
        ```text
        This is such a great func!!

        Args:
            a: first for a reason
            b: coming for `a`'s title

        ```
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:11:9
           |
         9 |             return 0
        10 |
        11 |         my_func(1, 2)
           |         ^^^^^-^
           |         |    |
           |         |    Cursor offset
           |         source
           |
        ");
    }

    #[test]
    fn hover_class() {
        let test = cursor_test(
            r#"
        class MyClass:
            '''
                This is such a great class!!

                    Don't you know?
                
                Everyone loves my class!!

            '''
            def __init__(self, val):
                """initializes MyClass (perfectly)"""
                self.val = val
            
            def my_method(self, a, b):
                '''This is such a great func!!

                Args:
                    a: first for a reason
                    b: coming for `a`'s title
                '''
                return 0

        MyCla<CURSOR>ss
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        <class 'MyClass'>
        ---------------------------------------------
        This is such a great class!!

            Don't you know?

        Everyone loves my class!!

        ---------------------------------------------
        ```python
        <class 'MyClass'>
        ```
        ---
        ```text
        This is such a great class!!

            Don't you know?

        Everyone loves my class!!

        ```
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:24:9
           |
        22 |                 return 0
        23 |
        24 |         MyClass
           |         ^^^^^-^
           |         |    |
           |         |    Cursor offset
           |         source
           |
        ");
    }

    #[test]
    fn hover_class_init() {
        let test = cursor_test(
            r#"
        class MyClass:
            '''
                This is such a great class!!

                    Don't you know?
                
                Everyone loves my class!!

            '''
            def __init__(self, val):
                """initializes MyClass (perfectly)"""
                self.val = val
            
            def my_method(self, a, b):
                '''This is such a great func!!

                Args:
                    a: first for a reason
                    b: coming for `a`'s title
                '''
                return 0

        x = MyCla<CURSOR>ss(0)
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        <class 'MyClass'>
        ---------------------------------------------
        This is such a great class!!

            Don't you know?

        Everyone loves my class!!

        ---------------------------------------------
        ```python
        <class 'MyClass'>
        ```
        ---
        ```text
        This is such a great class!!

            Don't you know?

        Everyone loves my class!!

        ```
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:24:13
           |
        22 |                 return 0
        23 |
        24 |         x = MyClass(0)
           |             ^^^^^-^
           |             |    |
           |             |    Cursor offset
           |             source
           |
        ");
    }

    #[test]
    fn hover_class_method() {
        let test = cursor_test(
            r#"
        class MyClass:
            '''
                This is such a great class!!

                    Don't you know?
                
                Everyone loves my class!!

            '''
            def __init__(self, val):
                """initializes MyClass (perfectly)"""
                self.val = val
            
            def my_method(self, a, b):
                '''This is such a great func!!

                Args:
                    a: first for a reason
                    b: coming for `a`'s title
                '''
                return 0

        x = MyClass(0)
        x.my_me<CURSOR>thod(2, 3)
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        bound method MyClass.my_method(a, b) -> Unknown
        ---------------------------------------------
        This is such a great func!!

        Args:
            a: first for a reason
            b: coming for `a`'s title

        ---------------------------------------------
        ```python
        bound method MyClass.my_method(a, b) -> Unknown
        ```
        ---
        ```text
        This is such a great func!!

        Args:
            a: first for a reason
            b: coming for `a`'s title

        ```
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:25:9
           |
        24 |         x = MyClass(0)
        25 |         x.my_method(2, 3)
           |         ^^^^^^^-^^^
           |         |      |
           |         |      Cursor offset
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
        ```python
        int
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:10:13
           |
         9 |         foo = Foo()
        10 |         foo.a
           |             -
           |             |
           |             source
           |             Cursor offset
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
        ```python
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
        ```python
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
            def test(ab: int):
                """my cool test

                Args:
                    ab: a nice little integer
                """
                return 0

            test(a<CURSOR>b= 123)
            "#,
        );

        // TODO: This should reveal `int` because the user hovers over the parameter and not the value.
        assert_snapshot!(test.hover(), @r"
        Literal[123]
        ---------------------------------------------
        ```python
        Literal[123]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:10:18
           |
         8 |                 return 0
         9 |
        10 |             test(ab= 123)
           |                  ^-
           |                  ||
           |                  |Cursor offset
           |                  source
           |
        ");
    }

    #[test]
    fn hover_union() {
        let test = cursor_test(
            r#"

            def foo(a, b):
                """The foo function"""
                return 0

            def bar(a, b):
                """The bar function"""
                return 1

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
        ```python
        (def foo(a, b) -> Unknown) | (def bar(a, b) -> Unknown)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:16:13
           |
        14 |                 a = bar
        15 |
        16 |             a
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

        test.write_file(
            "lib.py",
            r"
        '''
        The cool lib_py module!

        Wow this module rocks.
        '''
        a = 10
        ",
        )
        .unwrap();

        assert_snapshot!(test.hover(), @r"
        <module 'lib'>
        ---------------------------------------------
        ```python
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
    fn hover_module_import() {
        let mut test = cursor_test(
            r#"
            import li<CURSOR>b

            lib
            "#,
        );

        test.write_file(
            "lib.py",
            r"
        '''
        The cool lib_py module!

        Wow this module rocks.
        '''
        a = 10
        ",
        )
        .unwrap();

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_type_of_expression_with_type_var_type() {
        let test = cursor_test(
            r#"
            type Alias[T: int = bool] = list[T<CURSOR>]
            "#,
        );

        // TODO: This should render T@Alias once we create GenericContexts for type alias scopes.
        assert_snapshot!(test.hover(), @r#"
        typing.TypeVar("T", bound=int, default=bool)
        ---------------------------------------------
        ```python
        typing.TypeVar("T", bound=int, default=bool)
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
        "#);
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
        ```python
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
        ```python
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
        ```python
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
        ```python
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
        ```python
        Literal[2]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:5:15
          |
        3 |                 attr: int = 1
        4 |
        5 |             C.attr = 2
          |               ^^^^- Cursor offset
          |               |
          |               source
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
        ```python
        Unknown | Literal[1]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:5:15
          |
        3 |                 attr = 1
        4 |
        5 |             C.attr += 2
          |               ^^^^- Cursor offset
          |               |
          |               source
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
        ```python
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
        ```python
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
        ```python
        int
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:22
          |
        2 |         class Foo:
        3 |             def __init__(self, a: int):
        4 |                 self.a: int = a
          |                      ^- Cursor offset
          |                      |
          |                      source
          |
        ");
    }

    #[test]
    fn hover_type_narrowing() {
        let test = cursor_test(
            r#"
            def foo(a: str | None, b):
                '''
                    My cool func
                
                    Args:
                        a: hopefully a string, right?!   
                '''
                if a is not None:
                    print(a<CURSOR>)
            "#,
        );

        assert_snapshot!(test.hover(), @r"
        str
        ---------------------------------------------
        ```python
        str
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:10:27
           |
         8 |                 '''
         9 |                 if a is not None:
        10 |                     print(a)
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

            let Some(hover) = hover(&self.db, self.cursor.file, self.cursor.offset) else {
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
                    Span::from(source.file()).with_range(TextRange::empty(self.cursor.offset)),
                )
                .message("Cursor offset"),
            );

            write!(buf, "{}", diagnostic.display(&self.db, &config)).unwrap();

            buf
        }
    }
}
