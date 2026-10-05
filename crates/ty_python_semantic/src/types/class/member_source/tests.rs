use std::cell::{Cell, RefCell};
use std::future::Future;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use salsa::plumbing::AsId;
use salsa::prepared_source_probe;
use ty_python_core::ProgramFile;

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::{Provenance, global_symbol, place_from_declarations};
use crate::types::KnownClass;
use crate::types::class::implicit_attributes::{
    ImplicitNameSearchControl, implicit_attribute_bindings_with, implicit_attribute_names,
    implicit_name_index_with,
};
use crate::types::class::{
    code_generator_of_static_class, code_generator_of_static_class_ingredient,
    implicit_attribute_names_ingredient, static_code_generator_with,
};
use crate::types::signatures::effects::try_poll_immediate;

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file(
            "/src/leaves.py",
            r#"
from typing import ClassVar
from dataclasses import InitVar, KW_ONLY

def choose() -> bool: ...

class Plain:
    present: int
    bound: int = 1
    cv: ClassVar[int]
    init: InitVar[int]
    marker: KW_ONLY
    if choose():
        maybe: int
    def method(self):
        self.alpha = 1
        self.middle = 2
        self.omega = 3
        self.éclair = 4
        self.雪 = 5
"#,
        )
        .build()
}
fn class<'db>(db: &'db TestDb) -> anyhow::Result<StaticClassLiteral<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/leaves.py")?,
        env.program(db),
    );
    global_symbol(db, file, "Plain")
        .place
        .ignore_possibly_undefined()
        .and_then(Type::as_class_literal)
        .and_then(|class| class.as_static())
        .ok_or_else(|| anyhow::anyhow!("missing Plain class"))
}
fn finish<T>(future: impl Future<Output = Result<T, &'static str>>) -> anyhow::Result<T> {
    match try_poll_immediate(future) {
        Poll::Ready(result) => result.map_err(anyhow::Error::msg),
        Poll::Pending => anyhow::bail!("recording source effect unexpectedly suspended"),
    }
}
fn place(ty: Type<'_>, origin: TypeOrigin, definedness: Definedness) -> Place<'_> {
    Place::Defined(DefinedPlace {
        ty,
        origin,
        definedness,
        public_type_policy: PublicTypePolicy::Raw,
        provenance: Provenance::Unknown,
    })
}

// These recording effects exercise the new continuation API with actual declaration iterators.
// Supplied semantic child values are explicit controls, not cold member-query evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceEvent {
    Work(MemberSourceWork),
    Call(&'static str),
}

struct Recording<'db> {
    db: &'db TestDb,
    calls: RefCell<Vec<&'static str>>,
    work: RefCell<Vec<MemberSourceWork>>,
    reject: Option<usize>,
    reject_work: Option<usize>,
    events: RefCell<Vec<SourceEvent>>,
    generator: Option<CodeGeneratorKind<'db>>,
    named_field: bool,
    implicit: Member<'db>,
    binding: Option<Place<'db>>,
    public: Option<PlaceAndQualifiers<'db>>,
    kw_only: bool,
    stub: bool,
    slot: bool,
    dataclass_field: bool,
    getter: PlaceAndQualifiers<'db>,
    unions: RefCell<Vec<(Type<'db>, Type<'db>)>>,
}
impl<'db> Recording<'db> {
    fn new(db: &'db TestDb) -> Self {
        Self {
            db,
            calls: RefCell::new(Vec::new()),
            work: RefCell::new(Vec::new()),
            reject: None,
            reject_work: None,
            events: RefCell::new(Vec::new()),
            generator: None,
            named_field: false,
            implicit: Member::unbound(),
            binding: None,
            public: None,
            kw_only: false,
            stub: false,
            slot: false,
            dataclass_field: false,
            getter: Place::Undefined.into(),
            unions: RefCell::new(Vec::new()),
        }
    }
    fn record(&self, call: &'static str) -> Result<(), &'static str> {
        let mut calls = self.calls.borrow_mut();
        calls.push(call);
        self.events.borrow_mut().push(SourceEvent::Call(call));
        if self.reject == Some(calls.len() - 1) {
            Err("refused")
        } else {
            Ok(())
        }
    }
}
impl sealed::Sealed for Recording<'_> {}
impl<'db> MemberSourceEffects<'db> for Recording<'db> {
    type Error = &'static str;
    async fn checkpoint(&self, work: MemberSourceWork) -> Result<(), Self::Error> {
        let mut seen = self.work.borrow_mut();
        seen.push(work);
        self.events.borrow_mut().push(SourceEvent::Work(work));
        if self.reject_work == Some(seen.len() - 1) {
            Err("refused")
        } else {
            Ok(())
        }
    }
    async fn place_table(&self, scope: ScopeId<'db>) -> Result<&'db PlaceTable, Self::Error> {
        self.record("table")?;
        Ok(place_table(self.db, scope))
    }
    async fn use_def_map(&self, scope: ScopeId<'db>) -> Result<&'db UseDefMap<'db>, Self::Error> {
        self.record("use_def")?;
        Ok(use_def_map(self.db, scope))
    }
    async fn symbol_id(
        &self,
        table: &'db PlaceTable,
        name: &str,
    ) -> Result<Option<ScopedSymbolId>, Self::Error> {
        self.record("symbol")?;
        Ok(table.symbol_id(name))
    }
    async fn binding_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        bindings: BindingWithConstraintsIterator<'map, 'db>,
    ) -> Result<PlaceWithDefinition<'db>, Self::Error> {
        self.record("bindings")?;
        Ok(match self.binding {
            Some(place) => PlaceWithDefinition {
                place,
                first_definition: None,
            },
            None => place_from_bindings(self.db, env, bindings),
        })
    }
}
impl<'db> RawClassMemberEffects<'db> for Recording<'db> {
    async fn public_class_place(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.record("public_place")?;
        Ok(self.public.unwrap_or_else(|| {
            place_by_id(
                self.db,
                scope,
                symbol.into(),
                RequiresExplicitReExport::No,
                ConsideredDefinitions::EndOfScope,
            )
        }))
    }
}
// The direct provider uses the same immediate source operations so only body lowering differs.
struct DirectRecording<'call, 'db>(&'call Recording<'db>);

