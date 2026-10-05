use std::cell::RefCell;
use std::task::Poll;

use ruff_python_ast::name::Name;
use ty_python_core::platform::PythonPlatform;

use super::*;
use crate::ProgramEnvironment;
use crate::db::tests::{TestDb, setup_db};
use crate::types::mapping::effects::InlineMappingEffects;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::typevar::{
    ParamSpecAttrKind, TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance, TypeVarNonce,
};
use crate::types::{BindingContext, TypeVarKind};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Types,
    Entries,
    Next,
    Map(usize),
    Compare,
    Allocate(usize),
    Prefix(usize),
    Append,
    Retain,
    Finish,
    Classify,
    Kind,
    Tuple,
    MapTuple,
    Borrowed,
    SameTuple,
    SameKind,
    Payload,
    Context,
    Intern,
    Program,
    Environment,
    Fresh,
    Specialize,
    Materialize,
    Variables,
    Identity,
    Index,
    TypeAt(usize),
}

#[derive(Default)]
struct Recording {
    events: RefCell<Vec<Event>>,
    changed: Option<usize>,
    tuple_changed: bool,
    refuse_at: Option<usize>,
    visitors: RefCell<Vec<usize>>,
}

impl Recording {
    fn event(&self, event: Event) -> Result<(), usize> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.refuse_at == Some(index) {
            Err(index)
        } else {
            Ok(())
        }
    }
}

struct LookupRecording<'a, 'db> {
    db: &'db TestDb,
    recording: &'a Recording,
}

impl<'db> SpecializationLookupEffects<'db> for LookupRecording<'_, 'db> {
    type Error = usize;

    async fn generic_context(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<GenericContext<'db>, usize> {
        self.recording.event(Event::Context)?;
        Ok(specialization.generic_context(self.db))
    }

    async fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, usize> {
        self.recording.event(Event::Variables)?;
        Ok(context.variables_inner(self.db))
    }

    async fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, usize> {
        self.recording.event(Event::Identity)?;
        Ok(variable.identity(self.db))
    }

    async fn index(
        &self,
        variables: &'db ContextVariables<'db>,
        identity: BoundTypeVarIdentity<'db>,
    ) -> Result<Option<usize>, usize> {
        self.recording.event(Event::Index)?;
        Ok(variables.get_index_of(&identity))
    }

    async fn types(&self, specialization: Specialization<'db>) -> Result<&'db [Type<'db>], usize> {
        self.recording.event(Event::Types)?;
        Ok(specialization.types(self.db))
    }

    async fn type_at(
        &self,
        types: &'db [Type<'db>],
        index: usize,
    ) -> Result<Option<Type<'db>>, usize> {
        self.recording.event(Event::TypeAt(index))?;
        Ok(types.get(index).copied())
    }
}

impl<'db> SpecializationArgumentEffects<'db> for Recording {
    type Error = usize;
    type Cursor = ArgumentCursor<'db>;
    type Buffer = Vec<Type<'db>>;
    async fn types(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<&'db [Type<'db>], usize> {
        self.event(Event::Types)?;
        Ok(specialization.types(db))
    }
    async fn entries(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        types: &'db [Type<'db>],
    ) -> Result<Self::Cursor, usize> {
        self.event(Event::Entries)?;
        Ok(argument_cursor(
            specialization.generic_context(db).variables_inner(db),
            types,
        ))
    }
    async fn next(
        &self,
        cursor: &mut Self::Cursor,
    ) -> Result<Option<(usize, (BoundTypeVarInstance<'db>, Type<'db>))>, usize> {
        self.event(Event::Next)?;
        Ok(cursor.next())
    }
    async fn different(&self, left: Type<'db>, right: Type<'db>) -> Result<bool, usize> {
        self.event(Event::Compare)?;
        Ok(left != right)
    }
    async fn new_buffer(&self, original: &'db [Type<'db>]) -> Result<Self::Buffer, usize> {
        self.event(Event::Allocate(original.len()))?;
        Ok(Vec::with_capacity(original.len()))
    }
    async fn copy_prefix(
        &self,
        buffer: &mut Self::Buffer,
        original: &'db [Type<'db>],
        index: usize,
    ) -> Result<(), usize> {
        self.event(Event::Prefix(index))?;
        buffer.extend_from_slice(&original[..index]);
        Ok(())
    }
    async fn append(&self, buffer: &mut Self::Buffer, ty: Type<'db>) -> Result<(), usize> {
        self.event(Event::Append)?;
        buffer.push(ty);
        Ok(())
    }
    async fn retain_buffer(
        &self,
        target: &mut Option<Self::Buffer>,
        buffer: Self::Buffer,
    ) -> Result<(), usize> {
        self.event(Event::Retain)?;
        assert!(target.is_none());
        *target = Some(buffer);
        Ok(())
    }
    async fn finish(
        &self,
        original: &'db [Type<'db>],
        buffer: Option<Self::Buffer>,
    ) -> Result<Cow<'db, [Type<'db>]>, usize> {
        self.event(Event::Finish)?;
        Ok(buffer.map_or(Cow::Borrowed(original), Cow::Owned))
    }
}

