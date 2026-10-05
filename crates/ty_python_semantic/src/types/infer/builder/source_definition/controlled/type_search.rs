//! Source type searches retain the shared walk's admission and unavailable-operation boundaries.

use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::Type;
use crate::types::constraints::control::TddError;
use crate::types::visitor::SearchOperation;
use crate::types::visitor::runtime::{TypeSearchUnavailable, has_dynamic_with, has_typevar_with};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn has_typevar_source(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        self.environment_program(env).await?;
        has_typevar_with(self.db(), self.access.endpoint(), ty, self).await
    }

    pub(in crate::types::infer) async fn has_dynamic_source(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        self.environment_program(env).await?;
        has_dynamic_with(self.db(), self.access.endpoint(), ty, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeSearchUnavailable
    for &SourceEffects<'_, 'run, 'db, A>
{
    async fn unavailable<T>(
        &self,
        _db: &dyn Db,
        _endpoint: &TaskEndpoint<'_, '_>,
        operation: SearchOperation,
    ) -> RunResult<T> {
        self.work(1).await?;
        SourceEffects::unavailable(self, SourceOperation::TypeSearch(operation)).await
    }

    fn error(&self, _db: &dyn Db, error: TddError<RunError>) -> RunError {
        match error {
            TddError::Refused(error) => error,
            TddError::CapacityExhausted => {
                RunError::Contract("source type-search capacity exhausted")
            }
        }
    }
}