fn immediate<T>(future: impl Future<Output = Result<T, &'static str>>) -> Result<T, &'static str> {
    match try_poll_immediate(future) {
        Poll::Ready(result) => result,
        Poll::Pending => Err("recording operation unexpectedly suspended"),
    }
}

impl sealed::Sealed for DirectRecording<'_, '_> {}
impl<'db> SynchronousMemberSourceEffects<'db> for DirectRecording<'_, 'db> {
    type Error = &'static str;

    fn checkpoint(&self, work: MemberSourceWork) -> Result<(), Self::Error> {
        immediate(MemberSourceEffects::checkpoint(self.0, work))
    }

    fn place_table(&self, scope: ScopeId<'db>) -> Result<&'db PlaceTable, Self::Error> {
        immediate(MemberSourceEffects::place_table(self.0, scope))
    }

    fn use_def_map(&self, scope: ScopeId<'db>) -> Result<&'db UseDefMap<'db>, Self::Error> {
        immediate(MemberSourceEffects::use_def_map(self.0, scope))
    }

    fn symbol_id(
        &self,
        table: &'db PlaceTable,
        name: &str,
    ) -> Result<Option<ScopedSymbolId>, Self::Error> {
        immediate(MemberSourceEffects::symbol_id(self.0, table, name))
    }

    fn binding_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        bindings: BindingWithConstraintsIterator<'map, 'db>,
    ) -> Result<PlaceWithDefinition<'db>, Self::Error> {
        immediate(MemberSourceEffects::binding_place(self.0, env, bindings))
    }
}

impl<'db> SynchronousRawClassMemberEffects<'db> for DirectRecording<'_, 'db> {
    fn public_class_place(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        immediate(RawClassMemberEffects::public_class_place(
            self.0, scope, symbol,
        ))
    }
}

