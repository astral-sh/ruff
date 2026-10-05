//! Checks class defaults against the earlier variables in the same generic context.

use std::convert::Infallible;

use super::OrdinaryClassGenericCheckEffects;
use crate::types::diagnostic::report_invalid_typevar_default_reference;
use crate::types::generics::context_construction::ContextVariables;
use crate::types::typevar::TypeVarInstance;
use crate::types::visitor::find_over_type;
use crate::types::{
    BoundTypeVarInstance, GenericContext, KnownInstanceType, StaticClassLiteral, Type,
};

/// Selects the stored variables available for a default-reference comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::types::infer::builder) enum VariableRange {
    All,
    Before(usize),
    From(usize),
}

/// Borrows one contiguous part of a canonical context without copying its variables.
#[derive(Debug)]
pub(in crate::types::infer::builder) struct VariableCursor<'a, 'db> {
    variables: &'a ContextVariables<'db>,
    next: usize,
    end: usize,
}

impl<'a, 'db> VariableCursor<'a, 'db> {
    /// Starts a scan over all variables, an earlier prefix, or the current-and-later suffix.
    pub(in crate::types::infer::builder) fn new(
        variables: &'a ContextVariables<'db>,
        range: VariableRange,
    ) -> Self {
        let (next, end) = match range {
            VariableRange::All => (0, variables.len()),
            VariableRange::Before(end) => (0, end),
            VariableRange::From(next) => (next, variables.len()),
        };
        Self {
            variables,
            next,
            end,
        }
    }

    /// Advances in stored order, returning the position used to split a default's scope.
    pub(in crate::types::infer::builder) fn next(
        &mut self,
    ) -> Option<(usize, BoundTypeVarInstance<'db>)> {
        if self.next >= self.end {
            return None;
        }
        let position = self.next;
        let variable = GenericContext::variable_at_in(self.variables, position)?;
        self.next += 1;
        Some((position, variable))
    }
}

