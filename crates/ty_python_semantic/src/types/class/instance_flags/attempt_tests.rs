use std::cell::Cell;

use ruff_db::files::system_path_to_file;
use salsa::plumbing::AsId;

use super::tests::{class, database, executions};
use super::{
    AttemptInstanceFlagsEffects, ClassInstanceFlags, InstanceFlagsEffects, InstanceFlagsWork,
    inherited_instance_flags_with, own_custom_getattribute_with, sealed,
};
use crate::Db;
use crate::db::tests::TestDbBuilder;
use crate::place::global_symbol;
use crate::types::class::source::SourceClassEffects;
use crate::types::constructor::expansion_probe::{self, Incomplete, Observation};
use crate::types::instance::attempt::UnsupportedInstanceOperation;
use crate::types::mro::iteration::MroCursor;
use crate::types::mro::source::DeclarationMroCursor;
use crate::types::source_read::read_source;
use crate::types::{ClassBase, StaticClassLiteral, Type};

fn complete<T>(result: Result<Result<T, Incomplete>, Incomplete>) -> anyhow::Result<T> {
    result
        .and_then(|result| result)
        .map_err(|error| anyhow::anyhow!("{error:?}"))
}

#[test]
fn declared_defaults_remain_source_owned_when_reading_inherited_flags() -> anyhow::Result<()> {
    declared_defaults_flags("Base")
}

#[test]
fn declared_base_defaults_remain_source_owned_when_reading_inherited_flags() -> anyhow::Result<()> {
    declared_defaults_flags("Receiver")
}

fn declared_defaults_flags(name: &str) -> anyhow::Result<()> {
    for source in [
        "class Base[T = int]: ...\nclass Receiver(Base): ...\n",
        "from typing import Generic, TypeVar\nT = TypeVar('T', default=int)\nclass Base(Generic[T]): ...\nclass Receiver(Base): ...\n",
    ] {
        let db = TestDbBuilder::new()
            .with_file("/src/instance_flags.pyi", source)
            .build()?;
        let receiver = class(&db, name)?;
        let (actual, _) = expansion_probe::run_mro(&db, 100_000, || {
            read_source(&AttemptInstanceFlagsEffects::new(&db), || {
                receiver.inherited_instance_flags(&db)
            })
        });
        assert_eq!(complete(actual)?, ClassInstanceFlags::empty());
        if name == "Base" {
            let (generated, _) = expansion_probe::run_mro(&db, 100_000, || {
                inherited_instance_flags_with(&db, receiver, &AttemptInstanceFlagsEffects::new(&db))
            });
            assert_eq!(complete(generated)?, ClassInstanceFlags::empty());
        }
    }
    Ok(())
}

#[test]
fn declaration_metaclass_cursor_retains_exact_specialized_ancestors() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file(
            "/src/instance_flags.pyi",
            r#"
class Parent[T](type): ...
class Meta[T](Parent[list[T]]): ...
class Owner(metaclass=Meta[int]): ...
ExpectedMeta = Meta[int]
ExpectedParent = Parent[list[int]]
"#,
        )
        .build()?;
    let owner = class(&db, "Owner")?;
    let (actual, _) = expansion_probe::run_mro(&db, 100_000, || {
        let mut entries = Vec::new();
        if let Some(mut cursor) = DeclarationMroCursor::for_metaclass_of(&db, owner)? {
            while let Some(base) = cursor.next(&db)? {
                entries.push(base);
            }
        }
        Ok(entries)
    });
    let actual = complete(actual)?;
    let metaclass = owner
        .metaclass(&db)
        .to_class_type(&db)
        .ok_or_else(|| anyhow::anyhow!("missing metaclass"))?;
    assert_eq!(actual, metaclass.iter_mro(&db).collect::<Vec<_>>());
    let file = db.program_file(system_path_to_file(&db, "/src/instance_flags.pyi")?);
    for (index, name) in [(0, "ExpectedMeta"), (1, "ExpectedParent")] {
        let expected = global_symbol(&db, file, name)
            .place
            .ignore_possibly_undefined()
            .ok_or_else(|| anyhow::anyhow!("missing {name}"))?;
        assert!(matches!(expected, Type::GenericAlias(_)));
        assert_eq!(actual.get(index).copied().map(Type::from), Some(expected));
    }
    Ok(())
}

