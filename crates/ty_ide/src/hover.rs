use crate::docstring::Docstring;
use crate::goto::{GotoTarget, find_goto_target};
use crate::{Db, MarkupKind, RangedValue};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextSize};
use std::fmt;
use std::fmt::Formatter;
use ty_python_semantic::types::{KnownInstanceType, Type, TypeVarVariance};
use ty_python_semantic::{DisplaySettings, SemanticModel};

pub fn hover(db: &dyn Db, file: File, offset: TextSize) -> Option<RangedValue<Hover<'_>>> {
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &parsed, offset)?;

    if let GotoTarget::Expression(expr) = goto_target {
        if expr.is_literal_expr() {
            return None;
        }
    }

    let docs = goto_target
        .get_definition_targets(
            &model,
            ty_python_semantic::ImportAliasResolution::ResolveAliases,
        )
        .and_then(|definitions| definitions.docstring(db))
        .map(HoverContent::Docstring);

    let mut contents = Vec::new();
    if let Some(signature) = goto_target.call_type_simplified_by_overloads(&model) {
        contents.push(HoverContent::Signature(signature));
    } else if let Some(ty) = goto_target.inferred_type(&model) {
        tracing::debug!("Inferred type of covering node is {}", ty.display(db));
        contents.push(match ty {
            Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) => typevar
                .bind_pep695(db)
                .map_or(HoverContent::Type(ty, None), |typevar| {
                    HoverContent::Type(Type::TypeVar(typevar), Some(typevar.variance(db)))
                }),
            Type::TypeVar(typevar) => HoverContent::Type(ty, Some(typevar.variance(db))),
            _ => HoverContent::Type(ty, None),
        });
    }
    contents.extend(docs);

    if contents.is_empty() {
        return None;
    }

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
    pub const fn display<'a>(&'a self, db: &'db dyn Db, kind: MarkupKind) -> DisplayHover<'db, 'a> {
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

pub struct DisplayHover<'db, 'a> {
    db: &'db dyn Db,
    hover: &'a Hover<'db>,
    kind: MarkupKind,
}

impl fmt::Display for DisplayHover<'_, '_> {
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

#[derive(Debug, Clone)]
pub enum HoverContent<'db> {
    Signature(String),
    Type(Type<'db>, Option<TypeVarVariance>),
    Docstring(Docstring),
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
            HoverContent::Signature(signature) => {
                self.kind.fenced_code_block(&signature, "python").fmt(f)
            }
            HoverContent::Type(ty, variance) => {
                let variance = match variance {
                    Some(TypeVarVariance::Covariant) => " (covariant)",
                    Some(TypeVarVariance::Contravariant) => " (contravariant)",
                    Some(TypeVarVariance::Invariant) => " (invariant)",
                    Some(TypeVarVariance::Bivariant) => " (bivariant)",
                    None => "",
                };

                // Special types like `<special-form of whatever 'blahblah' with 'florps'>`
                // render poorly with python syntax-highlighting but well as xml
                let ty_string = ty
                    .display_with(self.db, DisplaySettings::default().multiline())
                    .to_string();
                let syntax = if ty_string.starts_with('<') {
                    "xml"
                } else {
                    "python"
                };
                self.kind
                    .fenced_code_block(format!("{ty_string}{variance}"), syntax)
                    .fmt(f)
            }
            HoverContent::Docstring(docstring) => docstring.render(self.kind).fmt(f),
        }
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
        """This is the docs for this value

        Wow these are good docs!
        """

        a<CURSOR>
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        Literal[10]
        ---------------------------------------------
        This is the docs for this value

        Wow these are good docs!

        ---------------------------------------------
        ```python
        Literal[10]
        ```
        ---
        This is the docs for this value  
          
        Wow these are good docs!
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:8:1
          |
        6 | """
        7 |
        8 | a
          | ^- Cursor offset
          | |
          | source
          |
        "#);
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
        def my_func(
            a,
            b
        ) -> Unknown
        ---------------------------------------------
        This is such a great func!!

        Args:
            a: first for a reason
            b: coming for `a`'s title

        ---------------------------------------------
        ```python
        def my_func(
            a,
            b
        ) -> Unknown
        ```
        ---
        This is such a great func!!  
          
        Args:  
        &nbsp;&nbsp;&nbsp;&nbsp;a: first for a reason  
        &nbsp;&nbsp;&nbsp;&nbsp;b: coming for `a`'s title
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:11:1
           |
         9 |     return 0
        10 |
        11 | my_func(1, 2)
           | ^^^^^-^
           | |    |
           | |    Cursor offset
           | source
           |
        ");
    }

    #[test]
    fn hover_function_def() {
        let test = cursor_test(
            r#"
        def my_fu<CURSOR>nc(a, b):
            '''This is such a great func!!

            Args:
                a: first for a reason
                b: coming for `a`'s title
            '''
            return 0
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        def my_func(
            a,
            b
        ) -> Unknown
        ---------------------------------------------
        This is such a great func!!

        Args:
            a: first for a reason
            b: coming for `a`'s title

        ---------------------------------------------
        ```python
        def my_func(
            a,
            b
        ) -> Unknown
        ```
        ---
        This is such a great func!!  
          
        Args:  
        &nbsp;&nbsp;&nbsp;&nbsp;a: first for a reason  
        &nbsp;&nbsp;&nbsp;&nbsp;b: coming for `a`'s title
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:5
          |
        2 | def my_func(a, b):
          |     ^^^^^-^
          |     |    |
          |     |    Cursor offset
          |     source
        3 |     '''This is such a great func!!
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
        ```xml
        <class 'MyClass'>
        ```
        ---
        This is such a great class!!  
          
        &nbsp;&nbsp;&nbsp;&nbsp;Don't you know?  
          
        Everyone loves my class!!
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:24:1
           |
        22 |         return 0
        23 |
        24 | MyClass
           | ^^^^^-^
           | |    |
           | |    Cursor offset
           | source
           |
        ");
    }

    #[test]
    fn hover_class_def() {
        let test = cursor_test(
            r#"
        class MyCla<CURSOR>ss:
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
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        <class 'MyClass'>
        ---------------------------------------------
        This is such a great class!!

            Don't you know?

        Everyone loves my class!!

        ---------------------------------------------
        ```xml
        <class 'MyClass'>
        ```
        ---
        This is such a great class!!  
          
        &nbsp;&nbsp;&nbsp;&nbsp;Don't you know?  
          
        Everyone loves my class!!
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^-^
          |       |    |
          |       |    Cursor offset
          |       source
        3 |     '''
        4 |         This is such a great class!!
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
        initializes MyClass (perfectly)

        ---------------------------------------------
        ```xml
        <class 'MyClass'>
        ```
        ---
        initializes MyClass (perfectly)
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:24:5
           |
        22 |         return 0
        23 |
        24 | x = MyClass(0)
           |     ^^^^^-^
           |     |    |
           |     |    Cursor offset
           |     source
           |
        ");
    }

    #[test]
    fn hover_class_init_attr() {
        let test = CursorTest::builder()
            .source(
                "mymod.py",
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
        "#,
            )
            .source(
                "main.py",
                r#"
        import mymod

        x = mymod.MyCla<CURSOR>ss(0)
        "#,
            )
            .build();

        assert_snapshot!(test.hover(), @r"
        <class 'MyClass'>
        ---------------------------------------------
        initializes MyClass (perfectly)

        ---------------------------------------------
        ```xml
        <class 'MyClass'>
        ```
        ---
        initializes MyClass (perfectly)
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:11
          |
        2 | import mymod
        3 |
        4 | x = mymod.MyClass(0)
          |           ^^^^^-^
          |           |    |
          |           |    Cursor offset
          |           source
          |
        ");
    }

    #[test]
    fn hover_class_init_no_init_docs() {
        let test = cursor_test(
            r#"
        class MyClass:
            '''
                This is such a great class!!

                    Don't you know?

                Everyone loves my class!!

            '''
            def __init__(self, val):
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
        ```xml
        <class 'MyClass'>
        ```
        ---
        This is such a great class!!  
          
        &nbsp;&nbsp;&nbsp;&nbsp;Don't you know?  
          
        Everyone loves my class!!
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:23:5
           |
        21 |         return 0
        22 |
        23 | x = MyClass(0)
           |     ^^^^^-^
           |     |    |
           |     |    Cursor offset
           |     source
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
        bound method MyClass.my_method(
            a,
            b
        ) -> Unknown
        ---------------------------------------------
        This is such a great func!!

        Args:
            a: first for a reason
            b: coming for `a`'s title

        ---------------------------------------------
        ```python
        bound method MyClass.my_method(
            a,
            b
        ) -> Unknown
        ```
        ---
        This is such a great func!!  
          
        Args:  
        &nbsp;&nbsp;&nbsp;&nbsp;a: first for a reason  
        &nbsp;&nbsp;&nbsp;&nbsp;b: coming for `a`'s title
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:25:3
           |
        24 | x = MyClass(0)
        25 | x.my_method(2, 3)
           |   ^^^^^-^^^
           |   |    |
           |   |    Cursor offset
           |   source
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
                """This is the docs for this value

