//! Transform lambda signatures without making their inferred return types part of their identity.

use crate::types::TypeOperations;
use crate::types::recursive::RecursiveReferenceMapper;
use crate::types::visitor::{TypeVisit, TypeVisitorContext, TypeVisitorHooks};
use std::cell::RefCell;

use rustc_hash::FxHashSet;
use salsa::plumbing::AsId;

use super::{
    CallableType, LambdaSignature, LambdaSource, SignatureSource, infer_lambda_signature,
    walk_callable_type,
};
use crate::types::constraints::resolution::type_dependencies;
use crate::types::generics::{
    ApplySpecialization, GenericContext, Specialization, walk_specialization_types,
};
use crate::types::infer::InferenceEnvironment;
use crate::types::typevar::TypeVarSet;
use crate::types::variance::{VarianceInferable, VarianceOrigin, VarianceTerm};
use crate::types::visitor::{TypeCollector, TypeVisitor, any_over_type_including_alias_arguments};
use crate::types::{
    ApplyTypeMappingVisitor, BindingContext, BoundTypeVarIdentity, BoundTypeVarInstance,
    MaterializationKind, Parameters, PromotionKind, PromotionMode, RecursiveType, SelfBinding,
    Type, TypeAliasType, TypeContext, TypeMapping, TypeVarVariance,
};
use crate::{Db, FxOrderSet, Program, ProgramEnvironment};
use ty_python_core::{scope::ScopeId, semantic_index};

impl<'db> Type<'db> {
    /// The source lambdas contained in this value, without expanding their signatures or inputs.
    pub(in crate::types) fn lambda_inference_sources(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
    ) -> &'db [ScopeId<'db>] {
        lambda_inference_sources(db, env.program(db), self, false)
    }

    /// Whether this type contains a source lambda or a saved inference input that depends on it.
    /// This is an occurs check on inference inputs, independent of the argument's outer type.
    pub(in crate::types) fn depends_on_lambda_inference(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        sources: &[ScopeId<'db>],
    ) -> bool {
        if sources.is_empty() {
            return false;
        }
        let dependencies = lambda_inference_sources(db, env.program(db), self, true);
        sources.iter().any(|source| dependencies.contains(source))
    }
}

/// Summarize only stored type structure. Evaluating a lambda body while collecting its input
/// dependencies would turn this check into another source of recursive inference.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn lambda_inference_sources<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
    ty: Type<'db>,
    include_inputs: bool,
) -> Box<[ScopeId<'db>]> {
    struct SourceVisitor<'db> {
        env: TypeOperations<'db>,
        include_inputs: bool,
        visited: TypeCollector<'db>,
        sources: RefCell<FxHashSet<ScopeId<'db>>>,
    }

    impl<'db> TypeVisitorHooks<'db> for SourceVisitor<'db> {
        fn program_environment(&self) -> &TypeOperations<'db> {
            &self.env
        }

        fn should_visit_lazy_type_attributes(&self) -> bool {
            false
        }

        fn visit_type(&self, db: &'db dyn Db, visit: &TypeVisit<'_, 'db, Self>, _ty: Type<'db>) {
            visit.walk_children_once(db, &self.visited);
        }

        fn visit_callable_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            callable: CallableType<'db>,
        ) {
            match callable.signature_source(db) {
                SignatureSource::Lambda(lambda) => {
                    self.sources.borrow_mut().insert(lambda.scope(db));
                    if self.include_inputs {
                        lambda.walk_inference_inputs(db, visit);
                    }
                }
                SignatureSource::Explicit(_) | SignatureSource::Projected(_) => {
                    walk_callable_type(db, callable, visit);
                }
            }
        }

        fn visit_type_alias_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            alias: TypeAliasType<'db>,
        ) {
            if let Some(arguments) = alias.specialization(db) {
                walk_specialization_types(db, arguments, visit);
            }
        }

        fn visit_recursive_type(
            &self,
            db: &'db dyn Db,
            visit: &TypeVisit<'_, 'db, Self>,
            recursive: RecursiveType<'db>,
        ) {
            if let Some(arguments) = recursive.arguments(db) {
                walk_specialization_types(db, arguments, visit);
            }
        }
    }

    let visitor = SourceVisitor {
        env: TypeOperations::new_root(db, ProgramEnvironment::from_program(program)),
        include_inputs,
        visited: TypeCollector::default(),
        sources: RefCell::default(),
    };
    TypeVisitorContext::new(&visitor).visit_type(db, ty);
    let mut sources: Box<[_]> = visitor.sources.into_inner().into_iter().collect();
    sources.sort_unstable_by_key(AsId::as_id);
    sources
}

/// Original inference inputs and the saved transformations of a lambda.
/// Each view stores its transformations directly, so expanding it never has to evaluate
/// another transformed view first.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct LambdaSignatureMapping<'db> {
    /// The original lambda, before any transformations.
    source: LambdaSignature<'db>,
    steps: Box<[LambdaTransformation<'db>]>,
}

/// Inputs that distinguish completed signatures of one source lambda during comparison.
/// Inference still uses the original lambda and all of its captured inputs.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(in crate::types) struct LambdaSignatureKey<'db> {
    source: LambdaSignature<'db>,
    parameters: Parameters<'db>,
    promotions: Box<[LambdaTransformation<'db>]>,
    arguments: Box<[(BoundTypeVarInstance<'db>, [Type<'db>; 2])]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
struct LambdaTransformation<'db> {
    mapping: LambdaMapping<'db>,
    context: TypeContext<'db>,
    materialize_typevar_bounds_and_defaults: bool,
}

