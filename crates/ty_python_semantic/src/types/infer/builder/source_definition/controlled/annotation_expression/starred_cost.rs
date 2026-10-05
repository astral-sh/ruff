//! Fixed source-event bounds for starred type-expression decisions and flag state.

use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::ExpressionNodeKey;

use crate::types::infer::{InferenceFlags, TypeExpressionFlags};
use crate::types::infer::builder::type_expression::{TypeExpressionPending, TypeExpressionRequest, TypeExpressionStep};
use crate::types::storage_quote::StorageQuote;
use crate::types::{BoundTypeVarInstance, Type, TypeVarKind};

const CALL_1: usize = 3 + 2;
const CALL_2: usize = 6 + 2;
const CALL_3: usize = 9 + 2;
const CALL_4: usize = 12 + 2;
const CHECKED_ADD: usize = 3 * CALL_2 + CALL_1 + 6;
const CHECKED_MUL: usize = 3 * CALL_2 + CALL_1 + 26;
const OPTION_PROPAGATION: usize = CALL_1 + 11;
const FLAG_CONTAINS: usize = 2 * CALL_2 + 6;
const FLAG_SET: usize = 2 * CALL_3 + 2 * CALL_2 + 13;
const FLAG_INSERT: usize = 3 * CALL_2 + 9;

/// A finite starred-expression operation whose child computations have separate admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Operation {
    Phase,
    MapPreparation,
    MapInsertion,
    Enter,
    Restore,
    ExactTuple,
    TypeVariable,
    Kind,
    Return,
    Child,
}

/// Quotes one fixed operation and its actual argument/result representation envelope.
///
/// Call this in an inline const block so preparing the fixed quotation performs no runtime work.
pub(super) const fn quote<'db>(operation: Operation) -> RunResult<(usize, usize)> {
    let work = match operation {
        Operation::Phase => CALL_4 + 6 * CALL_4 + 4 * CALL_3 + 2 * CALL_1 + 56,
        Operation::MapPreparation => 8 * CALL_1 + 13 + 3 * CHECKED_ADD + CHECKED_MUL
            + 6 * OPTION_PROPAGATION + 2 * CALL_2 + CALL_1 + 40,
        Operation::MapInsertion => CALL_3 + 10 * CALL_1 + 6 * CALL_2 + FLAG_INSERT + 24,
        Operation::Enter => CALL_2 + CALL_3 + FLAG_CONTAINS + FLAG_SET + 10,
        Operation::Restore => 2 * CALL_2 + FLAG_SET + 8,
        Operation::ExactTuple => 4 * CALL_1 + CALL_2 + 20,
        Operation::TypeVariable => 2 * CALL_1 + 8,
        Operation::Kind => 2 * CALL_1 + 8,
        Operation::Return => CALL_1 + 4,
        Operation::Child => 2 * CALL_4 + 2 * CALL_1 + 12,
    };
    let representations = [
        size_of::<TypeExpressionPending<'db, 'static>>(),
        size_of::<TypeExpressionRequest<'db, 'static>>(),
        size_of::<TypeExpressionStep<'db, 'static>>(),
        size_of::<RunResult<TypeExpressionStep<'db, 'static>>>(),
        size_of::<(Type<'db>, Option<BoundTypeVarInstance<'db>>, TypeVarKind)>(),
        size_of::<(InferenceFlags, TypeExpressionFlags, bool)>(),
        size_of::<Option<(StorageQuote, usize)>>(),
        size_of::<RunResult<(usize, usize)>>(),
        size_of::<(ExpressionNodeKey, TypeExpressionFlags)>(),
        size_of::<(usize, bool)>(),
        size_of::<Option<usize>>(),
        size_of::<(*mut (), *mut (), *mut (), *mut ())>(),
    ];
    let mut width = 0;
    let mut index = 0;
    while index < representations.len() {
        if representations[index] > width {
            width = representations[index];
        }
        index += 1;
    }
    match work.checked_mul(width) {
        Some(bytes) => Ok((work, bytes)),
        None => Err(RunError::Contract("starred operation quotation overflow")),
    }
}