impl<'db> StaticInstanceMemberEffects<'db> for Recording<'db> {
    async fn body_scope(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ScopeId<'db>, Self::Error> {
        Ok(class.body_scope(self.db))
    }
    async fn code_generator(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        self.record("codegen")?;
        Ok(self.generator)
    }
    async fn has_own_named_tuple_field(
        &self,
        _class: StaticClassLiteral<'db>,
        _name: &str,
    ) -> Result<bool, Self::Error> {
        self.record("named_field")?;
        Ok(self.named_field)
    }
    async fn declaration_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'map, 'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error> {
        self.record("declarations")?;
        Ok(place_from_declarations(self.db, env, declarations))
    }
    async fn imported_final<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        result: PlaceFromDeclarationsResult<'db>,
        imported: ImportedFinalCandidatesIterator<'map, 'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error> {
        self.record("imported")?;
        Ok(result.with_imported_final(self.db, env, imported))
    }
    async fn implicit_member(
        &self,
        _class: StaticClassLiteral<'db>,
        _name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.record("implicit")?;
        Ok(self.implicit)
    }
    async fn is_kw_only(&self, _ty: Type<'db>) -> Result<bool, Self::Error> {
        self.record("kw_only")?;
        Ok(self.kw_only)
    }
    async fn is_stub(&self, _class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.record("stub")?;
        Ok(self.stub)
    }
    async fn has_instance_slot(
        &self,
        _class: StaticClassLiteral<'db>,
        _name: &str,
    ) -> Result<bool, Self::Error> {
        self.record("slot")?;
        Ok(self.slot)
    }
    async fn is_own_dataclass_instance_field(
        &self,
        _class: StaticClassLiteral<'db>,
        _name: &str,
    ) -> Result<bool, Self::Error> {
        self.record("dataclass_field")?;
        Ok(self.dataclass_field)
    }
    async fn getter_member(
        &self,
        _env: &ProgramEnvironment<'db>,
        _ty: Type<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.record("getter")?;
        Ok(self.getter)
    }
    async fn union_two(
        &self,
        _env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.record("union")?;
        self.unions.borrow_mut().push((first, second));
        Ok(Type::int_literal(99))
    }
}

fn configure<'db>(db: &'db TestDb, case: usize) -> Recording<'db> {
    let mut effects = Recording::new(db);
    if case == 1 {
        effects.generator = Some(CodeGeneratorKind::NamedTuple);
        effects.named_field = true;
    }
    if matches!(case, 4 | 5 | 6 | 7 | 8 | 12) {
        effects.binding = Some(place(
            Type::int_literal(1),
            TypeOrigin::Inferred,
            Definedness::AlwaysDefined,
        ));
    }
    if case == 5 {
        effects.stub = true;
        effects.slot = true;
    }
    if case == 6 {
        effects.dataclass_field = true;
    }
    if case == 7 {
        effects.dataclass_field = true;
        effects.getter = Place::bound(Type::int_literal(1)).into();
    }
    if matches!(case, 8 | 9 | 12) {
        effects.implicit = Member {
            inner: place(
                Type::int_literal(2),
                TypeOrigin::Inferred,
                Definedness::AlwaysDefined,
            )
            .with_qualifiers(TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE),
        };
    }
    if case == 10 {
        effects.kw_only = true;
        effects.generator = Some(CodeGeneratorKind::DataclassLike(None));
    }
    effects
}

