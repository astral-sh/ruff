//! Owned receiver constraints preserve canonical query identity, solver ownership, and admission.
//! These internal controls observe runtime boundaries that Python mdtests cannot inspect.

use std::borrow::Cow;
use std::panic::AssertUnwindSafe;

use ruff_python_ast::name::Name;
use salsa::plumbing::ZalsaDatabase;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::FxOrderSet;
use crate::analysis::{ConstructorSignatureOperation, RelationOperation};
use crate::types::constraints::source::SourceStructuralResult;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder, OwnedConstraintSet};
use crate::types::relation::source::owned_constraints::package_terminal;
use crate::types::relation::source::receiver_constraint_observations::{
    self as receiver_observations, Stage,
};
use crate::types::relation::source::resources::RelationResourceAccess;
use crate::types::relation::source::resources::observations as relation_observations;
use crate::types::relation::source::retained::observations as child_observations;
use crate::types::relation::{TypeRelation, TypeVarEvaluation, owned_assignability_ingredient};
use crate::types::set_theoretic::{NegativeIntersectionElements, RecursivelyDefined};
use crate::types::signatures::constructor_preparation::{
    ConstructorSignatureEffects, InlineConstructorSignatureEffects,
};
use crate::types::{
    BoundTypeVarInstance, IntersectionType, TypeFormType, TypePair, TypeVarVariance, UnionType,
    legacy_inline,
};

const SOURCE: &str = "class Base: pass\nclass Product(Base): pass\nclass Other: pass\n";

/// Retains syntax-derived class definitions without inferring any class type.
#[derive(Clone, Copy, Debug)]
struct Definitions<'db> {
    base: Definition<'db>,
    product: Definition<'db>,
    other: Definition<'db>,
}

impl<'db> Definitions<'db> {
    fn new(prepared: &PreparedAnalysisFile<'db>) -> Self {
        let class = |name| {
            let class = prepared
                .parsed_module()
                .syntax()
                .body
                .iter()
                .filter_map(Stmt::as_class_def_stmt)
                .find(|class| class.name.as_str() == name)
                .expect("receiver-constraint fixture class");
            prepared.semantic_index().expect_single_definition(class)
        };
        Self {
            base: class("Base"),
            product: class("Product"),
            other: class("Other"),
        }
    }
}

/// Gets a nominal instance using controlled inference and the original instance constructor.
async fn instance<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    access: &A,
    program: Program<'db>,
    env: &ProgramEnvironment<'db>,
    definition: Definition<'db>,
) -> RunResult<Type<'db>> {
    let inference = access.definition(definition).await?;
    let effects = SourceEffects::new(access, program);
    let class = effects
        .local_with_fixed_transfers(3, 0, || {
            inference
                .original_class_type(definition)
                .ok_or(RunError::Contract(
                    "receiver-constraint fixture is not a class",
                ))
        })
        .await??;
    Type::instance_with(access.db(), env, &effects, ClassType::NonGeneric(class)).await
}

/// Selects nominal pairs whose ordinary owned result is terminal true or terminal false.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NominalCase {
    Subclass,
    Unrelated,
}

impl NominalCase {
    const fn expected(self) -> bool {
        match self {
            Self::Subclass => true,
            Self::Unrelated => false,
        }
    }
}

/// Distinguishes cold source-derived types from existing types supplied by a focused control.
#[derive(Clone, Copy, Debug)]
enum Inputs<'db> {
    Cold(Definitions<'db>, NominalCase),
    Existing(Type<'db>, Type<'db>),
}

/// Retains the exact operands and full owned constraints for ordinary parity checks.
#[derive(Debug)]
struct PairResult<'db> {
    source: Type<'db>,
    target: Type<'db>,
    constraints: Cow<'db, OwnedConstraintSet<'db>>,
}

/// Retains requested operand identities even if the owned query refuses or is cancelled.
#[derive(Clone, Copy, Debug)]
struct Operands<'db> {
    source: Type<'db>,
    target: Type<'db>,
}

/// Runs the original constructor receiver-constraint callback with the selected operands.
#[derive(Clone, Copy, Debug)]
struct Request<'db>(Inputs<'db>);

