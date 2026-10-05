use salsa::Database as _;
use salsa::prepared_source_probe::Status;
use ty_python_core::platform::PythonPlatform;

use super::*;
use crate::db::tests::TestDb;

fn file<'db>(db: &'db TestDb, env: &ProgramEnvironment<'db>, path: &str) -> ProgramFile<'db> {
    ProgramFile::new(db, system_path_to_file(db, path).unwrap(), env.program(db))
}

fn origin<'db>(db: &'db TestDb, file: ProgramFile<'db>, name: &str) -> StaticClassLiteral<'db> {
    explicit_global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .unwrap()
        .as_static()
        .unwrap()
}

fn assert_supported<T>(fact: &Fact<T>, stamp: Stamp) {
    assert_eq!(fact.stamp, stamp);
    assert!(!fact.support.is_empty());
    for read in &fact.support {
        assert_eq!(read.stamp, stamp);
        assert_eq!(read.status, Status::Final);
        assert!(read.parent.is_none());
    }
}

fn assert_retains_owner<T, U>(child: &Fact<T>, owner: &Fact<U>) {
    for read in &owner.support {
        assert!(child.support.iter().any(|retained| {
            retained.key == read.key
                && retained.memo_address == read.memo_address
                && retained.stamp == read.stamp
                && retained.status == read.status
        }));
    }
}

#[test]
fn construction_facts_preserve_canonical_bases_and_semantic_indices() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/bases.py",
            r#"
from typing import Any, Generic, Protocol, TypeVar
T = TypeVar("T")
class A: ...
class B: ...
class Empty: ...
class Single(A): ...
class Starred(*(A, B)): ...
class Duplicate(A, A): ...
class ExplicitAny(Any): ...
dynamic: Any
class DynamicAny(dynamic): ...
class Legacy(Generic[T]): ...
class Supports(Protocol[T]): ...
class Invalid(42): ...
class Modern[T]: ...
"#,
        )
        .build()?;
    let env = db.program_environment();
    let file = file(&db, &env, "/src/bases.py");
    let names = [
        "Empty",
        "Single",
        "Starred",
        "Duplicate",
        "ExplicitAny",
        "DynamicAny",
        "Legacy",
        "Supports",
        "Invalid",
        "Modern",
    ];
    let classes: Vec<_> = names.iter().map(|name| origin(&db, file, name)).collect();
    let prepared = Rc::new(
        PreparedDeclarations::prepare_construction_only(&db, file, classes.iter().copied())
            .unwrap(),
    );
    assert!(prepared.classes.is_empty());
    assert!(prepared.globals.is_empty());
    assert!(prepared.signatures.is_empty());
    assert!(Router::with_declarations(&db, &env, Rc::clone(&prepared)).is_ok());
    for (&class, name) in classes.iter().zip(names) {
        let observed = probe::capture(&db, || {
            let bases = prepared.explicit_bases(class).unwrap();
            let conversions: Vec<_> = (0..bases.len())
                .map(|index| prepared.converted_explicit_base(class, index).unwrap())
                .collect();
            (
                bases.to_vec(),
                conversions,
                prepared.has_pep_695_type_params(class).unwrap(),
                prepared.object_base(env.program(&db)).unwrap(),
            )
        })
        .unwrap();
        assert!(observed.reads.is_empty(), "{name}: {:?}", observed.reads);
        let (raw, conversions, pep695, object) = observed.value;
        assert_eq!(raw, class.explicit_bases(&db), "{name}");
        assert_eq!(pep695, class.has_pep_695_type_params(&db), "{name}");
        let class_env = ProgramEnvironment::from_scope(class.body_scope(&db));
        assert_eq!(object, ClassBase::object(&db, &class_env));
        for (index, (&ty, converted)) in raw.iter().zip(conversions).enumerate() {
            assert_eq!(
                converted,
                ClassBase::try_from_explicit_base(&db, &class_env, ty, Some(class.into())),
                "{name}[{index}]"
            );
        }
        let packet = &prepared.mro_construction[&class];
        assert_supported(&packet.context, prepared.stamp);
        assert_supported(&packet.explicit_bases, prepared.stamp);
        assert_supported(&packet.has_pep_695_type_params, prepared.stamp);
        assert_retains_owner(&packet.explicit_bases, &packet.context);
        assert_retains_owner(&packet.has_pep_695_type_params, &packet.context);
        for conversion in packet.converted_explicit_bases.iter().flatten() {
            assert_supported(conversion, prepared.stamp);
            assert_retains_owner(conversion, &packet.explicit_bases);
        }
    }
    assert!(prepared.explicit_bases(classes[0]).unwrap().is_empty());
    assert_eq!(prepared.explicit_bases(classes[2]).unwrap().len(), 2);
    assert_eq!(
        prepared.explicit_bases(classes[3]).unwrap()[0],
        prepared.explicit_bases(classes[3]).unwrap()[1]
    );
    assert_eq!(
        prepared.converted_explicit_base(classes[4], 0).unwrap(),
        Some(ClassBase::Any)
    );
    assert_ne!(
        prepared.converted_explicit_base(classes[5], 0).unwrap(),
        Some(ClassBase::Any)
    );
    assert_eq!(
        prepared.converted_explicit_base(classes[8], 0).unwrap(),
        None
    );
    assert_supported(prepared.object_base.as_ref().unwrap(), prepared.stamp);
    Ok(())
}

