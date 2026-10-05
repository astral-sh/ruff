//! Complete class-default and type-variable-shadow reports with admitted source reads.

mod legacy_order;
mod spans;

use std::fmt;

use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticMessage, Span};
use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::TextRange;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;

use super::ClassCheckEffects;
use crate::lint::LintMetadata;
use crate::types::context::lint_reporting::LintReportMetadata;
use crate::types::diagnostic::class_generics::{
    ClassGenericReportEffects, DefaultReference, OrderTail, OrderVariables, ReportText, ShadowReport, create_diagnostic,
    default_annotation, enclosing_binding_span_with, invalid_default_reference_with, legacy_default_order_with,
    report_message_with, shadow_report_with,
};
use crate::types::function::FunctionType;
use crate::types::infer::builder::TypeInferenceBuilder;
use crate::types::infer::builder::source_definition::controlled::{SourceAccess, SourceEffects};
use crate::types::infer::builder::source_definition::controlled::lint_diagnostic_cost::{
    BufferQuotePreparation, annotation_with_ty_span_quote, buffer_quote_preparation,
    diagnostic_with_capacity_quote, prepared_vec_push_quote, string_push_str_quote,
    string_with_capacity_quote, unique_diagnostic_mutation_quote,
};
#[cfg(test)]
use crate::types::infer::source_runtime::tests::class_generic_validation::{
    self as observations, ReportBoundary, ValidationStage,
};
use crate::types::local_transfer::{boxed_future_with_fixed_transfers_at, local_with_fixed_transfers_at};
use crate::types::local_transfer::collections::{checked as checked_quote, event_quote};
use crate::types::local_transfer::names::borrowed_name_quote;
use crate::types::typevar::TypeVarInstance;
use crate::types::{BoundTypeVarInstance, ClassLiteral, StaticClassLiteral, Type, TypeVarKind};

/// Adapts shared class-generic reporting to admitted operations. It borrows the
/// `TypeInferenceBuilder`, keeping its pending inference results and diagnostics alive while
/// child definition and source-span queries run, before the parent query publishes its result.
struct ClassReports<'effects, 'builder, 'access, 'run, 'db: 'run, 'ast, A> {
    checks: &'effects ClassCheckEffects<'builder, 'access, 'run, 'db, 'ast, A>,
    #[cfg(test)]
    class: StaticClassLiteral<'db>,
    #[cfg(test)]
    stage: ValidationStage,
}

impl<A> fmt::Debug for ClassReports<'_, '_, '_, '_, '_, '_, A> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("ClassReports")
            .field("checks", &std::ptr::from_ref(self.checks))
            .finish_non_exhaustive()
    }
}

