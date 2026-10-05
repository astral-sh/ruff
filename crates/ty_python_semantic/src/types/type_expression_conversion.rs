use std::convert::Infallible;

use smallvec::smallvec_inline;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;

use crate::types::infer::InferenceFlags;
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, GenericContext, InvalidTypeExpression,
    InvalidTypeExpressionError, KnownClass, KnownInstanceType, KnownUnion, NominalInstanceType,
    Specialization, Type, UnionType, todo_type,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

pub(in crate::types) mod known_instance;
pub(in crate::types) mod special_form;

#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
pub enum TypeConversionOperation {
    SpecializationTypeVarLookup,
    ApplySpecialization,
    NumericUnion,
    Recursive,
    UnboundRecursiveVariable,
    KnownInstanceUnionResult,
    KnownInstanceMetaType,
    SpecialFormSelf,
    SpecialFormCallable,
    Union,
    TypeAlias,
    UnionAliasExpansion,
    SubclassUnion,
    SubclassIntersection,
    SubclassProtocol,
    MissingArgumentDefault,
    MissingArgumentDiagnostic,
}

pub(in crate::types) struct ConversionFacts;

pub(in crate::types) struct InlineConversion<'db> {
    pub(in crate::types) db: &'db dyn Db,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousDefaultTypeSpecializationEffects)]
    pub(in crate::types) trait DefaultTypeSpecializationEffects<'db> {
        type Error;
        #[operation(local)]
        async fn new_variables(&self) -> Result<FxOrderSet<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(child)]
        async fn collect(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>, variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn context(&self, env: &ProgramEnvironment<'db>, variables: FxOrderSet<BoundTypeVarInstance<'db>>) -> Result<GenericContext<'db>, Self::Error>;
        #[operation(child)]
        async fn defaults(&self, context: GenericContext<'db>) -> Result<Specialization<'db>, Self::Error>;
        #[operation(child)]
        async fn apply(&self, ty: Type<'db>, specialization: Specialization<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(default_specialize_sync)]
    #[capabilities(effects = DefaultTypeSpecializationEffects)]
    #[passive_values()]
    pub(in crate::types) async fn default_specialize_with<'db, E: DefaultTypeSpecializationEffects<'db>>(
        ty: Type<'db>, env: &ProgramEnvironment<'db>, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let mut variables = effects.new_variables().await?;
        effects.collect(env, ty, &mut variables).await?;
        let context = effects.context(env, variables).await?;
        let specialization = effects.defaults(context).await?;
        effects.apply(ty, specialization).await
    }

    #[synchronous(SynchronousTypeExpressionConversionEffects)]
    pub(in crate::types) trait TypeExpressionConversionEffects<'db> {
        type Error;
        #[operation(source)]
        async fn class_known(&self, class: ClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(child)]
        async fn numeric_union(&self, env: &ProgramEnvironment<'db>, union: KnownUnion) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn class_default(&self, class: ClassLiteral<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(child)]
        async fn static_class_default(&self, class: crate::types::StaticClassLiteral<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(child)]
        async fn instance(&self, env: &ProgramEnvironment<'db>, class: ClassType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn nominal_known(&self, instance: NominalInstanceType<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(child)]
        async fn none(&self, env: &ProgramEnvironment<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn recursive(&self, recursive: crate::types::RecursiveType<'db>, scope: ScopeId<'db>, binding: Option<Definition<'db>>, flags: InferenceFlags) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error>;
        #[operation(child)]
        async fn unbound_recursive(&self) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error>;
        #[operation(child)]
        async fn known_instance(&self, known: KnownInstanceType<'db>, scope: ScopeId<'db>, binding: Option<Definition<'db>>, flags: InferenceFlags) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error>;
        #[operation(child)]
        async fn special_form(&self, special_form: crate::types::SpecialFormType, scope: ScopeId<'db>, binding: Option<Definition<'db>>, flags: InferenceFlags) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error>;
        #[operation(child)]
        async fn union(&self, union: UnionType<'db>, scope: ScopeId<'db>, binding: Option<Definition<'db>>, flags: InferenceFlags) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error>;
        #[operation(child)]
        async fn alias(&self, alias: crate::types::TypeAliasType<'db>, scope: ScopeId<'db>, binding: Option<Definition<'db>>, flags: InferenceFlags) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error>;
    }

    #[finite_capability]
    impl ConversionFacts {
        fn static_class<'db>(&self, class: ClassLiteral<'db>) -> Option<crate::types::StaticClassLiteral<'db>> { class.as_static() }
        fn non_generic<'db>(&self, class: ClassLiteral<'db>) -> ClassType<'db> { ClassType::NonGeneric(class) }
        fn environment<'db>(&self, scope: ScopeId<'db>) -> ProgramEnvironment<'db> { ProgramEnvironment::from_scope(scope) }
        fn float_special_case(&self, flags: InferenceFlags) -> bool { !flags.contains(InferenceFlags::DISABLE_INT_FLOAT_SPECIAL_CASE) }
        fn generic_class<'db>(&self, alias: crate::types::GenericAlias<'db>) -> ClassType<'db> { ClassType::from(alias) }
        fn invalid<'db>(&self, ty: Type<'db>, scope: ScopeId<'db>) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
            Err(InvalidTypeExpressionError { invalid_expressions: smallvec_inline![InvalidTypeExpression::InvalidType(ty, scope)], fallback_type: Type::unknown() })
        }
        fn unrecognized_typevar<'db>(&self) -> Type<'db> { todo_type!("unrecognized `typing.TypeVar` instances should be invalid type expressions") }
        fn unrecognized_typevartuple<'db>(&self) -> Type<'db> { todo_type!("unrecognized `typing.TypeVarTuple` instances \
                        should be invalid type expressions") }
        fn intersection<'db>(&self) -> Type<'db> { todo_type!("Type::Intersection.in_type_expression") }
    }

    #[synchronous(conversion_class_default_sync)]
    #[capabilities(effects = TypeExpressionConversionEffects, facts = ConversionFacts)]
    #[passive_values()]
    pub(in crate::types) async fn conversion_class_default_with<'db, E: TypeExpressionConversionEffects<'db>>(
        class: ClassLiteral<'db>, facts: ConversionFacts, effects: &E,
    ) -> Result<ClassType<'db>, E::Error> {
        match facts.static_class(class) {
            Some(class) => effects.static_class_default(class).await,
            None => Ok(facts.non_generic(class)),
        }
    }

    #[synchronous(in_type_expression_sync)]
    #[capabilities(effects = TypeExpressionConversionEffects, facts = ConversionFacts)]
    #[passive_values(KnownUnion::Complex, KnownUnion::Float)]
    pub(in crate::types) async fn in_type_expression_with<'db, E: TypeExpressionConversionEffects<'db>>(
        ty: Type<'db>, scope: ScopeId<'db>, binding: Option<Definition<'db>>, flags: InferenceFlags, facts: ConversionFacts, effects: &E,
    ) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, E::Error> {
        let env = facts.environment(scope);
        match ty {
            Type::Recursive(recursive) => effects.recursive(recursive, scope, binding, flags).await,
            Type::RecursiveVar(_) => effects.unbound_recursive().await,
            // Special cases for `float` and `complex`
            // https://typing.python.org/en/latest/spec/special-types.html#special-cases-for-float-and-complex
            Type::ClassLiteral(class) => {
                let ty = match effects.class_known(class).await? {
                    Some(KnownClass::Complex) => effects.numeric_union(&env, KnownUnion::Complex).await?,
                    Some(KnownClass::Float) if facts.float_special_case(flags) => effects.numeric_union(&env, KnownUnion::Float).await?,
                    _ => {
                        let class = effects.class_default(class).await?;
                        effects.instance(&env, class).await?
                    }
                };
                Ok(Ok(ty))
            }
            Type::GenericAlias(alias) => {
                let class = facts.generic_class(alias);
                Ok(Ok(effects.instance(&env, class).await?))
            }
            Type::KnownInstance(known) => effects.known_instance(known, scope, binding, flags).await,
            Type::SpecialForm(special_form) => effects.special_form(special_form, scope, binding, flags).await,
            Type::Union(union) => effects.union(union, scope, binding, flags).await,
            Type::Dynamic(_) | Type::Divergent(_) => Ok(Ok(ty)),
            Type::NominalInstance(instance) => match effects.nominal_known(instance).await? {
                Some(KnownClass::NoneType) => Ok(Ok(effects.none(&env).await?)),
                // TODO: Emit an invalid-type-form diagnostic and recover to `Unknown` for
                // unrecognized `TypeVar` and `TypeVarTuple` instances.
                Some(KnownClass::TypeVar) => Ok(Ok(facts.unrecognized_typevar())),
                Some(KnownClass::TypeVarTuple | KnownClass::ExtensionsTypeVarTuple) => Ok(Ok(facts.unrecognized_typevartuple())),
                _ => Ok(facts.invalid(ty, scope)),
            },
            Type::Intersection(_) => Ok(Ok(facts.intersection())),
            Type::TypeAlias(alias) => effects.alias(alias, scope, binding, flags).await,
            Type::SubclassOf(_) | Type::EnumComplement(_) | Type::LiteralValue(_) | Type::AlwaysTruthy | Type::AlwaysFalsy | Type::ModuleLiteral(_) | Type::TypeVar(_) | Type::Callable(_) | Type::BoundMethod(_) | Type::WrapperDescriptor(_) | Type::KnownBoundMethod(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_) | Type::Never | Type::FunctionLiteral(_) | Type::BoundSuper(_) | Type::ProtocolInstance(_) | Type::PropertyInstance(_) | Type::SlotDescriptor(_) | Type::TypeIs(_) | Type::TypeGuard(_) | Type::TypeForm(_) | Type::TypedDict(_) | Type::NewTypeInstance(_) => Ok(facts.invalid(ty, scope)),
        }
    }

    #[synchronous(SynchronousSubclassArgumentEffects)]
    pub(in crate::types) trait SubclassArgumentEffects<'db> {
        type Error;
        #[operation(child)]
        async fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn union_has_aliases(&self, union: UnionType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn expand_union_aliases(&self, env: &ProgramEnvironment<'db>, union: UnionType<'db>) -> Result<Type<'db>, Self::Error>;
    }
    #[synchronous(normalize_subclass_argument_sync)]
    #[capabilities(effects = SubclassArgumentEffects)]
    #[passive_values()]
    pub(in crate::types) async fn normalize_subclass_argument_with<'db, E: SubclassArgumentEffects<'db>>(
        db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let _ = db;
        let ty = effects.resolve_alias(ty).await?;
        if let Type::Union(union) = ty
            && effects.union_has_aliases(union).await?
        {
            return effects.expand_union_aliases(env, union).await;
        }
        Ok(ty)
    }
}

impl<'db> SynchronousDefaultTypeSpecializationEffects<'db> for InlineConversion<'db> {
    type Error = Infallible;
    fn new_variables(&self) -> Result<FxOrderSet<BoundTypeVarInstance<'db>>, Self::Error> {
        Ok(FxOrderSet::default())
    }
    fn collect(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<(), Self::Error> {
        ty.find_legacy_typevars(self.db, env, None, variables);
        Ok(())
    }
    fn context(
        &self,
        env: &ProgramEnvironment<'db>,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(GenericContext::from_typevar_instances(
            self.db, env, variables,
        ))
    }
    fn defaults(&self, context: GenericContext<'db>) -> Result<Specialization<'db>, Self::Error> {
        Ok(context.default_specialization(self.db, None))
    }
    fn apply(
        &self,
        ty: Type<'db>,
        specialization: Specialization<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(ty.apply_specialization(self.db, specialization))
    }
}