#[test]
fn construction_facts_preserve_inherited_forwarding_aliases() -> anyhow::Result<()> {
    for markdown in [PEP695, LEGACY] {
        let source = code(markdown, CASES[1])?;
        let db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file("/src/forwarding.py", &source)
            .build()?;
        let env = db.program_environment();
        let file = file(&db, &env, "/src/forwarding.py");
        let mut prepared = PreparedDeclarations::prepare(&db, file).unwrap();
        assert!(prepared.mro_construction.is_empty());
        assert!(prepared.object_base.is_none());
        for name in ["Forward", "Base"] {
            let class = class_named(&prepared, name);
            prepared.prepare_construction(class).unwrap();
            let packet = &prepared.mro_construction[&class];
            let original = &prepared.classes[&class].context;
            assert_eq!(packet.context.value, original.value);
            assert_eq!(packet.context.support.len(), original.support.len());
            assert_retains_owner(&packet.context, original);
            assert_eq!(prepared.context(class).unwrap(), original.value);
            let observed = probe::capture(&db, || {
                prepared
                    .explicit_bases(class)
                    .unwrap()
                    .iter()
                    .enumerate()
                    .map(|(index, ty)| {
                        (*ty, prepared.converted_explicit_base(class, index).unwrap())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap();
            assert!(observed.reads.is_empty());
            let class_env = ProgramEnvironment::from_scope(class.body_scope(&db));
            for (index, (raw, converted)) in observed.value.into_iter().enumerate() {
                assert_eq!(raw, class.explicit_bases(&db)[index]);
                assert_eq!(
                    converted,
                    ClassBase::try_from_explicit_base(&db, &class_env, raw, Some(class.into()))
                );
            }
        }
    }
    Ok(())
}

#[test]
fn construction_only_context_does_not_supply_namespace_or_proper_tail_facts() -> anyhow::Result<()>
{
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/bases.py",
            "class A: ...\nclass B: ...\nclass Both(A, B): ...\n",
        )
        .build()?;
    let env = db.program_environment();
    let file = file(&db, &env, "/src/bases.py");
    let class = origin(&db, file, "Both");
    let mut prepared = PreparedDeclarations::prepare_construction_only(&db, file, [class]).unwrap();
    let second = prepared.converted_explicit_base(class, 1).unwrap();
    prepared
        .mro_construction
        .get_mut(&class)
        .unwrap()
        .converted_explicit_bases[0] = None;
    let observed = probe::capture(&db, || {
        assert_eq!(
            prepared.converted_explicit_base(class, 0),
            Err(MissingDeclaration(DeclarationKey::ConvertedExplicitBase(
                class, 0
            )))
        );
        assert_eq!(prepared.converted_explicit_base(class, 1), Ok(second));
        assert_eq!(
            prepared.converted_explicit_base(class, 2),
            Err(MissingDeclaration(DeclarationKey::ConvertedExplicitBase(
                class, 2
            )))
        );
        assert!(prepared.context(class).is_ok());
        assert_eq!(
            prepared.namespace(class, "__init__"),
            Err(MissingDeclaration(DeclarationKey::Namespace(
                class,
                Name::new("__init__")
            )))
        );
        assert_eq!(
            prepared.inherited_generic_context(class),
            Err(MissingDeclaration(DeclarationKey::ClassInput(
                class,
                ClassInput::InheritedGenericContext
            )))
        );
        assert_eq!(
            prepared.proper_mro(class),
            Err(MissingDeclaration(DeclarationKey::ProperMro(class)))
        );
    })
    .unwrap();
    assert!(observed.reads.is_empty());
    prepared.mro_construction.remove(&class);
    let observed = probe::capture(&db, || {
        assert_eq!(
            prepared.context(class),
            Err(MissingDeclaration(DeclarationKey::Context(class)))
        );
        assert_eq!(
            prepared.explicit_bases(class),
            Err(MissingDeclaration(DeclarationKey::ExplicitBases(class)))
        );
        assert_eq!(
            prepared.has_pep_695_type_params(class),
            Err(MissingDeclaration(DeclarationKey::ClassInput(
                class,
                ClassInput::HasPep695TypeParams
            )))
        );
        assert_eq!(
            prepared.converted_explicit_base(class, 1),
            Err(MissingDeclaration(DeclarationKey::ConvertedExplicitBase(
                class, 1
            )))
        );
    })
    .unwrap();
    assert!(observed.reads.is_empty());
    prepared.object_base = None;
    assert_eq!(
        prepared.object_base(env.program(&db)),
        Err(MissingDeclaration(DeclarationKey::ObjectBase(
            env.program(&db)
        )))
    );
    Ok(())
}

#[test]
fn invalid_construction_inputs_do_not_weaken_full_preparation() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/invalid.py", "class Invalid(42): ...\n")
        .build()?;
    let env = db.program_environment();
    let file = file(&db, &env, "/src/invalid.py");
    let class = origin(&db, file, "Invalid");
    let prepared = PreparedDeclarations::prepare_construction_only(&db, file, [class]).unwrap();
    assert_eq!(prepared.converted_explicit_base(class, 0), Ok(None));
    assert!(prepared.classes.is_empty());
    assert!(
        matches!(PreparedDeclarations::prepare(&db, file), Err(PreparationError::InvalidMro(invalid)) if invalid == class)
    );
    Ok(())
}

#[test]
fn independently_captured_contexts_include_cached_and_source_free_tracked_roots()
-> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/empty.py", "")
        .build()?;
    let env = db.program_environment();
    let file = file(&db, &env, "/src/empty.py");
    for known in [KnownClass::Object, KnownClass::VersionInfo] {
        let class = known
            .to_class_literal(&db, &env)
            .as_class_literal()
            .unwrap()
            .as_static()
            .unwrap();
        let context = class.generic_context(&db);
        let prepared = PreparedDeclarations::prepare_construction_only(&db, file, [class]).unwrap();
        assert!(prepared.classes.is_empty());
        assert!(prepared.globals.is_empty());
        assert!(prepared.signatures.is_empty());
        assert_eq!(prepared.context(class), Ok(context));
        let owner = &prepared.mro_construction[&class].context;
        assert_supported(owner, prepared.stamp);
        assert!(owner.support.iter().any(|read| {
            salsa::Database::ingredient_debug_name(&db, read.key.ingredient_index())
                .contains("generic_context")
        }));
        assert_eq!(
            prepared.proper_mro(class),
            Err(MissingDeclaration(DeclarationKey::ProperMro(class)))
        );
    }
    Ok(())
}

#[test]
fn construction_packets_require_the_capsule_program_and_stamp() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/owner.py", "class Owner: ...\n")
        .with_file("/src/other.py", "class External: ...\n")
        .build()?;
    let stale_stamp = Stamp::current(&db);
    db.trigger_cancellation();
    assert!(!stale_stamp.belongs_to(&db));
    let env = db.program_environment();
    let owner_file = file(&db, &env, "/src/owner.py");
    let external = origin(&db, file(&db, &env, "/src/other.py"), "External");
    let mut prepared =
        PreparedDeclarations::prepare_construction_only(&db, owner_file, [external]).unwrap();
    assert!(prepared.context(external).is_ok());
    assert!(prepared.matches(&db, &env));

    let other_program = Program::new(&db, &PythonPlatform::All, env.resolver_environment(&db));
    assert_ne!(other_program, env.program(&db));
    let other_env = ProgramEnvironment::from_program(other_program);
    let foreign = origin(&db, file(&db, &other_env, "/src/other.py"), "External");
    let count = prepared.mro_construction.len();
    assert!(
        matches!(prepared.prepare_construction(foreign), Err(PreparationError::ProgramDomain(class)) if class == foreign)
    );
    assert_eq!(prepared.mro_construction.len(), count);
    assert_eq!(
        prepared.object_base(other_program),
        Err(MissingDeclaration(DeclarationKey::ObjectBase(
            other_program
        )))
    );

    // An active capsule borrows the database, so a global cancellation cannot be triggered
    // while it is retained. Use the actual prior epoch to exercise its stale-header check.
    prepared.stamp = stale_stamp;
    assert!(
        matches!(prepared.prepare_construction(external), Err(PreparationError::Observation(DeclarationKey::File(file), CaptureError::ChangedDatabaseStamp)) if file == owner_file)
    );
    assert_eq!(prepared.mro_construction.len(), count);
    Ok(())
}

