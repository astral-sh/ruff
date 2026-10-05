//! Decide when callable argument checking is deferred for a parameter containing `TypeVarTuple`.
//!
//! Ordinary and controlled call checking share this workaround for `TypeVarTuple` inference
//! from `*args` and preserving correlations across overloads. The declared callable's parameter
//! annotations are checked before converting the argument or expected type.

use std::convert::Infallible;
use std::slice;

use ty_mapping_probe_macros::shared_semantic_family;

use crate::types::callable::{CallableConversionRequest, UpcastPolicy};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::signatures::{CallableSignature, Parameter, Signature};
use crate::types::visitor::any_over_type;
use crate::types::{CallableType, CallableTypes, Type};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct TypeVarTupleCallableFacts;

#[derive(Clone, Copy)]
pub(in crate::types) enum CallableInspection {
    TypeVarTupleParameters,
    Overloaded,
    Generic,
    DynamicVariadic,
}

pub(super) struct OrdinaryTypeVarTupleCallableEffects<'env, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'env ProgramEnvironment<'db>,
    pub(super) recursion_guard: Option<&'env CallableRecursionGuard<'db>>,
}

shared_semantic_family! {
    #[synchronous(SynchronousTypeVarTupleCallableEffects)]
    pub(in crate::types) trait TypeVarTupleCallableEffects<'db> {
        type Error;

        #[operation(child)]
        async fn upcast(&self, ty: Type<'db>) -> Result<Option<CallableTypes<'db>>, Self::Error>;
        #[operation(source)]
        async fn signatures(&self, callable: CallableType<'db>) -> Result<&'db CallableSignature<'db>, Self::Error>;
        #[operation(local)]
        async fn cursor<'a, T>(&self, values: &'a [T]) -> Result<slice::Iter<'a, T>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next<'a, T>(&self, cursor: &mut slice::Iter<'a, T>) -> Result<Option<&'a T>, Self::Error>;
        #[operation(child)]
        async fn contains_typevartuple(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn inspect(&self, callables: &CallableTypes<'db>, inspection: CallableInspection) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn inspect_signature(&self, signature: &Signature<'db>, inspection: CallableInspection) -> Result<bool, Self::Error>;
    }

    #[finite_capability]
    impl TypeVarTupleCallableFacts {
        fn callables<'a, 'db>(&self, callables: &'a CallableTypes<'db>) -> &'a [CallableType<'db>] {
            callables.iter().as_slice()
        }

        fn callable<'db>(&self, callable: &CallableType<'db>) -> CallableType<'db> {
            *callable
        }

        fn signatures<'a, 'db>(&self, signatures: &'a CallableSignature<'db>) -> &'a [Signature<'db>] {
            signatures.iter().as_slice()
        }

        fn parameters<'a, 'db>(&self, signature: &'a Signature<'db>) -> &'a [Parameter<'db>] {
            signature.parameters().iter().as_slice()
        }

        fn overloaded(&self, signatures: &CallableSignature<'_>) -> bool {
            signatures.overloads.len() > 1
        }

        fn generic(&self, signature: &Signature<'_>) -> bool {
            signature.generic_context.is_some()
        }

        fn variadic(&self, parameter: &Parameter<'_>) -> bool {
            parameter.is_variadic()
        }

        fn annotation<'db>(&self, parameter: &Parameter<'db>) -> Type<'db> {
            parameter.annotated_type()
        }

        fn dynamic(&self, parameter: &Parameter<'_>) -> bool {
            parameter.annotated_type().is_dynamic()
        }
    }

    #[synchronous(should_defer_typevartuple_callable_sync)]
    #[capabilities(effects = TypeVarTupleCallableEffects)]
    #[passive_values(CallableInspection::TypeVarTupleParameters, CallableInspection::Overloaded, CallableInspection::Generic, CallableInspection::DynamicVariadic)]
    pub(in crate::types) async fn should_defer_typevartuple_callable_with<'db, E: TypeVarTupleCallableEffects<'db>>(
        declared: Type<'db>, expected: Type<'db>, argument: Type<'db>, effects: &E,
    ) -> Result<bool, E::Error> {
        let Some(declared_callables) = effects.upcast(declared).await? else {
            return Ok(false);
        };
        if !effects.inspect(&declared_callables, CallableInspection::TypeVarTupleParameters).await? {
            return Ok(false);
        }
        let Some(argument_callables) = effects.upcast(argument).await? else {
            return Ok(false);
        };
        if effects.inspect(&argument_callables, CallableInspection::Overloaded).await? {
            return Ok(true);
        }
        if !effects.inspect(&argument_callables, CallableInspection::Generic).await? {
            return Ok(false);
        }
        let Some(expected_callables) = effects.upcast(expected).await? else {
            return Ok(false);
        };
        effects.inspect(&expected_callables, CallableInspection::DynamicVariadic).await
    }

    #[synchronous(inspect_callables_sync)]
    #[capabilities(effects = TypeVarTupleCallableEffects, facts = TypeVarTupleCallableFacts)]
    #[passive_values()]
    pub(in crate::types) async fn inspect_callables_with<'db, E: TypeVarTupleCallableEffects<'db>>(
        callables: &CallableTypes<'db>, inspection: CallableInspection, facts: TypeVarTupleCallableFacts, effects: &E,
    ) -> Result<bool, E::Error> {
        let mut callables = effects.cursor(facts.callables(callables)).await?;
        #[cursor_loop]
        while let Some(callable) = effects.next(&mut callables).await? {
            let signatures = effects.signatures(facts.callable(callable)).await?;
            if let CallableInspection::Overloaded = inspection {
                if facts.overloaded(signatures) {
                    return Ok(true);
                }
            } else {
                let mut signatures = effects.cursor(facts.signatures(signatures)).await?;
                #[cursor_loop]
                while let Some(signature) = effects.next(&mut signatures).await? {
                    if effects.inspect_signature(signature, inspection).await? {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    #[synchronous(inspect_signature_sync)]
    #[capabilities(effects = TypeVarTupleCallableEffects, facts = TypeVarTupleCallableFacts)]
    #[passive_values()]
    pub(in crate::types) async fn inspect_signature_with<'db, E: TypeVarTupleCallableEffects<'db>>(
        signature: &Signature<'db>, inspection: CallableInspection, facts: TypeVarTupleCallableFacts, effects: &E,
    ) -> Result<bool, E::Error> {
        match inspection {
            CallableInspection::Generic => Ok(facts.generic(signature)),
            CallableInspection::Overloaded => Ok(false),
            CallableInspection::TypeVarTupleParameters => {
                let mut parameters = effects.cursor(facts.parameters(signature)).await?;
                #[cursor_loop]
                while let Some(parameter) = effects.next(&mut parameters).await? {
                    if effects.contains_typevartuple(facts.annotation(parameter)).await? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            CallableInspection::DynamicVariadic => {
                let mut parameters = effects.cursor(facts.parameters(signature)).await?;
                #[cursor_loop]
                while let Some(parameter) = effects.next(&mut parameters).await? {
                    if facts.variadic(parameter) {
                        return Ok(facts.dynamic(parameter));
                    }
                }
                Ok(false)
            }
        }
    }
}

impl<'db> SynchronousTypeVarTupleCallableEffects<'db>
    for OrdinaryTypeVarTupleCallableEffects<'_, 'db>
{
    type Error = Infallible;

    fn upcast(&self, ty: Type<'db>) -> Result<Option<CallableTypes<'db>>, Infallible> {
        Ok(
            CallableConversionRequest::new(ty, UpcastPolicy::default()).evaluate(
                self.db,
                self.env,
                self.recursion_guard,
            ),
        )
    }

    fn signatures(
        &self,
        callable: CallableType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Infallible> {
        Ok(callable.signatures(self.db))
    }

    fn cursor<'a, T>(&self, values: &'a [T]) -> Result<slice::Iter<'a, T>, Infallible> {
        Ok(values.iter())
    }

    fn next<'a, T>(&self, cursor: &mut slice::Iter<'a, T>) -> Result<Option<&'a T>, Infallible> {
        Ok(cursor.next())
    }

    fn contains_typevartuple(&self, ty: Type<'db>) -> Result<bool, Infallible> {
        Ok(any_over_type(
            self.db,
            self.env,
            ty,
            false,
            |ty| matches!(ty, Type::TypeVar(typevar) if typevar.is_typevartuple(self.db)),
        ))
    }

    fn inspect(
        &self,
        callables: &CallableTypes<'db>,
        inspection: CallableInspection,
    ) -> Result<bool, Infallible> {
        inspect_callables_sync(callables, inspection, TypeVarTupleCallableFacts, self)
    }

    fn inspect_signature(
        &self,
        signature: &Signature<'db>,
        inspection: CallableInspection,
    ) -> Result<bool, Infallible> {
        inspect_signature_sync(signature, inspection, TypeVarTupleCallableFacts, self)
    }
}
