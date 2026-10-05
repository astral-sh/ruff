use std::cell::RefCell;
use std::task::Poll;

use super::*;
use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use crate::db::tests::TestDbBuilder;
use crate::place::global_symbol;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::{ApplySpecialization, ClassLiteral};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Structural,
    Annotation,
    AnnotationContext,
    Specialization,
    Map,
    Compare,
    Checkpoint,
    Origin,
    Intern,
}

struct Recording<'db> {
    structural: bool,
    contexts: &'db [Type<'db>],
    expected_contexts: &'db [Type<'db>],
    mapped: Specialization<'db>,
    mapping_address: *const (),
    visitor_address: *const (),
    refuse: Option<Event>,
    events: RefCell<Vec<Event>>,
}

impl Recording<'_> {
    fn event(&self, event: Event) -> Result<(), Event> {
        self.events.borrow_mut().push(event);
        if self.refuse == Some(event) {
            Err(event)
        } else {
            Ok(())
        }
    }
}

impl<'db> SynchronousGenericAliasMappingEffects<'db> for Recording<'db> {
    type Error = Event;

    fn structural(&self, _mapping: &TypeMapping<'_, 'db>) -> Result<bool, Event> {
        self.event(Event::Structural)?;
        Ok(self.structural)
    }

    fn annotation(&self, context: TypeContext<'db>) -> Result<Option<Type<'db>>, Event> {
        self.event(Event::Annotation)?;
        Ok(context.annotation)
    }

    fn annotation_context(
        &self,
        _db: &'db dyn Db,
        _alias: GenericAlias<'db>,
        _annotation: Type<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<&'db [Type<'db>], Event> {
        self.event(Event::AnnotationContext)?;
        Ok(self.contexts)
    }

    fn specialization(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Event> {
        self.event(Event::Specialization)?;
        Ok(alias.specialization(db))
    }

    fn map_specialization(
        &self,
        _db: &'db dyn Db,
        _specialization: Specialization<'db>,
        mapping: &TypeMapping<'_, 'db>,
        contexts: &[Type<'db>],
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Specialization<'db>, Event> {
        self.event(Event::Map)?;
        assert_eq!(
            std::ptr::from_ref(mapping).cast::<()>(),
            self.mapping_address
        );
        assert_eq!(
            std::ptr::from_ref(visitor).cast::<()>(),
            self.visitor_address
        );
        assert_eq!(contexts, self.expected_contexts);
        Ok(self.mapped)
    }

    fn same_specialization(
        &self,
        left: Specialization<'db>,
        right: Specialization<'db>,
    ) -> Result<bool, Event> {
        self.event(Event::Compare)?;
        Ok(left == right)
    }

    fn checkpoint(&self, work: MappingWork) -> Result<(), Event> {
        assert!(matches!(work, MappingWork::GenericAliasIntern));
        self.event(Event::Checkpoint)
    }

    fn origin(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Event> {
        self.event(Event::Origin)?;
        Ok(alias.origin(db))
    }

    fn intern(
        &self,
        db: &'db dyn Db,
        origin: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> Result<GenericAlias<'db>, Event> {
        self.event(Event::Intern)?;
        Ok(GenericAlias::new(db, origin, specialization))
    }
}

impl<'db> GenericAliasMappingEffects<'db> for Recording<'db> {
    type Error = Event;

    async fn structural(&self, mapping: &TypeMapping<'_, 'db>) -> Result<bool, Event> {
        SynchronousGenericAliasMappingEffects::structural(self, mapping)
    }

    async fn annotation(&self, context: TypeContext<'db>) -> Result<Option<Type<'db>>, Event> {
        SynchronousGenericAliasMappingEffects::annotation(self, context)
    }

    async fn annotation_context(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
        annotation: Type<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<&'db [Type<'db>], Event> {
        SynchronousGenericAliasMappingEffects::annotation_context(
            self, db, alias, annotation, visitor,
        )
    }

    async fn specialization(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Event> {
        SynchronousGenericAliasMappingEffects::specialization(self, db, alias)
    }

    async fn map_specialization(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        mapping: &TypeMapping<'_, 'db>,
        contexts: &[Type<'db>],
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Specialization<'db>, Event> {
        SynchronousGenericAliasMappingEffects::map_specialization(
            self,
            db,
            specialization,
            mapping,
            contexts,
            visitor,
        )
    }

    async fn same_specialization(
        &self,
        left: Specialization<'db>,
        right: Specialization<'db>,
    ) -> Result<bool, Event> {
        SynchronousGenericAliasMappingEffects::same_specialization(self, left, right)
    }

    async fn checkpoint(&self, work: MappingWork) -> Result<(), Event> {
        SynchronousGenericAliasMappingEffects::checkpoint(self, work)
    }

    async fn origin(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Event> {
        SynchronousGenericAliasMappingEffects::origin(self, db, alias)
    }

    async fn intern(
        &self,
        db: &'db dyn Db,
        origin: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> Result<GenericAlias<'db>, Event> {
        SynchronousGenericAliasMappingEffects::intern(self, db, origin, specialization)
    }
}

fn run<'db>(
    synchronous: bool,
    db: &'db dyn Db,
    alias: GenericAlias<'db>,
    mapping: &TypeMapping<'_, 'db>,
    context: TypeContext<'db>,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    effects: &Recording<'db>,
) -> Result<GenericAlias<'db>, Event> {
    if synchronous {
        map_generic_alias_sync(db, alias, mapping, context, visitor, effects)
    } else {
        match try_poll_immediate(map_generic_alias_with(
            db, alias, mapping, context, visitor, effects,
        )) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("recording operations complete immediately"),
        }
    }
}

/// The synchronous and asynchronous GenericAlias mapping entries preserve the chosen argument
/// context, mapping, and visitor. An unchanged specialization reuses its alias; a changed
/// specialization requests the checkpoint before interning.
#[test]
fn wrapper_order_and_unchanged_alias_identity() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("/src/alias.py", "class Container[T]: ...\n")?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/alias.py")?,
        env.program(&db),
    );
    let origin = global_symbol(&db, file, "Container")
        .place
        .ignore_possibly_undefined()
        .and_then(Type::as_class_literal)
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("Container is not a static class"))?;
    let generic_context = origin
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("Container has no generic context"))?;
    let original = Specialization::new(
        &db,
        generic_context,
        Box::from([Type::bool_literal(false)]),
        None,
        None,
    );
    let changed = Specialization::new(
        &db,
        generic_context,
        Box::from([Type::bool_literal(true)]),
        None,
        None,
    );
    let alias = GenericAlias::new(&db, origin, original);
    let mapping = TypeMapping::ApplySpecialization(ApplySpecialization::specialization(original));
    let visitor = ApplyTypeMappingVisitor::new(&env);
    for synchronous in [false, true] {
        for structural in [false, true] {
            for annotated in [false, true] {
                for mapped in [original, changed] {
                    let context = TypeContext::new(annotated.then_some(Type::GenericAlias(alias)));
                    let mut expected = vec![Event::Structural];
                    if !structural {
                        expected.push(Event::Annotation);
                        if annotated {
                            expected.push(Event::AnnotationContext);
                        }
                    }
                    expected.extend([Event::Specialization, Event::Map, Event::Compare]);
                    if mapped != original {
                        expected.extend([Event::Checkpoint, Event::Origin, Event::Intern]);
                    }
                    let recording = Recording {
                        structural,
                        contexts: original.types(&db),
                        expected_contexts: if annotated && !structural {
                            original.types(&db)
                        } else {
                            &[]
                        },
                        mapped,
                        mapping_address: std::ptr::from_ref(&mapping).cast::<()>(),
                        visitor_address: std::ptr::from_ref(&visitor).cast::<()>(),
                        refuse: None,
                        events: RefCell::new(Vec::new()),
                    };
                    let result = run(
                        synchronous,
                        &db,
                        alias,
                        &mapping,
                        context,
                        &visitor,
                        &recording,
                    );
                    assert_eq!(result, Ok(GenericAlias::new(&db, origin, mapped)));
                    assert_eq!(*recording.events.borrow(), expected);
                    for (index, event) in expected.iter().copied().enumerate() {
                        let refused = Recording {
                            refuse: Some(event),
                            events: RefCell::new(Vec::new()),
                            ..recording
                        };
                        assert_eq!(
                            run(
                                synchronous,
                                &db,
                                alias,
                                &mapping,
                                context,
                                &visitor,
                                &refused
                            ),
                            Err(event)
                        );
                        assert_eq!(*refused.events.borrow(), expected[..=index]);
                    }
                }
            }
        }
    }
    Ok(())
}
