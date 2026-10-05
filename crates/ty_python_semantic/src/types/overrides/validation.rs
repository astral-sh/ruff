//! Override checks preserve inherited dependencies even when a class has no own members.

use std::collections::hash_set::IntoIter;
use std::convert::Infallible;

use rustc_hash::{FxHashMap, FxHashSet};
use ty_python_core::scope::ScopeId;

use super::{OverrideRulesConfig, check_class_declaration, check_inherited_method_conflicts};
use crate::types::context::InferContext;
use crate::types::enums::{EnumMetadata, enum_metadata};
use crate::types::list_members::{MemberWithDefinition, all_end_of_scope_members};
use crate::types::{ClassBase, ClassType, GenericAlias, StaticClassLiteral};

pub(in crate::types) type GenericOverrideBases<'db> =
    FxHashMap<StaticClassLiteral<'db>, GenericAlias<'db>>;

pub(in crate::types) struct OverrideCheckFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousOverrideCheckEffects)]
    pub(in crate::types) trait OverrideCheckEffects<'db> {
        type Error;

        #[operation(child)]
        async fn configuration(&self) -> Result<OverrideRulesConfig, Self::Error>;
        #[operation(source)]
        async fn scope(&self, class: StaticClassLiteral<'db>) -> Result<ScopeId<'db>, Self::Error>;
        #[operation(child)]
        async fn own_members(&self, scope: ScopeId<'db>) -> Result<FxHashSet<MemberWithDefinition<'db>>, Self::Error>;
        #[operation(child)]
        async fn identity_specialization(&self, class: StaticClassLiteral<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(child)]
        async fn inherited_conflicts(&self, class: StaticClassLiteral<'db>, specialized: ClassType<'db>, members: &FxHashSet<MemberWithDefinition<'db>>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn enum_metadata(&self, class: StaticClassLiteral<'db>) -> Result<Option<&'db EnumMetadata<'db>>, Self::Error>;
        #[operation(child)]
        async fn mro_bases(&self, class: ClassType<'db>) -> Result<Vec<ClassBase<'db>>, Self::Error>;
        #[operation(local)]
        async fn new_generic_bases(&self) -> Result<GenericOverrideBases<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_base(&self, bases: &[ClassBase<'db>], cursor: &mut usize) -> Result<Option<ClassBase<'db>>, Self::Error>;
        #[operation(source)]
        async fn generic_base(&self, base: ClassBase<'db>) -> Result<Option<(StaticClassLiteral<'db>, GenericAlias<'db>)>, Self::Error>;
        #[operation(local)]
        async fn insert_generic_base(&self, bases: &mut GenericOverrideBases<'db>, origin: StaticClassLiteral<'db>, base: GenericAlias<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn augment_ancestors(&self, class: ClassType<'db>, bases: &mut Vec<ClassBase<'db>>, generic: &GenericOverrideBases<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn member_cursor(&self, members: FxHashSet<MemberWithDefinition<'db>>) -> Result<IntoIter<MemberWithDefinition<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_member(&self, cursor: &mut IntoIter<MemberWithDefinition<'db>>) -> Result<Option<MemberWithDefinition<'db>>, Self::Error>;
        #[operation(child)]
        async fn check_member(&self, configuration: OverrideRulesConfig, enum_info: Option<&'db EnumMetadata<'db>>, class: ClassType<'db>, scope: ScopeId<'db>, bases: &[ClassBase<'db>], member: &MemberWithDefinition<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl OverrideCheckFacts {
        fn no_rules(&self, configuration: OverrideRulesConfig) -> bool {
            configuration.no_rules_enabled()
        }

        fn check_methods(&self, configuration: OverrideRulesConfig) -> bool {
            configuration.check_method_liskov_violations()
        }

        fn has_generic_bases(&self, bases: &GenericOverrideBases<'_>) -> bool {
            !bases.is_empty()
        }
    }

    #[synchronous(check_class_sync)]
    #[capabilities(effects = OverrideCheckEffects, facts = OverrideCheckFacts)]
    #[passive_values()]
    pub(in crate::types) async fn check_class_with<'db, E: OverrideCheckEffects<'db>>(
        class: StaticClassLiteral<'db>,
        inconsistent_generic_bases: bool,
        facts: OverrideCheckFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        let configuration = effects.configuration().await?;
        if facts.no_rules(configuration) {
            return Ok(());
        }

        let scope = effects.scope(class).await?;
        let own_members = effects.own_members(scope).await?;
        let specialized = effects.identity_specialization(class).await?;
        if facts.check_methods(configuration) && !inconsistent_generic_bases {
            effects.inherited_conflicts(class, specialized, &own_members).await?;
        }
        let enum_info = effects.enum_metadata(class).await?;
        let mut bases = effects.mro_bases(specialized).await?;
        if facts.check_methods(configuration) {
            let mut generic = effects.new_generic_bases().await?;
            let mut cursor = 0;
            #[cursor_loop]
            while let Some(base) = effects.next_base(&bases, &mut cursor).await? {
                if let Some((origin, alias)) = effects.generic_base(base).await? {
                    effects.insert_generic_base(&mut generic, origin, alias).await?;
                }
            }
            if facts.has_generic_bases(&generic) {
                effects.augment_ancestors(specialized, &mut bases, &generic).await?;
            }
        }

        let mut members = effects.member_cursor(own_members).await?;
        #[cursor_loop]
        while let Some(member) = effects.next_member(&mut members).await? {
            effects.check_member(configuration, enum_info, specialized, scope, &bases, &member).await?;
        }
        Ok(())
    }
}

pub(super) struct OrdinaryOverrideCheckEffects<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
}

impl<'db> SynchronousOverrideCheckEffects<'db> for OrdinaryOverrideCheckEffects<'_, 'db, '_> {
    type Error = Infallible;

    fn configuration(&self) -> Result<OverrideRulesConfig, Self::Error> {
        Ok(OverrideRulesConfig::from(self.context))
    }

    fn scope(&self, class: StaticClassLiteral<'db>) -> Result<ScopeId<'db>, Self::Error> {
        Ok(class.body_scope(self.context.db()))
    }

    fn own_members(
        &self,
        scope: ScopeId<'db>,
    ) -> Result<FxHashSet<MemberWithDefinition<'db>>, Self::Error> {
        Ok(all_end_of_scope_members(self.context.db(), scope).collect())
    }

    fn identity_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(class.identity_specialization(self.context.db()))
    }

    fn inherited_conflicts(
        &self,
        class: StaticClassLiteral<'db>,
        specialized: ClassType<'db>,
        members: &FxHashSet<MemberWithDefinition<'db>>,
    ) -> Result<(), Self::Error> {
        check_inherited_method_conflicts(self.context, class, specialized, members);
        Ok(())
    }

    fn enum_metadata(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<&'db EnumMetadata<'db>>, Self::Error> {
        Ok(enum_metadata(self.context.db(), class.into()))
    }

    fn mro_bases(&self, class: ClassType<'db>) -> Result<Vec<ClassBase<'db>>, Self::Error> {
        Ok(class.iter_mro(self.context.db()).skip(1).collect())
    }

    fn new_generic_bases(&self) -> Result<GenericOverrideBases<'db>, Self::Error> {
        Ok(FxHashMap::default())
    }

    fn next_base(
        &self,
        bases: &[ClassBase<'db>],
        cursor: &mut usize,
    ) -> Result<Option<ClassBase<'db>>, Self::Error> {
        Ok(next_override_base(bases, cursor))
    }

    fn generic_base(
        &self,
        base: ClassBase<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, GenericAlias<'db>)>, Self::Error> {
        Ok(base
            .into_class()
            .and_then(ClassType::into_generic_alias)
            .map(|base| (base.origin(self.context.db()), base)))
    }

    fn insert_generic_base(
        &self,
        bases: &mut GenericOverrideBases<'db>,
        origin: StaticClassLiteral<'db>,
        base: GenericAlias<'db>,
    ) -> Result<(), Self::Error> {
        bases.insert(origin, base);
        Ok(())
    }

    fn augment_ancestors(
        &self,
        class: ClassType<'db>,
        bases: &mut Vec<ClassBase<'db>>,
        generic: &GenericOverrideBases<'db>,
    ) -> Result<(), Self::Error> {
        let db = self.context.db();
        // Overrides must respect every inherited specialization. Keep the MRO's bases first
        // so the selected inherited method is unchanged.
        bases.extend(
            class
                .iter_explicit_ancestors(db, self.context.program_environment())
                .filter_map(ClassType::into_generic_alias)
                .filter(|ancestor| {
                    generic
                        .get(&ancestor.origin(db))
                        .is_some_and(|base| base != ancestor)
                })
                .map(|ancestor| ClassBase::Class(ClassType::Generic(ancestor))),
        );
        Ok(())
    }

    fn member_cursor(
        &self,
        members: FxHashSet<MemberWithDefinition<'db>>,
    ) -> Result<IntoIter<MemberWithDefinition<'db>>, Self::Error> {
        Ok(members.into_iter())
    }

    fn next_member(
        &self,
        cursor: &mut IntoIter<MemberWithDefinition<'db>>,
    ) -> Result<Option<MemberWithDefinition<'db>>, Self::Error> {
        Ok(cursor.next())
    }

    fn check_member(
        &self,
        configuration: OverrideRulesConfig,
        enum_info: Option<&'db EnumMetadata<'db>>,
        class: ClassType<'db>,
        scope: ScopeId<'db>,
        bases: &[ClassBase<'db>],
        member: &MemberWithDefinition<'db>,
    ) -> Result<(), Self::Error> {
        check_class_declaration(
            self.context,
            configuration,
            enum_info,
            class,
            scope,
            bases,
            member,
        );
        Ok(())
    }
}

pub(in crate::types) fn next_override_base<'db>(
    bases: &[ClassBase<'db>],
    cursor: &mut usize,
) -> Option<ClassBase<'db>> {
    let base = *bases.get(*cursor)?;
    *cursor += 1;
    Some(base)
}
