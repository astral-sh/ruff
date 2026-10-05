//! Retains independent comparisons from a closed runtime-alternative composition.

use std::rc::Rc;

use rustc_hash::FxHashSet;

use super::{
    CallableRelationStep, QueuedSignatureEffects, RelationContext, RelationGoal, RelationKey,
    RelationOutput, RelationRequest, TypeRelationChecker,
};
use crate::types::callable::scheduled_probe::{
    Boundary, ConversionPlan, ConversionTransform, DiagnosticSeedId, Router, run_with,
};
use crate::types::callable::{CallableConversionRequest, CallableType};
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::signatures::effects::SignatureEffect;
use crate::types::{ErrorContext, Type, UpcastPolicy};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CallableRelationVerdict {
    Compatible,
    Incompatible,
    Incomplete,
}

/// A callable relation whose result contains each completed comparison and its explanation.
#[derive(Clone, Copy)]
pub(crate) struct ComposedCallableRequest<'db, 'c> {
    request: RelationRequest<'db, 'c>,
}

#[derive(Clone, Copy)]
pub(crate) struct ComposedRelationPolicy {
    pub(crate) allowance: usize,
    pub(crate) reverse_execution: bool,
    pub(crate) reverse_merge: bool,
}

impl<'db, 'c> ComposedCallableRequest<'db, 'c> {
    pub(crate) fn new(
        constraints: &'c ConstraintSetBuilder<'db>,
        source: Type<'db>,
        target: CallableType<'db>,
        context: RelationContext<'db, 'c>,
    ) -> Result<Self, Boundary> {
        Ok(Self {
            request: RelationRequest::composed_callable(constraints, source, target, context)?,
        })
    }

    pub(crate) fn with_seed(self, seed: DiagnosticSeedId) -> Self {
        Self {
            request: self.request.with_seed(seed),
        }
    }

    pub(crate) fn source(self) -> Type<'db> {
        self.request.source()
    }

    pub(crate) fn comparison_source(self) -> Type<'db> {
        self.request.comparison_source()
    }

    /// Runs the relation and exports all committed comparisons under one total allowance.
    ///
    /// Aggregate scheduling answers carry no combined diagnostic context. They remain private;
    /// callers receive each complete leaf's explanation through this structured outcome.
    pub(crate) fn run(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        router: &Router<'db, 'c>,
        policy: ComposedRelationPolicy,
    ) -> Result<CallableRelationOutcome<'db, 'c>, Boundary> {
        router.validate_relation(self.request)?;
        // With E published edges and F compositions, traversal inspects at most E + 1
        // nodes, exports E child slots, and creates/visits F finish markers. This costs
        // at most 3E + 2F + 2. The scheduler charges every published edge and composition,
        // so E and F each cannot exceed semantic work: five units per semantic unit suffice.
        // Reserving this bound before execution makes every committed leaf exportable even
        // when a larger allowance exposes more of an earlier runtime alternative.
        let semantic_allowance = policy.allowance.saturating_sub(2) / 6;
        let transport_allowance = policy.allowance - semantic_allowance;
        let snapshot = run_with(
            db,
            env,
            router,
            semantic_allowance,
            policy.reverse_execution,
            policy.reverse_merge,
            |router| async { router.declare_relation(self.request) },
        )?;
        if let Some(Err(boundary)) = snapshot.consumer {
            return Err(boundary);
        }
        let mut outcome = self.request.callable_outcome(router, transport_allowance)?;
        outcome.semantic_work = snapshot.work();
        outcome.work = outcome
            .semantic_work
            .checked_add(outcome.transport_work)
            .ok_or(Boundary::CostOverflow)?;
        Ok(outcome)
    }
}

pub(crate) struct CompletedCallableComparison<'db, 'c> {
    pub(crate) request: ComposedCallableRequest<'db, 'c>,
    pub(crate) output: RelationOutput<'db, 'c>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UnresolvedReason {
    Pending,
    Boundary(Boundary),
    ConstraintSatisfiability,
    Cycle,
}

pub(crate) struct UnresolvedCallableComparison<'db, 'c> {
    pub(crate) request: ComposedCallableRequest<'db, 'c>,
    pub(crate) reason: UnresolvedReason,
}

pub(crate) struct CallableRelationOutcome<'db, 'c> {
    pub(crate) verdict: CallableRelationVerdict,
    pub(crate) completed: Vec<CompletedCallableComparison<'db, 'c>>,
    pub(crate) unresolved: Vec<UnresolvedCallableComparison<'db, 'c>>,
    /// These requests could not be inspected within the separate transport allowance.
    pub(crate) unvisited: Vec<RelationFrontier<'db, 'c>>,
    pub(crate) semantic_work: usize,
    pub(crate) transport_work: usize,
    pub(crate) work: usize,
}

#[derive(Clone, Copy)]
pub(crate) enum RelationFrontier<'db, 'c> {
    Inspect(ComposedCallableRequest<'db, 'c>),
    Finish(ComposedCallableRequest<'db, 'c>),
}

impl<'db, 'c> RelationRequest<'db, 'c> {
    fn alternative(self, source: Type<'db>) -> Self {
        let goal = match self.key.goal {
            RelationGoal::ComposedCallableSource {
                target, regularize, ..
            } => RelationGoal::ComposedCallableSource {
                target,
                regularize,
                upcast_source: None,
            },
            goal => goal,
        };
        Self {
            key: RelationKey {
                source,
                goal,
                ..self.key
            },
            ..self
        }
    }

