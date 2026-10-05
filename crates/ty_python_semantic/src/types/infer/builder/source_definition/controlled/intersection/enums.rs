//! Enum scans admit field reads and the storage retained between semantic children.

use std::alloc::Layout;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use salsa::execution_probe::{RunError, RunResult};
use smallvec::SmallVec;

use super::super::storage::{StorageQuote, ordered_merge, slots, table_merge};
use super::{SourceAccess, SourceEffects, SourceOperation, buffer_push_quote, quotation};
use crate::types::class_selection::NominalSelectionEffects;
use crate::types::enums::intersection::{
    self, EnumIntersectionEffects, EnumIntersectionFacts, ExcludedNames, OrderedExclusions,
    OrderedRest, Rest,
};
use crate::types::enums::{EnumClassLiteral, EnumComplement};
use crate::types::set_theoretic::finite_alternatives::{self, FiniteAlternativeEffects};
use crate::types::{
    ClassLiteral, ClassType, EnumLiteralType, IntersectionType, NegativeIntersectionElements,
    NominalInstanceType, Type,
};
use crate::{FxOrderSet, ProgramEnvironment};

struct EnumEffects<'source, 'access, 'run, 'db: 'run, A> {
    source: &'source SourceEffects<'access, 'run, 'db, A>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn intersection_alternatives(
        &self,
        env: &ProgramEnvironment<'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        finite_alternatives::produce_with(intersection, env, &EnumEffects { source: self }).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FiniteAlternativeEffects<'db>
    for EnumEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn positive(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<&'db FxOrderSet<Type<'db>>> {
        self.source
            .field(
                intersection
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .positive(),
            )
            .await
    }

    async fn negative(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<&'db NegativeIntersectionElements<'db>> {
        self.source
            .field(
                intersection
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .negative(),
            )
            .await
    }

    async fn enum_complement(
        &self,
        env: &ProgramEnvironment<'db>,
        positive: &FxOrderSet<Type<'db>>,
        negative: &NegativeIntersectionElements<'db>,
    ) -> RunResult<Option<EnumComplement<'db>>> {
        enum_complement(self.source, env, positive, negative).await
    }

    async fn remaining_literal_union(
        &self,
        env: &ProgramEnvironment<'db>,
        complement: EnumComplement<'db>,
    ) -> RunResult<Type<'db>> {
        remaining_literal_union(self.source, env, complement).await
    }
}

pub(super) async fn has_empty_enum_complement<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    source: &SourceEffects<'_, 'run, 'db, A>,
    env: &ProgramEnvironment<'db>,
    positive: &FxOrderSet<Type<'db>>,
    negative: &NegativeIntersectionElements<'db>,
) -> RunResult<bool> {
    intersection::has_empty_enum_complement_with(
        env,
        positive,
        negative,
        EnumIntersectionFacts,
        &EnumEffects { source },
    )
    .await
}

pub(super) async fn enum_complement<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    source: &SourceEffects<'_, 'run, 'db, A>,
    env: &ProgramEnvironment<'db>,
    positive: &FxOrderSet<Type<'db>>,
    negative: &NegativeIntersectionElements<'db>,
) -> RunResult<Option<EnumComplement<'db>>> {
    intersection::from_intersection_parts_with(
        env,
        positive,
        negative,
        EnumIntersectionFacts,
        &EnumEffects { source },
    )
    .await
}

pub(super) async fn is_singleton<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    source: &SourceEffects<'_, 'run, 'db, A>,
    complement: EnumComplement<'db>,
) -> RunResult<bool> {
    intersection::is_singleton_with(complement, EnumIntersectionFacts, &EnumEffects { source })
        .await
}

pub(super) async fn remaining_literal_union<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    source: &SourceEffects<'_, 'run, 'db, A>,
    _env: &ProgramEnvironment<'db>,
    _complement: EnumComplement<'db>,
) -> RunResult<Type<'db>> {
    source
        .unavailable(SourceOperation::EnumComplementLiteralUnion)
        .await
}

fn name_lookup_work(capacity: usize, incoming: usize, maximum: usize) -> Option<usize> {
    slots(capacity)?.checked_add(1)?.checked_mul(
        incoming
            .checked_add(maximum)?
            .checked_add(size_of::<Name>().checked_mul(2)?)?
            .checked_add(8)?,
    )
}