struct ArgumentMapper<'a>(&'a Recording);
impl<'db> SpecializationArgumentMapper<'db> for ArgumentMapper<'_> {
    type Error = usize;
    async fn map(
        &mut self,
        index: usize,
        _variable: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, usize> {
        self.0.event(Event::Map(index))?;
        Ok(if self.0.changed == Some(index) {
            Type::bool_literal(true)
        } else {
            ty
        })
    }
}

impl<'db> SpecializationMapEffects<'db> for Recording {
    type Error = usize;
    async fn materialization(
        &self,
        mapping: &TypeMapping<'_, 'db>,
    ) -> Result<Option<MaterializationKind>, usize> {
        self.event(Event::Classify)?;
        Ok(match mapping {
            TypeMapping::Materialize(kind) => Some(*kind),
            _ => None,
        })
    }
    async fn materialize(
        &self,
        _db: &'db dyn Db,
        specialization: Specialization<'db>,
        _kind: MaterializationKind,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Specialization<'db>, usize> {
        self.event(Event::Materialize)?;
        Ok(specialization)
    }
    async fn materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<Option<MaterializationKind>, usize> {
        self.event(Event::Kind)?;
        Ok(specialization.materialization_kind(db))
    }
    async fn map_arguments(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _contexts: &[Type<'db>],
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        _kind: &mut Option<MaterializationKind>,
    ) -> Result<Cow<'db, [Type<'db>]>, usize> {
        map_specialization_arguments_with(db, specialization, &mut ArgumentMapper(self), self).await
    }
    async fn tuple_inner(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<Option<TupleType<'db>>, usize> {
        self.event(Event::Tuple)?;
        Ok(specialization.tuple_inner(db))
    }
    async fn map_tuple(
        &self,
        db: &'db dyn Db,
        tuple: TupleType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<TupleType<'db>, usize> {
        self.event(Event::MapTuple)?;
        Ok(if self.tuple_changed {
            TupleType::homogeneous(db, visitor.env, Type::bool_literal(true))
        } else {
            tuple
        })
    }
    async fn arguments_borrowed(&self, types: &Cow<'db, [Type<'db>]>) -> Result<bool, usize> {
        self.event(Event::Borrowed)?;
        Ok(matches!(types, Cow::Borrowed(_)))
    }
    async fn same_tuple(
        &self,
        left: Option<TupleType<'db>>,
        right: Option<TupleType<'db>>,
    ) -> Result<bool, usize> {
        self.event(Event::SameTuple)?;
        Ok(left == right)
    }
    async fn same_kind(
        &self,
        left: Option<MaterializationKind>,
        right: Option<MaterializationKind>,
    ) -> Result<bool, usize> {
        self.event(Event::SameKind)?;
        Ok(left == right)
    }
    async fn payload(&self, _types: &Cow<'db, [Type<'db>]>) -> Result<(), usize> {
        self.event(Event::Payload)
    }
    async fn generic_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<GenericContext<'db>, usize> {
        self.event(Event::Context)?;
        Ok(specialization.generic_context(db))
    }
    async fn intern(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        types: Cow<'db, [Type<'db>]>,
        kind: Option<MaterializationKind>,
        tuple: Option<TupleType<'db>>,
    ) -> Result<Specialization<'db>, usize> {
        self.event(Event::Intern)?;
        Ok(Specialization::new(db, context, types, kind, tuple))
    }
}

