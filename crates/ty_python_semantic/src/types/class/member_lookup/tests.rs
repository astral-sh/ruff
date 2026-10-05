use std::cell::RefCell;
use std::future::Future;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ty_python_core::ProgramFile;
use ty_python_core::definition::Definition;

use super::{
    AugmentedBindings, ClassBase, ClassType, ImplicitAttribute, InstanceMemberResult,
    InstanceMroEffects, InstanceMroWork, Member, MroPendingBindings, Place, PlaceAndQualifiers,
    Provenance, Type, TypeQualifiers, UnionBuilder, mro_instance_member_with, sealed,
};
use crate::Db;
use crate::ProgramEnvironment;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class::ClassMetaclass;
use crate::types::class::namespace::{
    NamespaceLookupEffects, NamespaceLookupRequest, NamespaceLookupWork, namespace_lookup_with,
    sealed as namespace_sealed,
};
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::{MemberLookupPolicy, TypingModule};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file(
            "/src/lookup.py",
            "class A: ...\nclass B: ...\nclass C: ...\n",
        )
        .build()
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<ClassType<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/lookup.py")?,
        env.program(db),
    );
    global_symbol(db, file, name)
        .place
        .ignore_possibly_undefined()
        .and_then(|ty| ty.to_class_type(db))
        .ok_or_else(|| anyhow::anyhow!("missing class {name}"))
}

fn finish<T>(future: impl Future<Output = Result<T, &'static str>>) -> anyhow::Result<T> {
    match try_poll_immediate(future) {
        Poll::Ready(result) => result.map_err(anyhow::Error::msg),
        Poll::Pending => anyhow::bail!("recording lookup unexpectedly suspended"),
    }
}

// These effects exercise the shared continuations using explicit native members. They do not
// prepare member-query answers or establish that a class-backed lookup is cold or absent.
struct Namespace<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
    class: ClassType<'db>,
    initial: PlaceAndQualifiers<'db>,
    storage: PlaceAndQualifiers<'db>,
    own: PlaceAndQualifiers<'db>,
    inherited: PlaceAndQualifiers<'db>,
    binding_absent: bool,
    dynamic: Vec<ClassBase<'db>>,
    descriptor: bool,
    refuse: Option<&'static str>,
    events: RefCell<Vec<&'static str>>,
}

impl<'env, 'db> Namespace<'env, 'db> {
    fn new(db: &'db dyn Db, env: &'env ProgramEnvironment<'db>, class: ClassType<'db>) -> Self {
        Self {
            db,
            env,
            class,
            initial: Place::Undefined.into(),
            storage: Place::Undefined.into(),
            own: Place::Undefined.into(),
            inherited: Place::Undefined.into(),
            binding_absent: false,
            dynamic: Vec::new(),
            descriptor: false,
            refuse: None,
            events: RefCell::default(),
        }
    }
    fn enter(&self, name: &'static str) -> Result<(), &'static str> {
        self.events.borrow_mut().push(name);
        if self.refuse == Some(name) {
            Err("namespace refused")
        } else {
            Ok(())
        }
    }
    fn request(&self, policy: MemberLookupPolicy) -> NamespaceLookupRequest<'static, 'db> {
        NamespaceLookupRequest {
            class: self.class,
            name: "value",
            policy,
        }
    }
}

impl namespace_sealed::Sealed for Namespace<'_, '_> {}
impl<'db> NamespaceLookupEffects<'db> for Namespace<'_, 'db> {
    type Error = &'static str;
    type DynamicCursor = std::vec::IntoIter<ClassBase<'db>>;

