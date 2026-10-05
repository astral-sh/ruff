//! Resumable step-5 overload filtering for gradual argument types.

use std::collections::HashSet;
use std::sync::Arc;

use super::{BinderComparison, BinderCondition, CallArguments, CallableBinding};
use crate::db::Db;
use crate::types::typevar::TypeVarSet;
use crate::types::{ProgramEnvironment, Type, UnionBuilder};

struct OverloadFilterSlot<'db> {
    parameter: Type<'db>,
    argument: Type<'db>,
    variadic_argument: Option<Type<'db>>,
}

struct OverloadFilterCandidate<'db> {
    slots: Box<[OverloadFilterSlot<'db>]>,
    inferable_typevars: TypeVarSet<'db>,
    return_type: Type<'db>,
}

/// Either the next comparison or the completed filtering decision.
pub(super) enum OverloadFilterStep<'db> {
    Compare(PendingOverloadFilter<'db>),
    Complete(OverloadFilterResult),
}

impl<'db> OverloadFilterStep<'db> {
    pub(super) fn start(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding: &CallableBinding<'db>,
        arguments: &CallArguments<'_, 'db>,
        matching_overload_indexes: &[usize],
    ) -> Self {
        let candidates: Arc<[_]> = matching_overload_indexes
            .iter()
            .map(|&index| {
                let overload = &binding.overloads[index];
                let slots = overload
                    .argument_matches
                    .iter()
                    .zip(arguments.iter_types())
                    .flat_map(move |(matched_argument, argument_types)| {
                        matched_argument.iter().map(move |matched_parameter| {
                            // TODO: For an unannotated `self` / `cls` parameter, the type should be
                            // `typing.Self` / `type[typing.Self]`
                            let raw_parameter_type = overload.signature.parameters()
                                [matched_parameter.index]
                                .annotated_type();
                            let parameter_type = raw_parameter_type.apply_optional_specialization(
                                db,
                                overload.merged_specialization(db),
                            );
                            OverloadFilterSlot {
                                parameter: parameter_type,
                                // Argument types are cached by the raw parameter type, even when
                                // they were inferred using a return-context specialization.
                                argument: argument_types.get_for_declared_type(raw_parameter_type),
                                variadic_argument: matched_parameter.argument_type,
                            }
                        })
                    })
                    .collect();
                OverloadFilterCandidate {
                    slots,
                    inferable_typevars: overload.inferable_typevars,
                    return_type: overload.return_type(),
                }
            })
            .collect();

        let max_slot_count = candidates
            .iter()
            .map(|candidate| candidate.slots.len())
            .max()
            .unwrap_or(0);

        CompareParameterSlots {
            candidates,
            max_slot_count,
            participating_slot_indices: HashSet::new(),
            slot_index: 0,
            next_candidate: 0,
            first_parameter_type: None,
        }
        .advance(db, env)
    }
}

pub(super) struct OverloadFilterResult {
    pub(super) retained_count: usize,
    pub(super) is_ambiguous: bool,
}

/// Owns the continuation for exactly one unanswered comparison.
///
/// Resuming consumes this value, so an answer cannot be supplied before a comparison is yielded.
/// Candidate inputs are shared when a continuation is cloned; no binding or constraint builder is
/// borrowed while the comparison is pending.
#[derive(Clone)]
pub(super) struct PendingOverloadFilter<'db> {
    condition: BinderCondition<'db>,
    continuation: OverloadFilterContinuation<'db>,
}

impl<'db> PendingOverloadFilter<'db> {
    pub(super) fn condition(&self) -> BinderCondition<'db> {
        self.condition
    }

    pub(super) fn resume(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        answer: bool,
    ) -> OverloadFilterStep<'db> {
        match self.continuation {
            OverloadFilterContinuation::ParameterSlots(mut state) => {
                if !answer {
                    state.participating_slot_indices.insert(state.slot_index);
                }
                state.advance(db, env)
            }
            OverloadFilterContinuation::MaterializedArguments(state) => {
                if answer {
                    CompareReturnTypes {
                        candidates: state.candidates,
                        retained_count: state.next_candidate,
                        next_candidate: 1,
                    }
                    .advance()
                } else {
                    state.advance(db, env)
                }
            }
            OverloadFilterContinuation::ReturnTypes(state) => {
                if answer {
                    state.advance()
                } else {
                    OverloadFilterStep::Complete(OverloadFilterResult {
                        retained_count: state.retained_count,
                        is_ambiguous: true,
                    })
                }
            }
        }
    }
}