impl<'db> Request<'db> {
    /// Obtains the operands, runs the receiver-constraint callback, and returns both with its result.
    /// Arms optional Pending cancellation only after the operand setup has finished.
    async fn execute<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
        cancel_at_pending: Option<usize>,
        observed: Option<&Cell<Option<Operands<'db>>>>,
    ) -> RunResult<PairResult<'db>>
    where
        'db: 'run,
    {
        let db = access.db();
        let env = ProgramEnvironment::from_program(program);
        let effects = SourceEffects::new(access, program);
        let (source, target) = match self.0 {
            Inputs::Cold(definitions, case) => {
                let source = instance(access, program, &env, definitions.product).await?;
                let definition = match case {
                    NominalCase::Subclass => definitions.base,
                    NominalCase::Unrelated => definitions.other,
                };
                (source, instance(access, program, &env, definition).await?)
            }
            Inputs::Existing(source, target) => (source, target),
        };
        if let Some(observed) = observed {
            observed.set(Some(Operands { source, target }));
        }
        relation_observations::reset_assignability();
        if let Some(child) = cancel_at_pending {
            child_observations::reset(None);
            child_observations::set_cancel_at_pending(Some(child));
        }
        let constraints =
            ConstructorSignatureEffects::receiver_constraint(&effects, db, &env, source, target)
                .await?;
        Ok(PairResult {
            source,
            target,
            constraints,
        })
    }
}

impl<'db> MemberOperation<'db> for Request<'db> {
    type Output = PairResult<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        self.execute(access, program, None, None).await
    }
}

/// Cancels the actual first Pending of a retained child belonging to this owned query.
#[derive(Clone, Copy, Debug)]
struct CancellationRequest<'a, 'db> {
    request: Request<'db>,
    operands: &'a Cell<Option<Operands<'db>>>,
}

impl<'db> MemberOperation<'db> for CancellationRequest<'_, 'db> {
    type Output = PairResult<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        self.request
            .execute(access, program, Some(1), Some(self.operands))
            .await
    }
}

/// Creates a fresh database for every cold request, including admission-threshold search runs.
fn database() -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", SOURCE).unwrap();
    db
}

/// Resets only passive observers; it does not request a semantic query or change the budget.
fn reset() -> receiver_observations::Recording {
    observations::reset(None);
    receiver_observations::reset();
    relation_observations::reset_assignability();
    child_observations::reset(None);
    receiver_observations::Recording::start()
}

/// Checks that inference owners, retained child futures, and the analysis attempt have drained.
fn assert_drained() {
    assert_eq!(observations::counts().0, 0);
    assert_eq!(child_observations::progress().0, 0);
    let children = child_observations::snapshot();
    assert!(!children.overflowed, "{children:?}");
    let entered = children.events[..children.count]
        .iter()
        .filter(|event| matches!(event, Some(child_observations::Event::Entered(_))))
        .count();
    let retired = children.events[..children.count]
        .iter()
        .filter(|event| matches!(event, Some(child_observations::Event::Retired(_))))
        .count();
    assert_eq!(entered, retired, "{children:?}");
    if let Some(drained) = receiver_observations::snapshot().drained {
        assert_eq!(drained.live_children, 0);
        assert_eq!(drained.child_events, children.count);
    }
    assert_no_active_attempt();
}

/// Recomputes the ordinary lazy relation after controlled execution and compares its owned storage.
fn assert_ordinary_parity<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    result: &PairResult<'db>,
) {
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = ConstraintSetBuilder::new().into_owned(|builder| {
        result
            .source
            .when_constraint_set_assignable_to(db, &env, result.target, builder)
    });
    assert_eq!(&*result.constraints, &ordinary);
}

/// Finds the already-interned ordered pair without creating a key or executing its query.
fn existing_key<'db>(
    db: &'db TestDb,
    program: Program<'db>,
    source: Type<'db>,
    target: Type<'db>,
) -> salsa::DatabaseKeyIndex {
    let mut keys = TypePair::ingredient(db.zalsa())
        .entries(db.zalsa())
        .filter(|entry| {
            let (key_program, first, second) = entry.value().fields();
            *key_program == program && *first == source && *second == target
        });
    let id = keys
        .next()
        .expect("the owned-assignability request did not intern its ordered key")
        .key()
        .key_index();
    assert!(keys.next().is_none());
    owned_assignability_ingredient(db).database_key_index(id)
}

/// Each nominal comparison uses lazy type-variable evaluation with no inferable override.
/// It preserves its builder, environment, and all visitors.
fn assert_lazy_owners(expected: bool) {
    let snapshot = relation_observations::assignability_snapshot();
    assert_eq!(snapshot.root_count, 1, "{snapshot:?}");
    assert!(snapshot.pair_count > 0 && snapshot.pair_count <= snapshot.pairs.len());
    let root = snapshot.roots[0].unwrap().identity;
    assert_eq!(root.relation, TypeRelation::Assignability);
    assert_eq!(root.typevars, TypeVarEvaluation::Lazy);
    assert_eq!(root.inferable, None);
    assert!(root.given_is_original_never);
    assert!(
        snapshot.pairs[..snapshot.pair_count]
            .iter()
            .flatten()
            .all(|pair| pair.identity == root)
    );
    assert_eq!(snapshot.result_count, 1);
    let result = snapshot.results[0].unwrap();
    assert_eq!(result.terminal, Some(expected));
    assert!(result.original_terminal);
}

