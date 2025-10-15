use std::{fmt, vec};

use crate::Db;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_python_semantic::types::Type;
use ty_python_semantic::types::ide_support::inlay_hint_function_argument_details;
use ty_python_semantic::{HasType, SemanticModel};

#[derive(Debug, Clone)]
pub struct InlayHint {
    pub position: TextSize,
    pub kind: InlayHintKind,
    pub label: InlayHintLabel,
}

impl InlayHint {
    fn variable_type(position: TextSize, ty: Type, db: &dyn Db) -> Self {
        let label_parts = vec![
            ": ".into(),
            InlayHintLabelPart::new(ty.display(db).to_string()),
        ];

        Self {
            position,
            kind: InlayHintKind::Type,
            label: InlayHintLabel { parts: label_parts },
        }
    }

    fn call_argument_name(position: TextSize, name: &str) -> Self {
        let label_parts = vec![InlayHintLabelPart::new(name), "=".into()];

        Self {
            position,
            kind: InlayHintKind::CallArgumentName,
            label: InlayHintLabel { parts: label_parts },
        }
    }

    pub fn display(&self) -> InlayHintDisplay<'_> {
        InlayHintDisplay { inlay_hint: self }
    }
}

#[derive(Debug, Clone)]
pub enum InlayHintKind {
    Type,
    CallArgumentName,
}

#[derive(Debug, Clone)]
pub struct InlayHintLabel {
    parts: Vec<InlayHintLabelPart>,
}

impl InlayHintLabel {
    pub fn parts(&self) -> &[InlayHintLabelPart] {
        &self.parts
    }
}

pub struct InlayHintDisplay<'a> {
    inlay_hint: &'a InlayHint,
}

impl fmt::Display for InlayHintDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        for part in &self.inlay_hint.label.parts {
            write!(f, "{}", part.text)?;
        }
        Ok(())
    }
}

#[derive(Default, Debug, Clone)]
pub struct InlayHintLabelPart {
    text: String,

    target: Option<crate::NavigationTarget>,
}

impl InlayHintLabelPart {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            target: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn target(&self) -> Option<&crate::NavigationTarget> {
        self.target.as_ref()
    }
}

impl From<String> for InlayHintLabelPart {
    fn from(s: String) -> Self {
        Self {
            text: s,
            target: None,
        }
    }
}

impl From<&str> for InlayHintLabelPart {
    fn from(s: &str) -> Self {
        Self {
            text: s.to_string(),
            target: None,
        }
    }
}

pub fn inlay_hints(
    db: &dyn Db,
    file: File,
    range: TextRange,
    settings: &InlayHintSettings,
) -> Vec<InlayHint> {
    let mut visitor = InlayHintVisitor::new(db, file, range, settings);

    let ast = parsed_module(db, file).load(db);

    visitor.visit_body(ast.suite());

    visitor.hints
}

/// Settings to control the behavior of inlay hints.
#[derive(Clone, Debug)]
pub struct InlayHintSettings {
    /// Whether to show variable type hints.
    ///
    /// For example, this would enable / disable hints like the ones quoted below:
    /// ```python
    /// x": Literal[1]" = 1
    /// ```
    pub variable_types: bool,

    /// Whether to show call argument names.
    ///
    /// For example, this would enable / disable hints like the ones quoted below:
    /// ```python
    /// def foo(x: int): pass
    /// foo("x="1)
    /// ```
    pub call_argument_names: bool,
    // Add any new setting that enables additional inlays to `any_enabled`.
}

impl InlayHintSettings {
    pub fn any_enabled(&self) -> bool {
        self.variable_types || self.call_argument_names
    }
}

impl Default for InlayHintSettings {
    fn default() -> Self {
        Self {
            variable_types: true,
            call_argument_names: true,
        }
    }
}

struct InlayHintVisitor<'a, 'db> {
    db: &'db dyn Db,
    model: SemanticModel<'db>,
    hints: Vec<InlayHint>,
    in_assignment: bool,
    range: TextRange,
    settings: &'a InlayHintSettings,
}

impl<'a, 'db> InlayHintVisitor<'a, 'db> {
    fn new(db: &'db dyn Db, file: File, range: TextRange, settings: &'a InlayHintSettings) -> Self {
        Self {
            db,
            model: SemanticModel::new(db, file),
            hints: Vec::new(),
            in_assignment: false,
            range,
            settings,
        }
    }

