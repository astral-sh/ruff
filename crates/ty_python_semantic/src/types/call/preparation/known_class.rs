//! Build the special call signatures used for known class literals and native wrapper descriptors.
//!
//! The shared match keeps parameter and overload order. `KnownClassBindingEffects` supplies
//! canonical types and owns parameter-list and binding storage; the finite helpers only assemble
//! scalar fields and fixed-size arrays or move completed parameter lists into signatures.

use std::convert::Infallible;

use itertools::Either;
use ruff_python_ast::name::Name;
use ty_mapping_probe_macros::shared_semantic_family;

use crate::types::call::{Binding, Bindings, CallableBinding};
use crate::types::signatures::{ConcatenateTail, Parameter, Parameters, Signature};
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, GenericContext, KnownClass, Type, TypeFormType,
    TypeVarVariance, UnionType, WrapperDescriptorKind,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct KnownClassBindingFacts;

pub(in crate::types) enum WrapperDescriptorSignatures<'db> {
    Overloaded([Signature<'db>; 2]),
    Single(Signature<'db>),
}

shared_semantic_family! {
    #[synchronous(SynchronousKnownClassBindingEffects)]
    pub(in crate::types) trait KnownClassBindingEffects<'db> {
        type Error;

        #[operation(source)]
        async fn known(&self, class: ClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(child)]
        async fn instance(&self, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn subclass(&self, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn specialized_instance<const N: usize>(&self, class: KnownClass, arguments: [Type<'db>; N]) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn type_form(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn union_two(&self, left: Type<'db>, right: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn union<const N: usize>(&self, elements: [Type<'db>; N]) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn homogeneous_tuple(&self, element: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn empty_tuple(&self) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn synthetic_typevar(&self, name: &'static str, variance: TypeVarVariance) -> Result<BoundTypeVarInstance<'db>, Self::Error>;
        #[operation(child)]
        async fn generic_context<const N: usize>(&self, variables: [BoundTypeVarInstance<'db>; N]) -> Result<GenericContext<'db>, Self::Error>;
        #[operation(local)]
        async fn standard_parameters<const N: usize>(&self, parameters: [Parameter<'db>; N]) -> Result<Parameters<'db>, Self::Error>;
        #[operation(local)]
        async fn empty_parameters(&self) -> Result<Parameters<'db>, Self::Error>;
        #[operation(local)]
        async fn gradual_parameters(&self) -> Result<Parameters<'db>, Self::Error>;
        #[operation(local)]
        async fn concatenate_gradual<const N: usize>(&self, parameters: [Parameter<'db>; N]) -> Result<Parameters<'db>, Self::Error>;
        #[operation(child)]
        async fn single_callable(&self, signature: Signature<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn single_binding(&self, ty: Type<'db>, signature: Signature<'db>) -> Result<Bindings<'db>, Self::Error>;
        #[operation(local)]
        async fn overloaded_bindings<const N: usize>(&self, ty: Type<'db>, signatures: [Signature<'db>; N]) -> Result<Bindings<'db>, Self::Error>;
        #[operation(child)]
        async fn wrapper_signatures(&self, wrapper: WrapperDescriptorKind) -> Result<WrapperDescriptorSignatures<'db>, Self::Error>;
    }

    #[finite_capability]
    impl KnownClassBindingFacts {
        fn any<'db>(&self) -> Type<'db> { Type::any() }
        fn unknown<'db>(&self) -> Type<'db> { Type::unknown() }
        fn object<'db>(&self) -> Type<'db> { Type::object() }
        fn literal_string<'db>(&self) -> Type<'db> { Type::literal_string() }
        fn boolean<'db>(&self, value: bool) -> Type<'db> { Type::bool_literal(value) }
        fn integer<'db>(&self, value: i64) -> Type<'db> { Type::int_literal(value) }

        fn positional_only<'db>(&self, name: Option<&'static str>, annotation: Type<'db>, default: Option<Type<'db>>) -> Parameter<'db> {
            Parameter::positional_only(name.map(Name::new_static))
                .with_annotated_type(annotation)
                .with_optional_default_type(default)
        }

        fn positional_or_keyword<'db>(&self, name: &'static str, annotation: Type<'db>, default: Option<Type<'db>>) -> Parameter<'db> {
            Parameter::positional_or_keyword(Name::new_static(name))
                .with_annotated_type(annotation)
                .with_optional_default_type(default)
        }

        fn keyword_only<'db>(&self, name: &'static str, annotation: Type<'db>, default: Option<Type<'db>>) -> Parameter<'db> {
            Parameter::keyword_only(Name::new_static(name))
                .with_annotated_type(annotation)
                .with_optional_default_type(default)
        }

        fn signature<'db>(&self, parameters: Parameters<'db>, returns: Type<'db>) -> Signature<'db> {
            Signature::new(parameters, returns)
        }

        fn generic_signature<'db>(&self, context: GenericContext<'db>, parameters: Parameters<'db>, returns: Type<'db>) -> Signature<'db> {
            Signature::new_generic(Some(context), parameters, returns)
        }

        fn one<T>(&self, first: T) -> [T; 1] { [first] }
        fn two<T>(&self, first: T, second: T) -> [T; 2] { [first, second] }
        fn three<T>(&self, first: T, second: T, third: T) -> [T; 3] { [first, second, third] }
        fn four<T>(&self, first: T, second: T, third: T, fourth: T) -> [T; 4] { [first, second, third, fourth] }
    }

    #[synchronous(known_class_bindings_sync)]
    #[capabilities(effects = KnownClassBindingEffects, facts = KnownClassBindingFacts)]
    #[passive_values(KnownClass::Bool, KnownClass::Super, KnownClass::Warning, KnownClass::NoneType, KnownClass::Int, KnownClass::Deprecated, KnownClass::Str, KnownClass::TypeVar, KnownClass::ParamSpec, KnownClass::TypeVarTuple, KnownClass::FunctoolsPartial, KnownClass::Iterable, TypeVarVariance::Covariant, Type::TypeVar)]
    pub(in crate::types) async fn known_class_bindings_with<'db, E: KnownClassBindingEffects<'db>>(
        ty: Type<'db>,
        class: ClassLiteral<'db>,
        facts: KnownClassBindingFacts,
        effects: &E,
    ) -> Result<Option<Bindings<'db>>, E::Error> {
        let Some(known) = effects.known(class).await? else {
            return Ok(None);
        };
        // TODO: Some of these cases date back to when we didn't even support overloads yet; see if
        // any can be removed: https://github.com/astral-sh/ty/issues/2715
        let bindings = match known {
            KnownClass::Bool => {
                // ```py
                // class bool(int):
                //     def __new__(cls, o: object = ..., /) -> Self: ...
                // ```
                let parameter = facts.positional_only(Some("o"), facts.any(), Some(facts.boolean(false)));
                let parameters = effects.standard_parameters(facts.one(parameter)).await?;
                let returns = effects.instance(KnownClass::Bool).await?;
                effects.single_binding(ty, facts.signature(parameters, returns)).await?
            }

            KnownClass::Object => {
                // ```py
                // class object:
                //    def __init__(self) -> None: ...
                //    def __new__(cls) -> Self: ...
                // ```
                let parameters = effects.empty_parameters().await?;
                effects.single_binding(ty, facts.signature(parameters, facts.object())).await?
            }

            KnownClass::Super => {
                // ```py
                // class super:
                //     @overload
                //     def __init__(self, t: Any, obj: Any, /) -> None: ...
                //     @overload
                //     def __init__(self, t: Any, /) -> None: ...
                //     @overload
                //     def __init__(self) -> None: ...
                // ```
                let first = facts.positional_only(Some("t"), facts.any(), None);
                let second = facts.positional_only(Some("obj"), facts.any(), None);
                let parameters = effects.standard_parameters(facts.two(first, second)).await?;
                let returns = effects.instance(KnownClass::Super).await?;
                let two_arguments = facts.signature(parameters, returns);

                let parameter = facts.positional_only(Some("t"), facts.any(), None);
                let parameters = effects.standard_parameters(facts.one(parameter)).await?;
                let returns = effects.instance(KnownClass::Super).await?;
                let one_argument = facts.signature(parameters, returns);

                let parameters = effects.empty_parameters().await?;
                let returns = effects.instance(KnownClass::Super).await?;
                let no_arguments = facts.signature(parameters, returns);
                effects.overloaded_bindings(ty, facts.three(two_arguments, one_argument, no_arguments)).await?
            }

            KnownClass::Deprecated => {
                // ```py
                // class deprecated:
                //     def __new__(
                //         cls,
                //         message: LiteralString,
                //         /,
                //         *,
                //         category: type[Warning] | None = ...,
                //         stacklevel: int = 1
                //     ) -> Self: ...
                // ```
                let warning_class_type = effects.subclass(KnownClass::Warning).await?;
                let message = facts.positional_only(Some("message"), facts.literal_string(), None);
                let none = effects.instance(KnownClass::NoneType).await?;
                let category_type = effects.union_two(warning_class_type, none).await?;
                let category = facts.keyword_only("category", category_type, Some(warning_class_type));
                let int = effects.instance(KnownClass::Int).await?;
                let stacklevel = facts.keyword_only("stacklevel", int, Some(facts.integer(1)));
                let parameters = effects.standard_parameters(facts.three(message, category, stacklevel)).await?;
                let returns = effects.instance(KnownClass::Deprecated).await?;
                effects.single_binding(ty, facts.signature(parameters, returns)).await?
            }

            KnownClass::TypeAliasType | KnownClass::ExtensionsTypeAliasType => {
                // ```py
                // def __new__(
                //     cls,
                //     name: str,
                //     value: Any,
                //     *,
                //     type_params: tuple[TypeVar | ParamSpec | TypeVarTuple, ...] = ()
                // ) -> Self: ...
                // ```
                let str_type = effects.instance(KnownClass::Str).await?;
                let name = facts.positional_or_keyword("name", str_type, None);
                let type_form = effects.type_form(facts.object()).await?;
                let value = facts.positional_or_keyword("value", type_form, None);
                let typevar = effects.instance(KnownClass::TypeVar).await?;
                let paramspec = effects.instance(KnownClass::ParamSpec).await?;
                let typevartuple = effects.instance(KnownClass::TypeVarTuple).await?;
                let element = effects.union(facts.three(typevar, paramspec, typevartuple)).await?;
                let tuple = effects.homogeneous_tuple(element).await?;
                let empty_tuple = effects.empty_tuple().await?;
                let type_params = facts.keyword_only("type_params", tuple, Some(empty_tuple));
                let parameters = effects.standard_parameters(facts.three(name, value, type_params)).await?;
                effects.single_binding(ty, facts.signature(parameters, facts.unknown())).await?
            }

            // Keep the argument's full type in `MethodWrapper` instead of inferring one shared
            // ParamSpec and return type, which would lose attributes and overload correlations.
            KnownClass::Classmethod | KnownClass::Staticmethod => {
                let parameters = effects.gradual_parameters().await?;
                let signature = facts.signature(parameters, facts.object());
                let callable = effects.single_callable(signature).await?;
                let parameter = facts.positional_only(Some("f"), callable, None);
                let parameters = effects.standard_parameters(facts.one(parameter)).await?;
                effects.single_binding(ty, facts.signature(parameters, facts.unknown())).await?
            }

            KnownClass::Property => {
                let parameter = facts.positional_only(None, facts.any(), None);
                let parameters = effects.standard_parameters(facts.one(parameter)).await?;
                let getter_signature = facts.signature(parameters, facts.any());

                let first = facts.positional_only(None, facts.any(), None);
                let second = facts.positional_only(None, facts.any(), None);
                let parameters = effects.standard_parameters(facts.two(first, second)).await?;
                let none = effects.instance(KnownClass::NoneType).await?;
                let setter_signature = facts.signature(parameters, none);

                let parameter = facts.positional_only(None, facts.any(), None);
                let parameters = effects.standard_parameters(facts.one(parameter)).await?;
                let deleter_signature = facts.signature(parameters, facts.any());

                let getter = effects.single_callable(getter_signature).await?;
                let none = effects.instance(KnownClass::NoneType).await?;
                let getter_type = effects.union_two(getter, none).await?;
                let none = effects.instance(KnownClass::NoneType).await?;
                let fget = facts.positional_or_keyword("fget", getter_type, Some(none));

                let setter = effects.single_callable(setter_signature).await?;
                let none = effects.instance(KnownClass::NoneType).await?;
                let setter_type = effects.union_two(setter, none).await?;
                let none = effects.instance(KnownClass::NoneType).await?;
                let fset = facts.positional_or_keyword("fset", setter_type, Some(none));

                let deleter = effects.single_callable(deleter_signature).await?;
                let none = effects.instance(KnownClass::NoneType).await?;
                let deleter_type = effects.union_two(deleter, none).await?;
                let none = effects.instance(KnownClass::NoneType).await?;
                let fdel = facts.positional_or_keyword("fdel", deleter_type, Some(none));

                let str_type = effects.instance(KnownClass::Str).await?;
                let none = effects.instance(KnownClass::NoneType).await?;
                let doc_type = effects.union_two(str_type, none).await?;
                let none = effects.instance(KnownClass::NoneType).await?;
                let doc = facts.positional_or_keyword("doc", doc_type, Some(none));
                let parameters = effects.standard_parameters(facts.four(fget, fset, fdel, doc)).await?;
                effects.single_binding(ty, facts.signature(parameters, facts.unknown())).await?
            }

            KnownClass::FunctoolsPartial => {
                // ```py
                // class partial(Generic[_T]):
                //     def __new__(cls, func: Callable[..., _T], /, *args: Any, **kwargs: Any) -> Self: ...
                // ```
                let return_ty = effects.synthetic_typevar("_T", TypeVarVariance::Covariant).await?;
                let context = effects.generic_context(facts.one(return_ty)).await?;
                let parameters = effects.gradual_parameters().await?;
                let signature = facts.signature(parameters, Type::TypeVar(return_ty));
                let callable = effects.single_callable(signature).await?;
                let func = facts.positional_only(Some("func"), callable, None);
                let parameters = effects.concatenate_gradual(facts.one(func)).await?;
                let returns = effects.specialized_instance(KnownClass::FunctoolsPartial, facts.one(Type::TypeVar(return_ty))).await?;
                effects.single_binding(ty, facts.generic_signature(context, parameters, returns)).await?
            }

            KnownClass::Tuple => {
                let element_ty = effects.synthetic_typevar("T", TypeVarVariance::Covariant).await?;

                // ```py
                // class tuple(Sequence[_T_co]):
                //     @overload
                //     def __new__(cls) -> tuple[()]: ...
                //     @overload
                //     def __new__(cls, iterable: Iterable[_T_co]) -> tuple[_T_co, ...]: ...
                // ```
                let parameters = effects.empty_parameters().await?;
                let returns = effects.empty_tuple().await?;
                let empty_signature = facts.signature(parameters, returns);

                let context = effects.generic_context(facts.one(element_ty)).await?;
                let iterable_type = effects.specialized_instance(KnownClass::Iterable, facts.one(Type::TypeVar(element_ty))).await?;
                let iterable = facts.positional_only(Some("iterable"), iterable_type, None);
                let parameters = effects.standard_parameters(facts.one(iterable)).await?;
                let returns = effects.homogeneous_tuple(Type::TypeVar(element_ty)).await?;
                let iterable_signature = facts.generic_signature(context, parameters, returns);
                effects.overloaded_bindings(ty, facts.two(empty_signature, iterable_signature)).await?
            }

            _ => return Ok(None),
        };
        Ok(Some(bindings))
    }

    #[synchronous(wrapper_descriptor_signatures_sync)]
    #[capabilities(effects = KnownClassBindingEffects, facts = KnownClassBindingFacts)]
    #[passive_values(KnownClass::FunctionType, KnownClass::Property, KnownClass::Type, KnownClass::NoneType, WrapperDescriptorSignatures::Overloaded, WrapperDescriptorSignatures::Single)]
    pub(in crate::types) async fn wrapper_descriptor_signatures_with<'db, E: KnownClassBindingEffects<'db>>(
        wrapper: WrapperDescriptorKind,
        facts: KnownClassBindingFacts,
        effects: &E,
    ) -> Result<WrapperDescriptorSignatures<'db>, E::Error> {
        let class = match wrapper {
            WrapperDescriptorKind::FunctionTypeDunderGet => KnownClass::FunctionType,
            WrapperDescriptorKind::PropertyDunderGet => KnownClass::Property,
            WrapperDescriptorKind::PropertyDunderSet => {
                let object = facts.object();
                let property = effects.instance(KnownClass::Property).await?;
                let receiver = facts.positional_only(Some("self"), property, None);
                let instance = facts.positional_only(Some("instance"), object, None);
                let value = facts.positional_only(Some("value"), object, None);
                let parameters = effects.standard_parameters(facts.three(receiver, instance, value)).await?;
                return Ok(WrapperDescriptorSignatures::Single(facts.signature(parameters, facts.unknown())));
            }
            WrapperDescriptorKind::PropertyDunderDelete => {
                let property = effects.instance(KnownClass::Property).await?;
                let receiver = facts.positional_only(Some("self"), property, None);
                let instance = facts.positional_only(Some("instance"), facts.object(), None);
                let parameters = effects.standard_parameters(facts.two(receiver, instance)).await?;
                return Ok(WrapperDescriptorSignatures::Single(facts.signature(parameters, facts.unknown())));
            }
        };

        // Similar to what we do in `KnownBoundMethodType::callables`,
        // here we also model `types.FunctionType.__get__` (or builtins.property.__get__),
        // but now we consider a call to this as a function, i.e. we also expect the `self`
        // argument to be passed in.
        // Return types are supplied by `Bindings::evaluate_known_cases`.
        //
        // TODO: Consider merging these synthesized signatures with the ones in
        // `KnownBoundMethodType::callables`, since that one is just this signature
        // with the `self` parameters removed.
        let type_instance = effects.instance(KnownClass::Type).await?;
        let none = effects.instance(KnownClass::NoneType).await?;
        let descriptor = effects.instance(class).await?;
        let receiver = facts.positional_only(Some("self"), descriptor, None);
        let instance = facts.positional_only(Some("instance"), none, None);
        let owner = facts.positional_only(Some("owner"), type_instance, None);
        let parameters = effects.standard_parameters(facts.three(receiver, instance, owner)).await?;
        let class_access = facts.signature(parameters, facts.unknown());

        let receiver = facts.positional_only(Some("self"), descriptor, None);
        let instance = facts.positional_only(Some("instance"), facts.object(), None);
        let owner_type = effects.union_two(type_instance, none).await?;
        let owner = facts.positional_only(Some("owner"), owner_type, Some(none));
        let parameters = effects.standard_parameters(facts.three(receiver, instance, owner)).await?;
        let instance_access = facts.signature(parameters, facts.unknown());
        Ok(WrapperDescriptorSignatures::Overloaded(facts.two(class_access, instance_access)))
    }

    #[synchronous(wrapper_descriptor_bindings_sync)]
    #[capabilities(effects = KnownClassBindingEffects)]
    #[passive_values()]
    pub(in crate::types) async fn wrapper_descriptor_bindings_with<'db, E: KnownClassBindingEffects<'db>>(
        ty: Type<'db>,
        wrapper: WrapperDescriptorKind,
        effects: &E,
    ) -> Result<Bindings<'db>, E::Error> {
        match effects.wrapper_signatures(wrapper).await? {
            WrapperDescriptorSignatures::Overloaded(signatures) => effects.overloaded_bindings(ty, signatures).await,
            WrapperDescriptorSignatures::Single(signature) => effects.single_binding(ty, signature).await,
        }
    }
}

pub(in crate::types) fn wrapper_descriptor_signatures<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    wrapper: WrapperDescriptorKind,
) -> impl Iterator<Item = Signature<'db>> {
    match wrapper_descriptor_signatures_sync(
        wrapper,
        KnownClassBindingFacts,
        &OrdinaryKnownClassBindingEffects { db, env },
    ) {
        Ok(WrapperDescriptorSignatures::Overloaded(signatures)) => {
            Either::Left(signatures.into_iter())
        }
        Ok(WrapperDescriptorSignatures::Single(signature)) => {
            Either::Right(std::iter::once(signature))
        }
        Err(never) => match never {},
    }
}

