//! Class selection for instance types and their nominal fallbacks.

use std::convert::Infallible;

use crate::types::literal::{EnumLiteralType, LiteralFallback};
use crate::types::{
    BoundTypeVarInstance, ClassType, FunctionType, KnownClass, LiteralValueType, NewType,
    NominalInstanceType, PropertyInstanceType, ProtocolInstanceType, Specialization,
    StaticClassLiteral, Type, TypeVarBoundOrConstraints, TypedDictType,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct OrdinaryNominalSelection<'env, 'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'env ProgramEnvironment<'db>,
}

pub(in crate::types) struct LiteralFallbackFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousLiteralFallbackEffects)]
    pub(in crate::types) trait LiteralFallbackEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn known_instance(&self, env: &ProgramEnvironment<'db>, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn function_runtime_class(&self, function: FunctionType<'db>) -> Result<KnownClass, Self::Error>;
        #[operation(child)]
        async fn enum_instance(&self, env: &ProgramEnvironment<'db>, literal: EnumLiteralType<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl LiteralFallbackFacts {
        fn target<'db>(&self, literal: LiteralValueType<'db>) -> LiteralFallback<'db> {
            literal.fallback_target()
        }
    }

    #[synchronous(literal_fallback_instance_sync)]
    #[capabilities(effects = LiteralFallbackEffects, facts = LiteralFallbackFacts)]
    #[passive_values(KnownClass::ModuleType, LiteralFallback::Scalar, LiteralFallback::Enum)]
    pub(in crate::types) async fn literal_fallback_instance_with<'db, E: LiteralFallbackEffects<'db>>(
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        facts: LiteralFallbackFacts,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        effects.checkpoint().await?;
        let instance = match ty {
            Type::ModuleLiteral(_) => effects.known_instance(env, KnownClass::ModuleType).await?,
            Type::FunctionLiteral(function) => {
                let class = effects.function_runtime_class(function).await?;
                effects.known_instance(env, class).await?
            }
            Type::LiteralValue(literal) => match facts.target(literal) {
                LiteralFallback::Scalar(class) => effects.known_instance(env, class).await?,
                LiteralFallback::Enum(literal) => effects.enum_instance(env, literal).await?,
            },
            _ => return Ok(None),
        };
        Ok(Some(instance))
    }

    #[synchronous(SynchronousLiteralMetaTypeEffects)]
    pub(in crate::types) trait LiteralMetaTypeEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn known_class_literal(&self, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn enum_class_literal(&self, literal: EnumLiteralType<'db>) -> Result<Type<'db>, Self::Error>;
    }

    /// Returns the exact runtime class of a literal value, preserving its enum class when present.
    /// Known-class lookup retains its usual `Unknown` fallback when the class is unavailable.
    #[synchronous(literal_meta_type_sync)]
    #[capabilities(effects = LiteralMetaTypeEffects, facts = LiteralFallbackFacts)]
    #[passive_values(LiteralFallback::Scalar, LiteralFallback::Enum)]
    pub(in crate::types) async fn literal_meta_type_with<'db, E: LiteralMetaTypeEffects<'db>>(
        literal: LiteralValueType<'db>,
        facts: LiteralFallbackFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.checkpoint().await?;
        match facts.target(literal) {
            LiteralFallback::Scalar(class) => effects.known_class_literal(class).await,
            LiteralFallback::Enum(literal) => effects.enum_class_literal(literal).await,
        }
    }

    #[synchronous(SynchronousNominalSelectionEffects)]
    pub(in crate::types) trait NominalSelectionEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_type(&self, current: &mut Option<Type<'db>>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn nominal_class(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(local)]
        async fn typed_dict_class(&self, typed_dict: TypedDictType<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(child)]
        async fn instance_class(&self, instance: NominalInstanceType<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(source)]
        async fn protocol_class(&self, instance: ProtocolInstanceType<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(child)]
        async fn newtype_base(&self, newtype: NewType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn typevar_bound(&self, typevar: BoundTypeVarInstance<'db>) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error>;
        #[operation(child)]
        async fn literal_fallback(&self, literal: LiteralValueType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn property_fallback(&self, property: PropertyInstanceType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn known_instance(&self, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn static_class_literal(&self, class: ClassType<'db>) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error>;
    }

    #[synchronous(class_specialization_sync)]
    #[capabilities(effects = NominalSelectionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn class_specialization_with<'db, E: NominalSelectionEffects<'db>>(
        ty: Type<'db>,
        effects: &E,
    ) -> Result<Option<(StaticClassLiteral<'db>, Specialization<'db>)>, E::Error> {
        effects.checkpoint().await?;
        let class = match ty {
            Type::TypedDict(typed_dict) => effects.typed_dict_class(typed_dict).await?,
            _ => effects.nominal_class(ty).await?,
        };
        let Some(class) = class else {
            return Ok(None);
        };
        match effects.static_class_literal(class).await? {
            Some((class_literal, Some(specialization))) => Ok(Some((class_literal, specialization))),
            _ => Ok(None),
        }
    }

    #[synchronous(nominal_class_sync)]
    #[capabilities(effects = NominalSelectionEffects)]
    #[passive_values(KnownClass::MemberDescriptorType)]
    pub(in crate::types) async fn nominal_class_with<'db, E: NominalSelectionEffects<'db>>(
        ty: Type<'db>,
        effects: &E,
    ) -> Result<Option<ClassType<'db>>, E::Error> {
        #[passive_state]
        let mut current = Some(ty);
        #[cursor_loop]
        while let Some(ty) = effects.next_type(&mut current).await? {
            match effects.resolve_alias(ty).await? {
                Type::NominalInstance(instance) => {
                    return Ok(Some(effects.instance_class(instance).await?));
                }
                Type::ProtocolInstance(instance) => return effects.protocol_class(instance).await,
                Type::NewTypeInstance(newtype) => {
                    current = Some(effects.newtype_base(newtype).await?);
                }
                Type::TypeVar(typevar) => {
                    let Some(TypeVarBoundOrConstraints::UpperBound(bound)) =
                        effects.typevar_bound(typevar).await?
                    else {
                        return Ok(None);
                    };
                    current = Some(bound);
                }
                Type::LiteralValue(literal) => {
                    current = Some(effects.literal_fallback(literal).await?);
                }
                Type::PropertyInstance(property) => {
                    current = Some(effects.property_fallback(property).await?);
                }
                Type::SlotDescriptor(_) => {
                    current = Some(effects.known_instance(KnownClass::MemberDescriptorType).await?);
                }
                _ => return Ok(None),
            }
        }
        Ok(None)
    }
}

impl<'db> SynchronousLiteralFallbackEffects<'db> for OrdinaryNominalSelection<'_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn known_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_instance(self.db, env))
    }

    fn function_runtime_class(
        &self,
        function: FunctionType<'db>,
    ) -> Result<KnownClass, Self::Error> {
        Ok(function.runtime_class(self.db))
    }

    fn enum_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        literal: EnumLiteralType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(literal.enum_class_instance(self.db, env))
    }
}

impl<'db> SynchronousLiteralMetaTypeEffects<'db> for OrdinaryNominalSelection<'_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn known_class_literal(&self, class: KnownClass) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_class_literal(self.db, self.env))
    }

    fn enum_class_literal(&self, literal: EnumLiteralType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::ClassLiteral(literal.enum_class(self.db)))
    }
}

