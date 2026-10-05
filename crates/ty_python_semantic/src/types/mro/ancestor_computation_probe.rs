//! Test whether declaration plans retain specialization-dependent ancestor computations.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::Infallible;
use std::fmt::Write as _;
use std::io::Write as _;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ty_python_core::scope::ScopeId;

use super::base::{BaseMroStart, InlineBaseMroEffects, base_mro_start_sync};
use super::c3::{C3Work, InlineC3Effects, SynchronousC3Effects, capture_c3_sync};
use super::construction::{
    InlineStaticMroEffects, StaticMroFacts, StaticMroWork, SynchronousStaticMroEffects, sealed,
    static_mro_sync,
};
use super::field_reads::MroFieldReads;
use super::root::{InlineMroRootEffects, MroTailRequest, mro_first_sync, mro_tail_request_sync};
use super::{Mro, StaticMroError, StaticMroErrorKind};
use crate::db::tests::TestDbBuilder;
use crate::place::global_symbol;
use crate::types::class_base::ClassBase;
use crate::types::generics::Specialization;
use crate::types::{
    ApplyTypeMappingVisitor, ClassLiteral, ClassType, GenericAlias, MaterializationKind,
    StaticClassLiteral, Type,
};
use crate::{Db, ProgramEnvironment};

type SelectionCoordinates = Vec<(usize, usize)>;

fn capture_c3<'db>(
    db: &'db dyn Db,
    sequences: Vec<VecDeque<ClassBase<'db>>>,
) -> (Option<Mro<'db>>, SelectionCoordinates) {
    infallible(capture_c3_with(db, sequences, &InlineC3Effects))
}

fn capture_c3_with<'db, E: SynchronousC3Effects>(
    db: &'db dyn Db,
    sequences: Vec<VecDeque<ClassBase<'db>>>,
    effects: &E,
) -> Result<(Option<Mro<'db>>, SelectionCoordinates), E::Error> {
    capture_c3_sync(db, sequences, effects)
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PlanId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BaseId(usize);

#[derive(Clone, Copy, Debug)]
enum Entry<'db> {
    Root,
    Constant(ClassBase<'db>),
    First(BaseId),
    Tail(BaseId, usize),
    Direct(usize),
}

struct Base<'db> {
    declared: ClassBase<'db>,
    child: Option<PlanId>,
}

struct Plan<'db> {
    class: StaticClassLiteral<'db>,
    bases: Box<[Base<'db>]>,
    direct: Box<[ClassBase<'db>]>,
    entries: Box<[Entry<'db>]>,
    identities: Box<[Type<'db>]>,
}

#[derive(Default)]
struct Arena<'db> {
    plans: Vec<Plan<'db>>,
    by_class: HashMap<StaticClassLiteral<'db>, PlanId>,
    active: HashSet<StaticClassLiteral<'db>>,
}

struct Builder<'db> {
    db: &'db dyn Db,
    arena: RefCell<Arena<'db>>,
}

impl<'db> Builder<'db> {
    fn new(db: &'db dyn Db) -> Self {
        Self {
            db,
            arena: RefCell::default(),
        }
    }

    fn capture(&self, class: StaticClassLiteral<'db>) -> anyhow::Result<PlanId> {
        {
            let mut arena = self.arena.borrow_mut();
            if let Some(id) = arena.by_class.get(&class) {
                return Ok(*id);
            }
            anyhow::ensure!(
                arena.active.insert(class),
                "inheritance cycle is outside this experiment"
            );
        }
        let effects = CaptureEffects {
            builder: self,
            data: RefCell::default(),
        };
        let result = static_mro_sync(self.db, class, None, &effects);
        self.arena.borrow_mut().active.remove(&class);
        let mro = result?
            .map_err(|error| anyhow::anyhow!("unsupported declaration MRO error: {error:?}"))?;
        let data = effects.data.into_inner();
        let entries = match data.selected {
            Some(entries) => entries,
            None => {
                anyhow::ensure!(
                    class.explicit_bases(self.db).is_empty(),
                    "missing nontrivial recipe"
                );
                mro.iter()
                    .enumerate()
                    .map(|(index, base)| {
                        if index == 0 {
                            Entry::Root
                        } else {
                            Entry::Constant(*base)
                        }
                    })
                    .collect()
            }
        };
        anyhow::ensure!(
            entries.len() == mro.iter().len(),
            "recipe/output length differs"
        );
        let plan = Plan {
            class,
            bases: data.bases.into_boxed_slice(),
            direct: data.direct.into_boxed_slice(),
            entries: entries.into_boxed_slice(),
            identities: mro.iter().map(|base| base.mro_identity(self.db)).collect(),
        };
        let mut arena = self.arena.borrow_mut();
        let id = PlanId(arena.plans.len());
        arena.plans.push(plan);
        arena.by_class.insert(class, id);
        Ok(id)
    }
}

#[derive(Default)]
struct CaptureData<'db> {
    bases: Vec<Base<'db>>,
    sequences: Vec<Vec<Entry<'db>>>,
    direct: Vec<ClassBase<'db>>,
    selected: Option<Vec<Entry<'db>>>,
}

