//! Operations deferred until the cycle variables they consume have values.
//!
//! An operation whose operand is a cycle marker cannot be evaluated while the marker's value is
//! still being inferred. The query records the operation against a variable of its own and uses
//! that variable's marker as the operation's result.

use ruff_python_ast::name::Name;
use rustc_hash::{FxHashMap, FxHashSet};
use ty_python_core::EvaluationMode;
use ty_python_core::frozen::FrozenMap;

use crate::types::cycle_variable::{CycleOwner, CycleSlot, CycleVariable};
use crate::types::tuple::TupleLength;
use crate::types::{MemberLookupPolicy, Type, UnionType};
use crate::{Db, ProgramEnvironment};

/// An operation whose result is a cycle variable of the query that recorded it.
///
/// Inputs are ordinary types that may contain cycle markers.
#[derive(Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum Operation<'db> {
    Subscript {
        value: Type<'db>,
        key: Type<'db>,
    },
    Member {
        value: Type<'db>,
        name: Name,
        policy: MemberLookupPolicy,
    },
    Iterate {
        value: Type<'db>,
        mode: EvaluationMode,
    },
    /// One target of an unpacking assignment with `length` targets.
    Unpack {
        value: Type<'db>,
        length: TupleLength,
        index: usize,
    },
    Enter {
        value: Type<'db>,
        mode: EvaluationMode,
    },
    Await {
        value: Type<'db>,
    },
}

impl<'db> Operation<'db> {
    /// The types the operation consumes.
    pub(crate) fn inputs(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        match self {
            Self::Subscript { value, key } => [Some(*value), Some(*key)],
            Self::Member { value, .. }
            | Self::Iterate { value, .. }
            | Self::Unpack { value, .. }
            | Self::Enter { value, .. }
            | Self::Await { value } => [Some(*value), None],
        }
        .into_iter()
        .flatten()
    }

    /// Replaces every type the operation consumes, or returns `None` if `f` rejects one.
    pub(crate) fn map_inputs(
        &self,
        mut f: impl FnMut(Type<'db>) -> Option<Type<'db>>,
    ) -> Option<Self> {
        Some(match self {
            Self::Subscript { value, key } => Self::Subscript {
                value: f(*value)?,
                key: f(*key)?,
            },
            Self::Member {
                value,
                name,
                policy,
            } => Self::Member {
                value: f(*value)?,
                name: name.clone(),
                policy: *policy,
            },
            Self::Iterate { value, mode } => Self::Iterate {
                value: f(*value)?,
                mode: *mode,
            },
            Self::Unpack {
                value,
                length,
                index,
            } => Self::Unpack {
                value: f(*value)?,
                length: *length,
                index: *index,
            },
            Self::Enter { value, mode } => Self::Enter {
                value: f(*value)?,
                mode: *mode,
            },
            Self::Await { value } => Self::Await { value: f(*value)? },
        })
    }

    /// Combines the inputs recorded for the same variable by two cycle iterations.
    fn cycle_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: &Self,
    ) -> Self {
        let join = |current: Type<'db>, previous: Type<'db>| {
            if current == previous {
                current
            } else {
                UnionType::from_elements_cycle_recovery(db, env, [previous, current])
            }
        };
        match (self, previous) {
            (
                Self::Subscript { value, key },
                Self::Subscript {
                    value: previous_value,
                    key: previous_key,
                },
            ) => Self::Subscript {
                value: join(value, *previous_value),
                key: join(key, *previous_key),
            },
            (
                Self::Member {
                    value,
                    name,
                    policy,
                },
                Self::Member {
                    value: previous_value,
                    ..
                },
            ) => Self::Member {
                value: join(value, *previous_value),
                name,
                policy,
            },
            (
                Self::Iterate { value, mode },
                Self::Iterate {
                    value: previous_value,
                    ..
                },
            ) => Self::Iterate {
                value: join(value, *previous_value),
                mode,
            },
            (
                Self::Unpack {
                    value,
                    length,
                    index,
                },
                Self::Unpack {
                    value: previous_value,
                    ..
                },
            ) => Self::Unpack {
                value: join(value, *previous_value),
                length,
                index,
            },
            (
                Self::Enter { value, mode },
                Self::Enter {
                    value: previous_value,
                    ..
                },
            ) => Self::Enter {
                value: join(value, *previous_value),
                mode,
            },
            (
                Self::Await { value },
                Self::Await {
                    value: previous_value,
                },
            ) => Self::Await {
                value: join(value, *previous_value),
            },
            (current, _) => current,
        }
    }
}

