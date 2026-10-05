use std::cell::RefCell;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;

use super::*;
use crate::Db;
use crate::ProgramEnvironment;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::signatures::effects::try_poll_immediate;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event<'db> {
    Step(&'static str),
    Type(usize),
    Remember(StaticClassLiteral<'db>, usize, Type<'db>, usize),
    Report(usize, usize),
}

struct Recording<'a, 'db> {
    db: &'db TestDb,
    env: &'a ProgramEnvironment<'db>,
    events: RefCell<Vec<Event<'db>>>,
    refuse_at: Option<usize>,
}

impl<'db> Recording<'_, 'db> {
    fn record(&self, event: Event<'db>) -> Result<(), &'static str> {
        let mut events = self.events.borrow_mut();
        let position = events.len();
        events.push(event);
        if self.refuse_at == Some(position) {
            Err("refused")
        } else {
            Ok(())
        }
    }
}

// Ordinary mdtests cover diagnostic rendering. This matrix checks that both drivers retain
// the first concrete argument and stop at the same effect when any dependency refuses.
macro_rules! recording_effects {
    ($(
        fn $name:ident($this:ident $(, $argument:ident: $ty:ty)*) -> $output:ty $body:block
    )*) => {
        impl<'db> SynchronousGenericBaseCheckEffects<'db> for Recording<'_, 'db> {
            type Error = &'static str;
            type Ancestors<'state> = ExplicitClassAncestors<'state, 'db> where Self: 'state;
            $(fn $name(&$this $(, $argument: $ty)*) -> Result<$output, Self::Error> $body)*

            fn ancestors_next<'state>(&'state self, cursor: &mut Self::Ancestors<'state>) -> Result<Option<ClassType<'db>>, Self::Error> {
                self.record(Event::Step("ancestor"))?;
                Ok(cursor.next())
            }
        }

        impl<'db> GenericBaseCheckEffects<'db> for Recording<'_, 'db> {
            type Error = &'static str;
            type Ancestors<'state> = ExplicitClassAncestors<'state, 'db> where Self: 'state;
            $(async fn $name(&$this $(, $argument: $ty)*) -> Result<$output, Self::Error> {
                SynchronousGenericBaseCheckEffects::$name($this $(, $argument)*)
            })*

            async fn ancestors_next<'state>(&'state self, cursor: &mut Self::Ancestors<'state>) -> Result<Option<ClassType<'db>>, Self::Error> {
                SynchronousGenericBaseCheckEffects::ancestors_next(self, cursor)
            }
        }
    };
}

