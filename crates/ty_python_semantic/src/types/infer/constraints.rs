//! Constraints retained while a cyclic inference query is still being evaluated.
//!
//! Query results carry ordinary type approximations alongside immutable equations. Attribute reads
//! use anonymous protocols; other operations reuse ordinary inference once their inputs are known.
//! Producers use existing types with variables for unresolved outputs. Ordinary query bodies
//! substitute producers and solve the resulting constraints.

use std::cell::{Cell, RefCell};
use std::hash::Hash;

use ruff_python_ast::ExprContext;
use rustc_hash::{FxHashMap, FxHashSet};
use salsa::plumbing::AsId;
use ty_python_core::definition::Definition;
use ty_python_core::frozen::FrozenMap;
use ty_python_core::narrowing_constraints::ScopedNarrowingConstraint;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::scope::ScopeId;
use ty_python_core::unpack::Unpack;
use ty_python_core::{EvaluationMode, ExpressionNodeKey, NarrowingEvaluator, Program, use_def_map};

use super::InferenceRegion;
use crate::place::{Place, PlaceAndQualifiers, SymbolLookupKey};
use crate::reachability::predicate_scope;
use crate::types::class::AugmentedAttribute;
use crate::types::class::implicit_attributes::ImplicitAttributeName;
use crate::types::constraints::{
    ConstraintSet, ConstraintSetBuilder, PathBoundSolution, PathBounds,
    projection::{ProjectionError, SolutionBudget, SolutionProjection},
};
use crate::types::narrow::NarrowingEvaluatorExtension;
use crate::types::tuple::{TupleBuilder, TupleLength, TupleType};
use crate::types::typevar::{BindingContext, BoundTypeVarInstance, TypeVarSet};
use crate::types::{
    ApplySpecialization, GenericContext, KnownClass, MemberLookupKey, Specialization, Type,
    TypeContext, TypeMapping, UnionType, any_over_type, any_over_type_including_alias_arguments,
};
use crate::{Db, FxIndexMap, ProgramEnvironment};

#[salsa::tracked]
impl<'db> Type<'db> {
    /// Collect inference variables, including alias arguments, without evaluating lazy attributes.
    #[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
    fn inference_variable_context(
        self,
        db: &'db dyn Db,
        program: Program<'db>,
    ) -> Option<GenericContext<'db>> {
        let env = ProgramEnvironment::from_program(program);
        let variables = RefCell::new(Vec::new());
        any_over_type_including_alias_arguments(db, &env, self, |ty| {
            if let Type::TypeVar(variable) = ty
                && matches!(variable.binding_context(db), BindingContext::Inference(_))
            {
                variables.borrow_mut().push(variable);
            }
            false
        });
        let variables = variables.into_inner();
        if variables.is_empty() {
            None
        } else {
            Some(GenericContext::from_typevar_instances(db, &env, variables))
        }
    }
}

