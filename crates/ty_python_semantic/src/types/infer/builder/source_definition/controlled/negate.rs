//! Controlled type negation retains the ordinary atomic interning decisions.

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::negation::{NegationEffects, NegationFacts, negate_with};
use crate::types::{NegativeIntersectionElements, RecursiveType, Type};
use crate::{FxOrderSet, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn negate_type(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        negate_with(ty, NegationFacts, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NegationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn unbound_recursive(&self) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn unfold(&self, _recursive: RecursiveType<'db>) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::RecursiveTypeUnfold).await
    }

    async fn recurse(&self, _ty: Type<'db>) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn single_negative(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        let (positive, negative) = self
            .local(2, 0, || {
                (
                    FxOrderSet::default(),
                    NegativeIntersectionElements::Single(ty),
                )
            })
            .await?;
        Ok(Type::Intersection(
            self.access.intern_intersection(positive, negative).await?,
        ))
    }

    async fn normalize(&self, _ty: Type<'db>) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::Narrowing).await
    }
}