fn name_insert_quote(
    len: usize,
    capacity: usize,
    incoming: usize,
    maximum: usize,
    ordered: bool,
) -> Option<StorageQuote> {
    let (table, new_slots) = table_merge::<Name>(len, capacity, 1, 0)?;
    let mut quote = if ordered {
        ordered_merge::<Name>(len, capacity, 1)?
    } else {
        table
    };
    // These owners are insert-only. Hashing may revisit every retained name on growth;
    // collisions can compare against every slot. Name clones share their backing allocation.
    quote.work = quote
        .work
        .checked_add(name_lookup_work(capacity, incoming, maximum)?)?
        .checked_add(
            new_slots.checked_add(1)?.checked_mul(
                maximum
                    .max(incoming)
                    .checked_add(size_of::<Name>())?
                    .checked_add(8)?,
            )?,
        )?
        .checked_add(
            len.checked_mul(new_slots.checked_add(1)?)?
                .checked_mul(size_of::<Name>().checked_add(8)?)?,
        )?
        .checked_add(incoming.checked_add(size_of::<Name>())?)?
        .checked_add(quote.bytes.checked_mul(2)?)?;
    Layout::from_size_align(quote.bytes, align_of::<(usize, Name)>().max(16)).ok()?;
    Some(quote)
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> EnumIntersectionEffects<'db>
    for EnumEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.source.work(1).await
    }

    async fn next_positive(
        &self,
        values: &FxOrderSet<Type<'db>>,
        cursor: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.source
            .local(size_of::<Type<'db>>() + 4, 0, || {
                let result = values.get_index(*cursor).copied();
                *cursor += usize::from(result.is_some());
                result
            })
            .await
    }

    async fn next_negative(
        &self,
        values: &NegativeIntersectionElements<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.source
            .local(size_of::<Type<'db>>() + 5, 0, || {
                intersection::next_negative(values, cursor)
            })
            .await
    }

    async fn instance_enum_class(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<EnumClassLiteral<'db>>> {
        self.source.environment_program(env).await?;
        let class = NominalSelectionEffects::instance_class(self.source, instance).await?;
        let class = match class {
            ClassType::NonGeneric(class) => self.source.local(1, 0, || class).await?,
            ClassType::Generic(alias) => ClassLiteral::Static(
                self.source
                    .field(
                        alias
                            .field_requests(self.source.access.endpoint().field_request_context())
                            .origin(),
                    )
                    .await?,
            ),
        };
        self.source.enum_class_literal_source(class).await
    }

    async fn exhaustive(&self, class: EnumClassLiteral<'db>) -> RunResult<bool> {
        self.source
            .field(
                class
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .members_are_exhaustive(),
            )
            .await
    }

    async fn literal_class(
        &self,
        literal: EnumLiteralType<'db>,
    ) -> RunResult<EnumClassLiteral<'db>> {
        self.source
            .field(
                literal
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .enum_class_literal(),
            )
            .await
    }

    async fn literal_name(&self, literal: EnumLiteralType<'db>) -> RunResult<&'db Name> {
        self.source
            .field(
                literal
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .name(),
            )
            .await
    }

    async fn members(&self, class: EnumClassLiteral<'db>) -> RunResult<&'db [(Name, Type<'db>)]> {
        let members = self
            .source
            .field(
                class
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .members(),
            )
            .await?;
        self.source.local(1, 0, || members.as_ref()).await
    }

    async fn aliases(&self, class: EnumClassLiteral<'db>) -> RunResult<&'db [(Name, Name)]> {
        let aliases = self
            .source
            .field(
                class
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .aliases(),
            )
            .await?;
        self.source.local(1, 0, || aliases.as_ref()).await
    }

    async fn next_member(
        &self,
        members: &'db [(Name, Type<'db>)],
        cursor: &mut usize,
    ) -> RunResult<Option<&'db (Name, Type<'db>)>> {
        self.source
            .local(4, 0, || {
                let result = members.get(*cursor);
                *cursor += usize::from(result.is_some());
                result
            })
            .await
    }

    async fn names_equal(&self, left: &Name, right: &Name) -> RunResult<bool> {
        let work = quotation(
            self.source
                .local(3, 0, || {
                    left.as_str()
                        .len()
                        .checked_add(right.as_str().len())
                        .and_then(|n| n.checked_add(2))
                })
                .await?,
        )?;
        self.source.local(work, 0, || left == right).await
    }

    async fn find_alias(
        &self,
        aliases: &'db [(Name, Name)],
        name: &Name,
    ) -> RunResult<Option<&'db Name>> {
        let name_bytes = self.source.local(1, 0, || name.as_str().len()).await?;
        let mut maximum = 0;
        let mut cursor = 0;
        while let Some(bytes) = self
            .source
            .local(4, 0, || {
                let result = aliases.get(cursor).map(|(alias, _)| alias.as_str().len());
                cursor += usize::from(result.is_some());
                result
            })
            .await?
        {
            maximum = maximum.max(bytes);
        }
        let work = quotation(
            cursor
                .checked_add(1)
                .and_then(|n| n.checked_mul(name_bytes.checked_add(maximum)?.checked_add(8)?)),
        )?;
        self.source
            .local(work, 0, || {
                aliases
                    .binary_search_by(|(alias, _)| alias.cmp(name))
                    .ok()
                    .map(|index| &aliases[index].1)
            })
            .await
    }

    async fn resolve_member(
        &self,
        class: EnumClassLiteral<'db>,
        name: &Name,
    ) -> RunResult<Option<&'db Name>> {
        let member =
            intersection::resolve_member_entry_with(class, name, EnumIntersectionFacts, self)
                .await?;
        self.source
            .local(1, 0, || member.map(|(name, _)| name))
            .await
    }

    async fn new_exclusions(&self) -> RunResult<ExcludedNames> {
        self.source
            .local(size_of::<ExcludedNames>() * 2 + 1, 0, || ExcludedNames {
                names: FxHashSet::default(),
                max_name_bytes: 0,
            })
            .await
    }

    async fn exclude(&self, excluded: &mut ExcludedNames, name: &Name) -> RunResult<()> {
        let (len, capacity, maximum, incoming) = self
            .source
            .local(4, 0, || {
                (
                    excluded.names.len(),
                    excluded.names.capacity(),
                    excluded.max_name_bytes,
                    name.as_str().len(),
                )
            })
            .await?;
        let quote = quotation(name_insert_quote(len, capacity, incoming, maximum, false))?;
        self.source
            .local(quote.work, quote.bytes, || {
                excluded.max_name_bytes = maximum.max(incoming);
                excluded.names.insert(name.clone());
            })
            .await
    }

    async fn exclusions_empty(&self, excluded: &ExcludedNames) -> RunResult<bool> {
        self.source.local(1, 0, || excluded.names.is_empty()).await
    }

    async fn is_excluded(&self, excluded: &ExcludedNames, name: &Name) -> RunResult<bool> {
        let work = quotation(
            self.source
                .local(3, 0, || {
                    name_lookup_work(
                        excluded.names.capacity(),
                        name.as_str().len(),
                        excluded.max_name_bytes,
                    )
                })
                .await?,
        )?;
        self.source
            .local(work, 0, || excluded.names.contains(name))
            .await
    }

    async fn finish_exclusions(&self, excluded: ExcludedNames) -> RunResult<()> {
        // Every insertion prepays the added name and any new backing, including disposal
        // when a later source dependency refuses or the owning future is cancelled.
        self.source.work(1).await?;
        drop(excluded);
        Ok(())
    }

    async fn new_rest(&self) -> RunResult<Rest<'db>> {
        self.source
            .local(size_of::<Rest<'db>>() * 2 + 1, 0, || Rest {
                values: SmallVec::new(),
            })
            .await
    }

    async fn push_rest(&self, rest: &mut Rest<'db>, ty: Type<'db>) -> RunResult<()> {
        let storage = self
            .source
            .local(3, 0, || {
                (
                    rest.values.len(),
                    rest.values.capacity(),
                    rest.values.spilled(),
                )
            })
            .await?;
        let quote = quotation(buffer_push_quote::<Type<'db>>(storage))?;
        self.source
            .local(quote.work, quote.bytes, || rest.values.push(ty))
            .await
    }

    async fn next_rest(
        &self,
        rest: &Rest<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.source
            .local(size_of::<Type<'db>>() + 4, 0, || {
                let result = rest.values.get(*cursor).copied();
                *cursor += usize::from(result.is_some());
                result
            })
            .await
    }

    async fn finish_rest(&self, rest: Rest<'db>) -> RunResult<()> {
        self.source.work(1).await?;
        drop(rest);
        Ok(())
    }

    async fn new_ordered_exclusions(&self) -> RunResult<OrderedExclusions> {
        self.source
            .local(size_of::<OrderedExclusions>() * 2 + 1, 0, || {
                OrderedExclusions {
                    names: FxOrderSet::default(),
                    max_name_bytes: 0,
                }
            })
            .await
    }

    async fn push_ordered_exclusion(
        &self,
        excluded: &mut OrderedExclusions,
        name: &Name,
    ) -> RunResult<()> {
        let (len, capacity, maximum, incoming) = self
            .source
            .local(4, 0, || {
                (
                    excluded.names.len(),
                    excluded.names.capacity(),
                    excluded.max_name_bytes,
                    name.as_str().len(),
                )
            })
            .await?;
        let quote = quotation(name_insert_quote(len, capacity, incoming, maximum, true))?;
        self.source
            .local(quote.work, quote.bytes, || {
                excluded.max_name_bytes = maximum.max(incoming);
                excluded.names.insert(name.clone());
            })
            .await
    }

    async fn new_ordered_rest(&self) -> RunResult<OrderedRest<'db>> {
        self.source
            .local(size_of::<OrderedRest<'db>>() * 2 + 1, 0, || OrderedRest {
                values: FxOrderSet::default(),
                max_inline_bytes: 0,
            })
            .await
    }

    async fn push_ordered_rest(&self, rest: &mut OrderedRest<'db>, ty: Type<'db>) -> RunResult<()> {
        let (len, capacity, maximum, incoming) = self
            .source
            .local(4, 0, || {
                (
                    rest.values.len(),
                    rest.values.capacity(),
                    rest.max_inline_bytes,
                    ty.inline_payload_bytes(),
                )
            })
            .await?;
        let mut quote = quotation(ordered_merge::<Type<'db>>(len, capacity, 1))?;
        let (_, new_slots) = quotation(table_merge::<usize>(len, capacity, 1, 0))?;
        quote.work = quotation((|| {
            quote
                .work
                .checked_add(
                    new_slots
                        .checked_add(slots(capacity)?)?
                        .checked_add(1)?
                        .checked_mul(
                            maximum
                                .checked_add(incoming)?
                                .checked_add(size_of::<Type<'db>>().checked_mul(3)?)?
                                .checked_add(8)?,
                        )?,
                )?
                .checked_add(
                    len.checked_mul(new_slots.checked_add(1)?)?
                        .checked_mul(size_of::<(usize, Type<'db>)>().checked_add(8)?)?,
                )?
                .checked_add(quote.bytes.checked_mul(2)?)
        })())?;
        Layout::from_size_align(quote.bytes, align_of::<(usize, Type<'db>)>().max(16))
            .map_err(|_| RunError::Contract("enum rest layout overflow"))?;
        self.source
            .local(quote.work, quote.bytes, || {
                rest.max_inline_bytes = maximum.max(incoming);
                rest.values.insert(ty);
            })
            .await
    }

    async fn intern(
        &self,
        _class: EnumClassLiteral<'db>,
        _excluded: OrderedExclusions,
        _rest: OrderedRest<'db>,
    ) -> RunResult<EnumComplement<'db>> {
        self.source
            .unavailable(SourceOperation::EnumComplementIntern)
            .await
    }

    async fn complement_rest_empty(&self, complement: EnumComplement<'db>) -> RunResult<bool> {
        let rest = self
            .source
            .field(
                complement
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .rest(),
            )
            .await?;
        self.source.local(1, 0, || rest.is_empty()).await
    }

    async fn complement_class(
        &self,
        complement: EnumComplement<'db>,
    ) -> RunResult<EnumClassLiteral<'db>> {
        self.source
            .field(
                complement
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .enum_class_literal(),
            )
            .await
    }

    async fn member_count(&self, class: EnumClassLiteral<'db>) -> RunResult<usize> {
        let members = self
            .source
            .field(
                class
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .members(),
            )
            .await?;
        self.source.local(1, 0, || members.len()).await
    }

    async fn excluded_count(&self, complement: EnumComplement<'db>) -> RunResult<usize> {
        let excluded = self
            .source
            .field(
                complement
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .excluded_names(),
            )
            .await?;
        self.source.local(1, 0, || excluded.len()).await
    }
}