recording_effects! {
    fn empty_constraints(self) -> GenericBaseConstraints<'db> {
        self.record(Event::Step("empty"))?;
        Ok(GenericBaseConstraints::default())
    }
    fn next_type(self, types: &[Type<'db>], cursor: &mut usize) -> Option<(usize, Type<'db>)> {
        self.record(Event::Type(*cursor))?;
        Ok(next_generic_base_type(types, cursor))
    }
    fn has_generic_context(self, class: ClassLiteral<'db>) -> bool {
        self.record(Event::Step("context"))?;
        Ok(class.generic_context(self.db).is_some())
    }
    fn ancestors_start(self, class: ClassType<'db>) -> Self::Ancestors<'_> {
        self.record(Event::Step("ancestors"))?;
        Ok(class.iter_explicit_ancestors(self.db, self.env))
    }
    fn origin(self, alias: GenericAlias<'db>) -> StaticClassLiteral<'db> {
        self.record(Event::Step("origin"))?;
        Ok(alias.origin(self.db))
    }
    fn arguments(self, alias: GenericAlias<'db>) -> &'db [Type<'db>] {
        self.record(Event::Step("arguments"))?;
        Ok(alias.specialization(self.db).types(self.db))
    }
    fn is_dynamic(self, argument: Type<'db>) -> bool {
        self.record(Event::Step("dynamic"))?;
        Ok(argument.is_dynamic())
    }
    fn remember_argument(self, constraints: &mut GenericBaseConstraints<'db>, origin: StaticClassLiteral<'db>, parameter_index: usize, current: GenericBaseConstraint<'db>) -> GenericBaseConstraint<'db> {
        self.record(Event::Remember(origin, parameter_index, current.argument, current.base_index))?;
        Ok(*constraints.entry(GenericBaseParameter { origin, parameter_index }).or_insert(current))
    }
    fn same_argument(self, earlier: GenericBaseConstraint<'db>, argument: Type<'db>) -> bool {
        self.record(Event::Step("same argument"))?;
        Ok(earlier.has_argument(argument))
    }
    fn same_base(self, earlier: GenericBaseConstraint<'db>, base_index: usize) -> bool {
        self.record(Event::Step("same base"))?;
        Ok(earlier.has_base(base_index))
    }
    fn report_conflict(self, _header_range: TextRange, _base_nodes: Option<&[ast::Expr]>, _base: Type<'db>, _origin: StaticClassLiteral<'db>, earlier: GenericBaseConstraint<'db>, later: GenericBaseConstraint<'db>) -> () {
        self.record(Event::Report(earlier.base_index, later.base_index))
    }
}

#[test]
fn first_concrete_argument_and_refusal_order() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/generic_bases.py",
            r#"
from typing import Any
class Base[T, U]: ...
class Other[T, U]: ...
class Plain: ...
Gradual = Base[int, Any]
Text = Base[int, str]
Bytes = Base[int, bytes]
OtherText = Other[int, str]
class Inherited(Text, Bytes): ...
"#,
        )
        .build()?;
    let env = db.program_environment();
    let file = db.program_file(system_path_to_file(&db, "/src/generic_bases.py")?);
    let ty = |name| global_symbol(&db, file, name).place.expect_type();
    let base = ty("Base");
    let plain = ty("Plain");
    let gradual = ty("Gradual");
    let text = ty("Text");
    let bytes = ty("Bytes");
    let other = ty("OtherText");
    let inherited = ty("Inherited");
    let range = TextRange::default();
    let cases = [
        (vec![], false, None),
        (vec![base, Type::unknown()], false, None),
        (vec![plain], false, None),
        (vec![gradual, text], false, None),
        (vec![other, text], false, None),
        (vec![gradual, text, bytes], true, Some((1, 2))),
        (vec![bytes, gradual, text], true, Some((0, 2))),
        (vec![inherited], true, None),
    ];
    for (bases, inconsistent, report) in cases {
        let recording = |refuse_at| Recording {
            db: &db,
            env: &env,
            events: RefCell::default(),
            refuse_at,
        };
        let baseline = recording(None);
        assert_eq!(
            report_inconsistent_generic_bases_sync(range, &bases, None, &baseline),
            Ok(inconsistent)
        );
        let expected = baseline.events.into_inner();
        let reports: Vec<_> = expected
            .iter()
            .filter_map(|event| match event {
                Event::Report(earlier, later) => Some((*earlier, *later)),
                _ => None,
            })
            .collect();
        assert_eq!(reports, report.into_iter().collect::<Vec<_>>());
        if bases == [gradual, text, bytes] {
            let remembered: Vec<_> = expected
                .iter()
                .filter_map(|event| match event {
                    Event::Remember(_, parameter, _, base) => Some((*base, *parameter)),
                    _ => None,
                })
                .collect();
            assert_eq!(remembered, [(0, 0), (1, 0), (1, 1), (2, 0), (2, 1)]);
        }
        for refuse_at in (0..expected.len()).map(Some).chain([None]) {
            let synchronous = recording(refuse_at);
            let asynchronous = recording(refuse_at);
            let sync_result =
                report_inconsistent_generic_bases_sync(range, &bases, None, &synchronous);
            let Poll::Ready(async_result) = try_poll_immediate(
                report_inconsistent_generic_bases_with(range, &bases, None, &asynchronous),
            ) else {
                anyhow::bail!("recording effects unexpectedly suspended");
            };
            let result = refuse_at.map_or(Ok(inconsistent), |_| Err("refused"));
            assert_eq!(sync_result, result);
            assert_eq!(async_result, result);
            let end = refuse_at.map_or(expected.len(), |index| index + 1);
            assert_eq!(*synchronous.events.borrow(), expected[..end]);
            assert_eq!(*asynchronous.events.borrow(), expected[..end]);
        }
    }
    Ok(())
}