impl<'db> CompositionStartEffects<'db> for Recording {
    type Error = usize;
    type Environment = ProgramEnvironment<'db>;
    async fn generic_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<GenericContext<'db>, usize> {
        self.event(Event::Context)?;
        Ok(specialization.generic_context(db))
    }
    async fn program(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<Program<'db>, usize> {
        self.event(Event::Program)?;
        Ok(context.program(db))
    }
    async fn environment(&self, program: Program<'db>) -> Result<Self::Environment, usize> {
        self.event(Event::Environment)?;
        Ok(ProgramEnvironment::from_program(program))
    }
    async fn compose_fresh(
        &self,
        db: &'db dyn Db,
        base: Specialization<'db>,
        additional: Specialization<'db>,
        env: &Self::Environment,
    ) -> Result<Specialization<'db>, usize> {
        self.event(Event::Fresh)?;
        compose_specializations_with(
            db,
            base,
            additional,
            &ApplyTypeMappingVisitor::new(env),
            self,
        )
        .await
    }
}
impl<'db> CompositionEffects<'db> for Recording {
    type Error = usize;
    async fn specialization_mapping(
        &self,
        additional: Specialization<'db>,
    ) -> Result<OwnedTypeMapping<'db, 'db>, usize> {
        Ok(OwnedTypeMapping::Specialization {
            specialization: additional,
            specialize_self_domain: false,
            materialization_kind: None,
        })
    }
    async fn materialization_mapping(
        &self,
        kind: MaterializationKind,
    ) -> Result<OwnedTypeMapping<'db, 'db>, usize> {
        Ok(OwnedTypeMapping::Materialize(kind))
    }
    async fn materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<Option<MaterializationKind>, usize> {
        self.event(Event::Kind)?;
        Ok(specialization.materialization_kind(db))
    }
    async fn map_pass(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        mapping: OwnedTypeMapping<'db, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Specialization<'db>, usize> {
        self.visitors
            .borrow_mut()
            .push(std::ptr::from_ref(visitor).addr());
        match mapping {
            OwnedTypeMapping::Specialization {
                specialization: additional,
                specialize_self_domain: false,
                materialization_kind: None,
            } => {
                assert_eq!(
                    visitor.env.program(db),
                    additional.generic_context(db).program(db)
                );
                self.event(Event::Specialize)?;
            }
            OwnedTypeMapping::Materialize(_) => self.event(Event::Materialize)?,
            _ => panic!("unexpected composition mode"),
        }
        Ok(specialization)
    }
}

fn variable(db: &TestDb, index: usize) -> BoundTypeVarInstance<'_> {
    BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
            db,
            TypeVarIdentity::new(
                db,
                Name::new(format!("V{index}")),
                None,
                TypeVarKind::LegacyTypeVar,
            ),
            None,
            None,
            None,
        ),
        BindingContext::Synthetic(db.program_environment().program(db)),
        None,
        TypeVarNonce::NONE,
    )
}

fn input(db: &TestDb, variables: usize, types: usize, tuple: bool) -> Specialization<'_> {
    let env = db.program_environment();
    let context = GenericContext::from_typevar_instances(
        db,
        &env,
        (0..variables).map(|index| variable(db, index)),
    );
    Specialization::new(
        db,
        context,
        vec![Type::bool_literal(false); types].into_boxed_slice(),
        None,
        tuple.then(|| TupleType::homogeneous(db, &env, Type::bool_literal(false))),
    )
}

#[test]
fn stored_lookup_preserves_read_order_and_stops_at_refused_operations() {
    let db = setup_db();
    for (variables, types, queried, index, expected) in [
        (3, 2, 0, Some(0), Some(Type::bool_literal(false))),
        (3, 2, 1, Some(1), Some(Type::bool_literal(false))),
        (3, 2, 2, Some(2), None),
        (3, 2, 3, None, None),
        (2, 0, 0, Some(0), None),
        (0, 2, 0, None, None),
    ] {
        let specialization = input(&db, variables, types, false);
        let variable = variable(&db, queried);
        let recording = Recording::default();
        let effects = LookupRecording {
            db: &db,
            recording: &recording,
        };
        assert_eq!(
            try_poll_immediate(lookup_specialization_with(
                specialization,
                variable,
                &effects,
            )),
            Poll::Ready(Ok(expected))
        );
        let mut events = vec![
            Event::Context,
            Event::Variables,
            Event::Identity,
            Event::Index,
        ];
        if let Some(index) = index {
            events.extend([Event::Types, Event::TypeAt(index)]);
        }
        assert_eq!(*recording.events.borrow(), events);
        assert_eq!(
            lookup_specialization_sync(
                specialization,
                variable,
                &ordinary::OrdinarySpecializationLookup(&db),
            ),
            Ok(expected)
        );
        assert_eq!(specialization.get(&db, variable), expected);

        for refuse_at in 0..events.len() {
            let refused = Recording {
                refuse_at: Some(refuse_at),
                ..Recording::default()
            };
            let effects = LookupRecording {
                db: &db,
                recording: &refused,
            };
            assert_eq!(
                try_poll_immediate(lookup_specialization_with(
                    specialization,
                    variable,
                    &effects,
                )),
                Poll::Ready(Err(refuse_at))
            );
            assert_eq!(*refused.events.borrow(), events[..=refuse_at]);
        }
    }
}