/// Cold nominal comparisons publish full ordinary owned results under the original ordered key.
/// Controlled and ordinary same-revision reads then reuse that exact key and memo address.
#[test_case::test_case(NominalCase::Subclass; "terminal true")]
#[test_case::test_case(NominalCase::Unrelated; "terminal false")]
fn cold_nominal_results_use_original_key_and_memo(case: NominalCase) {
    let db = database();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let _recording = reset();
    let cold = capture(&db, || {
        controlled_member_operation(
            &prepared,
            Request(Inputs::Cold(Definitions::new(&prepared), case)),
            &funded(),
        )
    })
    .unwrap();
    cold.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(result)) = cold.value else {
        panic!("cold nominal {case:?}: {:?}", cold.value);
    };
    assert_lazy_owners(case.expected());
    assert_drained();
    let program = prepared.program_file().program(&db);
    let key = existing_key(&db, program, result.source, result.target);
    assert_eq!(
        receiver_observations::snapshot()
            .boundary(Stage::Owner)
            .after,
        1
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            owned_assignability_ingredient(&db),
            key.key_index()
        )
        .is_ok()
    );
    let first = cold.reads.iter().find(|read| read.key == key).unwrap();
    receiver_observations::reset();
    let warm = capture(&db, || {
        controlled_member_operation(
            &prepared,
            Request(Inputs::Existing(result.source, result.target)),
            &funded(),
        )
    })
    .unwrap();
    warm.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(reused)) = warm.value else {
        panic!("same-revision receiver constraint: {:?}", warm.value);
    };
    assert_eq!(reused.constraints, result.constraints);
    assert_eq!(
        receiver_observations::snapshot()
            .boundary(Stage::Owner)
            .before,
        0
    );
    assert!(
        warm.reads
            .iter()
            .any(|read| read.key == key && read.memo_address == first.memo_address)
    );
    let ordinary = capture(&db, || {
        let env = ProgramEnvironment::from_file(prepared.program_file());
        result
            .source
            .when_constraint_set_assignable_to_owned(&db, &env, result.target)
    })
    .unwrap();
    assert_eq!(ordinary.value, result.constraints);
    assert!(
        ordinary
            .reads
            .iter()
            .any(|read| read.key == key && read.memo_address == first.memo_address)
    );
    assert_ordinary_parity(&db, &prepared, &result);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_drained();
}

/// Selects the shared shortcut whose operands require no owned-relation body execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Shortcut {
    Equal,
    DynamicSource,
    DynamicTarget,
    Never,
    Object,
    Union,
    Intersection,
    EqualTypevar,
}

/// Constructs ordinary inputs without executing the owned-assignability query being controlled.
/// Raw union/intersection interners preserve the containment case instead of simplifying it away.
fn shortcut_inputs<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    case: Shortcut,
) -> Inputs<'db> {
    let first = Type::int_literal(1);
    let second = Type::int_literal(2);
    let (source, target) = match case {
        Shortcut::Equal => (first, first),
        Shortcut::DynamicSource => (Type::unknown(), first),
        Shortcut::DynamicTarget => (first, Type::unknown()),
        Shortcut::Never => (Type::Never, first),
        Shortcut::Object => (first, Type::object()),
        Shortcut::Union => (
            first,
            Type::Union(UnionType::new(
                db,
                Box::from([second, first]),
                RecursivelyDefined::No,
            )),
        ),
        Shortcut::Intersection => (
            Type::Intersection(IntersectionType::new(
                db,
                [Type::AlwaysTruthy, first].into_iter().collect::<FxOrderSet<_>>(),
                NegativeIntersectionElements::default(),
            )),
            first,
        ),
        Shortcut::EqualTypevar => {
            let variable = Type::TypeVar(BoundTypeVarInstance::synthetic(
                db,
                env,
                Name::new_static("T"),
                TypeVarVariance::Invariant,
            ));
            (variable, variable)
        }
    };
    Inputs::Existing(source, target)
}