impl<'db> SynchronousNominalSelectionEffects<'db> for OrdinaryNominalSelection<'_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn next_type(&self, current: &mut Option<Type<'db>>) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(current.take())
    }

    fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.resolve_type_alias(self.db))
    }

    fn nominal_class(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error> {
        nominal_class_sync(ty, self)
    }

    fn typed_dict_class(
        &self,
        typed_dict: TypedDictType<'db>,
    ) -> Result<Option<ClassType<'db>>, Self::Error> {
        Ok(typed_dict.defining_class())
    }

    fn instance_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(instance.class(self.db, self.env))
    }

    fn protocol_class(
        &self,
        instance: ProtocolInstanceType<'db>,
    ) -> Result<Option<ClassType<'db>>, Self::Error> {
        Ok(instance.class_origin(self.db).map(|class| *class))
    }

    fn newtype_base(&self, newtype: NewType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(newtype.concrete_base_type(self.db))
    }

    fn typevar_bound(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error> {
        Ok(typevar
            .typevar(self.db)
            .bound_or_constraints(self.db, self.env))
    }

    fn literal_fallback(&self, literal: LiteralValueType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(literal.fallback_instance(self.db, self.env))
    }

    fn property_fallback(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(property.instance_fallback(self.db, self.env))
    }

    fn known_instance(&self, class: KnownClass) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_instance(self.db, self.env))
    }

    fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error> {
        Ok(class.static_class_literal(self.db))
    }
}

