//! Admitted class and scope fields establish which index supplies the parent scope.

use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::ProgramFile;
use ty_python_core::scope::{FileScopeId, ScopeId};

use super::ClassCheckEffects;
use crate::Program;
use crate::types::StaticClassLiteral;
use crate::types::infer::builder::source_definition::controlled::{FixedFieldCopy, SourceAccess};
use crate::types::local_transfer::collections::{CALL_1, CALL_2, event_quote};
use crate::types::local_transfer::generated_field_quote;
use crate::types::local_transfer::scopes::scope_parent_quote;

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    pub(in crate::types::infer::builder::source_definition::controlled::class_checks) async fn generic_class_body_scope(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ScopeId<'db>> {
        let quote = generated_field_quote(
            |class: StaticClassLiteral<'db>, context| class.field_requests(context),
            |class: StaticClassLiteral<'db>, context| class.field_requests(context).body_scope(),
        );
        let endpoint = self.source.access.endpoint();
        let read = self
            .source
            .boxed_future_with_fixed_transfers(quote, || {
                let request = class
                    .field_requests(endpoint.field_request_context())
                    .body_scope();
                endpoint.read_field(request, &FixedFieldCopy)
            })
            .await?;
        Ok(read.await)
    }

    pub(in crate::types::infer::builder::source_definition::controlled::class_checks) async fn generic_scope_file(
        &self,
        scope: ScopeId<'db>,
    ) -> RunResult<ProgramFile<'db>> {
        let quote = generated_field_quote(
            |scope: ScopeId<'db>, context| scope.read_fields(context),
            |scope: ScopeId<'db>, context| scope.read_fields(context).program_file(),
        );
        let endpoint = self.source.access.endpoint();
        let read = self
            .source
            .boxed_future_with_fixed_transfers(quote, || {
                let request = scope
                    .read_fields(endpoint.field_request_context())
                    .program_file();
                endpoint.read_field(request, &FixedFieldCopy)
            })
            .await?;
        Ok(read.await)
    }

    pub(in crate::types::infer::builder::source_definition::controlled::class_checks) async fn check_generic_file_program(
        &self,
        file: ProgramFile<'db>,
    ) -> RunResult<()> {
        let quote = generated_field_quote(
            |file: ProgramFile<'db>, context| file.read_fields(context),
            |file: ProgramFile<'db>, context| file.read_fields(context).program(),
        );
        let endpoint = self.source.access.endpoint();
        let read = self
            .source
            .boxed_future_with_fixed_transfers(quote, || {
                let request = file.read_fields(endpoint.field_request_context()).program();
                endpoint.read_field(request, &FixedFieldCopy)
            })
            .await?;
        let program = read.await;
        // Both program checks compare generated interned handles. The quoted result also
        // retains the contract error on the rejected branch.
        let (work, bytes) = const {
            event_quote(
                7 * CALL_2 + 2 * CALL_1 + 26,
                &[
                    size_of::<(Program<'db>, Program<'db>)>(),
                    size_of::<RunResult<()>>(),
                ],
            )
        }
        .ok_or(RunError::Contract(
            "class program comparison quotation overflow",
        ))?;
        self.source
            .local_with_fixed_transfers(work, bytes, || self.source.check_program(program))
            .await?
    }

    pub(super) async fn generic_class_parent_scope(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<FileScopeId>> {
        self.source
            .boxed_future_with_fixed_transfers(
                Ok((10, size_of::<[(StaticClassLiteral<'db>, &Self); 2]>())),
                || self.check_generic_class_program(class),
            )
            .await?
            .await?;
        let scope = self
            .source
            .boxed_future_with_fixed_transfers(
                Ok((10, size_of::<[(StaticClassLiteral<'db>, &Self); 2]>())),
                || self.generic_class_body_scope(class),
            )
            .await?
            .await?;
        let file = self
            .source
            .boxed_future_with_fixed_transfers(
                Ok((10, size_of::<[(ScopeId<'db>, &Self); 2]>())),
                || self.generic_scope_file(scope),
            )
            .await?
            .await?;
        let (work, bytes) = const {
            event_quote(
                6 * CALL_2 + 4 * CALL_1 + 30,
                &[
                    size_of::<(ProgramFile<'db>, ProgramFile<'db>)>(),
                    size_of::<RunResult<()>>(),
                ],
            )
        }
        .ok_or(RunError::Contract(
            "class file comparison quotation overflow",
        ))?;
        self.source
            .local_with_fixed_transfers(work, bytes, || {
                if file != self.builder.program_file() {
                    Err(RunError::Contract(
                        "class validation index belongs to a different file",
                    ))
                } else {
                    Ok(())
                }
            })
            .await??;
        let quote = generated_field_quote(
            |scope: ScopeId<'db>, context| scope.read_fields(context),
            |scope: ScopeId<'db>, context| scope.read_fields(context).file_scope_id(),
        );
        let endpoint = self.source.access.endpoint();
        let read = self
            .source
            .boxed_future_with_fixed_transfers(quote, || {
                let request = scope
                    .read_fields(endpoint.field_request_context())
                    .file_scope_id();
                endpoint.read_field(request, &FixedFieldCopy)
            })
            .await?;
        let file_scope = read.await;
        let (work, bytes) = const { scope_parent_quote() }?;
        self.source
            .local_with_fixed_transfers(work, bytes, || {
                self.builder.index.scope(file_scope).parent()
            })
            .await
    }
}