#[test]
fn static_instance_source_keeps_shortcuts_and_refuses_each_reached_child() -> anyhow::Result<()> {
    let db = database()?;
    let class = class(&db)?;
    let env = db.program_environment();
    let names = [
        "missing", "present", "present", "init", "bound", "bound", "bound", "bound", "bound",
        "maybe", "marker", "cv", "init",
    ];
    for (case, name) in names.into_iter().enumerate() {
        let effects = configure(&db, case);
        let member = finish(static_own_instance_member_with(
            &env, class, name, &effects,
        ))?;
        let calls = effects.calls.borrow().clone();
        let prefix = [
            "codegen",
            "table",
            "symbol",
            "use_def",
            "declarations",
            "imported",
        ];
        match case {
            0 => {
                assert!(member.is_undefined());
                assert_eq!(calls, ["codegen", "table", "symbol", "implicit"]);
            }
            1 => {
                assert!(member.is_undefined());
                assert_eq!(calls, ["codegen", "named_field"]);
            }
            2 => {
                assert!(!member.is_undefined());
                assert_eq!(&calls[..6], &prefix);
                assert_eq!(&calls[6..], ["kw_only", "bindings"]);
            }
            3 => {
                assert!(member.is_undefined());
                assert_eq!(&calls[..6], &prefix);
                assert_eq!(&calls[6..], ["implicit"]);
            }
            4 => {
                assert!(member.is_undefined());
                assert_eq!(
                    &calls[6..],
                    ["kw_only", "bindings", "stub", "implicit", "dataclass_field"]
                );
            }
            5 => {
                assert!(!member.is_undefined());
                assert_eq!(&calls[6..], ["kw_only", "bindings", "stub", "slot"]);
            }
            6 => {
                assert!(!member.is_undefined());
                assert_eq!(
                    &calls[6..],
                    [
                        "kw_only",
                        "bindings",
                        "stub",
                        "implicit",
                        "dataclass_field",
                        "getter"
                    ]
                );
            }
            7 => {
                assert!(member.is_undefined());
                assert_eq!(
                    &calls[6..],
                    [
                        "kw_only",
                        "bindings",
                        "stub",
                        "implicit",
                        "dataclass_field",
                        "getter"
                    ]
                );
            }
            8 => {
                assert!(!member.is_undefined());
                assert_eq!(&calls[6..], ["kw_only", "bindings", "stub", "implicit"]);
                assert!(effects.unions.borrow().is_empty());
            }
            9 => {
                assert_eq!(member.inner.place.raw_type(), Some(Type::int_literal(99)));
                let unions = effects.unions.borrow();
                assert_eq!(unions.len(), 1);
                assert_eq!(unions[0].1, Type::int_literal(2));
                assert_ne!(unions[0].0, unions[0].1);
                assert!(matches!(
                    member.inner.place,
                    Place::Defined(DefinedPlace {
                        origin: TypeOrigin::Declared,
                        definedness: Definedness::PossiblyUndefined,
                        public_type_policy: PublicTypePolicy::Raw,
                        ..
                    })
                ));
            }
            10 => {
                assert!(member.is_undefined());
                assert_eq!(&calls[6..], ["kw_only", "codegen"]);
            }
            11 => {
                assert!(member.is_undefined());
                assert!(member.qualifiers().contains(TypeQualifiers::CLASS_VAR));
            }
            12 => {
                assert!(!member.is_undefined());
                assert_eq!(calls.iter().filter(|&&call| call == "implicit").count(), 2);
            }
            _ => anyhow::bail!("unknown source case"),
        }
        assert_eq!(
            effects.work.borrow().last(),
            Some(&MemberSourceWork::Publish)
        );
        for rejected in 0..calls.len() {
            let mut denied = configure(&db, case);
            denied.reject = Some(rejected);
            assert!(matches!(
                try_poll_immediate(static_own_instance_member_with(
                    &env, class, name, &denied
                )),
                Poll::Ready(Err("refused"))
            ));
            assert_eq!(&*denied.calls.borrow(), &calls[..=rejected]);
            assert!(!denied.work.borrow().contains(&MemberSourceWork::Publish));
        }
    }
    Ok(())
}