impl<'db> LambdaTransformation<'db> {
    fn parameters(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        parameters: Parameters<'db>,
    ) -> Parameters<'db> {
        let parameters = match &self.mapping {
            LambdaMapping::BindParameters(bound) => return bound.clone(),
            LambdaMapping::BindReceiver { .. } => parameters.bind_receiver(db),
            _ => parameters,
        };
        let visitor = ApplyTypeMappingVisitor {
            materialize_typevar_bounds_and_defaults: self.materialize_typevar_bounds_and_defaults,
            ..ApplyTypeMappingVisitor::new(env)
        };
        match self.mapping.as_type_mapping() {
            Some(mapping) => {
                parameters.apply_type_mapping_impl(db, &mapping, self.context, &visitor)
            }
            None => parameters,
        }
    }

    fn signature(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        mut signature: super::CallableSignature<'db>,
    ) -> super::CallableSignature<'db> {
        match &self.mapping {
            LambdaMapping::BindParameters(parameters) => {
                for overload in &mut signature.overloads {
                    *overload = overload.clone().with_parameters(parameters.clone());
                }
                signature
            }
            LambdaMapping::BindReceiver {
                receiver,
                self_binding,
            } => signature.bind_method_receiver(db, env, *receiver, self_binding.ty),
            LambdaMapping::ApplyReceiver {
                receiver,
                self_binding,
            } => signature.apply_self_with_receiver(db, env, *receiver, self_binding.ty),
            _ => {
                let visitor = ApplyTypeMappingVisitor {
                    materialize_typevar_bounds_and_defaults: self
                        .materialize_typevar_bounds_and_defaults,
                    ..ApplyTypeMappingVisitor::new(env)
                };
                match self.mapping.as_type_mapping() {
                    Some(mapping) => {
                        signature.apply_type_mapping_impl(db, &mapping, self.context, &visitor)
                    }
                    None => signature,
                }
            }
        }
    }
}

impl<'db> LambdaSignatureMapping<'db> {
    /// Expand the original body once, then apply the normalized transformations in order.
    /// A plan never recursively opens another transformed lambda to recover its source.
    pub(super) fn signature(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
    ) -> super::CallableSignature<'db> {
        let source = self.source;
        let env = env.in_environment(ProgramEnvironment::from_scope(source.scope(db)));
        self.steps.iter().fold(
            infer_lambda_signature(db, LambdaSource::of(db, source)).clone(),
            |signature, step| step.signature(db, &env, signature),
        )
    }
}

