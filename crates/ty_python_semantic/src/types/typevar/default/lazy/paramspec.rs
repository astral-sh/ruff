//! Converts inferred ParamSpec defaults into the parameter values consumed by specialization.

use std::convert::Infallible;

use super::OrdinaryLazyDefaultEffects;
use crate::types::callable::{CallableType, CallableTypeKind};
use crate::types::signatures::{CallableSignature, ParametersKind, Signature};
use crate::types::tuple::{Tuple, TupleSpec, TupleType};
use crate::types::typevar::TypeVarInstance;
use crate::types::{BoundTypeVarInstance, DynamicType, KnownClass, KnownInstanceType, NominalInstanceType, Parameter, Parameters, Type};

/// The inferred forms that have distinct ParamSpec-default conversion rules.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum ParamSpecDefaultInput<'db> {
    Nominal(NominalInstanceType<'db>),
    Bound(BoundTypeVarInstance<'db>),
    Unbound(TypeVarInstance<'db>),
    Todo,
    Unknown,
}

impl<'db> ParamSpecDefaultInput<'db> {
    pub(in crate::types) const fn new(ty: Type<'db>) -> Self {
        match ty {
            Type::NominalInstance(instance) => Self::Nominal(instance),
            Type::TypeVar(variable) => Self::Bound(variable),
            Type::KnownInstance(KnownInstanceType::TypeVar(variable)) => Self::Unbound(variable),
            Type::Dynamic(DynamicType::Todo(_)) => Self::Todo,
            // All other inferred forms recover as unknown parameters, including invalid defaults.
            _ => Self::Unknown,
        }
    }
}

/// The existing gradual parameter constructors used when a default is not a fixed list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::types) enum ParamSpecRecovery {
    Gradual,
    Todo,
    Unknown,
}

impl ParamSpecRecovery {
    pub(in crate::types) fn parameters<'db>(self) -> Parameters<'db> {
        match self {
            Self::Gradual => Parameters::gradual_form(),
            Self::Todo => Parameters::todo(),
            Self::Unknown => Parameters::unknown(),
        }
    }
}

/// Borrows the entries of a fixed tuple specification; variable tuple specifications have no fixed list.
pub(in crate::types) fn fixed_elements<'db>(spec: &'db TupleSpec<'db>) -> Option<&'db [Type<'db>]> {
    match spec {
        Tuple::Fixed(tuple) => Some(tuple.elements_slice()),
        // A ParamSpec default cannot contain a variable-length tuple; it is an invalid type expression.
        Tuple::Variable(_) => None,
    }
}

/// Advances a borrowed tuple cursor by one entry without retaining an owned type buffer.
pub(in crate::types) fn next_type<'db>(remaining: &mut &'db [Type<'db>]) -> Option<Type<'db>> {
    let (first, rest) = remaining.split_first()?;
    *remaining = rest;
    Some(*first)
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousParamSpecDefaultEffects)]
    pub(in crate::types) trait ParamSpecDefaultEffects<'db> {
        type Error;

        #[operation(local)]
        async fn classify(&self, ty: Type<'db>) -> Result<ParamSpecDefaultInput<'db>, Self::Error>;
        #[operation(source)]
        async fn nominal_known(&self, instance: NominalInstanceType<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(local)]
        async fn exact_tuple(&self, instance: NominalInstanceType<'db>) -> Result<Option<TupleType<'db>>, Self::Error>;
        #[operation(source)]
        async fn elements(&self, tuple: TupleType<'db>) -> Result<Option<&'db [Type<'db>]>, Self::Error>;
        #[operation(source)]
        async fn bound_is_paramspec(&self, variable: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn unbound_is_paramspec(&self, variable: TypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn new_parameters(&self, elements: &[Type<'db>]) -> Result<Vec<Parameter<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next(&self, remaining: &mut &'db [Type<'db>]) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn push(&self, parameters: &mut Vec<Parameter<'db>>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish(&self, parameters: Vec<Parameter<'db>>) -> Result<Parameters<'db>, Self::Error>;
        #[operation(local)]
        async fn recovery(&self, recovery: ParamSpecRecovery) -> Result<Parameters<'db>, Self::Error>;
        #[operation(child)]
        async fn callable(&self, parameters: Parameters<'db>) -> Result<Type<'db>, Self::Error>;
    }

    /// Converts an inferred default to a parameter-value callable, or preserves an existing ParamSpec.
    /// Fixed lists become positional parameters; other forms use the corresponding recovery parameters.
    #[synchronous(paramspec_default_sync)]
    #[capabilities(effects = ParamSpecDefaultEffects)]
    #[passive_values(ParamSpecRecovery::Gradual, ParamSpecRecovery::Todo, ParamSpecRecovery::Unknown, KnownClass::EllipsisType)]
    pub(in crate::types) async fn paramspec_default_with<'db, E: ParamSpecDefaultEffects<'db>>(
        ty: Type<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let parameters = match effects.classify(ty).await? {
            ParamSpecDefaultInput::Nominal(instance) => {
                match effects.nominal_known(instance).await? {
                    Some(KnownClass::EllipsisType) => effects.recovery(ParamSpecRecovery::Gradual).await?,
                    _ => {
                        let elements = match effects.exact_tuple(instance).await? {
                            Some(tuple) => effects.elements(tuple).await?,
                            None => None,
                        };
                        match elements {
                            Some(mut remaining) => {
                                let mut parameters = effects.new_parameters(remaining).await?;
                                #[cursor_loop]
                                while let Some(ty) = effects.next(&mut remaining).await? {
                                    effects.push(&mut parameters, ty).await?;
                                }
                                effects.finish(parameters).await?
                            }
                            None => effects.recovery(ParamSpecRecovery::Unknown).await?,
                        }
                    }
                }
            }
            ParamSpecDefaultInput::Bound(variable) => {
                if effects.bound_is_paramspec(variable).await? {
                    return Ok(ty);
                }
                effects.recovery(ParamSpecRecovery::Unknown).await?
            }
            ParamSpecDefaultInput::Unbound(variable) => {
                if effects.unbound_is_paramspec(variable).await? {
                    return Ok(ty);
                }
                effects.recovery(ParamSpecRecovery::Unknown).await?
            }
            ParamSpecDefaultInput::Todo => effects.recovery(ParamSpecRecovery::Todo).await?,
            ParamSpecDefaultInput::Unknown => effects.recovery(ParamSpecRecovery::Unknown).await?,
        };
        effects.callable(parameters).await
    }
}

impl<'db> SynchronousParamSpecDefaultEffects<'db> for OrdinaryLazyDefaultEffects<'db> {
    type Error = Infallible;

    fn classify(&self, ty: Type<'db>) -> Result<ParamSpecDefaultInput<'db>, Infallible> {
        Ok(ParamSpecDefaultInput::new(ty))
    }

    fn nominal_known(&self, instance: NominalInstanceType<'db>) -> Result<Option<KnownClass>, Infallible> {
        Ok(instance.known_class(self.db))
    }

    fn exact_tuple(&self, instance: NominalInstanceType<'db>) -> Result<Option<TupleType<'db>>, Infallible> {
        Ok(instance.exact_tuple())
    }

    fn elements(&self, tuple: TupleType<'db>) -> Result<Option<&'db [Type<'db>]>, Infallible> {
        Ok(fixed_elements(tuple.tuple(self.db)))
    }

    fn bound_is_paramspec(&self, variable: BoundTypeVarInstance<'db>) -> Result<bool, Infallible> {
        Ok(variable.is_paramspec(self.db))
    }

    fn unbound_is_paramspec(&self, variable: TypeVarInstance<'db>) -> Result<bool, Infallible> {
        Ok(variable.is_paramspec(self.db))
    }

    fn new_parameters(&self, elements: &[Type<'db>]) -> Result<Vec<Parameter<'db>>, Infallible> {
        Ok(Vec::with_capacity(elements.len()))
    }

    fn next(&self, remaining: &mut &'db [Type<'db>]) -> Result<Option<Type<'db>>, Infallible> {
        Ok(next_type(remaining))
    }

    fn push(&self, parameters: &mut Vec<Parameter<'db>>, ty: Type<'db>) -> Result<(), Infallible> {
        parameters.push(Parameter::positional_only(None).with_annotated_type(ty));
        Ok(())
    }

    fn finish(&self, parameters: Vec<Parameter<'db>>) -> Result<Parameters<'db>, Infallible> {
        Ok(Parameters::from_owned(parameters.into_boxed_slice(), ParametersKind::Standard))
    }

    fn recovery(&self, recovery: ParamSpecRecovery) -> Result<Parameters<'db>, Infallible> {
        Ok(recovery.parameters())
    }

    fn callable(&self, parameters: Parameters<'db>) -> Result<Type<'db>, Infallible> {
        let signature = Signature::new(parameters, Type::unknown()).into_paramspec_value();
        Ok(Type::Callable(CallableType::new(self.db, CallableSignature::single(signature), CallableTypeKind::ParamSpecValue)))
    }
}