#[test]
fn raw_class_and_runtime_binding_keep_distinct_absence_and_metadata() -> anyhow::Result<()> {
    let db = database()?;
    let class = class(&db)?;
    let scope = class.body_scope(&db);
    let env = db.program_environment();
    let declared = place(
        Type::int_literal(8),
        TypeOrigin::Declared,
        Definedness::AlwaysDefined,
    )
    .with_qualifiers(TypeQualifiers::INIT_VAR | TypeQualifiers::FINAL);
    let inferred = Place::Defined(DefinedPlace {
        ty: Type::int_literal(9),
        origin: TypeOrigin::Inferred,
        definedness: Definedness::PossiblyUndefined,
        public_type_policy: PublicTypePolicy::Promote,
        provenance: Provenance::MultipleDefinitions,
    });
    for (name, public, binding, expected_calls) in [
        ("missing", None, None, vec!["table", "symbol"]),
        (
            "present",
            Some(Place::declared(Type::int_literal(7)).into()),
            None,
            vec!["table", "symbol", "public_place"],
        ),
        (
            "present",
            Some(Place::Undefined.into()),
            None,
            vec!["table", "symbol", "public_place"],
        ),
        (
            "present",
            Some(declared),
            Some(inferred),
            vec!["table", "symbol", "public_place", "use_def", "bindings"],
        ),
        (
            "present",
            Some(declared),
            Some(Place::Undefined),
            vec!["table", "symbol", "public_place", "use_def", "bindings"],
        ),
    ] {
        let mut effects = Recording::new(&db);
        effects.public = public;
        effects.binding = binding;
        let member = finish(raw_class_member_with(
            scope,
            name,
            RawClassMemberFacts,
            &effects,
        ))?;
        assert_eq!(&*effects.calls.borrow(), &expected_calls);
        let mut direct = Recording::new(&db);
        direct.public = public;
        direct.binding = binding;
        assert_eq!(
            raw_class_member_sync(scope, name, RawClassMemberFacts, &DirectRecording(&direct)),
            Ok(member),
        );
        let events = effects.events.borrow().clone();
        assert_eq!(*direct.events.borrow(), events);
        if let Some(binding) = binding {
            let expected = match binding {
                Place::Undefined => Place::Undefined.with_qualifiers(declared.qualifiers),
                Place::Defined(place) => Place::Defined(DefinedPlace {
                    ty: Type::int_literal(8),
                    ..place
                })
                .with_qualifiers(declared.qualifiers),
            };
            assert_eq!(member.inner, expected);
        }
        for (rejected, event) in events.iter().enumerate() {
            for synchronous in [false, true] {
                let mut denied = Recording::new(&db);
                denied.public = public;
                denied.binding = binding;
                match event {
                    SourceEvent::Call(_) => {
                        denied.reject = Some(
                            events[..rejected]
                                .iter()
                                .filter(|event| matches!(event, SourceEvent::Call(_)))
                                .count(),
                        );
                    }
                    SourceEvent::Work(_) => {
                        denied.reject_work = Some(
                            events[..rejected]
                                .iter()
                                .filter(|event| matches!(event, SourceEvent::Work(_)))
                                .count(),
                        );
                    }
                }
                let result = if synchronous {
                    raw_class_member_sync(
                        scope,
                        name,
                        RawClassMemberFacts,
                        &DirectRecording(&denied),
                    )
                } else {
                    immediate(raw_class_member_with(
                        scope,
                        name,
                        RawClassMemberFacts,
                        &denied,
                    ))
                };
                assert_eq!(result, Err("refused"));
                assert_eq!(&*denied.events.borrow(), &events[..=rejected]);
            }
        }
        let mut retry = Recording::new(&db);
        retry.public = public;
        retry.binding = binding;
        assert_eq!(
            immediate(raw_class_member_with(
                scope,
                name,
                RawClassMemberFacts,
                &retry
            )),
            Ok(member)
        );
        assert_eq!(*retry.events.borrow(), events);
    }
    for (name, binding, absent, calls) in [
        ("missing", Place::Undefined, false, vec!["table", "symbol"]),
        (
            "present",
            Place::Undefined,
            true,
            vec!["table", "symbol", "use_def", "bindings"],
        ),
        (
            "present",
            inferred,
            false,
            vec!["table", "symbol", "use_def", "bindings"],
        ),
    ] {
        let mut effects = Recording::new(&db);
        effects.binding = Some(binding);
        assert_eq!(
            finish(runtime_binding_absent_with(&env, scope, name, &effects))?,
            absent
        );
        assert_eq!(&*effects.calls.borrow(), &calls);
        for rejected in 0..calls.len() {
            let mut denied = Recording::new(&db);
            denied.binding = Some(binding);
            denied.reject = Some(rejected);
            assert!(matches!(
                try_poll_immediate(runtime_binding_absent_with(&env, scope, name, &denied)),
                Poll::Ready(Err("refused"))
            ));
            assert_eq!(&*denied.calls.borrow(), &calls[..=rejected]);
        }
    }
    Ok(())
}

