use std::cell::RefCell;
use std::collections::VecDeque;
use std::convert::Infallible;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ty_python_core::scope::ScopeId;

use super::static_error_details_with;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class_base::ClassBase;
use crate::types::generics::Specialization;
use crate::types::mro::construction::{
    InlineStaticMroEffects, StaticMroFacts, StaticMroWork, SynchronousStaticMroEffects, sealed,
};
use crate::types::mro::{Mro, StaticMroError, StaticMroErrorKind};
use crate::types::{ClassLiteral, ClassType, StaticClassLiteral, Type};
use crate::{Db, ProgramEnvironment};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/mro_errors.pyi",
            r#"
from typing import Generic, TypeVar
K = TypeVar("K")
V = TypeVar("V")
class Reorder(Generic[K, V], dict): ...
"#,
        )
        .build()
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

fn accepted<T>(result: Result<T, Refused>) -> anyhow::Result<T> {
    result.map_err(|error| anyhow::anyhow!("unexpected refusal: {error:?}"))
}

struct Inputs<'db> {
    db: &'db TestDb,
    literal: StaticClassLiteral<'db>,
    class: ClassType<'db>,
    original: &'db [Type<'db>],
    resolved: Vec<ClassBase<'db>>,
}

impl<'db> Inputs<'db> {
    fn new(db: &'db TestDb, name: &str) -> anyhow::Result<Self> {
        let file = system_path_to_file(db, "/src/mro_errors.pyi")?;
        let literal = global_symbol(db, db.program_file(file), name)
            .place
            .expect_type()
            .as_class_literal()
            .and_then(ClassLiteral::as_static)
            .ok_or_else(|| anyhow::anyhow!("missing static class {name}"))?;
        let original = literal.explicit_bases(db);
        let env = db.program_environment();
        let resolved = original
            .iter()
            .map(|ty| {
                ClassBase::try_from_explicit_base(db, &env, *ty, Some(literal.into()))
                    .ok_or_else(|| anyhow::anyhow!("invalid fixture base: {ty:?}"))
            })
            .collect::<anyhow::Result<_>>()?;
        Ok(Self {
            db,
            literal,
            class: literal.apply_optional_specialization(db, None),
            original,
            resolved,
        })
    }

    fn run(
        &self,
        effects: &RecordingEffects<'db>,
    ) -> Result<Result<Mro<'db>, StaticMroError<'db>>, Refused> {
        static_error_details_with(
            self.db,
            &self.db.program_environment(),
            self.literal,
            self.class,
            self.original,
            &self.resolved,
            effects,
        )
    }

    fn classification_calls(&self) -> Vec<Event<'db>> {
        let mut events = vec![Event::Pep695(self.literal)];
        events.extend(
            self.original
                .iter()
                .enumerate()
                .map(|(index, ty)| Event::Convert(self.literal, index, *ty)),
        );
        events
    }

    fn assert_fallback(&self, mro: &Mro<'db>) {
        assert_eq!(
            &mro[..],
            [
                ClassBase::Class(self.class),
                ClassBase::unknown(),
                ClassBase::object(self.db, &self.db.program_environment()),
            ],
        );
    }

    fn assert_unresolvable(&self, error: &StaticMroError<'db>, generic_index: Option<usize>) {
        assert_eq!(
            error.reason(),
            &StaticMroErrorKind::UnresolvableMro {
                bases_list: self.original.into(),
                generic_index,
            },
        );
        self.assert_fallback(error.fallback_mro());
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Event<'db> {
    Checkpoint(StaticMroWork),
    Pep695(StaticClassLiteral<'db>),
    Convert(StaticClassLiteral<'db>, usize, Type<'db>),
    Object,
    Cycle(StaticClassLiteral<'db>, Option<Specialization<'db>>),
    Collect(ClassBase<'db>, Option<Specialization<'db>>),
    C3(Vec<VecDeque<ClassBase<'db>>>),
    Error(ClassType<'db>),
    Other(&'static str),
}

#[derive(Debug, Eq, PartialEq)]
struct Refused(usize);

struct RecordingEffects<'db> {
    inline: InlineStaticMroEffects<'db>,
    events: RefCell<Vec<Event<'db>>>,
    rejected_event: Option<usize>,
}

impl<'db> RecordingEffects<'db> {
    fn new(db: &'db dyn Db) -> Self {
        Self {
            inline: InlineStaticMroEffects::new(db),
            events: RefCell::default(),
            rejected_event: None,
        }
    }

    fn record(&self, event: Event<'db>) -> Result<(), Refused> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.rejected_event == Some(index) {
            Err(Refused(index))
        } else {
            Ok(())
        }
    }

    fn calls(&self) -> Vec<Event<'db>> {
        self.events
            .borrow()
            .iter()
            .filter(|event| !matches!(event, Event::Checkpoint(_)))
            .cloned()
            .collect()
    }
}

impl sealed::Sealed for RecordingEffects<'_> {}

impl<'db> StaticMroFacts<'db> for RecordingEffects<'db> {
    type Error = Refused;
}

