use std::cell::RefCell;
use std::convert::Infallible;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use salsa::Database as _;
use ty_python_core::ProgramFile;

use super::{
    ClassContextBaseCursor, ClassContextEffects, ClassContextWork, InlineClassContextEffects,
    generic_context_with, legacy_generic_context_with, sealed,
};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::{
    ClassLiteral, GenericContext, KnownClass, KnownInstanceType, StaticClassLiteral, Type,
};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/context.py",
            r#"
from typing import Generic, Protocol, TypeVar

T = TypeVar("T")
class Plain: ...
class Pep[U]: ...
class Legacy(Generic[T]): ...
class LegacyProtocol(Protocol[T]): ...
class Inherited(Legacy[T]): ...
class Explicit(Legacy[T], Generic[T]): ...
"#,
        )
        .build()
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    let env = db.program_environment();
    if name == "VersionInfo" {
        return KnownClass::VersionInfo
            .try_to_class_literal(db, &env)
            .ok_or_else(|| anyhow::anyhow!("missing VersionInfo class"));
    }
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/context.py")?,
        env.program(db),
    );
    global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing static class {name}"))
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

fn original_legacy_context<'db>(bases: &[Type<'db>]) -> Option<GenericContext<'db>> {
    bases.iter().find_map(|base| match base {
        Type::KnownInstance(
            KnownInstanceType::SubscriptedGeneric(context)
            | KnownInstanceType::SubscriptedProtocol(context),
        ) => Some(*context),
        _ => None,
    })
}

fn original_context<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Option<GenericContext<'db>> {
    if class.is_known(db, KnownClass::VersionInfo) {
        return None;
    }
    class
        .pep695_generic_context(db)
        .or_else(|| original_legacy_context(class.explicit_bases(db)))
        .or_else(|| class.inherited_legacy_generic_context(db))
}

#[test]
fn ordinary_selection_and_legacy_scan_preserve_raw_contexts() -> anyhow::Result<()> {
    let db = database()?;
    let effects = InlineClassContextEffects::new(&db);
    for name in [
        "Plain",
        "Pep",
        "Legacy",
        "LegacyProtocol",
        "Inherited",
        "Explicit",
        "VersionInfo",
    ] {
        let class = class(&db, name)?;
        let expected = original_context(&db, class);
        assert_eq!(
            infallible(generic_context_with(&db, class, &effects)),
            expected,
            "{name}"
        );
        assert_eq!(
            class.generic_context(&db),
            expected,
            "{name}: ordinary owner"
        );
        assert_eq!(
            expected.is_some(),
            !matches!(name, "Plain" | "VersionInfo"),
            "{name}"
        );
        assert_eq!(
            infallible(legacy_generic_context_with(class, &effects)),
            original_legacy_context(class.explicit_bases(&db)),
            "{name}",
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Read {
    Pep695,
    ExplicitBases,
    Inherited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Work(ClassContextWork),
    Before(Read),
    After(Read),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Refused(usize);

struct Recording<'db> {
    inline: InlineClassContextEffects<'db>,
    bases: Option<&'db [Type<'db>]>,
    events: RefCell<Vec<Event>>,
    refusal: Option<usize>,
}

impl<'db> Recording<'db> {
    fn new(db: &'db dyn Db, refusal: Option<usize>) -> Self {
        Self {
            inline: InlineClassContextEffects::new(db),
            bases: None,
            events: RefCell::new(Vec::new()),
            refusal,
        }
    }
}

impl Recording<'_> {
    fn record(&self, event: Event) -> Result<(), Refused> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.refusal == Some(index) {
            Err(Refused(index))
        } else {
            Ok(())
        }
    }

    fn read<T>(&self, read: Read, source: impl FnOnce() -> T) -> Result<T, Refused> {
        self.record(Event::Before(read))?;
        let result = source();
        self.record(Event::After(read))?;
        Ok(result)
    }
}

impl sealed::Sealed for Recording<'_> {}

impl<'db> ClassContextEffects<'db> for Recording<'db> {
    type Error = Refused;

    fn checkpoint(&self, work: ClassContextWork) -> Result<(), Refused> {
        self.record(Event::Work(work))
    }

    fn is_version_info(&self, class: StaticClassLiteral<'db>) -> Result<bool, Refused> {
        Ok(infallible(self.inline.is_version_info(class)))
    }

    fn pep695_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Refused> {
        self.read(Read::Pep695, || {
            infallible(self.inline.pep695_generic_context(class))
        })
    }

    fn legacy_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Refused> {
        legacy_generic_context_with(class, self)
    }

    fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&'db [Type<'db>], Refused> {
        self.read(Read::ExplicitBases, || {
            self.bases
                .unwrap_or_else(|| infallible(self.inline.explicit_bases(class)))
        })
    }

    fn next_base(
        &self,
        cursor: &mut ClassContextBaseCursor<'db>,
    ) -> Result<Option<(usize, Type<'db>)>, Refused> {
        Ok(cursor.next_base())
    }

    fn inherited_legacy_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Refused> {
        self.read(Read::Inherited, || {
            infallible(self.inline.inherited_legacy_generic_context(class))
        })
    }
}

