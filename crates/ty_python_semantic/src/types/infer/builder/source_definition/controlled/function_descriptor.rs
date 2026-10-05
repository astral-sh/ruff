use std::future::Future;
use std::pin::Pin;

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects};
use crate::types::callable::CallableTypeKind;
use crate::types::callable::function_descriptor::{
    FunctionBindingEffects, FunctionBindingFacts, bound_method_from_callable_with,
    function_like_kind_with, underlying_function_with,
};
use crate::types::function::descriptor::{
    FunctionDefinitionsCursor, FunctionTypeDescriptorEffects, function_is_classmethod_with,
    function_is_staticmethod_with, underlying_function_with as underlying_function_type_with,
    with_descriptor_kind_with,
};
use crate::types::function::{FunctionType, OverloadLiteral};
use crate::types::infer::builder::function::application::DecoratorApplicationEffects;
use crate::types::instance::{NominalClassFacts, nominal_known_class_with};
use crate::types::known_instance::{MethodWrapper, MethodWrapperKind};
use crate::types::member_lookup::class_dispatch::ClassMemberDispatchEffects;
use crate::types::method::BoundMethodReceiver;
use crate::types::storage_quote::StorageQuote;
use crate::types::{BoundMethodType, CallableType, KnownClass, Type};
use crate::{Program, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Admits a descriptor update action and its fixed factory/result transfers before execution.
    pub(super) async fn descriptor_update_local<T, F: FnOnce() -> T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: F,
    ) -> RunResult<T> {
        let quote = (|| {
            Ok((
                Self::checked(work.and_then(|work| work.checked_add(6)))?,
                Self::checked(
                    bytes
                        .and_then(|bytes| bytes.checked_add(size_of::<F>().checked_mul(2)?))
                        .and_then(|bytes| bytes.checked_add(size_of::<Option<F>>()))
                        .and_then(|bytes| bytes.checked_add(size_of::<T>().checked_mul(2)?))
                        .and_then(|bytes| bytes.checked_add(size_of::<RunResult<T>>().checked_mul(2)?)),
                )?,
            ))
        })();
        self.local_quoted(quote, action).await
    }

    /// Admits a boxed update continuation while retaining any owned captures through refusal drainage.
    pub(super) async fn descriptor_update_future<F: Future, M: FnOnce() -> F>(
        &self,
        make: M,
    ) -> RunResult<Pin<Box<F>>> {
        let bytes = size_of::<M>()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(size_of::<Option<M>>()))
            .and_then(|bytes| bytes.checked_add(size_of::<Pin<Box<F>>>().checked_mul(2)?))
            .and_then(|bytes| bytes.checked_add(size_of::<RunResult<Pin<Box<F>>>>().checked_mul(2)?))
            .and_then(|bytes| bytes.checked_add(size_of::<RunResult<()>>().checked_mul(2)?))
            .and_then(|bytes| bytes.checked_add(size_of::<Option<()>>()));
        let quote = bytes
            .map(|bytes| (6, bytes))
            .ok_or(RunError::Contract(
                "function descriptor future quotation overflow",
            ));
        // The owned factory remains in this future while the admission callback can refuse.
        // allocate_future separately funds the continuation's backing representation.
        self.local_quoted(quote, || ()).await?;
        self.allocate_future(make).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FunctionBindingEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn function_kind(&self, function: FunctionType<'db>) -> RunResult<CallableTypeKind> {
        self.allocate_future(|| function.callable_type_kind_with(self.db(), self))
            .await?
            .await
    }

    async fn callable_kind(&self, callable: CallableType<'db>) -> RunResult<CallableTypeKind> {
        self.field(
            callable
                .field_requests(self.access.endpoint().field_request_context())
                .kind(),
        )
        .await
    }

    async fn wrapper_kind(&self, wrapper: MethodWrapper<'db>) -> RunResult<MethodWrapperKind> {
        self.field(
            wrapper
                .field_requests(self.access.endpoint().field_request_context())
                .kind(),
        )
        .await
    }

    async fn function_underlying(&self, function: FunctionType<'db>) -> RunResult<Type<'db>> {
        let underlying = self
            .allocate_future(|| underlying_function_type_with(function, self))
            .await?
            .await?;
        self.initialize_value(|| Type::FunctionLiteral(underlying))
            .await
    }

    async fn callable_underlying(&self, callable: CallableType<'db>) -> RunResult<Type<'db>> {
        self.allocate_future(|| {
            DecoratorApplicationEffects::callable_with_kind(
                self,
                callable,
                CallableTypeKind::FunctionLike,
            )
        })
        .await?
        .await
    }

    async fn wrapper_wrapped(&self, wrapper: MethodWrapper<'db>) -> RunResult<Type<'db>> {
        self.field(
            wrapper
                .field_requests(self.access.endpoint().field_request_context())
                .wrapped(),
        )
        .await
    }

    async fn kind(&self, ty: Type<'db>) -> RunResult<Option<CallableTypeKind>> {
        self.allocate_future(|| function_like_kind_with(ty, FunctionBindingFacts, self))
            .await?
            .await
    }

    async fn underlying(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.allocate_future(|| underlying_function_with(ty, FunctionBindingFacts, self))
            .await?
            .await
    }

    async fn owner_is_none(&self, owner: Type<'db>) -> RunResult<bool> {
        let instance = self
            .initialize_value(|| owner.as_nominal_instance())
            .await?;
        let Some(instance) = instance else {
            return Ok(false);
        };
        let known = self
            .allocate_future(|| nominal_known_class_with(instance, NominalClassFacts, self))
            .await?
            .await?;
        self.initialize_value(|| known == Some(KnownClass::NoneType))
            .await
    }

    async fn meta_type(
        &self,
        instance: Type<'db>,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<Type<'db>> {
        self.allocate_future(|| ClassMemberDispatchEffects::meta_type(self, instance))
            .await?
            .await
    }

    async fn program(&self, env: &ProgramEnvironment<'db>) -> RunResult<Program<'db>> {
        self.allocate_future(|| self.environment_program(env))
            .await?
            .await
    }

    async fn bound_method(
        &self,
        func: Type<'db>,
        program: Program<'db>,
        receiver: Type<'db>,
    ) -> RunResult<Type<'db>> {
        let method = self
            .allocate_future(|| {
                bound_method_from_callable_with(func, program, receiver, FunctionBindingFacts, self)
            })
            .await?
            .await?;
        self.initialize_value(|| Type::BoundMethod(method)).await
    }

    async fn intern_bound_method(
        &self,
        func: Type<'db>,
        program: Program<'db>,
        class_method: bool,
        receiver: Type<'db>,
    ) -> RunResult<BoundMethodType<'db>> {
        self.check_program(program)?;
        let receiver = self
            .initialize_value(|| BoundMethodReceiver::Instance(receiver))
            .await?;
        self.allocate_future(|| {
            self.access
                .intern_bound_method(func, program, class_method, receiver)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FunctionTypeDescriptorEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Definitions = FunctionDefinitionsCursor<'db>;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn descriptor_kind(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<Option<CallableTypeKind>> {
        self.field(
            function
                .field_requests(self.access.endpoint().field_request_context())
                .descriptor_kind(),
        )
        .await
    }

    async fn definitions(&self, function: FunctionType<'db>) -> RunResult<Self::Definitions> {
        let (overloads, implementation) = self
            .allocate_future(|| function.overloads_and_implementation_with(self.db(), self))
            .await?
            .await?;
        self.initialize_value(|| overloads.iter().copied().chain(implementation))
            .await
    }

    async fn next_definition(
        &self,
        definitions: &mut Self::Definitions,
    ) -> RunResult<Option<OverloadLiteral<'db>>> {
        self.initialize_value(|| definitions.next()).await
    }

    async fn overload_is_classmethod(&self, overload: OverloadLiteral<'db>) -> RunResult<bool> {
        self.allocate_future(|| overload.is_classmethod_with(self.db(), self))
            .await?
            .await
    }

    async fn staticmethod_declaration(&self, function: FunctionType<'db>) -> RunResult<bool> {
        self.allocate_future(|| function.has_staticmethod_declaration_with(self.db(), self))
            .await?
            .await
    }

    async fn classmethod(&self, function: FunctionType<'db>) -> RunResult<bool> {
        self.allocate_future(|| function_is_classmethod_with(function, self))
            .await?
            .await
    }

    async fn staticmethod(&self, function: FunctionType<'db>) -> RunResult<bool> {
        self.allocate_future(|| function_is_staticmethod_with(function, self))
            .await?
            .await
    }

    async fn with_kind(
        &self,
        function: FunctionType<'db>,
        kind: CallableTypeKind,
    ) -> RunResult<FunctionType<'db>> {
        self.descriptor_update_future(|| with_descriptor_kind_with(function, kind, self))
            .await?
            .await
    }

    async fn rebuild(
        &self,
        function: FunctionType<'db>,
        kind: Option<CallableTypeKind>,
    ) -> RunResult<FunctionType<'db>> {
        let literal = self
            .field(
                function
                    .field_requests(self.access.endpoint().field_request_context())
                    .literal(),
            )
            .await?;
        let updated = self
            .descriptor_update_future(|| function.read_updated_signatures(self.access.endpoint()))
            .await?
            .await;
        let inspection = self
            .descriptor_update_local(Some(2), Some(0), || {
                updated
                    .as_deref()
                    .map(|updated| updated.clone_inspection_work())
                    .unwrap_or(Some(1))
            })
            .await?;
        let quote = self
            .descriptor_update_local(inspection, Some(0), || {
                updated
                    .as_deref()
                    .map(|updated| updated.clone_storage_quote())
                    .unwrap_or(Some(StorageQuote { work: 1, bytes: 0 }))
            })
            .await?;
        let work = quote.map(|quote| quote.work);
        let bytes = quote.map(|quote| quote.bytes);
        let updated = self
            .descriptor_update_local(work, bytes, || (*updated).clone())
            .await?;
        self.descriptor_update_future(move || {
            self.access.intern_function_type(literal, updated, kind)
        })
        .await?
        .await
    }

    async fn callable_kind(&self, function: FunctionType<'db>) -> RunResult<CallableTypeKind> {
        self.descriptor_update_future(|| function.callable_type_kind_with(self.db(), self))
            .await?
            .await
    }

    async fn same_kind(&self, left: CallableTypeKind, right: CallableTypeKind) -> RunResult<bool> {
        self.descriptor_update_local(Some(1), Some(0), || left == right)
            .await
    }
}
