//! The inline provider is a finite reference backend. These controls establish dispatcher
//! equivalence and lazy effect ordering; recursive-stack supervision belongs to the task driver.

use std::any::type_name;
use std::cell::{Cell, RefCell};
use std::fmt::Display;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use self::DependencyOutput::{
    Callable, Constraints, FieldConverter, FieldDefault, Type as TypeOutput, Wrapped, WrapperKind,
};
use super::{AsyncConstraintSet, AsyncIteratorConstraints, InlinePairEffects};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::dedicated::pydantic::ConfigBoolean;
use crate::types::known_instance::{
    FieldInstance, FunctoolsPartialInstance, InternedType, MethodWrapper, MethodWrapperKind,
};
use crate::types::relation::dependencies::{OrdinaryDependencies, RelationDependencies};
use crate::types::relation::guard::RelationGuardStep;
use crate::types::relation::{
    HasRelationToVisitor, IsDisjointVisitor, TypeRelation, TypeRelationChecker, TypeVarEvaluation,
};
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::type_form::TypeFormType;
use crate::types::typevar::TypeVarSet;
use crate::types::{
    ApplyTypeMappingVisitor, CallableType, ClassLiteral, ErrorContextTree, KnownClass,
    KnownInstanceType, Type, TypeGuardType, TypeIsType, UnionType,
};

fn complete<T, E: Display>(future: impl Future<Output = Result<T, E>>) -> anyhow::Result<T> {
    match pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result.map_err(|error| anyhow::anyhow!("{error}")),
        Poll::Pending => anyhow::bail!("the finite inline provider must complete synchronously"),
    }
}

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/pair_dispatch.py",
            r#"from collections.abc import Sequence
from typing import Protocol, TypedDict

class UniversalSet(Protocol): ...

class SupportsStr(Protocol):
    def __str__(self) -> str: ...

class Required(Protocol):
    value: int

class Good:
    value: int

class Bad:
    value: str

class Record(TypedDict):
    value: int

def integer(value: int) -> str: ...
def text(value: str) -> int: ...

strings: Sequence[str]
integers: Sequence[int]
"#,
        )
        .build()
}

fn fixture<'db>(db: &'db TestDb, name: &str, instance: bool) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/pair_dispatch.py")?,
        env.program(db),
    );
    let ty = global_symbol(db, file, name).place.expect_type();
    if !instance {
        return Ok(ty);
    }
    let class = ty
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing fixture class {name}"))?;
    Ok(Type::instance(db, &env, class.identity_specialization(db)))
}