/// Equality precedes type-variable exclusion; later scalar and containment shortcuts stay shared.
/// Ordinary input construction is separate from the cold controlled owned-relation request.
#[test_case::test_case(Shortcut::Equal; "equal")]
#[test_case::test_case(Shortcut::DynamicSource; "dynamic source")]
#[test_case::test_case(Shortcut::DynamicTarget; "dynamic target")]
#[test_case::test_case(Shortcut::Never; "never source")]
#[test_case::test_case(Shortcut::Object; "object target")]
#[test_case::test_case(Shortcut::Union; "union containment")]
#[test_case::test_case(Shortcut::Intersection; "intersection containment")]
#[test_case::test_case(Shortcut::EqualTypevar; "equal typevar")]
fn shortcuts_preserve_owned_terminal_results(case: Shortcut) {
    let db = database();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = shortcut_inputs(&db, &env, case);
    let _recording = reset();
    let captured = capture(&db, || {
        controlled_member_operation(&prepared, Request(input), &funded())
    })
    .unwrap();
    assert_eq!(
        captured.check_root_reads(),
        Err(salsa::prepared_source_probe::CaptureError::NoRootReads)
    );
    let Ok(AnalysisOutcome::Complete(result)) = captured.value else {
        panic!("shortcut {case:?}: {:?}", captured.value);
    };
    assert!(matches!(&result.constraints, Cow::Owned(_)));
    assert_eq!(&*result.constraints, &OwnedConstraintSet::always());
    assert_eq!(
        receiver_observations::snapshot()
            .boundary(Stage::Owner)
            .before,
        0
    );
    assert_ordinary_parity(&db, &prepared, &result);
    assert_drained();
}

/// Selects a scalar shortcut that must not bypass a distinct type variable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TypevarCase {
    NeverSource,
    DynamicSource,
    DynamicTarget,
    ObjectTarget,
}

/// Type variables reach their precise lazy-construction refusal before the later scalar shortcuts.
/// The variable is an ordinary preconstructed input; no owned-relation memo is prewarmed.
#[test_case::test_case(TypevarCase::NeverSource; "never before typevar")]
#[test_case::test_case(TypevarCase::DynamicSource; "dynamic before typevar")]
#[test_case::test_case(TypevarCase::DynamicTarget; "typevar before dynamic")]
#[test_case::test_case(TypevarCase::ObjectTarget; "typevar before object")]
fn distinct_typevars_exclude_later_shortcuts(case: TypevarCase) {
    let db = database();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let variable = Type::TypeVar(BoundTypeVarInstance::synthetic(
        &db,
        &env,
        Name::new_static("T"),
        TypeVarVariance::Invariant,
    ));
    let (source, target, operation) = match case {
        TypevarCase::NeverSource => (
            Type::Never,
            variable,
            RelationOperation::LazyTypevarLowerConstraint,
        ),
        TypevarCase::DynamicSource => (
            Type::unknown(),
            variable,
            RelationOperation::LazyTypevarLowerConstraint,
        ),
        TypevarCase::DynamicTarget => (
            variable,
            Type::unknown(),
            RelationOperation::LazyTypevarUpperConstraint,
        ),
        TypevarCase::ObjectTarget => (
            variable,
            Type::object(),
            RelationOperation::LazyTypevarUpperConstraint,
        ),
    };
    let _recording = reset();
    let result = controlled_member_operation(
        &prepared,
        Request(Inputs::Existing(source, target)),
        &funded(),
    );
    assert!(
        matches!(result, Ok(AnalysisOutcome::Incomplete { reason: AnalysisIncomplete::UnavailableOperation(found), completed: () }) if found == OperationId::Relation(operation)),
        "{result:?}"
    );
    let key = existing_key(&db, env.program(&db), source, target);
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            owned_assignability_ingredient(&db),
            key.key_index()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert_eq!(
        receiver_observations::snapshot()
            .boundary(Stage::Package)
            .after,
        0
    );
    assert_drained();
}

/// Runs terminal intersection directly or the constructor's already-filtered merge callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MergeEntry {
    Resource,
    Constructor,
}

/// Borrows ordinary-preconstructed operands through the complete controlled merge operation.
#[derive(Clone, Copy, Debug)]
struct MergeRequest<'a, 'db> {
    first: &'a OwnedConstraintSet<'db>,
    second: &'a OwnedConstraintSet<'db>,
    entry: MergeEntry,
}

impl<'db> MemberOperation<'db> for MergeRequest<'_, 'db> {
    type Output = Option<OwnedConstraintSet<'db>>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        match self.entry {
            MergeEntry::Resource => {
                match access
                    .resources()
                    .intersect_owned_terminals(access.db(), self.first, self.second, &effects)
                    .await?
                {
                    SourceStructuralResult::Complete(value) => Ok(Some(value)),
                    SourceStructuralResult::Unsupported => Err(RunError::Contract(
                        "terminal fixture unexpectedly required graph loading",
                    )),
                }
            }
            MergeEntry::Constructor => {
                let env = ProgramEnvironment::from_program(program);
                ConstructorSignatureEffects::intersect_constraints(
                    &effects,
                    access.db(),
                    &env,
                    self.first,
                    self.second,
                )
                .await
            }
        }
    }
}