/// An owned semantic mapping that can be applied when a lambda's return type is requested.
/// Structural substitutions never enter an inferred body and are handled separately.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum LambdaMapping<'db> {
    Specialize {
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
        materialization: Option<MaterializationKind>,
    },
    Promote(PromotionMode, PromotionKind),
    BindLegacyTypevars(BindingContext<'db>),
    FreshenBoundTypeVars(GenericContext<'db>, u32),
    BindSelf(SelfBinding<'db>),
    ReplaceSelf(Type<'db>),
    Materialize(MaterializationKind),
    ReplaceParameterDefaults,

    /// The exposed parameters have been bound; the source still determines the return type.
    BindParameters(Parameters<'db>),
    /// Bind a receiver without expanding the source signature.
    BindReceiver {
        receiver: Type<'db>,
        self_binding: SelfBinding<'db>,
    },
    /// Substitute the receiver of a signature whose visible parameters are already bound.
    ApplyReceiver {
        receiver: Type<'db>,
        self_binding: SelfBinding<'db>,
    },
}

impl<'db> LambdaMapping<'db> {
    /// Own the mapping's inputs, retaining only substitutions available to this lambda.
    pub(super) fn from_type_mapping(
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        lambda: LambdaSignature<'db>,
        mapping: &TypeMapping<'_, 'db>,
    ) -> Option<Self> {
        Some(match mapping {
            TypeMapping::ApplySpecialization(specialization)
            | TypeMapping::ApplySpecializationWithMaterialization { specialization, .. } => {
                if specialization.preserves_lazy_signatures() {
                    return None;
                }
                Self::Specialize {
                    specialization: {
                        let (variables, types): (Vec<_>, Vec<_>) = lambda
                            .captured_typevars(db, env)
                            .variables(db)
                            .filter_map(|variable| {
                                specialization.get(db, variable).map(|ty| (variable, ty))
                            })
                            .unzip();
                        // An absent substitution can still specialize a Self bound. Adding
                        // an identity substitution would suppress that existing behavior.
                        let context = GenericContext::from_typevar_instances(
                            db,
                            &ProgramEnvironment::from_scope(lambda.scope(db)),
                            variables,
                        );
                        Specialization::new(db, context, types.into_boxed_slice(), None, None)
                    },
                    specialize_self_domain: specialization.specialize_self_domain(),
                    materialization: match mapping {
                        TypeMapping::ApplySpecializationWithMaterialization {
                            materialization_kind,
                            ..
                        } => Some(*materialization_kind),
                        _ => None,
                    },
                }
            }
            TypeMapping::Promote(mode, kind) => Self::Promote(*mode, *kind),
            TypeMapping::BindLegacyTypevars(context) => Self::BindLegacyTypevars(*context),
            TypeMapping::FreshenBoundTypeVars {
                generic_context,
                delta,
            } => Self::FreshenBoundTypeVars(*generic_context, *delta),
            TypeMapping::BindSelf(binding) => Self::BindSelf(binding.clone()),
            TypeMapping::ReplaceSelf { new_upper_bound } => Self::ReplaceSelf(*new_upper_bound),
            TypeMapping::Materialize(kind) => Self::Materialize(*kind),
            TypeMapping::ReplaceParameterDefaults => Self::ReplaceParameterDefaults,

            TypeMapping::RescopeReturnCallables(_) => {
                return None;
            }
        })
    }

    fn as_type_mapping(&self) -> Option<TypeMapping<'_, 'db>> {
        Some(match self {
            Self::Specialize {
                specialization,
                specialize_self_domain,
                materialization,
            } => {
                let specialization = ApplySpecialization::Specialization {
                    specialization: *specialization,
                    specialize_self_domain: *specialize_self_domain,
                };
                match materialization {
                    Some(kind) => TypeMapping::ApplySpecializationWithMaterialization {
                        specialization,
                        materialization_kind: *kind,
                    },
                    None => TypeMapping::ApplySpecialization(specialization),
                }
            }
            Self::Promote(mode, kind) => TypeMapping::Promote(*mode, *kind),
            Self::BindLegacyTypevars(context) => TypeMapping::BindLegacyTypevars(*context),
            Self::FreshenBoundTypeVars(generic_context, delta) => {
                TypeMapping::FreshenBoundTypeVars {
                    generic_context: *generic_context,
                    delta: *delta,
                }
            }
            Self::BindSelf(binding) => TypeMapping::BindSelf(binding.clone()),
            Self::ReplaceSelf(new_upper_bound) => TypeMapping::ReplaceSelf {
                new_upper_bound: *new_upper_bound,
            },
            Self::Materialize(kind) => TypeMapping::Materialize(*kind),
            Self::ReplaceParameterDefaults => TypeMapping::ReplaceParameterDefaults,

            Self::BindParameters(_) => return None,
            Self::BindReceiver { self_binding, .. } | Self::ApplyReceiver { self_binding, .. } => {
                TypeMapping::BindSelf(self_binding.clone())
            }
        })
    }
}

impl<'db> CallableType<'db> {
    /// Reuse a view of this expression when its parameter and captured inputs are unchanged.
    /// Its inferred return is an output of those inputs, not another input for the same body.
    pub(in crate::types) fn lambda_with_same_inputs(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        scope: ScopeId<'db>,
        parameters: &Parameters<'db>,
        environment: InferenceEnvironment<'db>,
    ) -> Option<(LambdaSignature<'db>, LambdaSignature<'db>)> {
        let SignatureSource::Lambda(lambda) = self.signature_source(db) else {
            return None;
        };
        let source = lambda.source(db);
        if source.scope(db) != scope
            || source.parameters(db) != parameters
            || lambda.parameters(db) != parameters
            || source.environment(db) != environment
            || !lambda
                .captured_arguments(db, env)
                .all(|(parameter, arguments)| arguments == [Type::TypeVar(parameter); 2])
        {
            return None;
        }
        Some((source, *lambda))
    }
}

#[salsa::tracked]
impl<'db> LambdaSignature<'db> {
    pub(in crate::types) fn map_recursive_references(
        self,
        db: &'db dyn Db,
        mapper: &RecursiveReferenceMapper<'db>,
    ) -> Self {
        let map = |ty| mapper.map_type(db, ty);
        let deferred = self.mapping(db).as_ref().map(|plan| {
            let steps = plan
                .steps
                .iter()
                .cloned()
                .map(|mut step| {
                    match &mut step.mapping {
                        LambdaMapping::Specialize { specialization, .. } => {
                            *specialization = specialization.map_recursive_references(db, mapper);
                        }
                        LambdaMapping::BindSelf(binding) => binding.ty = map(binding.ty),
                        LambdaMapping::ReplaceSelf(ty) => *ty = map(*ty),
                        LambdaMapping::BindReceiver {
                            receiver,
                            self_binding,
                        }
                        | LambdaMapping::ApplyReceiver {
                            receiver,
                            self_binding,
                        } => {
                            *receiver = map(*receiver);
                            self_binding.ty = map(self_binding.ty);
                        }
                        LambdaMapping::BindParameters(parameters) => {
                            *parameters = parameters.map_recursive_references(db, mapper);
                        }
                        LambdaMapping::Promote(..)
                        | LambdaMapping::BindLegacyTypevars(_)
                        | LambdaMapping::FreshenBoundTypeVars(..)
                        | LambdaMapping::Materialize(_)
                        | LambdaMapping::ReplaceParameterDefaults => {}
                    }
                    step.context = step
                        .context
                        .with_annotation(step.context.annotation.map(map));
                    step
                })
                .collect();
            LambdaSignatureMapping {
                source: plan.source.map_recursive_references(db, mapper),
                steps,
            }
        });
        Self::new(
            db,
            self.parameters(db).map_recursive_references(db, mapper),
            self.scope(db),
            self.body(db),
            self.return_context(db)
                .with_annotation(self.return_context(db).annotation.map(map)),
            self.environment(db),
            deferred,
        )
    }

    /// Compose the source return equation with its captured arguments. Parameter binding is
    /// already reflected in the visible parameters, so it never requires opening the return.
    pub(super) fn variance_of(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        let source = LambdaSource::of(db, self);
        let return_variance = if self.mapping(db).is_none() {
            VarianceTerm::variable(db, VarianceOrigin::LambdaReturn(source), typevar)
        } else {
            VarianceTerm::join(
                db,
                self.captured_arguments(db, env)
                    .map(|(parameter, arguments)| {
                        let argument_variance = |argument| {
                            if parameter.is_paramspec(db) {
                                Self::paramspec_argument_variance(db, env, argument, typevar)
                                    .compose_thunk(db, || TypeVarVariance::Contravariant.into())
                            } else {
                                argument.variance_of(db, env, typevar)
                            }
                        };
                        VarianceTerm::substitute(
                            db,
                            VarianceTerm::variable(
                                db,
                                VarianceOrigin::LambdaReturn(source),
                                parameter.identity(db),
                            ),
                            argument_variance(arguments[0]),
                            argument_variance(arguments[1]),
                        )
                    }),
            )
        };
        VarianceTerm::join(
            db,
            [
                self.parameters(db).variance_of(db, env, typevar),
                return_variance,
            ],
        )
    }

    /// Give an unspecialized `ParamSpec` the variance of its callable parameter-list value.
    fn paramspec_argument_variance(
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        argument: Type<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        match argument {
            Type::Union(union) => VarianceTerm::join(
                db,
                union
                    .elements(db)
                    .iter()
                    .map(|argument| Self::paramspec_argument_variance(db, env, *argument, typevar)),
            ),
            Type::TypeVar(parameter) if parameter.is_paramspec(db) => argument
                .variance_of(db, env, typevar)
                .compose_thunk(db, || TypeVarVariance::Contravariant.into()),
            _ => argument.variance_of(db, env, typevar),
        }
    }

    /// Share comparisons of specialized views when the differing captures cannot affect their
    /// signatures. Computing this during construction would re-enter inference of the source.
    pub(super) fn comparison_key(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
    ) -> Option<LambdaSignatureKey<'db>> {
        // An untransformed lambda already has a single exact identity.
        let source = self.mapping(db).as_ref()?.source;
        let promotions = self.signature_promotions(db)?;
        if source.infer_body(db).is_provisional() {
            return None;
        }
        let dependencies = source.return_type_dependencies(db, env);
        if !dependencies.iter(db).all(|variable| {
            source
                .captured_typevars(db, env)
                .variables(db)
                .any(|captured| captured == variable)
        }) {
            return None;
        }
        let arguments = if dependencies == TypeVarSet::None {
            Box::default()
        } else {
            self.captured_arguments(db, env)
                .filter(|(variable, _)| variable.is_inferable(db, dependencies))
                .collect()
        };
        Some(LambdaSignatureKey {
            source,
            parameters: self.parameters(db).without_default_types(),
            promotions,
            arguments,
        })
    }

    /// Separate promotions from substitutions whose effects are captured by the argument types.
    /// Other transformations retain their exact identity until their effects can be compared.
    fn signature_promotions(self, db: &'db dyn Db) -> Option<Box<[LambdaTransformation<'db>]>> {
        let mut promotions = Vec::new();
        if let Some(plan) = self.mapping(db) {
            for step in &plan.steps {
                match step.mapping {
                    LambdaMapping::Promote(..) => promotions.push(step.clone()),
                    LambdaMapping::Specialize {
                        materialization: None,
                        ..
                    }
                    | LambdaMapping::BindParameters(_) => {}
                    _ => return None,
                }
            }
        }
        Some(promotions.into_boxed_slice())
    }

    /// Prove that two views of the same lambda have equivalent signatures.
    ///
    /// Promotion affects both the source signature and its substituted arguments. Compare the
    /// promotions separately from the substitutions, projecting each captured variable through
    /// both variance positions. Variables absent from the return type cannot distinguish the
    /// returns, even if their substitutions keep growing through recursive calls.
    ///
    /// This proof belongs to relation checking. Querying return dependencies while constructing
    /// a transformed lambda would re-enter inference of the body that is being transformed.
    pub(in crate::types) fn has_equivalent_signature(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        other: Self,
        equivalent: impl Fn(Type<'db>, Type<'db>) -> bool,
    ) -> bool {
        if self == other {
            return true;
        }
        if !self
            .parameters(db)
            .equals_ignoring_default_types(other.parameters(db))
        {
            return false;
        }

        let transformations = |lambda: Self| {
            let promotions: FxHashSet<_> = lambda
                .signature_promotions(db)?
                .into_vec()
                .into_iter()
                .collect();
            Some((lambda.source(db), promotions))
        };
        let Some((source, promotions)) = transformations(self) else {
            return false;
        };
        let Some((other_source, other_promotions)) = transformations(other) else {
            return false;
        };
        if source != other_source || promotions != other_promotions {
            return false;
        }

        let dependencies = source.return_type_dependencies(db, env);
        if dependencies == TypeVarSet::None {
            return true;
        }
        if !dependencies.iter(db).all(|variable| {
            source
                .captured_typevars(db, env)
                .variables(db)
                .any(|captured| captured == variable)
        }) {
            return false;
        }
        let arguments = |lambda: Self| {
            lambda
                .captured_arguments(db, env)
                .filter(move |(variable, _)| variable.is_inferable(db, dependencies))
        };
        arguments(self).zip(arguments(other)).all(
            |((left_variable, left), (right_variable, right))| {
                left_variable == right_variable
                    && left
                        .into_iter()
                        .zip(right)
                        .all(|(left, right)| left == right || equivalent(left, right))
            },
        )
    }

    /// Walk the stored inputs of this lambda, including captured inputs and deferred mappings.
    /// The return type inferred from the body is deliberately outside this traversal.
    fn walk_inference_inputs(self, db: &'db dyn Db, visitor: &impl TypeVisitor<'db>) {
        for parameter in self.parameters(db) {
            visitor.visit_type(db, parameter.annotated_type());
            if let Some(default) = parameter.eager_default_type() {
                visitor.visit_type(db, default);
            }
        }
        if let Some(annotation) = self.return_context(db).annotation {
            visitor.visit_type(db, annotation);
        }
        if let Some(parent) = self.environment(db).enclosing_lambda() {
            visitor.visit_type(db, Type::Callable(CallableType::from_lambda(db, parent)));
        }
        if let Some(plan) = self.mapping(db) {
            visitor.visit_type(
                db,
                Type::Callable(CallableType::from_lambda(db, plan.source)),
            );
            for step in &plan.steps {
                if let Some(annotation) = step.context.annotation {
                    visitor.visit_type(db, annotation);
                }
                match &step.mapping {
                    LambdaMapping::Specialize { specialization, .. } => {
                        walk_specialization_types(db, *specialization, visitor);
                    }
                    LambdaMapping::BindSelf(binding) => visitor.visit_type(db, binding.ty),
                    LambdaMapping::ReplaceSelf(ty) => visitor.visit_type(db, *ty),
                    LambdaMapping::BindReceiver {
                        receiver,
                        self_binding,
                    }
                    | LambdaMapping::ApplyReceiver {
                        receiver,
                        self_binding,
                    } => {
                        visitor.visit_type(db, *receiver);
                        visitor.visit_type(db, self_binding.ty);
                    }
                    LambdaMapping::BindParameters(parameters) => {
                        for parameter in parameters {
                            visitor.visit_type(db, parameter.annotated_type());
                            if let Some(default) = parameter.eager_default_type() {
                                visitor.visit_type(db, default);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Keep parameter binding independent of the inputs used to infer the body.
    pub(in crate::types) fn with_bound_parameters(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        parameters: Parameters<'db>,
    ) -> Self {
        if *self.parameters(db) == parameters {
            return self;
        }
        let env = env.in_environment(ProgramEnvironment::from_scope(self.scope(db)));
        self.apply_mapping(
            db,
            LambdaMapping::BindParameters(parameters),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(&env),
        )
    }

    pub(in crate::types) fn bind_receiver(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        receiver: Type<'db>,
        typing_self_type: Type<'db>,
    ) -> Self {
        self.apply_mapping(
            db,
            LambdaMapping::BindReceiver {
                receiver,
                self_binding: SelfBinding::new(db, env, typing_self_type, None),
            },
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(env),
        )
    }

    pub(super) fn apply_receiver(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        receiver: Type<'db>,
        typing_self_type: Type<'db>,
    ) -> Self {
        self.apply_mapping(
            db,
            LambdaMapping::ApplyReceiver {
                receiver,
                self_binding: SelfBinding::new(db, env, typing_self_type, None),
            },
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(env),
        )
    }

    /// The original inference inputs, independent of every saved transformation.
    pub(in crate::types) fn source(self, db: &'db dyn Db) -> Self {
        self.mapping(db).as_ref().map_or(self, |plan| plan.source)
    }

    /// Build a view with separate caches for each mapping and shared active recursion guards.
    fn with_transformations(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        steps: &[LambdaTransformation<'db>],
    ) -> Self {
        let source = self.source(db);
        if steps.is_empty() {
            return source;
        }
        let env = env.in_environment(ProgramEnvironment::from_scope(source.scope(db)));
        let parameters = steps
            .iter()
            .fold(source.parameters(db).clone(), |parameters, step| {
                step.parameters(db, &env, parameters)
            });
        Self::new(
            db,
            parameters,
            source.scope(db),
            source.body(db),
            source.return_context(db),
            source.environment(db),
            Some(LambdaSignatureMapping {
                source,
                steps: steps.into(),
            }),
        )
    }

    /// Project captured variables in both variance positions without expanding the body.
    pub(in crate::types) fn captured_arguments(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
    ) -> impl Iterator<Item = (BoundTypeVarInstance<'db>, [Type<'db>; 2])> {
        let source = self.source(db);
        let steps = self
            .mapping(db)
            .as_ref()
            .map_or(&[][..], |plan| plan.steps.as_ref());
        let env = env.in_environment(ProgramEnvironment::from_scope(source.scope(db)));
        source
            .captured_typevars(db, &env)
            .variables(db)
            .map(move |parameter| {
                let mut arguments = [Type::TypeVar(parameter); 2];
                for step in steps {
                    let Some(transformation) = step.mapping.as_type_mapping() else {
                        continue;
                    };
                    let visitor = ApplyTypeMappingVisitor {
                        materialize_typevar_bounds_and_defaults: step
                            .materialize_typevar_bounds_and_defaults,
                        ..ApplyTypeMappingVisitor::new(&env)
                    };
                    let flipped = transformation.flip();
                    for (argument, mapping) in arguments.iter_mut().zip([&transformation, &flipped])
                    {
                        *argument =
                            argument.apply_type_mapping_impl(db, mapping, step.context, &visitor);
                    }
                }
                (parameter, arguments)
            })
    }

    /// Transform the source return's dependency summary in the consuming operation.
    pub(in crate::types) fn return_type_dependencies(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
    ) -> TypeVarSet<'db> {
        let dependencies = source_return_type_dependencies(db, LambdaSource::of(db, self));
        let Some(plan) = self.mapping(db) else {
            return dependencies;
        };
        let env = env.in_environment(ProgramEnvironment::from_scope(self.scope(db)));
        env.context()
            .state()
            .lambda_return_dependencies
            .get_or_init(|| Box::new(crate::types::CycleDetector::new(())))
            .try_visit_active(
                db,
                &env,
                Type::Callable(CallableType::from_lambda(db, self)),
                || {
                    let mut dependencies = dependencies;
                    for step in &plan.steps {
                        let Some(transformation) = step.mapping.as_type_mapping() else {
                            continue;
                        };
                        let visitor = ApplyTypeMappingVisitor {
                            materialize_typevar_bounds_and_defaults: step
                                .materialize_typevar_bounds_and_defaults,
                            ..ApplyTypeMappingVisitor::new(&env)
                        };
                        let flipped = transformation.flip();
                        dependencies = type_dependencies(
                            db,
                            &env,
                            dependencies.iter(db).flat_map(|variable| {
                                [&transformation, &flipped].map(|mapping| {
                                    Type::TypeVar(variable).apply_type_mapping_impl(
                                        db,
                                        mapping,
                                        step.context,
                                        &visitor,
                                    )
                                })
                            }),
                        );
                    }
                    dependencies
                },
            )
            .unwrap_or(TypeVarSet::None)
    }

    /// Transform captured inputs without inferring the lambda's body.
    fn captured_typevars(self, db: &'db dyn Db, env: &TypeOperations<'db>) -> GenericContext<'db> {
        let source = LambdaSource::of(db, self).captured_typevars(db);
        let Some(plan) = self.mapping(db) else {
            return source;
        };
        let env = env.in_environment(ProgramEnvironment::from_scope(self.scope(db)));
        env.context()
            .state()
            .lambda_captures
            .get_or_init(|| Box::new(crate::types::CycleDetector::new(())))
            .try_visit_active(
                db,
                &env,
                Type::Callable(CallableType::from_lambda(db, self)),
                || {
                    let variables = RefCell::new(source.variables(db).collect::<FxOrderSet<_>>());
                    for step in &plan.steps {
                        let Some(transformation) = step.mapping.as_type_mapping() else {
                            continue;
                        };
                        let current = variables.take();
                        let visitor = ApplyTypeMappingVisitor {
                            materialize_typevar_bounds_and_defaults: step
                                .materialize_typevar_bounds_and_defaults,
                            ..ApplyTypeMappingVisitor::new(&env)
                        };
                        for variable in current {
                            for mapping in [&transformation, &transformation.flip()] {
                                let mapped = Type::TypeVar(variable).apply_type_mapping_impl(
                                    db,
                                    mapping,
                                    step.context,
                                    &visitor,
                                );
                                collect_captured_typevars(db, &env, mapped, &variables);
                            }
                        }
                    }
                    GenericContext::from_typevar_instances(db, &env, variables.into_inner())
                },
            )
            .unwrap_or_else(|| GenericContext::from_typevar_instances(db, &env, []))
    }

    /// Rewrite stored inputs without evaluating the lambda's body. Recursive binding
    /// and unfolding must also rewrite captured arguments in deferred specializations.
    pub(super) fn map_inputs(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        let map = |ty: Type<'db>| ty.apply_type_mapping_impl(db, mapping, tcx, visitor);
        let deferred = self.mapping(db).as_ref().map(|plan| {
            let steps = plan
                .steps
                .iter()
                .cloned()
                .map(|mut step| {
                    match &mut step.mapping {
                        LambdaMapping::Specialize { specialization, .. } => {
                            *specialization =
                                specialization.apply_type_mapping_impl(db, mapping, &[], visitor);
                        }
                        LambdaMapping::BindSelf(binding) => binding.ty = map(binding.ty),
                        LambdaMapping::ReplaceSelf(ty) => *ty = map(*ty),
                        LambdaMapping::BindReceiver {
                            receiver,
                            self_binding,
                        }
                        | LambdaMapping::ApplyReceiver {
                            receiver,
                            self_binding,
                        } => {
                            *receiver = map(*receiver);
                            self_binding.ty = map(self_binding.ty);
                        }
                        LambdaMapping::BindParameters(parameters) => {
                            *parameters =
                                parameters.apply_type_mapping_impl(db, mapping, tcx, visitor);
                        }
                        _ => {}
                    }
                    step.context = step
                        .context
                        .with_annotation(step.context.annotation.map(map));
                    step
                })
                .collect();
            LambdaSignatureMapping {
                source: plan.source.map_inputs(db, mapping, tcx, visitor),
                steps,
            }
        });
        Self::new(
            db,
            self.parameters(db)
                .apply_type_mapping_impl(db, mapping, tcx, visitor),
            self.scope(db),
            self.body(db),
            self.return_context(db)
                .with_annotation(self.return_context(db).annotation.map(map)),
            self.environment(db),
            deferred,
        )
    }

    /// Normalize transformations before interning a new view of the original inputs.
    pub(super) fn apply_mapping(
        self,
        db: &'db dyn Db,
        mut mapping: LambdaMapping<'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        let source = self.source(db);
        let mut steps = self
            .mapping(db)
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.steps.to_vec());
        if matches!(
            mapping,
            LambdaMapping::Promote(..) | LambdaMapping::Materialize(_)
        ) && let Some(transformation) = mapping.as_type_mapping()
        {
            let flipped = transformation.flip();
            for previous in steps.iter().rev() {
                if previous.context != tcx
                    || previous.materialize_typevar_bounds_and_defaults
                        != visitor.materialize_typevar_bounds_and_defaults
                {
                    break;
                }
                match (&mapping, &previous.mapping) {
                    (LambdaMapping::Promote(..), LambdaMapping::Promote(..))
                    | (LambdaMapping::Materialize(_), LambdaMapping::Materialize(_)) => {}
                    (
                        _,
                        LambdaMapping::Specialize {
                            specialization,
                            materialization: None,
                            ..
                        },
                    ) if specialization.types(db).iter().all(|argument| {
                        [&transformation, &flipped]
                            .into_iter()
                            .all(|transformation| {
                                argument.apply_type_mapping_impl(db, transformation, tcx, visitor)
                                    == *argument
                            })
                    }) => {}
                    _ => break,
                }
                if previous.mapping == mapping
                    || matches!(
                        (&previous.mapping, &mapping),
                        (LambdaMapping::Materialize(_), LambdaMapping::Materialize(_))
                    )
                {
                    return self;
                }
            }
        }
        if let LambdaMapping::Specialize {
            specialization,
            materialization: None,
            ..
        } = &mapping
            && specialization
                .generic_context(db)
                .variables(db)
                .zip(specialization.types(db))
                .all(|(variable, ty)| Type::TypeVar(variable) == *ty)
        {
            return self;
        }
        if let Some(previous) = steps.last()
            && previous.mapping == mapping
            && previous.context == tcx
            && previous.materialize_typevar_bounds_and_defaults
                == visitor.materialize_typevar_bounds_and_defaults
            && matches!(
                mapping,
                LambdaMapping::BindSelf(_)
                    | LambdaMapping::BindLegacyTypevars(_)
                    | LambdaMapping::ReplaceSelf(_)
                    | LambdaMapping::ReplaceParameterDefaults
                    | LambdaMapping::BindParameters(_)
                    | LambdaMapping::ApplyReceiver { .. }
            )
        {
            return self;
        }
        if let Some(previous) = steps.last()
            && previous.context == tcx
            && previous.materialize_typevar_bounds_and_defaults
                == visitor.materialize_typevar_bounds_and_defaults
            && let LambdaMapping::Specialize {
                specialization: previous_specialization,
                specialize_self_domain: previous_self_domain,
                materialization: None,
            } = &previous.mapping
            && let LambdaMapping::Specialize {
                specialization,
                specialize_self_domain,
                materialization: None,
            } = &mut mapping
            && specialize_self_domain == previous_self_domain
        {
            let transformation =
                TypeMapping::ApplySpecialization(ApplySpecialization::Specialization {
                    specialization: *specialization,
                    specialize_self_domain: *specialize_self_domain,
                });
            let previous_source =
                source.with_transformations(db, visitor.env, &steps[..steps.len() - 1]);
            let (variables, types): (Vec<_>, Vec<_>) = previous_source
                .captured_typevars(db, visitor.env)
                .variables(db)
                .filter_map(|variable| {
                    previous_specialization
                        .get(db, variable)
                        .map(|ty| ty.apply_type_mapping_impl(db, &transformation, tcx, visitor))
                        .or_else(|| specialization.get(db, variable))
                        .map(|ty| (variable, ty))
                })
                .unzip();
            let context = GenericContext::from_typevar_instances(db, visitor.env, variables);
            *specialization =
                Specialization::new(db, context, types.into_boxed_slice(), None, None);
            steps.pop();
        }
        steps.push(LambdaTransformation {
            mapping,
            context: tcx,
            materialize_typevar_bounds_and_defaults: visitor
                .materialize_typevar_bounds_and_defaults,
        });
        if let Some(last) = steps.last()
            && matches!(last.mapping, LambdaMapping::Promote(..))
        {
            // Promotions commute, but cannot cross a substitution or a materialization here.
            // Give each consecutive group one order so equivalent views share an identity.
            let key = |step: &LambdaTransformation<'db>| match step.mapping {
                LambdaMapping::Promote(mode, kind) => Some((
                    match kind {
                        PromotionKind::Regular => 0,
                        PromotionKind::ClassLiteralsOnly => 1,
                        PromotionKind::SingletonsOnly => 2,
                    },
                    matches!(mode, PromotionMode::On),
                )),
                _ => None,
            };
            let start = steps
                .iter()
                .rposition(|step| {
                    key(step).is_none()
                        || step.context != tcx
                        || step.materialize_typevar_bounds_and_defaults
                            != visitor.materialize_typevar_bounds_and_defaults
                })
                .map_or(0, |index| index + 1);
            steps[start..].sort_by_key(key);
        }
        source.with_transformations(db, visitor.env, &steps)
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use ty_python_core::ProgramFile;

    use crate::db::tests::setup_db;
    use crate::place::global_symbol;
    use crate::types::callable::{CallableType, SignatureSource};
    use crate::types::{
        ApplyTypeMappingVisitor, Parameters, PromotionKind, PromotionMode, Type, TypeContext,
        TypeMapping,
    };

    #[test]
    fn unused_enclosing_typevars_preserve_lambda_identity() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
class C[T]:
    value = lambda: 0

left = C[int].value
right = C[str].value
"#,
        )?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/a.py")?,
            env.program(&db),
        );
        assert_eq!(
            global_symbol(&db, file, "left").place.expect_type(),
            global_symbol(&db, file, "right").place.expect_type(),
        );
        Ok(())
    }

    #[test]
    fn promotions_preserve_signatures_and_reuse_equivalent_views() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
class C[T = float]:
    def get(self) -> T: ...

function = lambda value: (value, 1, C)
"#,
        )?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/a.py")?,
            env.program(&db),
        );
        let function = global_symbol(&db, file, "function")
            .place
            .expect_type()
            .as_callable()
            .ok_or_else(|| anyhow::anyhow!("expected a callable"))?;
        let explicit = function
            .inspect_signatures_root(&db, &env, |_, signatures| {
                function.with_signatures(&db, signatures.signatures.clone())
            })
            .ok_or_else(|| anyhow::anyhow!("unexpected recursive signature"))?;
        let promote = |callable, kind| {
            CallableType::apply_type_mapping_impl(
                callable,
                &db,
                &TypeMapping::Promote(PromotionMode::On, kind),
                TypeContext::default(),
                &ApplyTypeMappingVisitor::new(&env),
            )
        };
        let canonical = promote(
            promote(function, PromotionKind::Regular),
            PromotionKind::ClassLiteralsOnly,
        );
        for kinds in [
            [PromotionKind::Regular, PromotionKind::ClassLiteralsOnly],
            [PromotionKind::ClassLiteralsOnly, PromotionKind::Regular],
        ] {
            let mut deferred = function;
            let mut eager = explicit;
            for kind in kinds.into_iter().chain([PromotionKind::Regular]) {
                deferred = promote(deferred, kind);
                eager = promote(eager, kind);
                deferred
                    .inspect_signatures_root(&db, &env, |_, signatures| {
                        assert_eq!(Some(signatures.signatures), eager.explicit_signatures(&db));
                    })
                    .ok_or_else(|| anyhow::anyhow!("unexpected recursive signature"))?;
            }
            assert_eq!(deferred, canonical);
        }
        Ok(())
    }

    #[test]
    fn receiver_binding_preserves_the_lambda_source() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented("/src/a.py", "function = lambda self, value: (self, value)")?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/a.py")?,
            env.program(&db),
        );
        let Type::Callable(function) = global_symbol(&db, file, "function").place.expect_type()
        else {
            panic!("expected a callable");
        };
        let bound = function.bind_self(&db, &env, Type::object(), Type::object());
        assert_matches!(bound.signature_source(&db), SignatureSource::Lambda(_));
        assert_eq!(bound.single_parameters(&db).map(Parameters::len), Some(1));
        Ok(())
    }
}