struct CaptureEffects<'a, 'db> {
    builder: &'a Builder<'db>,
    data: RefCell<CaptureData<'db>>,
}

impl<'db> CaptureEffects<'_, 'db> {
    fn collect(
        &self,
        env: &ProgramEnvironment<'db>,
        base: ClassBase<'db>,
    ) -> anyhow::Result<(VecDeque<ClassBase<'db>>, Vec<Entry<'db>>)> {
        let db = self.builder.db;
        let start = infallible(base_mro_start_sync(
            db,
            env,
            base,
            None,
            &InlineBaseMroEffects::new(db),
        ));
        let (values, entries, child) = match start {
            BaseMroStart::Length2(values) => (
                VecDeque::from(values),
                values.into_iter().map(Entry::Constant).collect(),
                None,
            ),
            BaseMroStart::Length3(values) => (
                VecDeque::from(values),
                values.into_iter().map(Entry::Constant).collect(),
                None,
            ),
            BaseMroStart::Class(start) => {
                let ClassLiteral::Static(class) = start.class else {
                    anyhow::bail!("dynamic class is outside this experiment")
                };
                let child = self.builder.capture(class)?;
                let root_effects = InlineMroRootEffects::new(db);
                let first = infallible(mro_first_sync(
                    db,
                    start.class,
                    start.specialization,
                    &root_effects,
                ));
                let MroTailRequest::Static(_, specialization) = infallible(mro_tail_request_sync(
                    db,
                    start.class,
                    start.specialization,
                    &root_effects,
                )) else {
                    anyhow::bail!("non-static tail")
                };
                let evaluated = self
                    .builder
                    .arena
                    .borrow()
                    .evaluate(db, child, specialization)?;
                let id = BaseId(self.data.borrow().bases.len());
                let mut entries = vec![Entry::First(id)];
                entries.extend((1..evaluated.mro.iter().len()).map(|index| Entry::Tail(id, index)));
                let mut values = VecDeque::from([first]);
                values.extend(evaluated.mro.iter().skip(1).copied());
                (values, entries, Some(child))
            }
        };
        self.data.borrow_mut().bases.push(Base {
            declared: base,
            child,
        });
        Ok((values, entries))
    }
}

impl sealed::Sealed for CaptureEffects<'_, '_> {}

impl<'db> StaticMroFacts<'db> for CaptureEffects<'_, 'db> {
    type Error = anyhow::Error;
}

