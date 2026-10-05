use ruff_db::files::system_path_to_file;
use ruff_python_ast::name::Name;
use salsa::Database as _;
use ty_python_core::{ProgramFile, place_table};

use super::{ClassInstanceFlags, InlineInstanceFlagsEffects, own_custom_getattribute_with};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class::implicit_attributes::implicit_attribute_names;
use crate::types::{ClassBase, ClassLiteral, KnownClass, StaticClassLiteral, Type};

pub(super) fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file(
            "/src/instance_flags.pyi",
            r#"
from typing import Any, TypedDict

class Owner: ...
class Receiver(Owner): ...
class Own:
    def __getattribute__(self, name: str) -> Any: ...
class Inherited(Own): ...
class FromAny(Any): ...
class FromUnknown(Missing): ...
class Record(TypedDict):
    item: int
Dynamic = type("Dynamic", (), {})
class FromDynamic(Dynamic): ...
class Invalid(1): ...
class Cycle(Cycle): ...

def interceptor(self, name): ...
class Installer(type):
    def __init__(cls, *args):
        cls.__getattribute__ = interceptor
class Installed(metaclass=Installer): ...
class OwnInstalled(metaclass=Installer):
    def __getattribute__(self, name: str) -> Any: ...
"#,
        )
        .build()
}

pub(super) fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/instance_flags.pyi")?,
        env.program(db),
    );
    global_symbol(db, file, name)
        .place
        .ignore_possibly_undefined()
        .and_then(Type::as_class_literal)
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing static class {name}"))
}

pub(super) fn executions(db: &TestDb) -> Vec<String> {
    db.clone()
        .take_salsa_events()
        .into_iter()
        .filter_map(|event| {
            let salsa::EventKind::WillExecute { database_key } = event.kind else {
                return None;
            };
            let name = db.ingredient_debug_name(database_key.ingredient_index());
            let name = name
                .rsplit("::")
                .next()
                .unwrap_or(name.as_ref())
                .trim_end_matches('_');
            // Both paths retain a tracked flags owner with the same seed. Normalize only the
            // independent oracle's owner name; every child query remains in source order.
            Some(if name == "original_flags_inner" {
                "instance_flags_inner".to_owned()
            } else {
                name.to_owned()
            })
        })
        .collect()
}

fn original_own_attribute<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> bool {
    if matches!(class.known(db), Some(KnownClass::Object | KnownClass::Type)) {
        return false;
    }
    if place_table(db, class.body_scope(db))
        .symbol_id("__getattribute__")
        .is_some()
    {
        return true;
    }
    if !class.has_explicit_metaclass(db) {
        return false;
    }
    let Some(metaclass) = class.metaclass(db).to_class_type(db) else {
        return true;
    };
    metaclass.iter_mro(db).any(|base| match base {
        ClassBase::Any | ClassBase::Dynamic(_) | ClassBase::Divergent(_) => true,
        ClassBase::Class(base) => base.static_class_literal(db).is_none_or(|(base, _)| {
            implicit_attribute_names(db, base.body_scope(db))
                .binary_search(&Name::new_static("__getattribute__"))
                .is_ok()
        }),
        ClassBase::Generic | ClassBase::Protocol | ClassBase::TypedDict(_) => false,
    })
}

#[salsa::tracked(configuration = (pub(in crate::types) OriginalFlagsInnerConfiguration), attempt = ReturnOnly, returns(copy), cycle_initial=|_, _, _| ClassInstanceFlags::empty())]
fn original_flags_inner<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> ClassInstanceFlags {
    let mut flags = ClassInstanceFlags::empty();
    for base in class.iter_mro(db, None) {
        match base {
            ClassBase::Any => flags.insert(
                ClassInstanceFlags::INHERITS_FROM_EXPLICIT_ANY
                    | ClassInstanceFlags::HAS_DYNAMIC_GETATTRIBUTE,
            ),
            ClassBase::Dynamic(_) | ClassBase::Divergent(_) => {
                flags.insert(ClassInstanceFlags::HAS_DYNAMIC_GETATTRIBUTE);
            }
            ClassBase::TypedDict(_) => flags.insert(ClassInstanceFlags::TYPED_DICT),
            ClassBase::Class(base)
                if base
                    .static_class_literal(db)
                    .is_none_or(|(base, _)| original_own_attribute(db, base)) =>
            {
                flags.insert(ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE);
            }
            ClassBase::Class(_) | ClassBase::Generic | ClassBase::Protocol => {}
        }
    }
    flags
}