    fn add_type_hint(&mut self, position: TextSize, ty: Type<'db>) {
        if !self.settings.variable_types {
            return;
        }
        self.hints
            .push(InlayHint::variable_type(position, ty, self.db));
    }

    fn add_call_argument_name(&mut self, position: TextSize, name: &str) {
        if !self.settings.call_argument_names {
            return;
        }

        if name.starts_with('_') {
            return;
        }

        self.hints
            .push(InlayHint::call_argument_name(position, name));
    }
}

impl SourceOrderVisitor<'_> for InlayHintVisitor<'_, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'_>) -> TraversalSignal {
        if self.range.intersect(node.range()).is_some() {
            TraversalSignal::Traverse
        } else {
            TraversalSignal::Skip
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        let node = AnyNodeRef::from(stmt);

        if !self.enter_node(node).is_traverse() {
            return;
        }

        match stmt {
            Stmt::Assign(assign) => {
                self.in_assignment = true;
                for target in &assign.targets {
                    self.visit_expr(target);
                }
                self.in_assignment = false;

                self.visit_expr(&assign.value);

                return;
            }
            Stmt::Expr(expr) => {
                self.visit_expr(&expr.value);
                return;
            }
            // TODO
            Stmt::FunctionDef(_) => {}
            Stmt::For(_) => {}
            _ => {}
        }

        source_order::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'_ Expr) {
        match expr {
            Expr::Name(name) => {
                if self.in_assignment {
                    if name.ctx.is_store() {
                        let ty = expr.inferred_type(&self.model);
                        self.add_type_hint(expr.range().end(), ty);
                    }
                }
                source_order::walk_expr(self, expr);
            }
            Expr::Attribute(attribute) => {
                if self.in_assignment {
                    if attribute.ctx.is_store() {
                        let ty = expr.inferred_type(&self.model);
                        self.add_type_hint(expr.range().end(), ty);
                    }
                }
                source_order::walk_expr(self, expr);
            }
            Expr::Call(call) => {
                let argument_names =
                    inlay_hint_function_argument_details(self.db, &self.model, call)
                        .map(|details| details.argument_names)
                        .unwrap_or_default();

                self.visit_expr(&call.func);

                for (index, arg_or_keyword) in call.arguments.arguments_source_order().enumerate() {
                    if let Some(name) = argument_names.get(&index) {
                        self.add_call_argument_name(arg_or_keyword.range().start(), name);
                    }
                    self.visit_expr(arg_or_keyword.value());
                }
            }
            _ => {
                source_order::walk_expr(self, expr);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use insta::assert_snapshot;
    use ruff_db::{
        Db as _,
        files::{File, system_path_to_file},
        source::source_text,
    };
    use ruff_python_trivia::textwrap::dedent;
    use ruff_text_size::TextSize;

    use ruff_db::system::{DbWithWritableSystem, SystemPathBuf};
    use ty_project::ProjectMetadata;
    use ty_python_semantic::{
        Program, ProgramSettings, PythonPlatform, PythonVersionWithSource, SearchPathSettings,
    };

    pub(super) fn inlay_hint_test(source: &str) -> InlayHintTest {
        const START: &str = "<START>";
        const END: &str = "<END>";

        let mut db = ty_project::TestDb::new(ProjectMetadata::new(
            "test".into(),
            SystemPathBuf::from("/"),
        ));

        let source = dedent(source);

        let start = source.find(START);
        let end = source
            .find(END)
            .map(|x| if start.is_some() { x - START.len() } else { x })
            .unwrap_or(source.len());

        let range = TextRange::new(
            TextSize::try_from(start.unwrap_or_default()).unwrap(),
            TextSize::try_from(end).unwrap(),
        );

        let source = source.replace(START, "");
        let source = source.replace(END, "");

        db.write_file("main.py", source)
            .expect("write to memory file system to be successful");

        let file = system_path_to_file(&db, "main.py").expect("newly written file to existing");

        let search_paths = SearchPathSettings::new(vec![SystemPathBuf::from("/")])
            .to_search_paths(db.system(), db.vendored())
            .expect("Valid search path settings");

        Program::from_settings(
            &db,
            ProgramSettings {
                python_version: PythonVersionWithSource::default(),
                python_platform: PythonPlatform::default(),
                search_paths,
            },
        );

        InlayHintTest { db, file, range }
    }

    pub(super) struct InlayHintTest {
        pub(super) db: ty_project::TestDb,
        pub(super) file: File,
        pub(super) range: TextRange,
    }

    impl InlayHintTest {
        /// Returns the inlay hints for the given test case.
        ///
        /// All inlay hints are generated using the applicable settings. Use
        /// [`inlay_hints_with_settings`] to generate hints with custom settings.
        ///
        /// [`inlay_hints_with_settings`]: Self::inlay_hints_with_settings
        fn inlay_hints(&self) -> String {
            self.inlay_hints_with_settings(&InlayHintSettings {
                variable_types: true,
                call_argument_names: true,
            })
        }

        /// Returns the inlay hints for the given test case with custom settings.
        fn inlay_hints_with_settings(&self, settings: &InlayHintSettings) -> String {
            let hints = inlay_hints(&self.db, self.file, self.range, settings);

            let mut buf = source_text(&self.db, self.file).as_str().to_string();

            let mut offset = 0;

            for hint in hints {
                let end_position = (hint.position.to_u32() as usize) + offset;
                let hint_str = format!("[{}]", hint.display());
                buf.insert_str(end_position, &hint_str);
                offset += hint_str.len();
            }

            buf
        }
    }

    #[test]
    fn test_assign_statement() {
        let test = inlay_hint_test("x = 1");

        assert_snapshot!(test.inlay_hints(), @r"
        x[: Literal[1]] = 1
        ");
    }

    #[test]
    fn test_tuple_assignment() {
        let test = inlay_hint_test("x, y = (1, 'abc')");

        assert_snapshot!(test.inlay_hints(), @r#"
        x[: Literal[1]], y[: Literal["abc"]] = (1, 'abc')
        "#);
    }

    #[test]
    fn test_nested_tuple_assignment() {
        let test = inlay_hint_test("x, (y, z) = (1, ('abc', 2))");

        assert_snapshot!(test.inlay_hints(), @r#"
        x[: Literal[1]], (y[: Literal["abc"]], z[: Literal[2]]) = (1, ('abc', 2))
        "#);
    }

    #[test]
    fn test_assign_statement_with_type_annotation() {
        let test = inlay_hint_test("x: int = 1");

        assert_snapshot!(test.inlay_hints(), @r"
        x: int = 1
        ");
    }

    #[test]
    fn test_assign_statement_out_of_range() {
        let test = inlay_hint_test("<START>x = 1<END>\ny = 2");

        assert_snapshot!(test.inlay_hints(), @r"
        x[: Literal[1]] = 1
        y = 2
        ");
    }

    #[test]
    fn test_assign_attribute_of_instance() {
        let test = inlay_hint_test(
            "
            class A:
                def __init__(self, y):
                    self.x = 1
                    self.y = y

            a = A(2)
            a.y = 3
            ",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class A:
            def __init__(self, y):
                self.x[: Literal[1]] = 1
                self.y[: Unknown] = y

        a[: A] = A([y=]2)
        a.y[: Literal[3]] = 3
        ");
    }

    #[test]
    fn test_disabled_variable_types() {
        let test = inlay_hint_test("x = 1");

        assert_snapshot!(
            test.inlay_hints_with_settings(&InlayHintSettings {
                variable_types: false,
                ..Default::default()
            }),
            @r"
        x = 1
        "
        );
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter() {
        let test = inlay_hint_test(
            "
            def foo(x: int): pass
            foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int): pass
        foo([x=]1)
        ");
    }

    #[test]
    fn test_function_call_with_positional_only_parameter() {
        let test = inlay_hint_test(
            "
            def foo(x: int, /): pass
            foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int, /): pass
        foo(1)
        ");
    }

    #[test]
    fn test_function_call_with_variadic_parameter() {
        let test = inlay_hint_test(
            "
            def foo(*args: int): pass
            foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(*args: int): pass
        foo(1)
        ");
    }

    #[test]
    fn test_function_call_with_keyword_variadic_parameter() {
        let test = inlay_hint_test(
            "
            def foo(**kwargs: int): pass
            foo(x=1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(**kwargs: int): pass
        foo(x=1)
        ");
    }

    #[test]
    fn test_function_call_with_keyword_only_parameter() {
        let test = inlay_hint_test(
            "
            def foo(*, x: int): pass
            foo(x=1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(*, x: int): pass
        foo(x=1)
        ");
    }

    #[test]
    fn test_function_call_positional_only_and_positional_or_keyword_parameters() {
        let test = inlay_hint_test(
            "
            def foo(x: int, /, y: int): pass
            foo(1, 2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int, /, y: int): pass
        foo(1, [y=]2)
        ");
    }

    #[test]
    fn test_function_call_positional_only_and_variadic_parameters() {
        let test = inlay_hint_test(
            "
            def foo(x: int, /, *args: int): pass
            foo(1, 2, 3)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int, /, *args: int): pass
        foo(1, 2, 3)
        ");
    }

    #[test]
    fn test_function_call_positional_only_and_keyword_variadic_parameters() {
        let test = inlay_hint_test(
            "
            def foo(x: int, /, **kwargs: int): pass
            foo(1, x=2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int, /, **kwargs: int): pass
        foo(1, x=2)
        ");
    }

    #[test]
    fn test_class_constructor_call_init() {
        let test = inlay_hint_test(
            "
            class Foo:
                def __init__(self, x: int): pass
            Foo(1)
            f = Foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class Foo:
            def __init__(self, x: int): pass
        Foo([x=]1)
        f[: Foo] = Foo([x=]1)
        ");
    }

    #[test]
    fn test_class_constructor_call_new() {
        let test = inlay_hint_test(
            "
            class Foo:
                def __new__(cls, x: int): pass
            Foo(1)
            f = Foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class Foo:
            def __new__(cls, x: int): pass
        Foo([x=]1)
        f[: Foo] = Foo([x=]1)
        ");
    }

    #[test]
    fn test_class_constructor_call_meta_class_call() {
        let test = inlay_hint_test(
            "
            class MetaFoo:
                def __call__(self, x: int): pass
            class Foo(metaclass=MetaFoo):
                pass
            Foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class MetaFoo:
            def __call__(self, x: int): pass
        class Foo(metaclass=MetaFoo):
            pass
        Foo([x=]1)
        ");
    }

    #[test]
    fn test_callable_call() {
        let test = inlay_hint_test(
            "
            from typing import Callable
            def foo(x: Callable[[int], int]):
                x(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        from typing import Callable
        def foo(x: Callable[[int], int]):
            x(1)
        ");
    }

    #[test]
    fn test_instance_method_call() {
        let test = inlay_hint_test(
            "
            class Foo:
                def bar(self, y: int): pass
            Foo().bar(2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class Foo:
            def bar(self, y: int): pass
        Foo().bar([y=]2)
        ");
    }

    #[test]
    fn test_class_method_call() {
        let test = inlay_hint_test(
            "
            class Foo:
                @classmethod
                def bar(cls, y: int): pass
            Foo.bar(2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class Foo:
            @classmethod
            def bar(cls, y: int): pass
        Foo.bar([y=]2)
        ");
    }

    #[test]
    fn test_static_method_call() {
        let test = inlay_hint_test(
            "
            class Foo:
                @staticmethod
                def bar(y: int): pass
            Foo.bar(2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class Foo:
            @staticmethod
            def bar(y: int): pass
        Foo.bar([y=]2)
        ");
    }

    #[test]
    fn test_function_call_with_union_type() {
        let test = inlay_hint_test(
            "
            def foo(x: int | str): pass
            foo(1)
            foo('abc')",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int | str): pass
        foo([x=]1)
        foo([x=]'abc')
        ");
    }

    #[test]
    fn test_function_call_multiple_positional_arguments() {
        let test = inlay_hint_test(
            "
            def foo(x: int, y: str, z: bool): pass
            foo(1, 'hello', True)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int, y: str, z: bool): pass
        foo([x=]1, [y=]'hello', [z=]True)
        ");
    }

    #[test]
    fn test_function_call_mixed_positional_and_keyword() {
        let test = inlay_hint_test(
            "
            def foo(x: int, y: str, z: bool): pass
            foo(1, z=True, y='hello')",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int, y: str, z: bool): pass
        foo([x=]1, z=True, y='hello')
        ");
    }

    #[test]
    fn test_function_call_with_default_parameters() {
        let test = inlay_hint_test(
            "
            def foo(x: int, y: str = 'default', z: bool = False): pass
            foo(1)
            foo(1, 'custom')
            foo(1, 'custom', True)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int, y: str = 'default', z: bool = False): pass
        foo([x=]1)
        foo([x=]1, [y=]'custom')
        foo([x=]1, [y=]'custom', [z=]True)
        ");
    }

    #[test]
    fn test_nested_function_calls() {
        let test = inlay_hint_test(
            "
            def foo(x: int) -> int:
                return x * 2

            def bar(y: str) -> str:
                return y

            def baz(a: int, b: str, c: bool): pass

            baz(foo(5), bar(bar('test')), True)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int) -> int:
            return x * 2

        def bar(y: str) -> str:
            return y

        def baz(a: int, b: str, c: bool): pass

        baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
        ");
    }

    #[test]
    fn test_method_chaining() {
        let test = inlay_hint_test(
            "
            class A:
                def foo(self, value: int) -> 'A':
                    return self
                def bar(self, name: str) -> 'A':
                    return self
                def baz(self): pass
            A().foo(42).bar('test').baz()",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class A:
            def foo(self, value: int) -> 'A':
                return self
            def bar(self, name: str) -> 'A':
                return self
            def baz(self): pass
        A().foo([value=]42).bar([name=]'test').baz()
        ");
    }

    #[test]
    fn test_nexted_keyword_function_calls() {
        let test = inlay_hint_test(
            "
            def foo(x: str) -> str:
                return x
            def bar(y: int): pass
            bar(y=foo('test'))
            ",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: str) -> str:
            return x
        def bar(y: int): pass
        bar(y=foo([x=]'test'))
        ");
    }

    #[test]
    fn test_lambda_function_calls() {
        let test = inlay_hint_test(
            "
            foo = lambda x: x * 2
            bar = lambda a, b: a + b
            foo(5)
            bar(1, 2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        foo[: (x) -> Unknown] = lambda x: x * 2
        bar[: (a, b) -> Unknown] = lambda a, b: a + b
        foo([x=]5)
        bar([a=]1, [b=]2)
        ");
    }

    #[test]
    fn test_complex_parameter_combinations() {
        let test = inlay_hint_test(
            "
            def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
            foo(1, 'pos', 3.14, False, e=42)
            foo(1, 'pos', 3.14, e=42, f='custom')",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
        foo(1, 'pos', [c=]3.14, [d=]False, e=42)
        foo(1, 'pos', [c=]3.14, e=42, f='custom')
        ");
    }

    #[test]
    fn test_generic_function_calls() {
        let test = inlay_hint_test(
            "
            from typing import TypeVar, Generic

            T = TypeVar('T')

            def identity(x: T) -> T:
                return x

            identity(42)
            identity('hello')",
        );

        assert_snapshot!(test.inlay_hints(), @r###"
        from typing import TypeVar, Generic

        T[: typing.TypeVar] = TypeVar([name=]'T')

        def identity(x: T) -> T:
            return x

        identity([x=]42)
        identity([x=]'hello')
        "###);
    }

    #[test]
    fn test_overloaded_function_calls() {
        let test = inlay_hint_test(
            "
            from typing import overload

            @overload
            def foo(x: int) -> str: ...
            @overload
            def foo(x: str) -> int: ...
            def foo(x):
                return x

            foo(42)
            foo('hello')",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        from typing import overload

        @overload
        def foo(x: int) -> str: ...
        @overload
        def foo(x: str) -> int: ...
        def foo(x):
            return x

        foo([x=]42)
        foo([x=]'hello')
        ");
    }

    #[test]
    fn test_disabled_function_argument_names() {
        let test = inlay_hint_test(
            "
        def foo(x: int): pass
        foo(1)",
        );

        assert_snapshot!(test.inlay_hints_with_settings(&InlayHintSettings {
            call_argument_names: false,
            ..Default::default()
        }), @r"
        def foo(x: int): pass
        foo(1)
        ");
    }

    #[test]
    fn test_function_call_out_of_range() {
        let test = inlay_hint_test(
            "
            <START>def foo(x: int): pass
            def bar(y: int): pass
            foo(1)<END>
            bar(2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int): pass
        def bar(y: int): pass
        foo([x=]1)
        bar(2)
        ");
    }

    #[test]
    fn test_function_call_with_argument_name_starting_with_underscore() {
        let test = inlay_hint_test(
            "
            def foo(_x: int, y: int): pass
            foo(1, 2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(_x: int, y: int): pass
        foo(1, [y=]2)
        ");
    }
}