impl<'db> SynchronousStaticMroEffects<'db> for CaptureEffects<'_, 'db> {
    fn body_scope(&self, class: StaticClassLiteral<'db>) -> anyhow::Result<ScopeId<'db>> {
        Ok(infallible(
            InlineStaticMroEffects::new(self.builder.db).body_scope(class),
        ))
    }
    fn is_object(&self, class: ClassType<'db>) -> anyhow::Result<bool> {
        Ok(infallible(
            InlineStaticMroEffects::new(self.builder.db).is_object(class),
        ))
    }
    fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> anyhow::Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>> {
        Ok(infallible(
            InlineStaticMroEffects::new(self.builder.db).static_class_literal(class),
        ))
    }

    fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> anyhow::Result<&[Type<'db>]> {
        Ok(class.explicit_bases(self.builder.db))
    }
    fn has_pep_695_type_params(&self, class: StaticClassLiteral<'db>) -> anyhow::Result<bool> {
        Ok(class.has_pep_695_type_params(self.builder.db))
    }
    fn converted_explicit_base(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        index: usize,
        ty: Type<'db>,
    ) -> anyhow::Result<Option<ClassBase<'db>>> {
        Ok(infallible(
            InlineStaticMroEffects::new(self.builder.db)
                .converted_explicit_base(env, class, index, ty),
        ))
    }
    fn object_base(&self, env: &ProgramEnvironment<'db>) -> anyhow::Result<ClassBase<'db>> {
        Ok(ClassBase::object(self.builder.db, env))
    }

    fn checkpoint(&self, _work: StaticMroWork) -> anyhow::Result<()> {
        Ok(())
    }
    fn root_class(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> anyhow::Result<ClassType<'db>> {
        Ok(infallible(
            InlineStaticMroEffects::new(self.builder.db).root_class(class, specialization),
        ))
    }
    fn static_mro_is_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> anyhow::Result<bool> {
        Ok(infallible(
            InlineStaticMroEffects::new(self.builder.db).static_mro_is_cycle(class, specialization),
        ))
    }
    fn collect_single_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        root: ClassType<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> anyhow::Result<Mro<'db>> {
        anyhow::ensure!(additional.is_none(), "capture must use declaration context");
        let (values, entries) = self.collect(env, base)?;
        self.data.borrow_mut().selected =
            Some(std::iter::once(Entry::Root).chain(entries).collect());
        Ok(Mro::from(
            std::iter::once(ClassBase::Class(root))
                .chain(values)
                .collect::<Vec<_>>(),
        ))
    }
    fn collect_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> anyhow::Result<VecDeque<ClassBase<'db>>> {
        anyhow::ensure!(additional.is_none(), "capture must use declaration context");
        let (values, entries) = self.collect(env, base)?;
        self.data.borrow_mut().sequences.push(entries);
        Ok(values)
    }
    fn specialize_base(
        &self,
        base: ClassBase<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> anyhow::Result<ClassBase<'db>> {
        anyhow::ensure!(
            specialization.is_none(),
            "capture must use declaration context"
        );
        self.data.borrow_mut().direct.push(base);
        Ok(base.apply_optional_specialization(self.builder.db, specialization))
    }
    fn c3_merge(
        &self,
        sequences: Vec<VecDeque<ClassBase<'db>>>,
    ) -> anyhow::Result<Option<Mro<'db>>> {
        let mut data = self.data.borrow_mut();
        let mut recipes = vec![vec![Entry::Root]];
        recipes.append(&mut data.sequences);
        recipes.push((0..data.direct.len()).map(Entry::Direct).collect());
        anyhow::ensure!(
            sequences.len() == recipes.len()
                && sequences
                    .iter()
                    .zip(&recipes)
                    .all(|(a, b)| a.len() == b.len()),
            "C3 occurrence labels differ from inputs"
        );
        let (result, coordinates) = capture_c3(self.builder.db, sequences);
        if result.is_some() {
            data.selected = Some(
                coordinates
                    .into_iter()
                    .map(|(sequence, head)| recipes[sequence][head])
                    .collect(),
            );
        }
        Ok(result)
    }
    fn make_error(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        kind: StaticMroErrorKind<'db>,
    ) -> anyhow::Result<StaticMroError<'db>> {
        Ok(infallible(
            InlineStaticMroEffects::new(self.builder.db).make_error(env, class, kind),
        ))
    }
    fn failed_c3(
        &self,
        env: &ProgramEnvironment<'db>,
        class_literal: StaticClassLiteral<'db>,
        class: ClassType<'db>,
        original_bases: &[Type<'db>],
        resolved_bases: &[ClassBase<'db>],
    ) -> anyhow::Result<Result<Mro<'db>, StaticMroError<'db>>> {
        Ok(infallible(
            InlineStaticMroEffects::new(self.builder.db).failed_c3(
                env,
                class_literal,
                class,
                original_bases,
                resolved_bases,
            ),
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Binding<'db> {
    plan: PlanId,
    input: Option<Specialization<'db>>,
}

struct PreparedBase<'db> {
    first: ClassBase<'db>,
    tail: Option<Binding<'db>>,
}
struct Frame<'db> {
    binding: Binding<'db>,
    root: ClassBase<'db>,
    bases: Vec<PreparedBase<'db>>,
}
#[derive(Default, Debug)]
struct Counts {
    frames: usize,
    base_starts: usize,
    direct_applications: usize,
    shared_children: usize,
    max_stack: usize,
}
struct Evaluation<'db> {
    mro: Mro<'db>,
    counts: Counts,
}

impl<'db> Arena<'db> {
    fn evaluate(
        &self,
        db: &'db dyn Db,
        plan: PlanId,
        input: Option<Specialization<'db>>,
    ) -> anyhow::Result<Evaluation<'db>> {
        let root = Binding { plan, input };
        let mut complete: HashMap<Binding<'db>, Mro<'db>> = HashMap::new();
        let root_effects = InlineMroRootEffects::new(db);
        let make_frame = |binding: Binding<'db>| Frame {
            binding,
            root: infallible(mro_first_sync(
                db,
                self.plans[binding.plan.0].class.into(),
                binding.input,
                &root_effects,
            )),
            bases: Vec::new(),
        };
        let mut stack = vec![make_frame(root)];
        let mut counts = Counts {
            frames: 1,
            ..Counts::default()
        };
        while !stack.is_empty() {
            counts.max_stack = counts.max_stack.max(stack.len());
            let last = stack.len() - 1;
            let frame = &mut stack[last];
            let plan = &self.plans[frame.binding.plan.0];
            if let Some(tail) = frame.bases.last().and_then(|base| base.tail)
                && !complete.contains_key(&tail)
            {
                stack.push(make_frame(tail));
                counts.frames += 1;
                continue;
            }
            if let Some(base) = plan.bases.get(frame.bases.len()) {
                let env = ProgramEnvironment::from_scope(plan.class.body_scope(db));
                let start = infallible(base_mro_start_sync(
                    db,
                    &env,
                    base.declared,
                    frame.binding.input,
                    &InlineBaseMroEffects::new(db),
                ));
                counts.base_starts += 1;
                let prepared = match start {
                    BaseMroStart::Length2(values) => PreparedBase {
                        first: values[0],
                        tail: None,
                    },
                    BaseMroStart::Length3(values) => PreparedBase {
                        first: values[0],
                        tail: None,
                    },
                    BaseMroStart::Class(start) => {
                        let first = infallible(mro_first_sync(
                            db,
                            start.class,
                            start.specialization,
                            &root_effects,
                        ));
                        let MroTailRequest::Static(_, input) = infallible(mro_tail_request_sync(
                            db,
                            start.class,
                            start.specialization,
                            &root_effects,
                        )) else {
                            anyhow::bail!("non-static evaluation tail")
                        };
                        let Some(child) = base.child else {
                            anyhow::bail!("missing child plan")
                        };
                        let tail = Binding { plan: child, input };
                        if complete.contains_key(&tail) {
                            counts.shared_children += 1;
                        }
                        PreparedBase {
                            first,
                            tail: Some(tail),
                        }
                    }
                };
                frame.bases.push(prepared);
                continue;
            }
            let direct: Vec<_> = plan
                .direct
                .iter()
                .map(|base| base.apply_optional_specialization(db, frame.binding.input))
                .collect();
            counts.direct_applications += direct.len();
            let mut output = Vec::with_capacity(plan.entries.len());
            for entry in &plan.entries {
                output.push(match *entry {
                    Entry::Root => frame.root,
                    Entry::Constant(value) => value,
                    Entry::First(base) => frame.bases[base.0].first,
                    Entry::Tail(base, index) => {
                        let Some(tail) = frame.bases[base.0]
                            .tail
                            .and_then(|binding| complete.get(&binding))
                        else {
                            anyhow::bail!("child has not completed")
                        };
                        let Some(value) = tail.get(index) else {
                            anyhow::bail!("child length changed")
                        };
                        *value
                    }
                    Entry::Direct(index) => direct[index],
                });
            }
            anyhow::ensure!(
                output
                    .iter()
                    .map(|base| base.mro_identity(db))
                    .eq(plan.identities.iter().copied()),
                "specialization changed ordering identities"
            );
            complete.insert(frame.binding, Mro::from(output));
            stack.pop();
        }
        let expected_starts: usize = complete
            .keys()
            .map(|binding| self.plans[binding.plan.0].bases.len())
            .sum();
        let expected_direct: usize = complete
            .keys()
            .map(|binding| self.plans[binding.plan.0].direct.len())
            .sum();
        anyhow::ensure!(
            counts.base_starts == expected_starts
                && counts.direct_applications == expected_direct
                && counts.frames == complete.len(),
            "binding work was repeated"
        );
        let Some(mro) = complete.remove(&root) else {
            anyhow::bail!("root did not complete")
        };
        Ok(Evaluation { mro, counts })
    }
}