    fn comparison_source(self) -> Type<'db> {
        match self.key.goal {
            RelationGoal::ComposedCallableSource {
                upcast_source: Some(source),
                ..
            } => source,
            _ => self.key.source,
        }
    }

    /// Collects only committed leaf results, retaining an explicit frontier on exhaustion.
    fn callable_outcome(
        self,
        router: &Router<'db, 'c>,
        allowance: usize,
    ) -> Result<CallableRelationOutcome<'db, 'c>, Boundary> {
        router.validate_relation(self)?;
        if !matches!(self.key.goal, RelationGoal::ComposedCallableSource { .. }) {
            return Err(Boundary::RelationContext);
        }
        let mut outcome = CallableRelationOutcome {
            verdict: CallableRelationVerdict::Incomplete,
            completed: Vec::new(),
            unresolved: Vec::new(),
            unvisited: vec![RelationFrontier::Inspect(ComposedCallableRequest {
                request: self,
            })],
            semantic_work: 0,
            transport_work: 0,
            work: 0,
        };
        let mut active = FxHashSet::default();
        let mut finished = FxHashSet::default();
        let mut rejected = false;
        while let Some(frontier) = outcome.unvisited.last().copied() {
            if let RelationFrontier::Finish(request) = frontier {
                let request = request.request;
                let Some(marker_work) = outcome.transport_work.checked_add(1) else {
                    return Err(Boundary::CostOverflow);
                };
                if marker_work > allowance {
                    break;
                }
                outcome.transport_work = marker_work;
                outcome.unvisited.pop();
                active.remove(&request.key);
                finished.insert(request.key);
                continue;
            }
            let RelationFrontier::Inspect(request) = frontier else {
                continue;
            };
            let request = request.request;
            // A node pays for lookup and export before cloning either its child handle or its
            // immutable answer. Child slots pay separately before expanding the frontier.
            let Some(node_work) = outcome.transport_work.checked_add(2) else {
                return Err(Boundary::CostOverflow);
            };
            if node_work > allowance {
                break;
            }
            outcome.transport_work = node_work;
            if active.contains(&request.key) {
                outcome.unvisited.pop();
                outcome.unresolved.push(UnresolvedCallableComparison {
                    request: ComposedCallableRequest { request },
                    reason: UnresolvedReason::Cycle,
                });
                continue;
            }
            if finished.contains(&request.key) {
                outcome.unvisited.pop();
                continue;
            }
            if let Some(children) = router.relation_composition(request.key) {
                let Some(edge_work) = node_work
                    .checked_add(children.len())
                    .and_then(|work| work.checked_add(1))
                else {
                    return Err(Boundary::CostOverflow);
                };
                if edge_work > allowance {
                    break;
                }
                outcome.transport_work = edge_work;
                outcome.unvisited.pop();
                active.insert(request.key);
                outcome
                    .unvisited
                    .push(RelationFrontier::Finish(ComposedCallableRequest {
                        request,
                    }));
                outcome.unvisited.extend(
                    children.iter().rev().copied().map(|request| {
                        RelationFrontier::Inspect(ComposedCallableRequest { request })
                    }),
                );
                continue;
            }
            outcome.unvisited.pop();
            finished.insert(request.key);
            match router.relation_answer(request.key) {
                Some(Ok(output)) => {
                    rejected |= output.constraints.is_trivially_never_satisfied();
                    if !output.constraints.is_trivially_always_satisfied()
                        && !output.constraints.is_trivially_never_satisfied()
                    {
                        outcome.unresolved.push(UnresolvedCallableComparison {
                            request: ComposedCallableRequest { request },
                            reason: UnresolvedReason::ConstraintSatisfiability,
                        });
                    }
                    outcome.completed.push(CompletedCallableComparison {
                        request: ComposedCallableRequest { request },
                        output,
                    });
                }
                Some(Err(boundary)) => outcome.unresolved.push(UnresolvedCallableComparison {
                    request: ComposedCallableRequest { request },
                    reason: UnresolvedReason::Boundary(boundary),
                }),
                None => outcome.unresolved.push(UnresolvedCallableComparison {
                    request: ComposedCallableRequest { request },
                    reason: UnresolvedReason::Pending,
                }),
            }
        }
        outcome.verdict = if rejected {
            CallableRelationVerdict::Incompatible
        } else if outcome.unresolved.is_empty() && outcome.unvisited.is_empty() {
            CallableRelationVerdict::Compatible
        } else {
            CallableRelationVerdict::Incomplete
        };
        outcome.work = outcome.transport_work;
        Ok(outcome)
    }
}

