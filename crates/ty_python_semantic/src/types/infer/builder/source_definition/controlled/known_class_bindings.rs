//! Known-class and wrapper signatures use canonical type children and admitted owned storage.

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::call::bind::source::initial_overloads_quote;
use crate::types::call::preparation::known_class::{
    KnownClassBindingEffects, KnownClassBindingFacts, WrapperDescriptorSignatures,
    wrapper_descriptor_signatures_with,
};
use crate::types::call::{Binding, Bindings, CallableBinding};
use crate::types::callable::CallableTypeKind;
use crate::types::class::{
    KnownClassSubclassEffects, class_default_specialization_with, interpret_class_literal_lookup,
    known_class_to_subclass_of_with,
};
use crate::types::generics::context_construction::{
    ContextConstructionEffects, ContextVariables, context_from_typevars_with,
};
use crate::types::set_theoretic::assembly::{self, TypeAssemblyEffects, TypeElements};
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::signatures::source::parameters_storage_quote;
use crate::types::signatures::{
    CallableSignature, ConcatenateTail, Parameter, Parameters, Signature,
};
use crate::types::subclass_of::{SubclassConstructionFacts, SubclassOfInner, subclass_from_with};
use crate::types::tuple::TupleSpec;
use crate::types::tuple::construction::tuple_type;
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarKind};
use crate::types::{
    BindingContext, BoundTypeVarInstance, ClassLiteral, ClassType, GenericContext, KnownClass,
    StaticClassLiteral, Type, TypeVarVariance,
};
use crate::{Db, Program, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> KnownClassBindingEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn wrapper_signatures(
        &self,
        wrapper: crate::types::WrapperDescriptorKind,
    ) -> RunResult<WrapperDescriptorSignatures<'db>> {
        self.allocate_future(|| {
            wrapper_descriptor_signatures_with(wrapper, KnownClassBindingFacts, self)
        })
        .await?
        .await
    }

    async fn known(&self, class: ClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        let Some(class) = self.local(1, 0, || class.as_static()).await? else {
            return Ok(None);
        };
        self.field(
            class
                .field_requests(self.access.endpoint().field_request_context())
                .known(),
        )
        .await
    }

    async fn instance(&self, class: KnownClass) -> RunResult<Type<'db>> {
        self.access.known_class_instance(self.program, class).await
    }

    async fn subclass(&self, class: KnownClass) -> RunResult<Type<'db>> {
        known_class_to_subclass_of_with(class, self).await
    }

    async fn specialized_instance<const N: usize>(
        &self,
        _class: KnownClass,
        _arguments: [Type<'db>; N],
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::CallBindings).await
    }

    async fn type_form(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.access.intern_typeform(ty).await
    }

    async fn union_two(&self, left: Type<'db>, right: Type<'db>) -> RunResult<Type<'db>> {
        self.access.union_from_two_elements(left, right).await
    }

    async fn union<const N: usize>(&self, elements: [Type<'db>; N]) -> RunResult<Type<'db>> {
        let env = ProgramEnvironment::from_program(self.program);
        let mut elements = self
            .local(
                Self::checked(
                    size_of::<[Type<'db>; N]>()
                        .checked_mul(2)
                        .and_then(|n| n.checked_add(1)),
                )?,
                0,
                || KnownClassElements {
                    source: self,
                    elements: elements.into_iter(),
                },
            )
            .await?;
        assembly::union_from_elements(
            self.db(),
            &env,
            &mut elements,
            &KnownClassAssembly { source: self },
        )
        .await
    }

    async fn homogeneous_tuple(&self, element: Type<'db>) -> RunResult<Type<'db>> {
        let spec = self
            .local(size_of::<TupleSpec<'db>>() * 2 + 1, 0, || {
                TupleSpec::homogeneous(element)
            })
            .await?;
        let env = ProgramEnvironment::from_program(self.program);
        Ok(Type::tuple(tuple_type(self.db(), &env, &spec, self).await?))
    }

    async fn empty_tuple(&self) -> RunResult<Type<'db>> {
        let spec = self
            .local(size_of::<TupleSpec<'db>>() * 2 + 1, 0, || {
                TupleSpec::heterogeneous([])
            })
            .await?;
        Ok(Type::tuple(
            self.access.intern_tuple(self.program, spec).await?,
        ))
    }

    async fn synthetic_typevar(
        &self,
        name: &'static str,
        variance: TypeVarVariance,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        let name = self.local(1, 0, || Name::new_static(name)).await?;
        let identity = self
            .access
            .intern_typevar_identity(&name, None, TypeVarKind::Pep695TypeVar)
            .await?;
        let variable = self
            .access
            .intern_typevar_instance(identity, None, Some(variance), None)
            .await?;
        self.bind_typevar_in_context(variable, BindingContext::Synthetic(self.program))
            .await
    }

    async fn generic_context<const N: usize>(
        &self,
        variables: [BoundTypeVarInstance<'db>; N],
    ) -> RunResult<GenericContext<'db>> {
        let input = self
            .local(
                Self::checked(
                    size_of::<[BoundTypeVarInstance<'db>; N]>()
                        .checked_mul(2)
                        .and_then(|n| n.checked_add(1)),
                )?,
                0,
                || variables.into_iter(),
            )
            .await?;
        let env = ProgramEnvironment::from_program(self.program);
        context_from_typevars_with(
            self.db(),
            &env,
            input,
            &KnownClassContext::<_, N> { source: self },
        )
        .await
    }

    async fn standard_parameters<const N: usize>(
        &self,
        parameters: [Parameter<'db>; N],
    ) -> RunResult<Parameters<'db>> {
        let quote = parameters_storage_quote(N).ok_or(RunError::Contract(
            "known-class parameter quotation overflow",
        ))?;
        // Keep owners outside admission closures so refusal cannot retire them before child drain.
        let mut input = Some(parameters);
        let mut output = None;
        self.local(quote.work, quote.bytes, || {
            let parameters = input.take().ok_or(RunError::Contract(
                "known-class parameter input already consumed",
            ))?;
            output = Some(Parameters::standard(parameters));
            Ok(())
        })
        .await??;
        output.ok_or(RunError::Contract(
            "known-class parameters were not constructed",
        ))
    }

    async fn empty_parameters(&self) -> RunResult<Parameters<'db>> {
        KnownClassBindingEffects::standard_parameters(self, []).await
    }

    async fn gradual_parameters(&self) -> RunResult<Parameters<'db>> {
        let quote = parameters_storage_quote(2).ok_or(RunError::Contract(
            "known-class gradual parameter quotation overflow",
        ))?;
        let bytes = Self::checked(
            quote
                .bytes
                .checked_add(size_of::<Option<Parameters<'db>>>() + size_of::<Parameters<'db>>()),
        )?;
        let work = Self::checked(quote.work.checked_add(2))?;
        let mut output = None;
        self.local(work, bytes, || {
            output = Some(Parameters::gradual_form());
        })
        .await?;
        output.ok_or(RunError::Contract(
            "known-class gradual parameters were not constructed",
        ))
    }

    async fn concatenate_gradual<const N: usize>(
        &self,
        parameters: [Parameter<'db>; N],
    ) -> RunResult<Parameters<'db>> {
        let count = Self::checked(N.checked_add(2))?;
        let quote = parameters_storage_quote(count).ok_or(RunError::Contract(
            "known-class concatenate quotation overflow",
        ))?;
        let mut input = Some(parameters);
        let mut output = None;
        self.local(quote.work, quote.bytes, || {
            let parameters = input.take().ok_or(RunError::Contract(
                "known-class concatenate input already consumed",
            ))?;
            // Reserving the final length avoids growth before the gradual tail is appended.
            let mut prefix = Vec::with_capacity(count);
            prefix.extend(parameters);
            output = Some(Parameters::concatenate(
                self.db(),
                prefix,
                ConcatenateTail::Gradual,
            ));
            Ok(())
        })
        .await??;
        output.ok_or(RunError::Contract(
            "known-class concatenate was not constructed",
        ))
    }

    async fn single_callable(&self, signature: Signature<'db>) -> RunResult<Type<'db>> {
        self.single_callable_type(signature).await
    }

    async fn single_binding(
        &self,
        ty: Type<'db>,
        signature: Signature<'db>,
    ) -> RunResult<Bindings<'db>> {
        self.work(17).await?;
        let quote = initial_overloads_quote(std::slice::from_ref(&signature))
            .ok_or(RunError::Contract("known-class binding quotation overflow"))?;
        let carrier_bytes = Self::checked(
            size_of::<Option<Signature<'db>>>().checked_add(size_of::<Option<Bindings<'db>>>()),
        )?;
        self.local(2, carrier_bytes, || ()).await?;
        let mut input = Some(signature);
        let mut output = None;
        let action = || -> RunResult<()> {
            let signature = input.take().ok_or(RunError::Contract(
                "known-class binding signature already consumed",
            ))?;
            output = Some(Binding::single(ty, signature).into());
            Ok(())
        };
        let bytes = Self::checked(
            quote
                .bytes
                .checked_add(size_of::<Bindings<'db>>())
                .and_then(|bytes| bytes.checked_add(size_of::<RunResult<Bindings<'db>>>()))
                .and_then(|bytes| bytes.checked_add(size_of_val(&action)))
                .and_then(|bytes| bytes.checked_add(size_of::<RunResult<()>>())),
        )?;
        self.local(Self::checked(quote.work.checked_add(3))?, bytes, action)
            .await??;
        output.ok_or(RunError::Contract(
            "known-class binding was not constructed",
        ))
    }

    async fn overloaded_bindings<const N: usize>(
        &self,
        ty: Type<'db>,
        signatures: [Signature<'db>; N],
    ) -> RunResult<Bindings<'db>> {
        self.work(Self::checked(
            N.checked_mul(16).and_then(|work| work.checked_add(1)),
        )?)
        .await?;
        let quote = initial_overloads_quote(&signatures).ok_or(RunError::Contract(
            "known-class overload quotation overflow",
        ))?;
        let carrier_bytes = Self::checked(
            size_of::<Option<[Signature<'db>; N]>>()
                .checked_add(size_of::<Option<Bindings<'db>>>()),
        )?;
        self.local(2, carrier_bytes, || ()).await?;
        let mut input = Some(signatures);
        let mut output = None;
        let action = || -> RunResult<()> {
            let signatures = input.take().ok_or(RunError::Contract(
                "known-class overload signatures already consumed",
            ))?;
            output = Some(CallableBinding::from_overloads(ty, signatures).into());
            Ok(())
        };
        let bytes = Self::checked(
            quote
                .bytes
                .checked_add(size_of::<Bindings<'db>>())
                .and_then(|bytes| bytes.checked_add(size_of::<RunResult<Bindings<'db>>>()))
                .and_then(|bytes| bytes.checked_add(size_of_val(&action)))
                .and_then(|bytes| bytes.checked_add(size_of::<RunResult<()>>())),
        )?;
        self.local(Self::checked(quote.work.checked_add(3))?, bytes, action)
            .await??;
        output.ok_or(RunError::Contract(
            "known-class overloads were not constructed",
        ))
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> KnownClassSubclassEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn lookup(&self, class: KnownClass) -> RunResult<Option<StaticClassLiteral<'db>>> {
        let result = self.access.known_class_lookup(self.program, class).await?;
        self.local(1, 0, || interpret_class_literal_lookup(result))
            .await
    }

    async fn default_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassType<'db>> {
        class_default_specialization_with(class, self).await
    }

    async fn subclass_of(&self, class: ClassType<'db>) -> RunResult<Type<'db>> {
        subclass_from_with(
            SubclassOfInner::Class(class),
            SubclassConstructionFacts,
            self,
        )
        .await
    }
}

struct KnownClassElements<'source, 'access, 'run, 'db: 'run, A, const N: usize> {
    source: &'source SourceEffects<'access, 'run, 'db, A>,
    elements: std::array::IntoIter<Type<'db>, N>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>, const N: usize> TypeElements<'db>
    for KnownClassElements<'_, '_, 'run, 'db, A, N>
{
    type Error = RunError;
    type Item = Type<'db>;

    async fn next(&mut self) -> RunResult<Option<Type<'db>>> {
        self.source.local(2, 0, || self.elements.next()).await
    }
}

struct KnownClassAssembly<'source, 'access, 'run, 'db: 'run, A> {
    source: &'source SourceEffects<'access, 'run, 'db, A>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeAssemblyEffects<'db>
    for KnownClassAssembly<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn union<I: TypeElements<'db, Error = RunError>>(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: I::Item,
        second: I::Item,
        remaining: &mut I,
    ) -> RunResult<Type<'db>> {
        let mut builder = PairUnionEffects::new_union(self.source, env).await?;
        PairUnionEffects::union_add(self.source, &mut builder, first.into()).await?;
        PairUnionEffects::union_add(self.source, &mut builder, second.into()).await?;
        while let Some(element) = remaining.next().await? {
            PairUnionEffects::union_add(self.source, &mut builder, element.into()).await?;
        }
        PairUnionEffects::union_build(self.source, builder).await
    }

    async fn intersection<I: TypeElements<'db, Error = RunError>>(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _first: I::Item,
        _second: I::Item,
        _remaining: &mut I,
    ) -> RunResult<Type<'db>> {
        self.source.unavailable(SourceOperation::CallBindings).await
    }
}