impl<'db> SynchronousTypeExpressionConversionEffects<'db> for InlineConversion<'db> {
    type Error = Infallible;
    fn class_known(&self, class: ClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error> {
        Ok(class.known(self.db))
    }
    fn numeric_union(
        &self,
        env: &ProgramEnvironment<'db>,
        union: KnownUnion,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(union.to_type(self.db, env))
    }
    fn class_default(&self, class: ClassLiteral<'db>) -> Result<ClassType<'db>, Self::Error> {
        conversion_class_default_sync(class, ConversionFacts, self)
    }
    fn static_class_default(
        &self,
        class: crate::types::StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(class.default_specialization(self.db))
    }
    fn instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(Type::instance(self.db, env, class))
    }
    fn nominal_known(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(instance.known_class(self.db))
    }
    fn none(&self, env: &ProgramEnvironment<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::none(self.db, env))
    }
    fn recursive(
        &self,
        recursive: crate::types::RecursiveType<'db>,
        scope: ScopeId<'db>,
        binding: Option<Definition<'db>>,
        flags: InferenceFlags,
    ) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error> {
        Ok(Type::in_type_expression_recursive(
            self.db, recursive, scope, binding, flags,
        ))
    }
    fn unbound_recursive(
        &self,
    ) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error> {
        unreachable!("semantic operation on an unbound recursive variable")
    }
    fn known_instance(
        &self,
        known: KnownInstanceType<'db>,
        scope: ScopeId<'db>,
        binding: Option<Definition<'db>>,
        flags: InferenceFlags,
    ) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error> {
        known_instance::in_type_expression_known_instance_sync(
            known,
            scope,
            binding,
            flags,
            known_instance::KnownInstanceConversionFacts,
            self,
        )
    }
    fn special_form(
        &self,
        special_form: crate::types::SpecialFormType,
        scope: ScopeId<'db>,
        binding: Option<Definition<'db>>,
        flags: InferenceFlags,
    ) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error> {
        special_form::in_type_expression_special_form_sync(
            special_form,
            scope,
            binding,
            flags,
            special_form::SpecialFormConversionFacts,
            self,
        )
    }
    fn union(
        &self,
        union: UnionType<'db>,
        scope: ScopeId<'db>,
        binding: Option<Definition<'db>>,
        flags: InferenceFlags,
    ) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error> {
        Ok(Type::in_type_expression_union(
            self.db, &union, scope, binding, flags,
        ))
    }
    fn alias(
        &self,
        alias: crate::types::TypeAliasType<'db>,
        scope: ScopeId<'db>,
        binding: Option<Definition<'db>>,
        flags: InferenceFlags,
    ) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error> {
        Ok(alias
            .value_type(self.db)
            .in_type_expression_impl(self.db, scope, binding, flags))
    }
}

