//! Ordered code-generator selection for statement-defined classes.

use std::convert::Infallible;

use ty_python_core::scope::ScopeId;

use super::CodeGeneratorKind;
use super::member_source::InlineMemberSourceEffects;
use super::metaclass_selection::MetaclassSelectionResult;
use crate::ProgramEnvironment;
use crate::types::mro::MroIterator;
use crate::types::{
    ClassBase, ClassLiteral, ClassType, DataclassParams, DataclassTransformerParams, KnownClass,
    SpecialFormType, Specialization, StaticClassLiteral, Type,
};

pub(in crate::types) struct CodeGeneratorFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousCodeGeneratorEffects)]
    pub(in crate::types) trait CodeGeneratorEffects<'db> {
        type Error;
        type MroCursor;
        type ExplicitBasesCursor;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn body_scope(&self, class: StaticClassLiteral<'db>) -> Result<ScopeId<'db>, Self::Error>;
        #[operation(source)]
        async fn dataclass_params(&self, class: StaticClassLiteral<'db>) -> Result<Option<DataclassParams<'db>>, Self::Error>;
        #[operation(child)]
        async fn try_metaclass(&self, class: StaticClassLiteral<'db>) -> Result<MetaclassSelectionResult<'db>, Self::Error>;
        #[operation(child)]
        async fn known_type_class(&self, env: &ProgramEnvironment<'db>) -> Result<Option<StaticClassLiteral<'db>>, Self::Error>;
        #[operation(local)]
        async fn start_mro(&self, class: StaticClassLiteral<'db>) -> Result<Self::MroCursor, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_mro_base(&self, cursor: &mut Self::MroCursor) -> Result<Option<ClassBase<'db>>, Self::Error>;
        #[operation(source)]
        async fn static_class_literal(&self, class: ClassType<'db>) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error>;
        #[operation(source)]
        async fn dataclass_transformer_params(&self, class: StaticClassLiteral<'db>) -> Result<Option<DataclassTransformerParams<'db>>, Self::Error>;
        #[operation(child)]
        async fn dataclass_transformer_kind(&self, class: StaticClassLiteral<'db>, params: DataclassTransformerParams<'db>) -> Result<CodeGeneratorKind<'db>, Self::Error>;
        #[operation(child)]
        async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<Self::ExplicitBasesCursor, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_explicit_base(&self, cursor: &mut Self::ExplicitBasesCursor) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    }

    #[finite_capability]
    impl CodeGeneratorFacts {
        fn environment<'db>(&self, scope: ScopeId<'db>) -> ProgramEnvironment<'db> {
            ProgramEnvironment::from_scope(scope)
        }

        fn has_dataclass_params(&self, params: Option<DataclassParams<'_>>) -> bool {
            params.is_some()
        }

        fn is_type_base<'db>(&self, base: ClassBase<'db>, type_class: StaticClassLiteral<'db>) -> bool {
            base == ClassBase::Class(ClassType::NonGeneric(ClassLiteral::Static(type_class)))
        }
    }

    #[synchronous(code_generator_of_static_class_sync)]
    #[capabilities(effects = CodeGeneratorEffects, facts = CodeGeneratorFacts)]
    #[passive_values(CodeGeneratorKind::DataclassLike, CodeGeneratorKind::NamedTuple, CodeGeneratorKind::TypedDict)]
    pub(in crate::types) async fn code_generator_of_static_class_with<'db, E: CodeGeneratorEffects<'db>>(
        class: StaticClassLiteral<'db>,
        facts: CodeGeneratorFacts,
        effects: &E,
    ) -> Result<Option<CodeGeneratorKind<'db>>, E::Error> {
        effects.checkpoint().await?;
        let scope = effects.body_scope(class).await?;
        let env = facts.environment(scope);
        // If a class is directly decorated as a dataclass, it's a dataclass.
        // If a class' metaclass is a dataclass transformer, it's a dataclass.
        // If a class inherits from a base class that is a dataclass
        // transformer, it's a dataclass (unless it is a subclass of `type`,
        // in which case we assume the subclass is itself also meant for use
        // as a metaclass dataclass transformer, not itself supposed to be a
        // dataclass.)
        if facts.has_dataclass_params(effects.dataclass_params(class).await?) {
            return Ok(Some(CodeGeneratorKind::DataclassLike(None)));
        }
        if let Ok((_, Some(info))) = effects.try_metaclass(class).await? {
            return Ok(Some(effects.dataclass_transformer_kind(class, info.params).await?));
        }

        #[passive_state]
        let mut inherits_type = false;
        if let Some(type_class) = effects.known_type_class(&env).await? {
            let mut cursor = effects.start_mro(class).await?;
            #[cursor_loop]
            while let Some(base) = effects.next_mro_base(&mut cursor).await? {
                if facts.is_type_base(base, type_class) {
                    inherits_type = true;
                    break;
                }
            }
        }
        if !inherits_type {
            let mut cursor = effects.start_mro(class).await?;
            let _ = effects.next_mro_base(&mut cursor).await?;
            #[cursor_loop]
            while let Some(base) = effects.next_mro_base(&mut cursor).await? {
                if let ClassBase::Class(base) = base
                    && let Some((base, _)) = effects.static_class_literal(base).await?
                    && let Some(params) = effects.dataclass_transformer_params(base).await?
                {
                    return Ok(Some(effects.dataclass_transformer_kind(class, params).await?));
                }
            }
        }

        let mut bases = effects.explicit_bases(class).await?;
        #[cursor_loop]
        while let Some(base) = effects.next_explicit_base(&mut bases).await? {
            if let Type::SpecialForm(SpecialFormType::NamedTuple) = base {
                return Ok(Some(CodeGeneratorKind::NamedTuple));
            }
        }
        if effects.is_typed_dict(class).await? {
            Ok(Some(CodeGeneratorKind::TypedDict))
        } else {
            Ok(None)
        }
    }
}

impl<'db> SynchronousCodeGeneratorEffects<'db> for InlineMemberSourceEffects<'db> {
    type Error = Infallible;
    type MroCursor = MroIterator<'db>;
    type ExplicitBasesCursor = std::iter::Copied<std::slice::Iter<'db, Type<'db>>>;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn body_scope(&self, class: StaticClassLiteral<'db>) -> Result<ScopeId<'db>, Infallible> {
        Ok(class.body_scope(self.db))
    }

    fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<DataclassParams<'db>>, Infallible> {
        Ok(class.dataclass_params(self.db))
    }

    fn try_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<MetaclassSelectionResult<'db>, Infallible> {
        Ok(class.try_metaclass(self.db))
    }

    fn known_type_class(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<StaticClassLiteral<'db>>, Infallible> {
        Ok(KnownClass::Type.try_to_class_literal(self.db, env))
    }

    fn start_mro(&self, class: StaticClassLiteral<'db>) -> Result<Self::MroCursor, Infallible> {
        Ok(class.iter_mro(self.db, None))
    }

    fn next_mro_base(
        &self,
        cursor: &mut Self::MroCursor,
    ) -> Result<Option<ClassBase<'db>>, Infallible> {
        Ok(cursor.next())
    }

    fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Infallible> {
        Ok(class.static_class_literal(self.db))
    }

    fn dataclass_transformer_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<DataclassTransformerParams<'db>>, Infallible> {
        Ok(class.dataclass_transformer_params(self.db))
    }

    fn dataclass_transformer_kind(
        &self,
        class: StaticClassLiteral<'db>,
        params: DataclassTransformerParams<'db>,
    ) -> Result<CodeGeneratorKind<'db>, Infallible> {
        Ok(CodeGeneratorKind::from_dataclass_transformer(
            self.db, class, params,
        ))
    }

    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Self::ExplicitBasesCursor, Infallible> {
        Ok(class.explicit_bases(self.db).iter().copied())
    }

    fn next_explicit_base(
        &self,
        cursor: &mut Self::ExplicitBasesCursor,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(cursor.next())
    }

    fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.is_typed_dict(self.db))
    }
}