/// Distinguishes bound occurrences from the unbound values used by legacy defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::types::infer::builder) enum TypeVariableReference<'db> {
    Bound(BoundTypeVarInstance<'db>),
    Unbound(TypeVarInstance<'db>),
    Other,
}

/// Recognizes both representations that participate in the default-reference rule.
pub(in crate::types::infer::builder) const fn type_variable_reference(
    ty: Type<'_>,
) -> TypeVariableReference<'_> {
    match ty {
        Type::TypeVar(variable) => TypeVariableReference::Bound(variable),
        Type::KnownInstance(KnownInstanceType::TypeVar(variable)) => {
            TypeVariableReference::Unbound(variable)
        }
        _ => TypeVariableReference::Other,
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassDefaultReferenceEffects)]
    pub(in crate::types::infer::builder) trait ClassDefaultReferenceEffects<'db> {
        type Error;

        #[operation(source)]
        async fn variables(&self, context: GenericContext<'db>) -> Result<&'db ContextVariables<'db>, Self::Error>;
        #[operation(local)]
        async fn cursor<'a>(&self, variables: &'a ContextVariables<'db>, range: VariableRange) -> Result<VariableCursor<'a, 'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_variable(&self, cursor: &mut VariableCursor<'_, 'db>) -> Result<Option<(usize, BoundTypeVarInstance<'db>)>, Self::Error>;
        #[operation(source)]
        async fn bound_typevar(&self, variable: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(child)]
        async fn checked_default(&self, variable: TypeVarInstance<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn first_invalid(&self, default: Type<'db>, variables: &ContextVariables<'db>, position: usize) -> Result<Option<TypeVarInstance<'db>>, Self::Error>;
        #[operation(local)]
        async fn reference(&self, ty: Type<'db>) -> Result<TypeVariableReference<'db>, Self::Error>;
        #[operation(local)]
        async fn same_instance(&self, left: TypeVarInstance<'db>, right: TypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn contains_instance(&self, variables: &ContextVariables<'db>, range: VariableRange, variable: TypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report(&self, class: StaticClassLiteral<'db>, variable: TypeVarInstance<'db>, referenced: TypeVarInstance<'db>, is_later_in_list: bool) -> Result<(), Self::Error>;
    }

    /// Compares the actual variable instances in a stored prefix or suffix.
    /// Instances that share a [`TypeVarIdentity`](crate::types::typevar::TypeVarIdentity) can
    /// still have different bounds or defaults, so identity equality alone does not establish membership.
    #[synchronous(contains_default_reference_sync)]
    #[capabilities(effects = ClassDefaultReferenceEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn contains_default_reference_with<'db, E: ClassDefaultReferenceEffects<'db>>(
        variables: &ContextVariables<'db>,
        range: VariableRange,
        variable: TypeVarInstance<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        let mut cursor = effects.cursor(variables, range).await?;
        #[cursor_loop]
        while let Some(entry) = effects.next_variable(&mut cursor).await? {
            let (_, bound) = entry;
            let candidate = effects.bound_typevar(bound).await?;
            if effects.same_instance(candidate, variable).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns an encountered type variable unless an earlier context variable permits it.
    /// The type walker uses this optional result to retain the first invalid reference.
    #[synchronous(invalid_default_reference_sync)]
    #[capabilities(effects = ClassDefaultReferenceEffects)]
    #[passive_values(VariableRange::Before)]
    pub(in crate::types::infer::builder) async fn invalid_default_reference_with<'db, E: ClassDefaultReferenceEffects<'db>>(
        ty: Type<'db>,
        variables: &ContextVariables<'db>,
        position: usize,
        effects: &E,
    ) -> Result<Option<TypeVarInstance<'db>>, E::Error> {
        let variable = match effects.reference(ty).await? {
            TypeVariableReference::Bound(bound) => effects.bound_typevar(bound).await?,
            TypeVariableReference::Unbound(variable) => variable,
            TypeVariableReference::Other => return Ok(None),
        };
        if effects.contains_instance(variables, VariableRange::Before(position), variable).await? {
            Ok(None)
        } else {
            Ok(Some(variable))
        }
    }

    /// Checks every default and reports its first reference outside the earlier-variable prefix.
    /// The current-and-later suffix distinguishes a forward reference from an out-of-scope variable.
    #[synchronous(check_class_default_references_sync)]
    #[capabilities(effects = ClassDefaultReferenceEffects)]
    #[passive_values(VariableRange::All, VariableRange::From)]
    pub(in crate::types::infer::builder) async fn check_class_default_references_with<'db, E: ClassDefaultReferenceEffects<'db>>(
        class: StaticClassLiteral<'db>,
        context: GenericContext<'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        let variables = effects.variables(context).await?;
        let mut cursor = effects.cursor(variables, VariableRange::All).await?;
        #[cursor_loop]
        while let Some(entry) = effects.next_variable(&mut cursor).await? {
            let (position, bound) = entry;
            let variable = effects.bound_typevar(bound).await?;
            let Some(default) = effects.checked_default(variable).await? else {
                continue;
            };
            if let Some(referenced) = effects.first_invalid(default, variables, position).await? {
                let is_later_in_list = effects.contains_instance(variables, VariableRange::From(position), referenced).await?;
                effects.report(class, variable, referenced, is_later_in_list).await?;
            }
        }
        Ok(())
    }
}

impl<'db> SynchronousClassDefaultReferenceEffects<'db>
    for OrdinaryClassGenericCheckEffects<'_, 'db, '_>
{
    type Error = Infallible;

    fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Infallible> {
        Ok(context.variables_with_fields(salsa::FieldReads::new(self.context.db())))
    }

    fn cursor<'a>(
        &self,
        variables: &'a ContextVariables<'db>,
        range: VariableRange,
    ) -> Result<VariableCursor<'a, 'db>, Infallible> {
        Ok(VariableCursor::new(variables, range))
    }

    fn next_variable(
        &self,
        cursor: &mut VariableCursor<'_, 'db>,
    ) -> Result<Option<(usize, BoundTypeVarInstance<'db>)>, Infallible> {
        Ok(cursor.next())
    }

    fn bound_typevar(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarInstance<'db>, Infallible> {
        Ok(variable.typevar(self.context.db()))
    }

    fn checked_default(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(variable.default_type(self.context.db(), self.context.program_environment()))
    }

    fn first_invalid(
        &self,
        default: Type<'db>,
        variables: &ContextVariables<'db>,
        position: usize,
    ) -> Result<Option<TypeVarInstance<'db>>, Infallible> {
        Ok(find_over_type(
            self.context.db(),
            self.context.program_environment(),
            default,
            false,
            |ty| match invalid_default_reference_sync(ty, variables, position, self) {
                Ok(reference) => reference,
                Err(never) => match never {},
            },
        ))
    }

    fn reference(&self, ty: Type<'db>) -> Result<TypeVariableReference<'db>, Infallible> {
        Ok(type_variable_reference(ty))
    }

    fn same_instance(
        &self,
        left: TypeVarInstance<'db>,
        right: TypeVarInstance<'db>,
    ) -> Result<bool, Infallible> {
        Ok(left == right)
    }

    fn contains_instance(
        &self,
        variables: &ContextVariables<'db>,
        range: VariableRange,
        variable: TypeVarInstance<'db>,
    ) -> Result<bool, Infallible> {
        contains_default_reference_sync(variables, range, variable, self)
    }

    fn report(
        &self,
        class: StaticClassLiteral<'db>,
        variable: TypeVarInstance<'db>,
        referenced: TypeVarInstance<'db>,
        is_later_in_list: bool,
    ) -> Result<(), Infallible> {
        report_invalid_typevar_default_reference(
            self.context,
            class,
            variable,
            referenced,
            is_later_in_list,
        );
        Ok(())
    }
}