fn original_flags<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> ClassInstanceFlags {
    let mut flags = if let Some(known) = class.known(db) {
        if known.is_typed_dict_subclass() {
            ClassInstanceFlags::TYPED_DICT
        } else {
            ClassInstanceFlags::empty()
        }
    } else if class.has_explicit_bases(db) {
        return original_flags_inner(db, class);
    } else {
        ClassInstanceFlags::empty()
    };
    flags.set(
        ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE,
        original_own_attribute(db, class),
    );
    flags
}

#[test]
fn raw_flags_and_cold_source_order_match_the_original_bodies() -> anyhow::Result<()> {
    for name in [
        "Owner",
        "Receiver",
        "Own",
        "Inherited",
        "FromAny",
        "FromUnknown",
        "Record",
        "FromDynamic",
        "Invalid",
        "Cycle",
        "Installed",
        "OwnInstalled",
    ] {
        let original_db = database()?;
        let original_class = class(&original_db, name)?;
        executions(&original_db);
        let expected = original_flags(&original_db, original_class);
        let expected_reads = executions(&original_db);

        let shared_db = database()?;
        let shared_class = class(&shared_db, name)?;
        executions(&shared_db);
        assert_eq!(shared_class.instance_flags(&shared_db), expected, "{name}");
        assert_eq!(executions(&shared_db), expected_reads, "{name}");
    }
    Ok(())
}

#[test]
fn ordinary_bitset_retains_each_inheritance_property() -> anyhow::Result<()> {
    let db = database()?;
    for (name, expected) in [
        ("Owner", ClassInstanceFlags::empty()),
        ("Receiver", ClassInstanceFlags::empty()),
        ("Own", ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE),
        ("Inherited", ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE),
        (
            "FromAny",
            ClassInstanceFlags::INHERITS_FROM_EXPLICIT_ANY
                | ClassInstanceFlags::HAS_DYNAMIC_GETATTRIBUTE,
        ),
        ("FromUnknown", ClassInstanceFlags::HAS_DYNAMIC_GETATTRIBUTE),
        ("Record", ClassInstanceFlags::TYPED_DICT),
        ("FromDynamic", ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE),
        ("Invalid", ClassInstanceFlags::HAS_DYNAMIC_GETATTRIBUTE),
        ("Installed", ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE),
        ("OwnInstalled", ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE),
    ] {
        assert_eq!(class(&db, name)?.instance_flags(&db), expected, "{name}");
    }
    Ok(())
}

#[test]
fn explicit_metaclass_own_symbol_shortcut_preserves_ordinary_reads() -> anyhow::Result<()> {
    for name in ["Installed", "OwnInstalled"] {
        let original_db = database()?;
        let original_class = class(&original_db, name)?;
        executions(&original_db);
        let expected = original_own_attribute(&original_db, original_class);
        let expected_reads = executions(&original_db);

        let shared_db = database()?;
        let shared_class = class(&shared_db, name)?;
        executions(&shared_db);
        assert_eq!(
            own_custom_getattribute_with(
                &shared_db,
                shared_class,
                &InlineInstanceFlagsEffects::new(&shared_db)
            ),
            Ok(expected),
        );
        assert_eq!(executions(&shared_db), expected_reads, "{name}");
        assert!(expected, "{name}");
    }
    Ok(())
}

#[cfg(feature = "experimental-analysis")]
crate::types::class::runtime::class_memo_schema! {
    pub(in crate::types::class) type OriginalClassMemoSchema<'db> = crate::types::StaticClassLiteral<'static>;
    pub(in crate::types::class) fn register_original_class_memo;
    (original_flags_inner, salsa::execution_probe::FixedQueryKeyProfile)
}
