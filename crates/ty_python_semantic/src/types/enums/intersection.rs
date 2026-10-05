//! Enum intersection scans and canonical member lookup share their source-read order.

use std::convert::Infallible;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use ty_mapping_probe_macros::shared_semantic_family;

use super::{EnumClassLiteral, EnumComplement};
use crate::types::{EnumLiteralType, NegativeIntersectionElements, NominalInstanceType, Type};
use crate::{Db, FxOrderSet, ProgramEnvironment};

pub(in crate::types) struct EnumIntersectionFacts;

pub(in crate::types) struct ExcludedNames {
    pub(in crate::types) names: FxHashSet<Name>,
    pub(in crate::types) max_name_bytes: usize,
}

pub(in crate::types) struct Rest<'db> {
    pub(in crate::types) values: SmallVec<[Type<'db>; 1]>,
}

pub(in crate::types) struct OrderedExclusions {
    pub(in crate::types) names: FxOrderSet<Name>,
    pub(in crate::types) max_name_bytes: usize,
}

pub(in crate::types) struct OrderedRest<'db> {
    pub(in crate::types) values: FxOrderSet<Type<'db>>,
    pub(in crate::types) max_inline_bytes: usize,
}

pub(super) struct OrdinaryEnumIntersectionEffects<'db> {
    pub(super) db: &'db dyn Db,
}

pub(in crate::types) fn next_negative<'db>(
    values: &NegativeIntersectionElements<'db>,
    cursor: &mut usize,
) -> Option<Type<'db>> {
    let result = match values {
        NegativeIntersectionElements::Empty => None,
        NegativeIntersectionElements::Single(ty) => (*cursor == 0).then_some(*ty),
        NegativeIntersectionElements::Multiple(values) => values.get_index(*cursor).copied(),
    };
    *cursor += usize::from(result.is_some());
    result
}

