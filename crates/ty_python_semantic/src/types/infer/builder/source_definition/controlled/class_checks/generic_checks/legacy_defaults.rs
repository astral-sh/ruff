//! Admitted legacy-default ordering retains offenders until the complete report is published.

use std::fmt;

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};

use super::ClassCheckEffects;
use crate::types::generics::context_construction::ContextVariables;
use crate::types::infer::builder::post_inference::static_class::generic_checks::default_references::{
    ClassDefaultReferenceEffects, VariableCursor, VariableRange,
};
use crate::types::infer::builder::post_inference::static_class::generic_checks::legacy_defaults::{
    LegacyDefaultOrderEffects, LegacyDefaultReport, LegacyDefaultScan, check_legacy_default_order_with,
};
use crate::types::infer::builder::source_definition::controlled::SourceAccess;
use crate::types::infer::builder::source_definition::controlled::lint_diagnostic_cost::{
    BufferQuotePreparation, buffer_quote_preparation, empty_vec_quote, prepared_vec_push_quote,
    vec_reserve_exact_quote,
};
use crate::types::infer::builder::source_definition::controlled::storage::StorageQuote;
#[cfg(test)]
use crate::types::infer::source_runtime::tests::class_generic_validation::{
    self as observations, ReportBoundary, ValidationStage,
};
use crate::types::local_transfer::collections::{
    CALL_1, CALL_2, CALL_3, CALL_4, VECTOR_CAPACITY, VECTOR_LENGTH, event_quote,
};
use crate::types::typevar::TypeVarInstance;
use crate::types::{BoundTypeVarInstance, GenericContext, StaticClassLiteral, Type};

/// Keeps the class's unpublished builder borrowed while defaults and reports suspend.
struct LegacyDefaultChecks<'effects, 'builder, 'access, 'run, 'db: 'run, 'ast, A> {
    checks: &'effects ClassCheckEffects<'builder, 'access, 'run, 'db, 'ast, A>,
    #[cfg(test)]
    class: StaticClassLiteral<'db>,
}

impl<A> fmt::Debug for LegacyDefaultChecks<'_, '_, '_, '_, '_, '_, A> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LegacyDefaultChecks")
            .field("checks", &std::ptr::from_ref(self.checks))
            .finish_non_exhaustive()
    }
}

/// Carries an already-computed reservation and mutation quotation into its admitted action.
#[derive(Debug)]
struct OffenderAppend {
    quote: StorageQuote,
    reserve: usize,
}