#[test]
fn exhaustive_dispatch_body_matches_direct_checker() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let int = KnownClass::Int.to_instance(&db, &env);
    let text = KnownClass::Str.to_instance(&db, &env);
    let bytes = KnownClass::Bytes.to_instance(&db, &env);
    let universal = fixture(&db, "UniversalSet", true)?;
    let supports_str = fixture(&db, "SupportsStr", true)?;
    let required = fixture(&db, "Required", true)?;
    for (ty, expected) in [(universal, true), (supports_str, true), (required, false)] {
        let protocol = ty
            .as_protocol_instance()
            .ok_or_else(|| anyhow::anyhow!("fixture must retain a real protocol instance"))?;
        assert_eq!(protocol.is_equivalent_to_object(&db), expected);
        if ty == supports_str {
            assert!(protocol.interface(&db).includes_member(&db, "__str__"));
        }
    }
    let cases = [
        Type::Never,
        Type::any(),
        Type::unknown(),
        Type::object(),
        int,
        text,
        UnionType::from_two_elements(&db, &env, int, text),
        required,
        universal,
        supports_str,
        TypeFormType::from_type_expression(&db, int),
        TypeFormType::from_type_expression(&db, text),
        TypeIsType::from_type_expression(&db, int),
        TypeIsType::from_type_expression(&db, text),
        TypeGuardType::unbound(&db, int),
        TypeGuardType::unbound(&db, text),
        fixture(&db, "Good", true)?,
        fixture(&db, "Bad", true)?,
        fixture(&db, "Record", true)?,
        fixture(&db, "Good", false)?,
        fixture(&db, "integer", false)?,
        fixture(&db, "text", false)?,
    ];
    let strings = fixture(&db, "strings", false)?;
    let integers = fixture(&db, "integers", false)?;
    assert!(matches!(strings, Type::NominalInstance(_)));
    assert!(matches!(integers, Type::NominalInstance(_)));
    let literal_pairs = [
        (Type::string_literal(&db, ""), [text, int, strings]),
        (Type::string_literal(&db, "a"), [text, int, strings]),
        (Type::string_literal(&db, "aba"), [text, int, strings]),
        (Type::bytes_literal(&db, b""), [bytes, int, integers]),
        (Type::bytes_literal(&db, b"a"), [bytes, int, integers]),
        (Type::bytes_literal(&db, b"aba"), [bytes, int, integers]),
    ];
    let effects = InlinePairEffects {
        db: &db,
        dependencies: OrdinaryDependencies,
    };
    for relation in [
        TypeRelation::Assignability,
        TypeRelation::Subtyping,
        TypeRelation::Redundancy { pure: true },
    ] {
        for typevars in [TypeVarEvaluation::Eager, TypeVarEvaluation::Lazy] {
            let pairs = cases
                .into_iter()
                .flat_map(|source| cases.map(|target| (source, target)));
            let literals = literal_pairs
                .into_iter()
                .flat_map(|(source, targets)| targets.map(|target| (source, target)));
            for (source, target) in pairs.chain(literals) {
                let constraints = ConstraintSetBuilder::new();
                let relations = HasRelationToVisitor::default(&constraints);
                let disjointness = IsDisjointVisitor::default(&constraints);
                let signatures = SignatureRelationVisitor::default();
                let mapping = ApplyTypeMappingVisitor::new(&env);
                let mut ordinary = TypeRelationChecker::new(
                    &env,
                    relation,
                    &constraints,
                    TypeVarSet::None,
                    &relations,
                    &disjointness,
                    &signatures,
                    &mapping,
                );
                ordinary.typevar_evaluation = typevars;
                ordinary.context_tree = Some(ErrorContextTree::new(relation));
                let mut effectful = ordinary.clone();
                effectful.context_tree = Some(ErrorContextTree::new(relation));
                let expected = ordinary.check_type_pair_inner(&db, source, target);
                let actual = complete(effectful.check_type_pair_inner_with(
                    source,
                    target,
                    &effects,
                ))?;
                assert!(
                    actual.ownership_probe_same_set(expected),
                    "{relation:?}: {source:?} -> {target:?}"
                );
                assert_eq!(
                    ordinary.context_tree, effectful.context_tree,
                    "context for {relation:?}: {source:?} -> {target:?}"
                );
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct Audit {
    calls: Cell<usize>,
    refuse_at: Option<usize>,
}

impl RelationDependencies for &Audit {
    type Error = anyhow::Error;
    fn run<T>(&self, _db: &dyn Db, work: impl FnOnce() -> T) -> anyhow::Result<T> {
        let index = self.calls.get();
        self.calls.set(index + 1);
        anyhow::ensure!(self.refuse_at != Some(index), "refused");
        Ok(work())
    }
}

#[test]
fn refused_dispatch_child_does_not_advance_its_parent() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let source = fixture(&db, "Bad", true)?;
    let target = fixture(&db, "Required", true)?;
    let constraints = ConstraintSetBuilder::new();
    let relations = HasRelationToVisitor::default(&constraints);
    let disjointness = IsDisjointVisitor::default(&constraints);
    let signatures = SignatureRelationVisitor::default();
    let mapping = ApplyTypeMappingVisitor::new(&env);
    let checker = TypeRelationChecker::new(
        &env,
        TypeRelation::Assignability,
        &constraints,
        TypeVarSet::None,
        &relations,
        &disjointness,
        &signatures,
        &mapping,
    );
    let audit = Audit::default();
    let effects = InlinePairEffects {
        db: &db,
        dependencies: &audit,
    };
    let expected = complete(checker.check_type_pair_inner_with(
        source,
        target,
        &effects,
    ))?;
    let calls = audit.calls.get();
    assert!(calls > 1);
    for refuse_at in 0..calls {
        let audit = Audit {
            refuse_at: Some(refuse_at),
            ..Audit::default()
        };
        let effects = InlinePairEffects {
            db: &db,
            dependencies: &audit,
        };
        // Fresh visitors are independent retries; a completed guard cache would skip the body.
        let relations = HasRelationToVisitor::default(&constraints);
        let checker = TypeRelationChecker {
            relation_visitor: &relations,
            ..checker.clone()
        };
        assert!(
            complete(checker.check_type_pair_inner_with(
                source,
                target,
                &effects
            ))
            .is_err()
        );
        assert_eq!(audit.calls.get(), refuse_at + 1);
        assert_eq!(relations.ownership_probe_counts().0, 0);
        // Completed children can remain cached, but the unfinished parent must be retried.
        let RelationGuardStep::Evaluate(scope) =
            RelationGuardStep::start(&db, &checker, source, target, &OrdinaryDependencies)?
        else {
            anyhow::bail!("refused parent published a completed cache entry");
        };
        drop(scope);
        let retry = InlinePairEffects {
            db: &db,
            dependencies: OrdinaryDependencies,
        };
        let actual = complete(checker.check_type_pair_inner_with(
            source,
            target,
            &retry,
        ))?;
        assert!(actual.ownership_probe_same_set(expected));
    }
    for _ in 0..2 {
        let audit = Audit::default();
        let effects = InlinePairEffects {
            db: &db,
            dependencies: &audit,
        };
        let actual = complete(checker.check_type_pair_inner_with(
            source,
            target,
            &effects,
        ))?;
        assert!(actual.ownership_probe_same_set(expected));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DependencyOutput {
    Type,
    FieldDefault,
    FieldConverter,
    WrapperKind,
    Wrapped,
    Callable,
    Constraints,
}

impl DependencyOutput {
    fn of<T>() -> Option<Self> {
        let output = type_name::<T>();
        if output == type_name::<Type<'_>>() {
            Some(Self::Type)
        } else if output == type_name::<Option<Type<'_>>>() {
            Some(Self::FieldDefault)
        } else if output == type_name::<Option<(Type<'_>, Type<'_>)>>() {
            Some(Self::FieldConverter)
        } else if output == type_name::<MethodWrapperKind>() {
            Some(Self::WrapperKind)
        } else if output == type_name::<InternedType<'_>>() {
            Some(Self::Wrapped)
        } else if output == type_name::<CallableType<'_>>() {
            Some(Self::Callable)
        } else if output == type_name::<ConstraintSet<'_, '_>>() {
            Some(Self::Constraints)
        } else {
            None
        }
    }
}