/// Constructs an ordinary terminal input without executing an owned-relation query.
fn terminal<'db>(value: bool) -> OwnedConstraintSet<'db> {
    ConstraintSetBuilder::new().into_owned(|builder| ConstraintSet::from_bool(builder, value))
}

/// Terminal loads and conjunction preserve all four ordinary truth combinations.
/// False on the left short-circuits the second load, as in the ordinary solver.
#[test_case::test_case(true, true; "true and true")]
#[test_case::test_case(true, false; "true and false")]
#[test_case::test_case(false, true; "false and true")]
#[test_case::test_case(false, false; "false and false")]
fn terminal_merge_matches_ordinary_loading(first: bool, second: bool) {
    let db = database();
    let prepared = prepare(&db);
    let first_set = terminal(first);
    let second_set = terminal(second);
    let _recording = reset();
    let captured = capture(&db, || {
        controlled_member_operation(
            &prepared,
            MergeRequest {
                first: &first_set,
                second: &second_set,
                entry: MergeEntry::Resource,
            },
            &funded(),
        )
    })
    .unwrap();
    assert_eq!(
        captured.check_root_reads(),
        Err(salsa::prepared_source_probe::CaptureError::NoRootReads)
    );
    let Ok(AnalysisOutcome::Complete(Some(result))) = captured.value else {
        panic!("terminal merge: {:?}", captured.value);
    };
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = ConstraintSetBuilder::new().into_owned(|builder| {
        builder
            .load(&db, &env, &first_set)
            .and(&db, builder, || builder.load(&db, &env, &second_set))
    });
    assert_eq!(result, ordinary);
    assert_eq!(
        receiver_observations::snapshot().merge_loads,
        if first { 2 } else { 1 }
    );
    assert_eq!(
        receiver_observations::snapshot()
            .boundary(Stage::Merge)
            .after,
        1
    );
    assert_drained();
}

/// The constructor hook preserves ordinary terminal results, including absence for true/true.
#[test_case::test_case(true, true; "true and true")]
#[test_case::test_case(true, false; "true and false")]
#[test_case::test_case(false, true; "false and true")]
#[test_case::test_case(false, false; "false and false")]
fn constructor_terminal_merge_matches_ordinary(first: bool, second: bool) {
    let db = database();
    let prepared = prepare(&db);
    let first = terminal(first);
    let second = terminal(second);
    let _recording = reset();
    let result = controlled_member_operation(
        &prepared,
        MergeRequest {
            first: &first,
            second: &second,
            entry: MergeEntry::Constructor,
        },
        &funded(),
    );
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = legacy_inline(ConstructorSignatureEffects::intersect_constraints(
        &InlineConstructorSignatureEffects,
        &db,
        &env,
        &first,
        &second,
    ));
    assert_eq!(result, Ok(AnalysisOutcome::Complete(ordinary)));
    assert_eq!(
        receiver_observations::snapshot()
            .boundary(Stage::Merge)
            .after,
        1
    );
    assert_drained();
}

/// Builds a real conditional constraint using ordinary solver APIs without warming canonical queries.
fn nonterminal<'db>(db: &'db TestDb, env: &ProgramEnvironment<'db>) -> OwnedConstraintSet<'db> {
    let variable =
        BoundTypeVarInstance::synthetic(db, env, Name::new_static("T"), TypeVarVariance::Invariant);
    let bound = TypeFormType::from_type_expression(db, Type::int_literal(1));
    ConstraintSetBuilder::new().into_owned(|builder| {
        ConstraintSet::constrain_typevar_equivalence_bound(db, env, builder, variable, bound)
    })
}

/// A real nonterminal operand refuses before graph loading, even beside a false terminal.
#[test_case::test_case(false; "nonterminal on left")]
#[test_case::test_case(true; "nonterminal on right")]
fn nonterminal_merge_refuses_before_loading(reverse: bool) {
    let db = database();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let conditional = nonterminal(&db, &env);
    conditional.query(|_, set| assert!(set.to_owned_terminal().is_none()));
    let never = terminal(false);
    let (first, second) = if reverse {
        (&never, &conditional)
    } else {
        (&conditional, &never)
    };
    let _recording = reset();
    let result = controlled_member_operation(
        &prepared,
        MergeRequest {
            first,
            second,
            entry: MergeEntry::Constructor,
        },
        &funded(),
    );
    assert_eq!(
        result,
        Ok(unavailable(OperationId::ConstructorSignature(
            ConstructorSignatureOperation::ReceiverConstraintMerge
        )))
    );
    let snapshot = receiver_observations::snapshot();
    assert_eq!(snapshot.rejected_merges, 1);
    assert_eq!(snapshot.merge_loads, 0);
    assert_eq!(snapshot.boundary(Stage::Package).before, 0);
    assert_eq!(snapshot.boundary(Stage::Merge).after, 0);
    assert_drained();
}

