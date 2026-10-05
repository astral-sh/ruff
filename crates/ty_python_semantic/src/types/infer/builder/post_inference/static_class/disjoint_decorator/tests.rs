use std::cell::RefCell;
use std::task::Poll;

use ruff_db::Db as _;
use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextRange};
use ty_module_resolver::SearchPathSettings;
use ty_python_core::TestProgramDb;
use ty_python_core::program::{FallibleStrategy, Program};

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::signatures::effects::try_poll_immediate;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event<'db> {
    Next(usize),
    Expression(TextRange),
    Known(FunctionType<'db>),
    TypedDict(StaticClassLiteral<'db>, TextRange),
    Protocol(StaticClassLiteral<'db>, TextRange),
}

struct Recording<'a, 'db> {
    db: &'db TestDb,
    answers: &'a [(TextRange, Type<'db>)],
    events: RefCell<Vec<Event<'db>>>,
    reject_at: Option<usize>,
}

impl<'db> Recording<'_, 'db> {
    fn record(&self, event: Event<'db>) -> Result<(), &'static str> {
        let mut events = self.events.borrow_mut();
        let position = events.len();
        events.push(event);
        if self.reject_at == Some(position) {
            Err("refused")
        } else {
            Ok(())
        }
    }
}

impl<'db> SynchronousDisjointBaseDecoratorEffects<'db> for Recording<'_, 'db> {
    type Error = &'static str;

    fn next_decorator<'node>(
        &self,
        class_node: &'node ast::StmtClassDef,
        cursor: &mut usize,
    ) -> Result<Option<&'node ast::Decorator>, Self::Error> {
        self.record(Event::Next(*cursor))?;
        Ok(next_class_decorator(class_node, cursor))
    }

    fn expression_type(&self, expression: &ast::Expr) -> Result<Type<'db>, Self::Error> {
        self.record(Event::Expression(expression.range()))?;
        self.answers
            .iter()
            .find_map(|(range, ty)| (*range == expression.range()).then_some(*ty))
            .ok_or("unexpected decorator expression")
    }

    fn is_known_function(
        &self,
        function: FunctionType<'db>,
        known: KnownFunction,
    ) -> Result<bool, Self::Error> {
        assert_eq!(known, KnownFunction::DisjointBase);
        self.record(Event::Known(function))?;
        Ok(function.is_known(self.db, known))
    }

    fn report_typed_dict(
        &self,
        class: StaticClassLiteral<'db>,
        decorator: &ast::Decorator,
    ) -> Result<(), Self::Error> {
        self.record(Event::TypedDict(class, decorator.range()))
    }

    fn report_protocol(
        &self,
        class: StaticClassLiteral<'db>,
        decorator: &ast::Decorator,
    ) -> Result<(), Self::Error> {
        self.record(Event::Protocol(class, decorator.range()))
    }
}

impl<'db> DisjointBaseDecoratorEffects<'db> for Recording<'_, 'db> {
    type Error = &'static str;

    async fn next_decorator<'node>(
        &self,
        class_node: &'node ast::StmtClassDef,
        cursor: &mut usize,
    ) -> Result<Option<&'node ast::Decorator>, Self::Error> {
        SynchronousDisjointBaseDecoratorEffects::next_decorator(self, class_node, cursor)
    }

    async fn expression_type(&self, expression: &ast::Expr) -> Result<Type<'db>, Self::Error> {
        SynchronousDisjointBaseDecoratorEffects::expression_type(self, expression)
    }

    async fn is_known_function(
        &self,
        function: FunctionType<'db>,
        known: KnownFunction,
    ) -> Result<bool, Self::Error> {
        SynchronousDisjointBaseDecoratorEffects::is_known_function(self, function, known)
    }

    async fn report_typed_dict(
        &self,
        class: StaticClassLiteral<'db>,
        decorator: &ast::Decorator,
    ) -> Result<(), Self::Error> {
        SynchronousDisjointBaseDecoratorEffects::report_typed_dict(self, class, decorator)
    }

    async fn report_protocol(
        &self,
        class: StaticClassLiteral<'db>,
        decorator: &ast::Decorator,
    ) -> Result<(), Self::Error> {
        SynchronousDisjointBaseDecoratorEffects::report_protocol(self, class, decorator)
    }
}

