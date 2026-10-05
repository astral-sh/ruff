//! Shared inference of inherited slots and instance-dictionary storage.

use std::convert::Infallible;

use ruff_python_ast::name::Name;

use super::{InstanceDictionary, InstanceLayout};
use crate::FxIndexSet;
use crate::types::class::member_source::InlineMemberSourceEffects;
use crate::types::mro::MroIterator;
use crate::types::{ClassBase, ClassLiteral, ClassType, KnownClass, StaticClassLiteral};

#[cfg(test)]
mod tests;

pub(in crate::types) struct InstanceLayoutFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousInstanceLayoutEffects)]
    pub(in crate::types) trait InstanceLayoutEffects<'db> {
        type Error;
        type MroCursor;
        type Slots;
        type NamesCursor;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn is_protocol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn unknown(&self) -> Result<InstanceLayout, Self::Error>;
        #[operation(local)]
        async fn new_slots(&self) -> Result<Self::Slots, Self::Error>;
        #[operation(local)]
        async fn start_mro(&self, class: StaticClassLiteral<'db>) -> Result<Self::MroCursor, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_mro_base(&self, cursor: &mut Self::MroCursor) -> Result<Option<ClassBase<'db>>, Self::Error>;
        #[operation(source)]
        async fn class_literal(&self, class: ClassType<'db>) -> Result<ClassLiteral<'db>, Self::Error>;
        #[operation(child)]
        async fn slot_names(&self, class: StaticClassLiteral<'db>) -> Result<Option<Self::NamesCursor>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_name(&self, cursor: &mut Self::NamesCursor) -> Result<Option<&'db Name>, Self::Error>;
        #[operation(local)]
        async fn is_dictionary_name(&self, name: &Name) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn insert_slot(&self, slots: &mut Self::Slots, name: &Name) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn has_explicit_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn known(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(local)]
        async fn finish(&self, slots: Self::Slots, dictionary: InstanceDictionary) -> Result<InstanceLayout, Self::Error>;
    }

    #[finite_capability]
    impl InstanceLayoutFacts {
        fn inherited_with(&self, dictionary: InstanceDictionary, base: InstanceDictionary) -> InstanceDictionary {
            dictionary.inherited_with(base)
        }

        fn known_dictionary(&self, class: Option<KnownClass>) -> Option<InstanceDictionary> {
            class.and_then(InstanceDictionary::for_known_class)
        }
    }

    #[synchronous(instance_layout_sync)]
    #[capabilities(effects = InstanceLayoutEffects, facts = InstanceLayoutFacts)]
    #[passive_values(InstanceDictionary::Absent, InstanceDictionary::Present, InstanceDictionary::Unknown)]
    pub(in crate::types) async fn instance_layout_with<'db, E: InstanceLayoutEffects<'db>>(
        class: StaticClassLiteral<'db>,
        facts: InstanceLayoutFacts,
        effects: &E,
    ) -> Result<InstanceLayout, E::Error> {
        effects.checkpoint().await?;
        if effects.is_protocol(class).await? {
            return effects.unknown().await;
        }

        let mut slots = effects.new_slots().await?;
        #[passive_state]
        let mut dictionary = InstanceDictionary::Absent;
        let mut cursor = effects.start_mro(class).await?;

        #[cursor_loop]
        while let Some(base) = effects.next_mro_base(&mut cursor).await? {
            let base = match base {
                ClassBase::Class(base) => base,
                ClassBase::Any | ClassBase::Divergent(_) | ClassBase::Dynamic(_) => {
                    dictionary = facts.inherited_with(dictionary, InstanceDictionary::Unknown);
                    continue;
                }
                ClassBase::TypedDict(_) | ClassBase::Generic | ClassBase::Protocol => continue,
            };

            let base = match effects.class_literal(base).await? {
                ClassLiteral::Static(base) => base,
                // Functional named tuples synthesize empty slots, while TypedDict instances use
                // dictionary item storage rather than an instance-attribute dictionary.
                ClassLiteral::DynamicNamedTuple(_) | ClassLiteral::DynamicTypedDict(_) => continue,
                // Enum instances retain an instance dictionary even when the enum is created
                // through the functional API.
                ClassLiteral::DynamicEnum(_) => {
                    dictionary = InstanceDictionary::Present;
                    continue;
                }
                ClassLiteral::Dynamic(_) => {
                    dictionary = facts.inherited_with(dictionary, InstanceDictionary::Unknown);
                    continue;
                }
            };

            if let Some(mut names) = effects.slot_names(base).await? {
                #[cursor_loop]
                while let Some(name) = effects.next_name(&mut names).await? {
                    if effects.is_dictionary_name(name).await? {
                        dictionary = InstanceDictionary::Present;
                    }
                    effects.insert_slot(&mut slots, name).await?;
                }
            } else if effects.has_explicit_slots(base).await? {
                dictionary = facts.inherited_with(dictionary, InstanceDictionary::Unknown);
            } else if let Some(known_dictionary) = facts.known_dictionary(effects.known(base).await?) {
                dictionary = facts.inherited_with(dictionary, known_dictionary);
            } else if !effects.is_protocol(base).await? {
                dictionary = InstanceDictionary::Present;
            }
        }

        effects.finish(slots, dictionary).await
    }
}

pub(in crate::types) fn finish_slots(
    slots: FxIndexSet<Name>,
    dictionary: InstanceDictionary,
) -> InstanceLayout {
    InstanceLayout {
        slots: slots.into_iter().collect(),
        dictionary,
    }
}

impl<'db> SynchronousInstanceLayoutEffects<'db> for InlineMemberSourceEffects<'db> {
    type Error = Infallible;
    type MroCursor = MroIterator<'db>;
    type Slots = FxIndexSet<Name>;
    type NamesCursor = std::slice::Iter<'db, Name>;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn is_protocol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.is_protocol(self.db))
    }

    fn unknown(&self) -> Result<InstanceLayout, Infallible> {
        Ok(InstanceLayout::unknown())
    }

    fn new_slots(&self) -> Result<Self::Slots, Infallible> {
        Ok(FxIndexSet::default())
    }

    fn start_mro(&self, class: StaticClassLiteral<'db>) -> Result<Self::MroCursor, Infallible> {
        Ok(class.iter_mro(self.db, None))
    }

    fn next_mro_base(
        &self,
        cursor: &mut Self::MroCursor,
    ) -> Result<Option<ClassBase<'db>>, Infallible> {
        Ok(cursor.next())
    }

    fn class_literal(&self, class: ClassType<'db>) -> Result<ClassLiteral<'db>, Infallible> {
        Ok(class.class_literal(self.db))
    }

    fn slot_names(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<Self::NamesCursor>, Infallible> {
        Ok(class.slot_names(self.db).map(<[Name]>::iter))
    }

    fn next_name(&self, cursor: &mut Self::NamesCursor) -> Result<Option<&'db Name>, Infallible> {
        Ok(cursor.next())
    }

    fn is_dictionary_name(&self, name: &Name) -> Result<bool, Infallible> {
        Ok(name == "__dict__")
    }

    fn insert_slot(&self, slots: &mut Self::Slots, name: &Name) -> Result<(), Infallible> {
        slots.insert(name.clone());
        Ok(())
    }

    fn has_explicit_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.has_explicit_slots(self.db))
    }

    fn known(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Infallible> {
        Ok(class.known(self.db))
    }

    fn finish(
        &self,
        slots: Self::Slots,
        dictionary: InstanceDictionary,
    ) -> Result<InstanceLayout, Infallible> {
        Ok(finish_slots(slots, dictionary))
    }
}
