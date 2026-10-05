use salsa::execution_probe::RunResult;

use super::{SourceAccess, SourceEffects};
use crate::types::context::ProgramEnvironmentSource;
use crate::{Program, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn environment_program(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<Program<'db>> {
        // Admit the source read, eventual program check, and cache update together. The attempt
        // keeps this charge across child awaits; the deferred work only compares and stores handles.
        // The cache update creates the program ID and source variant, replaces the cell, and
        // retires its previous value. The extra bytes cover the Cell::set argument, its transfer
        // into the cell, and the old cell value, each bounded by the environment wrapper's size.
        let source = self.local_with_fixed_transfers(
            10,
            size_of::<ProgramEnvironment<'db>>() * 3,
            || env.source(),
        ).await?;
        let file = match source {
            ProgramEnvironmentSource::Program(program) => {
                self.check_program(program)?;
                return Ok(program);
            }
            ProgramEnvironmentSource::File(file) => file,
            ProgramEnvironmentSource::Definition(definition) => {
                self.definition_file(definition).await?
            }
            ProgramEnvironmentSource::Scope(scope) => self.scope_file(scope).await?,
        };
        let request = self.local_with_fixed_transfers(4, 0, || {
            let fields = self.access.endpoint().field_request_context();
            file.read_fields(fields).program()
        }).await?;
        let program = self.field(request).await?;
        self.check_program(program)?;
        env.cache_program(program);
        Ok(program)
    }
}
