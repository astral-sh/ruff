//! Candidate identity comparisons distinguish finite decisions from required source fields.

use crate::Db;
use crate::types::{
    FunctionType, NewType, ProtocolInstanceType, RecursiveType, Type, TypeAliasType, TypedDictType,
};

/// A candidate comparison decided by type equality/tags, or the fields still needed to decide it.
/// A positive candidate remains subject to the cycle detector's complete identity comparison.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum TypeIdentityCandidate<'db> {
    Decided(bool),
    Function(FunctionType<'db>, FunctionType<'db>),
    NewType(NewType<'db>, NewType<'db>),
    Protocol(ProtocolInstanceType<'db>, ProtocolInstanceType<'db>),
    Alias(TypeAliasType<'db>, TypeAliasType<'db>),
    TypedDict(TypedDictType<'db>, TypedDictType<'db>),
    Recursive(RecursiveType<'db>, RecursiveType<'db>),
}

impl<'db> TypeIdentityCandidate<'db> {
    /// Selects the ordinary candidate comparison without reading source fields.
    pub(in crate::types) fn new(left: Type<'db>, right: Type<'db>) -> Self {
        if left == right {
            return Self::Decided(true);
        }
        match (left, right) {
            (Type::FunctionLiteral(left), Type::FunctionLiteral(right)) => {
                Self::Function(left, right)
            }
            (Type::NewTypeInstance(left), Type::NewTypeInstance(right)) => {
                Self::NewType(left, right)
            }
            (Type::ProtocolInstance(left), Type::ProtocolInstance(right)) => {
                Self::Protocol(left, right)
            }
            (Type::TypeAlias(left), Type::TypeAlias(right)) => Self::Alias(left, right),
            (Type::TypedDict(left), Type::TypedDict(right)) => Self::TypedDict(left, right),
            (Type::Recursive(left), Type::Recursive(right)) => Self::Recursive(left, right),
            // Distinct variants, and unequal values outside these families, cannot share identity.
            _ => Self::Decided(false),
        }
    }

    /// Completes the selected comparison through the ordinary source operations.
    pub(in crate::types) fn resolve(self, db: &'db dyn Db) -> bool {
        match self {
            Self::Decided(value) => value,
            Self::Function(left, right) => left.literal(db) == right.literal(db),
            Self::NewType(left, right) => left.definition(db) == right.definition(db),
            Self::Protocol(left, right) => left.definition(db) == right.definition(db),
            Self::Alias(left, right) => left.definition(db) == right.definition(db),
            Self::TypedDict(left, right) => left.definition(db) == right.definition(db),
            Self::Recursive(left, right) => left.definition(db) == right.definition(db),
        }
    }

    /// Returns a decided candidate, leaving source-dependent comparisons unresolved.
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) const fn field_free(self) -> Option<bool> {
        match self {
            Self::Decided(value) => Some(value),
            Self::Function(..)
            | Self::NewType(..)
            | Self::Protocol(..)
            | Self::Alias(..)
            | Self::TypedDict(..)
            | Self::Recursive(..) => None,
        }
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) use admitted::{
    IDENTITY_MODE_ADMISSION_WORK, IDENTITY_MODE_WORK, admit_candidate_step, field_free_candidate,
    identity_mode_admission_bytes, identity_mode_bytes,
};

#[cfg(any(test, feature = "experimental-analysis"))]
mod admitted {
    use std::convert::Infallible;
    use std::ops::ControlFlow;