/// Source dependency summaries are independent of a caller's saved transformations.
#[salsa::tracked(
    returns(copy),
    cycle_initial=|_, _, _| TypeVarSet::None,
    cycle_fn=|db, _, previous: &TypeVarSet<'db>, current, _| previous.merge(db, current),
    heap_size=ruff_memory_usage::heap_size,
)]
fn source_return_type_dependencies<'db>(
    db: &'db dyn Db,
    source: LambdaSource<'db>,
) -> TypeVarSet<'db> {
    let lambda = source.input(db);
    let env = TypeOperations::new_root(db, ProgramEnvironment::from_scope(lambda.scope(db)));
    type_dependencies(
        db,
        &env,
        [infer_lambda_signature(db, source).overload_return_type_or_unknown(db, &env)],
    )
}

#[salsa::tracked(
    returns(copy),
    cycle_initial=|db, _, source: LambdaSource<'db>| GenericContext::from_typevar_instances(
        db, &ProgramEnvironment::from_scope(source.input(db).scope(db)), []
    ),
    heap_size=ruff_memory_usage::heap_size,
)]
fn source_captured_typevars<'db>(db: &'db dyn Db, input: LambdaSource<'db>) -> GenericContext<'db> {
    let source = input.input(db);
    let scope = source.scope(db);
    let env = TypeOperations::new_root(db, ProgramEnvironment::from_scope(scope));
    let index = semantic_index(db, scope.program_file(db));
    let mut scopes = vec![scope.file_scope_id(db)];
    let mut position = 0;
    while let Some(current) = scopes.get(position).copied() {
        scopes.extend(index.child_scopes(current).map(|(id, _)| id));
        position += 1;
    }
    let local_scopes: FxHashSet<_> = scopes.iter().copied().collect();
    let mut captured_scopes = FxHashSet::default();
    let mut free_names = FxHashSet::default();
    for current in scopes {
        for symbol in index.place_table(current).symbols() {
            if !symbol.is_used() || symbol.is_local() {
                continue;
            }
            let owner = index
                .visible_ancestor_scopes(current)
                .skip(1)
                .find_map(|(id, _)| {
                    let places = index.place_table(id);
                    places
                        .symbol_id(symbol.name())
                        .filter(|symbol| places.symbol(*symbol).is_local())
                        .map(|_| id)
                });
            if owner.is_some_and(|owner| local_scopes.contains(&owner)) {
                continue;
            }
            free_names.insert(symbol.name());
            if let Some(owner) = owner.filter(|owner| !owner.is_global()) {
                captured_scopes.extend(index.ancestor_scopes(owner).map(|(id, _)| id));
            }
        }
    }
    // A generic environment is an input only when the body reads a value from it, or
    // names one of its type parameters. Closed lambdas must not acquire substitutions
    // for unrelated enclosing parameters, such as T in `class C[T]: f = lambda: 0`.
    let mut lexical_variables = FxOrderSet::default();
    if !free_names.is_empty() {
        for (ancestor_id, ancestor) in index.ancestor_scopes(scope.file_scope_id(db)) {
            if let Some(context) = GenericContext::lexical_of_node(db, &env, ancestor.node(), index)
            {
                lexical_variables.extend(context.variables(db).filter(|variable| {
                    captured_scopes.contains(&ancestor_id) || free_names.contains(variable.name(db))
                }));
            }
        }
    }
    let variables = RefCell::new(lexical_variables);
    for parameter in source.parameters(db) {
        collect_captured_typevars(db, &env, parameter.annotated_type(), &variables);
        if let Some(default) = parameter.eager_default_type() {
            collect_captured_typevars(db, &env, default, &variables);
        }
    }
    if let Some(annotation) = source.return_context(db).annotation {
        collect_captured_typevars(db, &env, annotation, &variables);
    }

    GenericContext::from_typevar_instances(db, &env, variables.into_inner())
}

impl<'db> LambdaSource<'db> {
    pub(in crate::types) fn captured_typevars(self, db: &'db dyn Db) -> GenericContext<'db> {
        source_captured_typevars(db, self)
    }
}

fn collect_captured_typevars<'db>(
    db: &'db dyn Db,
    env: &TypeOperations<'db>,
    ty: Type<'db>,
    variables: &RefCell<FxOrderSet<BoundTypeVarInstance<'db>>>,
) {
    any_over_type_including_alias_arguments(db, env, ty, |ty| {
        match ty {
            Type::TypeVar(variable) => {
                variables.borrow_mut().insert(variable);
            }
            Type::Callable(callable)
                if let SignatureSource::Lambda(lambda) = callable.signature_source(db) =>
            {
                let captured = lambda.captured_typevars(db, env);
                variables.borrow_mut().extend(captured.variables(db));
            }
            _ => {}
        }
        false
    });
}
