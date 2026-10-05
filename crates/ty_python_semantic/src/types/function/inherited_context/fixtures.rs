//! Canonical stored inputs for internal function-context controls; no semantic queries run here.

use super::updated_signatures;
use crate::Db;
use crate::types::callable::{CallableType, CallableTypeKind};
use crate::types::function::{FunctionLiteral, FunctionType, OverloadLiteral};
use crate::types::signatures::CallableSignature;

/// Selects the constructed literal's separate-implementation flag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum LiteralKind {
    Single,
    SeparateImplementation,
}

/// Constructs canonical metadata with an updated public signature and exact implementation storage.
/// The supplied last definition is real source syntax; its signature remains unevaluated here.
/// The literal flag is constructed explicitly and does not establish an overloaded declaration.
pub(in crate::types) fn function<'db>(
    db: &'db dyn Db,
    last_definition: OverloadLiteral<'db>,
    kind: LiteralKind,
    signature: CallableSignature<'db>,
    implementations: Option<Box<[CallableType<'db>]>>,
    descriptor: Option<CallableTypeKind>,
) -> FunctionType<'db> {
    let literal = FunctionLiteral {
        last_definition,
        overloaded: match kind {
            LiteralKind::Single => false,
            LiteralKind::SeparateImplementation => true,
        },
    };
    FunctionType::new_internal(
        db,
        literal,
        updated_signatures(signature, implementations),
        descriptor,
    )
}