/// The complete query inputs that own an inferred output.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum InferenceOwner<'db> {
    Region(InferenceRegion<'db>),
    Member(MemberLookupKey<'db>, Option<Type<'db>>),
    Symbol(SymbolLookupKey<'db>),
    Attribute(ImplicitAttributeName<'db>),
    AugmentedAttribute(AugmentedAttribute<'db>),
    Unpack(Unpack<'db>),
    Narrowing(InferenceVariable<'db>, InferenceNarrowing<'db>),
    Promotion(InferenceVariable<'db>),
    Lookup(InferenceVariable<'db>),
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, get_size2::GetSize, salsa::SalsaValue,
)]
pub(crate) enum InferenceSlot<'db> {
    Root,
    /// A candidate in an ordered public lookup.
    Lookup(usize),
    Expression(ExpressionNodeKey),
    /// Auxiliary outputs of an expression, such as a mapping spread's key and value types.
    Component(ExpressionNodeKey, usize),
    Binding(Definition<'db>),
    /// One result per collection parameter, identified by its full interned ID.
    TypeArgument(ExpressionNodeKey, u64),
    /// Reaching assignments can share a predicate while supplying different types.
    NarrowedBinding(ExpressionNodeKey, Definition<'db>),
}

/// A variable belongs to one query output, independently of its current approximation.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct InferenceVariableInner<'db> {
    #[returns(copy)]
    pub(crate) program: Program<'db>,
    #[returns(copy)]
    owner: InferenceOwner<'db>,
    #[returns(copy)]
    slot: InferenceSlot<'db>,
    #[returns(copy)]
    specialization: Option<Specialization<'db>>,
}

impl get_size2::GetSize for InferenceVariableInner<'_> {}

/// Identity of an output whose type is still being inferred across a query cycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct InferenceVariable<'db>(InferenceVariableInner<'db>);

impl<'db> InferenceVariable<'db> {
    pub(crate) fn new(
        db: &'db dyn Db,
        program: Program<'db>,
        owner: InferenceOwner<'db>,
        slot: InferenceSlot<'db>,
    ) -> Self {
        Self(InferenceVariableInner::new(db, program, owner, slot, None))
    }

    pub(crate) fn program(self, db: &'db dyn Db) -> Program<'db> {
        self.0.program(db)
    }

    /// Identify a lookup candidate within a fixed query or expression context.
    pub(crate) fn lookup_part(self, db: &'db dyn Db, index: usize) -> Self {
        Self::new(
            db,
            self.program(db),
            InferenceOwner::Lookup(self),
            InferenceSlot::Lookup(index),
        )
    }

    fn specialized(self, db: &'db dyn Db, specialization: Specialization<'db>) -> Self {
        let owner = self.0.owner(db);
        // Derived lookup identities follow their inputs so specializing a retained graph and
        // evaluating the specialized lookup produce the same variables.
        let specialized_owner = match owner {
            InferenceOwner::Lookup(input) => Some(InferenceOwner::Lookup(
                input.specialized(db, specialization),
            )),
            InferenceOwner::Promotion(input) => Some(InferenceOwner::Promotion(
                input.specialized(db, specialization),
            )),
            _ => None,
        };
        if let Some(owner) = specialized_owner {
            return Self::new(db, self.program(db), owner, self.0.slot(db));
        }
        if let InferenceOwner::Member(key, receiver) = owner {
            let key = MemberLookupKey::new(
                db,
                key.program(db),
                key.ty(db)
                    .apply_optional_owner_specialization_to_member(db, Some(specialization)),
                key.name(db).clone(),
                key.policy(db),
            );
            return Self::new(
                db,
                self.program(db),
                InferenceOwner::Member(
                    key,
                    receiver.map(|ty| {
                        ty.apply_optional_owner_specialization_to_member(db, Some(specialization))
                    }),
                ),
                self.0.slot(db),
            );
        }
        let previous = self.0.specialization(db);
        let context = previous.map_or(specialization.generic_context(db), |previous| {
            previous
                .generic_context(db)
                .merge(db, specialization.generic_context(db))
        });
        let types: Vec<_> = context
            .variables(db)
            .map(|variable| {
                previous
                    .and_then(|previous| previous.get(db, variable))
                    .unwrap_or(Type::TypeVar(variable))
                    .apply_optional_owner_specialization_to_member(db, Some(specialization))
            })
            .collect();
        if context
            .variables(db)
            .zip(&types)
            .all(|(variable, ty)| *ty == Type::TypeVar(variable))
        {
            return self;
        }
        let specialization = Specialization::new(db, context, types.into_boxed_slice(), None, None);
        Self(InferenceVariableInner::new(
            db,
            self.program(db),
            owner,
            self.0.slot(db),
            Some(specialization),
        ))
    }

    fn typevar(self, db: &'db dyn Db) -> BoundTypeVarInstance<'db> {
        BoundTypeVarInstance::inferred(db, self)
    }

    fn ty(self, db: &'db dyn Db) -> Type<'db> {
        Type::TypeVar(self.typevar(db))
    }
}

impl<'db> MemberLookupKey<'db> {
    /// Identify the ordinary member lookup independently of its current inferred value.
    pub(crate) fn inference_variable(self, db: &'db dyn Db) -> InferenceVariable<'db> {
        InferenceVariable::new(
            db,
            self.program(db),
            InferenceOwner::Member(self, None),
            InferenceSlot::Root,
        )
    }
}

impl<'db> SymbolLookupKey<'db> {
    /// Identify a public symbol lookup independently of its current inferred value.
    pub(crate) fn inference_variable(self, db: &'db dyn Db) -> InferenceVariable<'db> {
        InferenceVariable::new(
            db,
            self.scope(db).program(db),
            InferenceOwner::Symbol(self),
            InferenceSlot::Root,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum Equation<'db> {
    /// A recursive query reference whose producer has not returned yet.
    Pending,
    /// The lower bound supplied by the producer of this output.
    Value(Type<'db>),
    /// A read operation constraining its input and result, without assuming it is valid.
    Requirement {
        source: Type<'db>,
        target: Type<'db>,
    },
    Operation(InferenceOperation<'db>),
}

impl<'db> Equation<'db> {
    /// Keep a producer's stable identity while normalizing its changing approximation.
    fn cycle_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        match (self, previous) {
            (Self::Pending, previous) => previous,
            (current, Self::Pending) => current,
            (Self::Value(current), Self::Value(previous)) => {
                Self::Value(current.cycle_normalized(db, env, previous, cycle))
            }
            (
                Self::Requirement {
                    source: current_source,
                    target: current_target,
                },
                Self::Requirement {
                    source: previous_source,
                    target: previous_target,
                },
            ) => Self::Requirement {
                source: current_source.cycle_normalized(db, env, previous_source, cycle),
                target: current_target.cycle_normalized(db, env, previous_target, cycle),
            },
            (Self::Operation(current), Self::Operation(previous)) => {
                Self::Operation(current.cycle_normalized(db, env, previous, cycle))
            }
            (current, _) => current,
        }
    }

    fn visit_types(self, mut visit: impl FnMut(Type<'db>)) {
        match self {
            Self::Pending => {}
            Self::Value(ty) => visit(ty),
            Self::Requirement { source, target } => {
                visit(source);
                visit(target);
            }
            Self::Operation(operation) => {
                operation.map(|ty| {
                    visit(ty);
                    ty
                });
            }
        }
    }
}

/// An operation evaluated from its inferred inputs in the ordinary query body.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum InferenceOperation<'db> {
    Subscript {
        value: Type<'db>,
        key: Type<'db>,
    },
    Iterate {
        value: Type<'db>,
        mode: EvaluationMode,
    },
    Promote {
        value: Type<'db>,
        promotion: InferencePromotion,
    },
    Narrow {
        value: Type<'db>,
        narrowing: InferenceNarrowing<'db>,
    },
    MappingKey(Type<'db>),
    MappingValue(Type<'db>),
    Unpack {
        value: Type<'db>,
        length: TupleLength,
        index: usize,
    },
    Concatenate {
        left: Type<'db>,
        right: Type<'db>,
        right_length: Option<usize>,
    },
}

impl<'db> InferenceOperation<'db> {
    /// Preserve an operation's stable shape while normalizing its type inputs.
    fn cycle_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        let (current, previous) = match (self, previous) {
            (
                Self::Promote {
                    value: current_value,
                    promotion: current_promotion,
                },
                Self::Promote {
                    value: previous_value,
                    promotion: previous_promotion,
                },
            ) => {
                let promotion = current_promotion.cycle_normalized(previous_promotion);
                (
                    Self::Promote {
                        value: current_value,
                        promotion,
                    },
                    Self::Promote {
                        value: previous_value,
                        promotion,
                    },
                )
            }
            operations => operations,
        };
        if current == previous {
            return current;
        }
        let shape = current.map(|_| Type::Never);
        let previous_shape = previous.map(|_| Type::Never);
        if shape != previous_shape {
            // Only corresponding type fields can be normalized. A different non-type shape can
            // give those fields different meanings, so do not pair unrelated inputs.
            return current;
        }
        let mut previous_types = Vec::new();
        previous.map(|ty| {
            previous_types.push(ty);
            ty
        });
        let mut index = 0;
        current.map(|ty| {
            let previous = previous_types[index];
            index += 1;
            ty.cycle_normalized(db, env, previous, cycle)
        })
    }

    /// Charge an estimate of input traversal and pairwise type comparisons before evaluation.
    fn charge_inputs(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        remaining: &mut usize,
    ) -> Result<(), ProjectionError> {
        let mut terms = 0usize;
        let mut comparisons = 0usize;
        self.map(|ty| {
            let count = Cell::new(0usize);
            any_over_type(db, env, ty, false, |_| {
                count.set(count.get().saturating_add(1));
                count.get() > *remaining
            });
            comparisons = comparisons.saturating_add(terms.saturating_mul(count.get()));
            terms = terms.saturating_add(count.get());
            ty
        });
        *remaining = remaining
            .checked_sub(comparisons.saturating_add(terms))
            .ok_or(ProjectionError::TraversalBudgetExceeded)?;
        Ok(())
    }

    fn map(self, mut f: impl FnMut(Type<'db>) -> Type<'db>) -> Self {
        match self {
            Self::Subscript { value, key } => Self::Subscript {
                value: f(value),
                key: f(key),
            },
            Self::Iterate { value, mode } => Self::Iterate {
                value: f(value),
                mode,
            },
            Self::MappingKey(value) => Self::MappingKey(f(value)),
            Self::Promote { value, promotion } => Self::Promote {
                value: f(value),
                promotion,
            },
            Self::Narrow { value, narrowing } => Self::Narrow {
                value: f(value),
                narrowing,
            },
            Self::MappingValue(value) => Self::MappingValue(f(value)),
            Self::Unpack {
                value,
                length,
                index,
            } => Self::Unpack {
                value: f(value),
                length,
                index,
            },
            Self::Concatenate {
                left,
                right,
                right_length,
            } => Self::Concatenate {
                left: f(left),
                right: f(right),
                right_length,
            },
        }
    }

    fn evaluate(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: InferenceVariable<'db>,
    ) -> Option<Type<'db>> {
        match self {
            Self::Subscript { value, key } => value.subscript(db, env, key, ExprContext::Load).ok(),
            Self::Iterate { value, mode } => Some(
                value
                    .try_iterate_with_mode(db, env, mode)
                    .ok()?
                    .homogeneous_element_type(db, env),
            ),
            Self::MappingKey(value) => Some(value.unpack_keys_and_items(db, env)?.0),
            Self::Promote { value, promotion } => {
                // Wait for a value to promote; forwarding an unresolved reference creates mutable
                // aliases that can repeatedly unfold recursive types.
                if let Type::TypeVar(reference) = value
                    && matches!(reference.binding_context(db), BindingContext::Inference(_))
                {
                    return None;
                }
                Some(promotion.apply(db, env, value))
            }
            Self::Narrow { value, narrowing } => {
                // Generic filtering requires the subject's arguments, not an unresolved variable.
                if value
                    .inference_variable_context(db, env.program(db))
                    .is_some()
                {
                    return None;
                }
                let narrowed = use_def_map(db, narrowing.scope)
                    .narrowing_evaluator(narrowing.constraint)
                    .narrow(db, env, value, narrowing.place);
                Some(narrowed.apply_optional_owner_specialization_to_member(
                    db,
                    variable.0.specialization(db),
                ))
            }
            Self::MappingValue(value) => Some(value.unpack_keys_and_items(db, env)?.1),
            Self::Unpack {
                value,
                length,
                index,
            } => value.unpacked_target(db, env, length, index),
            Self::Concatenate {
                left,
                right,
                right_length,
            } => {
                let left = left.try_iterate(db, env).ok()?;
                let mut right = right.try_iterate(db, env).ok()?.into_owned();
                if let Some(length) = right_length {
                    right = right.resize(db, env, TupleLength::Fixed(length)).ok()?;
                }
                let tuple = TupleBuilder::with_capacity(0)
                    .concat(db, env, &left)
                    .concat(db, env, &right)
                    .build();
                Some(Type::tuple(TupleType::new(db, env, &tuple)))
            }
        }
    }
}

/// Dependency-first groups of mutually recursive inference equations.
struct InferenceGraph {
    components: Vec<Vec<usize>>,
}

impl InferenceGraph {
    fn from_equations<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        equations: &[(InferenceVariable<'db>, Equation<'db>)],
    ) -> Self {
        let indices: FxIndexMap<_, _> = equations
            .iter()
            .enumerate()
            .map(|(index, (variable, _))| (*variable, index))
            .collect();
        let mut graph = vec![Vec::new(); equations.len()];
        for (index, (_, equation)) in equations.iter().enumerate() {
            let mut dependencies = Vec::new();
            equation.visit_types(|ty| {
                let Some(context) = ty.inference_variable_context(db, env.program(db)) else {
                    return;
                };
                for variable in context.variables(db) {
                    if let BindingContext::Inference(variable) = variable.binding_context(db)
                        && let Some(dependency) = indices.get(&variable)
                        && !dependencies.contains(dependency)
                    {
                        dependencies.push(*dependency);
                    }
                }
            });
            graph[index] = dependencies;
        }
        let components = Self::dependency_first_components(&graph)
            .into_iter()
            .map(|mut component| {
                component.sort_unstable();
                component
            })
            .collect();
        Self { components }
    }

    fn dependency_first_components(graph: &[Vec<usize>]) -> Vec<Vec<usize>> {
        struct State<'a> {
            graph: &'a [Vec<usize>],
            next_index: usize,
            indices: Vec<Option<usize>>,
            lowlinks: Vec<usize>,
            stack: Vec<usize>,
            on_stack: Vec<bool>,
            components: Vec<Vec<usize>>,
        }

        impl State<'_> {
            fn visit(&mut self, node: usize) {
                let index = self.next_index;
                self.next_index += 1;
                self.indices[node] = Some(index);
                self.lowlinks[node] = index;
                self.stack.push(node);
                self.on_stack[node] = true;

                for dependency in &self.graph[node] {
                    if self.indices[*dependency].is_none() {
                        self.visit(*dependency);
                        self.lowlinks[node] = self.lowlinks[node].min(self.lowlinks[*dependency]);
                    } else if self.on_stack[*dependency]
                        && let Some(dependency_index) = self.indices[*dependency]
                    {
                        self.lowlinks[node] = self.lowlinks[node].min(dependency_index);
                    }
                }

                if self.lowlinks[node] == index {
                    let mut component = Vec::new();
                    while let Some(dependency) = self.stack.pop() {
                        self.on_stack[dependency] = false;
                        component.push(dependency);
                        if dependency == node {
                            break;
                        }
                    }
                    self.components.push(component);
                }
            }
        }

        let mut state = State {
            graph,
            next_index: 0,
            indices: vec![None; graph.len()],
            lowlinks: vec![0; graph.len()],
            stack: Vec::new(),
            on_stack: vec![false; graph.len()],
            components: Vec::new(),
        };
        for node in 0..graph.len() {
            if state.indices[node].is_none() {
                state.visit(node);
            }
        }
        state.components
    }
}

/// Promotion selected by the ordinary inference path for a value or collection element.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum InferencePromotion {
    Regular,
    Attribute,
    Collection {
        tuple_size: bool,
        unconstrained: bool,
    },
}

impl InferencePromotion {
    /// Keep the stricter collection widening rule seen in either cycle iteration.
    fn cycle_normalized(self, previous: Self) -> Self {
        match (self, previous) {
            (
                Self::Collection {
                    tuple_size,
                    unconstrained,
                },
                Self::Collection {
                    tuple_size: previous_tuple_size,
                    unconstrained: previous_unconstrained,
                },
            ) => Self::Collection {
                tuple_size: tuple_size && previous_tuple_size,
                unconstrained: unconstrained && previous_unconstrained,
            },
            (current, _) => current,
        }
    }

    pub(crate) fn apply<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        value: Type<'db>,
    ) -> Type<'db> {
        match self {
            Self::Regular => value.promote(db, env),
            Self::Attribute => value.promote(db, env).promote_singletons(db, env),
            Self::Collection {
                tuple_size,
                unconstrained,
            } => value.promote_collection_element_type(db, env, tuple_size, unconstrained),
        }
    }
}

/// The lexical predicate graph and target of a deferred narrowing operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct InferenceNarrowing<'db> {
    scope: ScopeId<'db>,
    place: ScopedPlaceId,
    constraint: ScopedNarrowingConstraint,
}

/// A complete set of equations shared by symbolic outputs.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct InferenceEquations<'db> {
    #[returns(ref)]
    equations: Box<[(InferenceVariable<'db>, Equation<'db>)]>,
}

impl get_size2::GetSize for InferenceEquations<'_> {}

#[salsa::tracked]
impl<'db> InferenceEquations<'db> {
    /// Share producer bindings across outputs that retain the same equations.
    #[salsa::tracked(
        returns(as_ref),
        cycle_initial=|_, _, _, _| None,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn solve(
        self,
        db: &'db dyn Db,
        program: Program<'db>,
    ) -> Option<Box<[(BoundTypeVarInstance<'db>, Type<'db>)]>> {
        let env = &ProgramEnvironment::from_program(program);
        let equations = self.equations(db);
        if equations
            .iter()
            .any(|(_, equation)| matches!(equation, Equation::Pending))
        {
            return None;
        }

        let graph = InferenceGraph::from_equations(db, env, equations);
        let mut producers = InferenceSolutions {
            db,
            env,
            bindings: FxIndexMap::default(),
            active: Vec::new(),
            resolved: FxIndexMap::default(),
            evaluated: FxHashMap::default(),
            growing: FxHashSet::default(),
        };
        let mut budget = SolutionBudget::default();
        for component in graph.components {
            let equations: Vec<_> = component.iter().map(|index| equations[*index]).collect();
            for (variable, equation) in &equations {
                if let Equation::Value(ty) = equation {
                    producers.bindings.insert(variable.typevar(db), *ty);
                }
            }
            producers.resolved.clear();

            let operations: Vec<_> = equations
                .iter()
                .filter_map(|(variable, equation)| match equation {
                    Equation::Operation(operation) => Some((*variable, *operation)),
                    _ => None,
                })
                .collect();
            let mut seen = FxHashSet::default();
            loop {
                let (mut changed, operations_complete) = producers
                    .evaluate_operations(&operations, &mut budget)
                    .ok()?;
                let previous_requirements: Vec<_> = equations
                    .iter()
                    .filter(|(_, equation)| matches!(equation, Equation::Requirement { .. }))
                    .map(|(variable, _)| {
                        let variable = variable.typevar(db);
                        (variable, producers.bindings.swap_remove(&variable))
                    })
                    .collect();
                producers.resolved.clear();
                let resolved = producers.solve_requirements(&equations)?;
                for ((variable, previous), ty) in previous_requirements.into_iter().zip(resolved) {
                    if previous.is_none_or(|previous| !previous.is_equivalent_to(db, env, ty)) {
                        changed = true;
                        producers.bindings.insert(variable, ty);
                    } else if let Some(previous) = previous {
                        producers.bindings.insert(variable, previous);
                    }
                }
                producers.resolved.clear();
                if !changed && operations_complete {
                    break;
                }
                // Overload selection can oscillate between approximations. Repeating any state,
                // rather than only the immediately preceding state, cannot make further progress.
                let state: Vec<_> = equations
                    .iter()
                    .filter_map(|(variable, _)| {
                        producers
                            .bindings
                            .get(&variable.typevar(db))
                            .map(|ty| (variable.typevar(db), *ty))
                    })
                    .collect();
                if !seen.insert(state) {
                    return None;
                }
            }
        }
        let mut bindings: Vec<_> = producers.bindings.into_iter().collect();
        bindings.sort_unstable_by_key(|(variable, _)| variable.as_id());
        Some(bindings.into_boxed_slice())
    }
}

/// A symbolic type and the equations needed to interpret its variables.
/// Only its resolved approximation is passed to ordinary type checking.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct SymbolicType<'db> {
    #[returns(copy)]
    pub(crate) ty: Type<'db>,
    #[returns(copy)]
    graph: InferenceEquations<'db>,
}

impl get_size2::GetSize for SymbolicType<'_> {}

impl<'db> SymbolicType<'db> {
    /// The equations retained alongside this output.
    fn equations(self, db: &'db dyn Db) -> &'db [(InferenceVariable<'db>, Equation<'db>)] {
        self.graph(db).equations(db)
    }

    /// Apply an operation to this value while retaining its dependencies.
    pub(crate) fn apply(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        owner: InferenceOwner<'db>,
        expression: ExpressionNodeKey,
        operation: impl FnOnce(Type<'db>) -> InferenceOperation<'db>,
    ) -> Self {
        let mut constraints = InferenceConstraints::default();
        let value = constraints.import(db, self);
        let result = constraints.apply(
            db,
            env,
            owner,
            InferenceSlot::Expression(expression),
            operation(value),
        );
        constraints.finish(db, result)
    }

    pub(crate) fn initial(
        db: &'db dyn Db,
        program: Program<'db>,
        owner: InferenceOwner<'db>,
        slot: InferenceSlot<'db>,
    ) -> Self {
        let variable = InferenceVariable::new(db, program, owner, slot);
        Self::new(
            db,
            variable.ty(db),
            InferenceEquations::new(db, vec![(variable, Equation::Pending)].into_boxed_slice()),
        )
    }

    fn specialized(self, db: &'db dyn Db, specialization: Specialization<'db>) -> Self {
        let env = ProgramEnvironment::from_program(specialization.generic_context(db).program(db));
        let context = GenericContext::from_typevar_instances(
            db,
            &env,
            self.equations(db)
                .iter()
                .map(|(variable, _)| variable.typevar(db)),
        );
        let types: Vec<_> = context
            .variables(db)
            .map(|variable| match variable.binding_context(db) {
                BindingContext::Inference(variable) => {
                    variable.specialized(db, specialization).ty(db)
                }
                _ => Type::TypeVar(variable),
            })
            .collect();
        let rename = Specialization::new(db, context, types.into_boxed_slice(), None, None);
        let apply = |ty: Type<'db>| {
            ty.apply_optional_owner_specialization_to_member(db, Some(specialization))
                .apply_specialization(db, rename)
        };
        let mut equations: Vec<_> = self
            .equations(db)
            .iter()
            .map(|(variable, equation)| {
                let equation = match equation {
                    Equation::Pending => Equation::Pending,
                    Equation::Value(ty) => Equation::Value(apply(*ty)),
                    Equation::Requirement { source, target } => Equation::Requirement {
                        source: apply(*source),
                        target: apply(*target),
                    },
                    Equation::Operation(operation) => Equation::Operation(operation.map(apply)),
                };
                (variable.specialized(db, specialization), equation)
            })
            .collect();
        // Different cycle entry points can produce the same specialized graph in different
        // insertion orders. Sort the rewritten identities so the interned slices compare equal.
        equations.sort_unstable_by_key(|(variable, _)| variable.0.as_id());
        Self::new(
            db,
            apply(self.ty(db)),
            InferenceEquations::new(db, equations.into_boxed_slice()),
        )
    }

    pub(crate) fn from_union(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        values: impl IntoIterator<Item = (Type<'db>, Option<Self>)>,
    ) -> Option<Self> {
        let mut constraints = InferenceConstraints::default();
        let mut types = Vec::new();
        let mut symbolic = false;
        for (ty, value) in values {
            types.push(if let Some(value) = value {
                symbolic = true;
                constraints.import(db, value)
            } else {
                ty
            });
        }
        symbolic.then(|| constraints.finish(db, UnionType::from_elements(db, env, types)))
    }

    pub(crate) fn bind(
        self,
        db: &'db dyn Db,
        program: Program<'db>,
        owner: InferenceOwner<'db>,
        slot: InferenceSlot<'db>,
    ) -> Self {
        let mut constraints = InferenceConstraints::default();
        let value = constraints.import(db, self);
        let output = constraints.define(db, program, owner, slot, value);
        constraints.finish(db, output)
    }

    pub(crate) fn bind_place(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        owner: InferenceOwner<'db>,
        place: Place<'db>,
    ) -> Place<'db> {
        let Place::Defined(mut defined) = place else {
            return place;
        };
        if let Some(symbolic) = defined.symbolic {
            let symbolic = symbolic.bind(db, env.program(db), owner, InferenceSlot::Root);
            if let Some(resolved) = symbolic.resolve(db, env) {
                defined.ty = resolved;
            }
            defined.symbolic = Some(symbolic);
        }
        Place::Defined(defined)
    }

    /// Widen a captured binding after its cyclic inputs have been solved.
    pub(crate) fn promote_binding(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding: Definition<'db>,
    ) -> Self {
        let input = InferenceVariable::new(
            db,
            env.program(db),
            InferenceOwner::Region(InferenceRegion::Definition(binding)),
            InferenceSlot::Binding(binding),
        );
        self.promote(db, env, input, InferencePromotion::Regular)
    }

    /// Apply promotion after the complete input type is available.
    pub(crate) fn promote(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        input: InferenceVariable<'db>,
        promotion: InferencePromotion,
    ) -> Self {
        let mut constraints = InferenceConstraints::default();
        let value = constraints.import(db, self);
        let value = constraints.promote(db, env, input, value, promotion);
        constraints.finish(db, value)
    }

    /// Preserve predicate evaluation until this binding's cyclic inputs have been solved.
    pub(crate) fn narrow_binding(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding: Definition<'db>,
        evaluator: &NarrowingEvaluator<'_, 'db>,
    ) -> Self {
        let input = InferenceVariable::new(
            db,
            env.program(db),
            InferenceOwner::Region(InferenceRegion::Definition(binding)),
            InferenceSlot::Binding(binding),
        );
        let mut constraints = InferenceConstraints::default();
        let value = constraints.import(db, self);
        let value = constraints.narrow(db, env, input, value, evaluator, binding.place(db));
        constraints.finish(db, value)
    }

    pub(crate) fn map(self, db: &'db dyn Db, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Self {
        Self::new(db, f(self.ty(db)), self.graph(db))
    }

    /// Retain every dependency reached by equivalent cycle results.
    ///
    /// Variable identities are stable across iterations, but their equation payloads can contain
    /// the current type approximation. Normalize those payloads using the same fixed-point rule
    /// as ordinary inferred types before Salsa compares the retained graph.
    pub(crate) fn cycle_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        let mut constraints = InferenceConstraints::default();
        constraints.import(db, previous);
        for (variable, equation) in self.equations(db) {
            let equation = constraints
                .equations
                .get(variable)
                .map_or(*equation, |previous| {
                    equation.cycle_normalized(db, env, *previous, cycle)
                });
            constraints.equations.insert(*variable, equation);
        }
        let ty = self.ty(db);
        let previous_ty = previous.ty(db);
        constraints.finish(
            db,
            if ty == previous_ty {
                ty
            } else {
                ty.cycle_normalized(db, env, previous_ty, cycle)
            },
        )
    }

    /// Normalize graphs stored in one query result by their stable output slots.
    pub(crate) fn cycle_normalized_map<K>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        current: FrozenMap<K, Self>,
        previous: &FrozenMap<K, Self>,
        cycle: &salsa::Cycle,
    ) -> FrozenMap<K, Self>
    where
        K: Copy + Eq + Hash + Ord,
    {
        let mut normalized: FxHashMap<_, _> = current.into_iter().collect();
        for (slot, previous) in previous {
            if let Some(current) = normalized.get_mut(slot) {
                *current = current.cycle_normalized(db, env, *previous, cycle);
            } else {
                normalized.insert(*slot, *previous);
            }
        }
        FrozenMap::from(normalized)
    }

    /// Resolve this output using the shared solution of its complete equations.
    pub(crate) fn resolve(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        let bindings = self.graph(db).solve(db, env.program(db))?;
        let mut producers = InferenceSolutions {
            db,
            env,
            bindings: bindings.iter().copied().collect(),
            active: Vec::new(),
            resolved: FxIndexMap::default(),
            evaluated: FxHashMap::default(),
            growing: FxHashSet::default(),
        };
        let ty = producers.resolve_type(self.ty(db));
        producers.is_ground(ty).then_some(ty)
    }
}

/// Mutable state belongs to one query invocation. Cached values contain immutable equations.
#[derive(Default)]
pub(crate) struct InferenceConstraints<'db> {
    equations: FxIndexMap<InferenceVariable<'db>, Equation<'db>>,
}

impl<'db> InferenceConstraints<'db> {
    /// Merge dependencies carried by an input and return its symbolic type.
    pub(crate) fn import(&mut self, db: &'db dyn Db, value: SymbolicType<'db>) -> Type<'db> {
        for (variable, equation) in value.equations(db) {
            if !matches!(equation, Equation::Pending) || !self.equations.contains_key(variable) {
                self.equations.insert(*variable, *equation);
            }
        }
        value.ty(db)
    }

    /// Retain promotion so that types learned later receive the same widening as known values.
    pub(crate) fn promote(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        input: InferenceVariable<'db>,
        value: Type<'db>,
        promotion: InferencePromotion,
    ) -> Type<'db> {
        self.apply(
            db,
            env,
            InferenceOwner::Promotion(input),
            InferenceSlot::Root,
            InferenceOperation::Promote { value, promotion },
        )
    }

    /// Apply a lexical narrowing constraint once the subject's type arguments are known.
    pub(crate) fn narrow(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        input: InferenceVariable<'db>,
        value: Type<'db>,
        evaluator: &NarrowingEvaluator<'_, 'db>,
        place: ScopedPlaceId,
    ) -> Type<'db> {
        let constraint = evaluator.constraint();
        match constraint {
            ScopedNarrowingConstraint::ALWAYS_TRUE => return value,
            ScopedNarrowingConstraint::ALWAYS_FALSE => return Type::Never,
            _ => {}
        }
        let predicate = evaluator
            .narrowing_constraints()
            .get_interior_node(constraint)
            .atom;
        let narrowing = InferenceNarrowing {
            scope: predicate_scope(db, &evaluator.predicates()[predicate]),
            place,
            constraint,
        };
        self.apply(
            db,
            env,
            InferenceOwner::Narrowing(input, narrowing),
            InferenceSlot::Root,
            InferenceOperation::Narrow { value, narrowing },
        )
    }

    /// Close a recursive reference with the type computed by its owning query.
    pub(crate) fn define(
        &mut self,
        db: &'db dyn Db,
        program: Program<'db>,
        owner: InferenceOwner<'db>,
        slot: InferenceSlot<'db>,
        ty: Type<'db>,
    ) -> Type<'db> {
        let variable = InferenceVariable::new(db, program, owner, slot);
        // Keep output identities independent of where evaluation entered the query cycle, but do
        // not replace a producer equation imported from an earlier query result.
        if self
            .equations
            .get(&variable)
            .is_some_and(|equation| !matches!(equation, Equation::Pending))
        {
            return variable.ty(db);
        }
        if ty != variable.ty(db) {
            self.equations.insert(variable, Equation::Value(ty));
        }
        variable.ty(db)
    }

    /// Constrain an attribute read using a read-only protocol member.
    pub(crate) fn read_member(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        owner: InferenceOwner<'db>,
        expression: ExpressionNodeKey,
        source: Type<'db>,
        name: &str,
    ) -> Type<'db> {
        let variable = InferenceVariable::new(
            db,
            env.program(db),
            owner,
            InferenceSlot::Expression(expression),
        );
        let result = variable.ty(db);
        let target = Type::protocol_with_readonly_members(db, env, [(name, result)]);
        self.equations
            .insert(variable, Equation::Requirement { source, target });
        result
    }

    /// Retain an operation until its inputs have been supplied by their producers.
    pub(crate) fn apply(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        owner: InferenceOwner<'db>,
        slot: InferenceSlot<'db>,
        operation: InferenceOperation<'db>,
    ) -> Type<'db> {
        let variable = InferenceVariable::new(db, env.program(db), owner, slot);
        let result = variable.ty(db);
        self.equations
            .insert(variable, Equation::Operation(operation));
        result
    }

    /// Represent a mapping spread by a dictionary with the same key and value types.
    pub(crate) fn unpack_mapping(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        owner: InferenceOwner<'db>,
        expression: ExpressionNodeKey,
        value: Type<'db>,
    ) -> Type<'db> {
        let elements = [
            (0, InferenceOperation::MappingKey(value)),
            (1, InferenceOperation::MappingValue(value)),
        ]
        .map(|(index, operation)| {
            self.apply(
                db,
                env,
                owner,
                InferenceSlot::Component(expression, index),
                operation,
            )
        });
        KnownClass::Dict.to_specialized_instance(db, env, &elements)
    }

    /// Freeze the equations so they can be retained by Salsa query results.
    pub(crate) fn finish(&self, db: &'db dyn Db, ty: Type<'db>) -> SymbolicType<'db> {
        let mut equations: Vec<_> = self
            .equations
            .iter()
            .map(|(key, value)| (*key, *value))
            .collect();
        // The same dependency graph can be collected from different cycle entry points. Salsa
        // compares this slice structurally while finding a fixed point, so insertion order would
        // make equal graphs look different and prevent cycle recovery from converging.
        equations.sort_unstable_by_key(|(variable, _)| variable.0.as_id());
        SymbolicType::new(
            db,
            ty,
            InferenceEquations::new(db, equations.into_boxed_slice()),
        )
    }
}

