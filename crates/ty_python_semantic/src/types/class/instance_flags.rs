//! Ordered classification of the properties shared by a class's instances.

use std::convert::Infallible;

use ruff_python_ast::name::Name;
use ty_python_core::place_table;

use super::ClassInstanceFlags;
use super::implicit_attributes::implicit_attribute_names;
use super::source::{SourceClassEffects, SourceClassError};
use crate::Db;
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_sync};
use crate::types::mro::root::InlineMroRootEffects;
use crate::types::mro::source::DeclarationMroCursor;
use crate::types::source_read::{SourceReadControl, read_source};
use crate::types::{
    ClassBase, ClassLiteral, ClassType, KnownClass, Specialization, StaticClassLiteral,
};

#[cfg(test)]
use crate::types::constructor::expansion_probe::{self, Incomplete};
#[cfg(test)]
use crate::types::instance::attempt::UnsupportedInstanceOperation;
#[cfg(test)]
use crate::types::mro::attempt::AttemptMroEffects;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "experimental-analysis"))]
pub(super) use tests::{OriginalClassMemoSchema, register_original_class_memo};

#[cfg(test)]
mod attempt_tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum InstanceFlagsWork {
    Select,
    OwnAttribute,
    BodySymbol,
    Metaclass,
    Advance,
    Classify,
    Update,
    Publish,
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

pub(in crate::types) trait InstanceFlagsEffects<'db>: sealed::Sealed {
    type Error;
    type Cursor;

    fn checkpoint(&self, work: InstanceFlagsWork) -> Result<(), Self::Error>;
    fn inherited_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassInstanceFlags, Self::Error>;
    fn start_mro(&self, class: StaticClassLiteral<'db>) -> Result<Self::Cursor, Self::Error>;
    fn next_base(&self, cursor: &mut Self::Cursor) -> Result<Option<ClassBase<'db>>, Self::Error>;
    fn has_own_symbol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    fn metaclass_custom_getattribute(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error>;
}

impl<'db> StaticClassLiteral<'db> {
    pub(in crate::types) fn instance_flags_with<E: InstanceFlagsEffects<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<ClassInstanceFlags, E::Error> {
        queued_instance_flags_sync(self, InstanceFlagFacts, &InlineQueuedFlags { db, effects })
    }
}

pub(in crate::types) fn inherited_instance_flags_with<'db, E: InstanceFlagsEffects<'db>>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    effects: &E,
) -> Result<ClassInstanceFlags, E::Error> {
    queued_inherited_flags_sync(class, InstanceFlagFacts, &InlineQueuedFlags { db, effects })
}

/// Return whether this class defines its own non-default `__getattribute__`.
///
/// An explicit metaclass can install the method even when the class body does not define it:
///
/// ```python
/// def interceptor(self, name): ...
///
/// class Meta(type):
///     def __init__(cls, *args):
///         cls.__getattribute__ = interceptor
///
/// class Example(metaclass=Meta): ...
/// ```
pub(in crate::types) fn own_custom_getattribute_with<'db, E: InstanceFlagsEffects<'db>>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    queued_own_getattribute_sync(class, &InlineQueuedFlags { db, effects })
}

pub(in crate::types) struct InlineInstanceFlagsEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> InlineInstanceFlagsEffects<'db> {
    pub(in crate::types) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl sealed::Sealed for InlineInstanceFlagsEffects<'_> {}

impl<'db> InstanceFlagsEffects<'db> for InlineInstanceFlagsEffects<'db> {
    type Error = Infallible;
    type Cursor = MroCursor<'db>;

    fn start_mro(&self, class: StaticClassLiteral<'db>) -> Result<Self::Cursor, Infallible> {
        Ok(MroCursor::new(class.into(), None))
    }

    fn checkpoint(&self, _: InstanceFlagsWork) -> Result<(), Infallible> {
        Ok(())
    }

    fn inherited_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassInstanceFlags, Infallible> {
        Ok(class.inherited_instance_flags(self.db))
    }

    fn next_base(&self, cursor: &mut MroCursor<'db>) -> Result<Option<ClassBase<'db>>, Infallible> {
        mro_next_sync(
            self.db,
            cursor,
            MroDirection::Forward,
            &InlineMroRootEffects::new(self.db),
        )
    }

    fn has_own_symbol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(place_table(self.db, class.body_scope(self.db))
            .symbol_id("__getattribute__")
            .is_some())
    }

    fn metaclass_custom_getattribute(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Infallible> {
        let Some(metaclass) = class.metaclass(self.db).to_class_type(self.db) else {
            return Ok(true);
        };
        Ok(metaclass
            .iter_mro(self.db)
            .any(|base| installs_custom_getattribute(self.db, base)))
    }
}