#[test]
fn stored_lookup_uses_binding_attributes_and_nonce_but_not_defaults() {
    let db = setup_db();
    let env = db.program_environment();
    let program = env.program(&db);
    let platform = if *program.python_platform(&db) == PythonPlatform::All {
        PythonPlatform::Identifier("linux".into())
    } else {
        PythonPlatform::All
    };
    let foreign = Program::new(&db, &platform, program.resolver_environment(&db));
    assert_ne!(foreign, program);
    let raw = TypeVarInstance::new(
        &db,
        TypeVarIdentity::new(
            &db,
            Name::new_static("P"),
            None,
            TypeVarKind::LegacyParamSpec,
        ),
        None,
        None,
        None,
    );
    let bound = BoundTypeVarInstance::new(
        &db,
        raw,
        BindingContext::Synthetic(program),
        None,
        TypeVarNonce::NONE,
    );
    let variables = [
        bound,
        BoundTypeVarInstance::new(
            &db,
            raw,
            BindingContext::Synthetic(foreign),
            None,
            TypeVarNonce::NONE,
        ),
        BoundTypeVarInstance::new(
            &db,
            raw,
            BindingContext::Synthetic(program),
            Some(ParamSpecAttrKind::Args),
            TypeVarNonce::NONE,
        ),
        BoundTypeVarInstance::new(
            &db,
            raw,
            BindingContext::Synthetic(program),
            Some(ParamSpecAttrKind::Kwargs),
            TypeVarNonce::NONE,
        ),
        BoundTypeVarInstance::new(
            &db,
            raw,
            BindingContext::Synthetic(program),
            None,
            TypeVarNonce::NONE.increment(),
        ),
    ];
    let context = GenericContext::from_typevar_instances(&db, &env, variables);
    assert_eq!(context.variables_inner(&db).len(), variables.len());
    let arguments = [
        Type::bool_literal(false),
        Type::bool_literal(true),
        Type::Never,
        Type::any(),
        Type::unknown(),
    ];
    let specialization = Specialization::new(&db, context, Box::from(arguments), None, None);
    let changed_default = BoundTypeVarInstance::new(
        &db,
        TypeVarInstance::new(
            &db,
            raw.identity(&db),
            None,
            None,
            Some(TypeVarDefaultEvaluation::Eager(Type::bool_literal(true))),
        ),
        bound.binding_context(&db),
        None,
        TypeVarNonce::NONE,
    );
    assert_ne!(changed_default, bound);
    assert_eq!(changed_default.identity(&db), bound.identity(&db));
    for (variable, expected) in variables
        .into_iter()
        .zip(arguments)
        .chain([(changed_default, arguments[0])])
    {
        let recording = Recording::default();
        let effects = LookupRecording {
            db: &db,
            recording: &recording,
        };
        assert_eq!(
            try_poll_immediate(lookup_specialization_with(
                specialization,
                variable,
                &effects,
            )),
            Poll::Ready(Ok(Some(expected)))
        );
        assert_eq!(
            lookup_specialization_sync(
                specialization,
                variable,
                &ordinary::OrdinarySpecializationLookup(&db),
            ),
            Ok(Some(expected))
        );
        assert_eq!(specialization.get(&db, variable), Some(expected));
    }
}

#[test]
fn arguments_allocate_only_at_the_first_change_and_preserve_zip_exhaustion() {
    let db = setup_db();
    for (variables, types) in [(3, 3), (2, 3), (3, 2), (0, 3)] {
        let specialization = input(&db, variables, types, false);
        let visited = variables.min(types);
        for changed in std::iter::once(None).chain((0..visited).map(Some)) {
            let effects = Recording {
                changed,
                ..Recording::default()
            };
            let Poll::Ready(Ok(result)) = try_poll_immediate(map_specialization_arguments_with(
                &db,
                specialization,
                &mut ArgumentMapper(&effects),
                &effects,
            )) else {
                panic!("argument traversal did not complete")
            };
            let events = effects.events.borrow();
            assert_eq!(
                events.iter().filter(|event| **event == Event::Next).count(),
                visited + 1
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|event| matches!(event, Event::Map(_)))
                    .count(),
                visited
            );
            if let Some(index) = changed {
                let mut expected = vec![Type::bool_literal(false); visited];
                expected[index] = Type::bool_literal(true);
                assert!(matches!(result, Cow::Owned(_)));
                assert_eq!(&*result, &expected);
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| **event == Event::Allocate(types))
                        .count(),
                    1
                );
                assert!(events.contains(&Event::Prefix(index)));
            } else {
                assert!(matches!(result, Cow::Borrowed(_)));
                assert_eq!(&*result, specialization.types(&db));
                assert!(
                    !events
                        .iter()
                        .any(|event| matches!(event, Event::Allocate(_)))
                );
            }
        }
    }
}

