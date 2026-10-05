//! Admitted parameter scans and owned diagnostic construction for PEP 695 classes.

use std::fmt;

use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticMessage};
use ruff_python_ast as ast;
#[cfg(test)]
use ruff_text_size::{Ranged, TextRange};
use salsa::execution_probe::{RunError, RunResult};

use super::ClassCheckEffects;
use crate::types::context::lint_reporting::LintReportMetadata;
use crate::types::infer::builder::post_inference::type_param_validation::{
    DefaultStep, ParameterAnnotation, ParameterMessage, ParameterReport, ParameterScan,
    ParameterValidationEffects, ParameterValidationFacts, SinglePackStep, TypeParameterOwner,
    annotate_parameter, check_defaults_after_pack_with, check_single_pack_with,
    create_parameter_diagnostic, parameter_message_with, parameter_report_with,
};
use crate::types::infer::builder::TypeInferenceBuilder;
use crate::types::infer::builder::source_definition::controlled::{SourceAccess, SourceEffects};
use crate::types::infer::builder::source_definition::controlled::storage::StorageQuote;
use crate::types::infer::builder::source_definition::controlled::lint_diagnostic_cost::{
    BufferQuotePreparation, annotation_with_ty_span_quote, buffer_quote_preparation,
    diagnostic_info_quote, diagnostic_with_capacity_quote, empty_vec_quote, prepared_vec_push_quote,
    string_push_str_quote, string_with_capacity_quote, unique_diagnostic_mutation_quote,
    vec_reserve_exact_quote,
};
#[cfg(test)]
use crate::types::infer::source_runtime::tests::pep695_parameter_validation::{
    self as observations, ReportBoundary, ValidationStage,
};
use crate::types::local_transfer::{
    boxed_future_with_fixed_transfers_at, local_with_fixed_transfers_at,
};

/// Keeps the source provider and unpublished builder borrowed throughout a scan and its report's lifetime.
struct ParameterChecks<'effects, 'builder, 'access, 'run, 'db: 'run, 'ast, A> {
    checks: &'effects ClassCheckEffects<'builder, 'access, 'run, 'db, 'ast, A>,
    #[cfg(test)]
    class_range: TextRange,
    #[cfg(test)]
    stage: ValidationStage,
}

impl<A> fmt::Debug for ParameterChecks<'_, '_, '_, '_, '_, '_, A> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("ParameterChecks")
            .field("checks", &std::ptr::from_ref(self.checks))
            .finish_non_exhaustive()
    }
}

fn checked(value: Option<usize>) -> RunResult<usize> {
    value.ok_or(RunError::Contract("parameter validation quotation overflow"))
}