#[test]
fn failed_capture_does_not_publish_a_construction_packet_or_object_fact() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/atomic.py", "class A: ...\nclass B: ...\n")
        .build()?;
    let env = db.program_environment();
    let file = file(&db, &env, "/src/atomic.py");
    let mut prepared = PreparedDeclarations::prepare(&db, file).unwrap();
    let a = class_named(&prepared, "A");
    let b = class_named(&prepared, "B");

    // A retained full context is already supported. Rejection of the next capture leaves
    // the construction packet and object fact unpublished.
    let observed = probe::capture(&db, || prepared.prepare_construction(a)).unwrap();
    assert!(
        matches!(observed.value, Err(PreparationError::Observation(DeclarationKey::ExplicitBases(class), CaptureError::NestedCapture)) if class == a)
    );
    assert!(prepared.mro_construction.is_empty());
    assert!(prepared.object_base.is_none());
    prepared.prepare_construction(a).unwrap();
    let object = prepared.object_base(env.program(&db)).unwrap();
    let observed = probe::capture(&db, || prepared.prepare_construction(b)).unwrap();
    assert!(
        matches!(observed.value, Err(PreparationError::Observation(DeclarationKey::ExplicitBases(class), CaptureError::NestedCapture)) if class == b)
    );
    assert_eq!(prepared.mro_construction.len(), 1);
    assert!(prepared.mro_construction.contains_key(&a));
    assert_eq!(prepared.object_base(env.program(&db)), Ok(object));

    let observed = probe::capture(&db, || {
        PreparedDeclarations::prepare_construction_only(&db, file, [b])
    })
    .unwrap();
    assert!(
        matches!(observed.value, Err(PreparationError::Observation(DeclarationKey::Context(class), CaptureError::NestedCapture)) if class == b)
    );
    Ok(())
}

