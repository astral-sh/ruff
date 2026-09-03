//! Operations deferred until the cycle variables they consume have values.
//!
//! An operation whose operand is a cycle marker cannot be evaluated while the marker's value is
//! still being inferred. The query records the operation against a variable of its own and uses
//! that variable's marker as the operation's result.

use ruff_python_ast::name::Name;
use rustc_hash::FxHashMap;
use ty_python_core::EvaluationMode;
use ty_python_core::frozen::FrozenMap;

use crate::types::cycle_variable::{CycleOwner, CycleSlot, CycleVariable};
use crate::types::tuple::TupleLength;
use crate::types::{MemberLookupPolicy, Type};
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

/// The operations one query deferred, keyed by the variable standing for each result.
pub(crate) type CycleEquations<'db> = FrozenMap<CycleVariable<'db>, Operation<'db>>;

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
    /// Materialized markers already behave like `object` or `Never` and are left as they are.
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
                let variable = CycleVariable::derived(db, self.owner, slot, marker.variable());
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