    async fn checkpoint(&self, work: NamespaceLookupWork) -> Result<(), Self::Error> {
        if work == NamespaceLookupWork::Publish {
            self.enter("publish")?;
        }
        Ok(())
    }
    async fn find_in_mro(
        &self,
        _: Type<'db>,
        _: &str,
        _: MemberLookupPolicy,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error> {
        self.enter("mro")?;
        Ok(Some(self.initial))
    }
    async fn inferred_metaclass(
        &self,
        _: ClassType<'db>,
    ) -> Result<ClassMetaclass<'db>, Self::Error> {
        self.enter("metaclass")?;
        Ok(ClassMetaclass::Selected(Type::object()))
    }
    async fn for_inheritance(&self, _: ClassMetaclass<'db>) -> Result<Type<'db>, Self::Error> {
        self.enter("inheritance")?;
        Ok(Type::object())
    }
    async fn instance_approximation(&self, _: Type<'db>) -> Result<Option<Type<'db>>, Self::Error> {
        self.enter("approximation")?;
        Ok(Some(Type::object()))
    }
    async fn nominal_class(&self, _: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error> {
        self.enter("nominal")?;
        Ok(Some(self.class))
    }
    async fn instance_member(
        &self,
        _: ClassType<'db>,
        _: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.enter("storage")?;
        Ok(self.storage)
    }
    async fn own_member(
        &self,
        _: NamespaceLookupRequest<'_, 'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.enter("own")?;
        Ok(self.own)
    }
    async fn runtime_binding_absent(
        &self,
        _: ClassType<'db>,
        _: &str,
    ) -> Result<bool, Self::Error> {
        self.enter("binding")?;
        Ok(self.binding_absent)
    }
    async fn inherited_member(
        &self,
        _: NamespaceLookupRequest<'_, 'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.enter("inherited")?;
        Ok(self.inherited)
    }
    async fn fall_back_to(
        &self,
        member: PlaceAndQualifiers<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.enter("fallback")?;
        Ok(member.or_fall_back_to(self.db, self.env, || fallback))
    }
    async fn start_dynamic_mro(
        &self,
        _: ClassType<'db>,
    ) -> Result<Self::DynamicCursor, Self::Error> {
        self.enter("dynamic")?;
        Ok(self.dynamic.clone().into_iter())
    }
    async fn next_dynamic_base(
        &self,
        cursor: &mut Self::DynamicCursor,
    ) -> Result<Option<ClassBase<'db>>, Self::Error> {
        self.enter("next_dynamic")?;
        Ok(cursor.next())
    }
    async fn may_be_data_descriptor(&self, _: Type<'db>) -> Result<bool, Self::Error> {
        self.enter("descriptor")?;
        Ok(self.descriptor)
    }
    async fn filter_possible_data_descriptors(
        &self,
        ty: Type<'db>,
    ) -> Result<(Type<'db>, bool), Self::Error> {
        self.enter("filter")?;
        Ok((ty, true))
    }
}

#[test]
fn namespace_steps_preserve_lazy_metaclass_storage() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let class = class(&db, "A")?;
    let expected = [
        "mro",
        "metaclass",
        "inheritance",
        "approximation",
        "nominal",
        "storage",
        "publish",
    ];
    for policy in [
        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        MemberLookupPolicy::default(),
    ] {
        let effects = Namespace::new(&db, &env, class);
        assert_eq!(
            finish(namespace_lookup_with(
                Type::object(),
                effects.request(policy),
                &effects
            ))?,
            Place::Undefined.into()
        );
        assert_eq!(&*effects.events.borrow(), &expected);
        for (index, child) in expected[..expected.len() - 1].iter().enumerate() {
            let mut effects = Namespace::new(&db, &env, class);
            effects.refuse = Some(child);
            assert_eq!(
                try_poll_immediate(namespace_lookup_with(
                    Type::object(),
                    effects.request(policy),
                    &effects
                )),
                Poll::Ready(Err("namespace refused"))
            );
            assert_eq!(&*effects.events.borrow(), &expected[..=index]);
        }
    }
    Ok(())
}

#[test]
fn namespace_steps_preserve_present_storage_precedence() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let class = class(&db, "A")?;
    for class_var in [false, true] {
        for policy in [
            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            MemberLookupPolicy::default(),
        ] {
            let mut effects = Namespace::new(&db, &env, class);
            effects.storage = Place::bound(Type::any())
                .with_qualifiers(TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE);
            effects.own = Place::declared(Type::Never).with_qualifiers(if class_var {
                TypeQualifiers::CLASS_VAR | TypeQualifiers::READ_ONLY
            } else {
                TypeQualifiers::READ_ONLY
            });
            effects.binding_absent = true;
            let result = finish(namespace_lookup_with(
                Type::object(),
                effects.request(policy),
                &effects,
            ))?;
            assert_eq!(
                result,
                if class_var {
                    effects.own
                } else {
                    effects.storage
                }
            );
            let mut expected = vec![
                "mro",
                "metaclass",
                "inheritance",
                "approximation",
                "nominal",
                "storage",
                "own",
            ];
            if !class_var {
                expected.push("binding");
            }
            expected.extend(["inherited", "fallback", "fallback"]);
            if policy == MemberLookupPolicy::default() {
                expected.extend(["dynamic", "next_dynamic"]);
            }
            expected.push("publish");
            assert_eq!(&*effects.events.borrow(), &expected);
        }
    }
    let mut effects = Namespace::new(&db, &env, class);
    effects.storage = Place::bound(Type::any()).into();
    effects.dynamic = vec![ClassBase::Class(class), ClassBase::Any];
    effects.descriptor = true;
    let result = finish(namespace_lookup_with(
        Type::object(),
        effects.request(MemberLookupPolicy::default()),
        &effects,
    ))?;
    assert_eq!(result, effects.storage);
    assert_eq!(
        &effects.events.borrow()[11..],
        &[
            "dynamic",
            "next_dynamic",
            "next_dynamic",
            "descriptor",
            "filter",
            "fallback",
            "publish"
        ]
    );
    Ok(())
}

#[derive(Clone, Copy)]
struct Row<'db> {
    class: ClassType<'db>,
    own: Member<'db>,
    implicit: Option<ImplicitAttribute<'db>>,
    class_member: Member<'db>,
}

