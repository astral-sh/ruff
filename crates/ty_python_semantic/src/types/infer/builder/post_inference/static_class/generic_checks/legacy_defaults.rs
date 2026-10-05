//! Checks the order of checked defaults in a legacy class's canonical type-variable list.

use std::convert::Infallible;

use ruff_python_ast as ast;

use super::OrdinaryClassGenericCheckEffects;
use super::default_references::{
    SynchronousClassDefaultReferenceEffects, VariableCursor, VariableRange,
};
use crate::types::diagnostic::report_invalid_type_param_order;
use crate::types::generics::context_construction::ContextVariables;
use crate::types::typevar::TypeVarInstance;
use crate::types::{BoundTypeVarInstance, GenericContext, StaticClassLiteral, Type};

/// Retains the first default and, once ordering is invalid, the first later variable without one.
#[derive(Debug, Clone, Copy)]
enum Ordering<'db> {
    NoDefault,
    Default(TypeVarInstance<'db>),
    Invalid {
        first_default: TypeVarInstance<'db>,
        first_offender: TypeVarInstance<'db>,
    },
}

/// Keeps later offending variables in encounter order across checked-default requests.
#[derive(Debug)]
pub(in crate::types::infer::builder) struct LegacyDefaultScan<'db> {
    ordering: Ordering<'db>,
    pub later_offenders: Vec<TypeVarInstance<'db>>,
}

impl<'db> LegacyDefaultScan<'db> {
    /// Starts a scan without a default or offending variable.
    pub(in crate::types::infer::builder) const fn new() -> Self {
        Self {
            ordering: Ordering::NoDefault,
            later_offenders: Vec::new(),
        }
    }

    /// Updates the fixed state, returning only additional offenders that need buffer admission.
    pub(in crate::types::infer::builder) fn observe(
        &mut self,
        variable: TypeVarInstance<'db>,
        default: Option<Type<'db>>,
    ) -> Option<TypeVarInstance<'db>> {
        match (self.ordering, default.is_some()) {
            (Ordering::NoDefault, true) => self.ordering = Ordering::Default(variable),
            (Ordering::Default(first_default), false) => {
                self.ordering = Ordering::Invalid {
                    first_default,
                    first_offender: variable,
                };
            }
            (Ordering::Invalid { .. }, false) => return Some(variable),
            (Ordering::NoDefault, false)
            | (Ordering::Default(_) | Ordering::Invalid { .. }, true) => {}
        }
        None
    }

    /// Borrows a report only when the scan has an explicit first offender.
    pub(in crate::types::infer::builder) fn report(&self) -> Option<LegacyDefaultReport<'_, 'db>> {
        match self.ordering {
            Ordering::Invalid {
                first_default,
                first_offender,
            } => Some(LegacyDefaultReport {
                first_default,
                first_offender,
                later_offenders: &self.later_offenders,
            }),
            Ordering::NoDefault | Ordering::Default(_) => None,
        }
    }
}

