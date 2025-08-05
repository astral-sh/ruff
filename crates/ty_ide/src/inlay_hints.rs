use crate::Db;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::fmt;
use std::fmt::Formatter;
use ty_python_semantic::types::{Type, inlay_hint_function_argument_details};
use ty_python_semantic::{HasType, SemanticModel};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InlayHint<'db> {
    pub position: TextSize,
    pub content: InlayHintContent<'db>,
}

impl<'db> InlayHint<'db> {
    pub const fn display(&self, db: &'db dyn Db) -> DisplayInlayHint<'_, 'db> {
        self.content.display(db)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum InlayHintContent<'db> {
    Type(Type<'db>),
    FunctionArgumentName(String),
}

impl<'db> InlayHintContent<'db> {
    pub const fn display(&self, db: &'db dyn Db) -> DisplayInlayHint<'_, 'db> {
        DisplayInlayHint { db, hint: self }
    }
}

pub struct DisplayInlayHint<'a, 'db> {
    db: &'db dyn Db,
    hint: &'a InlayHintContent<'db>,
}

impl fmt::Display for DisplayInlayHint<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.hint {
            InlayHintContent::Type(ty) => {
                write!(f, ": {}", ty.display(self.db))
            }
            InlayHintContent::FunctionArgumentName(name) => {
                write!(f, "{name}=")
            }
        }
    }
}

pub fn inlay_hints(db: &dyn Db, file: File, range: TextRange) -> Vec<InlayHint<'_>> {
    let mut visitor = InlayHintVisitor::new(db, file, range);

    let ast = parsed_module(db, file).load(db);

    visitor.visit_body(ast.suite());

    // Sort hints by position to ensure correct insertion order
    visitor.hints.sort_by_key(|hint| hint.position);

    visitor.hints
}

struct InlayHintVisitor<'db> {
    db: &'db dyn Db,
    model: SemanticModel<'db>,
    hints: Vec<InlayHint<'db>>,
    in_assignment: bool,
    range: TextRange,
}

impl<'db> InlayHintVisitor<'db> {
    fn new(db: &'db dyn Db, file: File, range: TextRange) -> Self {
        Self {
            db,
            model: SemanticModel::new(db, file),
            hints: Vec::new(),
            in_assignment: false,
            range,
        }
    }

    fn add_type_hint(&mut self, position: TextSize, ty: Type<'db>) {
        self.hints.push(InlayHint {
            position,
            content: InlayHintContent::Type(ty),
        });
    }

    fn add_function_argument_name(&mut self, position: TextSize, name: String) {
        self.hints.push(InlayHint {
            position,
            content: InlayHintContent::FunctionArgumentName(name),
        });
    }
}

impl SourceOrderVisitor<'_> for InlayHintVisitor<'_> {
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
            Expr::Call(call) => {
                if let Some(details) =
                    inlay_hint_function_argument_details(self.db, &self.model, call)
                {
                    for (position, name) in details.argument_names {
                        self.add_function_argument_name(position, name);
                    }
                }
                for arg in &call.arguments.args {
                    self.visit_expr(arg);
                }
                for kw in &call.arguments.keywords {
                    self.visit_expr(&kw.value);
                }
                self.visit_expr(&call.func);
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
        fn inlay_hints(&self) -> String {
            let hints = inlay_hints(&self.db, self.file, self.range);

            let mut buf = source_text(&self.db, self.file).as_str().to_string();

            let mut offset = 0;

            for hint in hints {
                let end_position = (hint.position.to_u32() as usize) + offset;
                let hint_str = format!("[{}]", hint.display(&self.db));
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
            "from typing import Callable\ndef _(x: Callable[[int], int]):\n    x(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        from typing import Callable
        def _(x: Callable[[int], int]):
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
    fn test_function_call_out_of_range() {
        let test = inlay_hint_test(
            "
            def foo(x: int): pass
            <START>foo(1)<END>
            bar(2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int): pass
        foo([x=]1)
        bar(2)
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
            def outer(x: int) -> int:
                return x * 2

            def inner(y: str) -> str:
                return y.upper()

            def process(a: int, b: str): pass

            process(outer(5), inner(inner('test')))",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def outer(x: int) -> int:
            return x * 2

        def inner(y: str) -> str:
            return y.upper()

        def process(a: int, b: str): pass

        process([a=]outer([x=]5), [b=]inner([y=]inner([y=]'test')))
        ");
    }

    #[test]
    fn test_method_chaining() {
        let test = inlay_hint_test(
            "
            class Builder:
                def with_value(self, value: int) -> 'Builder':
                    return self
                def with_name(self, name: str) -> 'Builder':
                    return self
                def build(self): pass
            Builder().with_value(42).with_name('test').build()",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class Builder:
            def with_value(self, value: int) -> 'Builder':
                return self
            def with_name(self, name: str) -> 'Builder':
                return self
            def build(self): pass
        Builder().with_value([value=]42).with_name([name=]'test').build()
        ");
    }

    #[test]
    fn test_lambda_function_calls() {
        let test = inlay_hint_test(
            "
            f = lambda x: x * 2
            g = lambda a, b: a + b
            f(5)
            g(1, 2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        f[: (x) -> Unknown] = lambda x: x * 2
        g[: (a, b) -> Unknown] = lambda a, b: a + b
        f([x=]5)
        g([a=]1, [b=]2)
        ");
    }

    #[test]
    fn test_builtin_function_calls() {
        let test = inlay_hint_test(
            "
            len([1, 2, 3])
            max(1, 2, 3)
            print('hello', 'world', sep=' ')",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        len([1, 2, 3])
        max(1, 2, 3)
        print('hello', 'world', sep=' ')
        ");
    }

    #[test]
    fn test_complex_parameter_combinations() {
        let test = inlay_hint_test(
            "
            def complex_func(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
            complex_func(1, 'pos', 3.14, False, e=42)
            complex_func(1, 'pos', 3.14, e=42, f='custom')",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def complex_func(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
        complex_func(1, 'pos', [c=]3.14, [d=]False, e=42)
        complex_func(1, 'pos', [c=]3.14, e=42, f='custom')
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

        assert_snapshot!(test.inlay_hints(), @r"
        from typing import TypeVar, Generic

        T[: typing.TypeVar] = TypeVar([name=]'T')

        def identity(x: T) -> T:
            return x

        identity([x=]42)
        identity([x=]'hello')
        ");
    }

    #[test]
    fn test_overloaded_function_calls() {
        let test = inlay_hint_test(
            "
            from typing import overload

            @overload
            def process(x: int) -> str: ...
            @overload
            def process(x: str) -> int: ...
            def process(x):
                return x
            
            process(42)
            process('hello')",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        from typing import overload

        @overload
        def process(x: int) -> str: ...
        @overload
        def process(x: str) -> int: ...
        def process(x):
            return x

        process([x=]42)
        process([x=]'hello')
        ");
    }
}