struct Names<'db> {
    db: &'db TestDb,
    calls: RefCell<Vec<&'static str>>,
    comparisons: RefCell<Vec<(usize, usize)>>,
    reject_comparison: Option<usize>,
    inferred: Cell<bool>,
}
impl sealed::Sealed for Names<'_> {}
impl ImplicitNameSearchControl for Names<'_> {
    type Error = &'static str;
    fn comparison(
        &self,
        candidate_bytes: usize,
        requested_bytes: usize,
    ) -> Result<(), Self::Error> {
        let mut comparisons = self.comparisons.borrow_mut();
        comparisons.push((candidate_bytes, requested_bytes));
        if self.reject_comparison == Some(comparisons.len() - 1) {
            Err("comparison refused")
        } else {
            Ok(())
        }
    }
}
impl<'db> ImplicitAttributeEffects<'db> for Names<'db> {
    async fn body_scope(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ScopeId<'db>, Self::Error> {
        Ok(class.body_scope(self.db))
    }
    type Error = &'static str;
    async fn checkpoint(&self, _work: MemberSourceWork) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn names(&self, scope: ScopeId<'db>) -> Result<&'db [Name], Self::Error> {
        self.calls.borrow_mut().push("names");
        Ok(implicit_attribute_names(self.db, scope))
    }
    async fn find_name(
        &self,
        names: &'db [Name],
        name: &str,
    ) -> Result<Option<usize>, Self::Error> {
        self.calls.borrow_mut().push("find");
        implicit_name_index_with(names, name, self)
    }
    async fn infer_named_attribute(
        &self,
        _scope: ScopeId<'db>,
        _name: &'db Name,
        _target: MethodDecorator,
    ) -> Result<ImplicitAttribute<'db>, Self::Error> {
        self.calls.borrow_mut().push("infer");
        self.inferred.set(true);
        Err("inference refused")
    }
}
fn names(db: &TestDb, reject_comparison: Option<usize>) -> Names<'_> {
    Names {
        db,
        calls: RefCell::new(Vec::new()),
        comparisons: RefCell::new(Vec::new()),
        reject_comparison,
        inferred: Cell::new(false),
    }
}

#[test]
fn implicit_names_use_actual_ordered_source_and_refuse_before_inference() -> anyhow::Result<()> {
    let db = database()?;
    let class = class(&db)?;
    let scope = class.body_scope(&db);
    let ordered = implicit_attribute_names(&db, scope);
    assert!(ordered.iter().any(|name| name.as_str() == "雪"));
    for requested in [
        "alpha", "middle", "omega", "éclair", "雪", "missing", "", "zzzz",
    ] {
        let expected_comparisons = RefCell::new(Vec::new());
        let expected = ordered
            .binary_search_by(|candidate| {
                expected_comparisons
                    .borrow_mut()
                    .push((candidate.len(), requested.len()));
                candidate.as_str().cmp(requested)
            })
            .ok();
        let effects = names(&db, None);
        assert_eq!(
            implicit_name_index_with(ordered, requested, &effects),
            Ok(expected)
        );
        assert_eq!(
            *effects.comparisons.borrow(),
            *expected_comparisons.borrow()
        );
        for index in 0..expected_comparisons.borrow().len() {
            let denied = names(&db, Some(index));
            assert_eq!(
                implicit_name_index_with(ordered, requested, &denied),
                Err("comparison refused")
            );
            assert_eq!(
                &*denied.comparisons.borrow(),
                &expected_comparisons.borrow()[..=index]
            );
            assert!(!denied.inferred.get());
        }
        let effects = names(&db, None);
        let result = try_poll_immediate(implicit_attribute_bindings_with(
            class,
            requested,
            MethodDecorator::None,
            &effects,
        ));
        if expected.is_some() {
            assert!(matches!(result, Poll::Ready(Err("inference refused"))));
            assert_eq!(&*effects.calls.borrow(), &["names", "find", "infer"]);
        } else {
            let Poll::Ready(Ok(attribute)) = result else {
                anyhow::bail!("absent implicit name did not finish");
            };
            assert!(attribute.member.is_undefined());
            assert!(attribute.augmented_bindings.is_none());
            assert_eq!(&*effects.calls.borrow(), &["names", "find"]);
        }
        let denied = names(&db, Some(0));
        assert!(matches!(
            try_poll_immediate(implicit_attribute_bindings_with(
                class,
                requested,
                MethodDecorator::None,
                &denied
            )),
            Poll::Ready(Err("comparison refused"))
        ));
        assert!(!denied.inferred.get());
    }
    Ok(())
}