#[test]
fn source_metaclass_classification_preserves_shortcuts() -> anyhow::Result<()> {
    for own in [false, true] {
        let source = format!(
            r#"
def interceptor(self, name): ...
class Parent[T](type): ...
class Meta[T](Parent[list[T]]):
    def __init__(cls, *args):
        cls.__getattribute__ = interceptor
class Owner(metaclass=Meta[int]):
    {}
"#,
            if own {
                "def __getattribute__(self, name: str): ..."
            } else {
                "..."
            }
        );
        let db = TestDbBuilder::new()
            .with_file("/src/instance_flags.pyi", &source)
            .build()?;
        let owner = class(&db, "Owner")?;
        if !own {
            owner.metaclass(&db);
        }
        executions(&db);
        let (result, _) = expansion_probe::run_mro(&db, 100_000, || {
            own_custom_getattribute_with(&db, owner, &SourceClassEffects::new(&db))
        });
        assert!(complete(result)?);
        let reads = executions(&db);
        assert!(
            !reads.iter().any(|name| name == "source_alias_mro"),
            "{reads:?}"
        );
        if own {
            assert!(
                !reads
                    .iter()
                    .any(|name| name.contains("metaclass") || name == "implicit_attribute_names"),
                "{reads:?}"
            );
        } else {
            assert!(
                reads.iter().any(|name| name == "implicit_attribute_names"),
                "{reads:?}"
            );
            let (entries, _) = expansion_probe::run_mro(&db, 100_000, || {
                let Some(mut cursor) = DeclarationMroCursor::for_metaclass_of(&db, owner)? else {
                    return Ok(Vec::new());
                };
                Ok(vec![cursor.next(&db)?, cursor.next(&db)?])
            });
            assert_eq!(complete(entries)?.iter().flatten().count(), 2);
            let reads = executions(&db);
            assert!(
                reads.iter().any(|name| name == "source_alias_mro"),
                "{reads:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn flags_provider_refuses_without_installed_mro_effects() -> anyhow::Result<()> {
    let db = database()?;
    let receiver = class(&db, "Receiver")?;
    executions(&db);
    let (result, _) = expansion_probe::run(&db, 100_000, || {
        receiver.instance_flags_with(&db, &AttemptInstanceFlagsEffects::new(&db))
    });
    assert_eq!(
        result.and_then(|result| result),
        Err(Incomplete::UnsupportedInstanceOperation(
            UnsupportedInstanceOperation::UncontrolledMro
        ))
    );
    assert!(executions(&db).is_empty());
    Ok(())
}

#[test]
fn installed_flags_preserve_raw_values_and_cold_source_order() -> anyhow::Result<()> {
    for name in ["Owner", "Receiver", "Own", "Inherited", "Invalid", "Cycle"] {
        let ordinary_db = database()?;
        let ordinary = class(&ordinary_db, name)?;
        executions(&ordinary_db);
        let expected = ordinary.instance_flags(&ordinary_db);
        let expected_reads = executions(&ordinary_db);

        let db = database()?;
        let owner = class(&db, name)?;
        executions(&db);
        let (result, _) = expansion_probe::run_mro(&db, 100_000, || {
            read_source(&AttemptInstanceFlagsEffects::new(&db), || {
                owner.instance_flags(&db)
            })
        });
        assert_eq!(complete(result)?, expected, "{name}");
        assert_eq!(executions(&db), expected_reads, "{name}");
    }
    Ok(())
}

#[test]
fn metaclass_refusal_occurs_only_after_the_own_symbol_shortcut() -> anyhow::Result<()> {
    for (name, expected) in [
        (
            "Installed",
            Err(Incomplete::UnsupportedInstanceOperation(
                UnsupportedInstanceOperation::MetaclassAttributeClassification,
            )),
        ),
        (
            "OwnInstalled",
            Ok(ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE),
        ),
    ] {
        let db = database()?;
        let owner = class(&db, name)?;
        executions(&db);
        let (result, _) = expansion_probe::run_mro(&db, 100_000, || {
            owner.instance_flags_with(&db, &AttemptInstanceFlagsEffects::new(&db))
        });
        assert_eq!(result.and_then(|result| result), expected, "{name}");
        let reads = executions(&db);
        assert!(reads.iter().any(|name| name == "place_table"), "{reads:?}");
        assert!(
            !reads
                .iter()
                .any(|name| name.contains("metaclass") || name == "implicit_attribute_names"),
            "{reads:?}"
        );
    }
    Ok(())
}

struct Refusing<'db> {
    db: &'db dyn Db,
    inner: AttemptInstanceFlagsEffects<'db>,
    work: InstanceFlagsWork,
    occurrence: usize,
    seen: Cell<usize>,
    symbol_reads: Cell<usize>,
}

impl sealed::Sealed for Refusing<'_> {}

impl<'db> InstanceFlagsEffects<'db> for Refusing<'db> {
    type Error = Incomplete;
    type Cursor = MroCursor<'db>;

    fn start_mro(&self, class: StaticClassLiteral<'db>) -> Result<Self::Cursor, Incomplete> {
        self.inner.start_mro(class)
    }

    fn checkpoint(&self, work: InstanceFlagsWork) -> Result<(), Incomplete> {
        self.inner.checkpoint(work)?;
        if work == self.work {
            let occurrence = self.seen.get();
            self.seen.set(occurrence + 1);
            if occurrence == self.occurrence {
                return Err(expansion_probe::refuse(self.db, Incomplete::Interrupted));
            }
        }
        Ok(())
    }

    fn inherited_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassInstanceFlags, Incomplete> {
        self.inner.inherited_flags(class)
    }

    fn next_base(&self, cursor: &mut MroCursor<'db>) -> Result<Option<ClassBase<'db>>, Incomplete> {
        self.inner.next_base(cursor)
    }

    fn has_own_symbol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Incomplete> {
        let result = self.inner.has_own_symbol(class)?;
        self.symbol_reads.set(self.symbol_reads.get() + 1);
        Ok(result)
    }

    fn metaclass_custom_getattribute(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Incomplete> {
        self.inner.metaclass_custom_getattribute(class)
    }
}

#[test]
fn refusal_stops_before_symbol_reads_tail_advancement_and_publication() -> anyhow::Result<()> {
    for (work, occurrence) in [
        (InstanceFlagsWork::BodySymbol, 0),
        (InstanceFlagsWork::Advance, 1),
        (InstanceFlagsWork::Publish, 0),
    ] {
        let db = database()?;
        let receiver = class(&db, "Receiver")?;
        let untouched = class(&db, "Inherited")?;
        let effects = Refusing {
            db: &db,
            inner: AttemptInstanceFlagsEffects::new(&db),
            work,
            occurrence,
            seen: Cell::new(0),
            symbol_reads: Cell::new(0),
        };
        executions(&db);
        let (result, _) = expansion_probe::run_mro(&db, 100_000, || {
            assert_eq!(
                inherited_instance_flags_with(&db, receiver, &effects),
                Err(Incomplete::Interrupted),
            );
            let prefix = executions(&db);
            match work {
                InstanceFlagsWork::BodySymbol => {
                    assert_eq!(effects.symbol_reads.get(), 0);
                    assert!(
                        !prefix.iter().any(|name| name == "try_mro_unspecialized"),
                        "{prefix:?}"
                    );
                }
                InstanceFlagsWork::Advance => {
                    assert_eq!(effects.symbol_reads.get(), 1);
                    assert!(
                        prefix.iter().any(|name| name == "place_table"),
                        "{prefix:?}"
                    );
                    assert!(
                        !prefix.iter().any(|name| name == "try_mro_unspecialized"),
                        "{prefix:?}"
                    );
                }
                InstanceFlagsWork::Publish => {
                    assert_eq!(effects.symbol_reads.get(), 2);
                    assert!(
                        prefix.iter().any(|name| name == "try_mro_unspecialized"),
                        "{prefix:?}"
                    );
                }
                _ => {}
            }
            assert!(untouched.instance_flags(&db).is_empty());
            assert!(
                executions(&db).is_empty(),
                "stopped raw flags owner read a child"
            );
        });
        assert_eq!(result, Err(Incomplete::Interrupted));
        assert_eq!(effects.seen.get(), occurrence + 1);

        for _ in 0..2 {
            let (result, _) = expansion_probe::run_mro(&db, 100_000, || {
                receiver.instance_flags_with(&db, &AttemptInstanceFlagsEffects::new(&db))
            });
            assert!(complete(result)?.is_empty());
        }
    }
    Ok(())
}

#[test]
fn interrupted_tracked_flags_retry_at_the_same_revision() -> anyhow::Result<()> {
    let source = r#"
from typing import Any

class Owner: ...
class Factory:
    def __new__(cls) -> type[Owner]: ...
class Receiver(Factory()): ...
class Inherited:
    def __getattribute__(self, name: str) -> Any: ...
"#;
    let ordinary_db = TestDbBuilder::new()
        .with_file("/src/instance_flags.pyi", source)
        .build()?;
    let expected = class(&ordinary_db, "Receiver")?.instance_flags(&ordinary_db);

    let db = TestDbBuilder::new()
        .with_file("/src/instance_flags.pyi", source)
        .build()?;
    let receiver = class(&db, "Receiver")?;
    let other = class(&db, "Inherited")?;
    let factory = class(&db, "Factory")?;
    executions(&db);
    let (result, statistics) = expansion_probe::run_mro_observed(&db, 3, || {
        let result = read_source(&AttemptInstanceFlagsEffects::new(&db), || {
            receiver.instance_flags(&db)
        });
        assert_eq!(result, Err(Incomplete::Allowance));
        let first = executions(&db);
        assert!(
            first.iter().any(|name| name == "instance_flags_inner"),
            "{first:?}"
        );
        assert!(other.instance_flags(&db).is_empty());
        assert!(executions(&db).is_empty());
        // Calling the cold tracked owner itself is permitted, but its body must demand no child.
        assert!(other.inherited_instance_flags(&db).is_empty());
        assert_eq!(executions(&db), ["instance_flags_inner"]);
    });
    assert_eq!(result, Err(Incomplete::Allowance));
    assert!(statistics.observations().iter().any(|observation| matches!(
        observation,
        Observation::Constructor(id) if *id == factory.as_id()
    )));

    for retry in 0..2 {
        let (result, _) = expansion_probe::run_mro(&db, 100_000, || {
            receiver.instance_flags_with(&db, &AttemptInstanceFlagsEffects::new(&db))
        });
        assert_eq!(complete(result)?, expected);
        let reads = executions(&db);
        assert_eq!(
            reads.iter().any(|name| name == "instance_flags_inner"),
            retry == 0,
            "{reads:?}"
        );
        if retry == 0 {
            assert!(
                reads.iter().any(|name| name == "try_mro_unspecialized"),
                "{reads:?}"
            );
        }
    }
    let (result, _) = expansion_probe::run_mro(&db, 100_000, || {
        other.instance_flags_with(&db, &AttemptInstanceFlagsEffects::new(&db))
    });
    assert_eq!(
        complete(result)?,
        ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE
    );
    Ok(())
}
