//! Shared validation of class generic contexts and type-variable scoping.

pub(in crate::types::infer::builder) mod base_shadowing;
pub(in crate::types::infer::builder) mod default_references;
pub(in crate::types::infer::builder) mod legacy_defaults;
pub(in crate::types::infer::builder) mod own_shadowing;

use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_python_core::{SemanticIndex, scope::FileScopeId};

use crate::FxIndexSet;
use crate::types::context::InferContext;
use crate::types::diagnostic::INVALID_GENERIC_CLASS;
use crate::types::generics::GenericContext;
use crate::types::typevar::BoundTypeVarInstance;
use crate::types::StaticClassLiteral;

pub(super) struct OrdinaryClassGenericCheckEffects<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
    pub(super) index: &'a SemanticIndex<'db>,
}

pub(in crate::types::infer::builder) struct ClassGenericCheckFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassGenericCheckEffects)]
    pub(in crate::types::infer::builder) trait ClassGenericCheckEffects<'db> {
        type Error;

        #[operation(child)]
        async fn pep695_context(&self, class: StaticClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn inherited_context(&self, class: StaticClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn check_inherited_variables(&self, class_node: &ast::StmtClassDef, generic_context: GenericContext<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn legacy_context(&self, class: StaticClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn check_inherited_subset(&self, class_node: &ast::StmtClassDef, legacy: GenericContext<'db>, inherited: GenericContext<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_type_params(&self, class_node: &ast::StmtClassDef, type_params: &ast::TypeParams) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn invalid_generic_class_enabled(&self) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn check_legacy_defaults(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef, generic_context: GenericContext<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_default_references(&self, class: StaticClassLiteral<'db>, generic_context: GenericContext<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn parent_scope(&self, class: StaticClassLiteral<'db>) -> Result<Option<FileScopeId>, Self::Error>;
        #[operation(child)]
        async fn generic_context(&self, class: StaticClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn check_own_shadowing(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef, parent: FileScopeId, generic_context: GenericContext<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn base_variables(&self, class: StaticClassLiteral<'db>) -> Result<FxIndexSet<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_base_variable(&self, variables: &FxIndexSet<BoundTypeVarInstance<'db>>, cursor: &mut usize) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(child)]
        async fn check_base_shadowing(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef, parent: FileScopeId, base_typevar: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl ClassGenericCheckFacts {
        fn type_params<'node>(&self, class_node: &'node ast::StmtClassDef) -> Option<&'node ast::TypeParams> {
            class_node.type_params.as_deref()
        }
    }

    #[synchronous(check_generic_context_sync)]
    #[capabilities(effects = ClassGenericCheckEffects, facts = ClassGenericCheckFacts)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_generic_context_with<'db, E: ClassGenericCheckEffects<'db>>(
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        facts: ClassGenericCheckFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        // If the class is generic, verify that its generic context does not violate any of
        // the typevar scoping rules.
        if matches!(effects.pep695_context(class).await?, Some(_))
            && let Some(generic_context) = effects.inherited_context(class).await?
        {
            effects.check_inherited_variables(class_node, generic_context).await?;
        }

        if let (Some(legacy), Some(inherited)) = (
            effects.legacy_context(class).await?,
            effects.inherited_context(class).await?,
        ) {
            effects.check_inherited_subset(class_node, legacy, inherited).await?;
        }

        // Check that no type parameter with a default follows a TypeVarTuple.
        // This is prohibited by the typing spec because a TypeVarTuple consumes
        // all remaining positional type arguments.
        if let Some(type_params) = facts.type_params(class_node) {
            effects.check_type_params(class_node, type_params).await?;
        }

        if effects.invalid_generic_class_enabled().await? {
            if matches!(effects.pep695_context(class).await?, None)
                && let Some(generic_context) = effects.legacy_context(class).await?
            {
                effects.check_legacy_defaults(class, class_node, generic_context).await?;
            }

            // Check that type variable defaults only reference type variables
            // that precede them in the type parameter list.
            let default_context = match effects.pep695_context(class).await? {
                Some(generic_context) => Some(generic_context),
                None => effects.legacy_context(class).await?,
            };
            if let Some(generic_context) = default_context {
                effects.check_default_references(class, generic_context).await?;
            }

            if let Some(parent) = effects.parent_scope(class).await? {
                // Check that the class's own type parameters don't shadow
                // type variables from enclosing scopes (by name).
                if let Some(generic_context) = effects.generic_context(class).await? {
                    effects.check_own_shadowing(class, class_node, parent, generic_context).await?;
                }

                // Check that the class's base classes don't reference type
                // variables from enclosing scopes (by identity).
                let base_variables = effects.base_variables(class).await?;
                let mut cursor = 0;
                #[cursor_loop]
                while let Some(base_typevar) = effects.next_base_variable(&base_variables, &mut cursor).await? {
                    effects.check_base_shadowing(class, class_node, parent, base_typevar).await?;
                }
            }
        }
        Ok(())
    }
}

/// Advances through the collected base variables in their original order.
/// Controlled callers admit the cursor step before invoking this helper.
pub(in crate::types::infer::builder) fn next_class_base_variable<'db>(
    variables: &FxIndexSet<BoundTypeVarInstance<'db>>,
    cursor: &mut usize,
) -> Option<BoundTypeVarInstance<'db>> {
    let variable = variables.get_index(*cursor).copied()?;
    *cursor += 1;
    Some(variable)
}

impl<'db> SynchronousClassGenericCheckEffects<'db>
    for OrdinaryClassGenericCheckEffects<'_, 'db, '_>
{
    type Error = Infallible;

    fn pep695_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        Ok(class.pep695_generic_context(self.context.db()))
    }

    fn inherited_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        Ok(class.inherited_legacy_generic_context(self.context.db()))
    }

    fn check_inherited_variables(
        &self,
        class_node: &ast::StmtClassDef,
        generic_context: GenericContext<'db>,
    ) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        if let Some(typevar) = generic_context
            .variables(db)
            .find(|typevar| !typevar.typevar(db).is_self(db))
            && let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, class_node)
        {
            builder.into_diagnostic(format_args!(
                "Legacy type variable `{}` cannot be used in a PEP 695 class base",
                typevar.name(db),
            ));
        }
        Ok(())
    }

    fn legacy_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        Ok(class.legacy_generic_context(self.context.db()))
    }

    fn check_inherited_subset(
        &self,
        class_node: &ast::StmtClassDef,
        legacy: GenericContext<'db>,
        inherited: GenericContext<'db>,
    ) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        if !inherited.is_subset_of(db, legacy)
            && let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, class_node)
        {
            builder.into_diagnostic(
                "`Generic` base class must include all type \
                    variables used in other base classes",
            );
        }
        Ok(())
    }

    fn check_type_params(
        &self,
        class_node: &ast::StmtClassDef,
        type_params: &ast::TypeParams,
    ) -> Result<(), Self::Error> {
        let context = self.context;
        super::super::type_param_validation::check_single_typevar_tuple_pep695(
            context,
            type_params,
            super::super::type_param_validation::TypeParameterOwner::GenericClass(
                &class_node.name.id,
            ),
        );
        super::super::type_param_validation::check_no_default_after_typevar_tuple_pep695(
            context,
            type_params,
        );
        Ok(())
    }

    fn invalid_generic_class_enabled(&self) -> Result<bool, Self::Error> {
        Ok(self.context.is_lint_enabled(&INVALID_GENERIC_CLASS))
    }

    fn check_legacy_defaults(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        generic_context: GenericContext<'db>,
    ) -> Result<(), Self::Error> {
        legacy_defaults::check_legacy_default_order_sync(class, class_node, generic_context, self)
    }

    fn check_default_references(
        &self,
        class: StaticClassLiteral<'db>,
        generic_context: GenericContext<'db>,
    ) -> Result<(), Self::Error> {
        default_references::check_class_default_references_sync(class, generic_context, self)
    }

    fn parent_scope(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<FileScopeId>, Self::Error> {
        let db = self.context.db();
        Ok(class.body_scope(db).scope(db).parent())
    }

    fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        Ok(class.generic_context(self.context.db()))
    }

    fn check_own_shadowing(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        parent: FileScopeId,
        generic_context: GenericContext<'db>,
    ) -> Result<(), Self::Error> {
        own_shadowing::check_class_own_shadowing_sync(
            self.index, class, class_node, parent, generic_context, self,
        )
    }

    fn base_variables(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<FxIndexSet<BoundTypeVarInstance<'db>>, Self::Error> {
        Ok(class.typevars_referenced_in_bases(self.context.db()))
    }

    fn next_base_variable(
        &self,
        variables: &FxIndexSet<BoundTypeVarInstance<'db>>,
        cursor: &mut usize,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error> {
        Ok(next_class_base_variable(variables, cursor))
    }

    fn check_base_shadowing(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        parent: FileScopeId,
        base_typevar: BoundTypeVarInstance<'db>,
    ) -> Result<(), Self::Error> {
        base_shadowing::check_class_base_shadowing_sync(
            self.index, class, class_node, parent, base_typevar, self,
        )
    }
}