fn checked(value: Option<usize>) -> RunResult<usize> {
    value.ok_or(RunError::Contract("legacy default scan quotation overflow"))
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    /// Completes the ordinary ordering scan and its report before returning to later class checks.
    pub(super) async fn check_class_legacy_default_order(
        &self,
        class: StaticClassLiteral<'db>,
        node: &ast::StmtClassDef,
        context: GenericContext<'db>,
    ) -> RunResult<()> {
        let effects = self.source.local_with_fixed_transfers(6, 0, || LegacyDefaultChecks {
            checks: self,
            #[cfg(test)]
            class,
        }).await?;
        // The local helper admits the entire unboxed future; this quote adds its semantic
        // arguments and returned result across the enclosing call and await.
        self.source.local_quoted_with_fixed_transfers(
            Ok((CALL_4 + 8, size_of::<[(StaticClassLiteral<'db>, &ast::StmtClassDef, GenericContext<'db>, &LegacyDefaultChecks<'_, '_, '_, 'run, 'db, '_, A>); 2]>() + size_of::<[RunResult<()>; 4]>())),
            || check_legacy_default_order_with(class, node, context, &effects),
        ).await?.await?;
        #[cfg(test)]
        observations::validation_completed(
            self.builder.context.file(),
            class,
            ValidationStage::LegacyDefaultOrder,
            &self.builder.context.retained_diagnostics(),
        );
        Ok(())
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> LegacyDefaultOrderEffects<'db>
    for LegacyDefaultChecks<'_, '_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn variables(&self, context: GenericContext<'db>) -> RunResult<&'db ContextVariables<'db>> {
        self.checks.source.local_quoted_with_fixed_transfers(
            Ok((CALL_2 + 8, size_of::<[(&Self, GenericContext<'db>); 2]>() + size_of::<[RunResult<&ContextVariables<'db>>; 4]>())),
            || ClassDefaultReferenceEffects::variables(self.checks, context),
        ).await?.await
    }

    async fn cursor<'a>(&self, variables: &'a ContextVariables<'db>) -> RunResult<VariableCursor<'a, 'db>> {
        self.checks.source.local_quoted_with_fixed_transfers(
            Ok((CALL_3 + 8, size_of::<[(&Self, &ContextVariables<'db>, VariableRange); 2]>() + size_of::<[RunResult<VariableCursor<'_, 'db>>; 4]>())),
            || ClassDefaultReferenceEffects::cursor(self.checks, variables, VariableRange::All),
        ).await?.await
    }

    async fn next_variable(&self, cursor: &mut VariableCursor<'_, 'db>) -> RunResult<Option<(usize, BoundTypeVarInstance<'db>)>> {
        self.checks.source.local_quoted_with_fixed_transfers(
            Ok((CALL_2 + 8, size_of::<[(&Self, &mut VariableCursor<'_, 'db>); 2]>() + size_of::<[RunResult<Option<(usize, BoundTypeVarInstance<'db>)>>; 4]>())),
            || ClassDefaultReferenceEffects::next_variable(self.checks, cursor),
        ).await?.await
    }

    async fn bound_typevar(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<TypeVarInstance<'db>> {
        self.checks.source.local_quoted_with_fixed_transfers(
            Ok((CALL_2 + 8, size_of::<[(&Self, BoundTypeVarInstance<'db>); 2]>() + size_of::<[RunResult<TypeVarInstance<'db>>; 4]>())),
            || ClassDefaultReferenceEffects::bound_typevar(self.checks, variable),
        ).await?.await
    }

    async fn checked_default(&self, variable: TypeVarInstance<'db>) -> RunResult<Option<Type<'db>>> {
        self.checks.source.local_quoted_with_fixed_transfers(
            Ok((CALL_2 + 8, size_of::<[(&Self, TypeVarInstance<'db>); 2]>() + size_of::<[RunResult<Option<Type<'db>>>; 4]>())),
            || ClassDefaultReferenceEffects::checked_default(self.checks, variable),
        ).await?.await
    }

    async fn new_scan(&self) -> RunResult<LegacyDefaultScan<'db>> {
        // The empty Vec quote prepays its header and retirement. The enum, field writes,
        // constructor return and passive state retirement add six bounded operations.
        let quote = const {
            match empty_vec_quote::<TypeVarInstance<'db>>() {
                Ok((work, bytes)) => Ok((work + CALL_1 + 6, bytes + size_of::<LegacyDefaultScan<'db>>())),
                Err(error) => Err(error),
            }
        };
        self.checks.source.local_quoted_with_fixed_transfers(quote, LegacyDefaultScan::new).await
    }

    async fn observe(&self, scan: &mut LegacyDefaultScan<'db>, variable: TypeVarInstance<'db>, default: Option<Type<'db>>) -> RunResult<Option<TypeVarInstance<'db>>> {
        // One method call and is_some, two tag selections, copied handles, at most one
        // state replacement, and the shared conditional/loop continuation are finite.
        let quote = const {
            event_quote(
                CALL_3 + CALL_1 + 24,
                &[size_of::<(&mut LegacyDefaultScan<'db>, TypeVarInstance<'db>, Option<Type<'db>>)>(),
                  size_of::<LegacyDefaultScan<'db>>(), size_of::<Option<TypeVarInstance<'db>>>()],
            )
        }.ok_or(RunError::Contract("legacy default observation quotation overflow"))?;
        self.checks.source.local_with_fixed_transfers(quote.0, quote.1, || scan.observe(variable, default)).await
    }

    async fn retain(&self, scan: &mut LegacyDefaultScan<'db>, variable: TypeVarInstance<'db>) -> RunResult<()> {
        // Metadata selection and quote construction precede allocation. Reservation funds
        // backing storage and relocation; each pushed handle prepays its eventual retirement.
        const METADATA_WORK: usize = VECTOR_LENGTH + VECTOR_CAPACITY + 6 * CALL_2 + 6 * CALL_1 + 44;
        const METADATA_BYTES: usize = METADATA_WORK * size_of::<RunResult<OffenderAppend>>();
        let preparation = const {
            match (
                buffer_quote_preparation(BufferQuotePreparation::VecReserveExact),
                buffer_quote_preparation(BufferQuotePreparation::PreparedVecPush),
            ) {
                (Ok((a, ab)), Ok((b, bb))) => Ok((a + b + METADATA_WORK, ab + bb + METADATA_BYTES)),
                (Err(error), _) | (_, Err(error)) => Err(error),
            }
        };
        let admission = self.checks.source.local_quoted_with_fixed_transfers(preparation, || -> RunResult<OffenderAppend> {
            let len = scan.later_offenders.len();
            let capacity = scan.later_offenders.capacity();
            let required = checked(len.checked_add(1))?;
            let target = if required > capacity { checked(capacity.checked_mul(2))?.max(required).max(4) } else { capacity };
            let reserve = if required > capacity { target - len } else { 0 };
            let (reserve_work, reserve_bytes) = vec_reserve_exact_quote::<TypeVarInstance<'db>>(len, capacity, reserve)?;
            let (push_work, push_bytes) = prepared_vec_push_quote::<TypeVarInstance<'db>>()?;
            Ok(OffenderAppend {
                quote: StorageQuote {
                    work: checked(reserve_work.checked_add(push_work).and_then(|work| work.checked_add(4)))?,
                    bytes: checked(reserve_bytes.checked_add(push_bytes))?,
                },
                reserve,
            })
        }).await??;
        self.checks.source.local_with_fixed_transfers(admission.quote.work, admission.quote.bytes, || {
            scan.later_offenders.reserve_exact(admission.reserve);
            scan.later_offenders.push(variable);
        }).await?;
        #[cfg(test)]
        observations::report_boundary(
            self.checks.builder.context.file(),
            self.class,
            ValidationStage::LegacyDefaultOrder,
            ReportBoundary::Retention,
        );
        Ok(())
    }

    async fn report_evidence<'a>(&self, scan: &'a LegacyDefaultScan<'db>) -> RunResult<Option<LegacyDefaultReport<'a, 'db>>> {
        // The Vec-to-slice borrow follows its pointer and length; no elements are copied.
        let quote = const {
            event_quote(
                5 * CALL_1 + CALL_2 + VECTOR_LENGTH + crate::types::local_transfer::collections::POINTER_ACCESS + 24,
                &[size_of::<&LegacyDefaultScan<'db>>(), size_of::<Option<LegacyDefaultReport<'_, 'db>>>(), size_of::<Vec<TypeVarInstance<'db>>>()],
            )
        }.ok_or(RunError::Contract("legacy default report evidence quotation overflow"))?;
        self.checks.source.local_with_fixed_transfers(quote.0, quote.1, || scan.report()).await
    }

    async fn report(&self, class: StaticClassLiteral<'db>, node: &ast::StmtClassDef, report: LegacyDefaultReport<'_, 'db>) -> RunResult<()> {
        self.checks.source.local_quoted_with_fixed_transfers(
            Ok((crate::types::local_transfer::collections::CALL_5 + 16, size_of::<[(&Self, StaticClassLiteral<'db>, &ast::StmtClassDef, LegacyDefaultReport<'_, 'db>); 2]>() + size_of::<[RunResult<()>; 4]>())),
            || self.checks.report_legacy_default_order(class, node, report.first_default, report.first_offender, report.later_offenders),
        ).await?.await
    }
}
