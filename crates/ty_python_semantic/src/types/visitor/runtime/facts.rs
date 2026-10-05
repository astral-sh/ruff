//! Finite stored-value operations between the shared walk's effect calls.

use salsa::execution_probe::RunResult;

use crate::types::constraints::{ConstraintId, OwnedConstraintTypeCursor};
use crate::types::local_transfer::hash_table;
use crate::types::local_transfer::collections::{
    CALL_1, CALL_2, NONNULL_AS_PTR, NONNULL_NEW_UNCHECKED_WORK, POINTER_ACCESS,
    POINTER_ADD_WORK, FixedQuote, add_quotes, checked, event_quote, fixed_quote,
};
use crate::types::visitor::{NonAtomicType, StoredTypeSequence, TypeWalkEvent, TypeWalkPolicy, WalkAction};
use crate::types::visitor::search::{TypeSearchDecision, TypeSearchDescent};
use crate::types::{CallableType, Parameter, Signature, Specialization, Type};

const SPLIT_FIRST: usize = CALL_1 + 8;
const ARC_DEREF: usize = 4 * CALL_1 + NONNULL_AS_PTR + 8;
const OPTION_BORROW: usize = 2 * CALL_1 + CALL_2 + 10;
const PARAMETERS: usize = 3 * CALL_1 + ARC_DEREF + 7;
const RECEIVER_BORROW: usize = 3 * CALL_1 + 2 * CALL_2 + OPTION_BORROW + 18;
const RECEIVER_NEW: usize = CALL_1 + OPTION_BORROW + ARC_DEREF + 10;