/// Whether a complete leaf applies regularization before comparing its signatures.
pub(crate) fn composition_regularizes(key: RelationKey<'_>) -> bool {
    matches!(
        key.goal,
        RelationGoal::ComposedCallableSource {
            regularize: true,
            ..
        }
    )
}

/// Reserve fanout work before the evaluator maps, publishes, or folds child requests.
pub(crate) fn composition_payload_debit<'db>(
    router: &Router<'db, '_>,
    key: RelationKey<'db>,
) -> Result<usize, Boundary> {
    if !matches!(key.goal, RelationGoal::ComposedCallableSource { .. }) {
        return Ok(0);
    }
    let conversion = CallableConversionRequest::new(key.source, UpcastPolicy::from(key.relation));
    let count = match router.conversion_plan(conversion) {
        Some(ConversionPlan::Alternatives(children)) => children.len(),
        Some(ConversionPlan::Transform { .. }) => 1,
        _ => 0,
    };
    count
        .checked_mul(8)
        .and_then(|debit| debit.checked_add(1))
        .ok_or(Boundary::CostOverflow)
}

pub(super) async fn evaluate_composed<'db, 'c>(
    db: &'db dyn Db,
    checker: &TypeRelationChecker<'_, 'c, 'db>,
    router: &Router<'db, 'c>,
    request: RelationRequest<'db, 'c>,
    target: CallableType<'db>,
    regularize: bool,
    upcast_source: Option<Type<'db>>,
) -> Result<ConstraintSet<'db, 'c>, Boundary> {
    let pending = match CallableRelationStep::start(db, checker, request.key.source, target) {
        CallableRelationStep::Complete(result) => return Ok(result),
        CallableRelationStep::Convert(pending) => pending,
    };
    let result = match router
        .relation_conversion_plan_demand(request.key, pending.request)
        .await
    {
        ConversionPlan::Alternatives(alternatives) => {
            let children: Rc<[_]> = alternatives
                .iter()
                .map(|alternative| request.alternative(alternative.source_type()))
                .collect();
            router.publish_relation_composition(request.key, Rc::clone(&children))?;
            let mut rejected = false;
            for child in children.iter() {
                let output = router.relation_demand(request.key, *child).await?;
                if output.constraints.is_trivially_never_satisfied() {
                    rejected = true;
                } else if !output.constraints.is_trivially_always_satisfied() {
                    return Err(Boundary::SignatureEffect(
                        SignatureEffect::ConstraintSatisfiability,
                    ));
                }
            }
            ConstraintSet::from_bool(checker.constraints, !rejected)
        }
        ConversionPlan::Transform {
            input,
            transformation,
        } => {
            let regularize = match transformation {
                ConversionTransform::Identity => regularize,
                ConversionTransform::Regularize => true,
                _ => return Err(Boundary::SemanticOperation),
            };
            let child = RelationRequest {
                key: RelationKey {
                    source: input.source_type(),
                    goal: RelationGoal::ComposedCallableSource {
                        target,
                        regularize,
                        upcast_source: Some(upcast_source.unwrap_or(request.key.source)),
                    },
                    ..request.key
                },
                ..request
            };
            router.publish_relation_composition(request.key, Rc::from([child]))?;
            router
                .relation_demand(request.key, child)
                .await?
                .constraints
        }
        ConversionPlan::Complete(answer) => {
            let Some(mut callables) = answer? else {
                return Ok(pending.visit.finish(checker.never()));
            };
            if regularize {
                callables = ConversionTransform::Regularize.apply(db, checker.env, callables);
            }
            let effects = QueuedSignatureEffects {
                router,
                parent: request.key,
                env: checker.env,
            };
            let result = checker
                .check_callables_vs_callable_with(db, &effects, &callables, pending.target)
                .await?;
            let diagnostic_source = upcast_source.unwrap_or(pending.source);
            if result.is_trivially_never_satisfied()
                && checker.should_provide_callable_upcast_context(diagnostic_source)
                && let Some(context) = checker.report_context()
                && let Some(callable) = callables.exactly_one()
            {
                // A complete leaf already has its full overload set. Building a union display
                // would need normalization, so multiple converted callables retain only the
                // explanations produced by the shared comparator.
                context.push(ErrorContext::InferredCallableType {
                    source: diagnostic_source,
                    callable: Type::Callable(callable),
                });
            }
            result
        }
    };
    Ok(pending.visit.finish(result))
}