fn compare_fixture(source: &str, label: &str, require_sharing: bool) -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/ancestor_plan.py", source)
        .build()?;
    let file = system_path_to_file(&db, "/src/ancestor_plan.py")?;
    let target = |name: &str| -> anyhow::Result<GenericAlias<'_>> {
        let ty = global_symbol(&db, db.program_file(file), name)
            .place
            .expect_type();
        let Type::GenericAlias(alias) = ty else {
            anyhow::bail!("{label}: {name} is not an alias: {ty:?}")
        };
        Ok(alias)
    };
    let first = target("target")?;
    let second = target("other")?;
    anyhow::ensure!(
        first.origin(&db) == second.origin(&db),
        "targets must share one declaration"
    );
    let builder = Builder::new(&db);
    let root = builder.capture(first.origin(&db))?;
    let arena = builder.arena.into_inner();
    let plan_count = arena.plans.len();
    let env = db.program_environment();
    let render = |mro: &Mro<'_>| {
        mro.iter()
            .map(|base| Type::from(*base).display(&db, &env).to_string())
            .collect::<Vec<_>>()
    };
    let mut inputs = vec![
        None,
        Some(first.specialization(&db)),
        Some(second.specialization(&db)),
    ];
    for kind in [MaterializationKind::Top, MaterializationKind::Bottom] {
        inputs.push(Some(first.specialization(&db).materialize_impl(
            &db,
            kind,
            &ApplyTypeMappingVisitor::new(&env),
        )));
    }
    let mut all_shared = true;
    for input in inputs {
        let candidate = arena.evaluate(&db, root, input)?;
        let original = first
            .origin(&db)
            .try_mro(&db, input)
            .map_err(|error| anyhow::anyhow!("{label}: oracle failed: {error:?}"))?;
        anyhow::ensure!(
            *original == candidate.mro,
            "{label} input={input:?}: original={:?}; candidate={:?}",
            render(original),
            render(&candidate.mro)
        );
        all_shared &= candidate.counts.shared_children > 0;
        writeln!(
            std::io::stderr(),
            "MATCH {label}: input={input:?} plans={plan_count} entries={} base_operands={} counts={:?}",
            arena
                .plans
                .iter()
                .map(|plan| plan.entries.len())
                .sum::<usize>(),
            arena
                .plans
                .iter()
                .map(|plan| plan.bases.len())
                .sum::<usize>(),
            candidate.counts
        )?;

        // The iterator has a separate first entry and normalizes only its tail request.
        let effects = InlineMroRootEffects::new(&db);
        let first_entry = infallible(mro_first_sync(
            &db,
            first.origin(&db).into(),
            input,
            &effects,
        ));
        let MroTailRequest::Static(_, tail_input) = infallible(mro_tail_request_sync(
            &db,
            first.origin(&db).into(),
            input,
            &effects,
        )) else {
            anyhow::bail!("non-static iterator")
        };
        let tail = arena.evaluate(&db, root, tail_input)?;
        let candidate_iterator: Vec<_> = std::iter::once(first_entry)
            .chain(tail.mro.iter().skip(1).copied())
            .collect();
        let original_iterator: Vec<_> =
            super::MroIterator::new(&db, first.origin(&db).into(), input).collect();
        anyhow::ensure!(
            original_iterator == candidate_iterator,
            "{label}: iterator differs"
        );
    }
    anyhow::ensure!(
        !require_sharing || all_shared,
        "{label}: shared child binding was not exercised"
    );
    Ok(())
}

