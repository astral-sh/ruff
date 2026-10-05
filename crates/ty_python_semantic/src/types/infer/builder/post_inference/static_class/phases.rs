//! Shared ordering of static-class validation after deferred inference.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_python_core::SemanticIndex;

use crate::types::class::{CodeGeneratorKind, ExpandedClassBaseEntry};
use crate::types::context::InferContext;
use crate::types::diagnostic::{INVALID_GENERIC_CLASS, IncompatibleBases};
use crate::types::protocol_class::ProtocolClass;
use crate::types::{
    ClassLiteral, ClassType, GenericContext, KnownClass, StaticClassLiteral, Type, overrides,
};

pub(in crate::types::infer::builder) struct StaticClassBaseChecks<'node, 'db> {
    pub(super) expanded_entries: Vec<ExpandedClassBaseEntry<'node, 'db>>,
    pub(super) disjoint_bases: IncompatibleBases<'db>,
    pub(super) direct_typed_dict_bases: Vec<ClassType<'db>>,
}

pub(super) struct OrdinaryStaticClassDefinitionEffects<'a, 'db, 'ast, F> {
    pub(super) context: &'a InferContext<'db, 'ast>,
    pub(super) index: &'a SemanticIndex<'db>,
    pub(super) file_expression_type: &'a F,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousStaticClassDefinitionEffects)]
    pub(in crate::types::infer::builder) trait StaticClassDefinitionEffects<'db> {
        type Error;

        #[operation(child)]
        async fn check_inheritance_cycle(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn check_slots(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_generic_enum(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn class_kind(&self, class: StaticClassLiteral<'db>) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error>;
        #[operation(child)]
        async fn check_named_tuple(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn is_protocol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn check_disjoint_base_decorator(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef, class_kind: Option<CodeGeneratorKind<'db>>, is_protocol: bool) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_dataclass_application(&self, class: StaticClassLiteral<'db>, is_protocol: bool) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_explicit_bases<'node>(&self, class: StaticClassLiteral<'db>, class_node: &'node ast::StmtClassDef, class_kind: Option<CodeGeneratorKind<'db>>, is_protocol: bool) -> Result<StaticClassBaseChecks<'node, 'db>, Self::Error>;
        #[operation(child)]
        async fn check_mro(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef, bases: &mut StaticClassBaseChecks<'_, 'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn check_total_ordering(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_metaclass(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_arguments(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef, class_kind: Option<CodeGeneratorKind<'db>>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_generic_context(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_dataclass_fields(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef, field_policy: CodeGeneratorKind<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_overrides(&self, class: StaticClassLiteral<'db>, inconsistent_generic_bases: bool) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn namespace_metaclass(&self, class: StaticClassLiteral<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn builtin_type(&self) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn same_type(&self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn check_namespace(&self, class: StaticClassLiteral<'db>, metaclass: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn is_final(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn check_abstract_methods(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_final_values(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_protocol_variance(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn nominal_variance_enabled(&self) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn generic_context(&self, class: StaticClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn check_nominal_variance(&self, class: StaticClassLiteral<'db>, generic_context: GenericContext<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn check_typed_dict(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef, direct_typed_dict_bases: &[ClassType<'db>]) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_members(&self, class: StaticClassLiteral<'db>, field_policy: CodeGeneratorKind<'db>) -> Result<(), Self::Error>;
    }

    #[synchronous(check_static_class_definitions_sync)]
    #[capabilities(effects = StaticClassDefinitionEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_static_class_definitions_with<'db, E: StaticClassDefinitionEffects<'db>>(
        ty: Type<'db>,
        class_node: &ast::StmtClassDef,
        effects: &E,
    ) -> Result<(), E::Error> {
        let Type::ClassLiteral(ClassLiteral::Static(class)) = ty else {
            return Ok(());
        };

        if effects.check_inheritance_cycle(class, class_node).await? {
            return Ok(());
        }

        effects.check_slots(class).await?;
        effects.check_generic_enum(class, class_node).await?;
        let class_kind = effects.class_kind(class).await?;
        if matches!(class_kind, Some(CodeGeneratorKind::NamedTuple)) {
            effects.check_named_tuple(class).await?;
        }
        let is_protocol = effects.is_protocol(class).await?;
        effects.check_disjoint_base_decorator(class, class_node, class_kind, is_protocol).await?;
        effects.check_dataclass_application(class, is_protocol).await?;
        let mut bases = effects.check_explicit_bases(class, class_node, class_kind, is_protocol).await?;
        let inconsistent_generic_bases = effects.check_mro(class, class_node, &mut bases).await?;
        effects.check_total_ordering(class, class_node).await?;
        effects.check_metaclass(class, class_node).await?;
        effects.check_arguments(class, class_node, class_kind).await?;
        effects.check_generic_context(class, class_node).await?;
        if let Some(field_policy @ CodeGeneratorKind::DataclassLike(_)) = effects.class_kind(class).await? {
            effects.check_dataclass_fields(class, class_node, field_policy).await?;
        }

        // Check for violations of the Liskov Substitution Principle,
        // and for violations of other rules relating to invalid overrides of some sort.
        effects.check_overrides(class, inconsistent_generic_bases).await?;

        // Check compatibility between class namespace values and metaclass-populated attributes.
        let metaclass = effects.namespace_metaclass(class).await?;
        let builtin_type = effects.builtin_type().await?;
        if !effects.same_type(metaclass, builtin_type).await? {
            effects.check_namespace(class, metaclass).await?;
        }

        // Exclude `Protocol` classes. It is possible to subtype a `Protocol` class
        // without subclassing it, so an `@final` `Protocol` class with unimplemented abstract
        // methods is not inherently broken in the same way as a non-`Protocol` final class
        // with unimplemented abstract methods.
        if effects.is_final(class).await? && !effects.is_protocol(class).await? {
            effects.check_abstract_methods(class, class_node).await?;
        }

        // Check for Final-qualified declarations without a value.
        effects.check_final_values(class).await?;
        if effects.is_protocol(class).await? {
            effects.check_protocol_variance(class).await?;
        } else if effects.nominal_variance_enabled().await?
            && let Some(generic_context) = effects.generic_context(class).await?
        {
            effects.check_nominal_variance(class, generic_context).await?;
        }
        if effects.is_typed_dict(class).await? {
            effects.check_typed_dict(class, class_node, &bases.direct_typed_dict_bases).await?;
        }
        if let Some(field_policy) = effects.class_kind(class).await? {
            effects.check_members(class, field_policy).await?;
        }
        Ok(())
    }
}

impl<'db, F: Fn(&ast::Expr) -> Type<'db>> SynchronousStaticClassDefinitionEffects<'db>
    for OrdinaryStaticClassDefinitionEffects<'_, 'db, '_, F>
{
    type Error = Infallible;

    fn check_inheritance_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
    ) -> Result<bool, Self::Error> {
        Ok(super::check_inheritance_cycle(
            self.context,
            class,
            class_node,
        ))
    }

    fn check_slots(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        super::check_class_slots(self.context, class, self.index);
        Ok(())
    }

    fn check_generic_enum(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
    ) -> Result<(), Self::Error> {
        super::check_generic_enum(self.context, class, class_node);
        Ok(())
    }

    fn class_kind(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        Ok(CodeGeneratorKind::from_class(
            self.context.db(),
            class.into(),
        ))
    }

    fn check_named_tuple(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        super::check_named_tuple(self.context, class);
        Ok(())
    }

    fn is_protocol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.is_protocol(self.context.db()))
    }

    fn check_disjoint_base_decorator(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        class_kind: Option<CodeGeneratorKind<'db>>,
        is_protocol: bool,
    ) -> Result<(), Self::Error> {
        super::check_disjoint_base_decorator(
            self.context,
            class,
            class_node,
            class_kind,
            is_protocol,
            self.file_expression_type,
        );
        Ok(())
    }

    fn check_dataclass_application(
        &self,
        class: StaticClassLiteral<'db>,
        is_protocol: bool,
    ) -> Result<(), Self::Error> {
        super::check_dataclass_application(self.context, class, is_protocol);
        Ok(())
    }

    fn check_explicit_bases<'node>(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &'node ast::StmtClassDef,
        class_kind: Option<CodeGeneratorKind<'db>>,
        is_protocol: bool,
    ) -> Result<StaticClassBaseChecks<'node, 'db>, Self::Error> {
        Ok(super::check_explicit_bases(
            self.context,
            class,
            class_node,
            self.index,
            class_kind,
            is_protocol,
        ))
    }

    fn check_mro(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        bases: &mut StaticClassBaseChecks<'_, 'db>,
    ) -> Result<bool, Self::Error> {
        Ok(super::check_mro(self.context, class, class_node, bases))
    }

    fn check_total_ordering(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
    ) -> Result<(), Self::Error> {
        super::check_total_ordering(self.context, class, class_node, self.file_expression_type);
        Ok(())
    }

    fn check_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
    ) -> Result<(), Self::Error> {
        super::check_metaclass(self.context, class, class_node);
        Ok(())
    }

    fn check_arguments(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        class_kind: Option<CodeGeneratorKind<'db>>,
    ) -> Result<(), Self::Error> {
        super::check_arguments(
            self.context,
            class,
            class_node,
            class_kind,
            self.file_expression_type,
        );
        Ok(())
    }

    fn check_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
    ) -> Result<(), Self::Error> {
        super::check_generic_context(self.context, class, class_node, self.index);
        Ok(())
    }

    fn check_dataclass_fields(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        field_policy: CodeGeneratorKind<'db>,
    ) -> Result<(), Self::Error> {
        super::check_dataclass_fields(self.context, class, class_node, field_policy, self.index);
        Ok(())
    }

    fn check_overrides(
        &self,
        class: StaticClassLiteral<'db>,
        inconsistent_generic_bases: bool,
    ) -> Result<(), Self::Error> {
        overrides::check_class(self.context, class, inconsistent_generic_bases);
        Ok(())
    }

    fn namespace_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(class
            .inferred_metaclass(self.context.db())
            .for_inheritance(self.context.db(), self.context.program_environment()))
    }

    fn builtin_type(&self) -> Result<Type<'db>, Self::Error> {
        Ok(
            KnownClass::Type
                .to_class_literal(self.context.db(), self.context.program_environment()),
        )
    }

    fn same_type(&self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error> {
        Ok(left == right)
    }

    fn check_namespace(
        &self,
        class: StaticClassLiteral<'db>,
        metaclass: Type<'db>,
    ) -> Result<(), Self::Error> {
        super::check_class_namespace_against_metaclass_members(
            self.context,
            class,
            metaclass,
            self.index,
        );
        Ok(())
    }

    fn is_final(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.is_final(self.context.db()))
    }

    fn check_abstract_methods(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
    ) -> Result<(), Self::Error> {
        super::check_final_class_abstract_methods(self.context, class, class_node);
        Ok(())
    }

    fn check_final_values(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        super::check_class_final_without_value(self.context, class, self.index);
        Ok(())
    }

    fn check_protocol_variance(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        let protocol = ProtocolClass::from_class(ClassType::NonGeneric(class.into()));
        protocol.validate_members(self.context);
        protocol.validate_type_parameter_variance(self.context);
        Ok(())
    }

    fn nominal_variance_enabled(&self) -> Result<bool, Self::Error> {
        Ok(self.context.is_lint_enabled(&INVALID_GENERIC_CLASS))
    }

    fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        Ok(class.generic_context(self.context.db()))
    }

    fn check_nominal_variance(
        &self,
        class: StaticClassLiteral<'db>,
        generic_context: GenericContext<'db>,
    ) -> Result<(), Self::Error> {
        super::super::function::check_class_method_typevar_variance(
            self.context,
            class,
            generic_context,
        );
        Ok(())
    }

    fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.is_typed_dict(self.context.db()))
    }

    fn check_typed_dict(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        direct_typed_dict_bases: &[ClassType<'db>],
    ) -> Result<(), Self::Error> {
        super::super::typed_dict::validate_typed_dict_class(
            self.context,
            class,
            class_node,
            direct_typed_dict_bases,
        );
        Ok(())
    }

    fn check_members(
        &self,
        class: StaticClassLiteral<'db>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> Result<(), Self::Error> {
        class.validate_members(self.context, field_policy);
        Ok(())
    }
}
