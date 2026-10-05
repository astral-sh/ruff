//! Canonical PEP 695 declarations, including immediate diagnostics and deferred ownership.

use std::convert::Infallible;

use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast as ast;
use ruff_text_size::TextRange;
use ty_python_core::ast_node_ref::AstNodeRef;
use ty_python_core::definition::Definition;

use super::super::{DeclaredAndInferredType, TypeInferenceBuilder};
use crate::types::diagnostic::INVALID_TYPE_VARIABLE_CONSTRAINTS;
use crate::types::infer::type_parameter_header::{
    TypeParameterHeader, TypeParameterHeaderInput, infer_type_parameter_header,
};
use crate::types::{KnownInstanceType, Type};

/// A declaration node whose AST is borrowed only after source access has been admitted.
#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer::builder) enum TypeParameterDefinitionNode<'a> {
    TypeVar(&'a AstNodeRef<ast::TypeParamTypeVar>),
    ParamSpec(&'a AstNodeRef<ast::TypeParamParamSpec>),
    TypeVarTuple(&'a AstNodeRef<ast::TypeParamTypeVarTuple>),
}

impl TypeParameterDefinitionNode<'_> {
    /// Resolves this declaration in the prepared module that owns its node index.
    pub(in crate::types::infer::builder) fn node<'ast>(
        self,
        module: &'ast ParsedModuleRef,
    ) -> ast::TypeParamRef<'ast> {
        match self {
            Self::TypeVar(node) => ast::TypeParamRef::TypeVar(node.node(module)),
            Self::ParamSpec(node) => ast::TypeParamRef::ParamSpec(node.node(module)),
            Self::TypeVarTuple(node) => ast::TypeParamRef::TypeVarTuple(node.node(module)),
        }
    }
}

/// Finite conversion from a completed header to its declaration type.
#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) struct TypeParameterDeclarationFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTypeParameterDeclarationEffects)]
    pub(in crate::types::infer) trait TypeParameterDeclarationEffects<'db, 'ast> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn header(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, node: ast::TypeParamRef<'_>) -> Result<TypeParameterHeader<'db>, Self::Error>;
        #[operation(child)]
        async fn invalid_constraint_count(&self, builder: &TypeInferenceBuilder<'db, 'ast>, range: TextRange) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn record_deferred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn bind_declaration(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, node: ast::TypeParamRef<'_>, definition: Definition<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl TypeParameterDeclarationFacts {
        fn declared_type<'db>(&self, header: TypeParameterHeader<'db>) -> Type<'db> {
            Type::KnownInstance(KnownInstanceType::TypeVar(header.variable))
        }
    }

    /// Stores one declaration after its immediate diagnostic and deferred-owner effects succeed.
    #[synchronous(infer_type_parameter_definition_sync)]
    #[capabilities(effects = TypeParameterDeclarationEffects, facts = TypeParameterDeclarationFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn infer_type_parameter_definition_with<'db, 'ast, E: TypeParameterDeclarationEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        node: ast::TypeParamRef<'_>,
        facts: TypeParameterDeclarationFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        effects.checkpoint().await?;
        let header = effects.header(builder, definition, node).await?;
        if let Some(range) = header.invalid_constraint_count {
            effects.invalid_constraint_count(builder, range).await?;
        }
        if let Some(deferred) = header.deferred {
            effects.record_deferred(builder, deferred).await?;
        }
        let ty = facts.declared_type(header);
        effects.bind_declaration(builder, node, definition, ty).await
    }
}

/// Ordinary diagnostic and storage effects for all three PEP 695 parameter kinds.
#[derive(Clone, Copy, Debug)]
struct OrdinaryTypeParameterDeclarationEffects;

impl<'db, 'ast> SynchronousTypeParameterDeclarationEffects<'db, 'ast>
    for OrdinaryTypeParameterDeclarationEffects
{
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn header(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        node: ast::TypeParamRef<'_>,
    ) -> Result<TypeParameterHeader<'db>, Infallible> {
        Ok(infer_type_parameter_header(
            builder.db(),
            definition,
            TypeParameterHeaderInput::from(node),
        ))
    }

    fn invalid_constraint_count(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        range: TextRange,
    ) -> Result<(), Infallible> {
        if let Some(diagnostic) = builder
            .context
            .report_lint(&INVALID_TYPE_VARIABLE_CONSTRAINTS, range)
        {
            diagnostic.into_diagnostic("TypeVar must have at least two constrained types");
        }
        Ok(())
    }

    fn record_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<(), Infallible> {
        builder.deferred.insert(definition);
        Ok(())
    }

    fn bind_declaration(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        node: ast::TypeParamRef<'_>,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.add_declaration_with_binding(
            ast::AnyNodeRef::from(node),
            definition,
            &DeclaredAndInferredType::are_the_same_type(ty),
        );
        Ok(())
    }
}

/// Runs ordinary declaration inference through the shared three-kind algorithm.
pub(in crate::types::infer::builder) fn infer_type_parameter_definition<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    definition: Definition<'db>,
    node: ast::TypeParamRef<'_>,
) {
    match infer_type_parameter_definition_sync(
        builder,
        definition,
        node,
        TypeParameterDeclarationFacts,
        &OrdinaryTypeParameterDeclarationEffects,
    ) {
        Ok(()) => {}
        Err(never) => match never {},
    }
}
