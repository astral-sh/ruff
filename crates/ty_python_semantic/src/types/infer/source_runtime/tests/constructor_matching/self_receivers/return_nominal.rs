//! Constructed nominal inputs exercise both retained return-callable mapping modes.
//! Source class inference supplies fixture metadata before the controlled mapping operation.

use rustc_hash::FxHashMap;

use super::*;
use crate::FxIndexMap;
use crate::place::global_symbol;
use crate::types::generics::return_locations::TypeVarLocations;
use crate::types::generics::return_scoping::ReturnScopeEffects;
use crate::types::mapping::OwnedTypeMapping;
use crate::types::mapping::return_callables::{
    ReturnCallableReplacements, ReturnTypevarReplacements,
};
use crate::types::{ApplySpecialization, CallableType, GenericAlias, Specialization};

const SOURCE: &str = "from typing import Any\nclass Product[T]:\n    def target(self): ...\nclass Plain: pass\nclass AnyBase[T](Any): pass\n";

/// Selects type-variable renaming or replacement of complete callable handles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    ReturnCallables,
    RescopeReturnCallables,
}

/// Carries optional constructed replacements into the real retained-map storage providers.
#[derive(Clone, Copy, Debug)]
enum Replacement<'db> {
    Variable(Option<(BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>)>),
    Callable(Option<(CallableType<'db>, CallableType<'db>)>),
}

/// Returns both the mapped type and the observed identity of its retained replacement-map owner.
#[derive(Debug)]
struct Mapped<'db> {
    ty: Type<'db>,
    mapping: OwnedMappingSnapshot,
}

/// Applies one retained return mapping after the production providers construct its map.
#[derive(Clone, Copy, Debug)]
struct Mapping<'db> {
    input: Type<'db>,
    replacement: Replacement<'db>,
}

impl<'db> MemberOperation<'db> for Mapping<'db> {
    type Output = Mapped<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        let mapping = match self.replacement {
            Replacement::Variable(pair) => {
                let mut values = ReturnScopeEffects::renamings(&effects, &[]).await?;
                if let Some((original, renamed)) = pair {
                    ReturnScopeEffects::insert_renaming(&effects, &mut values, original, renamed)
                        .await?;
                }
                let retained = ReturnScopeEffects::retain_renamings(&effects, values).await?;
                effects
                    .local_with_fixed_transfers(1, 0, || {
                        OwnedTypeMapping::ReturnCallables(retained)
                    })
                    .await?
            }
            Replacement::Callable(pair) => {
                let locations = effects
                    .local_with_fixed_transfers(2, 0, TypeVarLocations::default)
                    .await?;
                let mut state = ReturnScopeEffects::state(&effects, locations).await?;
                if let Some((original, replacement)) = pair {
                    ReturnScopeEffects::insert_replacement(
                        &effects,
                        &mut state,
                        original,
                        replacement,
                    )
                    .await?;
                }
                let retained =
                    ReturnScopeEffects::retain_replacements(&effects, &mut state).await?;
                effects
                    .local_with_fixed_transfers(1, 0, || {
                        OwnedTypeMapping::RescopeReturnCallables(retained)
                    })
                    .await?
            }
        };
        let observed = effects
            .local_with_fixed_transfers(2, 0, || OwnedMappingSnapshot::from(mapping))
            .await?;
        let ty = effects.apply_mapping(self.input, program, mapping).await?;
        effects
            .local_with_fixed_transfers(2, 0, || Mapped {
                ty,
                mapping: observed,
            })
            .await
    }
}

impl<'db> Mapping<'db> {
    /// Runs ordinary mapping after controlled completion using equivalent borrowed replacements.
    fn ordinary(self, db: &'db TestDb, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match self.replacement {
            Replacement::Variable(pair) => {
                let values = FxIndexMap::from_iter(pair);
                self.input.apply_type_mapping(
                    db,
                    env,
                    &TypeMapping::ApplySpecialization(ApplySpecialization::ReturnCallables(
                        ReturnTypevarReplacements::Borrowed(&values),
                    )),
                    TypeContext::default(),
                )
            }
            Replacement::Callable(pair) => {
                let values = FxHashMap::from_iter(pair);
                self.input.apply_type_mapping(
                    db,
                    env,
                    &TypeMapping::RescopeReturnCallables(ReturnCallableReplacements::Borrowed(
                        &values,
                    )),
                    TypeContext::default(),
                )
            }
        }
    }
}

/// Selects a plain non-generic nominal or nested generic nominals with an ordinary/explicit-Any root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Shape {
    NonGeneric,
    Generic,
    ExplicitAny,
}

/// Fetches source class metadata for an internal constructed-input control.
fn class<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    name: &str,
) -> ClassLiteral<'db> {
    global_symbol(db, prepared.program_file(), name)
        .place
        .ignore_possibly_undefined()
        .and_then(Type::as_class_literal)
        .expect("nominal fixture class")
}