#[cfg(test)]
mod tests {
    use std::any::type_name;
    use std::cell::RefCell;
    use std::task::Poll;

    use ruff_db::files::system_path_to_file;
    use ruff_python_ast::name::Name;
    use salsa::execution_probe::FieldRequest;
    use ty_python_core::ProgramFile;

    use super::*;
    use crate::db::tests::{TestDb, TestDbBuilder};
    use crate::place::{ConsideredDefinitions, global_symbol, symbol};
    use crate::types::callable::CallableTypeKind;
    use crate::types::callable::conversion::FunctionConversionEffects;
    use crate::types::function::{
        FunctionDecorators, FunctionLiteral, FunctionMetadataEffects,
        LegacyFunctionIdentityEffects, OverloadLiteral, identity_sealed,
    };
    use crate::types::signatures::effects::try_poll_immediate;
    use crate::types::{CallableSignature, CallableType, ClassLiteral};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Operation<'db> {
        Checkpoint,
        Known(KnownClass),
        Function(FunctionType<'db>),
        Enum(EnumLiteralType<'db>),
    }

    struct ObservedFallback<'env, 'db> {
        ordinary: OrdinaryNominalSelection<'env, 'db>,
        operations: RefCell<Vec<Operation<'db>>>,
        refuse: Option<Operation<'db>>,
        function_class: Option<KnownClass>,
    }

    impl<'db> ObservedFallback<'_, 'db> {
        fn record(&self, operation: Operation<'db>) -> Result<(), Operation<'db>> {
            self.operations.borrow_mut().push(operation);
            if self.refuse == Some(operation) {
                Err(operation)
            } else {
                Ok(())
            }
        }
    }

    impl<'db> LiteralFallbackEffects<'db> for ObservedFallback<'_, 'db> {
        type Error = Operation<'db>;

        async fn checkpoint(&self) -> Result<(), Self::Error> {
            self.record(Operation::Checkpoint)
        }

        async fn known_instance(
            &self,
            env: &ProgramEnvironment<'db>,
            class: KnownClass,
        ) -> Result<Type<'db>, Self::Error> {
            assert!(std::ptr::eq(env, self.ordinary.env));
            self.record(Operation::Known(class))?;
            SynchronousLiteralFallbackEffects::known_instance(&self.ordinary, env, class)
                .map_err(|never| match never {})
        }

        async fn function_runtime_class(
            &self,
            function: FunctionType<'db>,
        ) -> Result<KnownClass, Self::Error> {
            self.record(Operation::Function(function))?;
            if let Some(class) = self.function_class {
                return Ok(class);
            }
            self.ordinary
                .function_runtime_class(function)
                .map_err(|never| match never {})
        }

        async fn enum_instance(
            &self,
            env: &ProgramEnvironment<'db>,
            literal: EnumLiteralType<'db>,
        ) -> Result<Type<'db>, Self::Error> {
            assert!(std::ptr::eq(env, self.ordinary.env));
            self.record(Operation::Enum(literal))?;
            self.ordinary
                .enum_instance(env, literal)
                .map_err(|never| match never {})
        }
    }

    #[test]
    fn literal_fallback_preserves_dispatch_environment_and_refusal_order() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file(
                "/src/literal_fallback.py",
                "import math\nfrom enum import Enum\nfrom typing import Literal\nclass Choice(Enum):\n    A = 1\n    B = 2\nmember: Literal[Choice.A] = Choice.A\ndef function(): ...\n",
            )
            .build()?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/literal_fallback.py")?,
            env.program(&db),
        );
        let symbol = |name| global_symbol(&db, file, name).place.expect_type();
        let Type::FunctionLiteral(function) = symbol("function") else {
            anyhow::bail!("function fixture must be a function literal");
        };
        let Some(member) = symbol("member").as_enum_literal() else {
            anyhow::bail!("member fixture must be an enum literal");
        };
        let module = symbol("math");
        assert!(matches!(module, Type::ModuleLiteral(_)));
        let mut cases = vec![
            (
                Type::FunctionLiteral(function),
                Some(KnownClass::FunctionType.to_instance(&db, &env)),
                vec![
                    Operation::Function(function),
                    Operation::Known(KnownClass::FunctionType),
                ],
            ),
            (
                Type::enum_literal(member),
                Some(member.enum_class_instance(&db, &env)),
                vec![Operation::Enum(member)],
            ),
        ];
        for (ty, class) in [
            (module, KnownClass::ModuleType),
            (Type::int_literal(1), KnownClass::Int),
            (Type::bool_literal(true), KnownClass::Bool),
            (Type::bool_literal(false), KnownClass::Bool),
            (Type::string_literal(&db, "value"), KnownClass::Str),
            (Type::literal_string(), KnownClass::Str),
            (Type::bytes_literal(&db, b"value"), KnownClass::Bytes),
        ] {
            cases.push((
                ty,
                Some(class.to_instance(&db, &env)),
                vec![Operation::Known(class)],
            ));
        }
        for ty in [Type::Never, Type::any(), Type::object(), symbol("Choice")] {
            cases.push((ty, None, Vec::new()));
        }

        for (ty, expected, mut operations) in cases {
            operations.insert(0, Operation::Checkpoint);
            let mut effects = ObservedFallback {
                ordinary: OrdinaryNominalSelection { db: &db, env: &env },
                operations: RefCell::default(),
                refuse: None,
                function_class: None,
            };
            assert_eq!(ty.literal_fallback_instance(&db, &env), expected);
            assert_eq!(
                try_poll_immediate(literal_fallback_instance_with(
                    ty,
                    &env,
                    LiteralFallbackFacts,
                    &effects
                )),
                Poll::Ready(Ok(expected)),
            );
            assert_eq!(*effects.operations.borrow(), operations);
            for (index, operation) in operations.iter().copied().enumerate() {
                effects.refuse = Some(operation);
                effects.operations.borrow_mut().clear();
                assert_eq!(
                    try_poll_immediate(literal_fallback_instance_with(
                        ty,
                        &env,
                        LiteralFallbackFacts,
                        &effects
                    )),
                    Poll::Ready(Err(operation)),
                );
                assert_eq!(*effects.operations.borrow(), operations[..=index]);
            }
            if let Type::FunctionLiteral(function) = ty {
                effects.refuse = None;
                for class in [KnownClass::Classmethod, KnownClass::Staticmethod] {
                    effects.function_class = Some(class);
                    effects.operations.borrow_mut().clear();
                    assert_eq!(
                        try_poll_immediate(literal_fallback_instance_with(
                            ty,
                            &env,
                            LiteralFallbackFacts,
                            &effects
                        )),
                        Poll::Ready(Ok(Some(class.to_instance(&db, &env)))),
                    );
                    assert_eq!(
                        *effects.operations.borrow(),
                        [
                            Operation::Checkpoint,
                            Operation::Function(function),
                            Operation::Known(class),
                        ]
                    );
                }
            }
        }
        Ok(())
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FunctionOperation<'db> {
        DescriptorKind,
        Literal,
        Decorators,
        Name,
        Metadata(OverloadLiteral<'db>),
        Local(Option<usize>),
        Signature,
        Callable,
    }

    #[derive(Default)]
    struct ObservedFunction<'db> {
        operations: RefCell<Vec<FunctionOperation<'db>>>,
        empty_metadata: bool,
    }

    impl identity_sealed::Sealed for ObservedFunction<'_> {}

    impl<'db> FunctionMetadataEffects<'db> for ObservedFunction<'db> {
        type Error = &'static str;

        async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
            let output = type_name::<R::Output>();
            let operation = if output == type_name::<Option<CallableTypeKind>>() {
                FunctionOperation::DescriptorKind
            } else if output == type_name::<FunctionLiteral<'db>>() {
                FunctionOperation::Literal
            } else if output == type_name::<FunctionDecorators>() {
                FunctionOperation::Decorators
            } else if output == type_name::<&Name>() {
                FunctionOperation::Name
            } else {
                return Err("runtime-class selection requested an unrelated field");
            };
            self.operations.borrow_mut().push(operation);
            Ok(request.read_ordinary())
        }

        async fn overloads_and_implementation(
            &self,
            db: &'db dyn Db,
            last_definition: OverloadLiteral<'db>,
        ) -> Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), Self::Error>
        {
            self.operations
                .borrow_mut()
                .push(FunctionOperation::Metadata(last_definition));
            if self.empty_metadata {
                return Ok((&[], None));
            }
            LegacyFunctionIdentityEffects
                .overloads_and_implementation(db, last_definition)
                .await
                .map_err(|never| match never {})
        }
    }

    impl<'db> FunctionConversionEffects<'db> for ObservedFunction<'db> {
        async fn local<T>(
            &self,
            work: Option<usize>,
            action: impl FnOnce() -> T,
        ) -> Result<T, Self::Error> {
            self.operations
                .borrow_mut()
                .push(FunctionOperation::Local(work));
            Ok(action())
        }

        async fn signature(
            &self,
            _db: &'db dyn Db,
            _function: FunctionType<'db>,
        ) -> Result<&'db CallableSignature<'db>, Self::Error> {
            self.operations
                .borrow_mut()
                .push(FunctionOperation::Signature);
            Err("runtime-class selection must not infer signatures")
        }

        async fn callable(
            &self,
            _db: &'db dyn Db,
            _signatures: &'db CallableSignature<'db>,
            _kind: CallableTypeKind,
        ) -> Result<CallableType<'db>, Self::Error> {
            self.operations
                .borrow_mut()
                .push(FunctionOperation::Callable);
            Err("runtime-class selection must not intern callables")
        }
    }

    fn function_database() -> anyhow::Result<TestDb> {
        TestDbBuilder::new()
            .with_file(
                "/src/runtime_class.py",
                r#"from typing import overload

class Methods:
    def plain(self): ...
    @classmethod
    def class_method(cls): ...
    @staticmethod
    def static_method(): ...
    def __new__(cls): ...
    def __init_subclass__(cls): ...
    def __class_getitem__(cls, item): ...
    @classmethod
    @staticmethod
    def both(cls): ...

    @overload
    @classmethod
    def class_overloads(cls, value: int) -> int: ...
    @overload
    @classmethod
    def class_overloads(cls, value: str) -> str: ...
    @classmethod
    def class_overloads(cls, value): ...

    @overload
    @staticmethod
    def static_overloads(value: int) -> int: ...
    @overload
    @staticmethod
    def static_overloads(value: str) -> str: ...
    @staticmethod
    def static_overloads(value): ...

    @overload
    @classmethod
    def mixed_overloads(cls, value: int) -> int: ...
    @overload
    @staticmethod
    def mixed_overloads(value: str) -> str: ...
    @classmethod
    def mixed_overloads(cls, value): ...

    @overload
    @classmethod
    def mixed_implementation(cls, value: int) -> int: ...
    @overload
    @classmethod
    def mixed_implementation(cls, value: str) -> str: ...
    def mixed_implementation(self, value): ...

    @overload
    @classmethod
    def no_implementation(cls, value: int) -> int: ...
    @overload
    @classmethod
    def no_implementation(cls, value: str) -> str: ...
"#,
            )
            .build()
    }

    fn declared_function<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<FunctionType<'db>> {
        let env = db.program_environment();
        let file = ProgramFile::new(
            db,
            system_path_to_file(db, "/src/runtime_class.py")?,
            env.program(db),
        );
        let Type::ClassLiteral(ClassLiteral::Static(class)) =
            global_symbol(db, file, "Methods").place.expect_type()
        else {
            anyhow::bail!("Methods fixture must be a static class");
        };
        let Type::FunctionLiteral(function) = symbol(
            db,
            class.body_scope(db),
            name,
            ConsideredDefinitions::AllReachable,
        )
        .place
        .expect_type() else {
            anyhow::bail!("{name} fixture must be a function literal");
        };
        Ok(FunctionType::new(db, function.literal(db), None))
    }

    #[test]
    fn runtime_class_uses_real_declarations_in_metadata_order() -> anyhow::Result<()> {
        let db = function_database()?;
        let decorators = FunctionOperation::Decorators;
        let name = FunctionOperation::Name;
        for (method, expected, metadata, reads) in [
            (
                "plain",
                KnownClass::FunctionType,
                None,
                vec![decorators, name, decorators, name],
            ),
            (
                "class_method",
                KnownClass::Classmethod,
                None,
                vec![decorators],
            ),
            (
                "static_method",
                KnownClass::Staticmethod,
                None,
                vec![decorators, name, decorators],
            ),
            (
                "__new__",
                KnownClass::Staticmethod,
                None,
                vec![decorators, name, decorators, name],
            ),
            (
                "__init_subclass__",
                KnownClass::Classmethod,
                None,
                vec![decorators, name],
            ),
            (
                "__class_getitem__",
                KnownClass::Classmethod,
                None,
                vec![decorators, name],
            ),
            ("both", KnownClass::Classmethod, None, vec![decorators]),
            (
                "class_overloads",
                KnownClass::Classmethod,
                Some((2, true)),
                vec![decorators, decorators, decorators],
            ),
            (
                "static_overloads",
                KnownClass::Staticmethod,
                Some((2, true)),
                vec![decorators, name, decorators, decorators, decorators],
            ),
            (
                "mixed_overloads",
                KnownClass::FunctionType,
                Some((2, true)),
                vec![decorators, decorators, name, decorators, name],
            ),
            (
                "mixed_implementation",
                KnownClass::FunctionType,
                Some((2, true)),
                vec![decorators, decorators, decorators, name, decorators, name],
            ),
            (
                "no_implementation",
                KnownClass::Classmethod,
                Some((2, false)),
                vec![decorators, decorators],
            ),
        ] {
            let function = declared_function(&db, method)?;
            let mut operations = vec![
                FunctionOperation::DescriptorKind,
                FunctionOperation::Literal,
            ];
            if let Some((count, implemented)) = metadata {
                let (overloads, implementation) = function.overloads_and_implementation(&db);
                assert_eq!(
                    (overloads.len(), implementation.is_some()),
                    (count, implemented)
                );
                operations.push(FunctionOperation::Metadata(
                    function.literal(&db).last_definition,
                ));
                operations.push(FunctionOperation::Local(Some((count + 2) * 2)));
            } else {
                operations.push(FunctionOperation::Local(Some(4)));
            }
            operations.extend(reads);
            let original = if function.is_classmethod(&db) {
                KnownClass::Classmethod
            } else if function.is_staticmethod(&db) {
                KnownClass::Staticmethod
            } else {
                KnownClass::FunctionType
            };
            assert_eq!(original, expected, "{method}");
            assert_eq!(function.runtime_class(&db), expected, "{method}");
            let effects = ObservedFunction::default();
            assert_eq!(
                try_poll_immediate(function.runtime_class_with(&db, &effects)),
                Poll::Ready(Ok(expected)),
                "{method}"
            );
            assert_eq!(*effects.operations.borrow(), operations, "{method}");
        }
        Ok(())
    }

    #[test]
    fn runtime_class_descriptor_overrides_skip_overload_discovery() -> anyhow::Result<()> {
        let db = function_database()?;
        for (kind, expected) in [
            (CallableTypeKind::ClassMethodLike, KnownClass::Classmethod),
            (CallableTypeKind::StaticMethodLike, KnownClass::Staticmethod),
            (CallableTypeKind::FunctionLike, KnownClass::FunctionType),
            (CallableTypeKind::Regular, KnownClass::FunctionType),
            (CallableTypeKind::DunderParamSpec, KnownClass::FunctionType),
            (CallableTypeKind::ParamSpecValue, KnownClass::FunctionType),
        ] {
            let method = if kind == CallableTypeKind::ClassMethodLike {
                "static_overloads"
            } else {
                "class_overloads"
            };
            let function = declared_function(&db, method)?.with_descriptor_kind(&db, kind);
            let effects = ObservedFunction {
                empty_metadata: true,
                ..ObservedFunction::default()
            };
            assert_eq!(function.runtime_class(&db), expected);
            assert_eq!(
                try_poll_immediate(function.runtime_class_with(&db, &effects)),
                Poll::Ready(Ok(expected))
            );
            assert_eq!(
                *effects.operations.borrow(),
                [FunctionOperation::DescriptorKind]
            );
        }
        Ok(())
    }

    #[test]
    fn runtime_class_empty_recovery_metadata_retains_the_function_class() -> anyhow::Result<()> {
        let db = function_database()?;
        let function = declared_function(&db, "class_overloads")?;
        let effects = ObservedFunction {
            empty_metadata: true,
            ..ObservedFunction::default()
        };
        assert_eq!(
            try_poll_immediate(function.runtime_class_with(&db, &effects)),
            Poll::Ready(Ok(KnownClass::FunctionType))
        );
        assert_eq!(
            *effects.operations.borrow(),
            [
                FunctionOperation::DescriptorKind,
                FunctionOperation::Literal,
                FunctionOperation::Metadata(function.literal(&db).last_definition),
                FunctionOperation::Local(Some(4)),
            ]
        );
        Ok(())
    }
}