struct KnownClassContext<'source, 'access, 'run, 'db: 'run, A, const N: usize> {
    source: &'source SourceEffects<'access, 'run, 'db, A>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>, const N: usize> ContextConstructionEffects<'db>
    for KnownClassContext<'_, '_, 'run, 'db, A, N>
{
    type Error = RunError;
    type Input = std::array::IntoIter<BoundTypeVarInstance<'db>, N>;

    async fn program(&self, env: &ProgramEnvironment<'db>) -> RunResult<Program<'db>> {
        ContextConstructionEffects::program(self.source, env).await
    }

    async fn input_lower_bound(&self, input: &Self::Input) -> RunResult<usize> {
        self.source.local(1, 0, || input.len()).await
    }

    async fn next_variable(
        &self,
        input: &mut Self::Input,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.source.local(2, 0, || input.next()).await
    }

    async fn new_variables(&self, lower_bound: usize) -> RunResult<ContextVariables<'db>> {
        ContextConstructionEffects::new_variables(self.source, lower_bound).await
    }

    async fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        ContextConstructionEffects::identity(self.source, variable).await
    }

    async fn insert(
        &self,
        variables: &mut ContextVariables<'db>,
        identity: BoundTypeVarIdentity<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        ContextConstructionEffects::insert(self.source, variables, identity, variable).await
    }

    async fn shrink(&self, variables: &mut ContextVariables<'db>) -> RunResult<()> {
        ContextConstructionEffects::shrink(self.source, variables).await
    }

    async fn intern(
        &self,
        program: Program<'db>,
        variables: ContextVariables<'db>,
    ) -> RunResult<GenericContext<'db>> {
        ContextConstructionEffects::intern(self.source, program, variables).await
    }

    async fn publish(&self, context: GenericContext<'db>) -> RunResult<GenericContext<'db>> {
        ContextConstructionEffects::publish(self.source, context).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn single_callable_type(
        &self,
        signature: Signature<'db>,
    ) -> RunResult<Type<'db>> {
        self.local(1, size_of::<Option<Signature<'db>>>(), || ())
            .await?;
        let mut input = Some(signature);
        let mut output = None;
        self.local(
            8,
            size_of::<CallableSignature<'db>>() + size_of::<Option<CallableSignature<'db>>>(),
            || {
                let signature = input
                    .take()
                    .ok_or(RunError::Contract("callable signature already consumed"))?;
                output = Some(CallableSignature::single(signature));
                Ok(())
            },
        )
        .await??;
        let signatures =
            output.ok_or(RunError::Contract("callable signature was not constructed"))?;
        let callable = self
            .access
            .owned_callable_type(signatures, CallableTypeKind::Regular, None)
            .await?;
        self.initialize_value(|| Type::Callable(callable)).await
    }
}