#[test]
fn first_matching_decorator_keeps_diagnostic_priority_and_refusal_prefixes() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file(
            "/typeshed/stdlib/typing_extensions.pyi",
            "def disjoint_base(cls): ...\ndef final(cls): ...\n",
        )
        .with_file(
            "/typeshed/stdlib/VERSIONS",
            "builtins: 3.0-\ntyping_extensions: 3.0-\n",
        )
        .with_file("/typeshed/stdlib/builtins.pyi", "class object: ...\n")
        .with_file(
            "/src/decorators.py",
            r#"
from typing_extensions import disjoint_base, final

class Plain: ...

@0
@final
@disjoint_base
@disjoint_base
class Mixed: ...

@0
@final
class Unmatched: ...

class Empty: ...
"#,
        )
        .build()?;
    let search_paths = SearchPathSettings {
        custom_typeshed: Some("/typeshed".into()),
        ..SearchPathSettings::new(vec!["/src".into()])
    }
    .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)?;
    search_paths.try_register_static_roots(&db);
    let mut settings = db.program_settings().clone();
    settings.search_paths = search_paths;
    let program = Program::from_settings(&db, &settings);
    let file = program.program_file(&db, system_path_to_file(&db, "/src/decorators.py")?);
    let symbol = |name| {
        global_symbol(&db, file, name)
            .place
            .ignore_possibly_undefined()
            .ok_or_else(|| anyhow::anyhow!("missing {name}"))
    };
    let class = symbol("Plain")?
        .as_class_literal()
        .and_then(|class| class.as_static())
        .ok_or_else(|| anyhow::anyhow!("missing Plain class"))?;
    let other = symbol("final")?
        .as_function_literal()
        .ok_or_else(|| anyhow::anyhow!("missing final function"))?;
    let disjoint = symbol("disjoint_base")?
        .as_function_literal()
        .ok_or_else(|| anyhow::anyhow!("missing disjoint_base function"))?;
    let module = parsed_module(&db, file.python_file(&db)).load(&db);

    for (name, kind, is_protocol) in [
        ("Mixed", Some(CodeGeneratorKind::TypedDict), true),
        ("Mixed", None, true),
        ("Mixed", None, false),
        ("Unmatched", Some(CodeGeneratorKind::TypedDict), true),
        ("Empty", Some(CodeGeneratorKind::TypedDict), true),
    ] {
        let node = module
            .suite()
            .iter()
            .find_map(|statement| match statement {
                ast::Stmt::ClassDef(node) if node.name.as_str() == name => Some(node),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("missing {name} node"))?;
        let answers: Vec<_> = node
            .decorator_list
            .iter()
            .map(|decorator| decorator.expression.range())
            .zip([
                Type::int_literal(0),
                Type::FunctionLiteral(other),
                Type::FunctionLiteral(disjoint),
                Type::FunctionLiteral(disjoint),
            ])
            .collect();
        let expected = match node.decorator_list.as_slice() {
            [nonfunction, other_decorator, first, _] => {
                let mut events = vec![
                    Event::Next(0),
                    Event::Expression(nonfunction.expression.range()),
                    Event::Next(1),
                    Event::Expression(other_decorator.expression.range()),
                    Event::Known(other),
                    Event::Next(2),
                    Event::Expression(first.expression.range()),
                    Event::Known(disjoint),
                ];
                match (kind, is_protocol) {
                    (Some(CodeGeneratorKind::TypedDict), _) => {
                        events.push(Event::TypedDict(class, first.range()));
                    }
                    (None, true) => events.push(Event::Protocol(class, first.range())),
                    _ => {}
                }
                events
            }
            [nonfunction, other_decorator] => vec![
                Event::Next(0),
                Event::Expression(nonfunction.expression.range()),
                Event::Next(1),
                Event::Expression(other_decorator.expression.range()),
                Event::Known(other),
                Event::Next(2),
            ],
            [] => vec![Event::Next(0)],
            _ => anyhow::bail!("unexpected {name} decorators"),
        };
        for reject_at in (0..expected.len()).map(Some).chain([None]) {
            let recording = || Recording {
                db: &db,
                answers: &answers,
                events: RefCell::default(),
                reject_at,
            };
            let synchronous = recording();
            let asynchronous = recording();
            let result =
                check_disjoint_base_decorator_sync(class, node, kind, is_protocol, &synchronous);
            assert_eq!(result, reject_at.map_or(Ok(()), |_| Err("refused")));
            assert_eq!(
                try_poll_immediate(check_disjoint_base_decorator_with(
                    class,
                    node,
                    kind,
                    is_protocol,
                    &asynchronous,
                )),
                Poll::Ready(result),
            );
            let end = reject_at.map_or(expected.len(), |index| index + 1);
            assert_eq!(*synchronous.events.borrow(), expected[..end]);
            assert_eq!(*asynchronous.events.borrow(), expected[..end]);
        }
    }
    Ok(())
}