pub(in crate::types) fn has_empty_enum_complement<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    positive: &FxOrderSet<Type<'db>>,
    negative: &NegativeIntersectionElements<'db>,
) -> bool {
    match has_empty_enum_complement_sync(
        env,
        positive,
        negative,
        EnumIntersectionFacts,
        &OrdinaryEnumIntersectionEffects { db },
    ) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousEnumIntersectionEffects)]
    pub(in crate::types) trait EnumIntersectionEffects<'db> {
        type Error;

        #[operation(local)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_positive(&self, values: &FxOrderSet<Type<'db>>, cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_negative(&self, values: &NegativeIntersectionElements<'db>, cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn instance_enum_class(&self, env: &ProgramEnvironment<'db>, instance: NominalInstanceType<'db>) -> Result<Option<EnumClassLiteral<'db>>, Self::Error>;
        #[operation(source)]
        async fn exhaustive(&self, class: EnumClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn literal_class(&self, literal: EnumLiteralType<'db>) -> Result<EnumClassLiteral<'db>, Self::Error>;
        #[operation(source)]
        async fn literal_name(&self, literal: EnumLiteralType<'db>) -> Result<&'db Name, Self::Error>;
        #[operation(source)]
        async fn members(&self, class: EnumClassLiteral<'db>) -> Result<&'db [(Name, Type<'db>)], Self::Error>;
        #[operation(source)]
        async fn aliases(&self, class: EnumClassLiteral<'db>) -> Result<&'db [(Name, Name)], Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_member(&self, members: &'db [(Name, Type<'db>)], cursor: &mut usize) -> Result<Option<&'db (Name, Type<'db>)>, Self::Error>;
        #[operation(local)]
        async fn names_equal(&self, left: &Name, right: &Name) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn find_alias(&self, aliases: &'db [(Name, Name)], name: &Name) -> Result<Option<&'db Name>, Self::Error>;
        #[operation(child)]
        async fn resolve_member(&self, class: EnumClassLiteral<'db>, name: &Name) -> Result<Option<&'db Name>, Self::Error>;
        #[operation(local)]
        async fn new_exclusions(&self) -> Result<ExcludedNames, Self::Error>;
        #[operation(local)]
        async fn exclude(&self, excluded: &mut ExcludedNames, name: &Name) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn exclusions_empty(&self, excluded: &ExcludedNames) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn is_excluded(&self, excluded: &ExcludedNames, name: &Name) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn finish_exclusions(&self, excluded: ExcludedNames) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn new_rest(&self) -> Result<Rest<'db>, Self::Error>;
        #[operation(local)]
        async fn push_rest(&self, rest: &mut Rest<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_rest(&self, rest: &Rest<'db>, cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn finish_rest(&self, rest: Rest<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn new_ordered_exclusions(&self) -> Result<OrderedExclusions, Self::Error>;
        #[operation(local)]
        async fn push_ordered_exclusion(&self, excluded: &mut OrderedExclusions, name: &Name) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn new_ordered_rest(&self) -> Result<OrderedRest<'db>, Self::Error>;
        #[operation(local)]
        async fn push_ordered_rest(&self, rest: &mut OrderedRest<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn intern(&self, class: EnumClassLiteral<'db>, excluded: OrderedExclusions, rest: OrderedRest<'db>) -> Result<EnumComplement<'db>, Self::Error>;
        #[operation(source)]
        async fn complement_rest_empty(&self, complement: EnumComplement<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn complement_class(&self, complement: EnumComplement<'db>) -> Result<EnumClassLiteral<'db>, Self::Error>;
        #[operation(source)]
        async fn member_count(&self, class: EnumClassLiteral<'db>) -> Result<usize, Self::Error>;
        #[operation(source)]
        async fn excluded_count(&self, complement: EnumComplement<'db>) -> Result<usize, Self::Error>;
    }

    #[finite_capability]
    impl EnumIntersectionFacts {
        fn enum_literal<'db>(&self, ty: Type<'db>) -> Option<EnumLiteralType<'db>> {
            ty.as_enum_literal()
        }

        fn member_name<'db>(&self, member: &'db (Name, Type<'db>)) -> &'db Name {
            &member.0
        }

        fn one_remaining(&self, members: usize, excluded: usize) -> bool {
            members - excluded == 1
        }

        fn same_class<'db>(&self, left: EnumClassLiteral<'db>, right: EnumClassLiteral<'db>) -> bool {
            left == right
        }
    }

    #[synchronous(resolve_member_entry_sync)]
    #[capabilities(effects = EnumIntersectionEffects, facts = EnumIntersectionFacts)]
    #[passive_values()]
    pub(in crate::types) async fn resolve_member_entry_with<'db, E: EnumIntersectionEffects<'db>>(
        class: EnumClassLiteral<'db>,
        name: &Name,
        facts: EnumIntersectionFacts,
        effects: &E,
    ) -> Result<Option<&'db (Name, Type<'db>)>, E::Error> {
        let members = effects.members(class).await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(member) = effects.next_member(members, &mut cursor).await? {
            if effects.names_equal(facts.member_name(member), name).await? {
                return Ok(Some(member));
            }
        }
        let aliases = effects.aliases(class).await?;
        let Some(canonical_name) = effects.find_alias(aliases, name).await? else {
            return Ok(None);
        };
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(member) = effects.next_member(members, &mut cursor).await? {
            if effects.names_equal(facts.member_name(member), canonical_name).await? {
                return Ok(Some(member));
            }
        }
        Ok(None)
    }

    #[synchronous(has_empty_enum_complement_sync)]
    #[capabilities(effects = EnumIntersectionEffects, facts = EnumIntersectionFacts)]
    #[passive_values()]
    pub(in crate::types) async fn has_empty_enum_complement_with<'db, E: EnumIntersectionEffects<'db>>(
        env: &ProgramEnvironment<'db>,
        positive: &FxOrderSet<Type<'db>>,
        negative: &NegativeIntersectionElements<'db>,
        facts: EnumIntersectionFacts,
        effects: &E,
    ) -> Result<bool, E::Error> {
        effects.checkpoint().await?;
        let mut positive_cursor = 0;
        #[cursor_loop]
        while let Some(positive) = effects.next_positive(positive, &mut positive_cursor).await? {
            let Type::NominalInstance(instance) = positive else {
                continue;
            };
            let Some(enum_class) = effects.instance_enum_class(env, instance).await? else {
                continue;
            };
            if !effects.exhaustive(enum_class).await? {
                continue;
            }
            let mut excluded = effects.new_exclusions().await?;
            let mut negative_cursor = 0;
            #[cursor_loop]
            while let Some(negative) = effects.next_negative(negative, &mut negative_cursor).await? {
                let Some(literal) = facts.enum_literal(negative) else {
                    continue;
                };
                if !facts.same_class(effects.literal_class(literal).await?, enum_class) {
                    continue;
                }
                let name = effects.literal_name(literal).await?;
                let Some(canonical_name) = effects.resolve_member(enum_class, name).await? else {
                    continue;
                };
                effects.exclude(&mut excluded, canonical_name).await?;
            }
            if effects.exclusions_empty(&excluded).await? {
                effects.finish_exclusions(excluded).await?;
                continue;
            }
            let members = effects.members(enum_class).await?;
            let mut member_cursor = 0;
            #[passive_state]
            let mut all_excluded = true;
            #[cursor_loop]
            while let Some(member) = effects.next_member(members, &mut member_cursor).await? {
                if !effects.is_excluded(&excluded, facts.member_name(member)).await? {
                    all_excluded = false;
                    break;
                }
            }
            effects.finish_exclusions(excluded).await?;
            if all_excluded {
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[synchronous(from_intersection_parts_sync)]
    #[capabilities(effects = EnumIntersectionEffects, facts = EnumIntersectionFacts)]
    #[passive_values()]
    pub(in crate::types) async fn from_intersection_parts_with<'db, E: EnumIntersectionEffects<'db>>(
        env: &ProgramEnvironment<'db>,
        positive: &FxOrderSet<Type<'db>>,
        negative: &NegativeIntersectionElements<'db>,
        facts: EnumIntersectionFacts,
        effects: &E,
    ) -> Result<Option<EnumComplement<'db>>, E::Error> {
        effects.checkpoint().await?;
        #[passive_state]
        let mut enum_class = None;
        let mut rest = effects.new_rest().await?;
        let mut positive_cursor = 0;
        #[cursor_loop]
        while let Some(positive) = effects.next_positive(positive, &mut positive_cursor).await? {
            let Type::NominalInstance(instance) = positive else {
                effects.push_rest(&mut rest, positive).await?;
                continue;
            };
            let Some(class) = effects.instance_enum_class(env, instance).await? else {
                effects.push_rest(&mut rest, positive).await?;
                continue;
            };
            match enum_class {
                Some(_) => {
                    effects.finish_rest(rest).await?;
                    return Ok(None);
                }
                None => {}
            }
            enum_class = Some(class);
        }
        let Some(enum_class) = enum_class else {
            effects.finish_rest(rest).await?;
            return Ok(None);
        };
        if !effects.exhaustive(enum_class).await? {
            effects.finish_rest(rest).await?;
            return Ok(None);
        }
        let mut excluded = effects.new_exclusions().await?;
        let mut negative_cursor = 0;
        #[cursor_loop]
        while let Some(negative) = effects.next_negative(negative, &mut negative_cursor).await? {
            let Some(literal) = facts.enum_literal(negative) else {
                effects.finish_exclusions(excluded).await?;
                effects.finish_rest(rest).await?;
                return Ok(None);
            };
            if !facts.same_class(effects.literal_class(literal).await?, enum_class) {
                effects.finish_exclusions(excluded).await?;
                effects.finish_rest(rest).await?;
                return Ok(None);
            }
            let name = effects.literal_name(literal).await?;
            let Some(canonical_name) = effects.resolve_member(enum_class, name).await? else {
                effects.finish_exclusions(excluded).await?;
                effects.finish_rest(rest).await?;
                return Ok(None);
            };
            effects.exclude(&mut excluded, canonical_name).await?;
        }
        if effects.exclusions_empty(&excluded).await? {
            effects.finish_exclusions(excluded).await?;
            effects.finish_rest(rest).await?;
            return Ok(None);
        }
        let members = effects.members(enum_class).await?;
        let mut ordered_excluded = effects.new_ordered_exclusions().await?;
        let mut member_cursor = 0;
        #[cursor_loop]
        while let Some(member) = effects.next_member(members, &mut member_cursor).await? {
            let name = facts.member_name(member);
            if effects.is_excluded(&excluded, name).await? {
                effects.push_ordered_exclusion(&mut ordered_excluded, name).await?;
            }
        }
        let mut ordered_rest = effects.new_ordered_rest().await?;
        let mut rest_cursor = 0;
        #[cursor_loop]
        while let Some(ty) = effects.next_rest(&rest, &mut rest_cursor).await? {
            effects.push_ordered_rest(&mut ordered_rest, ty).await?;
        }
        effects.finish_rest(rest).await?;
        let complement = effects.intern(enum_class, ordered_excluded, ordered_rest).await?;
        effects.finish_exclusions(excluded).await?;
        Ok(Some(complement))
    }

    #[synchronous(is_singleton_sync)]
    #[capabilities(effects = EnumIntersectionEffects, facts = EnumIntersectionFacts)]
    #[passive_values()]
    pub(in crate::types) async fn is_singleton_with<'db, E: EnumIntersectionEffects<'db>>(
        complement: EnumComplement<'db>,
        facts: EnumIntersectionFacts,
        effects: &E,
    ) -> Result<bool, E::Error> {
        if !effects.complement_rest_empty(complement).await? {
            return Ok(false);
        }
        let class = effects.complement_class(complement).await?;
        let members = effects.member_count(class).await?;
        let excluded = effects.excluded_count(complement).await?;
        Ok(facts.one_remaining(members, excluded))
    }
}

impl<'db> SynchronousEnumIntersectionEffects<'db> for OrdinaryEnumIntersectionEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn next_positive(
        &self,
        values: &FxOrderSet<Type<'db>>,
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let result = values.get_index(*cursor).copied();
        *cursor += usize::from(result.is_some());
        Ok(result)
    }
    fn next_negative(
        &self,
        values: &NegativeIntersectionElements<'db>,
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(next_negative(values, cursor))
    }
    fn instance_enum_class(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> Result<Option<EnumClassLiteral<'db>>, Self::Error> {
        Ok(instance
            .class_literal(self.db, env)
            .into_enum_class(self.db))
    }
    fn exhaustive(&self, class: EnumClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.members_are_exhaustive(self.db))
    }
    fn literal_class(
        &self,
        literal: EnumLiteralType<'db>,
    ) -> Result<EnumClassLiteral<'db>, Self::Error> {
        Ok(literal.enum_class_literal(self.db))
    }
    fn literal_name(&self, literal: EnumLiteralType<'db>) -> Result<&'db Name, Self::Error> {
        Ok(literal.name(self.db))
    }
    fn members(
        &self,
        class: EnumClassLiteral<'db>,
    ) -> Result<&'db [(Name, Type<'db>)], Self::Error> {
        Ok(class.members(self.db))
    }
    fn aliases(&self, class: EnumClassLiteral<'db>) -> Result<&'db [(Name, Name)], Self::Error> {
        Ok(class.aliases(self.db))
    }
    fn next_member(
        &self,
        members: &'db [(Name, Type<'db>)],
        cursor: &mut usize,
    ) -> Result<Option<&'db (Name, Type<'db>)>, Self::Error> {
        let result = members.get(*cursor);
        *cursor += usize::from(result.is_some());
        Ok(result)
    }
    fn names_equal(&self, left: &Name, right: &Name) -> Result<bool, Self::Error> {
        Ok(left == right)
    }
    fn find_alias(
        &self,
        aliases: &'db [(Name, Name)],
        name: &Name,
    ) -> Result<Option<&'db Name>, Self::Error> {
        Ok(aliases
            .binary_search_by(|(alias, _)| alias.cmp(name))
            .ok()
            .map(|index| &aliases[index].1))
    }
    fn resolve_member(
        &self,
        class: EnumClassLiteral<'db>,
        name: &Name,
    ) -> Result<Option<&'db Name>, Self::Error> {
        Ok(
            resolve_member_entry_sync(class, name, EnumIntersectionFacts, self)?
                .map(|(member, _)| member),
        )
    }
    fn new_exclusions(&self) -> Result<ExcludedNames, Self::Error> {
        Ok(ExcludedNames {
            names: FxHashSet::default(),
            max_name_bytes: 0,
        })
    }
    fn exclude(&self, excluded: &mut ExcludedNames, name: &Name) -> Result<(), Self::Error> {
        excluded.max_name_bytes = excluded.max_name_bytes.max(name.as_str().len());
        excluded.names.insert(name.clone());
        Ok(())
    }
    fn exclusions_empty(&self, excluded: &ExcludedNames) -> Result<bool, Self::Error> {
        Ok(excluded.names.is_empty())
    }
    fn is_excluded(&self, excluded: &ExcludedNames, name: &Name) -> Result<bool, Self::Error> {
        Ok(excluded.names.contains(name))
    }
    fn finish_exclusions(&self, excluded: ExcludedNames) -> Result<(), Self::Error> {
        drop(excluded);
        Ok(())
    }
    fn new_rest(&self) -> Result<Rest<'db>, Self::Error> {
        Ok(Rest {
            values: SmallVec::default(),
        })
    }
    fn push_rest(&self, rest: &mut Rest<'db>, ty: Type<'db>) -> Result<(), Self::Error> {
        rest.values.push(ty);
        Ok(())
    }
    fn next_rest(
        &self,
        rest: &Rest<'db>,
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let result = rest.values.get(*cursor).copied();
        *cursor += usize::from(result.is_some());
        Ok(result)
    }
    fn finish_rest(&self, rest: Rest<'db>) -> Result<(), Self::Error> {
        drop(rest);
        Ok(())
    }
    fn new_ordered_exclusions(&self) -> Result<OrderedExclusions, Self::Error> {
        Ok(OrderedExclusions {
            names: FxOrderSet::default(),
            max_name_bytes: 0,
        })
    }
    fn push_ordered_exclusion(
        &self,
        excluded: &mut OrderedExclusions,
        name: &Name,
    ) -> Result<(), Self::Error> {
        excluded.max_name_bytes = excluded.max_name_bytes.max(name.as_str().len());
        excluded.names.insert(name.clone());
        Ok(())
    }
    fn new_ordered_rest(&self) -> Result<OrderedRest<'db>, Self::Error> {
        Ok(OrderedRest {
            values: FxOrderSet::default(),
            max_inline_bytes: 0,
        })
    }
    fn push_ordered_rest(
        &self,
        rest: &mut OrderedRest<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        rest.max_inline_bytes = rest.max_inline_bytes.max(ty.inline_payload_bytes());
        rest.values.insert(ty);
        Ok(())
    }
    fn intern(
        &self,
        class: EnumClassLiteral<'db>,
        excluded: OrderedExclusions,
        rest: OrderedRest<'db>,
    ) -> Result<EnumComplement<'db>, Self::Error> {
        Ok(EnumComplement::new(
            self.db,
            class,
            excluded.names,
            rest.values,
        ))
    }
    fn complement_rest_empty(&self, complement: EnumComplement<'db>) -> Result<bool, Self::Error> {
        Ok(complement.rest(self.db).is_empty())
    }
    fn complement_class(
        &self,
        complement: EnumComplement<'db>,
    ) -> Result<EnumClassLiteral<'db>, Self::Error> {
        Ok(complement.enum_class_literal(self.db))
    }
    fn member_count(&self, class: EnumClassLiteral<'db>) -> Result<usize, Self::Error> {
        Ok(class.members(self.db).len())
    }
    fn excluded_count(&self, complement: EnumComplement<'db>) -> Result<usize, Self::Error> {
        Ok(complement.excluded_names(self.db).len())
    }
}