#[derive(Clone)]
enum OverloadFilterContinuation<'db> {
    ParameterSlots(CompareParameterSlots<'db>),
    MaterializedArguments(CompareMaterializedArguments<'db>),
    ReturnTypes(CompareReturnTypes<'db>),
}

#[derive(Clone)]
struct CompareParameterSlots<'db> {
    candidates: Arc<[OverloadFilterCandidate<'db>]>,
    max_slot_count: usize,
    participating_slot_indices: HashSet<usize>,
    slot_index: usize,
    next_candidate: usize,
    first_parameter_type: Option<Type<'db>>,
}

impl<'db> CompareParameterSlots<'db> {
    fn advance(
        mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> OverloadFilterStep<'db> {
        while self.slot_index < self.max_slot_count {
            while let Some(candidate) = self.candidates.get(self.next_candidate) {
                self.next_candidate += 1;
                let current_parameter_type = candidate
                    .slots
                    .get(self.slot_index)
                    .map(|slot| slot.parameter);
                match (self.first_parameter_type, current_parameter_type) {
                    (Some(first_parameter_type), Some(current_parameter_type)) => {
                        return OverloadFilterStep::Compare(PendingOverloadFilter {
                            condition: BinderCondition::Always(BinderComparison::Equivalent {
                                left: first_parameter_type,
                                right: current_parameter_type,
                            }),
                            continuation: OverloadFilterContinuation::ParameterSlots(self),
                        });
                    }
                    (Some(_), None) => {
                        self.participating_slot_indices.insert(self.slot_index);
                    }
                    (None, Some(current_parameter_type)) => {
                        self.first_parameter_type = Some(current_parameter_type);
                    }
                    (None, None) => {}
                }
            }
            self.slot_index += 1;
            self.next_candidate = 0;
            self.first_parameter_type = None;
        }

        CompareMaterializedArguments {
            candidates: self.candidates,
            max_slot_count: self.max_slot_count,
            participating_slot_indices: self.participating_slot_indices,
            next_candidate: 0,
        }
        .advance(db, env)
    }
}

#[derive(Clone)]
struct CompareMaterializedArguments<'db> {
    candidates: Arc<[OverloadFilterCandidate<'db>]>,
    max_slot_count: usize,
    participating_slot_indices: HashSet<usize>,
    next_candidate: usize,
}

impl<'db> CompareMaterializedArguments<'db> {
    fn advance(
        mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> OverloadFilterStep<'db> {
        let Some(current_candidate) = self.candidates.get(self.next_candidate) else {
            return CompareReturnTypes {
                retained_count: self.candidates.len(),
                candidates: self.candidates,
                next_candidate: 1,
            }
            .advance();
        };

        let mut union_argument_type_builders =
            std::iter::repeat_with(|| UnionBuilder::new(db, env))
                .take(self.max_slot_count)
                .collect::<Vec<_>>();

        for candidate in &*self.candidates {
            for (slot_index, slot) in candidate.slots.iter().enumerate() {
                if self.participating_slot_indices.contains(&slot_index) {
                    let argument_type = slot.variadic_argument.unwrap_or_else(|| {
                        current_candidate
                            .slots
                            .get(slot_index)
                            .map_or(Type::unknown(), |slot| slot.argument)
                    });
                    union_argument_type_builders[slot_index]
                        .add_in_place(argument_type.top_materialization(db, env));
                }
            }
        }

        let top_materialized_argument_type = Type::heterogeneous_tuple(
            db,
            env,
            union_argument_type_builders
                .into_iter()
                .filter_map(|builder| {
                    if builder.is_empty() {
                        None
                    } else {
                        Some(builder.build())
                    }
                }),
        );

        let mut union_parameter_types = std::iter::repeat_with(|| UnionBuilder::new(db, env))
            .take(self.max_slot_count)
            .collect::<Vec<_>>();
        for candidate in &self.candidates[..=self.next_candidate] {
            for (slot_index, slot) in candidate.slots.iter().enumerate() {
                if self.participating_slot_indices.contains(&slot_index) {
                    union_parameter_types[slot_index].add_in_place(slot.parameter);
                }
            }
        }

        let parameter_types = Type::heterogeneous_tuple(
            db,
            env,
            union_parameter_types.into_iter().filter_map(|builder| {
                if builder.is_empty() {
                    None
                } else {
                    Some(builder.build())
                }
            }),
        );

        let condition = BinderCondition::Always(BinderComparison::Assignable {
            source: top_materialized_argument_type,
            target: parameter_types,
            inferable_typevars: current_candidate.inferable_typevars,
        });
        self.next_candidate += 1;
        OverloadFilterStep::Compare(PendingOverloadFilter {
            condition,
            continuation: OverloadFilterContinuation::MaterializedArguments(self),
        })
    }
}

#[derive(Clone)]
struct CompareReturnTypes<'db> {
    candidates: Arc<[OverloadFilterCandidate<'db>]>,
    retained_count: usize,
    next_candidate: usize,
}

impl<'db> CompareReturnTypes<'db> {
    fn advance(mut self) -> OverloadFilterStep<'db> {
        // Once filtering is complete, equivalent return types let step 6 select the first
        // remaining overload. A difference makes the call ambiguous.
        if self.next_candidate < self.retained_count
            && let Some(first) = self.candidates.first()
            && let Some(current) = self.candidates.get(self.next_candidate)
        {
            let condition = BinderCondition::Always(BinderComparison::Equivalent {
                left: current.return_type,
                right: first.return_type,
            });
            self.next_candidate += 1;
            return OverloadFilterStep::Compare(PendingOverloadFilter {
                condition,
                continuation: OverloadFilterContinuation::ReturnTypes(self),
            });
        }

        OverloadFilterStep::Complete(OverloadFilterResult {
            retained_count: self.retained_count,
            is_ambiguous: false,
        })
    }
}
