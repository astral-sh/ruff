//! Ordered generic-context selection and inspection of explicit legacy bases.

use std::convert::Infallible;

use crate::Db;
use crate::types::{GenericContext, KnownClass, KnownInstanceType, StaticClassLiteral, Type};

pub(in crate::types) mod inherited;
pub(in crate::types) mod pep695;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ClassContextWork {
    Select,
    Pep695,
    ExplicitBases,
    LegacyBase { index: usize },
    Inherited,
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

pub(in crate::types) struct ClassContextBaseCursor<'db> {
    bases: std::iter::Enumerate<std::iter::Copied<std::slice::Iter<'db, Type<'db>>>>,
}

impl<'db> ClassContextBaseCursor<'db> {
    fn new(bases: &'db [Type<'db>]) -> Self {
        Self {
            bases: bases.iter().copied().enumerate(),
        }
    }

    pub(in crate::types) fn next_base(&mut self) -> Option<(usize, Type<'db>)> {
        self.bases.next()
    }
}

const fn empty_class_bases<'db>() -> &'db [Type<'db>] {
    &[]
}

ty_mapping_probe_macros::shared_semantic_family! {
    /// Child reads distinguish operational refusal from an absent generic context.
    /// Their query bodies own declaration and inherited-context construction.
    #[synchronous(ClassContextEffects)]
    pub(in crate::types) trait AsyncClassContextEffects<'db>: sealed::Sealed {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, work: ClassContextWork) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn is_version_info(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn pep695_generic_context(&self, class: StaticClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn legacy_generic_context(&self, class: StaticClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_base(&self, cursor: &mut ClassContextBaseCursor<'db>) -> Result<Option<(usize, Type<'db>)>, Self::Error>;
        #[operation(child)]
        async fn inherited_legacy_generic_context(&self, class: StaticClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
    }

    #[synchronous(generic_context_with)]
    #[capabilities(effects = AsyncClassContextEffects)]
    #[passive_values(ClassContextWork::Select, ClassContextWork::Pep695, ClassContextWork::Inherited)]
    #[inline]
    pub(in crate::types) async fn generic_context_async_with<'db, E: AsyncClassContextEffects<'db>>(
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
        effects: &E,
    ) -> Result<Option<GenericContext<'db>>, E::Error> {
        let _ = db;
        effects.checkpoint(ClassContextWork::Select).await?;
        // Typeshed declarations inspect sys.version_info while their classes are being resolved.
        // Its stored identity avoids introducing a generic-context dependency cycle.
        if effects.is_version_info(class).await? {
            return Ok(None);
        }

        effects.checkpoint(ClassContextWork::Pep695).await?;
        if let Some(context) = effects.pep695_generic_context(class).await? {
            return Ok(Some(context));
        }
        if let Some(context) = effects.legacy_generic_context(class).await? {
            return Ok(Some(context));
        }

        // An explicit Generic or Protocol context takes precedence over inherited type variables.
        effects.checkpoint(ClassContextWork::Inherited).await?;
        effects.inherited_legacy_generic_context(class).await
    }

    #[synchronous(legacy_generic_context_with)]
    #[capabilities(effects = AsyncClassContextEffects)]
    #[passive_values(ClassContextBaseCursor::new, ClassContextWork::ExplicitBases, ClassContextWork::LegacyBase)]
    #[inline]
    pub(in crate::types) async fn legacy_generic_context_async_with<'db, E: AsyncClassContextEffects<'db>>(
        class: StaticClassLiteral<'db>,
        effects: &E,
    ) -> Result<Option<GenericContext<'db>>, E::Error> {
        effects.checkpoint(ClassContextWork::ExplicitBases).await?;
        let bases = effects.explicit_bases(class).await?;
        let mut cursor = ClassContextBaseCursor::new(bases);
        #[cursor_loop]
        while let Some(entry) = effects.next_base(&mut cursor).await? {
            let (index, base) = entry;
            effects.checkpoint(ClassContextWork::LegacyBase { index }).await?;
            if let Type::KnownInstance(
                KnownInstanceType::SubscriptedGeneric(context)
                | KnownInstanceType::SubscriptedProtocol(context),
            ) = base {
                return Ok(Some(context));
            }
        }
        Ok(None)
    }

    #[synchronous(SynchronousClassContextSourceEffects)]
    pub(in crate::types) trait ClassContextSourceEffects<'db> {
        type Error;

        #[operation(source)]
        async fn has_type_params(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn pep695_generic_context_inner(&self, class: StaticClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn explicit_bases_inner(&self, class: StaticClassLiteral<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(child)]
        async fn inherited_legacy_generic_context_inner(&self, class: StaticClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
    }

    #[synchronous(pep695_generic_context_sync)]
    #[capabilities(effects = ClassContextSourceEffects)]
    #[passive_values()]
    pub(in crate::types) async fn pep695_generic_context_with<'db, E: ClassContextSourceEffects<'db>>(
        class: StaticClassLiteral<'db>, effects: &E,
    ) -> Result<Option<GenericContext<'db>>, E::Error> {
        if !effects.has_type_params(class).await? {
            return Ok(None);
        }
        effects.pep695_generic_context_inner(class).await
    }

    #[synchronous(explicit_class_bases_sync)]
    #[capabilities(effects = ClassContextSourceEffects)]
    #[passive_values(empty_class_bases)]
    pub(in crate::types) async fn explicit_class_bases_with<'db, E: ClassContextSourceEffects<'db>>(
        class: StaticClassLiteral<'db>, effects: &E,
    ) -> Result<&'db [Type<'db>], E::Error> {
        if !effects.has_explicit_bases(class).await? {
            return Ok(empty_class_bases());
        }
        effects.explicit_bases_inner(class).await
    }

    #[synchronous(inherited_legacy_generic_context_sync)]
    #[capabilities(effects = ClassContextSourceEffects)]
    #[passive_values()]
    pub(in crate::types) async fn inherited_legacy_generic_context_with<'db, E: ClassContextSourceEffects<'db>>(
        class: StaticClassLiteral<'db>, effects: &E,
    ) -> Result<Option<GenericContext<'db>>, E::Error> {
        if !effects.has_explicit_bases(class).await? {
            return Ok(None);
        }
        effects.inherited_legacy_generic_context_inner(class).await
    }
}

pub(in crate::types) struct InlineClassContextEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> InlineClassContextEffects<'db> {
    pub(in crate::types) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl sealed::Sealed for InlineClassContextEffects<'_> {}

impl<'db> ClassContextEffects<'db> for InlineClassContextEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self, _work: ClassContextWork) -> Result<(), Infallible> {
        Ok(())
    }

    fn is_version_info(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.is_known(self.db, KnownClass::VersionInfo))
    }

    fn pep695_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(class.pep695_generic_context(self.db))
    }

    fn legacy_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        legacy_generic_context_with(class, self)
    }

    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Infallible> {
        Ok(class.explicit_bases(self.db))
    }

    fn next_base(
        &self,
        cursor: &mut ClassContextBaseCursor<'db>,
    ) -> Result<Option<(usize, Type<'db>)>, Infallible> {
        Ok(cursor.next_base())
    }

    fn inherited_legacy_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(class.inherited_legacy_generic_context(self.db))
    }
}