impl<'db> SynchronousSubclassArgumentEffects<'db> for InlineConversion<'db> {
    type Error = Infallible;
    fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.resolve_type_alias(self.db))
    }
    fn union_has_aliases(&self, union: UnionType<'db>) -> Result<bool, Self::Error> {
        Ok(union.elements(self.db).iter().any(|ty| ty.is_alias_like()))
    }
    fn expand_union_aliases(
        &self,
        env: &ProgramEnvironment<'db>,
        union: UnionType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(union.expand_aliases(self.db, env))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::task::Poll;

    use super::*;
    use crate::db::tests::setup_db;
    use crate::types::signatures::effects::try_poll_immediate;

    struct DefaultSequence<'db> {
        events: RefCell<Vec<&'static str>>,
        refuse: Option<&'static str>,
        context: GenericContext<'db>,
        specialization: Specialization<'db>,
    }

    impl DefaultSequence<'_> {
        fn record(&self, event: &'static str) -> Result<(), &'static str> {
            self.events.borrow_mut().push(event);
            if self.refuse == Some(event) {
                Err(event)
            } else {
                Ok(())
            }
        }
    }

    impl<'db> DefaultTypeSpecializationEffects<'db> for DefaultSequence<'db> {
        type Error = &'static str;
        async fn new_variables(
            &self,
        ) -> Result<FxOrderSet<BoundTypeVarInstance<'db>>, Self::Error> {
            self.record("allocate")?;
            Ok(FxOrderSet::default())
        }
        async fn collect(
            &self,
            _env: &ProgramEnvironment<'db>,
            ty: Type<'db>,
            variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        ) -> Result<(), Self::Error> {
            self.record("collect")?;
            assert_eq!(ty, Type::Never);
            assert!(variables.is_empty());
            Ok(())
        }
        async fn context(
            &self,
            _env: &ProgramEnvironment<'db>,
            variables: FxOrderSet<BoundTypeVarInstance<'db>>,
        ) -> Result<GenericContext<'db>, Self::Error> {
            self.record("context")?;
            assert!(variables.is_empty());
            Ok(self.context)
        }
        async fn defaults(
            &self,
            context: GenericContext<'db>,
        ) -> Result<Specialization<'db>, Self::Error> {
            self.record("defaults")?;
            assert_eq!(context, self.context);
            Ok(self.specialization)
        }
        async fn apply(
            &self,
            ty: Type<'db>,
            specialization: Specialization<'db>,
        ) -> Result<Type<'db>, Self::Error> {
            self.record("apply")?;
            assert_eq!(ty, Type::Never);
            assert_eq!(specialization, self.specialization);
            Ok(ty)
        }
    }

    #[test]
    fn empty_default_specialization_retains_every_dependency_and_refusal() {
        let db = setup_db();
        let env = db.program_environment();
        let context = GenericContext::from_typevar_instances(&db, &env, []);
        let specialization = context.default_specialization(&db, None);
        let sequence = ["allocate", "collect", "context", "defaults", "apply"];
        for refusal in [None, Some(0), Some(1), Some(2), Some(3), Some(4)] {
            let effects = DefaultSequence {
                events: RefCell::default(),
                refuse: refusal.map(|index| sequence[index]),
                context,
                specialization,
            };
            let actual = try_poll_immediate(default_specialize_with(Type::Never, &env, &effects));
            if let Some(index) = refusal {
                assert_eq!(actual, Poll::Ready(Err(sequence[index])));
                assert_eq!(*effects.events.borrow(), sequence[..=index]);
            } else {
                assert_eq!(actual, Poll::Ready(Ok(Type::Never)));
                assert_eq!(*effects.events.borrow(), sequence);
            }
        }
    }
}
