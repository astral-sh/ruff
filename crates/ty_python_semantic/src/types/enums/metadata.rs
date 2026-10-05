//! Enum metadata selection before constructing static or dynamic member metadata.

use std::convert::Infallible;

use crate::types::class::DynamicEnumLiteral;
use crate::types::{ClassLiteral, KnownClass, StaticClassLiteral};
use crate::{Db, ProgramEnvironment};

use super::{
    EnumMetadata, dynamic_enum_metadata, is_enum_class_by_inheritance, static_enum_member_metadata,
};

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousEnumMetadataEffects)]
    pub(in crate::types) trait EnumMetadataEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn known_class(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(local)]
        async fn is_enum_subclass_with_members(&self, class: KnownClass) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn dynamic_enum_metadata(&self, class: DynamicEnumLiteral<'db>) -> Result<Option<EnumMetadata<'db>>, Self::Error>;
        #[operation(source)]
        async fn static_program_environment(&self, class: StaticClassLiteral<'db>) -> Result<ProgramEnvironment<'db>, Self::Error>;
        #[operation(child)]
        async fn is_enum_class_by_inheritance(&self, class: StaticClassLiteral<'db>, env: &ProgramEnvironment<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn static_enum_member_metadata(&self, class: StaticClassLiteral<'db>, env: ProgramEnvironment<'db>) -> Result<Option<EnumMetadata<'db>>, Self::Error>;
    }

    #[synchronous(enum_metadata_sync)]
    #[capabilities(effects = EnumMetadataEffects)]
    #[passive_values()]
    pub(in crate::types) async fn enum_metadata_with<'db, E: EnumMetadataEffects<'db>>(
        class: ClassLiteral<'db>,
        effects: &E,
    ) -> Result<Option<EnumMetadata<'db>>, E::Error> {
        effects.checkpoint().await?;
        let class = match class {
            ClassLiteral::Static(class) => class,
            ClassLiteral::Dynamic(..) => {
                // Classes created via `type` cannot be enums; the following fails at runtime:
                // ```python
                // import enum
                //
                // class BaseEnum(enum.Enum):
                //     pass
                //
                // MyEnum = type("MyEnum", (BaseEnum,), {"A": 1, "B": 2})
                // ```
                return Ok(None);
            }
            ClassLiteral::DynamicNamedTuple(..) | ClassLiteral::DynamicTypedDict(..) => return Ok(None),
            ClassLiteral::DynamicEnum(class) => return effects.dynamic_enum_metadata(class).await,
        };

        // This is a fast path to avoid traversing the MRO of known classes.
        if let Some(known_class) = effects.known_class(class).await?
            && !effects.is_enum_subclass_with_members(known_class).await?
        {
            return Ok(None);
        }

        let env = effects.static_program_environment(class).await?;
        if !effects.is_enum_class_by_inheritance(class, &env).await? {
            return Ok(None);
        }

        effects.static_enum_member_metadata(class, env).await
    }
}