/// Requests terminal packaging of a real borrowed solver result while its builder stays alive.
#[derive(Clone, Copy)]
struct PackageRequest<'c, 'db> {
    builder: &'c ConstraintSetBuilder<'db>,
    value: ConstraintSet<'db, 'c>,
}

impl<'db> MemberOperation<'db> for PackageRequest<'_, 'db> {
    type Output = OwnedConstraintSet<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        package_terminal(
            self.builder,
            self.value,
            &SourceEffects::new(access, program),
        )
        .await
    }
}

/// Packaging a real conditional result preserves the explicit owned-compaction refusal.
/// Ordinary construction supplies the borrowed test input.
#[test]
fn nonterminal_packaging_preserves_compaction_refusal() {
    let db = database();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let variable = BoundTypeVarInstance::synthetic(
        &db,
        &env,
        Name::new_static("T"),
        TypeVarVariance::Invariant,
    );
    let bound = TypeFormType::from_type_expression(&db, Type::int_literal(1));
    let builder = ConstraintSetBuilder::new();
    let value =
        ConstraintSet::constrain_typevar_equivalence_bound(&db, &env, &builder, variable, bound);
    assert!(value.to_owned_terminal().is_none());
    let _recording = reset();
    let result = controlled_member_operation(
        &prepared,
        PackageRequest {
            builder: &builder,
            value,
        },
        &funded(),
    );
    assert_eq!(
        result,
        Ok(unavailable(OperationId::Relation(
            RelationOperation::OwnedCompaction
        )))
    );
    let boundary = receiver_observations::snapshot().boundary(Stage::Package);
    assert_eq!(boundary.before, 1);
    assert_eq!(boundary.after, 0);
    assert_drained();
}

/// Clones one existing condition through the constructor's `clone_constraints` callback.
#[derive(Clone, Copy, Debug)]
struct CloneRequest<'a, 'db>(&'a OwnedConstraintSet<'db>);

impl<'db> MemberOperation<'db> for CloneRequest<'_, 'db> {
    type Output = OwnedConstraintSet<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        ConstructorSignatureEffects::clone_constraints(&SourceEffects::new(access, program), self.0)
            .await
    }
}

/// A sole real nonterminal receiver set is cloned intact without loading or merging its graph.
#[test]
fn single_nonterminal_clone_preserves_full_storage() {
    let db = database();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = nonterminal(&db, &env);
    let _recording = reset();
    let captured = capture(&db, || {
        controlled_member_operation(&prepared, CloneRequest(&input), &funded())
    })
    .unwrap();
    assert_eq!(
        captured.check_root_reads(),
        Err(salsa::prepared_source_probe::CaptureError::NoRootReads)
    );
    assert_eq!(captured.value, Ok(AnalysisOutcome::Complete(input.clone())));
    assert_eq!(receiver_observations::snapshot().merge_loads, 0);
    assert_drained();
}

/// An ordinary-produced conditional memo remains reusable; this is explicitly a warm-cache control.
/// It does not establish support for producing nonterminal owned constraints in a cold run.
#[test]
fn ordinary_nonterminal_memo_is_reused_unchanged() {
    let db = database();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let source = Type::TypeVar(BoundTypeVarInstance::synthetic(
        &db,
        &env,
        Name::new_static("T"),
        TypeVarVariance::Invariant,
    ));
    let target = Type::int_literal(1);
    let ordinary = capture(&db, || {
        source.when_constraint_set_assignable_to_owned(&db, &env, target)
    })
    .unwrap();
    ordinary
        .value
        .query(|_, set| assert!(set.to_owned_terminal().is_none()));
    let key = existing_key(&db, env.program(&db), source, target);
    let original = ordinary.reads.iter().find(|read| read.key == key).unwrap();
    let _recording = reset();
    let controlled = capture(&db, || {
        controlled_member_operation(
            &prepared,
            Request(Inputs::Existing(source, target)),
            &funded(),
        )
    })
    .unwrap();
    controlled.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(result)) = controlled.value else {
        panic!("existing nonterminal memo: {:?}", controlled.value);
    };
    assert_eq!(result.constraints, ordinary.value);
    assert!(
        controlled
            .reads
            .iter()
            .any(|read| read.key == original.key && read.memo_address == original.memo_address)
    );
    assert_eq!(
        receiver_observations::snapshot()
            .boundary(Stage::Owner)
            .before,
        0
    );
    assert_drained();
}