struct Generator<'db> {
    db: &'db TestDb,
    queried: Cell<bool>,
    refuse: bool,
}
impl sealed::Sealed for Generator<'_> {}
impl<'db> StaticCodeGeneratorEffects<'db> for Generator<'db> {
    async fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<DataclassParams<'db>>, Self::Error> {
        Ok(class.dataclass_params(self.db))
    }
    async fn known(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(class.known(self.db))
    }
    async fn has_explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(class.has_explicit_bases(self.db))
    }
    async fn has_explicit_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(class.has_explicit_metaclass(self.db))
    }

    type Error = &'static str;
    async fn checkpoint(&self, _work: MemberSourceWork) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn code_generator_query(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        self.queried.set(true);
        if self.refuse {
            Err("codegen refused")
        } else {
            Ok(code_generator_of_static_class(self.db, class))
        }
    }
}

#[test]
fn source_selectors_keep_real_ingredient_identity_and_known_class_queries() -> anyhow::Result<()> {
    let db = database()?;
    let plain = class(&db)?;
    let env = db.program_environment();
    let effects = Generator {
        db: &db,
        queried: Cell::new(false),
        refuse: false,
    };
    assert!(finish(static_code_generator_with(plain, &effects))?.is_none());
    assert!(!effects.queried.get());
    for known in [KnownClass::Object, KnownClass::Type] {
        let class = known
            .try_to_class_literal(&db, &env)
            .ok_or_else(|| anyhow::anyhow!("missing builtin class"))?;
        let effects = Generator {
            db: &db,
            queried: Cell::new(false),
            refuse: false,
        };
        let expected = code_generator_of_static_class(&db, class);
        assert_eq!(
            finish(static_code_generator_with(class, &effects))?,
            expected
        );
        assert!(effects.queried.get());
        let effects = Generator {
            db: &db,
            queried: Cell::new(false),
            refuse: true,
        };
        assert!(matches!(
            try_poll_immediate(static_code_generator_with(class, &effects)),
            Poll::Ready(Err("codegen refused"))
        ));
        let key = code_generator_of_static_class_ingredient(&db).database_key_index(class.as_id());
        let capture =
            prepared_source_probe::capture(&db, || code_generator_of_static_class(&db, class))
                .map_err(|error| anyhow::anyhow!("capture failed: {error:?}"))?;
        assert!(
            capture
                .reads
                .iter()
                .any(|read| read.parent.is_none() && read.key == key)
        );
    }
    let scope = plain.body_scope(&db);
    let key = implicit_attribute_names_ingredient(&db).database_key_index(scope.as_id());
    let capture = prepared_source_probe::capture(&db, || implicit_attribute_names(&db, scope))
        .map_err(|error| anyhow::anyhow!("capture failed: {error:?}"))?;
    assert!(
        capture
            .reads
            .iter()
            .any(|read| read.parent.is_none() && read.key == key)
    );
    Ok(())
}