struct FieldAudit<'a, 'db, 'c> {
    relations: &'a HasRelationToVisitor<'db, 'c>,
    calls: Cell<usize>,
    outputs: RefCell<Vec<(usize, DependencyOutput, usize)>>,
    refuse_at: Option<usize>,
}

impl<'a, 'db, 'c> FieldAudit<'a, 'db, 'c> {
    fn new(relations: &'a HasRelationToVisitor<'db, 'c>, refuse_at: Option<usize>) -> Self {
        Self {
            relations,
            calls: Cell::new(0),
            outputs: RefCell::default(),
            refuse_at,
        }
    }
}

impl RelationDependencies for &FieldAudit<'_, '_, '_> {
    type Error = anyhow::Error;

    fn run<T>(&self, _db: &dyn Db, work: impl FnOnce() -> T) -> anyhow::Result<T> {
        let index = self.calls.get();
        self.calls.set(index + 1);
        // Observe the real inline operations without substituting field values or comparisons.
        if let Some(output) = DependencyOutput::of::<T>() {
            self.outputs.borrow_mut().push((
                index,
                output,
                self.relations.ownership_probe_counts().0,
            ));
        }
        anyhow::ensure!(self.refuse_at != Some(index), "refused");
        Ok(work())
    }
}

#[test]
fn stored_pair_fields_preserve_lazy_reads_and_guard_order() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let field = |default, converter| {
        Type::KnownInstance(KnownInstanceType::Field(FieldInstance::new(
            &db,
            Some(default),
            true,
            None,
            None,
            Some((Type::unknown(), converter)),
            ConfigBoolean::Unspecified,
        )))
    };
    let wrapper = |wrapped| {
        Type::KnownInstance(KnownInstanceType::MethodWrapper(MethodWrapper::new(
            &db,
            wrapped,
            MethodWrapperKind::Staticmethod,
        )))
    };
    let callable = CallableType::unknown(&db);
    let partial = |wrapped| {
        Type::KnownInstance(KnownInstanceType::FunctoolsPartial(
            FunctoolsPartialInstance::new(&db, InternedType::new(&db, wrapped), callable),
        ))
    };
    let type_form = |argument| TypeFormType::from_type_expression(&db, argument);
    let cases = [
        (
            "failed default skips converter",
            field(Type::bool_literal(false), Type::bool_literal(true)),
            Type::bool_literal(true),
            false,
            &[(FieldDefault, 0), (Constraints, 0)][..],
            None,
        ),
        (
            "converter follows successful default",
            field(Type::bool_literal(true), Type::bool_literal(false)),
            Type::bool_literal(true),
            false,
            &[
                (FieldDefault, 0),
                (Constraints, 0),
                (FieldConverter, 0),
                (Constraints, 0),
                (Constraints, 0),
            ],
            None,
        ),
        (
            "wrapper kinds precede guarded wrapped values",
            wrapper(Type::Never),
            wrapper(Type::object()),
            true,
            &[
                (WrapperKind, 0),
                (WrapperKind, 0),
                (TypeOutput, 1),
                (TypeOutput, 1),
                (Constraints, 1),
                (Constraints, 1),
            ],
            Some(&[(WrapperKind, 0), (WrapperKind, 0)][..]),
        ),
        (
            "type form arguments remain inside the guard",
            type_form(Type::Never),
            type_form(Type::object()),
            true,
            &[
                (TypeOutput, 1),
                (TypeOutput, 1),
                (Constraints, 1),
                (Constraints, 1),
            ],
            Some(&[]),
        ),
        (
            "failed wrapped comparison skips partial callables",
            partial(Type::object()),
            partial(Type::Never),
            false,
            &[
                (Wrapped, 1),
                (TypeOutput, 1),
                (Wrapped, 1),
                (TypeOutput, 1),
                (Constraints, 1),
                (Constraints, 1),
            ],
            Some(&[]),
        ),
        (
            "partial callables follow wrapped comparison",
            partial(Type::Never),
            partial(Type::object()),
            true,
            &[
                (Wrapped, 1),
                (TypeOutput, 1),
                (Wrapped, 1),
                (TypeOutput, 1),
                (Constraints, 1),
                (Callable, 1),
                (Callable, 1),
                (Constraints, 1),
                (Constraints, 1),
                (Constraints, 1),
            ],
            Some(&[]),
        ),
        (
            "type is reads both arguments before the first comparison",
            TypeIsType::from_type_expression(&db, Type::object()),
            TypeIsType::from_type_expression(&db, Type::Never),
            false,
            &[(TypeOutput, 0), (TypeOutput, 0), (Constraints, 0)],
            None,
        ),
        (
            "type is reuses arguments for the reverse comparison",
            TypeIsType::from_type_expression(&db, Type::Never),
            TypeIsType::from_type_expression(&db, Type::object()),
            false,
            &[
                (TypeOutput, 0),
                (TypeOutput, 0),
                (Constraints, 0),
                (Constraints, 0),
                (Constraints, 0),
            ],
            None,
        ),
        (
            "type guard reads both return types before comparison",
            TypeGuardType::unbound(&db, Type::Never),
            TypeGuardType::unbound(&db, Type::object()),
            true,
            &[(TypeOutput, 0), (TypeOutput, 0), (Constraints, 0)],
            None,
        ),
        (
            "reflexivity skips stored arguments",
            type_form(Type::Never),
            type_form(Type::Never),
            true,
            &[],
            None,
        ),
    ];
    for (name, source, target, expected, outputs, cached_outputs) in cases {
        let constraints = ConstraintSetBuilder::new();
        let relations = HasRelationToVisitor::default(&constraints);
        let disjointness = IsDisjointVisitor::default(&constraints);
        let signatures = SignatureRelationVisitor::default();
        let mapping = ApplyTypeMappingVisitor::new(&env);
        let checker = TypeRelationChecker::new(
            &env,
            TypeRelation::Assignability,
            &constraints,
            TypeVarSet::None,
            &relations,
            &disjointness,
            &signatures,
            &mapping,
        );
        let expected = ConstraintSet::from_bool(&constraints, expected);
        let audit = FieldAudit::new(&relations, None);
        let effects = InlinePairEffects {
            db: &db,
            dependencies: &audit,
        };
        let actual = complete(checker.check_type_pair_inner_with(source, target, &effects))?;
        assert!(actual.ownership_probe_same_set(expected), "{name}");
        let observed = audit.outputs.borrow().clone();
        assert_eq!(
            observed
                .iter()
                .map(|(_, output, active)| (*output, *active))
                .collect::<Vec<_>>(),
            outputs,
            "{name}",
        );
        assert_eq!(relations.ownership_probe_counts().0, 0, "{name}");

        if let Some(cached_outputs) = cached_outputs {
            let audit = FieldAudit::new(&relations, None);
            let effects = InlinePairEffects {
                db: &db,
                dependencies: &audit,
            };
            let actual = complete(checker.check_type_pair_inner_with(source, target, &effects))?;
            assert!(actual.ownership_probe_same_set(expected), "cached {name}");
            assert_eq!(
                audit
                    .outputs
                    .borrow()
                    .iter()
                    .map(|(_, output, active)| (*output, *active))
                    .collect::<Vec<_>>(),
                cached_outputs,
                "cached {name}",
            );
        }

        for refuse_at in 0..audit.calls.get() {
            let relations = HasRelationToVisitor::default(&constraints);
            let checker = TypeRelationChecker {
                relation_visitor: &relations,
                ..checker.clone()
            };
            let audit = FieldAudit::new(&relations, Some(refuse_at));
            let effects = InlinePairEffects {
                db: &db,
                dependencies: &audit,
            };
            assert!(
                complete(checker.check_type_pair_inner_with(source, target, &effects)).is_err(),
                "{name}: refusal {refuse_at}",
            );
            assert_eq!(audit.calls.get(), refuse_at + 1, "{name}");
            assert_eq!(
                *audit.outputs.borrow(),
                observed
                    .iter()
                    .copied()
                    .filter(|(index, _, _)| *index <= refuse_at)
                    .collect::<Vec<_>>(),
                "{name}: refusal {refuse_at}",
            );
            assert_eq!(relations.ownership_probe_counts().0, 0, "{name}");
            let retry = InlinePairEffects {
                db: &db,
                dependencies: OrdinaryDependencies,
            };
            let actual = complete(checker.check_type_pair_inner_with(source, target, &retry))?;
            assert!(actual.ownership_probe_same_set(expected), "retry {name}");
        }
    }
    Ok(())
}