/// Storage admission and the matching reservation for one retained default-bearing parameter.
#[derive(Debug)]
struct DefaultAppend {
    quote: StorageQuote,
    reserve: usize,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    /// Runs both parameter checks in order; a disabled duplicate-pack lint never skips the defaults check.
    pub(super) async fn check_parameter_lists(&self, class_node: &ast::StmtClassDef, params: &ast::TypeParams) -> RunResult<()> {
        let single = ParameterChecks {
            checks: self,
            #[cfg(test)] class_range: class_node.range(),
            #[cfg(test)] stage: ValidationStage::SinglePack,
        };
        // Each wrapper admits its evaluated arguments and callee bindings. The shared boxed
        // helper separately charges future storage, fixed forwarding and result carriers.
        let bytes = size_of::<[(&ast::TypeParams, TypeParameterOwner<'_>, &ParameterChecks<'_, '_, '_, '_, '_, '_, A>); 2]>();
        boxed_future_with_fixed_transfers_at(self.source.access.endpoint(), Ok((14, bytes)), || {
            check_single_pack_with(params, TypeParameterOwner::GenericClass(&class_node.name.id), &single)
        }).await?.await?;
        #[cfg(test)]
        observations::validation_completed(self.builder.db(), self.builder.context.file(), class_node.range(), ValidationStage::SinglePack, &self.builder.context.retained_diagnostics());

        let defaults = ParameterChecks {
            checks: self,
            #[cfg(test)] class_range: class_node.range(),
            #[cfg(test)] stage: ValidationStage::DefaultsAfterPack,
        };
        let bytes = size_of::<[(&ast::TypeParams, ParameterValidationFacts, &ParameterChecks<'_, '_, '_, '_, '_, '_, A>); 2]>();
        boxed_future_with_fixed_transfers_at(self.source.access.endpoint(), Ok((17, bytes)), || {
            check_defaults_after_pack_with(params, ParameterValidationFacts, &defaults)
        }).await?.await?;
        #[cfg(test)]
        observations::validation_completed(self.builder.db(), self.builder.context.file(), class_node.range(), ValidationStage::DefaultsAfterPack, &self.builder.context.retained_diagnostics());
        Ok(())
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ParameterChecks<'_, '_, '_, 'run, 'db, '_, A> {
    /// Admits a finite action and its callback/result carriers while retaining captured owners on refusal.
    async fn local<T>(&self, work: usize, bytes: usize, action: impl FnOnce() -> T) -> RunResult<T> {
        local_with_fixed_transfers_at(self.checks.source.access.endpoint(), work, bytes, action).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ParameterValidationEffects
    for ParameterChecks<'_, '_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn scan<'a>(&self, params: &'a ast::TypeParams) -> RunResult<ParameterScan<'a>> {
        // Four field initializations, the return and five passive owner-retirement steps
        // accompany the separately quoted empty Vec header and its eventual disposal.
        let (work, bytes) = const {
            match empty_vec_quote::<&ast::TypeParam>() {
                Ok((work, bytes)) => match (
                    work.checked_add(10),
                    bytes.checked_add(size_of::<(&ast::TypeParams, usize, Option<&ast::TypeParamTypeVarTuple>, Vec<&ast::TypeParam>)>()),
                ) {
                    (Some(work), Some(bytes)) => Ok((work, bytes)),
                    _ => Err(RunError::Contract("parameter scan quotation overflow")),
                },
                Err(error) => Err(error),
            }
        }?;
        self.local(work, bytes, || ParameterScan::new(params)).await
    }

    async fn next_single<'a>(&self, scan: &mut ParameterScan<'a>) -> RunResult<Option<SinglePackStep<'a>>> {
        // Index/read/advance (6), kind/first-pack selection (5), retained/result writes (4),
        // and the shared loop's match/branch/back-edge handling (5).
        let bytes = size_of::<[(&mut ParameterScan<'_>, Option<&ast::TypeParam>, Option<&ast::TypeParamTypeVarTuple>); 2]>();
        self.local(20, bytes, || scan.next_single()).await
    }

    async fn next_default<'a>(&self, scan: &mut ParameterScan<'a>) -> RunResult<Option<DefaultStep<'a>>> {
        // Index/read/advance (6), first/default/kind selection (7), retained/result writes (4),
        // and the shared loop's match/append selection/back edge (6).
        let bytes = size_of::<[(&mut ParameterScan<'_>, Option<&ast::TypeParam>, Option<&ast::Expr>); 2]>();
        self.local(23, bytes, || scan.next_default()).await
    }

    async fn retain_default<'a>(&self, scan: &mut ParameterScan<'a>, param: &'a ast::TypeParam) -> RunResult<()> {
        // Length/capacity access, checked doubling and the two max selections are paid before
        // choosing a geometric reservation.
        const METADATA_WORK: usize = 6 * 5 + 6 * 8 + 34;
        const METADATA_BYTES: usize = METADATA_WORK * size_of::<RunResult<DefaultAppend>>();
        let (work, bytes) = const {
            match (
                buffer_quote_preparation(BufferQuotePreparation::VecReserveExact),
                buffer_quote_preparation(BufferQuotePreparation::PreparedVecPush),
            ) {
                (Ok((reserve_work, reserve_bytes)), Ok((push_work, push_bytes))) => {
                    match (reserve_work.checked_add(push_work), reserve_bytes.checked_add(push_bytes)) {
                        (Some(work), Some(bytes)) => match (work.checked_add(METADATA_WORK), bytes.checked_add(METADATA_BYTES)) {
                            (Some(work), Some(bytes)) => Ok((work, bytes)),
                            _ => Err(RunError::Contract("parameter append preparation overflow")),
                        },
                        _ => Err(RunError::Contract("parameter append preparation overflow")),
                    }
                }
                (Err(error), _) | (_, Err(error)) => Err(error),
            }
        }?;
        let admission = self.local(work, bytes, || -> RunResult<DefaultAppend> {
            let len = scan.defaults.len();
            let capacity = scan.defaults.capacity();
            let required = checked(len.checked_add(1))?;
            let target = if required > capacity { checked(capacity.checked_mul(2))?.max(required).max(4) } else { capacity };
            let reserve = if required > capacity { target - len } else { 0 };
            let (reserve_work, reserve_bytes) = vec_reserve_exact_quote::<&ast::TypeParam>(len, capacity, reserve)?;
            let (push_work, push_bytes) = prepared_vec_push_quote::<&ast::TypeParam>()?;
            let work = checked(reserve_work.checked_add(push_work).and_then(|n| n.checked_add(1)))?;
            let bytes = checked(reserve_bytes.checked_add(push_bytes))?;
            Ok(DefaultAppend { quote: StorageQuote { work, bytes }, reserve })
        }).await??;
        // Reservation pays requested backing and possible capacity relocation. Each appended
        // reference also prepays its passive retirement, whether or not the buffer grows.
        self.local(admission.quote.work, admission.quote.bytes, || { scan.defaults.reserve_exact(admission.reserve); scan.defaults.push(param); }).await
    }

    async fn report(&self, report: ParameterReport<'_>) -> RunResult<()> {
        let bytes = size_of::<[(ParameterReport<'_>, ParameterValidationFacts, &Self); 2]>();
        // Eleven call/argument operations and twenty finite report decisions/initializations.
        boxed_future_with_fixed_transfers_at(self.checks.source.access.endpoint(), Ok((31, bytes)), || {
            parameter_report_with(report, ParameterValidationFacts, self)
        }).await?.await
    }

    async fn begin(&self, report: ParameterReport<'_>) -> RunResult<Option<LintReportMetadata>> {
        #[cfg(test)]
        observations::report_boundary(self.checks.builder.db(), self.checks.builder.context.file(), self.class_range, self.stage, ReportBoundary::Eligibility);
        let (lint, range) = self.local(8, 0, || (report.lint(), report.range())).await?;
        let bytes = size_of::<[(&SourceEffects<'_, 'run, 'db, A>, &TypeInferenceBuilder<'db, '_>, &crate::lint::LintMetadata, ruff_text_size::TextRange); 2]>();
        boxed_future_with_fixed_transfers_at(self.checks.source.access.endpoint(), Ok((16, bytes)), || {
            self.checks.source.begin_lint_report(self.checks.builder, lint, range)
        }).await?.await
    }

    async fn message(&self, message: ParameterMessage<'_>) -> RunResult<DiagnosticMessage> {
        let bytes = size_of::<[(ParameterMessage<'_>, &Self); 2]>();
        boxed_future_with_fixed_transfers_at(self.checks.source.access.endpoint(), Ok((12, bytes)), || {
            parameter_message_with(message, self)
        }).await?.await
    }

    async fn create(&self, metadata: &LintReportMetadata, report: ParameterReport<'_>, headline: DiagnosticMessage) -> RunResult<Diagnostic> {
        #[cfg(test)]
        observations::report_boundary(self.checks.builder.db(), self.checks.builder.context.file(), self.class_range, self.stage, ReportBoundary::Construction);
        // These operations select the two capacities and account for the shared constructor's
        // argument getters. Layout evaluation happens only after this preparation is admitted.
        const METADATA_WORK: usize = 3 * 5 + 2 * 8 + 16;
        const METADATA_BYTES: usize = METADATA_WORK * size_of::<RunResult<StorageQuote>>();
        let (work, bytes) = const {
            match buffer_quote_preparation(BufferQuotePreparation::DiagnosticWithCapacity) {
                Ok((work, bytes)) => match (work.checked_add(METADATA_WORK), bytes.checked_add(METADATA_BYTES)) {
                    (Some(work), Some(bytes)) => Ok((work, bytes)),
                    _ => Err(RunError::Contract("parameter diagnostic preparation overflow")),
                },
                Err(error) => Err(error),
            }
        }?;
        let quote = self.local(work, bytes, || -> RunResult<StorageQuote> {
            let annotations = report.annotation_count();
            let subdiagnostics = 1 + usize::from(metadata.verbose);
            let (work, bytes) = diagnostic_with_capacity_quote(annotations, subdiagnostics)?;
            // The shared wrapper reads the lint id/severity and capacities again before the
            // prepared constructor. Its owned input message already has prepaid retirement.
            let work = checked(work.checked_add(METADATA_WORK))?;
            let bytes = checked(bytes.checked_add(METADATA_BYTES))?;
            Ok(StorageQuote { work, bytes })
        }).await??;
        self.local(quote.work, quote.bytes, || create_parameter_diagnostic(metadata, report, headline)).await
    }

    async fn concise(&self, diagnostic: &mut Diagnostic, message: DiagnosticMessage) -> RunResult<()> {
        let (work, bytes) = self.local(32, 32 * size_of::<RunResult<(usize, usize)>>(), || -> RunResult<(usize, usize)> {
            let (work, bytes) = unique_diagnostic_mutation_quote()?;
            Ok((checked(work.checked_add(6))?, checked(bytes.checked_add(size_of::<DiagnosticMessage>()))?))
        }).await??;
        // The unique Arc path is shared; the added field replacement moves an already owned
        // message into the empty concise-message slot and preserves its disposal funding.
        self.local(work, bytes, || diagnostic.set_concise_message(message)).await
    }

    async fn next_annotation<'a>(&self, report: ParameterReport<'a>, cursor: &mut usize) -> RunResult<Option<ParameterAnnotation<'a>>> {
        let bytes = size_of::<[Option<&ast::TypeParam>; 2]>() + size_of::<[usize; 4]>();
        self.local(22, bytes, || { let annotation = report.annotation(*cursor); *cursor += usize::from(annotation.is_some()); annotation }).await
    }

    async fn annotate(&self, metadata: &LintReportMetadata, diagnostic: &mut Diagnostic, annotation: ParameterAnnotation<'_>, message: DiagnosticMessage) -> RunResult<()> {
        // The shared wrapper chooses the role and forwards the completed annotation to
        // Diagnostic::annotate. Metadata constructed from a ty File preserves the span invariant.
        const WRAPPER_WORK: usize = 14 + 8 + 8;
        const WRAPPER_BYTES: usize = WRAPPER_WORK * size_of::<Annotation>();
        let (work, bytes) = self.local(96, 96 * size_of::<RunResult<(usize, usize)>>(), || -> RunResult<(usize, usize)> {
            let (work, bytes) = unique_diagnostic_mutation_quote()?;
            let (annotation_work, annotation_bytes) = annotation_with_ty_span_quote()?;
            let (push_work, push_bytes) = prepared_vec_push_quote::<Annotation>()?;
            let work = checked(work.checked_add(annotation_work).and_then(|n| n.checked_add(push_work)).and_then(|n| n.checked_add(WRAPPER_WORK)))?;
            let bytes = checked(bytes.checked_add(annotation_bytes).and_then(|n| n.checked_add(push_bytes)).and_then(|n| n.checked_add(WRAPPER_BYTES)))?;
            Ok((work, bytes))
        }).await??;
        self.local(work, bytes, || annotate_parameter(metadata, diagnostic, annotation, message)).await
    }

    async fn info(&self, diagnostic: &mut Diagnostic, message: DiagnosticMessage) -> RunResult<()> {
        let (work, bytes) = self.local(16, 16 * size_of::<RunResult<(usize, usize)>>(), diagnostic_info_quote).await??;
        self.local(work, bytes, || diagnostic.info(message)).await
    }

    async fn finish(&self, metadata: LintReportMetadata, diagnostic: Diagnostic) -> RunResult<()> {
        #[cfg(test)]
        observations::report_boundary(self.checks.builder.db(), self.checks.builder.context.file(), self.class_range, self.stage, ReportBoundary::Insertion);
        let bytes = size_of::<[(&SourceEffects<'_, 'run, 'db, A>, &TypeInferenceBuilder<'db, '_>, LintReportMetadata, Diagnostic); 2]>();
        boxed_future_with_fixed_transfers_at(self.checks.source.access.endpoint(), Ok((16, bytes)), || {
            self.checks.source.finish_lint_report(self.checks.builder, metadata, diagnostic)
        }).await?.await?;
        #[cfg(test)]
        observations::diagnostic_inserted(self.checks.builder.db(), self.checks.builder.context.file(), self.class_range, self.stage, &self.checks.builder.context.retained_diagnostics());
        Ok(())
    }

    async fn next_part<'a>(&self, message: ParameterMessage<'a>, cursor: &mut usize) -> RunResult<Option<&'a str>> {
        // The longest descriptor branch selects one indexed name with fixed arithmetic and reads.
        let bytes = size_of::<[Option<&ast::TypeParam>; 2]>() + size_of::<[usize; 6]>();
        self.local(32, bytes, || { let part = message.part(*cursor); *cursor += usize::from(part.is_some()); part }).await
    }

    async fn add_length(&self, length: &mut usize, part: &str) -> RunResult<()> {
        self.local(4, 0, || -> RunResult<()> { *length = checked(length.checked_add(part.len()))?; Ok(()) }).await?
    }

    async fn buffer(&self, length: usize) -> RunResult<String> {
        let (work, bytes) = const { buffer_quote_preparation(BufferQuotePreparation::StringWithCapacity) }?;
        let (work, bytes) = self.local(work, bytes, || string_with_capacity_quote(length)).await??;
        self.local(work, bytes, || String::with_capacity(length)).await
    }

    async fn append(&self, buffer: &mut String, part: &str) -> RunResult<()> {
        // str::len follows its byte-slice length; its fixed wrappers are part of preparation.
        let (work, bytes) = const {
            match buffer_quote_preparation(BufferQuotePreparation::StringPushStr) {
                Ok((work, bytes)) => match (work.checked_add(6 * 5 + 3), bytes.checked_add((6 * 5 + 3) * size_of::<&str>())) {
                    (Some(work), Some(bytes)) => Ok((work, bytes)),
                    _ => Err(RunError::Contract("parameter text preparation overflow")),
                },
                Err(error) => Err(error),
            }
        }?;
        let (work, bytes) = self.local(work, bytes, || string_push_str_quote(part.len())).await??;
        self.local(work, bytes, || buffer.push_str(part)).await
    }

    async fn finish_message(&self, buffer: String) -> RunResult<DiagnosticMessage> {
        let bytes = size_of::<[(&SourceEffects<'_, 'run, 'db, A>, String); 2]>();
        // The two-pass message builder fills its requested capacity exactly. Reuse the
        // admitted String/Vec/boxed-slice conversion and its concrete carrier inventory.
        boxed_future_with_fixed_transfers_at(self.checks.source.access.endpoint(), Ok((9, bytes)), || {
            self.checks.source.finish_lint_message(buffer)
        }).await?.await
    }
}