/// Selects which independent cumulative allowance is reduced for an admission control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resource {
    Work,
    Bytes,
}

impl Resource {
    fn policy(self, limit: usize) -> AnalysisPolicy {
        match self {
            Self::Work => AnalysisPolicy {
                semantic_work_limit: limit,
                ..funded()
            },
            Self::Bytes => AnalysisPolicy {
                requested_bytes_limit: limit,
                ..funded()
            },
        }
    }

    fn limit(self) -> usize {
        match self {
            Self::Work => funded().semantic_work_limit,
            Self::Bytes => funded().requested_bytes_limit,
        }
    }

    const fn reason(self) -> AnalysisIncomplete {
        match self {
            Self::Work => AnalysisIncomplete::WorkLimit,
            Self::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
        }
    }
}

/// Preserves the complete successful value across the two operations exercised by admission tests.
#[derive(Debug)]
enum BoundaryResult<'db> {
    Pair(PairResult<'db>),
    Merge(Option<OwnedConstraintSet<'db>>),
}

/// Uses cold source operands for production and preconstructed terminals for the direct merge.
#[derive(Clone, Copy, Debug)]
struct BoundaryRequest<'a, 'db> {
    definitions: Definitions<'db>,
    stage: Stage,
    never: &'a OwnedConstraintSet<'db>,
    operands: Option<&'a Cell<Option<Operands<'db>>>>,
}

impl<'db> MemberOperation<'db> for BoundaryRequest<'_, 'db> {
    type Output = BoundaryResult<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        match self.stage {
            Stage::Owner | Stage::Package | Stage::Transfer => {
                Request(Inputs::Cold(self.definitions, NominalCase::Unrelated))
                    .execute(access, program, None, self.operands)
                    .await
                    .map(BoundaryResult::Pair)
            }
            Stage::Merge => MergeRequest {
                first: self.never,
                second: self.never,
                entry: MergeEntry::Constructor,
            }
            .run(access, program)
            .await
            .map(BoundaryResult::Merge),
        }
    }
}

/// Reports whether the selected operation completes under the policy, using a fresh cold database.
fn reaches_boundary(stage: Stage, policy: &AnalysisPolicy) -> bool {
    let db = database();
    let prepared = prepare(&db);
    let never = terminal(false);
    let _recording = reset();
    let _result = controlled_member_operation(
        &prepared,
        BoundaryRequest {
            definitions: Definitions::new(&prepared),
            stage,
            never: &never,
            operands: None,
        },
        policy,
    );
    let completed = receiver_observations::snapshot().boundary(stage).after > 0;
    assert_drained();
    completed
}

/// Work and bytes independently refuse before the selected admitted operation completes.
/// Every search point uses a fresh database; only the refused run is retried in the same revision.
/// Producer refusal leaves no final owned-query memo, and a funded retry completes.
#[test_case::test_case(Stage::Owner, Resource::Work; "owner work")]
#[test_case::test_case(Stage::Owner, Resource::Bytes; "owner bytes")]
#[test_case::test_case(Stage::Package, Resource::Work; "packaging work")]
#[test_case::test_case(Stage::Package, Resource::Bytes; "packaging bytes")]
#[test_case::test_case(Stage::Merge, Resource::Work; "merge work")]
#[test_case::test_case(Stage::Merge, Resource::Bytes; "merge bytes")]
#[test_case::test_case(Stage::Transfer, Resource::Work; "final transfer work")]
#[test_case::test_case(Stage::Transfer, Resource::Bytes; "final transfer bytes")]
fn resource_refusal_precedes_completion_and_allows_retry(stage: Stage, resource: Resource) {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(reaches_boundary(stage, &resource.policy(high)));
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if reaches_boundary(stage, &resource.policy(middle)) {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = database();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let never = terminal(false);
    let operands = Cell::new(None);
    let request = BoundaryRequest {
        definitions: Definitions::new(&prepared),
        stage,
        never: &never,
        operands: Some(&operands),
    };
    let _recording = reset();
    let result = controlled_member_operation(&prepared, request, &resource.policy(low));
    assert!(
        matches!(result, Ok(AnalysisOutcome::Incomplete { reason, completed: () }) if reason == resource.reason()),
        "{stage:?}/{resource:?}: {result:?}"
    );
    let boundary = receiver_observations::snapshot().boundary(stage);
    assert!(boundary.before > 0, "{stage:?}/{resource:?}: {boundary:?}");
    assert_eq!(boundary.after, 0, "{stage:?}/{resource:?}: {boundary:?}");
    assert_drained();
    let key = match stage {
        Stage::Owner | Stage::Package | Stage::Transfer => {
            let operands = operands
                .get()
                .expect("the refused query did not record its operands");
            let key = existing_key(
                &db,
                prepared.program_file().program(&db),
                operands.source,
                operands.target,
            );
            assert_eq!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    owned_assignability_ingredient(&db),
                    key.key_index()
                )
                .map(|_| ()),
                Err(FinalSourceError::MissingMemo)
            );
            Some(key)
        }
        Stage::Merge => None,
    };
    receiver_observations::reset();
    let retry = controlled_member_operation(&prepared, request, &funded());
    let Ok(AnalysisOutcome::Complete(value)) = retry else {
        panic!("funded {stage:?}/{resource:?} retry: {retry:?}");
    };
    assert!(receiver_observations::snapshot().boundary(stage).after > 0);
    match value {
        BoundaryResult::Pair(result) => {
            let key = key.expect("producer refusal must retain its canonical key");
            assert_eq!(
                key,
                existing_key(
                    &db,
                    prepared.program_file().program(&db),
                    result.source,
                    result.target
                )
            );
            assert!(
                receiver_observations::snapshot()
                    .boundary(Stage::Owner)
                    .after
                    > 0
            );
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    owned_assignability_ingredient(&db),
                    key.key_index()
                )
                .is_ok()
            );
            assert_ordinary_parity(&db, &prepared, &result);
        }
        BoundaryResult::Merge(result) => assert_eq!(result, Some(never)),
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_drained();
}