fn checked(value: Option<usize>) -> RunResult<usize> {
    value.ok_or(RunError::Contract("class generic report quotation overflow"))
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    /// Reports the complete nonempty list of legacy parameters missing defaults after the first default.
    pub(super) async fn report_legacy_default_order(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        first_default: TypeVarInstance<'db>,
        first_offender: TypeVarInstance<'db>,
        later_offenders: &[TypeVarInstance<'db>],
    ) -> RunResult<()> {
        let endpoint = self.source.access.endpoint();
        let reports = local_with_fixed_transfers_at(
            endpoint, 8, size_of::<[(&Self, StaticClassLiteral<'db>); 2]>(),
            || ClassReports {
                checks: self,
                #[cfg(test)] class,
                #[cfg(test)] stage: ValidationStage::LegacyDefaultOrder,
            },
        ).await?;
        let bytes = size_of::<[(StaticClassLiteral<'db>, &ast::StmtClassDef, TypeVarInstance<'db>, TypeVarInstance<'db>, &[TypeVarInstance<'db>], &ClassReports<'_, '_, '_, '_, '_, '_, A>); 2]>();
        // The factory funds fixed report branches and transfers; each cursor step and owned
        // diagnostic or message separately funds its work, storage, and retirement.
        reports.local(80, bytes, || {
            legacy_default_order_with(class, class_node, first_default, first_offender, later_offenders, &reports)
        }).await?.await
    }

    /// Reports an invalid default reference, including every available definition annotation.
    pub(super) async fn report_invalid_default_reference(
        &self, class: StaticClassLiteral<'db>, bad_default: TypeVarInstance<'db>,
        referenced: TypeVarInstance<'db>, is_later_in_list: bool,
    ) -> RunResult<()> {
        let reports = ClassReports {
            checks: self,
            #[cfg(test)] class,
            #[cfg(test)] stage: ValidationStage::DefaultReferences,
        };
        let bytes = size_of::<[(StaticClassLiteral<'db>, TypeVarInstance<'db>, TypeVarInstance<'db>, DefaultReference, &ClassReports<'_, '_, '_, '_, '_, '_, A>); 2]>();
        // The selected discriminant and report's finite branches surround separately admitted effects.
        boxed_future_with_fixed_transfers_at(self.source.access.endpoint(), Ok((39, bytes)), || {
            let reference = if is_later_in_list { DefaultReference::LaterParameter } else { DefaultReference::OutOfScope };
            invalid_default_reference_with(class, bad_default, referenced, reference, &reports)
        }).await?.await
    }

    /// Reports one class-owned or base variable shadowing a binding in an enclosing definition.
    pub(super) async fn report_typevar_shadow(
        &self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef,
        variable: BoundTypeVarInstance<'db>, other: BoundTypeVarInstance<'db>,
        #[cfg(test)] stage: ValidationStage,
    ) -> RunResult<()> {
        let reports = ClassReports {
            checks: self,
            #[cfg(test)] class,
            #[cfg(test)] stage,
        };
        let name = reports.bound_name(variable).await?;
        let range = reports.static_class_range(class).await?;
        let kind = reports.bound_kind(variable).await?;
        let report = reports.local(26, size_of::<[ShadowReport<'_, 'db>; 2]>(), || ShadowReport {
            typevar_name: name, owner_kind: "class", owner_name: &class_node.name.id,
            range, kind, other,
        }).await?;
        let bytes = size_of::<[(ShadowReport<'_, 'db>, &ClassReports<'_, '_, '_, '_, '_, '_, A>); 2]>();
        boxed_future_with_fixed_transfers_at(self.source.access.endpoint(), Ok((40, bytes)), || {
            shadow_report_with(report, &reports)
        }).await?.await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassReports<'_, '_, '_, 'run, 'db, '_, A> {
    /// Admits a finite action, leaving its captures alive until refused children have drained.
    async fn local<T>(&self, work: usize, bytes: usize, action: impl FnOnce() -> T) -> RunResult<T> {
        local_with_fixed_transfers_at(self.checks.source.access.endpoint(), work, bytes, action).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassGenericReportEffects<'db>
    for ClassReports<'_, '_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<&'db [Type<'db>]> {
        self.legacy_explicit_bases(class).await
    }

    async fn next_legacy_base(&self, bases: &[Type<'db>], cursor: &mut usize) -> RunResult<Option<(usize, Type<'db>)>> {
        self.legacy_base_step(bases, cursor).await
    }

    async fn legacy_base_range(&self, node: &ast::StmtClassDef, bases: &[Type<'db>], index: Option<usize>) -> RunResult<TextRange> {
        self.legacy_range(node, bases, index).await
    }

    async fn order_primary(&self, first: TypeVarInstance<'db>, remaining: &[TypeVarInstance<'db>]) -> RunResult<DiagnosticMessage> {
        self.legacy_primary(first, remaining).await
    }

    async fn order_tail<'a>(&self, first: TypeVarInstance<'db>, remaining: &'a [TypeVarInstance<'db>]) -> RunResult<OrderTail<'a, 'db>> {
        self.legacy_tail(first, remaining).await
    }

    async fn order_names(&self, variables: &OrderVariables<'_, 'db>) -> RunResult<Vec<&'db Name>> {
        self.legacy_name_buffer(variables).await
    }

    async fn next_order_variable(&self, variables: &mut OrderVariables<'_, 'db>) -> RunResult<Option<TypeVarInstance<'db>>> {
        self.legacy_variable_step(variables).await
    }

    async fn retain_order_name(&self, names: &mut Vec<&'db Name>, name: &'db Name) -> RunResult<()> {
        self.legacy_retain_name(names, name).await
    }

    async fn class_range(&self, class: StaticClassLiteral<'db>) -> RunResult<TextRange> {
        self.static_class_range(class).await
    }

    async fn begin(&self, lint: &'static LintMetadata, range: TextRange) -> RunResult<Option<LintReportMetadata>> {
        #[cfg(test)]
        observations::report_boundary(self.checks.builder.context.file(), self.class, self.stage, ReportBoundary::Eligibility);
        let bytes = size_of::<[(&SourceEffects<'_, 'run, 'db, A>, &TypeInferenceBuilder<'db, '_>, &LintMetadata, TextRange); 2]>();
        boxed_future_with_fixed_transfers_at(self.checks.source.access.endpoint(), Ok((16, bytes)), || {
            self.checks.source.begin_lint_report(self.checks.builder, lint, range)
        }).await?.await
    }

    async fn name(&self, variable: TypeVarInstance<'db>) -> RunResult<&'db Name> {
        self.variable_name(variable).await
    }

    async fn next_annotation(&self, bad: TypeVarInstance<'db>, referenced: TypeVarInstance<'db>, reference: DefaultReference, cursor: &mut usize) -> RunResult<Option<TypeVarInstance<'db>>> {
        let bytes = size_of::<[(TypeVarInstance<'db>, TypeVarInstance<'db>, DefaultReference, usize); 2]>() + size_of::<[Option<TypeVarInstance<'db>>; 2]>();
        self.local(24, bytes, || {
            let result = default_annotation(bad, referenced, reference, *cursor);
            *cursor += usize::from(result.is_some());
            result
        }).await
    }

    async fn definition_span(&self, variable: TypeVarInstance<'db>) -> RunResult<Option<Span>> {
        self.variable_definition_span(variable).await
    }

    async fn binding_definition(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<Option<Definition<'db>>> {
        self.variable_binding_definition(variable).await
    }

    async fn binding_type(&self, definition: Definition<'db>) -> RunResult<Type<'db>> {
        self.canonical_binding_type(definition).await
    }

    async fn class_span(&self, class: ClassLiteral<'db>) -> RunResult<Span> {
        self.class_header_span(class).await
    }

    async fn function_span(&self, function: FunctionType<'db>) -> RunResult<Span> {
        self.function_signature_span(function).await
    }

    async fn enclosing_span(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<Option<Span>> {
        let bytes = size_of::<[(BoundTypeVarInstance<'db>, &Self); 2]>();
        boxed_future_with_fixed_transfers_at(self.checks.source.access.endpoint(), Ok((24, bytes)), || {
            enclosing_binding_span_with(variable, self)
        }).await?.await
    }

    async fn bound_kind(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<TypeVarKind> {
        self.variable_bound_kind(variable).await
    }

    async fn message(&self, text: ReportText<'_>) -> RunResult<DiagnosticMessage> {
        let bytes = size_of::<[(ReportText<'_>, &Self); 2]>();
        boxed_future_with_fixed_transfers_at(self.checks.source.access.endpoint(), Ok((20, bytes)), || {
            report_message_with(text, self)
        }).await?.await
    }

    async fn create(&self, metadata: &LintReportMetadata, headline: DiagnosticMessage, primary: Option<DiagnosticMessage>, annotation_capacity: usize) -> RunResult<Diagnostic> {
        #[cfg(test)]
        observations::report_boundary(self.checks.builder.context.file(), self.class, self.stage, ReportBoundary::Construction);
        const WRAPPER: usize = 3 * 5 + 3 * 8 + 22;
        const BYTES: usize = WRAPPER * size_of::<(&LintReportMetadata, DiagnosticMessage, Option<DiagnosticMessage>, usize)>();
        const PRIMARY: RunResult<(usize, usize)> = match (unique_diagnostic_mutation_quote(), annotation_with_ty_span_quote(), prepared_vec_push_quote::<Annotation>()) {
            (Ok((a, ab)), Ok((b, bb)), Ok((c, cb))) => Ok((a + b + c, ab + bb + cb)),
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => Err(error),
        };
        let (work, bytes) = const {
            match buffer_quote_preparation(BufferQuotePreparation::DiagnosticWithCapacity) {
                Ok((work, bytes)) => Ok((work + WRAPPER, bytes + BYTES)),
                Err(error) => Err(error),
            }
        }?;
        let (work, bytes) = self.local(work, bytes, || -> RunResult<(usize, usize)> {
            let (work, bytes) = diagnostic_with_capacity_quote(annotation_capacity, usize::from(metadata.verbose))?;
            let primary = PRIMARY?;
            Ok((checked(work.checked_add(WRAPPER).and_then(|w| w.checked_add(primary.0)))?, checked(bytes.checked_add(BYTES).and_then(|b| b.checked_add(primary.1)))?))
        }).await??;
        self.local(work, bytes, || create_diagnostic(metadata, headline, primary, annotation_capacity)).await
    }

    async fn concise(&self, diagnostic: &mut Diagnostic, message: DiagnosticMessage) -> RunResult<()> {
        let (work, bytes) = const {
            match unique_diagnostic_mutation_quote() {
                Ok((work, bytes)) => Ok((work + 6, bytes + size_of::<[(&mut Diagnostic, DiagnosticMessage); 2]>())),
                Err(error) => Err(error),
            }
        }?;
        self.local(work, bytes, || diagnostic.set_concise_message(message)).await
    }

    async fn annotate(&self, diagnostic: &mut Diagnostic, span: Span, message: DiagnosticMessage) -> RunResult<()> {
        let (work, bytes) = const {
            match (unique_diagnostic_mutation_quote(), annotation_with_ty_span_quote(), prepared_vec_push_quote::<Annotation>()) {
                (Ok((a, ab)), Ok((b, bb)), Ok((c, cb))) => Ok((a + b + c + 30, ab + bb + cb + 30 * size_of::<(Annotation, DiagnosticMessage)>())),
                (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => Err(error),
            }
        }?;
        self.local(work, bytes, || diagnostic.annotate(Annotation::secondary(span).message(message))).await
    }

    async fn additional_primary(&self, metadata: &LintReportMetadata, diagnostic: &mut Diagnostic, message: DiagnosticMessage) -> RunResult<()> {
        self.legacy_additional_primary(metadata, diagnostic, message).await
    }

    async fn finish(&self, metadata: LintReportMetadata, diagnostic: Diagnostic) -> RunResult<()> {
        #[cfg(test)]
        observations::report_boundary(self.checks.builder.context.file(), self.class, self.stage, ReportBoundary::Insertion);
        let bytes = size_of::<[(&SourceEffects<'_, 'run, 'db, A>, &TypeInferenceBuilder<'db, '_>, LintReportMetadata, Diagnostic); 2]>();
        boxed_future_with_fixed_transfers_at(self.checks.source.access.endpoint(), Ok((16, bytes)), || {
            self.checks.source.finish_lint_report(self.checks.builder, metadata, diagnostic)
        }).await?.await?;
        #[cfg(test)]
        observations::diagnostic_inserted(self.checks.builder.context.file(), self.class, self.stage, &self.checks.builder.context.retained_diagnostics());
        Ok(())
    }

    async fn next_part<'a>(&self, text: ReportText<'a>, cursor: &mut usize) -> RunResult<Option<&'a str>> {
        let (work, bytes) = const {
            match (borrowed_name_quote(), checked_quote(event_quote(legacy_order::MESSAGE_PART_WORK, &[
                size_of::<ReportText<'_>>(), size_of::<(&[&Name], usize)>(),
                size_of::<(&mut usize, Option<&str>)>(), size_of::<Option<usize>>(),
            ]))) {
                (Ok((work, bytes)), Ok((part_work, part_bytes))) => Ok((work + part_work, bytes + part_bytes)),
                (Err(error), _) | (_, Err(error)) => Err(error),
            }
        }?;
        self.local(work, bytes, || {
            let part = text.part(*cursor);
            *cursor += usize::from(part.is_some());
            part
        }).await
    }

    async fn add_length(&self, length: &mut usize, part: &str) -> RunResult<()> {
        self.local(6 * 5 + 8 + 12, size_of::<[(&mut usize, &str, Option<usize>); 4]>(), || -> RunResult<()> {
            *length = checked(length.checked_add(part.len()))?;
            Ok(())
        }).await?
    }

    async fn buffer(&self, length: usize) -> RunResult<String> {
        let (work, bytes) = const { buffer_quote_preparation(BufferQuotePreparation::StringWithCapacity) }?;
        let (work, bytes) = self.local(work, bytes, || string_with_capacity_quote(length)).await??;
        self.local(work, bytes, || String::with_capacity(length)).await
    }

    async fn append(&self, buffer: &mut String, part: &str) -> RunResult<()> {
        let (work, bytes) = const {
            match buffer_quote_preparation(BufferQuotePreparation::StringPushStr) {
                Ok((work, bytes)) => Ok((work + 6 * 5 + 3, bytes + (6 * 5 + 3) * size_of::<&str>())),
                Err(error) => Err(error),
            }
        }?;
        let (work, bytes) = self.local(work, bytes, || string_push_str_quote(part.len())).await??;
        self.local(work, bytes, || buffer.push_str(part)).await
    }

    async fn finish_message(&self, buffer: String) -> RunResult<DiagnosticMessage> {
        let bytes = size_of::<[(&SourceEffects<'_, 'run, 'db, A>, String); 2]>();
        boxed_future_with_fixed_transfers_at(self.checks.source.access.endpoint(), Ok((9, bytes)), || {
            self.checks.source.finish_lint_message(buffer)
        }).await?.await
    }
}