struct Instance<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
    rows: Vec<Row<'db>>,
    non_data: bool,
    refuse_reserve: bool,
    events: RefCell<Vec<&'static str>>,
    reserve_lengths: RefCell<Vec<usize>>,
    inferred_lengths: RefCell<Vec<usize>>,
}

impl<'env, 'db> Instance<'env, 'db> {
    fn new(db: &'db dyn Db, env: &'env ProgramEnvironment<'db>, rows: Vec<Row<'db>>) -> Self {
        Self {
            db,
            env,
            rows,
            non_data: true,
            refuse_reserve: false,
            events: RefCell::default(),
            reserve_lengths: RefCell::default(),
            inferred_lengths: RefCell::default(),
        }
    }
    fn row(&self, class: ClassType<'db>) -> Result<Row<'db>, &'static str> {
        self.rows
            .iter()
            .find(|row| row.class == class)
            .copied()
            .ok_or("missing recording row")
    }
    fn event(&self, event: &'static str) {
        self.events.borrow_mut().push(event);
    }
}

impl sealed::Sealed for Instance<'_, '_> {}
impl<'db> InstanceMroEffects<'db, std::vec::IntoIter<ClassBase<'db>>> for Instance<'_, 'db> {
    type Error = &'static str;

    async fn checkpoint(&self, work: InstanceMroWork) -> Result<(), Self::Error> {
        match work {
            InstanceMroWork::Publish => self.event("publish"),
            InstanceMroWork::ClearAugmented { .. } => self.event("clear"),
            _ => {}
        }
        Ok(())
    }
    async fn new_union(&self) -> Result<UnionBuilder<'db>, Self::Error> {
        self.event("new_union");
        Ok(UnionBuilder::new(self.db, self.env))
    }
    async fn advance(
        &self,
        cursor: &mut std::vec::IntoIter<ClassBase<'db>>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error> {
        self.event("advance");
        Ok(cursor.next())
    }
    async fn own_instance_member(
        &self,
        class: ClassType<'db>,
        _: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.event("own");
        Ok(self.row(class)?.own)
    }
    async fn implicit_attribute(
        &self,
        class: ClassType<'db>,
        _: &str,
    ) -> Result<Option<ImplicitAttribute<'db>>, Self::Error> {
        self.event("implicit");
        Ok(self.row(class)?.implicit)
    }
    async fn push_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
        class: ClassType<'db>,
        bindings: AugmentedBindings<'db>,
    ) -> Result<(), Self::Error> {
        self.event("reserve");
        self.reserve_lengths.borrow_mut().push(pending.len());
        if self.refuse_reserve {
            Err("reserve refused")
        } else {
            pending.push((class, bindings));
            Ok(())
        }
    }
    async fn clear_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error> {
        pending.clear();
        Ok(())
    }
    async fn finish_pending(
        &self,
        pending: Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error> {
        drop(pending);
        Ok(())
    }
    async fn infer_augmented(
        &self,
        bindings: MroPendingBindings<'_, 'db>,
    ) -> Result<(Type<'db>, Provenance<'db>), Self::Error> {
        self.event("infer");
        self.inferred_lengths
            .borrow_mut()
            .push(bindings.pending_count());
        Ok((Type::any(), Provenance::Unknown))
    }
    async fn union_add(
        &self,
        union: UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<UnionBuilder<'db>, Self::Error> {
        self.event("add");
        Ok(union.add(ty))
    }
    async fn own_class_member(
        &self,
        class: ClassType<'db>,
        _: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.event("own_class");
        Ok(self.row(class)?.class_member)
    }
    async fn is_definitely_non_data_descriptor(&self, _: Type<'db>) -> Result<bool, Self::Error> {
        self.event("descriptor");
        Ok(self.non_data)
    }
    async fn union_build(&self, union: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error> {
        self.event("build");
        Ok(union.build())
    }
}

#[test]
fn instance_mro_preserves_pending_and_declared_shortcuts() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let a = class(&db, "A")?;
    let b = class(&db, "B")?;
    let c = class(&db, "C")?;
    let bindings = AugmentedBindings::new(&db, Box::<[Definition<'_>]>::default());
    let absent = Member::unbound();
    let declared = Member::definitely_declared(Type::object());
    let inferred = Member {
        inner: Place::bound(Type::any()).into(),
    };
    let row = Row {
        class: a,
        own: absent,
        implicit: Some(ImplicitAttribute {
            member: inferred,
            augmented_bindings: Some(bindings),
        }),
        class_member: absent,
    };
    let effects = Instance::new(&db, &env, vec![row]);
    assert_eq!(
        finish(mro_instance_member_with(
            "value",
            vec![ClassBase::Class(a)].into_iter(),
            &effects
        ))?,
        InstanceMemberResult::Done(Place::Undefined.into())
    );
    assert_eq!(
        &*effects.events.borrow(),
        &[
            "new_union",
            "advance",
            "own",
            "implicit",
            "advance",
            "publish"
        ]
    );

    let effects = Instance::new(
        &db,
        &env,
        vec![Row {
            own: declared,
            implicit: None,
            ..row
        }],
    );
    assert_eq!(
        finish(mro_instance_member_with(
            "value",
            vec![ClassBase::Class(a), ClassBase::Class(b)].into_iter(),
            &effects
        ))?,
        InstanceMemberResult::Done(declared.inner)
    );
    assert_eq!(
        &*effects.events.borrow(),
        &["new_union", "advance", "own", "implicit", "publish"]
    );

    let implicit_declared = Member {
        inner: Place::declared(Type::Never)
            .with_qualifiers(TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE),
    };
    let effects = Instance::new(
        &db,
        &env,
        vec![
            Row {
                own: inferred,
                implicit: None,
                ..row
            },
            Row {
                class: b,
                own: declared,
                implicit: None,
                ..row
            },
            Row {
                class: c,
                own: implicit_declared,
                implicit: None,
                ..row
            },
        ],
    );
    assert_eq!(
        finish(mro_instance_member_with(
            "value",
            vec![
                ClassBase::Class(a),
                ClassBase::Class(b),
                ClassBase::Class(c)
            ]
            .into_iter(),
            &effects
        ))?,
        InstanceMemberResult::Done(implicit_declared.inner)
    );
    assert_eq!(
        &*effects.events.borrow(),
        &[
            "new_union",
            "advance",
            "own",
            "implicit",
            "add",
            "advance",
            "own",
            "implicit",
            "advance",
            "own",
            "implicit",
            "publish"
        ]
    );

    let mut effects = Instance::new(
        &db,
        &env,
        vec![
            Row {
                implicit: Some(ImplicitAttribute {
                    member: absent,
                    augmented_bindings: Some(bindings),
                }),
                class_member: declared,
                ..row
            },
            Row {
                class: b,
                own: declared,
                implicit: None,
                ..row
            },
        ],
    );
    effects.non_data = false;
    assert_eq!(
        finish(mro_instance_member_with(
            "value",
            vec![ClassBase::Class(a), ClassBase::Class(b)].into_iter(),
            &effects
        ))?,
        InstanceMemberResult::Done(declared.inner)
    );
    assert_eq!(
        &*effects.events.borrow(),
        &[
            "new_union",
            "advance",
            "own",
            "implicit",
            "reserve",
            "own_class",
            "descriptor",
            "clear",
            "advance",
            "own",
            "implicit",
            "publish"
        ]
    );
    assert!(effects.inferred_lengths.borrow().is_empty());

    let effects = Instance::new(&db, &env, vec![]);
    assert_eq!(
        finish(mro_instance_member_with(
            "value",
            vec![
                ClassBase::Generic,
                ClassBase::Protocol,
                ClassBase::TypedDict(TypingModule::Typing)
            ]
            .into_iter(),
            &effects
        ))?,
        InstanceMemberResult::TypedDict
    );
    assert_eq!(
        &*effects.events.borrow(),
        &["new_union", "advance", "advance", "advance", "publish"]
    );
    Ok(())
}

#[test]
fn instance_mro_reserve_refusal_precedes_push() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let a = class(&db, "A")?;
    let b = class(&db, "B")?;
    let bindings = AugmentedBindings::new(&db, Box::<[Definition<'_>]>::default());
    let absent = Member::unbound();
    let rows = vec![
        Row {
            class: a,
            own: absent,
            implicit: Some(ImplicitAttribute {
                member: absent,
                augmented_bindings: Some(bindings),
            }),
            class_member: absent,
        },
        Row {
            class: b,
            own: Member {
                inner: Place::bound(Type::any()).into(),
            },
            implicit: None,
            class_member: absent,
        },
    ];
    let mut effects = Instance::new(&db, &env, rows.clone());
    effects.refuse_reserve = true;
    assert_eq!(
        try_poll_immediate(mro_instance_member_with(
            "value",
            vec![ClassBase::Class(a), ClassBase::Class(b)].into_iter(),
            &effects
        )),
        Poll::Ready(Err("reserve refused"))
    );
    assert_eq!(&*effects.reserve_lengths.borrow(), &[0]);
    assert_eq!(
        &*effects.events.borrow(),
        &["new_union", "advance", "own", "implicit", "reserve"]
    );

    let effects = Instance::new(&db, &env, rows);
    let result = finish(mro_instance_member_with(
        "value",
        vec![ClassBase::Class(a), ClassBase::Class(b)].into_iter(),
        &effects,
    ))?;
    assert_eq!(
        result,
        InstanceMemberResult::Done(
            Place::bound(Type::any()).with_qualifiers(TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE)
        )
    );
    assert_eq!(&*effects.reserve_lengths.borrow(), &[0]);
    assert_eq!(&*effects.inferred_lengths.borrow(), &[1]);
    assert_eq!(
        &*effects.events.borrow(),
        &[
            "new_union",
            "advance",
            "own",
            "implicit",
            "reserve",
            "own_class",
            "advance",
            "own",
            "implicit",
            "add",
            "infer",
            "add",
            "clear",
            "advance",
            "build",
            "publish"
        ]
    );

    let effects = Instance::new(&db, &env, vec![]);
    assert_eq!(
        finish(mro_instance_member_with(
            "value",
            vec![].into_iter(),
            &effects
        ))?,
        InstanceMemberResult::Done(Place::Undefined.into())
    );
    assert_eq!(
        &*effects.events.borrow(),
        &["new_union", "advance", "publish"]
    );
    Ok(())
}