#[test]
fn lazy_constraint_callbacks_preserve_short_circuits_and_refusal() -> anyhow::Result<()> {
    let db = database()?;
    let constraints = ConstraintSetBuilder::new();
    let audit = Audit::default();
    let effects = InlinePairEffects {
        db: &db,
        dependencies: &audit,
    };
    let children = Cell::new(0);
    let never = ConstraintSet::from_bool(&constraints, false);
    let always = ConstraintSet::from_bool(&constraints, true);
    let result = complete(never.and_with(
        &constraints,
        || async {
            children.set(children.get() + 1);
            Ok(always)
        },
        &effects,
    ))?;
    assert!(result.ownership_probe_same_set(never));
    let result = complete(always.or_with(
        &constraints,
        || async {
            children.set(children.get() + 1);
            Ok(never)
        },
        &effects,
    ))?;
    assert!(result.ownership_probe_same_set(always));
    assert_eq!(children.get(), 0);
    let result = complete([never, always].into_iter().when_all_with(
        &constraints,
        |value| {
            let children = &children;
            async move {
                children.set(children.get() + 1);
                Ok(value)
            }
        },
        &effects,
    ))?;
    assert!(result.ownership_probe_same_set(never));
    assert_eq!(children.get(), 1);
    let result = complete([always, always].into_iter().when_all_with(
        &constraints,
        |_| async {
            children.set(children.get() + 1);
            anyhow::bail!("child refused")
        },
        &effects,
    ));
    assert!(result.is_err());
    assert_eq!(children.get(), 2);
    Ok(())
}