/// Constructs a nominal instance with exactly one stored generic argument.
fn generic_instance<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    class: ClassLiteral<'db>,
    argument: Type<'db>,
) -> Type<'db> {
    let origin = class.as_static().expect("static nominal fixture class");
    let context = origin
        .generic_context(db)
        .expect("generic nominal fixture class");
    let specialization = Specialization::new(db, context, Box::from([argument]), None, None);
    Type::instance(
        db,
        env,
        ClassType::Generic(GenericAlias::new(db, origin, specialization)),
    )
}

/// Both return modes traverse non-generic nominals even with empty maps. Populated maps replace
/// a variable or callable inside two generic aliases while every nested child retains the original
/// visitor and map owner. Explicit-Any inheritance remains represented on the rebuilt instance.
#[test_case::test_case(Mode::ReturnCallables, Shape::NonGeneric; "renaming empty non-generic")]
#[test_case::test_case(Mode::RescopeReturnCallables, Shape::NonGeneric; "callable empty non-generic")]
#[test_case::test_case(Mode::ReturnCallables, Shape::Generic; "renaming nested generic")]
#[test_case::test_case(Mode::RescopeReturnCallables, Shape::Generic; "callable nested generic")]
#[test_case::test_case(Mode::ReturnCallables, Shape::ExplicitAny; "renaming explicit Any")]
#[test_case::test_case(Mode::RescopeReturnCallables, Shape::ExplicitAny; "callable explicit Any")]
fn return_nominal_modes_preserve_results_and_retained_owners(mode: Mode, shape: Shape) {
    let db = database(SOURCE);
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let product = class(&db, &prepared, "Product");
    let context = product.generic_context(&db).expect("Product context");
    let original = context.variables(&db).next().expect("Product variable");
    let renamed = BoundTypeVarInstance::new(
        &db,
        original.typevar(&db),
        original.binding_context(&db),
        None,
        original.freshness(&db).increment(),
    );
    let original_callable = CallableType::single(
        &db,
        Signature::new(Parameters::empty(), Type::TypeVar(original)),
    );
    let replacement_callable = CallableType::single(
        &db,
        Signature::new(Parameters::empty(), Type::TypeVar(renamed)),
    );
    let (argument, replacement_argument, replacement) = match mode {
        Mode::ReturnCallables => (
            Type::TypeVar(original),
            Type::TypeVar(renamed),
            Replacement::Variable((shape != Shape::NonGeneric).then_some((original, renamed))),
        ),
        Mode::RescopeReturnCallables => (
            Type::Callable(original_callable),
            Type::Callable(replacement_callable),
            Replacement::Callable(
                (shape != Shape::NonGeneric).then_some((original_callable, replacement_callable)),
            ),
        ),
    };
    let outer = if shape == Shape::ExplicitAny {
        class(&db, &prepared, "AnyBase")
    } else {
        product
    };
    let input = match shape {
        Shape::NonGeneric => Type::instance(
            &db,
            &env,
            ClassType::NonGeneric(class(&db, &prepared, "Plain")),
        ),
        Shape::Generic | Shape::ExplicitAny => {
            let inner = generic_instance(&db, &env, product, argument);
            generic_instance(&db, &env, outer, inner)
        }
    };
    let request = Mapping { input, replacement };
    let revision = salsa::plumbing::current_revision(&db);
    mapping_observations::reset(None);
    let result = controlled_member_operation(&prepared, request, &funded());
    let Ok(AnalysisOutcome::Complete(actual)) = result else {
        panic!("return nominal mapping: {result:?}");
    };
    let expected = match shape {
        Shape::NonGeneric => input,
        Shape::Generic | Shape::ExplicitAny => {
            let expected_inner = generic_instance(&db, &env, product, replacement_argument);
            generic_instance(&db, &env, outer, expected_inner)
        }
    };
    assert_eq!(actual.ty, expected);
    assert_eq!(actual.ty, request.ordinary(&db, &env));
    let Type::NominalInstance(instance) = actual.ty else {
        panic!("expected nominal result");
    };
    assert_eq!(
        instance.inherits_from_explicit_any(),
        shape == Shape::ExplicitAny
    );
    let snapshot = mapping_observations::mapping_snapshot();
    assert_eq!(snapshot.root_count, 1);
    let root = snapshot.roots[0].expect("nominal mapping root");
    assert_eq!(root.mapping, actual.mapping);
    assert_eq!(
        root.program,
        Some(prepared.program_file().program(&db).as_id()),
        "{snapshot:?}",
    );
    if shape != Shape::NonGeneric {
        assert_ne!(actual.ty, input);
        assert!(snapshot.child_count >= 2);
    }
    assert!(
        snapshot.children[..snapshot.child_count]
            .iter()
            .flatten()
            .all(|child| child.mapping == actual.mapping
                && child.visitor == root.visitor),
        "{snapshot:?}",
    );
    // Recursive-child observations record visitor and map identities, but no program identity.
    //
    assert!(
        snapshot.children[..snapshot.child_count]
            .iter()
            .flatten()
            .all(|child| child.program.is_none()),
        "{snapshot:?}",
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