/// Cancellation starts at an actual retained child's first Pending and drains it before pool release.
/// Fixpoint masking can defer local cancellation until the canonical query completes; its certified
/// memo is then reused by exact key. An unfinished query has no final memo and executes on retry.
#[test]
fn actual_pending_cancellation_drains_children_and_preserves_publication_rules() {
    let db = database();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let request = Request(Inputs::Cold(
        Definitions::new(&prepared),
        NominalCase::Subclass,
    ));
    let operands = Cell::new(None);
    let _recording = reset();
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(
            &prepared,
            CancellationRequest {
                request,
                operands: &operands,
            },
            &funded(),
        )
    }));
    assert!(
        matches!(cancelled, Err(salsa::Cancelled::Local)),
        "{cancelled:?}"
    );
    let children = child_observations::snapshot();
    let pending = children.events[..children.count]
        .iter()
        .position(|event| *event == Some(child_observations::Event::Pending(1)))
        .expect("selected child did not return Pending");
    let retired = children.events[..children.count]
        .iter()
        .position(|event| *event == Some(child_observations::Event::Retired(1)))
        .expect("selected child did not retire");
    assert!(pending < retired);
    let drained = receiver_observations::snapshot()
        .drained
        .expect("driver-drain observation is missing");
    assert_eq!(drained.live_children, 0);
    assert!(retired < drained.child_events);
    assert_drained();
    let operands = operands
        .get()
        .expect("the cancelled query did not record its operands");
    let key = existing_key(
        &db,
        prepared.program_file().program(&db),
        operands.source,
        operands.target,
    );
    let memo = FinalSourceMemo::certify(
        &db as &dyn Db,
        owned_assignability_ingredient(&db),
        key.key_index(),
    );
    let completed = match memo {
        Ok(_) => {
            assert!(
                receiver_observations::snapshot()
                    .boundary(Stage::Transfer)
                    .after
                    > 0
            );
            true
        }
        Err(error) => {
            assert_eq!(error, FinalSourceError::MissingMemo);
            false
        }
    };
    receiver_observations::reset();
    child_observations::reset(None);
    let retry = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    retry.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(result)) = retry.value else {
        panic!("funded cancellation retry: {:?}", retry.value);
    };
    let retry_key = existing_key(
        &db,
        prepared.program_file().program(&db),
        result.source,
        result.target,
    );
    assert_eq!(key, retry_key);
    assert!(retry.reads.iter().any(|read| read.key == key));
    if completed {
        assert_eq!(
            receiver_observations::snapshot()
                .boundary(Stage::Owner)
                .before,
            0
        );
    } else {
        assert!(
            receiver_observations::snapshot()
                .boundary(Stage::Transfer)
                .after
                > 0
        );
        assert!(
            receiver_observations::snapshot()
                .boundary(Stage::Owner)
                .after
                > 0
        );
    }
    assert_ordinary_parity(&db, &prepared, &result);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_drained();
}