/// A nonempty ordered collection of offenders, anchored to the first default-bearing variable.
#[derive(Debug, Clone, Copy)]
pub(in crate::types::infer::builder) struct LegacyDefaultReport<'a, 'db> {
    pub first_default: TypeVarInstance<'db>,
    pub first_offender: TypeVarInstance<'db>,
    pub later_offenders: &'a [TypeVarInstance<'db>],
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousLegacyDefaultOrderEffects)]
    pub(in crate::types::infer::builder) trait LegacyDefaultOrderEffects<'db> {
        type Error;

        #[operation(source)]
        async fn variables(&self, context: GenericContext<'db>) -> Result<&'db ContextVariables<'db>, Self::Error>;
        #[operation(local)]
        async fn cursor<'a>(&self, variables: &'a ContextVariables<'db>) -> Result<VariableCursor<'a, 'db>, Self::Error>;
        #[operation(local)] #[progress]
        async fn next_variable(&self, cursor: &mut VariableCursor<'_, 'db>) -> Result<Option<(usize, BoundTypeVarInstance<'db>)>, Self::Error>;
        #[operation(source)]
        async fn bound_typevar(&self, variable: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(child)]
        async fn checked_default(&self, variable: TypeVarInstance<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn new_scan(&self) -> Result<LegacyDefaultScan<'db>, Self::Error>;
        #[operation(local)]
        async fn observe(&self, scan: &mut LegacyDefaultScan<'db>, variable: TypeVarInstance<'db>, default: Option<Type<'db>>) -> Result<Option<TypeVarInstance<'db>>, Self::Error>;
        #[operation(local)]
        async fn retain(&self, scan: &mut LegacyDefaultScan<'db>, variable: TypeVarInstance<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn report_evidence<'a>(&self, scan: &'a LegacyDefaultScan<'db>) -> Result<Option<LegacyDefaultReport<'a, 'db>>, Self::Error>;
        #[operation(child)]
        async fn report(&self, class: StaticClassLiteral<'db>, node: &ast::StmtClassDef, report: LegacyDefaultReport<'_, 'db>) -> Result<(), Self::Error>;
    }

    /// Evaluates every default before reporting all variables without defaults after the first one with a default.
    #[synchronous(check_legacy_default_order_sync)]
    #[capabilities(effects = LegacyDefaultOrderEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_legacy_default_order_with<'db, E: LegacyDefaultOrderEffects<'db>>(
        class: StaticClassLiteral<'db>,
        node: &ast::StmtClassDef,
        context: GenericContext<'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        let variables = effects.variables(context).await?;
        let mut cursor = effects.cursor(variables).await?;
        let mut scan = effects.new_scan().await?;
        #[cursor_loop]
        while let Some(entry) = effects.next_variable(&mut cursor).await? {
            let (_, bound) = entry;
            let variable = effects.bound_typevar(bound).await?;
            let default = effects.checked_default(variable).await?;
            if let Some(offender) = effects.observe(&mut scan, variable, default).await? {
                effects.retain(&mut scan, offender).await?;
            }
        }
        if let Some(report) = effects.report_evidence(&scan).await? {
            effects.report(class, node, report).await?;
        }
        Ok(())
    }
}

impl<'db> SynchronousLegacyDefaultOrderEffects<'db>
    for OrdinaryClassGenericCheckEffects<'_, 'db, '_>
{
    type Error = Infallible;

    fn variables(&self, context: GenericContext<'db>) -> Result<&'db ContextVariables<'db>, Infallible> {
        SynchronousClassDefaultReferenceEffects::variables(self, context)
    }

    fn cursor<'a>(&self, variables: &'a ContextVariables<'db>) -> Result<VariableCursor<'a, 'db>, Infallible> {
        SynchronousClassDefaultReferenceEffects::cursor(self, variables, VariableRange::All)
    }

    fn next_variable(&self, cursor: &mut VariableCursor<'_, 'db>) -> Result<Option<(usize, BoundTypeVarInstance<'db>)>, Infallible> {
        SynchronousClassDefaultReferenceEffects::next_variable(self, cursor)
    }

    fn bound_typevar(&self, variable: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Infallible> {
        SynchronousClassDefaultReferenceEffects::bound_typevar(self, variable)
    }

    fn checked_default(&self, variable: TypeVarInstance<'db>) -> Result<Option<Type<'db>>, Infallible> {
        SynchronousClassDefaultReferenceEffects::checked_default(self, variable)
    }

    fn new_scan(&self) -> Result<LegacyDefaultScan<'db>, Infallible> {
        Ok(LegacyDefaultScan::new())
    }

    fn observe(&self, scan: &mut LegacyDefaultScan<'db>, variable: TypeVarInstance<'db>, default: Option<Type<'db>>) -> Result<Option<TypeVarInstance<'db>>, Infallible> {
        Ok(scan.observe(variable, default))
    }

    fn retain(&self, scan: &mut LegacyDefaultScan<'db>, variable: TypeVarInstance<'db>) -> Result<(), Infallible> {
        scan.later_offenders.push(variable);
        Ok(())
    }

    fn report_evidence<'a>(&self, scan: &'a LegacyDefaultScan<'db>) -> Result<Option<LegacyDefaultReport<'a, 'db>>, Infallible> {
        Ok(scan.report())
    }

    fn report(&self, class: StaticClassLiteral<'db>, node: &ast::StmtClassDef, report: LegacyDefaultReport<'_, 'db>) -> Result<(), Infallible> {
        report_invalid_type_param_order(
            self.context,
            class,
            node,
            report.first_default,
            report.first_offender,
            report.later_offenders,
        );
        Ok(())
    }
}