#[test]
fn ordered_ancestor_computations_preserve_specializations() -> anyhow::Result<()> {
    let cases = [
        (
            "root",
            "class Root[T]: ...\ntarget = Root[int]\nother = Root[str]",
        ),
        (
            "composition",
            "class Base[T]: ...\nclass Middle[U](Base[list[U]]): ...\nclass Root[V](Middle[set[V]]): ...\ntarget = Root[int]\nother = Root[str]",
        ),
        (
            "variadic tuple",
            "class Root[*Ts](tuple[*Ts]): ...\ntarget = Root[int, str]\nother = Root[bytes, bool, float]",
        ),
        (
            "forwarded variadic tuple",
            "class Base[*Ts](tuple[*Ts]): ...\nclass Mid[*Ts](Base[*Ts]): ...\nclass Root[*Us](Mid[int, *Us]): ...\ntarget = Root[str, bool]\nother = Root[bytes]",
        ),
        (
            "right diamond",
            "class Anc[T]: ...\nclass Left[T](Anc[list[T]]): ...\nclass Right[T](Anc[set[T]]): ...\nclass Mid[U](Right[dict[str, U]], Left[U]): ...\nclass Root[V](Mid[list[V]]): ...\ntarget = Root[int]\nother = Root[bytes]",
        ),
        (
            "left diamond",
            "class Anc[T]: ...\nclass Left[T](Anc[list[T]]): ...\nclass Right[T](Anc[set[T]]): ...\nclass Mid[U](Left[U], Right[dict[str, U]]): ...\nclass Root[V](Mid[list[V]]): ...\ntarget = Root[int]\nother = Root[bytes]",
        ),
        (
            "default root context",
            "class Base[T]: ...\nclass Root[T = int](Base[T]): ...\ntarget = Root[str]\nother = Root[bytes]",
        ),
        (
            "dependent defaults",
            "class Base[T, U]: ...\nclass Root[T = int, U = list[T]](Base[T, U]): ...\ntarget = Root[str]\nother = Root[bytes]",
        ),
        (
            "fixed tuple",
            "class Root[T](tuple[T, int]): ...\ntarget = Root[str]\nother = Root[bytes]",
        ),
        (
            "paramspec",
            "class Base[**P]: ...\nclass Root[**P](Base[P]): ...\ntarget = Root[[int, str]]\nother = Root[[bytes]]",
        ),
        (
            "explicit Any",
            "from typing import Any\nclass Base[T](Any): ...\nclass Root[T](Base[T]): ...\ntarget = Root[int]\nother = Root[Any]",
        ),
        (
            "materialized Any",
            "from typing import Any\nclass Base[T]: ...\nclass Middle[T](Base[list[T]]): ...\nclass Root[T](Middle[T]): ...\ntarget = Root[Any]\nother = Root[int]",
        ),
        (
            "legacy composition",
            "from typing import Generic, TypeVar\nT = TypeVar('T')\nU = TypeVar('U')\nV = TypeVar('V')\nclass Base(Generic[T]): ...\nclass Middle(Base[list[U]]): ...\nclass Root(Middle[set[V]]): ...\ntarget = Root[int]\nother = Root[str]",
        ),
        (
            "legacy diamond",
            "from typing import Generic, TypeVar\nT = TypeVar('T')\nclass Base(Generic[T]): ...\nclass Left(Base[list[T]]): ...\nclass Right(Base[set[T]]): ...\nclass Root(Left[T], Right[T]): ...\ntarget = Root[int]\nother = Root[str]",
        ),
    ];
    let mut failures = Vec::new();
    for (label, source) in cases {
        if let Err(error) = compare_fixture(source, label, false) {
            failures.push(format!("{label}: {error:#}"));
        }
    }
    anyhow::ensure!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

#[test]
fn repeated_diamonds_share_exact_child_bindings() -> anyhow::Result<()> {
    for depth in [2, 4, 8, 16] {
        let mut source = String::from("class Base0[T]: ...\n");
        for level in 1..=depth {
            let previous = level - 1;
            write!(
                source,
                "class Left{level}[T](Base{previous}[T]): ...\nclass Right{level}[T](Base{previous}[T]): ...\nclass Base{level}[T](Left{level}[T], Right{level}[T]): ...\n"
            )?;
        }
        write!(
            source,
            "target = Base{depth}[int]\nother = Base{depth}[str]\n"
        )?;
        compare_fixture(&source, &format!("shared diamond depth {depth}"), true)?;
    }
    Ok(())
}

#[test]
fn deep_chains_keep_flat_plan_ownership() -> anyhow::Result<()> {
    for depth in [4, 8, 16, 32] {
        let mut source = String::from("class Base0[T]: ...\n");
        for level in 1..=depth {
            let previous = level - 1;
            writeln!(source, "class Base{level}[T](Base{previous}[T]): ...")?;
        }
        write!(
            source,
            "target = Base{depth}[int]\nother = Base{depth}[str]\n"
        )?;
        compare_fixture(&source, &format!("chain depth {depth}"), false)?;
    }
    Ok(())
}

#[test]
fn c3_coordinates_retain_occurrences_and_original_offsets() -> anyhow::Result<()> {
    let db = TestDbBuilder::new().build()?;
    let object = ClassBase::object(&db, &db.program_environment());
    let sequences = vec![
        VecDeque::new(),
        VecDeque::from([ClassBase::Generic, object]),
        VecDeque::from([ClassBase::Generic, object]),
        VecDeque::from([object]),
    ];
    let (mro, coordinates) = capture_c3(&db, sequences.clone());
    assert_eq!(mro, super::c3_merge(&db, sequences));
    assert_eq!(coordinates, [(1, 0), (1, 1)]);
    let (_, coordinates) = capture_c3(
        &db,
        vec![
            VecDeque::new(),
            VecDeque::from([ClassBase::Generic]),
            VecDeque::from([ClassBase::Any, object]),
            VecDeque::from([object]),
        ],
    );
    assert_eq!(coordinates, [(1, 0), (2, 0), (2, 1)]);
    assert!(infallible(InlineC3Effects.start_occurrences(1)).is_none());
    Ok(())
}

struct RejectAppend;
impl super::c3::sealed::Sealed for RejectAppend {}
impl SynchronousC3Effects for RejectAppend {
    type Error = ();
    fn mro_identity<'db>(
        &self,
        fields: MroFieldReads<'db>,
        base: ClassBase<'db>,
    ) -> Result<Type<'db>, ()> {
        Ok(fields.mro_identity(base))
    }
    fn checkpoint(&self, work: C3Work) -> Result<(), ()> {
        if matches!(work, C3Work::OutputAppend { .. }) {
            Err(())
        } else {
            Ok(())
        }
    }
}