/// Quotes one search visit's finite decision and the resulting decision's return transfers.
/// The runtime adapter separately admits its unboxed future through the local-transfer helper.
pub(super) const fn search_decision_quote<T>() -> RunResult<(usize, usize)> {
    // Two has_result calls include their default and comparison operations. The remaining
    // calls cover TypeKind conversion; branches, bindings and constructors take at most 44 events.
    // Runtime results are bool or an optional interned key, whose comparison with None is finite.
    let decisions = 5 * CALL_2 + 3 * CALL_1 + 44;
    checked(add_quotes(event_quote(decisions, &[
        size_of::<T>(), size_of::<Type<'_>>(), size_of::<TypeWalkPolicy>(),
        size_of::<Option<Option<Specialization<'_>>>>(), size_of::<NonAtomicType<'_>>(),
        size_of::<TypeSearchDescent<'_>>(), size_of::<TypeSearchDecision<'_, T>>(),
        size_of::<bool>(),
    ]), fixed_quote(4, [
        // The shared return, adapter await and return, and caller await each forward the result.
        (4, size_of::<RunResult<TypeSearchDecision<'_, T>>>()),
    ])))
}

/// Quotes the scheduling decision and construction of its selected pending action.
/// Remembering, pending storage, and their cleanup are admitted by their existing operations.
pub(super) const fn search_descent_quote() -> RunResult<(usize, usize)> {
    checked(add_quotes(event_quote(3 * CALL_2 + 28, &[
        size_of::<TypeSearchDescent<'_>>(), size_of::<Type<'_>>(),
        size_of::<NonAtomicType<'_>>(), size_of::<Option<Specialization<'_>>>(),
        size_of::<bool>(),
    ]), fixed_quote(5, [
        (1, size_of::<WalkAction<'_>>()),
        (4, size_of::<RunResult<()>>()),
    ])))
}

/// Quotes facts and frame transfers for the action that was removed from the pending stack.
/// Receiver construction is charged only for a nonempty signature frame with receiver constraints.
pub(super) fn frame_quote(action: Option<&WalkAction<'_>>) -> RunResult<(usize, usize)> {
    match action {
        None => checked(const { event_quote(6, &[size_of::<Option<TypeWalkEvent<'_>>>()]) }),
        Some(WalkAction::Signatures(signatures)) => {
            let Some(signature) = signatures.first() else {
                return checked(const { common_frame_quote() });
            };
            if signature.receiver_constraints().is_some() {
                checked(const { signature_frame_quote(true) })
            } else {
                checked(const { signature_frame_quote(false) })
            }
        }
        Some(WalkAction::ConstraintTypes(_)) => checked(const {
            add_quotes(common_frame_quote(), fixed_quote(0, [
                (4, size_of::<OwnedConstraintTypeCursor<'_, '_>>()),
            ]))
        }),
        Some(WalkAction::SkippedLazy | WalkAction::EndScope | WalkAction::Visit(_)
            | WalkAction::Expand(_) | WalkAction::ExitDepth { .. } | WalkAction::Types(_)
            | WalkAction::StoredTypes(_)
            | WalkAction::Parameters(_) | WalkAction::GenericContext { .. }
            | WalkAction::SpecializationTypes(_) | WalkAction::TypeVarBounds(_)
            | WalkAction::TypeVarDefault(_) | WalkAction::FunctionImplementations(_)
            | WalkAction::Callables(_) | WalkAction::TypeAliasValue(_)
            | WalkAction::ProtocolInterface(_) | WalkAction::ProtocolMembers { .. }
            | WalkAction::ProtocolMember { .. } | WalkAction::TypedDictFields(_)
            | WalkAction::TypedDictExtra(_) | WalkAction::NewTypeBase(_)
            | WalkAction::FieldConverter(_)) => checked(const { common_frame_quote() }),
    }
}

/// Quotes selecting a frame quotation from an already removed, still retained action.
pub(super) const fn frame_preparation_quote() -> RunResult<(usize, usize)> {
    checked(const { event_quote(4 * CALL_1 + SPLIT_FIRST + RECEIVER_BORROW + 14, &[
        size_of::<Option<&WalkAction<'_>>>(), size_of::<&Signature<'_>>(),
        size_of::<RunResult<(usize, usize)>>(), size_of::<bool>(),
    ]) })
}

const fn common_frame_quote() -> FixedQuote {
    frame_fields_quote(4 * CALL_2 + 5 * CALL_1 + 2 * SPLIT_FIRST + 64)
}

const fn frame_fields_quote(fields: usize) -> FixedQuote {
    add_quotes(event_quote(fields, &[
        size_of::<(&Signature<'_>, &[Signature<'_>])>(),
        size_of::<(&Parameter<'_>, &[Parameter<'_>])>(),
        size_of::<(Type<'_>, &[Type<'_>])>(), size_of::<Option<Type<'_>>>(),
        size_of::<Option<&Signature<'_>>>(), size_of::<TypeWalkPolicy>(),
        size_of::<usize>(), size_of::<bool>(),
    ]), fixed_quote(0, [
        (6, size_of::<WalkAction<'_>>()), (4, size_of::<Option<TypeWalkEvent<'_>>>()),
    ]))
}

const fn signature_frame_quote(receiver: bool) -> FixedQuote {
    let fields = add_quotes(frame_fields_quote(4 * CALL_2 + 5 * CALL_1 + 2 * SPLIT_FIRST
        + PARAMETERS + RECEIVER_BORROW + 64), fixed_quote(0, [
        (4, size_of::<Option<OwnedConstraintTypeCursor<'_, '_>>>()),
    ]));
    if !receiver { return fields; }
    let empty_receiver = match hash_table::empty_quote::<ConstraintId, ()>() {
        Ok(quote) => Some(quote), Err(_) => None,
    };
    add_quotes(add_quotes(fields, empty_receiver), add_quotes(event_quote(RECEIVER_NEW, &[
        size_of::<(&Signature<'_>, &[Signature<'_>])>(),
        size_of::<(&Parameter<'_>, &[Parameter<'_>])>(),
        size_of::<(Type<'_>, &[Type<'_>])>(), size_of::<Option<Type<'_>>>(),
        size_of::<Option<&Signature<'_>>>(), size_of::<TypeWalkPolicy>(),
        size_of::<usize>(), size_of::<bool>(),
    ]), fixed_quote(0, [
        (4, size_of::<OwnedConstraintTypeCursor<'_, '_>>()),
    ])))
}

