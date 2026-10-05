use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::place::PlaceAndQualifiers;
use crate::types::member_lookup::normalization::{
    MemberNormalizationEffects, MemberNormalizationFacts, member_cycle_normalized_with,
};
use crate::types::{DescriptorOrigin, LookupParts, MemberLookupResult, ResolvedMember};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn member_lookup_parts(
        &self,
        result: MemberLookupResult<'db>,
    ) -> RunResult<LookupParts<'db>> {
        self.work(size_of::<LookupParts<'db>>() + 4).await?;
        let (member, error) = match result {
            Ok(member) => (member, None),
            Err(error) => {
                let fields = error.field_requests(self.access.endpoint().field_request_context());
                (
                    self.field(fields.fallback_member()).await?,
                    Some(self.field(fields.kind()).await?),
                )
            }
        };
        match member {
            ResolvedMember::Plain(member) => Ok(LookupParts {
                member,
                error,
                properties: None,
                descriptor: DescriptorOrigin::default(),
            }),
            ResolvedMember::WithMetadata(metadata) => {
                let fields =
                    metadata.field_requests(self.access.endpoint().field_request_context());
                Ok(LookupParts {
                    member: self.field(fields.member()).await?,
                    error,
                    properties: self.field(fields.properties()).await?,
                    descriptor: self.field(fields.descriptor()).await?,
                })
            }
        }
    }

    pub(in crate::types::infer) async fn normalize_member_cycle(
        &self,
        env: &ProgramEnvironment<'db>,
        current: MemberLookupResult<'db>,
        previous: MemberLookupResult<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> RunResult<MemberLookupResult<'db>> {
        member_cycle_normalized_with(
            current,
            env,
            previous,
            cycle,
            MemberNormalizationFacts,
            self,
        )
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MemberNormalizationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn parts(&self, result: MemberLookupResult<'db>) -> RunResult<LookupParts<'db>> {
        self.member_lookup_parts(result).await
    }

    async fn normalize_place(
        &self,
        env: &ProgramEnvironment<'db>,
        current: PlaceAndQualifiers<'db>,
        previous: PlaceAndQualifiers<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.normalize_place_cycle(env, current, previous, cycle)
            .await
    }

    async fn result(&self, parts: LookupParts<'db>) -> RunResult<MemberLookupResult<'db>> {
        self.access.member_result(parts).await
    }
}