#[test]
fn c3_partial_and_nested_observations_do_not_escape() -> anyhow::Result<()> {
    let db = TestDbBuilder::new().build()?;
    let one = vec![VecDeque::from([ClassBase::Generic])];
    assert_eq!(capture_c3_with(&db, one.clone(), &RejectAppend), Err(()));
    let (failed, trace) = capture_c3(
        &db,
        vec![
            VecDeque::from([ClassBase::Any]),
            VecDeque::from([ClassBase::Generic, ClassBase::Protocol]),
            VecDeque::from([ClassBase::Protocol, ClassBase::Generic]),
        ],
    );
    assert!(failed.is_none());
    assert!(trace.is_empty());
    struct NestedCapture<'db> {
        db: &'db dyn Db,
        inner: RefCell<Option<SelectionCoordinates>>,
    }
    impl super::c3::sealed::Sealed for NestedCapture<'_> {}
    impl SynchronousC3Effects for NestedCapture<'_> {
        type Error = Infallible;

        fn mro_identity<'db>(
            &self,
            fields: MroFieldReads<'db>,
            base: ClassBase<'db>,
        ) -> Result<Type<'db>, Infallible> {
            Ok(fields.mro_identity(base))
        }

        fn checkpoint(&self, work: C3Work) -> Result<(), Infallible> {
            if work == C3Work::SelectedIdentity && self.inner.borrow().is_none() {
                let (_, inner) = capture_c3(
                    self.db,
                    vec![VecDeque::new(), VecDeque::from([ClassBase::Protocol])],
                );
                *self.inner.borrow_mut() = Some(inner);
            }
            Ok(())
        }
    }

    let nested = NestedCapture {
        db: &db,
        inner: RefCell::default(),
    };
    let (_, outer) = infallible(capture_c3_with(&db, one, &nested));
    assert_eq!(outer, [(0, 0)]);
    assert_eq!(nested.inner.into_inner(), Some(vec![(1, 0)]));
    Ok(())
}