pub(super) struct InlineEnumMetadataEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> InlineEnumMetadataEffects<'db> {
    pub(super) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl<'db> SynchronousEnumMetadataEffects<'db> for InlineEnumMetadataEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn known_class(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Infallible> {
        Ok(class.known(self.db))
    }

    fn is_enum_subclass_with_members(&self, class: KnownClass) -> Result<bool, Infallible> {
        Ok(class.is_enum_subclass_with_members())
    }

    fn dynamic_enum_metadata(
        &self,
        class: DynamicEnumLiteral<'db>,
    ) -> Result<Option<EnumMetadata<'db>>, Infallible> {
        Ok(dynamic_enum_metadata(self.db, class))
    }

    fn static_program_environment(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ProgramEnvironment<'db>, Infallible> {
        Ok(ProgramEnvironment::from_file(class.program_file(self.db)))
    }

    fn is_enum_class_by_inheritance(
        &self,
        class: StaticClassLiteral<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<bool, Infallible> {
        Ok(is_enum_class_by_inheritance(self.db, env, class))
    }

    fn static_enum_member_metadata(
        &self,
        class: StaticClassLiteral<'db>,
        env: ProgramEnvironment<'db>,
    ) -> Result<Option<EnumMetadata<'db>>, Infallible> {
        Ok(static_enum_member_metadata(self.db, class, env))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use ruff_db::files::system_path_to_file;
    use salsa::Database as _;
    use ty_python_core::ProgramFile;

    use super::{
        ClassLiteral, Db, DynamicEnumLiteral, EnumMetadata, Infallible, InlineEnumMetadataEffects,
        KnownClass, ProgramEnvironment, StaticClassLiteral, SynchronousEnumMetadataEffects,
        enum_metadata_sync,
    };
    use crate::db::tests::{TestDb, TestDbBuilder};
    use crate::place::global_symbol;
    use crate::types::Type;
    use crate::types::enums::enum_metadata;

    fn database() -> anyhow::Result<TestDb> {
        TestDbBuilder::new()
            .with_file(
                "/src/metadata.py",
                r#"
from builtins import object as Object
from enum import Enum
from typing import NamedTuple, TypedDict

class Plain: ...

class Static(Enum):
    FIRST = 1
    ALIAS = 1
    SECOND = 2

Functional = Enum("Functional", {"FIRST": 1, "ALIAS": 1, "SECOND": 2})
Dynamic = type("Dynamic", (), {})
Tuple = NamedTuple("Tuple", [("value", int)])
Dictionary = TypedDict("Dictionary", {"value": int})
"#,
            )
            .build()
    }

    fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<ClassLiteral<'db>> {
        let env = db.program_environment();
        let file = ProgramFile::new(
            db,
            system_path_to_file(db, "/src/metadata.py")?,
            env.program(db),
        );
        global_symbol(db, file, name)
            .place
            .expect_type()
            .as_class_literal()
            .ok_or_else(|| anyhow::anyhow!("missing class {name}"))
    }

    fn infallible<T>(result: Result<T, Infallible>) -> T {
        match result {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Event {
        Checkpoint,
        KnownClass,
        ClassifyKnownClass,
        StaticEnvironment,
        ClassifyStaticClass,
        Static,
        Dynamic,
    }

    struct Recording<'db> {
        inline: InlineEnumMetadataEffects<'db>,
        events: RefCell<Vec<Event>>,
        refuse: Option<Event>,
    }

    impl<'db> Recording<'db> {
        fn new(db: &'db dyn Db, refuse: Option<Event>) -> Self {
            Self {
                inline: InlineEnumMetadataEffects::new(db),
                events: RefCell::default(),
                refuse,
            }
        }

        fn record(&self, event: Event) -> Result<(), Event> {
            self.events.borrow_mut().push(event);
            if self.refuse == Some(event) {
                Err(event)
            } else {
                Ok(())
            }
        }
    }

    impl<'db> SynchronousEnumMetadataEffects<'db> for Recording<'db> {
        type Error = Event;

        fn checkpoint(&self) -> Result<(), Event> {
            self.record(Event::Checkpoint)
        }

        fn known_class(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Event> {
            self.record(Event::KnownClass)?;
            Ok(infallible(self.inline.known_class(class)))
        }

        fn is_enum_subclass_with_members(&self, class: KnownClass) -> Result<bool, Event> {
            self.record(Event::ClassifyKnownClass)?;
            Ok(infallible(self.inline.is_enum_subclass_with_members(class)))
        }

        fn dynamic_enum_metadata(
            &self,
            class: DynamicEnumLiteral<'db>,
        ) -> Result<Option<EnumMetadata<'db>>, Event> {
            self.record(Event::Dynamic)?;
            Ok(infallible(self.inline.dynamic_enum_metadata(class)))
        }

        fn static_program_environment(
            &self,
            class: StaticClassLiteral<'db>,
        ) -> Result<ProgramEnvironment<'db>, Event> {
            self.record(Event::StaticEnvironment)?;
            Ok(infallible(self.inline.static_program_environment(class)))
        }

        fn is_enum_class_by_inheritance(
            &self,
            class: StaticClassLiteral<'db>,
            env: &ProgramEnvironment<'db>,
        ) -> Result<bool, Event> {
            self.record(Event::ClassifyStaticClass)?;
            Ok(infallible(
                self.inline.is_enum_class_by_inheritance(class, env),
            ))
        }

        fn static_enum_member_metadata(
            &self,
            class: StaticClassLiteral<'db>,
            env: ProgramEnvironment<'db>,
        ) -> Result<Option<EnumMetadata<'db>>, Event> {
            self.record(Event::Static)?;
            Ok(infallible(
                self.inline.static_enum_member_metadata(class, env),
            ))
        }
    }

    #[test]
    fn selection_preserves_static_and_dynamic_member_metadata() -> anyhow::Result<()> {
        let db = database()?;
        for name in [
            "Enum",
            "Object",
            "Plain",
            "Static",
            "Functional",
            "Dynamic",
            "Tuple",
            "Dictionary",
        ] {
            let class = class(&db, name)?;
            assert!(match name {
                "Functional" => matches!(class, ClassLiteral::DynamicEnum(_)),
                "Dynamic" => matches!(class, ClassLiteral::Dynamic(_)),
                "Tuple" => matches!(class, ClassLiteral::DynamicNamedTuple(_)),
                "Dictionary" => matches!(class, ClassLiteral::DynamicTypedDict(_)),
                _ => matches!(class, ClassLiteral::Static(_)),
            });
            let effects = Recording::new(&db, None);
            let metadata = enum_metadata_sync(class, &effects)
                .map_err(|event| anyhow::anyhow!("unexpected refusal at {event:?}"))?;
            assert_eq!(metadata.as_ref(), enum_metadata(&db, class), "{name}");
            if matches!(name, "Static" | "Functional") {
                let metadata =
                    metadata.ok_or_else(|| anyhow::anyhow!("missing metadata for {name}"))?;
                assert_eq!(metadata.members.len(), 2, "{name}");
                assert_eq!(
                    metadata.members.get("FIRST"),
                    Some(&Type::int_literal(1)),
                    "{name}"
                );
                assert_eq!(
                    metadata.members.get("SECOND"),
                    Some(&Type::int_literal(2)),
                    "{name}"
                );
                assert_eq!(
                    metadata.aliases.get("ALIAS").map(|name| name.as_str()),
                    Some("FIRST"),
                    "{name}"
                );
            } else {
                assert!(metadata.is_none(), "{name}");
            }
            let expected = match name {
                "Enum" | "Object" => vec![
                    Event::Checkpoint,
                    Event::KnownClass,
                    Event::ClassifyKnownClass,
                ],
                "Plain" => vec![
                    Event::Checkpoint,
                    Event::KnownClass,
                    Event::StaticEnvironment,
                    Event::ClassifyStaticClass,
                ],
                "Static" => vec![
                    Event::Checkpoint,
                    Event::KnownClass,
                    Event::StaticEnvironment,
                    Event::ClassifyStaticClass,
                    Event::Static,
                ],
                "Functional" => vec![Event::Checkpoint, Event::Dynamic],
                _ => vec![Event::Checkpoint],
            };
            assert_eq!(*effects.events.borrow(), expected, "{name}");
        }
        Ok(())
    }

    #[test]
    fn refusal_stops_selection_without_returning_absence() -> anyhow::Result<()> {
        let db = database()?;
        for name in ["Enum", "Plain", "Static", "Functional", "Dynamic"] {
            let class = class(&db, name)?;
            let completed = Recording::new(&db, None);
            enum_metadata_sync(class, &completed)
                .map_err(|event| anyhow::anyhow!("unexpected refusal at {event:?}"))?;
            let events = completed.events.into_inner();
            for (index, event) in events.iter().copied().enumerate() {
                let effects = Recording::new(&db, Some(event));
                assert_eq!(enum_metadata_sync(class, &effects), Err(event), "{name}");
                assert_eq!(*effects.events.borrow(), events[..=index], "{name}");
            }
        }
        Ok(())
    }
}