                Wow these are good docs!
                """
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
          --> main.py:14:5
           |
        13 | foo = Foo()
        14 | foo.a
           |     -
           |     |
           |     source
           |     Cursor offset
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
        def foo(
            a,
            b
        ) -> Unknown
        ---------------------------------------------
        ```python
        def foo(
            a,
            b
        ) -> Unknown
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:1
          |
        2 | def foo(a, b): ...
        3 |
        4 | foo
          | ^^^- Cursor offset
          | |
          | source
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
         --> main.py:3:5
          |
        2 | def foo(a: int, b: int, c: int):
        3 |     a + b == c
          |     ^^^^^^^^-^
          |     |       |
          |     |       Cursor offset
          |     source
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
          --> main.py:10:6
           |
         8 |     return 0
         9 |
        10 | test(ab= 123)
           |      ^-
           |      ||
           |      |Cursor offset
           |      source
           |
        ");
    }

    #[test]
    fn hover_keyword_parameter_def() {
        let test = cursor_test(
            r#"
            def test(a<CURSOR>b: int):
                """my cool test

                Args:
                    ab: a nice little integer
                """
                return 0
            "#,
        );

        assert_snapshot!(test.hover(), @r#"
        int
        ---------------------------------------------
        ```python
        int
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:10
          |
        2 | def test(ab: int):
          |          ^-
          |          ||
          |          |Cursor offset
          |          source
        3 |     """my cool test
          |
        "#);
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
          --> main.py:16:1
           |
        14 |     a = bar
        15 |
        16 | a
           | ^- Cursor offset
           | |
           | source
           |
        ");
    }

    #[test]
    fn hover_string_annotation1() {
        let test = cursor_test(
            r#"
        a: "MyCla<CURSOR>ss" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        MyClass
        ---------------------------------------------
        some docs

        ---------------------------------------------
        ```python
        MyClass
        ```
        ---
        some docs
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:5
          |
        2 | a: "MyClass" = 1
          |     ^^^^^-^
          |     |    |
          |     |    Cursor offset
          |     source
        3 |
        4 | class MyClass:
          |
        "#);
    }

    #[test]
    fn hover_string_annotation2() {
        let test = cursor_test(
            r#"
        a: "None | MyCl<CURSOR>ass" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        some docs

        ---------------------------------------------
        some docs
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:12
          |
        2 | a: "None | MyClass" = 1
          |            ^^^^-^^
          |            |   |
          |            |   Cursor offset
          |            source
        3 |
        4 | class MyClass:
          |
        "#);
    }

    #[test]
    fn hover_string_annotation3() {
        let test = cursor_test(
            r#"
        a: "None |<CURSOR> MyClass" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_string_annotation4() {
        let test = cursor_test(
            r#"
        a: "None | MyClass<CURSOR>" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        some docs

        ---------------------------------------------
        some docs
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:12
          |
        2 | a: "None | MyClass" = 1
          |            ^^^^^^^- Cursor offset
          |            |
          |            source
        3 |
        4 | class MyClass:
          |
        "#);
    }

    #[test]
    fn hover_string_annotation5() {
        let test = cursor_test(
            r#"
        a: "None | MyClass"<CURSOR> = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_string_annotation_dangling1() {
        let test = cursor_test(
            r#"
        a: "MyCl<CURSOR>ass |" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_string_annotation_dangling2() {
        let test = cursor_test(
            r#"
        a: "MyCl<CURSOR>ass | No" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        some docs

        ---------------------------------------------
        some docs
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:5
          |
        2 | a: "MyClass | No" = 1
          |     ^^^^-^^
          |     |   |
          |     |   Cursor offset
          |     source
        3 |
        4 | class MyClass:
          |
        "#);
    }

    #[test]
    fn hover_string_annotation_dangling3() {
        let test = cursor_test(
            r#"
        a: "MyClass | N<CURSOR>o" = 1

        class MyClass:
            """some docs"""
        "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_string_annotation_recursive() {
        let test = cursor_test(
            r#"
        ab: "a<CURSOR>b"
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        Unknown
        ---------------------------------------------
        ```python
        Unknown
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:6
          |
        2 | ab: "ab"
          |      ^-
          |      ||
          |      |Cursor offset
          |      source
          |
        "#);
    }

    #[test]
    fn hover_string_annotation_unknown() {
        let test = cursor_test(
            r#"
        x: "foo<CURSOR>bar"
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        Unknown
        ---------------------------------------------
        ```python
        Unknown
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:5
          |
        2 | x: "foobar"
          |     ^^^-^^
          |     |  |
          |     |  Cursor offset
          |     source
          |
        "#);
    }

    #[test]
    fn hover_overload_type_disambiguated1() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

a<CURSOR>b(1)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int):
    """the int overload"""

@overload
def ab(a: str): ...
    """the str overload"""
"#,
            )
            .build();

        assert_snapshot!(test.hover(), @r"
        def ab(a: int) -> Unknown
        ---------------------------------------------
        the int overload

        ---------------------------------------------
        ```python
        def ab(a: int) -> Unknown
        ```
        ---
        the int overload
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1)
          | ^-
          | ||
          | |Cursor offset
          | source
          |
        ");
    }

    #[test]
    fn hover_overload_type_disambiguated2() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
from mymodule import ab

a<CURSOR>b("hello")
"#,
            )
            .source(
                "mymodule.py",
                r#"
def ab(a):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int):
    """the int overload"""

@overload
def ab(a: str):
    """the str overload"""
"#,
            )
            .build();

        assert_snapshot!(test.hover(), @r#"
        def ab(a: str) -> Unknown
        ---------------------------------------------
        the int overload

        ---------------------------------------------
        ```python
        def ab(a: str) -> Unknown
        ```
        ---
        the int overload
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab("hello")
          | ^-
          | ||
          | |Cursor offset
          | source
          |
        "#);
    }

    #[test]
    fn hover_overload_arity_disambiguated1() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

a<CURSOR>b(1, 2)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a, b = None):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int, b: int):
    """the two arg overload"""

@overload
def ab(a: int):
    """the one arg overload"""
"#,
            )
            .build();

        assert_snapshot!(test.hover(), @r"
        def ab(
            a: int,
            b: int
        ) -> Unknown
        ---------------------------------------------
        the two arg overload

        ---------------------------------------------
        ```python
        def ab(
            a: int,
            b: int
        ) -> Unknown
        ```
        ---
        the two arg overload
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1, 2)
          | ^-
          | ||
          | |Cursor offset
          | source
          |
        ");
    }

    #[test]
    fn hover_overload_arity_disambiguated2() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

a<CURSOR>b(1)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a, b = None):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int, b: int):
    """the two arg overload"""

@overload
def ab(a: int):
    """the one arg overload"""
"#,
            )
            .build();

        assert_snapshot!(test.hover(), @r"
        def ab(a: int) -> Unknown
        ---------------------------------------------
        the two arg overload

        ---------------------------------------------
        ```python
        def ab(a: int) -> Unknown
        ```
        ---
        the two arg overload
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1)
          | ^-
          | ||
          | |Cursor offset
          | source
          |
        ");
    }

    #[test]
    fn hover_overload_keyword_disambiguated1() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