/// Replaces every unmaterialized marker at the top level of `result` that is also a top-level
/// element of one of `operands` by the opaque marker of its cycle head.
///
/// An operation that passes a marker through unchanged without recording an equation leaves the
/// marker naming the operand rather than the result. The opaque marker names neither, so the
/// solver leaves it alone and cycle recovery treats it like the head's own marker.
pub(crate) fn opaque_passthrough<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    operands: impl IntoIterator<Item = Type<'db>>,
    result: Type<'db>,
) -> Type<'db> {
    let mut passed = FxHashSet::default();
    for operand in operands {
        let single = [operand];
        let elements: &[Type<'db>] = match operand {
            Type::Union(union) => union.elements(db),
            _ => &single,
        };
        for element in elements {
            if let Type::Divergent(marker) = element
                && marker.materialization_kind().is_none()
                && let Some(variable) = marker.variable()
            {
                passed.insert(variable);
            }
        }
    }
    if passed.is_empty() {
        return result;
    }
    let replace = |ty: Type<'db>| match ty {
        Type::Divergent(marker)
            if marker.materialization_kind().is_none()
                && marker
                    .variable()
                    .is_some_and(|variable| passed.contains(&variable)) =>
        {
            Type::Divergent(marker.opaque(db))
        }
        _ => ty,
    };
    match result {
        Type::Union(union) => union.map(db, env, |element| replace(*element)),
        _ => replace(result),
    }
}

/// The operations one query deferred, keyed by the variable standing for each result.
pub(crate) type CycleEquations<'db> = FrozenMap<CycleVariable<'db>, Operation<'db>>;

/// Retains the operations recorded by both cycle iterations.
///
/// An iteration can leave out an operation that an earlier iteration recorded, and the inputs of
/// an operation can change between iterations. Keeping every operation and joining changed inputs
/// makes the table grow monotonically, so the fixed-point iteration converges.
pub(crate) fn cycle_normalized_equations<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    current: CycleEquations<'db>,
    previous: &CycleEquations<'db>,
) -> CycleEquations<'db> {
    if previous.is_empty() {
        return current;
    }
    let mut merged: FxHashMap<_, _> = previous.iter().cloned().collect();
    for (variable, operation) in current {
        let operation = match merged.get(&variable) {
            Some(previous_operation) => operation.cycle_normalized(db, env, previous_operation),
            None => operation,
        };
        merged.insert(variable, operation);
    }
    FrozenMap::from(merged)
}

/// Collects the operations a query defers while it infers types.
#[derive(Debug)]
pub(crate) struct DeferredOperations<'db> {
    owner: CycleOwner<'db>,
    equations: FxHashMap<CycleVariable<'db>, Operation<'db>>,
}

impl<'db> DeferredOperations<'db> {
    pub(crate) fn new(owner: CycleOwner<'db>) -> Self {
        Self {
            owner,
            equations: FxHashMap::default(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.equations.is_empty()
    }

    /// The operations recorded so far.
    pub(crate) fn equations(&self) -> &FxHashMap<CycleVariable<'db>, Operation<'db>> {
        &self.equations
    }

    pub(crate) fn extend(
        &mut self,
        equations: impl IntoIterator<Item = (CycleVariable<'db>, Operation<'db>)>,
    ) {
        self.equations.extend(equations);
    }

    pub(crate) fn finish(self) -> CycleEquations<'db> {
        FrozenMap::from(self.equations)
    }

    /// Replaces every cycle marker that an operation passed through unchanged from `operand` to
    /// `result` with the marker of a new variable, defined by the operation applied to that
    /// marker alone.
    ///
    /// Materialized markers already behave like `object` or `Never`, and markers that belong to
    /// no cycle head cannot be resolved; both are left as they are.
    pub(crate) fn defer_passthrough(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        operand: Type<'db>,
        result: Type<'db>,
        slot: CycleSlot,
        operation: impl Fn(Type<'db>) -> Operation<'db>,
    ) -> Type<'db> {
        let single = [operand];
        let operand_elements: &[Type<'db>] = match operand {
            Type::Union(union) => union.elements(db),
            _ => &single,
        };
        let mut replace = |ty: Type<'db>| match ty {
            Type::Divergent(marker)
                if marker.materialization_kind().is_none() && operand_elements.contains(&ty) =>
            {
                let Some(input) = marker.variable() else {
                    return ty;
                };
                let variable = CycleVariable::derived(db, self.owner, slot, input);
                self.equations.insert(variable, operation(ty));
                Type::divergent_variable(variable)
            }
            _ => ty,
        };
        match result {
            Type::Union(union) => union.map(db, env, |element| replace(*element)),
            _ => replace(result),
        }
    }
}

impl<'db> IntoIterator for DeferredOperations<'db> {
    type Item = (CycleVariable<'db>, Operation<'db>);
    type IntoIter = std::collections::hash_map::IntoIter<CycleVariable<'db>, Operation<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.equations.into_iter()
    }
}
