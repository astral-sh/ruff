//! Property provenance uses admitted field reads and the existing property interner.

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects};
use crate::types::function::FunctionType;
use crate::types::method::BoundMethodReceiver;
use crate::types::property_provenance::PropertyProvenanceEffects;
use crate::types::{
    BoundMethodType, PropertyAccessorDefinitions, PropertyInstanceClass, PropertyInstanceType, Type,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> PropertyProvenanceEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(size_of::<PropertyAccessorDefinitions<'db>>() * 2 + 32)
            .await
    }

    async fn definitions(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> RunResult<PropertyAccessorDefinitions<'db>> {
        self.field(
            property
                .field_requests(self.access.endpoint().field_request_context())
                .accessor_definitions(),
        )
        .await
    }

    async fn bound_receiver(&self, method: BoundMethodType<'db>) -> RunResult<Type<'db>> {
        let receiver = self
            .field(method.receiver_request(self.access.endpoint().field_request_context()))
            .await?;
        self.local(1, 0, || match receiver {
            BoundMethodReceiver::Instance(receiver)
            | BoundMethodReceiver::Constrained { receiver, .. } => receiver,
        })
        .await
    }

    async fn bound_callable(&self, method: BoundMethodType<'db>) -> RunResult<Type<'db>> {
        self.field(
            method
                .field_requests(self.access.endpoint().field_request_context())
                .func(),
        )
        .await
    }

    async fn function_name(&self, function: FunctionType<'db>) -> RunResult<&'db Name> {
        let literal = self
            .field(
                function
                    .field_requests(self.access.endpoint().field_request_context())
                    .literal(),
            )
            .await?;
        self.field(literal.last_definition.field_requests(self.db()).name())
            .await
    }

    async fn getter(&self, property: PropertyInstanceType<'db>) -> RunResult<Option<Type<'db>>> {
        self.field(
            property
                .field_requests(self.access.endpoint().field_request_context())
                .getter(),
        )
        .await
    }

    async fn setter(&self, property: PropertyInstanceType<'db>) -> RunResult<Option<Type<'db>>> {
        self.field(
            property
                .field_requests(self.access.endpoint().field_request_context())
                .setter(),
        )
        .await
    }

    async fn deleter(&self, property: PropertyInstanceType<'db>) -> RunResult<Option<Type<'db>>> {
        self.field(
            property
                .field_requests(self.access.endpoint().field_request_context())
                .deleter(),
        )
        .await
    }

    async fn instance_class(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> RunResult<PropertyInstanceClass<'db>> {
        self.field(
            property
                .field_requests(self.access.endpoint().field_request_context())
                .instance_class(),
        )
        .await
    }

    async fn same_accessor(
        &self,
        retained: Option<Type<'db>>,
        supplied: Type<'db>,
    ) -> RunResult<bool> {
        let work = self
            .local(3, 0, || {
                retained
                    .map_or(0, Type::inline_payload_bytes)
                    .checked_add(supplied.inline_payload_bytes())
                    .and_then(|bytes| bytes.checked_add(size_of::<Type<'db>>() * 2 + 1))
            })
            .await?;
        self.local(Self::checked(work)?, 0, || retained == Some(supplied))
            .await
    }

    async fn intern_property(
        &self,
        getter: Option<Type<'db>>,
        setter: Option<Type<'db>>,
        deleter: Option<Type<'db>>,
        instance_class: PropertyInstanceClass<'db>,
        definitions: PropertyAccessorDefinitions<'db>,
    ) -> RunResult<PropertyInstanceType<'db>> {
        self.access
            .intern_property(getter, setter, deleter, instance_class, definitions)
            .await
    }
}