#[test]
fn selection_shortcuts_and_explicit_precedence_preserve_read_order() -> anyhow::Result<()> {
    let db = database()?;
    for (name, expected_reads, scans) in [
        ("VersionInfo", vec![], 0),
        ("Pep", vec![Read::Pep695], 0),
        ("Legacy", vec![Read::Pep695, Read::ExplicitBases], 1),
        ("LegacyProtocol", vec![Read::Pep695, Read::ExplicitBases], 1),
        ("Explicit", vec![Read::Pep695, Read::ExplicitBases], 2),
        (
            "Inherited",
            vec![Read::Pep695, Read::ExplicitBases, Read::Inherited],
            1,
        ),
        (
            "Plain",
            vec![Read::Pep695, Read::ExplicitBases, Read::Inherited],
            0,
        ),
    ] {
        let class = class(&db, name)?;
        let effects = Recording::new(&db, None);
        assert_eq!(
            generic_context_with(&db, class, &effects),
            Ok(original_context(&db, class)),
            "{name}"
        );
        let events = effects.events.into_inner();
        let reads: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Before(read) => Some(*read),
                _ => None,
            })
            .collect();
        assert_eq!(reads, expected_reads, "{name}");
        let entries: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Work(ClassContextWork::LegacyBase { index }) => Some(*index),
                _ => None,
            })
            .collect();
        assert_eq!(entries, (0..scans).collect::<Vec<_>>(), "{name}");
        if name == "VersionInfo" {
            assert_eq!(events, [Event::Work(ClassContextWork::Select)]);
        }
    }
    Ok(())
}

#[test]
fn legacy_scan_admits_each_entry_and_stops_at_the_first_generic_or_protocol() -> anyhow::Result<()>
{
    let db = database()?;
    let literal = class(&db, "Plain")?;
    let first = original_context(&db, class(&db, "Pep")?)
        .ok_or_else(|| anyhow::anyhow!("missing first context"))?;
    let second = original_context(&db, class(&db, "Legacy")?)
        .ok_or_else(|| anyhow::anyhow!("missing second context"))?;
    assert_ne!(first, second);
    for first_base in [
        KnownInstanceType::SubscriptedGeneric(first),
        KnownInstanceType::SubscriptedProtocol(first),
    ] {
        let bases = [
            Type::unknown(),
            Type::int_literal(1),
            Type::KnownInstance(first_base),
            Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(second)),
        ];
        let mut effects = Recording::new(&db, None);
        effects.bases = Some(&bases);
        assert_eq!(
            legacy_generic_context_with(literal, &effects),
            Ok(Some(first))
        );
        assert_eq!(original_legacy_context(&bases), Some(first));
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Work(ClassContextWork::ExplicitBases),
                Event::Before(Read::ExplicitBases),
                Event::After(Read::ExplicitBases),
                Event::Work(ClassContextWork::LegacyBase { index: 0 }),
                Event::Work(ClassContextWork::LegacyBase { index: 1 }),
                Event::Work(ClassContextWork::LegacyBase { index: 2 }),
            ]
        );
    }
    Ok(())
}

#[test]
fn refusal_at_every_work_and_read_boundary_suppresses_later_selection() -> anyhow::Result<()> {
    let db = database()?;
    for name in ["VersionInfo", "Pep", "Explicit", "Inherited", "Plain"] {
        let class = class(&db, name)?;
        let baseline = Recording::new(&db, None);
        assert_eq!(
            generic_context_with(&db, class, &baseline),
            Ok(original_context(&db, class))
        );
        let expected = baseline.events.into_inner();
        for index in 0..expected.len() {
            let effects = Recording::new(&db, Some(index));
            assert_eq!(
                generic_context_with(&db, class, &effects),
                Err(Refused(index)),
                "{name}: {index}"
            );
            assert_eq!(
                *effects.events.borrow(),
                expected[..=index],
                "{name}: {index}"
            );
        }
    }
    Ok(())
}

fn executions(db: &TestDb) -> Vec<String> {
    db.clone()
        .take_salsa_events()
        .into_iter()
        .filter_map(|event| {
            let salsa::EventKind::WillExecute { database_key } = event.kind else {
                return None;
            };
            Some(
                db.ingredient_debug_name(database_key.ingredient_index())
                    .into_owned(),
            )
        })
        .collect()
}

fn cold_reads(name: &str, shared: bool) -> anyhow::Result<(bool, Vec<String>)> {
    let db = database()?;
    let class = class(&db, name)?;
    executions(&db);
    let context = if shared {
        infallible(generic_context_with(
            &db,
            class,
            &InlineClassContextEffects::new(&db),
        ))
    } else {
        original_context(&db, class)
    };
    Ok((context.is_some(), executions(&db)))
}

#[test]
fn independent_cold_source_reads_match_before_result_comparison() -> anyhow::Result<()> {
    for name in [
        "Plain",
        "Pep",
        "Legacy",
        "LegacyProtocol",
        "Inherited",
        "Explicit",
        "VersionInfo",
    ] {
        let (expected, expected_reads) = cold_reads(name, false)?;
        let (actual, actual_reads) = cold_reads(name, true)?;
        assert_eq!(actual_reads, expected_reads, "{name}");
        if name == "VersionInfo" {
            assert!(actual_reads.is_empty());
        }
        if name == "Inherited" {
            assert!(
                actual_reads
                    .iter()
                    .any(|name| name.contains("inherited_legacy_generic_context_inner")),
                "missing cold inherited query: {actual_reads:?}"
            );
        }
        assert_eq!(actual, expected, "{name}");
    }
    Ok(())
}