#[test]
fn capture_remainder_rejection_discards_previously_captured_inputs() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/atomic.py", "class A: ...\nclass B(A): ...\n")
        .build()?;
    let env = db.program_environment();
    let file = file(&db, &env, "/src/atomic.py");
    for retain_previous_packet in [false, true] {
        let mut prepared = PreparedDeclarations::prepare(&db, file).unwrap();
        let a = class_named(&prepared, "A");
        let b = class_named(&prepared, "B");
        if retain_previous_packet {
            prepared.prepare_construction(a).unwrap();
        }
        let previous_object = prepared.object_base.as_ref().map(|fact| fact.value);
        let owner = &prepared.classes[&b].context;
        let class_env = ProgramEnvironment::from_scope(b.body_scope(&db));
        let context = owner.stored(owner.value);
        let raw: Fact<Box<[Type<'_>]>> = context
            .derive(&db, DeclarationKey::ExplicitBases(b), || {
                b.explicit_bases(&db).into()
            })
            .unwrap();
        assert_supported(&context, prepared.stamp);
        assert_supported(&raw, prepared.stamp);
        assert_retains_owner(&raw, &context);
        assert_eq!(raw.value.len(), 1);

        // Exercise the capture boundary after the same supported context and raw-base reads
        // used by preparation have completed. The enclosing capture rejects the next fact.
        let observed = probe::capture(&db, || {
            prepared.capture_construction_remainder(b, &class_env, context, raw)
        })
        .unwrap();
        assert!(matches!(observed.value, Err(PreparationError::Observation(
            DeclarationKey::ClassInput(class, ClassInput::HasPep695TypeParams),
            CaptureError::NestedCapture,
        )) if class == b));
        assert_eq!(
            prepared.mro_construction.len(),
            usize::from(retain_previous_packet)
        );
        assert_eq!(
            prepared.mro_construction.contains_key(&a),
            retain_previous_packet
        );
        assert!(!prepared.mro_construction.contains_key(&b));
        assert_eq!(
            prepared.object_base.as_ref().map(|fact| fact.value),
            previous_object
        );
    }
    Ok(())
}