fn installs_custom_getattribute<'db>(db: &'db dyn Db, base: ClassBase<'db>) -> bool {
    match base {
        ClassBase::Any | ClassBase::Dynamic(_) | ClassBase::Divergent(_) => true,
        ClassBase::Class(base) => base.static_class_literal(db).is_none_or(|(base, _)| {
            implicit_attribute_names(db, base.body_scope(db))
                .binary_search(&Name::new_static("__getattribute__"))
                .is_ok()
        }),
        ClassBase::Generic | ClassBase::Protocol | ClassBase::TypedDict(_) => false,
    }
}

impl sealed::Sealed for SourceClassEffects<'_> {}

impl<'db> InstanceFlagsEffects<'db> for SourceClassEffects<'db> {
    type Error = SourceClassError;
    type Cursor = DeclarationMroCursor<'db>;

    fn checkpoint(&self, _: InstanceFlagsWork) -> Result<(), SourceClassError> {
        self.check()
    }

    fn inherited_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassInstanceFlags, SourceClassError> {
        read_source(self, || class.inherited_instance_flags(self.db))
    }

    fn start_mro(&self, class: StaticClassLiteral<'db>) -> Result<Self::Cursor, SourceClassError> {
        self.check()?;
        Ok(DeclarationMroCursor::new(class))
    }

    fn next_base(
        &self,
        cursor: &mut Self::Cursor,
    ) -> Result<Option<ClassBase<'db>>, SourceClassError> {
        cursor.next(self.db)
    }

    fn has_own_symbol(&self, class: StaticClassLiteral<'db>) -> Result<bool, SourceClassError> {
        read_source(self, || {
            place_table(self.db, class.body_scope(self.db))
                .symbol_id("__getattribute__")
                .is_some()
        })
    }

    fn metaclass_custom_getattribute(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, SourceClassError> {
        let Some(mut cursor) = DeclarationMroCursor::for_metaclass_of(self.db, class)? else {
            return Ok(true);
        };
        while let Some(base) = cursor.next(self.db)? {
            if read_source(self, || installs_custom_getattribute(self.db, base))? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
pub(in crate::types) struct AttemptInstanceFlagsEffects<'db> {
    db: &'db dyn Db,
}

#[cfg(test)]
impl<'db> AttemptInstanceFlagsEffects<'db> {
    pub(in crate::types) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

#[cfg(test)]
impl SourceReadControl for AttemptInstanceFlagsEffects<'_> {
    type Error = Incomplete;

    fn check(&self) -> Result<(), Self::Error> {
        expansion_probe::continue_work(self.db)?;
        if !expansion_probe::mro_effects_enabled() {
            return Err(expansion_probe::refuse(
                self.db,
                Incomplete::UnsupportedInstanceOperation(
                    UnsupportedInstanceOperation::UncontrolledMro,
                ),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
impl sealed::Sealed for AttemptInstanceFlagsEffects<'_> {}

#[cfg(test)]
impl<'db> InstanceFlagsEffects<'db> for AttemptInstanceFlagsEffects<'db> {
    type Error = Incomplete;
    type Cursor = MroCursor<'db>;

    fn start_mro(&self, class: StaticClassLiteral<'db>) -> Result<Self::Cursor, Incomplete> {
        self.check()?;
        Ok(MroCursor::new(class.into(), None))
    }

    fn checkpoint(&self, work: InstanceFlagsWork) -> Result<(), Self::Error> {
        let _charge = expansion_probe::charge_ledger::scope(&work);
        self.check()?;
        expansion_probe::charge_work(self.db, 1)
    }

    fn inherited_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassInstanceFlags, Self::Error> {
        read_source(self, || class.inherited_instance_flags(self.db))
    }

    fn next_base(
        &self,
        cursor: &mut MroCursor<'db>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error> {
        self.check()?;
        mro_next_sync(
            self.db,
            cursor,
            MroDirection::Forward,
            &AttemptMroEffects::new(self.db),
        )
    }

    fn has_own_symbol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        let table = read_source(self, || place_table(self.db, class.body_scope(self.db)))?;
        Ok(table.symbol_id("__getattribute__").is_some())
    }

    fn metaclass_custom_getattribute(
        &self,
        _: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        self.check()?;
        Err(expansion_probe::refuse(
            self.db,
            Incomplete::UnsupportedInstanceOperation(
                UnsupportedInstanceOperation::MetaclassAttributeClassification,
            ),
        ))
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) struct InstanceFlagFacts;

ty_mapping_probe_macros::shared_semantic_family! {
#[synchronous(SynchronousQueuedInstanceFlags)]
pub(in crate::types) trait QueuedInstanceFlags<'db> {
    type Error;
    type Cursor;
    #[operation(checkpoint)]
    async fn checkpoint(&self, work: InstanceFlagsWork) -> Result<(), Self::Error>;
    #[operation(local)]
    async fn known_class(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
    #[operation(local)]
    async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    #[operation(local)]
    async fn has_explicit_metaclass(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    #[operation(source)]
    async fn inherited_flags(&self, class: StaticClassLiteral<'db>) -> Result<ClassInstanceFlags, Self::Error>;
    #[operation(child)]
    async fn own_attribute(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    #[operation(source)]
    async fn has_own_symbol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    #[operation(child)]
    async fn metaclass_custom_getattribute(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    #[operation(local)]
    async fn start_mro(&self, class: StaticClassLiteral<'db>) -> Result<Self::Cursor, Self::Error>;
    #[operation(child)]
    #[progress]
    async fn next_base(&self, cursor: &mut Self::Cursor) -> Result<Option<ClassBase<'db>>, Self::Error>;
    #[operation(source)]
    async fn static_class_literal(&self, class: ClassType<'db>) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error>;
}

#[synchronous(SynchronousExplicitAnyInheritanceEffects)]
pub(in crate::types) trait ExplicitAnyInheritanceEffects<'db> {
    type Error;
    #[operation(child)]
    async fn without_inference(&self, class: ClassLiteral<'db>) -> Result<Option<bool>, Self::Error>;
    #[operation(child)]
    async fn instance_flags(&self, class: ClassLiteral<'db>) -> Result<ClassInstanceFlags, Self::Error>;
}

#[finite_capability]
impl InstanceFlagFacts {



    fn empty(&self) -> ClassInstanceFlags { ClassInstanceFlags::empty() }
    fn known_flags(&self, known: KnownClass) -> ClassInstanceFlags {
        if known.is_typed_dict_subclass() { ClassInstanceFlags::TYPED_DICT } else { ClassInstanceFlags::empty() }
    }
    fn with_custom(&self, mut flags: ClassInstanceFlags, custom: bool) -> ClassInstanceFlags {
        flags.set(ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE, custom); flags
    }
    fn explicit_any(&self) -> ClassInstanceFlags {
        ClassInstanceFlags::INHERITS_FROM_EXPLICIT_ANY | ClassInstanceFlags::HAS_DYNAMIC_GETATTRIBUTE
    }
    fn dynamic(&self) -> ClassInstanceFlags { ClassInstanceFlags::HAS_DYNAMIC_GETATTRIBUTE }
    fn typed_dict(&self) -> ClassInstanceFlags { ClassInstanceFlags::TYPED_DICT }
    fn union(&self, mut flags: ClassInstanceFlags, inherited: ClassInstanceFlags) -> ClassInstanceFlags {
        flags.insert(inherited); flags
    }
    fn inherits_explicit_any(&self, flags: ClassInstanceFlags) -> bool {
        flags.contains(ClassInstanceFlags::INHERITS_FROM_EXPLICIT_ANY)
    }
}

#[synchronous(queued_inherited_flags_sync)]
#[capabilities(effects = QueuedInstanceFlags, facts = InstanceFlagFacts)]
#[passive_values(InstanceFlagsWork)]
pub(in crate::types) async fn queued_inherited_flags_with<'db, E: QueuedInstanceFlags<'db>>(
    class: StaticClassLiteral<'db>, facts: InstanceFlagFacts, effects: &E,
) -> Result<ClassInstanceFlags, E::Error> {
    effects.checkpoint(InstanceFlagsWork::Select).await?;
    #[passive_state]
    let mut flags = facts.empty();
    let mut cursor = effects.start_mro(class).await?;
    effects.checkpoint(InstanceFlagsWork::Advance).await?;
    #[cursor_loop]
    while let Some(base) = effects.next_base(&mut cursor).await? {
        effects.checkpoint(InstanceFlagsWork::Classify).await?;
        let inherited = match base {
            ClassBase::Any => facts.explicit_any(),
            ClassBase::Dynamic(_) | ClassBase::Divergent(_) => facts.dynamic(),
            ClassBase::TypedDict(_) => facts.typed_dict(),
            ClassBase::Class(base) => {
                let custom = match effects.static_class_literal(base).await? {
                    None => true,
                    Some((base, _)) => effects.own_attribute(base).await?,
                };
                facts.with_custom(facts.empty(), custom)
            }
            ClassBase::Generic | ClassBase::Protocol => facts.empty(),
        };
        effects.checkpoint(InstanceFlagsWork::Update).await?;
        flags = facts.union(flags, inherited);
        effects.checkpoint(InstanceFlagsWork::Advance).await?;
    }
    effects.checkpoint(InstanceFlagsWork::Publish).await?;
    Ok(flags)
}

#[synchronous(inherits_from_explicit_any_sync)]
#[capabilities(effects = ExplicitAnyInheritanceEffects, facts = InstanceFlagFacts)]
#[passive_values()]
pub(in crate::types) async fn inherits_from_explicit_any_with<'db, E: ExplicitAnyInheritanceEffects<'db>>(
    class: ClassLiteral<'db>, facts: InstanceFlagFacts, effects: &E,
) -> Result<bool, E::Error> {
    if let Some(inherits) = effects.without_inference(class).await? {
        return Ok(inherits);
    }
    let flags = effects.instance_flags(class).await?;
    Ok(facts.inherits_explicit_any(flags))
}

#[synchronous(queued_instance_flags_sync)]
#[capabilities(effects = QueuedInstanceFlags, facts = InstanceFlagFacts)]
#[passive_values(InstanceFlagsWork)]
pub(in crate::types) async fn queued_instance_flags_with<'db, E: QueuedInstanceFlags<'db>>(
    class: StaticClassLiteral<'db>, facts: InstanceFlagFacts, effects: &E,
) -> Result<ClassInstanceFlags, E::Error> {
    effects.checkpoint(InstanceFlagsWork::Select).await?;
    let flags = if let Some(known) = effects.known_class(class).await? {
        facts.known_flags(known)
    } else if effects.has_explicit_bases(class).await? {
        let flags = effects.inherited_flags(class).await?;
        effects.checkpoint(InstanceFlagsWork::Publish).await?;
        return Ok(flags);
    } else { facts.empty() };
    let own_attribute = effects.own_attribute(class).await?;
    effects.checkpoint(InstanceFlagsWork::Update).await?;
    let flags = facts.with_custom(flags, own_attribute);
    effects.checkpoint(InstanceFlagsWork::Publish).await?;
    Ok(flags)
}

#[synchronous(queued_own_getattribute_sync)]
#[capabilities(effects = QueuedInstanceFlags)]
#[passive_values(InstanceFlagsWork, KnownClass::Object, KnownClass::Type)]
pub(in crate::types) async fn queued_own_getattribute_with<'db, E: QueuedInstanceFlags<'db>>(
    class: StaticClassLiteral<'db>, effects: &E,
) -> Result<bool, E::Error> {
    effects.checkpoint(InstanceFlagsWork::OwnAttribute).await?;
    if let Some(KnownClass::Object | KnownClass::Type) = effects.known_class(class).await? { return Ok(false); }
    effects.checkpoint(InstanceFlagsWork::BodySymbol).await?;
    if effects.has_own_symbol(class).await? { return Ok(true); }
    if !effects.has_explicit_metaclass(class).await? { return Ok(false); }
    effects.checkpoint(InstanceFlagsWork::Metaclass).await?;
    effects.metaclass_custom_getattribute(class).await
}
}

impl InstanceFlagFacts {
    pub(in crate::types) fn known<'db>(
        &self,
        fields: crate::types::mro::field_reads::MroFieldReads<'db>,
        class: StaticClassLiteral<'db>,
    ) -> Option<KnownClass> {
        fields.static_known(class)
    }
    pub(in crate::types) fn explicit_bases<'db>(
        &self,
        fields: crate::types::mro::field_reads::MroFieldReads<'db>,
        class: StaticClassLiteral<'db>,
    ) -> bool {
        fields.has_explicit_bases(class)
    }
    pub(in crate::types) fn explicit_metaclass<'db>(
        &self,
        fields: crate::types::mro::field_reads::MroFieldReads<'db>,
        class: StaticClassLiteral<'db>,
    ) -> bool {
        fields.has_explicit_metaclass(class)
    }
}

struct InlineQueuedFlags<'a, 'db, E> {
    db: &'db dyn Db,
    effects: &'a E,
}

impl<'db, E: InstanceFlagsEffects<'db>> SynchronousQueuedInstanceFlags<'db>
    for InlineQueuedFlags<'_, 'db, E>
{
    type Error = E::Error;
    type Cursor = E::Cursor;
    fn checkpoint(&self, work: InstanceFlagsWork) -> Result<(), Self::Error> {
        self.effects.checkpoint(work)
    }
    fn known_class(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(InstanceFlagFacts.known(
            crate::types::mro::field_reads::MroFieldReads::new(self.db),
            class,
        ))
    }
    fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(InstanceFlagFacts.explicit_bases(
            crate::types::mro::field_reads::MroFieldReads::new(self.db),
            class,
        ))
    }
    fn has_explicit_metaclass(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(InstanceFlagFacts.explicit_metaclass(
            crate::types::mro::field_reads::MroFieldReads::new(self.db),
            class,
        ))
    }
    fn inherited_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassInstanceFlags, Self::Error> {
        self.effects.inherited_flags(class)
    }
    fn own_attribute(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        own_custom_getattribute_with(self.db, class, self.effects)
    }
    fn has_own_symbol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.effects.has_own_symbol(class)
    }
    fn metaclass_custom_getattribute(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        self.effects.metaclass_custom_getattribute(class)
    }

    fn start_mro(&self, class: StaticClassLiteral<'db>) -> Result<Self::Cursor, Self::Error> {
        self.effects.start_mro(class)
    }

    fn next_base(&self, cursor: &mut Self::Cursor) -> Result<Option<ClassBase<'db>>, Self::Error> {
        self.effects.next_base(cursor)
    }

    fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error> {
        Ok(class.static_class_literal(self.db))
    }
}

pub(super) struct InlineExplicitAnyInheritanceEffects<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> SynchronousExplicitAnyInheritanceEffects<'db>
    for InlineExplicitAnyInheritanceEffects<'db>
{
    type Error = Infallible;

    fn without_inference(&self, class: ClassLiteral<'db>) -> Result<Option<bool>, Infallible> {
        Ok(class.inherits_from_explicit_any_without_inference(self.db))
    }

    fn instance_flags(&self, class: ClassLiteral<'db>) -> Result<ClassInstanceFlags, Infallible> {
        Ok(class.instance_flags(self.db))
    }
}