    use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};

    use super::{Type, TypeIdentityCandidate};
    use crate::types::cyclic::CycleIdentityMode;
    use crate::types::local_transfer::collections::{CALL_1, CALL_2};

    /// Work prepaid by ExactScan for optional-identity initialization and mode selection.
    /// Both modes perform these after an exact-key miss, even when the active stack is empty.
    // Four mode-selection events, two Option<Id> initialization events and two unit results.
    pub(in crate::types) const IDENTITY_MODE_WORK: usize = 8;

    /// Quotes transfers for initializing the optional identity and selecting its discovery mode.
    /// Call only in a constant initializer: this quote does not fund its own computation.
    /// `I` is the key's `HasIdentity::Id` type.
    pub(in crate::types) const fn identity_mode_bytes<I>() -> usize {
        4 * size_of::<CycleIdentityMode>() + 2 * size_of::<Option<I>>()
    }

    // Two endpoint calls and two Try::branch calls, plus separate bounds for both possible
    // residual/error conversions. The 32 direct events cover receiver access, argument/result
    // construction, branch selection, bindings and error returns. Refusal paths may overpay.
    pub(in crate::types) const IDENTITY_MODE_ADMISSION_WORK: usize = 2 * CALL_2 + 6 * CALL_1 + 32;

    /// Quotes the transfers in two supplemental work/resource admissions, including refusal.
    /// Call only in a constant initializer: this quote does not fund its own computation.
    /// `R` is the enclosing adapter's complete result type.
    pub(in crate::types) const fn identity_mode_admission_bytes<R>() -> usize {
        let widths = [
            size_of::<(&TaskEndpoint<'_, '_>, usize)>(),
            size_of::<(&TaskEndpoint<'_, '_>, ExecutionWork)>(),
            size_of::<RunResult<()>>(),
            size_of::<R>(),
            size_of::<ControlFlow<Result<Infallible, RunError>, ()>>(),
            size_of::<usize>(),
        ];
        let mut width = 0;
        let mut index = 0;
        while index < widths.len() {
            if widths[index] > width {
                width = widths[index];
            }
            index += 1;
        }
        IDENTITY_MODE_ADMISSION_WORK * width
    }

    /// Admits the scan callback's fixed key copies and component/mode bookkeeping.
    /// Each reached component separately admits its classification and result carriers.
    /// `R` is the callback's result type, including its error, for result construction and returns.
    pub(in crate::types) fn admit_candidate_step<K, R>(
        endpoint: &TaskEndpoint<'_, '_>,
    ) -> RunResult<()> {
        endpoint.admit_work(16)?;
        endpoint.admit(ExecutionWork::Resource {
            requested_bytes: size_of::<K>() * 2 + size_of::<&Type<'_>>() * 4 + size_of::<R>() * 3,
        })?;
        endpoint.check_completion()
    }

    /// Admits and classifies one candidate component without performing its source reads.
    /// The caller invokes this separately for source and target, preserving short-circuit order.
    pub(in crate::types) fn field_free_candidate<'db>(
        endpoint: &TaskEndpoint<'_, 'db>,
        left: &Type<'db>,
        right: &Type<'db>,
    ) -> RunResult<Option<bool>> {
        // This fixed bound covers argument copies, equality/tag decisions, candidate and result
        // construction/returns, and their retirement. Representation widths contribute bytes only.
        endpoint.admit_work(32)?;
        endpoint.admit(ExecutionWork::Resource {
            requested_bytes: size_of::<Type<'db>>() * 4
                + size_of::<TypeIdentityCandidate<'db>>() * 2
                + size_of::<Option<bool>>() * 2
                + size_of::<RunResult<Option<bool>>>() * 2,
        })?;
        endpoint.check_completion()?;
        Ok(TypeIdentityCandidate::new(*left, *right).field_free())
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_python_ast::PythonVersion;
    use ty_python_core::ProgramFile;

    use super::*;
    use crate::db::tests::TestDbBuilder;
    use crate::place::global_symbol;
    use crate::types::KnownInstanceType;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Family {
        Function,
        NewType,
        Protocol,
        Alias,
        TypedDict,
        Recursive,
    }

    // A mdtest cannot observe whether candidate classification requires a source read. Each
    // family keeps unequal handles unresolved while equality and distinct variants need no fields.
    #[test_case::test_case(Family::Function; "function")]
    #[test_case::test_case(Family::NewType; "newtype")]
    #[test_case::test_case(Family::Protocol; "protocol")]
    #[test_case::test_case(Family::Alias; "alias")]
    #[test_case::test_case(Family::TypedDict; "typed dict")]
    #[test_case::test_case(Family::Recursive; "recursive")]
    fn candidate_families_preserve_required_fields(family: Family) -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file(
                "/src/identity.py",
                "from typing import NewType, Protocol, TypedDict\ndef f(): ...\ndef g(): ...\nN1 = NewType('N1', int)\nN2 = NewType('N2', int)\nn1: N1\nn2: N2\nclass P1(Protocol):\n    value: int\nclass P2(Protocol):\n    value: str\np1: P1\np2: P2\nclass D1(TypedDict):\n    value: int\nclass D2(TypedDict):\n    value: str\nd1: D1\nd2: D2\ntype A1 = int\ntype A2 = str\nR1 = tuple[int, \"R1 | None\"]\nR2 = tuple[str, \"R2 | None\"]\nr1: R1\nr2: R2\n",
            )
            .build()?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/identity.py")?,
            env.program(&db),
        );
        let symbol = |name| global_symbol(&db, file, name).place.expect_type();
        let (left, right) = match family {
            Family::Function => (symbol("f"), symbol("g")),
            Family::NewType => (symbol("n1"), symbol("n2")),
            Family::Protocol => (symbol("p1"), symbol("p2")),
            Family::TypedDict => (symbol("d1"), symbol("d2")),
            Family::Recursive => {
                let (left, right) = (symbol("r1"), symbol("r2"));
                assert!(matches!(left, Type::Recursive(_)));
                assert!(matches!(right, Type::Recursive(_)));
                (left, right)
            }
            Family::Alias => {
                let Type::KnownInstance(KnownInstanceType::TypeAliasType(left)) = symbol("A1")
                else {
                    anyhow::bail!("expected the first alias definition");
                };
                let Type::KnownInstance(KnownInstanceType::TypeAliasType(right)) = symbol("A2")
                else {
                    anyhow::bail!("expected the second alias definition");
                };
                (Type::TypeAlias(left), Type::TypeAlias(right))
            }
        };
        assert_ne!(left, right);
        assert_eq!(TypeIdentityCandidate::new(left, right).field_free(), None);
        assert!(!left.may_share_type_identity(&db, right));
        assert_eq!(
            TypeIdentityCandidate::new(left, left).field_free(),
            Some(true)
        );
        assert!(left.may_share_type_identity(&db, left));
        assert_eq!(
            TypeIdentityCandidate::new(left, Type::object()).field_free(),
            Some(false)
        );
        assert!(!left.may_share_type_identity(&db, Type::object()));
        Ok(())
    }
}