#[test]
fn non_generic_fast_paths_retain_root_and_base_recipes() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file(
            "/src/plain_mro.py",
            "class Base: ...\nclass Root(Base): ...\n",
        )
        .build()?;
    let file = system_path_to_file(&db, "/src/plain_mro.py")?;
    let ty = global_symbol(&db, db.program_file(file), "Root")
        .place
        .expect_type();
    let Type::ClassLiteral(ClassLiteral::Static(class)) = ty else {
        anyhow::bail!("expected static class")
    };
    let builder = Builder::new(&db);
    let root = builder.capture(class)?;
    let ClassBase::Class(object) = ClassBase::object(&db, &db.program_environment()) else {
        anyhow::bail!("expected object class")
    };
    let Some((object, None)) = object.static_class_literal(&db) else {
        anyhow::bail!("expected static object")
    };
    builder.capture(object)?;
    let arena = builder.arena.into_inner();
    assert_eq!(arena.plans[root.0].bases.len(), 1);
    assert!(matches!(arena.plans[root.0].entries[0], Entry::Root));
    assert!(matches!(arena.plans[root.0].entries[1], Entry::First(_)));
    let evaluated = arena.evaluate(&db, root, None)?;
    assert_eq!(Ok(&evaluated.mro), class.try_mro(&db, None));
    assert!(arena.plans.iter().any(|plan| plan.entries.len() == 1));
    assert!(arena.plans.iter().any(|plan| plan.entries.len() == 2));
    Ok(())
}