a<CURSOR>b(1, b=2)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a, *, b = None, c = None):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int):
    """keywordless overload"""

@overload
def ab(a: int, *, b: int):
    """b overload"""

@overload
def ab(a: int, *, c: int):
    """c overload"""
"#,
            )
            .build();

        assert_snapshot!(test.hover(), @r"
        def ab(
            a: int,
            *,
            b: int
        ) -> Unknown
        ---------------------------------------------
        keywordless overload

        ---------------------------------------------
        ```python
        def ab(
            a: int,
            *,
            b: int
        ) -> Unknown
        ```
        ---
        keywordless overload
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1, b=2)
          | ^-
          | ||
          | |Cursor offset
          | source
          |
        ");
    }

    #[test]
    fn hover_overload_keyword_disambiguated2() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

a<CURSOR>b(1, c=2)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a, *, b = None, c = None):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int):
    """keywordless overload"""

@overload
def ab(a: int, *, b: int):
    """b overload"""

@overload
def ab(a: int, *, c: int):
    """c overload"""
"#,
            )
            .build();

        assert_snapshot!(test.hover(), @r"
        def ab(
            a: int,
            *,
            c: int
        ) -> Unknown
        ---------------------------------------------
        keywordless overload

        ---------------------------------------------
        ```python
        def ab(
            a: int,
            *,
            c: int
        ) -> Unknown
        ```
        ---
        keywordless overload
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:1
          |
        2 | from mymodule import ab
        3 |
        4 | ab(1, c=2)
          | ^-
          | ||
          | |Cursor offset
          | source
          |
        ");
    }

    #[test]
    fn hover_overload_ambiguous() {
        let test = cursor_test(
            r#"
            from typing import overload

            @overload
            def foo(a: int, b):
                """The first overload"""
                return 0

            @overload
            def foo(a: str, b):
                """The second overload"""
                return 1

            if random.choice([True, False]):
                a = 1
            else:
                a = "hello"

            foo<CURSOR>(a, 2)
            "#,
        );

        assert_snapshot!(test.hover(), @r#"
        def foo(
            a: int,
            b
        ) -> Unknown
        def foo(
            a: str,
            b
        ) -> Unknown
        ---------------------------------------------
        The first overload

        ---------------------------------------------
        ```python
        def foo(
            a: int,
            b
        ) -> Unknown
        def foo(
            a: str,
            b
        ) -> Unknown
        ```
        ---
        The first overload
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:19:1
           |
        17 |     a = "hello"
        18 |
        19 | foo(a, 2)
           | ^^^- Cursor offset
           | |
           | source
           |
        "#);
    }

    #[test]
    fn hover_overload_ambiguous_compact() {
        let test = cursor_test(
            r#"
            from typing import overload

            @overload
            def foo(a: int):
                """The first overload"""
                return 0

            @overload
            def foo(a: str):
                """The second overload"""
                return 1

            if random.choice([True, False]):
                a = 1
            else:
                a = "hello"

            foo<CURSOR>(a)
            "#,
        );

        assert_snapshot!(test.hover(), @r#"
        def foo(a: int) -> Unknown
        def foo(a: str) -> Unknown
        ---------------------------------------------
        The first overload

        ---------------------------------------------
        ```python
        def foo(a: int) -> Unknown
        def foo(a: str) -> Unknown
        ```
        ---
        The first overload
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:19:1
           |
        17 |     a = "hello"
        18 |
        19 | foo(a)
           | ^^^- Cursor offset
           | |
           | source
           |
        "#);
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
        The cool lib_py module!

        Wow this module rocks.

        ---------------------------------------------
        ```xml
        <module 'lib'>
        ```
        ---
        The cool lib/_py module!  
          
        Wow this module rocks.
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:1
          |
        2 | import lib
        3 |
        4 | lib
          | ^^-
          | | |
          | | Cursor offset
          | source
          |
        ");
    }

    #[test]
    fn hover_nonlocal_binding() {
        let test = cursor_test(
            r#"
def outer():
    x = "outer_value"

    def inner():
        nonlocal x
        x = "modified"
        return x<CURSOR>  # Should find the nonlocal x declaration in outer scope

    return inner
"#,
        );

        // Should find the variable declaration in the outer scope, not the nonlocal statement
        assert_snapshot!(test.hover(), @r#"
        Literal["modified"]
        ---------------------------------------------
        ```python
        Literal["modified"]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:8:16
           |
         6 |         nonlocal x
         7 |         x = "modified"
         8 |         return x  # Should find the nonlocal x declaration in outer scope
           |                ^- Cursor offset
           |                |
           |                source
         9 |
        10 |     return inner
           |
        "#);
    }

    #[test]
    fn hover_nonlocal_stmt() {
        let test = cursor_test(
            r#"
def outer():
    xy = "outer_value"

    def inner():
        nonlocal x<CURSOR>y
        xy = "modified"
        return x  # Should find the nonlocal x declaration in outer scope

    return inner
"#,
        );

        // Should find the variable declaration in the outer scope, not the nonlocal statement
        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_global_binding() {
        let test = cursor_test(
            r#"
global_var = "global_value"

def function():
    global global_var
    global_var = "modified"
    return global_<CURSOR>var  # Should find the global variable declaration
"#,
        );

        // Should find the global variable declaration, not the global statement
        assert_snapshot!(test.hover(), @r#"
        Literal["modified"]
        ---------------------------------------------
        ```python
        Literal["modified"]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:7:12
          |
        5 |     global global_var
        6 |     global_var = "modified"
        7 |     return global_var  # Should find the global variable declaration
          |            ^^^^^^^-^^
          |            |      |
          |            |      Cursor offset
          |            source
          |
        "#);
    }

    #[test]
    fn hover_global_stmt() {
        let test = cursor_test(
            r#"
global_var = "global_value"

def function():
    global global_<CURSOR>var
    global_var = "modified"
    return global_var  # Should find the global variable declaration
"#,
        );

        // Should find the global variable declaration, not the global statement
        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_match_name_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_match_name_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.hover(), @r#"
        @Todo
        ---------------------------------------------
        ```python
        @Todo
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:5:17
          |
        3 |     match command.split():
        4 |         case ["get", ab]:
        5 |             x = ab
          |                 ^-
          |                 ||
          |                 |Cursor offset
          |                 source
          |
        "#);
    }

    #[test]
    fn hover_match_rest_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", *a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_match_rest_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", *ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.hover(), @r#"
        @Todo
        ---------------------------------------------
        ```python
        @Todo
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:5:17
          |
        3 |     match command.split():
        4 |         case ["get", *ab]:
        5 |             x = ab
          |                 ^-
          |                 ||
          |                 |Cursor offset
          |                 source
          |
        "#);
    }

    #[test]
    fn hover_match_as_stmt() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ("a" | "b") as a<CURSOR>b]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_match_as_binding() {
        let test = cursor_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ("a" | "b") as ab]:
                        x = a<CURSOR>b
            "#,
        );

        assert_snapshot!(test.hover(), @r#"
        @Todo
        ---------------------------------------------
        ```python
        @Todo
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:5:17
          |
        3 |     match command.split():
        4 |         case ["get", ("a" | "b") as ab]:
        5 |             x = ab
          |                 ^-
          |                 ||
          |                 |Cursor offset
          |                 source
          |
        "#);
    }

    #[test]
    fn hover_match_keyword_stmt() {
        let test = cursor_test(
            r#"
            class Click:
                __match_args__ = ("position", "button")
                def __init__(self, pos, btn):
                    self.position: int = pos
                    self.button: str = btn

            def my_func(event: Click):
                match event:
                    case Click(x, button=a<CURSOR>b):
                        x = ab
            "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_match_keyword_binding() {
        let test = cursor_test(
            r#"
            class Click:
                __match_args__ = ("position", "button")
                def __init__(self, pos, btn):
                    self.position: int = pos
                    self.button: str = btn

            def my_func(event: Click):
                match event:
                    case Click(x, button=ab):
                        x = a<CURSOR>b
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
          --> main.py:11:17
           |
         9 |     match event:
        10 |         case Click(x, button=ab):
        11 |             x = ab
           |                 ^-
           |                 ||
           |                 |Cursor offset
           |                 source
           |
        ");
    }

    #[test]
    fn hover_match_class_name() {
        let test = cursor_test(
            r#"
            class Click:
                __match_args__ = ("position", "button")
                def __init__(self, pos, btn):
                    self.position: int = pos
                    self.button: str = btn

            def my_func(event: Click):
                match event:
                    case Cl<CURSOR>ick(x, button=ab):
                        x = ab
            "#,
        );

        assert_snapshot!(test.hover(), @r"
        <class 'Click'>
        ---------------------------------------------
        ```xml
        <class 'Click'>
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:10:14
           |
         8 | def my_func(event: Click):
         9 |     match event:
        10 |         case Click(x, button=ab):
           |              ^^-^^
           |              | |
           |              | Cursor offset
           |              source
        11 |             x = ab
           |
        ");
    }

    #[test]
    fn hover_match_class_field_name() {
        let test = cursor_test(
            r#"
            class Click:
                __match_args__ = ("position", "button")
                def __init__(self, pos, btn):
                    self.position: int = pos
                    self.button: str = btn

            def my_func(event: Click):
                match event:
                    case Click(x, but<CURSOR>ton=ab):
                        x = ab
            "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_typevar_name_stmt() {
        let test = cursor_test(
            r#"
            type Alias1[A<CURSOR>B: int = bool] = tuple[AB, list[AB]]
            "#,
        );

        assert_snapshot!(test.hover(), @r"
        AB@Alias1 (invariant)
        ---------------------------------------------
        ```python
        AB@Alias1 (invariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:13
          |
        2 | type Alias1[AB: int = bool] = tuple[AB, list[AB]]
          |             ^-
          |             ||
          |             |Cursor offset
          |             source
          |
        ");
    }

    #[test]
    fn hover_typevar_name_binding() {
        let test = cursor_test(
            r#"
            type Alias1[AB: int = bool] = tuple[A<CURSOR>B, list[AB]]
            "#,
        );

        assert_snapshot!(test.hover(), @r"
        AB@Alias1 (invariant)
        ---------------------------------------------
        ```python
        AB@Alias1 (invariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:37
          |
        2 | type Alias1[AB: int = bool] = tuple[AB, list[AB]]
          |                                     ^-
          |                                     ||
          |                                     |Cursor offset
          |                                     source
          |
        ");
    }

    #[test]
    fn hover_typevar_spec_stmt() {
        let test = cursor_test(
            r#"
            from typing import Callable
            type Alias2[**A<CURSOR>B = [int, str]] = Callable[AB, tuple[AB]]
            "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_typevar_spec_binding() {
        let test = cursor_test(
            r#"
            from typing import Callable
            type Alias2[**AB = [int, str]] = Callable[A<CURSOR>B, tuple[AB]]
            "#,
        );

        // TODO: This should just be `**AB@Alias2 (<variance>)`
        // https://github.com/astral-sh/ty/issues/1581
        assert_snapshot!(test.hover(), @r"
        (**AB@Alias2) -> tuple[AB@Alias2]
        ---------------------------------------------
        ```python
        (**AB@Alias2) -> tuple[AB@Alias2]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:3:43
          |
        2 | from typing import Callable
        3 | type Alias2[**AB = [int, str]] = Callable[AB, tuple[AB]]
          |                                           ^-
          |                                           ||
          |                                           |Cursor offset
          |                                           source
          |
        ");
    }

    #[test]
    fn hover_typevar_tuple_stmt() {
        let test = cursor_test(
            r#"
            type Alias3[*A<CURSOR>B = ()] = tuple[tuple[*AB], tuple[*AB]]
            "#,
        );

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_typevar_tuple_binding() {
        let test = cursor_test(
            r#"
            type Alias3[*AB = ()] = tuple[tuple[*A<CURSOR>B], tuple[*AB]]
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
         --> main.py:2:38
          |
        2 | type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]
          |                                      ^-
          |                                      ||
          |                                      |Cursor offset
          |                                      source
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

        assert_snapshot!(test.hover(), @r"
        <module 'lib'>
        ---------------------------------------------
        The cool lib_py module!

        Wow this module rocks.

        ---------------------------------------------
        ```xml
        <module 'lib'>
        ```
        ---
        The cool lib/_py module!  
          
        Wow this module rocks.
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:8
          |
        2 | import lib
          |        ^^-
          |        | |
          |        | Cursor offset
          |        source
        3 |
        4 | lib
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
        T@Alias (invariant)
        ---------------------------------------------
        ```python
        T@Alias (invariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:34
          |
        2 | type Alias[T: int = bool] = list[T]
          |                                  ^- Cursor offset
          |                                  |
          |                                  source
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

        // TODO: Should this be constravariant instead?
        assert_snapshot!(test.hover(), @r"
        P@Alias (bivariant)
        ---------------------------------------------
        ```python
        P@Alias (bivariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:41
          |
        2 | type Alias[**P = [int, str]] = Callable[P, int]
          |                                         ^- Cursor offset
          |                                         |
          |                                         source
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
         --> main.py:2:31
          |
        2 | type Alias[*Ts = ()] = tuple[*Ts]
          |                               ^^- Cursor offset
          |                               |
          |                               source
          |
        ");
    }

    #[test]
    fn hover_variable_assignment() {
        let test = cursor_test(
            r#"
            value<CURSOR> = 1
            """This is the docs for this value

            Wow these are good docs!
            """
            "#,
        );

        assert_snapshot!(test.hover(), @r#"
        Literal[1]
        ---------------------------------------------
        This is the docs for this value

        Wow these are good docs!

        ---------------------------------------------
        ```python
        Literal[1]
        ```
        ---
        This is the docs for this value  
          
        Wow these are good docs!
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:1
          |
        2 | value = 1
          | ^^^^^- Cursor offset
          | |
          | source
        3 | """This is the docs for this value
          |
        "#);
    }

    #[test]
    fn hover_augmented_assignment() {
        let test = cursor_test(
            r#"
            value = 1
            """This is the docs for this value

            Wow these are good docs!
            """
            value<CURSOR> += 2
            """Other docs???

            Is this allowed???
            """
            "#,
        );

        // We currently show the *previous* value of the variable (1), not the new one (3).
        // Showing the new value might be more intuitive for some users, but the actual 'use'
        // of the `value` symbol here in read-context is `1`. This comment mainly exists to
        // signal that it might be okay to revisit this in the future and reveal 3 instead.
        assert_snapshot!(test.hover(), @r#"
        Literal[1]
        ---------------------------------------------
        This is the docs for this value

        Wow these are good docs!

        ---------------------------------------------
        ```python
        Literal[1]
        ```
        ---
        This is the docs for this value  
          
        Wow these are good docs!
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:7:1
          |
        5 | Wow these are good docs!
        6 | """
        7 | value += 2
          | ^^^^^- Cursor offset
          | |
          | source
        8 | """Other docs???
          |
        "#);
    }

    #[test]
    fn hover_attribute_assignment() {
        let test = cursor_test(
            r#"
            class C:
                attr: int = 1
                """This is the docs for this value

                Wow these are good docs!
                """

            C.attr<CURSOR> = 2
            """Other docs???

            Is this allowed???
            """
            "#,
        );

        assert_snapshot!(test.hover(), @r#"
        Literal[2]
        ---------------------------------------------
        This is the docs for this value

        Wow these are good docs!

        ---------------------------------------------
        ```python
        Literal[2]
        ```
        ---
        This is the docs for this value  
          
        Wow these are good docs!
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:9:3
           |
         7 |     """
         8 |
         9 | C.attr = 2
           |   ^^^^- Cursor offset
           |   |
           |   source
        10 | """Other docs???
           |
        "#);
    }

    #[test]
    fn hover_augmented_attribute_assignment() {
        let test = cursor_test(
            r#"
            class C:
                attr = 1
                """This is the docs for this value

                Wow these are good docs!
                """

            C.attr<CURSOR> += 2
            """Other docs???

            Is this allowed???
            """
            "#,
        );

        // See the comment in the `hover_augmented_assignment` test above. The same
        // reasoning applies here.
        assert_snapshot!(test.hover(), @r#"
        Unknown | Literal[1]
        ---------------------------------------------
        This is the docs for this value

        Wow these are good docs!

        ---------------------------------------------
        ```python
        Unknown | Literal[1]
        ```
        ---
        This is the docs for this value  
          
        Wow these are good docs!
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:9:3
           |
         7 |     """
         8 |
         9 | C.attr += 2
           |   ^^^^- Cursor offset
           |   |
           |   source
        10 | """Other docs???
           |
        "#);
    }

    #[test]
    fn hover_annotated_assignment() {
        let test = cursor_test(
            r#"
        class Foo:
            a<CURSOR>: int
            """This is the docs for this value

            Wow these are good docs!
            """
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        int
        ---------------------------------------------
        This is the docs for this value

        Wow these are good docs!

        ---------------------------------------------
        ```python
        int
        ```
        ---
        This is the docs for this value  
          
        Wow these are good docs!
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:3:5
          |
        2 | class Foo:
        3 |     a: int
          |     ^- Cursor offset
          |     |
          |     source
        4 |     """This is the docs for this value
          |
        "#);
    }

    #[test]
    fn hover_annotated_assignment_with_rhs() {
        let test = cursor_test(
            r#"
        class Foo:
            a<CURSOR>: int = 1
            """This is the docs for this value

            Wow these are good docs!
            """
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        Literal[1]
        ---------------------------------------------
        This is the docs for this value

        Wow these are good docs!

        ---------------------------------------------
        ```python
        Literal[1]
        ```
        ---
        This is the docs for this value  
          
        Wow these are good docs!
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:3:5
          |
        2 | class Foo:
        3 |     a: int = 1
          |     ^- Cursor offset
          |     |
          |     source
        4 |     """This is the docs for this value
          |
        "#);
    }

    #[test]
    fn hover_annotated_assignment_with_rhs_use() {
        let test = cursor_test(
            r#"
        class Foo:
            a: int = 1
            """This is the docs for this value

            Wow these are good docs!
            """
        
        x = Foo()
        x.a<CURSOR>
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        int
        ---------------------------------------------
        This is the docs for this value

        Wow these are good docs!

        ---------------------------------------------
        ```python
        int
        ```
        ---
        This is the docs for this value  
          
        Wow these are good docs!
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:10:3
           |
         9 | x = Foo()
        10 | x.a
           |   ^- Cursor offset
           |   |
           |   source
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
                """This is the docs for this value

                Wow these are good docs!
                """
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        int
        ---------------------------------------------
        This is the docs for this value

        Wow these are good docs!

        ---------------------------------------------
        ```python
        int
        ```
        ---
        This is the docs for this value  
          
        Wow these are good docs!
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:14
          |
        2 | class Foo:
        3 |     def __init__(self, a: int):
        4 |         self.a: int = a
          |              ^- Cursor offset
          |              |
          |              source
        5 |         """This is the docs for this value
          |
        "#);
    }

    #[test]
    fn hover_annotated_attribute_assignment_use() {
        let test = cursor_test(
            r#"
        class Foo:
            def __init__(self, a: int):
                self.a: int = a
                """This is the docs for this value

                Wow these are good docs!
                """

        x = Foo(1)
        x.a<CURSOR>
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        int
        ---------------------------------------------
        This is the docs for this value

        Wow these are good docs!

        ---------------------------------------------
        ```python
        int
        ```
        ---
        This is the docs for this value  
          
        Wow these are good docs!
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:11:3
           |
        10 | x = Foo(1)
        11 | x.a
           |   ^- Cursor offset
           |   |
           |   source
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
          --> main.py:10:15
           |
         8 |     '''
         9 |     if a is not None:
        10 |         print(a)
           |               ^- Cursor offset
           |               |
           |               source
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
    fn hover_complex_type1() {
        let test = cursor_test(
            r#"
        from typing import Callable, Any, List
        def ab(x: int, y: Callable[[int, int], Any], z: List[int]) -> int: ...

        a<CURSOR>b
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        def ab(
            x: int,
            y: (int, int, /) -> Any,
            z: list[int]
        ) -> int
        ---------------------------------------------
        ```python
        def ab(
            x: int,
            y: (int, int, /) -> Any,
            z: list[int]
        ) -> int
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:5:1
          |
        3 | def ab(x: int, y: Callable[[int, int], Any], z: List[int]) -> int: ...
        4 |
        5 | ab
          | ^-
          | ||
          | |Cursor offset
          | source
          |
        ");
    }

    #[test]
    fn hover_complex_type2() {
        let test = cursor_test(
            r#"
        from typing import Callable, Tuple, Any
        ab: Tuple[Any, int, Callable[[int, int], Any]] = ...

        a<CURSOR>b
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        tuple[Any, int, (int, int, /) -> Any]
        ---------------------------------------------
        ```python
        tuple[Any, int, (int, int, /) -> Any]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:5:1
          |
        3 | ab: Tuple[Any, int, Callable[[int, int], Any]] = ...
        4 |
        5 | ab
          | ^-
          | ||
          | |Cursor offset
          | source
          |
        ");
    }

    #[test]
    fn hover_complex_type3() {
        let test = cursor_test(
            r#"
        from typing import Callable, Any
        ab:  Callable[[int, int], Any] | None  = ...

        a<CURSOR>b
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        ((int, int, /) -> Any) | None
        ---------------------------------------------
        ```python
        ((int, int, /) -> Any) | None
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:5:1
          |
        3 | ab:  Callable[[int, int], Any] | None  = ...
        4 |
        5 | ab
          | ^-
          | ||
          | |Cursor offset
          | source
          |
        ");
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

    #[test]
    fn hover_func_with_concat_docstring() {
        let test = cursor_test(
            r#"
        def a<CURSOR>b():
            """wow cool docs""" """and docs"""
            return
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        def ab() -> Unknown
        ---------------------------------------------
        wow cool docsand docs

        ---------------------------------------------
        ```python
        def ab() -> Unknown
        ```
        ---
        wow cool docsand docs
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:5
          |
        2 | def ab():
          |     ^-
          |     ||
          |     |Cursor offset
          |     source
        3 |     """wow cool docs""" """and docs"""
        4 |     return
          |
        "#);
    }

    #[test]
    fn hover_func_with_plus_docstring() {
        let test = cursor_test(
            r#"
        def a<CURSOR>b():
            """wow cool docs""" + """and docs"""
            return
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        def ab() -> Unknown
        ---------------------------------------------
        ```python
        def ab() -> Unknown
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:5
          |
        2 | def ab():
          |     ^-
          |     ||
          |     |Cursor offset
          |     source
        3 |     """wow cool docs""" + """and docs"""
        4 |     return
          |
        "#);
    }

    #[test]
    fn hover_func_with_slash_docstring() {
        let test = cursor_test(
            r#"
        def a<CURSOR>b():
            """wow cool docs""" \
            """and docs"""
            return
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        def ab() -> Unknown
        ---------------------------------------------
        wow cool docsand docs

        ---------------------------------------------
        ```python
        def ab() -> Unknown
        ```
        ---
        wow cool docsand docs
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:5
          |
        2 | def ab():
          |     ^-
          |     ||
          |     |Cursor offset
          |     source
        3 |     """wow cool docs""" \
        4 |     """and docs"""
          |
        "#);
    }

    #[test]
    fn hover_func_with_sameline_commented_docstring() {
        let test = cursor_test(
            r#"
        def a<CURSOR>b():
            """wow cool docs""" # and a comment
            """and docs"""      # that shouldn't be included
            return
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        def ab() -> Unknown
        ---------------------------------------------
        wow cool docs

        ---------------------------------------------
        ```python
        def ab() -> Unknown
        ```
        ---
        wow cool docs
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:5
          |
        2 | def ab():
          |     ^-
          |     ||
          |     |Cursor offset
          |     source
        3 |     """wow cool docs""" # and a comment
        4 |     """and docs"""      # that shouldn't be included
          |
        "#);
    }

    #[test]
    fn hover_func_with_nextline_commented_docstring() {
        let test = cursor_test(
            r#"
        def a<CURSOR>b():
            """wow cool docs""" 
            # and a comment that shouldn't be included
            """and docs"""
            return
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        def ab() -> Unknown
        ---------------------------------------------
        wow cool docs

        ---------------------------------------------
        ```python
        def ab() -> Unknown
        ```
        ---
        wow cool docs
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:5
          |
        2 | def ab():
          |     ^-
          |     ||
          |     |Cursor offset
          |     source
        3 |     """wow cool docs""" 
        4 |     # and a comment that shouldn't be included
          |
        "#);
    }

    #[test]
    fn hover_func_with_parens_docstring() {
        let test = cursor_test(
            r#"
        def a<CURSOR>b():
            (
                """wow cool docs""" 
                """and docs"""
            )
            return
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        def ab() -> Unknown
        ---------------------------------------------
        wow cool docsand docs

        ---------------------------------------------
        ```python
        def ab() -> Unknown
        ```
        ---
        wow cool docsand docs
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:5
          |
        2 | def ab():
          |     ^-
          |     ||
          |     |Cursor offset
          |     source
        3 |     (
        4 |         """wow cool docs""" 
          |
        "#);
    }

    #[test]
    fn hover_func_with_nextline_commented_parens_docstring() {
        let test = cursor_test(
            r#"
        def a<CURSOR>b():
            (
                """wow cool docs""" 
                # and a comment that shouldn't be included
                """and docs"""
            )
            return
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        def ab() -> Unknown
        ---------------------------------------------
        wow cool docsand docs

        ---------------------------------------------
        ```python
        def ab() -> Unknown
        ```
        ---
        wow cool docsand docs
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:5
          |
        2 | def ab():
          |     ^-
          |     ||
          |     |Cursor offset
          |     source
        3 |     (
        4 |         """wow cool docs""" 
          |
        "#);
    }

    #[test]
    fn hover_attribute_docstring_spill() {
        let test = cursor_test(
            r#"
        if True:
            a<CURSOR>b = 1
        "this shouldn't be a docstring but also it doesn't matter much"
        "#,
        );

        assert_snapshot!(test.hover(), @r#"
        Literal[1]
        ---------------------------------------------
        ```python
        Literal[1]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:3:5
          |
        2 | if True:
        3 |     ab = 1
          |     ^-
          |     ||
          |     |Cursor offset
          |     source
        4 | "this shouldn't be a docstring but also it doesn't matter much"
          |
        "#);
    }

    #[test]
    fn hover_class_typevar_variance() {
        let test = cursor_test(
            r#"
        class Covariant[T<CURSOR>]:
            def get(self) -> T:
                raise ValueError
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        T@Covariant (covariant)
        ---------------------------------------------
        ```python
        T@Covariant (covariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:17
          |
        2 | class Covariant[T]:
          |                 ^- Cursor offset
          |                 |
          |                 source
        3 |     def get(self) -> T:
        4 |         raise ValueError
          |
        ");

        let test = cursor_test(
            r#"
        class Covariant[T]:
            def get(self) -> T<CURSOR>:
                raise ValueError
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        T@Covariant (covariant)
        ---------------------------------------------
        ```python
        T@Covariant (covariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:3:22
          |
        2 | class Covariant[T]:
        3 |     def get(self) -> T:
          |                      ^- Cursor offset
          |                      |
          |                      source
        4 |         raise ValueError
          |
        ");

        let test = cursor_test(
            r#"
        class Contravariant[T<CURSOR>]:
            def set(self, x: T):
                pass
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        T@Contravariant (contravariant)
        ---------------------------------------------
        ```python
        T@Contravariant (contravariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:21
          |
        2 | class Contravariant[T]:
          |                     ^- Cursor offset
          |                     |
          |                     source
        3 |     def set(self, x: T):
        4 |         pass
          |
        ");

        let test = cursor_test(
            r#"
        class Contravariant[T]:
            def set(self, x: T<CURSOR>):
                pass
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        T@Contravariant (contravariant)
        ---------------------------------------------
        ```python
        T@Contravariant (contravariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:3:22
          |
        2 | class Contravariant[T]:
        3 |     def set(self, x: T):
          |                      ^- Cursor offset
          |                      |
          |                      source
        4 |         pass
          |
        ");
    }

    #[test]
    fn hover_function_typevar_variance() {
        let test = cursor_test(
            r#"
        def covariant[T<CURSOR>]() -> T:
            raise ValueError
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        T@covariant (covariant)
        ---------------------------------------------
        ```python
        T@covariant (covariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:15
          |
        2 | def covariant[T]() -> T:
          |               ^- Cursor offset
          |               |
          |               source
        3 |     raise ValueError
          |
        ");

        let test = cursor_test(
            r#"
        def covariant[T]() -> T<CURSOR>:
            raise ValueError
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        T@covariant (covariant)
        ---------------------------------------------
        ```python
        T@covariant (covariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:23
          |
        2 | def covariant[T]() -> T:
          |                       ^- Cursor offset
          |                       |
          |                       source
        3 |     raise ValueError
          |
        ");

        let test = cursor_test(
            r#"
        def contravariant[T<CURSOR>](x: T):
            pass
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        T@contravariant (contravariant)
        ---------------------------------------------
        ```python
        T@contravariant (contravariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:19
          |
        2 | def contravariant[T](x: T):
          |                   ^- Cursor offset
          |                   |
          |                   source
        3 |     pass
          |
        ");

        let test = cursor_test(
            r#"
        def contravariant[T](x: T<CURSOR>):
            pass
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        T@contravariant (contravariant)
        ---------------------------------------------
        ```python
        T@contravariant (contravariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:25
          |
        2 | def contravariant[T](x: T):
          |                         ^- Cursor offset
          |                         |
          |                         source
        3 |     pass
          |
        ");
    }

    #[test]
    fn hover_type_alias_typevar_variance() {
        let test = cursor_test(
            r#"
        type List[T<CURSOR>] = list[T]
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        T@List (invariant)
        ---------------------------------------------
        ```python
        T@List (invariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:11
          |
        2 | type List[T] = list[T]
          |           ^- Cursor offset
          |           |
          |           source
          |
        ");

        let test = cursor_test(
            r#"
        type List[T] = list[T<CURSOR>]
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        T@List (invariant)
        ---------------------------------------------
        ```python
        T@List (invariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:21
          |
        2 | type List[T] = list[T]
          |                     ^- Cursor offset
          |                     |
          |                     source
          |
        ");

        let test = cursor_test(
            r#"
        type Tuple[T<CURSOR>] = tuple[T]
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        T@Tuple (covariant)
        ---------------------------------------------
        ```python
        T@Tuple (covariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:12
          |
        2 | type Tuple[T] = tuple[T]
          |            ^- Cursor offset
          |            |
          |            source
          |
        ");

        let test = cursor_test(
            r#"
        type Tuple[T] = tuple[T<CURSOR>]
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        T@Tuple (covariant)
        ---------------------------------------------
        ```python
        T@Tuple (covariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:23
          |
        2 | type Tuple[T] = tuple[T]
          |                       ^- Cursor offset
          |                       |
          |                       source
          |
        ");
    }

    #[test]
    fn hover_legacy_typevar_variance() {
        let test = cursor_test(
            r#"
        from typing import TypeVar

        T<CURSOR> = TypeVar('T', covariant=True)

        def covariant() -> T:
            raise ValueError
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        TypeVar
        ---------------------------------------------
        ```python
        TypeVar
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:1
          |
        2 | from typing import TypeVar
        3 |
        4 | T = TypeVar('T', covariant=True)
          | ^- Cursor offset
          | |
          | source
        5 |
        6 | def covariant() -> T:
          |
        ");

        let test = cursor_test(
            r#"
        from typing import TypeVar

        T = TypeVar('T', covariant=True)

        def covariant() -> T<CURSOR>:
            raise ValueError
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        T@covariant (covariant)
        ---------------------------------------------
        ```python
        T@covariant (covariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:6:20
          |
        4 | T = TypeVar('T', covariant=True)
        5 |
        6 | def covariant() -> T:
          |                    ^- Cursor offset
          |                    |
          |                    source
        7 |     raise ValueError
          |
        ");

        let test = cursor_test(
            r#"
        from typing import TypeVar

        T<CURSOR> = TypeVar('T', contravariant=True)

        def contravariant(x: T):
            pass
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        TypeVar
        ---------------------------------------------
        ```python
        TypeVar
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:4:1
          |
        2 | from typing import TypeVar
        3 |
        4 | T = TypeVar('T', contravariant=True)
          | ^- Cursor offset
          | |
          | source
        5 |
        6 | def contravariant(x: T):
          |
        ");

        let test = cursor_test(
            r#"
        from typing import TypeVar

        T = TypeVar('T', contravariant=True)

        def contravariant(x: T<CURSOR>):
            pass
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        T@contravariant (contravariant)
        ---------------------------------------------
        ```python
        T@contravariant (contravariant)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:6:22
          |
        4 | T = TypeVar('T', contravariant=True)
        5 |
        6 | def contravariant(x: T):
          |                      ^- Cursor offset
          |                      |
          |                      source
        7 |     pass
          |
        ");
    }

    #[test]
    fn hover_binary_operator_literal() {
        let test = cursor_test(
            r#"
        result = 5 <CURSOR>+ 3
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        bound method int.__add__(value: int, /) -> int
        ---------------------------------------------
        Return self+value.

        ---------------------------------------------
        ```python
        bound method int.__add__(value: int, /) -> int
        ```
        ---
        Return self+value.
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:12
          |
        2 | result = 5 + 3
          |            -
          |            |
          |            source
          |            Cursor offset
          |
        ");
    }

    #[test]
    fn hover_binary_operator_overload() {
        let test = cursor_test(
            r#"
            from __future__ import annotations
            from typing import overload

            class Test:
                @overload
                def __add__(self, other: Test, /) -> Test:  ...
                @overload
                def __add__(self, other: Other, /) -> Test: ...
                def __add__(self, other: Test | Other, /) -> Test:
                    return self

            class Other: ...

            Test() <CURSOR>+ Test()
        "#,
        );

        // TODO: We should only show the matching overload here.
        // https://github.com/astral-sh/ty/issues/73
        assert_snapshot!(test.hover(), @r"
        def __add__(other: Test, /) -> Test
        def __add__(other: Other, /) -> Test
        ---------------------------------------------
        ```python
        def __add__(other: Test, /) -> Test
        def __add__(other: Other, /) -> Test
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:15:8
           |
        13 | class Other: ...
        14 |
        15 | Test() + Test()
           |        -
           |        |
           |        source
           |        Cursor offset
           |
        ");
    }

    #[test]
    fn hover_binary_operator_union() {
        let test = cursor_test(
            r#"
            from __future__ import annotations

            class Test:
                def __add__(self, other: Other, /) -> Other:
                    return other

            class Other:
                def __add__(self, other: Other, /) -> Other:
                    return self

            def _(a: Test | Other):
                a +<CURSOR> Other()
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        (bound method Test.__add__(other: Other, /) -> Other) | (bound method Other.__add__(other: Other, /) -> Other)
        ---------------------------------------------
        ```python
        (bound method Test.__add__(other: Other, /) -> Other) | (bound method Other.__add__(other: Other, /) -> Other)
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
          --> main.py:13:7
           |
        12 | def _(a: Test | Other):
        13 |     a + Other()
           |       ^- Cursor offset
           |       |
           |       source
           |
        ");
    }

    #[test]
    fn hover_float_annotation() {
        let test = cursor_test(
            r#"
            a: float<CURSOR> = 3.14
        "#,
        );

        assert_snapshot!(test.hover(), @r"
        int | float
        ---------------------------------------------
        Convert a string or number to a floating-point number, if possible.

        ---------------------------------------------
        ```python
        int | float
        ```
        ---
        Convert a string or number to a floating-point number, if possible.
        ---------------------------------------------
        info[hover]: Hovered content is
         --> main.py:2:4
          |
        2 | a: float = 3.14
          |    ^^^^^- Cursor offset
          |    |
          |    source
          |
        ");
    }

    #[test]
    fn hover_submodule_import_from_use() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg.submod import val

                x = sub<CURSOR>pkg
                "#,
            )
            .source("mypackage/subpkg/__init__.py", r#""#)
            .source(
                "mypackage/subpkg/submod.py",
                r#"
                val: int = 0
                "#,
            )
            .build();

        // The module is correct
        assert_snapshot!(test.hover(), @r"
        <module 'mypackage.subpkg'>
        ---------------------------------------------
        ```xml
        <module 'mypackage.subpkg'>
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> mypackage/__init__.py:4:5
          |
        2 | from .subpkg.submod import val
        3 |
        4 | x = subpkg
          |     ^^^-^^
          |     |  |
          |     |  Cursor offset
          |     source
          |
        ");
    }

    #[test]
    fn hover_submodule_import_from_def() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .sub<CURSOR>pkg.submod import val

                x = subpkg
                "#,
            )
            .source("mypackage/subpkg/__init__.py", r#""#)
            .source(
                "mypackage/subpkg/submod.py",
                r#"
                val: int = 0
                "#,
            )
            .build();

        // The module is correct
        assert_snapshot!(test.hover(), @r"
        <module 'mypackage.subpkg'>
        ---------------------------------------------
        ```xml
        <module 'mypackage.subpkg'>
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> mypackage/__init__.py:2:7
          |
        2 | from .subpkg.submod import val
          |       ^^^-^^
          |       |  |
          |       |  Cursor offset
          |       source
        3 |
        4 | x = subpkg
          |
        ");
    }

    #[test]
    fn hover_submodule_import_from_wrong_use() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg.submod import val

                x = sub<CURSOR>mod
                "#,
            )
            .source("mypackage/subpkg/__init__.py", r#""#)
            .source(
                "mypackage/subpkg/submod.py",
                r#"
                val: int = 0
                "#,
            )
            .build();

        // Unknown is correct
        assert_snapshot!(test.hover(), @r"
        Unknown
        ---------------------------------------------
        ```python
        Unknown
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> mypackage/__init__.py:4:5
          |
        2 | from .subpkg.submod import val
        3 |
        4 | x = submod
          |     ^^^-^^
          |     |  |
          |     |  Cursor offset
          |     source
          |
        ");
    }

    #[test]
    fn hover_submodule_import_from_wrong_def() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg.sub<CURSOR>mod import val

                x = submod
                "#,
            )
            .source("mypackage/subpkg/__init__.py", r#""#)
            .source(
                "mypackage/subpkg/submod.py",
                r#"
                val: int = 0
                "#,
            )
            .build();

        // The submodule is correct
        assert_snapshot!(test.hover(), @r"
        <module 'mypackage.subpkg.submod'>
        ---------------------------------------------
        ```xml
        <module 'mypackage.subpkg.submod'>
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> mypackage/__init__.py:2:14
          |
        2 | from .subpkg.submod import val
          |              ^^^-^^
          |              |  |
          |              |  Cursor offset
          |              source
        3 |
        4 | x = submod
          |
        ");
    }

    #[test]
    fn hover_submodule_import_from_confusing_shadowed_def() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .sub<CURSOR>pkg import subpkg

                x = subpkg
                "#,
            )
            .source(
                "mypackage/subpkg/__init__.py",
                r#"
                subpkg: int = 10
                "#,
            )
            .build();

        // The module is correct
        assert_snapshot!(test.hover(), @r"
        <module 'mypackage.subpkg'>
        ---------------------------------------------
        ```xml
        <module 'mypackage.subpkg'>
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> mypackage/__init__.py:2:7
          |
        2 | from .subpkg import subpkg
          |       ^^^-^^
          |       |  |
          |       |  Cursor offset
          |       source
        3 |
        4 | x = subpkg
          |
        ");
    }

    #[test]
    fn hover_submodule_import_from_confusing_real_def() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg import sub<CURSOR>pkg

                x = subpkg
                "#,
            )
            .source(
                "mypackage/subpkg/__init__.py",
                r#"
                subpkg: int = 10
                "#,
            )
            .build();

        // int is correct
        assert_snapshot!(test.hover(), @r"
        int
        ---------------------------------------------
        ```python
        int
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> mypackage/__init__.py:2:21
          |
        2 | from .subpkg import subpkg
          |                     ^^^-^^
          |                     |  |
          |                     |  Cursor offset
          |                     source
        3 |
        4 | x = subpkg
          |
        ");
    }

    #[test]
    fn hover_submodule_import_from_confusing_use() {
        let test = CursorTest::builder()
            .source(
                "mypackage/__init__.py",
                r#"
                from .subpkg import subpkg

                x = sub<CURSOR>pkg
                "#,
            )
            .source(
                "mypackage/subpkg/__init__.py",
                r#"
                subpkg: int = 10
                "#,
            )
            .build();

        // int is correct
        assert_snapshot!(test.hover(), @r"
        int
        ---------------------------------------------
        ```python
        int
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> mypackage/__init__.py:4:5
          |
        2 | from .subpkg import subpkg
        3 |
        4 | x = subpkg
          |     ^^^-^^
          |     |  |
          |     |  Cursor offset
          |     source
          |
        ");
    }

    #[test]
    fn hover_tuple_assignment_target() {
        let test = CursorTest::builder()
            .source(
                "test.py",
                r#"
                (x, y)<CURSOR> = "test", 10
                "#,
            )
            .build();

        assert_snapshot!(test.hover(), @"Hover provided no content");
    }

    #[test]
    fn hover_named_expression_target() {
        let test = CursorTest::builder()
            .source(
                "mymod.py",
                r#"
                if a<CURSOR> := 10:
                    pass
                "#,
            )
            .build();

        assert_snapshot!(test.hover(), @r###"
        Literal[10]
        ---------------------------------------------
        ```python
        Literal[10]
        ```
        ---------------------------------------------
        info[hover]: Hovered content is
         --> mymod.py:2:4
          |
        2 | if a := 10:
          |    ^- Cursor offset
          |    |
          |    source
        3 |     pass
          |
        "###);
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