#[test]
fn publication_rejects_a_stale_stamp_after_all_facts_are_captured() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/atomic.py", "class A: ...\nclass B(A): ...\n")
        .build()?;
    let stale_stamp = Stamp::current(&db);
    db.trigger_cancellation();
    assert!(!stale_stamp.belongs_to(&db));
    let env = db.program_environment();
    let file = file(&db, &env, "/src/atomic.py");
    for retain_previous_packet in [false, true] {
        let mut prepared = PreparedDeclarations::prepare(&db, file).unwrap();
        let a = class_named(&prepared, "A");
        let b = class_named(&prepared, "B");
        if retain_previous_packet {
            prepared.prepare_construction(a).unwrap();
        }
        let previous_object = prepared.object_base.as_ref().map(|fact| fact.value);
        let owner = &prepared.classes[&b].context;
        let class_env = ProgramEnvironment::from_scope(b.body_scope(&db));
        let context = owner.stored(owner.value);
        let raw: Fact<Box<[Type<'_>]>> = context
            .derive(&db, DeclarationKey::ExplicitBases(b), || {
                b.explicit_bases(&db).into()
            })
            .unwrap();
        let (facts, object) = prepared
            .capture_construction_remainder(b, &class_env, context, raw)
            .unwrap();
        assert_supported(&facts.context, prepared.stamp);
        assert_supported(&facts.explicit_bases, prepared.stamp);
        assert_supported(&facts.has_pep_695_type_params, prepared.stamp);
        assert_eq!(facts.converted_explicit_bases.len(), 1);
        assert_supported(
            facts.converted_explicit_bases[0].as_ref().unwrap(),
            prepared.stamp,
        );
        assert_eq!(object.is_some(), !retain_previous_packet);
        if let Some(object) = &object {
            assert_supported(object, prepared.stamp);
        }
        assert!(!prepared.mro_construction.contains_key(&b));
        assert_eq!(
            prepared.object_base.as_ref().map(|fact| fact.value),
            previous_object
        );

        // This tests the final publication guard directly: the complete captured values remain
        // valid, but a stale capsule header cannot authorize inserting them.
        prepared.stamp = stale_stamp;
        assert!(matches!(prepared.publish_construction(b, facts, object),
            Err(PreparationError::Observation(DeclarationKey::File(observed_file), CaptureError::ChangedDatabaseStamp))
                if observed_file == file));
        assert_eq!(
            prepared.mro_construction.len(),
            usize::from(retain_previous_packet)
        );
        assert_eq!(
            prepared.mro_construction.contains_key(&a),
            retain_previous_packet
        );
        assert!(!prepared.mro_construction.contains_key(&b));
        assert_eq!(
            prepared.object_base.as_ref().map(|fact| fact.value),
            previous_object
        );
    }
    Ok(())
}
