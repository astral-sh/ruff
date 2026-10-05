//! Constructor dependencies admitted by the queued evaluator.

use std::future::{Future, ready};

use super::effects::{ConstructorCallableRequest, ConstructorEffects};
use super::{ConstructorMember, ConstructorMembers, InitializerBinding};
use crate::place::Place;
use crate::types::callable::CallableConversionRequest;
use crate::types::callable::scheduled_probe::{Boundary, Router};
use crate::types::generics::GenericContext;
use crate::types::instance::effects::QueuedInstanceEffects;
use crate::types::signatures::Signature;
use crate::types::signatures::effects::sealed;
use crate::types::{
    BoundMethodType, CallableType, CallableTypes, ClassType, DescriptorOrigin, FunctionType,
    SubclassOfInner, Type, UnionType,
};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
pub(crate) enum ConstructorEffect {
    InstanceApproximation,
    MetaclassCall,
    NewMethod,
    RawInitializer,
    BindInitializer,
    ObjectNew,
    EnumMetadata,
    ClassGenericContext,
    ExpandInitializer,
    BindNewSelf,
    InitializerSelfAnnotation,
    MergeGenericContext,
    BindInitializerSignature,
    RemoveUnusedTypevars,
    SpecializeObjectNew,
    ObjectNewCallable,
    NewReturnAssignable,
}

pub(in crate::types) struct QueuedConstructorEffects<'eval, 'db, 'c> {
    pub router: &'eval Router<'db, 'c>,
    pub parent: ConstructorCallableRequest<'db>,
}

impl sealed::Sealed for QueuedConstructorEffects<'_, '_, '_> {}

impl<'db> ConstructorEffects<'db> for QueuedConstructorEffects<'_, 'db, '_> {
    type Error = Boundary;

    async fn instance_approximation(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let class = match receiver {
            Type::GenericAlias(alias) => ClassType::Generic(alias),
            Type::SubclassOf(subclass)
                if let SubclassOfInner::Class(class) = subclass.subclass_of() =>
            {
                class
            }
            _ => {
                return Err(Boundary::ConstructorEffect(
                    ConstructorEffect::InstanceApproximation,
                ));
            }
        };
        Type::instance_with(db, env, &QueuedInstanceEffects, class)
            .await
            .map(Some)
            .map_err(Boundary::InstanceEffect)
    }

    fn metaclass_call(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _members: ConstructorMembers<'db>,
    ) -> impl Future<Output = Result<ConstructorMember<'db>, Self::Error>> {
        ready(Err(Boundary::ConstructorEffect(
            ConstructorEffect::MetaclassCall,
        )))
    }

    fn new_method(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _members: ConstructorMembers<'db>,
    ) -> impl Future<Output = Result<ConstructorMember<'db>, Self::Error>> {
        ready(Err(Boundary::ConstructorEffect(
            ConstructorEffect::NewMethod,
        )))
    }

    fn raw_initializer(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _members: ConstructorMembers<'db>,
    ) -> impl Future<Output = Result<Place<'db>, Self::Error>> {
        ready(Err(Boundary::ConstructorEffect(
            ConstructorEffect::RawInitializer,
        )))
    }

    fn bind_initializer(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _members: ConstructorMembers<'db>,
        _initializer: Type<'db>,
    ) -> impl Future<Output = Result<InitializerBinding<'db>, Self::Error>> {
        ready(Err(Boundary::ConstructorEffect(
            ConstructorEffect::BindInitializer,
        )))
    }

    fn object_new(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _members: ConstructorMembers<'db>,
    ) -> impl Future<Output = Result<Place<'db>, Self::Error>> {
        ready(Err(Boundary::ConstructorEffect(
            ConstructorEffect::ObjectNew,
        )))
    }

    async fn convert(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        request: CallableConversionRequest<'db>,
        _origin: DescriptorOrigin<'db>,
    ) -> Result<Option<CallableTypes<'db>>, Self::Error> {
        self.router
            .constructor_conversion_demand(self.parent, request)
            .await
    }

    fn is_actual_enum(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _class: ClassType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Err(Boundary::ConstructorEffect(
            ConstructorEffect::EnumMetadata,
        )))
    }

    fn class_generic_context(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _class: ClassType<'db>,
    ) -> impl Future<Output = Result<Option<GenericContext<'db>>, Self::Error>> {
        ready(Err(Boundary::ConstructorEffect(
            ConstructorEffect::ClassGenericContext,
        )))
    }

    fn expand_initializer(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _initializer: Type<'db>,
    ) -> impl Future<Output = Result<Option<UnionType<'db>>, Self::Error>> {
        ready(Err(Boundary::ConstructorEffect(
            ConstructorEffect::ExpandInitializer,
        )))
    }

    fn bind_new_self(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _callable: CallableType<'db>,
        _receiver: Type<'db>,
        _instance: Type<'db>,
    ) -> impl Future<Output = Result<CallableType<'db>, Self::Error>> {
        ready(Err(Boundary::ConstructorEffect(
            ConstructorEffect::BindNewSelf,
        )))
    }

    fn initializer_self_annotation(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        method: Option<BoundMethodType<'db>>,
        _signature: &Signature<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        ready(if method.is_none() {
            Ok(None)
        } else {
            Err(Boundary::ConstructorEffect(
                ConstructorEffect::InitializerSelfAnnotation,
            ))
        })
    }

    fn merge_generic_context(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        left: Option<GenericContext<'db>>,
        right: Option<GenericContext<'db>>,
    ) -> impl Future<Output = Result<Option<GenericContext<'db>>, Self::Error>> {
        ready(match (left, right) {
            (None, context) | (context, None) => Ok(context),
            (Some(_), Some(_)) => Err(Boundary::ConstructorEffect(
                ConstructorEffect::MergeGenericContext,
            )),
        })
    }

    fn bind_initializer_signature(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _signature: Signature<'db>,
        _method: BoundMethodType<'db>,
    ) -> impl Future<Output = Result<Signature<'db>, Self::Error>> {
        ready(Err(Boundary::ConstructorEffect(
            ConstructorEffect::BindInitializerSignature,
        )))
    }

    fn remove_unused_typevars(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        signature: Signature<'db>,
    ) -> impl Future<Output = Result<Signature<'db>, Self::Error>> {
        ready(if signature.generic_context.is_none() {
            Ok(signature)
        } else {
            Err(Boundary::ConstructorEffect(
                ConstructorEffect::RemoveUnusedTypevars,
            ))
        })
    }

    fn specialize_object_new(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _function: FunctionType<'db>,
        _context: GenericContext<'db>,
    ) -> impl Future<Output = Result<FunctionType<'db>, Self::Error>> {
        ready(Err(Boundary::ConstructorEffect(
            ConstructorEffect::SpecializeObjectNew,
        )))
    }

    fn object_new_callable(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _function: FunctionType<'db>,
        _instance: Type<'db>,
    ) -> impl Future<Output = Result<Option<CallableType<'db>>, Self::Error>> {
        ready(Err(Boundary::ConstructorEffect(
            ConstructorEffect::ObjectNewCallable,
        )))
    }

    fn new_return_assignable(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _return_type: Type<'db>,
        _instance: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Err(Boundary::ConstructorEffect(
            ConstructorEffect::NewReturnAssignable,
        )))
    }
}