#[test]
fn equal_declaration_payloads_retain_distinct_tuple_paths() -> anyhow::Result<()> {
    let db = TestDbBuilder::new().with_python_version(PythonVersion::PY313).with_file(
        "/src/collision.py", "class Left[*Ts](tuple[*Ts]): ...\nclass Right[*Ts](tuple[int, *Ts]): ...\nleft = Left[str, bytes]\nright = Right[str, bytes]\n",
    ).build()?;
    let file = system_path_to_file(&db, "/src/collision.py")?;
    let alias = |name| -> anyhow::Result<GenericAlias<'_>> {
        let ty = global_symbol(&db, db.program_file(file), name)
            .place
            .expect_type();
        let Type::GenericAlias(alias) = ty else {
            anyhow::bail!("expected alias")
        };
        Ok(alias)
    };
    let left = alias("left")?;
    let right = alias("right")?;
    let left_declaration = left
        .origin(&db)
        .try_mro(&db, None)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let right_declaration = right
        .origin(&db)
        .try_mro(&db, None)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let left_specialized = left.try_mro(&db).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let right_specialized = right.try_mro(&db).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    assert!(
        left_declaration.iter().any(|original| {
            right_declaration.iter().any(|other| original == other) && {
                let identity = original.mro_identity(&db);
                let specialized_left = left_specialized
                    .iter()
                    .find(|base| base.mro_identity(&db) == identity);
                let specialized_right = right_specialized
                    .iter()
                    .find(|base| base.mro_identity(&db) == identity);
                specialized_left.is_some()
                    && specialized_right.is_some()
                    && specialized_left != specialized_right
            }
        }),
        "fixture must make equal declaration payloads diverge after specialization"
    );
    for parents in ["Left[*Ts], Right[*Ts]", "Right[*Ts], Left[*Ts]"] {
        let source = format!(
            "class Left[*Ts](tuple[*Ts]): ...\nclass Right[*Ts](tuple[int, *Ts]): ...\nclass Root[*Ts]({parents}): ...\ntarget = Root[str, bytes]\nother = Root[float]\n"
        );
        compare_fixture(&source, &format!("equal tuple ancestors: {parents}"), false)?;
    }
    Ok(())
}