/// Substitute partial solutions without replacing unresolved variables with a dynamic type.
struct InferenceSolutions<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    bindings: FxIndexMap<BoundTypeVarInstance<'db>, Type<'db>>,
    active: Vec<BoundTypeVarInstance<'db>>,
    resolved: FxIndexMap<BoundTypeVarInstance<'db>, Type<'db>>,
    evaluated: FxHashMap<(InferenceVariable<'db>, InferenceOperation<'db>), Option<Type<'db>>>,
    growing: FxHashSet<BoundTypeVarInstance<'db>>,
}

enum OperationBinding {
    Changed,
    Stable,
    RejectedGrowth,
}

impl<'db> InferenceSolutions<'_, 'db> {
    fn is_ground(&self, ty: Type<'db>) -> bool {
        ty.inference_variable_context(self.db, self.env.program(self.db))
            .is_none()
    }

    /// Use only complete union alternatives to start an operation whose inputs form a cycle.
    fn productive_type(&self, ty: Type<'db>) -> Option<Type<'db>> {
        if self.is_ground(ty) {
            return Some(ty);
        }
        let Type::Union(union) = ty else {
            return None;
        };
        let mut elements = union
            .elements(self.db)
            .iter()
            .copied()
            .filter(|ty| self.is_ground(*ty))
            .peekable();
        elements.peek()?;
        Some(UnionType::from_elements(self.db, self.env, elements))
    }

    fn evaluate_operations(
        &mut self,
        operations: &[(InferenceVariable<'db>, InferenceOperation<'db>)],
        budget: &mut SolutionBudget,
    ) -> Result<(bool, bool), ProjectionError> {
        let mut changed = false;
        let mut complete = true;
        let mut deferred = Vec::new();
        for (variable, operation) in operations {
            self.active.push(variable.typevar(self.db));
            let operation = operation.map(|ty| self.operation_input(ty));
            self.active.pop();
            let mut operation_complete = true;
            let productive = operation.map(|ty| {
                self.productive_type(ty).unwrap_or_else(|| {
                    complete = false;
                    operation_complete = false;
                    ty
                })
            });
            if operation_complete {
                let Some(ty) = self.evaluate_operation(*variable, productive, budget)? else {
                    complete = false;
                    continue;
                };
                match self
                    .bind_operation(*variable, ty, &mut budget.type_terms)
                    .ok_or(ProjectionError::TypeBudgetExceeded)?
                {
                    OperationBinding::Changed => changed = true,
                    OperationBinding::Stable => {}
                    OperationBinding::RejectedGrowth => complete = false,
                }
            } else {
                deferred.push((*variable, operation));
            }
        }
        // Exhaust known inputs before evaluating unresolved ones. An overload selected from an
        // unresolved argument can otherwise seed a wider, self-sustaining solution to the cycle.
        if !changed {
            for (variable, operation) in deferred {
                let Some(ty) = self.evaluate_operation(variable, operation, budget)? else {
                    continue;
                };
                match self
                    .bind_operation(variable, ty, &mut budget.type_terms)
                    .ok_or(ProjectionError::TypeBudgetExceeded)?
                {
                    OperationBinding::Changed => changed = true,
                    OperationBinding::Stable | OperationBinding::RejectedGrowth => {}
                }
            }
        }
        Ok((changed, complete))
    }

    fn evaluate_operation(
        &mut self,
        variable: InferenceVariable<'db>,
        operation: InferenceOperation<'db>,
        budget: &mut SolutionBudget,
    ) -> Result<Option<Type<'db>>, ProjectionError> {
        let key = (variable, operation);
        if let Some(result) = self.evaluated.get(&key) {
            return Ok(*result);
        }
        // An exhausted budget ends the whole solve, unlike an unresolved operation that can
        // become evaluable after another producer supplies its inputs.
        operation.charge_inputs(self.db, self.env, &mut budget.visits)?;
        let result = operation.evaluate(self.db, self.env, variable);
        self.evaluated.insert(key, result);
        Ok(result)
    }

    fn bind_operation(
        &mut self,
        variable: InferenceVariable<'db>,
        ty: Type<'db>,
        remaining: &mut usize,
    ) -> Option<OperationBinding> {
        let variable = variable.typevar(self.db);
        if self
            .bindings
            .get(&variable)
            .is_some_and(|previous| previous.is_equivalent_to(self.db, self.env, ty))
        {
            return Some(OperationBinding::Stable);
        }
        let grows = self
            .bindings
            .get(&variable)
            .copied()
            .is_some_and(|previous| self.contains_nested_type(ty, previous));
        if grows {
            // One structural change can still reach a fixed point after overload selection
            // changes. Preserve it and reject only the next self-embedding update.
            if !self.growing.insert(variable) {
                // Repeatedly embedding the previous result has no finite structural fixed point.
                // Keeping the last approximation lets the outer solve reject the incomplete
                // equation instead of unfolding it indefinitely.
                return Some(OperationBinding::RejectedGrowth);
            }
        } else {
            self.growing.remove(&variable);
        }
        // Constructed recursive types can grow without a finite solution. Bound all visited
        // type nodes, including nested generic arguments, and discard incomplete results.
        let fuel = Cell::new(*remaining);
        if any_over_type(self.db, self.env, ty, false, |_| {
            let Some(next) = fuel.get().checked_sub(1) else {
                return true;
            };
            fuel.set(next);
            false
        }) {
            return None;
        }
        *remaining = fuel.get();
        self.bindings.insert(variable, ty);
        self.resolved.clear();
        Some(OperationBinding::Changed)
    }

    fn contains_nested_type(&self, ty: Type<'db>, target: Type<'db>) -> bool {
        match ty {
            Type::Union(union) => union.elements(self.db).iter().copied().any(|element| {
                element != target
                    && any_over_type(self.db, self.env, element, false, |ty| ty == target)
            }),
            _ => ty != target && any_over_type(self.db, self.env, ty, false, |ty| ty == target),
        }
    }

    fn solve_requirements(
        &mut self,
        equations: &[(InferenceVariable<'db>, Equation<'db>)],
    ) -> Option<Vec<Type<'db>>> {
        let db = self.db;
        let env = self.env;
        // Only requirement outputs are rebound; other producers retain their defining equations.
        let outputs: Vec<_> = equations
            .iter()
            .filter(|(_, equation)| matches!(equation, Equation::Requirement { .. }))
            .map(|(variable, _)| self.resolve_type(variable.ty(db)))
            .collect();
        let builder = ConstraintSetBuilder::new();
        let inferable = TypeVarSet::from_typevars(
            db,
            equations.iter().map(|(variable, _)| variable.typevar(db)),
        );
        let mut constraints = ConstraintSet::from_bool(&builder, true);
        for (_, equation) in equations {
            let Equation::Requirement { source, target } = equation else {
                continue;
            };
            let source = self.resolve_type(*source);
            let target = self.resolve_type(*target);
            let owned = source.when_constraint_set_assignable_to_owned(db, env, target);
            let next = builder.load(db, env, &owned);
            constraints = constraints.intersect(db, &builder, next);
        }

        let result = constraints
            .try_fold_solutions(
                db,
                env,
                inferable,
                SolutionBudget::default(),
                |_variance, bound| {
                    if bound.evidence_lower().is_none() {
                        PathBoundSolution::Unsolved
                    } else {
                        PathBounds::default_solve(db, env, &builder, bound)
                    }
                },
                vec![Type::Never; outputs.len()],
                |previous, bindings, budget| {
                    let mut solutions = InferenceSolutions {
                        db,
                        env,
                        bindings: bindings
                            .iter()
                            .map(|binding| (binding.bound_typevar, binding.solution))
                            .collect(),
                        active: Vec::new(),
                        resolved: FxIndexMap::default(),
                        evaluated: FxHashMap::default(),
                        growing: FxHashSet::default(),
                    };
                    previous
                        .into_iter()
                        .zip(&outputs)
                        .map(|(previous, output)| {
                            let ty = solutions.resolve_type(*output);
                            budget.charge_type(db, ty)?;
                            Ok(UnionType::from_two_elements(db, env, previous, ty))
                        })
                        .collect()
                },
            )
            .ok()?;
        match result {
            SolutionProjection::Constrained(types) => Some(types),
            SolutionProjection::Unconstrained => Some(outputs),
            SolutionProjection::Unsatisfiable => None,
        }
    }

    fn resolve_type(&mut self, ty: Type<'db>) -> Type<'db> {
        let Some(context) = ty.inference_variable_context(self.db, self.env.program(self.db))
        else {
            return ty;
        };
        let types: Vec<_> = context
            .variables(self.db)
            .map(|variable| self.resolve_variable(variable))
            .collect();
        let specialization =
            Specialization::new(self.db, context, types.into_boxed_slice(), None, None);
        ty.apply_type_mapping(
            self.db,
            self.env,
            &TypeMapping::ApplySpecialization(ApplySpecialization::specialization(specialization)),
            TypeContext::default(),
        )
    }

    /// Preserve references inside incomplete constructed types instead of repeatedly unfolding
    /// them, as in X = tuple[X, int]. Ground inputs can use their complete specialization.
    fn operation_input(&mut self, ty: Type<'db>) -> Type<'db> {
        let resolved = self.resolve_type(ty);
        if self.is_ground(resolved) {
            resolved
        } else {
            self.resolve_outer_type(ty)
        }
    }

    fn resolve_outer_type(&self, ty: Type<'db>) -> Type<'db> {
        // Collapse cycles of aliases and unions before descending into constructed types.
        // R >= int | S, S >= R has the solution int for both variables.
        let mut pending = vec![ty];
        let mut seen = FxHashSet::default();
        let mut alternatives = Vec::new();
        while let Some(ty) = pending.pop() {
            match ty {
                Type::TypeVar(variable)
                    if matches!(
                        variable.binding_context(self.db),
                        BindingContext::Inference(_)
                    ) =>
                {
                    if seen.insert(variable) {
                        if let Some(ty) = self.binding(variable) {
                            pending.push(ty);
                        } else {
                            alternatives.push(Type::TypeVar(variable));
                        }
                    }
                }
                // Preserve complete unions instead of rebuilding them in a different order.
                Type::Union(union) if !self.is_ground(ty) => {
                    pending.extend(union.elements(self.db).iter().rev());
                }
                _ => alternatives.push(ty),
            }
        }
        if alternatives.is_empty() {
            ty
        } else {
            UnionType::from_elements(self.db, self.env, alternatives)
        }
    }

    fn resolve_variable(&mut self, variable: BoundTypeVarInstance<'db>) -> Type<'db> {
        if self.active.contains(&variable) {
            return self.binding(variable).unwrap_or(Type::TypeVar(variable));
        }
        if let Some(ty) = self.resolved.get(&variable) {
            return *ty;
        }
        let ty = self.resolve_outer_type(Type::TypeVar(variable));
        self.active.push(variable);
        let ty = self.resolve_type(ty);
        self.active.pop();
        if self.active.is_empty() {
            self.resolved.insert(variable, ty);
        }
        ty
    }

    /// A partial result cannot expand itself while its producer is being evaluated.
    /// Ground results can feed back into the next iteration without unfolding recursion.
    fn binding(&self, variable: BoundTypeVarInstance<'db>) -> Option<Type<'db>> {
        self.bindings
            .get(&variable)
            .copied()
            .filter(|ty| !self.active.contains(&variable) || self.is_ground(*ty))
    }
}

impl<'db> PlaceAndQualifiers<'db> {
    /// Apply a generic owner's arguments to both the member type and its pending constraints.
    pub(crate) fn apply_owner_specialization(
        mut self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Self {
        if let Some(specialization) = specialization
            && let Place::Defined(place) = &mut self.place
        {
            place.ty = place
                .ty
                .apply_optional_owner_specialization_to_member(db, Some(specialization));
            place.symbolic = place
                .symbolic
                .map(|symbolic| symbolic.specialized(db, specialization));
        }
        self
    }
}