/// Quotes fixed/variable tuple inspection and the resulting borrowed frame arguments.
pub(super) const fn tuple_quote() -> RunResult<(usize, usize)> {
    checked(const {
        // prefix/suffix use bounded range indexing into one boxed slice, without copying it.
        let range = 3 * CALL_2 + CALL_1 + POINTER_ADD_WORK + 18;
        event_quote(7 * CALL_1 + 2 * CALL_2 + 2 * range + 28, &[
            size_of::<Option<(&[Type<'_>], Type<'_>, &[Type<'_>])>>(),
            size_of::<&[Type<'_>]>(), size_of::<Type<'_>>(), size_of::<usize>(),
        ])
    })
}

/// Quotes finite type-expansion decisions without constructing stored iterators.
pub(super) const fn expansion_quote() -> RunResult<(usize, usize)> {
    checked(const { expansion_fields_quote(12 * CALL_1 + 8 * CALL_2 + 72) })
}

/// Quotes the selected expansion branch, including ordered iterators only when it creates them.
pub(super) fn children_quote(kind: NonAtomicType<'_>) -> RunResult<(usize, usize)> {
    match kind {
        NonAtomicType::Intersection(_) => const { checked(iterator_expansion_quote(2)) },
        NonAtomicType::EnumComplement(_) | NonAtomicType::Union(_)
        | NonAtomicType::FunctionLiteral(_) | NonAtomicType::Callable(_)
        | NonAtomicType::BoundMethod(_) | NonAtomicType::BoundSuper(_) | NonAtomicType::MethodWrapper(_)
        | NonAtomicType::GenericAlias(_) | NonAtomicType::KnownInstance(_)
        | NonAtomicType::SubclassOf(_) | NonAtomicType::NominalInstance(_)
        | NonAtomicType::PropertyInstance(_) | NonAtomicType::SlotDescriptor(_)
        | NonAtomicType::TypeIs(_) | NonAtomicType::TypeGuard(_) | NonAtomicType::TypeForm(_)
        | NonAtomicType::TypeVar(_) | NonAtomicType::ProtocolInstance(_)
        | NonAtomicType::TypedDict(_) | NonAtomicType::TypeAlias(_) | NonAtomicType::Recursive(_)
        | NonAtomicType::NewTypeInstance(_) => const { expansion_quote() },
    }
}

/// Quotes inspecting an expansion tag to select its fixed quotation.
pub(super) const fn expansion_preparation_quote() -> RunResult<(usize, usize)> {
    checked(const { event_quote(CALL_1 + 4, &[
        size_of::<NonAtomicType<'_>>(), size_of::<RunResult<(usize, usize)>>(),
    ]) })
}

const fn iterator_expansion_quote(count: usize) -> FixedQuote {
        let slice_iter = 3 * CALL_1 + NONNULL_NEW_UNCHECKED_WORK
            + NONNULL_AS_PTR + POINTER_ADD_WORK + 12;
        let ordered = 7 * CALL_1 + POINTER_ACCESS + slice_iter + 14;
        expansion_fields_quote(count * ordered + 12 * CALL_1 + 8 * CALL_2 + 72)
}

const fn expansion_fields_quote(work: usize) -> FixedQuote {
        event_quote(work, &[
            size_of::<StoredTypeSequence<'_>>(), size_of::<Option<Type<'_>>>(),
            size_of::<&[Type<'_>]>(), size_of::<&[CallableType<'_>]>(),
            size_of::<TypeWalkPolicy>(), size_of::<usize>(), size_of::<bool>(),
        ])
}