impl<'db> SynchronousStaticMroEffects<'db> for RecordingEffects<'db> {
    fn body_scope(&self, class: StaticClassLiteral<'db>) -> Result<ScopeId<'db>, Refused> {
        Ok(infallible(self.inline.body_scope(class)))
    }

    fn is_object(&self, class: ClassType<'db>) -> Result<bool, Refused> {
        Ok(infallible(self.inline.is_object(class)))
    }

    fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Refused> {
        Ok(infallible(self.inline.static_class_literal(class)))
    }

    fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&[Type<'db>], Refused> {
        self.record(Event::Other("explicit bases"))?;
        Ok(infallible(self.inline.explicit_bases(class)))
    }

    fn has_pep_695_type_params(&self, class: StaticClassLiteral<'db>) -> Result<bool, Refused> {
        self.record(Event::Pep695(class))?;
        Ok(infallible(self.inline.has_pep_695_type_params(class)))
    }

    fn converted_explicit_base(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        index: usize,
        ty: Type<'db>,
    ) -> Result<Option<ClassBase<'db>>, Refused> {
        self.record(Event::Convert(class, index, ty))?;
        Ok(infallible(
            self.inline.converted_explicit_base(env, class, index, ty),
        ))
    }

    fn object_base(&self, env: &ProgramEnvironment<'db>) -> Result<ClassBase<'db>, Refused> {
        self.record(Event::Object)?;
        Ok(infallible(self.inline.object_base(env)))
    }

    fn checkpoint(&self, work: StaticMroWork) -> Result<(), Refused> {
        self.record(Event::Checkpoint(work))?;
        infallible(self.inline.checkpoint(work));
        Ok(())
    }

    fn root_class(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassType<'db>, Refused> {
        self.record(Event::Other("root"))?;
        Ok(infallible(self.inline.root_class(class, specialization)))
    }

    fn static_mro_is_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<bool, Refused> {
        self.record(Event::Cycle(class, specialization))?;
        Ok(infallible(
            self.inline.static_mro_is_cycle(class, specialization),
        ))
    }

    fn collect_single_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        root: ClassType<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<Mro<'db>, Refused> {
        self.record(Event::Other("single collection"))?;
        Ok(infallible(
            self.inline
                .collect_single_base_mro(env, root, base, additional),
        ))
    }

    fn collect_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<VecDeque<ClassBase<'db>>, Refused> {
        self.record(Event::Collect(base, additional))?;
        Ok(infallible(
            self.inline.collect_base_mro(env, base, additional),
        ))
    }

    fn specialize_base(
        &self,
        base: ClassBase<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassBase<'db>, Refused> {
        self.record(Event::Other("specialization"))?;
        Ok(infallible(
            self.inline.specialize_base(base, specialization),
        ))
    }

    fn c3_merge(
        &self,
        sequences: Vec<VecDeque<ClassBase<'db>>>,
    ) -> Result<Option<Mro<'db>>, Refused> {
        self.record(Event::C3(sequences.clone()))?;
        Ok(infallible(self.inline.c3_merge(sequences)))
    }

    fn make_error(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        kind: StaticMroErrorKind<'db>,
    ) -> Result<StaticMroError<'db>, Refused> {
        self.record(Event::Error(class))?;
        Ok(infallible(self.inline.make_error(env, class, kind)))
    }

    fn failed_c3(
        &self,
        env: &ProgramEnvironment<'db>,
        class_literal: StaticClassLiteral<'db>,
        class: ClassType<'db>,
        original_bases: &[Type<'db>],
        resolved_bases: &[ClassBase<'db>],
    ) -> Result<Result<Mro<'db>, StaticMroError<'db>>, Refused> {
        self.record(Event::Other("error details"))?;
        Ok(infallible(self.inline.failed_c3(
            env,
            class_literal,
            class,
            original_bases,
            resolved_bases,
        )))
    }
}

fn cycle_event<'db>(db: &'db dyn Db, base: ClassBase<'db>) -> anyhow::Result<Event<'db>> {
    if let ClassBase::Class(class) = base
        && let Some((literal, specialization)) = class.static_class_literal(db)
    {
        Ok(Event::Cycle(literal, specialization))
    } else {
        anyhow::bail!("expected a static fixture base: {base:?}")
    }
}

fn merge_event<'db>(db: &'db TestDb, bases: &[ClassBase<'db>]) -> Event<'db> {
    let inline = InlineStaticMroEffects::new(db);
    let mut sequences: Vec<_> = bases
        .iter()
        .map(|base| infallible(inline.collect_base_mro(&db.program_environment(), *base, None)))
        .collect();
    sequences.push(bases.iter().copied().collect());
    Event::C3(sequences)
}

#[test]
fn generic_reorder_preserves_dependency_order_and_propagates_refusal() -> anyhow::Result<()> {
    let db = database()?;
    let inputs = Inputs::new(&db, "Reorder")?;
    let baseline = RecordingEffects::new(&db);
    let Err(error) = accepted(inputs.run(&baseline))? else {
        anyhow::bail!("conflicting bases produced a successful MRO");
    };
    inputs.assert_unresolvable(&error, Some(0));
    let dict = inputs.resolved[1];
    let mut expected = inputs.classification_calls();
    expected.extend([
        cycle_event(&db, dict)?,
        Event::Collect(dict, None),
        Event::Collect(ClassBase::Generic, None),
        merge_event(&db, &[dict, ClassBase::Generic]),
        Event::Error(inputs.class),
    ]);
    assert_eq!(baseline.calls(), expected);

    let baseline = baseline.events.into_inner();
    for index in 0..baseline.len() {
        let mut effects = RecordingEffects::new(&db);
        effects.rejected_event = Some(index);
        assert!(
            matches!(inputs.run(&effects), Err(Refused(actual)) if actual == index),
            "swallowed refusal at event {index}",
        );
        assert_eq!(
            effects.events.into_inner(),
            baseline[..=index],
            "performed a later dependency after refusal at event {index}",
        );
    }
    Ok(())
}