pub(in crate::types) fn wrapper_descriptor_bindings<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    wrapper: WrapperDescriptorKind,
) -> Bindings<'db> {
    match wrapper_descriptor_bindings_sync(
        ty,
        wrapper,
        &OrdinaryKnownClassBindingEffects { db, env },
    ) {
        Ok(bindings) => bindings,
        Err(never) => match never {},
    }
}

pub(in crate::types) fn known_class_bindings<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    class: ClassLiteral<'db>,
) -> Option<Bindings<'db>> {
    match known_class_bindings_sync(
        ty,
        class,
        KnownClassBindingFacts,
        &OrdinaryKnownClassBindingEffects { db, env },
    ) {
        Ok(bindings) => bindings,
        Err(never) => match never {},
    }
}

struct OrdinaryKnownClassBindingEffects<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
}

impl<'db> SynchronousKnownClassBindingEffects<'db> for OrdinaryKnownClassBindingEffects<'_, 'db> {
    type Error = Infallible;

    fn wrapper_signatures(
        &self,
        wrapper: WrapperDescriptorKind,
    ) -> Result<WrapperDescriptorSignatures<'db>, Self::Error> {
        wrapper_descriptor_signatures_sync(wrapper, KnownClassBindingFacts, self)
    }

    fn known(&self, class: ClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error> {
        Ok(class.known(self.db))
    }

    fn instance(&self, class: KnownClass) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_instance(self.db, self.env))
    }

    fn subclass(&self, class: KnownClass) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_subclass_of(self.db, self.env))
    }

    fn specialized_instance<const N: usize>(
        &self,
        class: KnownClass,
        arguments: [Type<'db>; N],
    ) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_specialized_instance(self.db, self.env, &arguments))
    }

    fn type_form(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(TypeFormType::from_type_expression(self.db, ty))
    }

    fn union_two(&self, left: Type<'db>, right: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(UnionType::from_two_elements(self.db, self.env, left, right))
    }

    fn union<const N: usize>(&self, elements: [Type<'db>; N]) -> Result<Type<'db>, Self::Error> {
        Ok(UnionType::from_elements(self.db, self.env, elements))
    }

    fn homogeneous_tuple(&self, element: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::homogeneous_tuple(self.db, self.env, element))
    }

    fn empty_tuple(&self) -> Result<Type<'db>, Self::Error> {
        Ok(Type::empty_tuple(self.db, self.env))
    }

    fn synthetic_typevar(
        &self,
        name: &'static str,
        variance: TypeVarVariance,
    ) -> Result<BoundTypeVarInstance<'db>, Self::Error> {
        Ok(BoundTypeVarInstance::synthetic(
            self.db,
            self.env,
            Name::new_static(name),
            variance,
        ))
    }

    fn generic_context<const N: usize>(
        &self,
        variables: [BoundTypeVarInstance<'db>; N],
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(GenericContext::from_typevar_instances(
            self.db, self.env, variables,
        ))
    }

    fn standard_parameters<const N: usize>(
        &self,
        parameters: [Parameter<'db>; N],
    ) -> Result<Parameters<'db>, Self::Error> {
        Ok(Parameters::standard(parameters))
    }

    fn empty_parameters(&self) -> Result<Parameters<'db>, Self::Error> {
        Ok(Parameters::empty())
    }

    fn gradual_parameters(&self) -> Result<Parameters<'db>, Self::Error> {
        Ok(Parameters::gradual_form())
    }

    fn concatenate_gradual<const N: usize>(
        &self,
        parameters: [Parameter<'db>; N],
    ) -> Result<Parameters<'db>, Self::Error> {
        Ok(Parameters::concatenate(
            self.db,
            Vec::from(parameters),
            ConcatenateTail::Gradual,
        ))
    }

    fn single_callable(&self, signature: Signature<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::single_callable(self.db, signature))
    }

    fn single_binding(
        &self,
        ty: Type<'db>,
        signature: Signature<'db>,
    ) -> Result<Bindings<'db>, Self::Error> {
        Ok(Binding::single(ty, signature).into())
    }

    fn overloaded_bindings<const N: usize>(
        &self,
        ty: Type<'db>,
        signatures: [Signature<'db>; N],
    ) -> Result<Bindings<'db>, Self::Error> {
        Ok(CallableBinding::from_overloads(ty, signatures).into())
    }
}