#[test]
fn container_preserves_tuple_order_reconstruction_and_short_circuit_reads() {
    let db = setup_db();
    let env = db.program_environment();
    let visitor = ApplyTypeMappingVisitor::new(&env);
    let specialization = input(&db, 3, 3, true);
    let mapping = TypeMapping::ApplySpecialization(
        crate::types::ApplySpecialization::specialization(specialization),
    );
    for (changed, tuple_changed) in [(None, false), (Some(1), false), (None, true)] {
        let effects = Recording {
            changed,
            tuple_changed,
            ..Recording::default()
        };
        let Poll::Ready(Ok(result)) = try_poll_immediate(map_specialization_with(
            &db,
            specialization,
            &mapping,
            &[],
            &visitor,
            &effects,
        )) else {
            panic!("container mapping did not complete")
        };
        let events = effects.events.borrow().clone();
        assert!(
            events.iter().position(|event| *event == Event::Finish)
                < events.iter().position(|event| *event == Event::Tuple)
        );
        assert_eq!(
            events.iter().filter(|event| **event == Event::Kind).count(),
            if changed.is_none() && !tuple_changed {
                2
            } else {
                1
            }
        );
        assert_eq!(
            result.generic_context(&db),
            specialization.generic_context(&db)
        );
        assert_eq!(result.materialization_kind(&db), None);
        assert_eq!(
            result == specialization,
            changed.is_none() && !tuple_changed
        );
        if let Some(index) = changed {
            assert_eq!(result.types(&db)[index], Type::bool_literal(true));
        }
        assert_eq!(
            result.tuple_inner(&db) == specialization.tuple_inner(&db),
            !tuple_changed
        );
        for refuse_at in 0..events.len() {
            let refused = Recording {
                changed,
                tuple_changed,
                refuse_at: Some(refuse_at),
                ..Recording::default()
            };
            assert!(
                matches!(try_poll_immediate(map_specialization_with(&db, specialization, &mapping, &[], &visitor, &refused)), Poll::Ready(Err(index)) if index == refuse_at)
            );
            assert_eq!(*refused.events.borrow(), events[..=refuse_at]);
        }
    }
    let direct = Recording::default();
    assert!(matches!(
        try_poll_immediate(map_specialization_with(
            &db,
            specialization,
            &TypeMapping::Materialize(MaterializationKind::Top),
            &[],
            &visitor,
            &direct
        )),
        Poll::Ready(Ok(_))
    ));
    assert_eq!(
        *direct.events.borrow(),
        [Event::Classify, Event::Materialize]
    );
    let ordinary = ordinary::MappingSpecializationEffects(&InlineMappingEffects);
    assert_eq!(
        map_specialization_sync(&db, specialization, &mapping, &[], &visitor, &ordinary),
        Ok(specialization)
    );
}

#[test]
fn composition_shares_one_visitor_and_stops_before_later_passes_on_refusal() {
    let db = setup_db();
    let base = input(&db, 1, 1, false);
    for kind in [None, Some(MaterializationKind::Top)] {
        let additional = base.with_materialization_kind(&db, kind);
        let effects = Recording::default();
        assert!(
            matches!(try_poll_immediate(compose_specialization_root_with(&db, base, additional, &effects)), Poll::Ready(Ok(result)) if result == base)
        );
        let mut expected = vec![
            Event::Context,
            Event::Program,
            Event::Environment,
            Event::Fresh,
            Event::Specialize,
            Event::Kind,
        ];
        if kind.is_some() {
            expected.push(Event::Materialize);
        }
        assert_eq!(*effects.events.borrow(), expected);
        let visitors = effects.visitors.borrow();
        assert_eq!(visitors.len(), if kind.is_some() { 2 } else { 1 });
        assert!(visitors.iter().all(|visitor| *visitor == visitors[0]));
        for refuse_at in 0..expected.len() {
            let refused = Recording {
                refuse_at: Some(refuse_at),
                ..Recording::default()
            };
            assert!(
                matches!(try_poll_immediate(compose_specialization_root_with(&db, base, additional, &refused)), Poll::Ready(Err(index)) if index == refuse_at)
            );
            assert_eq!(*refused.events.borrow(), expected[..=refuse_at]);
        }
    }
}
