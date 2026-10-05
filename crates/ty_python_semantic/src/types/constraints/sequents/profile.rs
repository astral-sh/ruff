//! Finite key payloads and passive retirement for the canonical sequent queries.

use salsa::execution_probe::{PassiveMemoProfile, QueryKeyProfile};
use salsa::plumbing::function::{Configuration, InternedQueryConfiguration};
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::{SequentGroup, SequentMap};
use crate::types::constraints::variables::Constraint;
use crate::{Db, Program};

pub(in crate::types::constraints) struct SingleSequentProfile;

impl<C> PassiveMemoProfile<C> for SingleSequentProfile
where
    C: for<'a> Configuration<Output<'a> = SequentMap<'a>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        output.retirement_work()
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        output.retirement_work_with(&mut || fuel.consume(1))
    }
}

impl<C> QueryKeyProfile<C> for SingleSequentProfile
where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Program<'a>, Constraint<'a>)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
{
    fn input_work<'db>(
        input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
    ) -> Option<usize> {
        constraint_input_work(input.1)
    }

    fn input_work_bounded<'db>(
        input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as QueryKeyProfile<C>>::input_work(input).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types::constraints) struct PairSequentProfile;

impl<C> PassiveMemoProfile<C> for PairSequentProfile
where
    C: for<'a> Configuration<Output<'a> = SequentMap<'a>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        output.retirement_work()
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        output.retirement_work_with(&mut || fuel.consume(1))
    }
}

impl<C> QueryKeyProfile<C> for PairSequentProfile
where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (Program<'a>, Constraint<'a>, Constraint<'a>),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
{
    fn input_work<'db>(
        input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
    ) -> Option<usize> {
        constraint_input_work(input.1)?.checked_add(constraint_input_work(input.2)?)
    }

    fn input_work_bounded<'db>(
        input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as QueryKeyProfile<C>>::input_work(input).ok_or(QuoteError::Overflow)
    }
}

fn constraint_input_work(constraint: Constraint<'_>) -> Option<usize> {
    // These are shallow copies of the inline bound and typevar handles, not resolved types.
    constraint
        .type_pair()
        .into_iter()
        .try_fold(0usize, |work, ty| {
            work.checked_add(ty.inline_payload_bytes())
        })
}

impl SequentMap<'_> {
    fn retirement_work(&self) -> Option<usize> {
        self.retirement_work_with(&mut || Ok(())).ok()
    }

    fn retirement_work_with(
        &self,
        admit: &mut impl FnMut() -> Result<(), QuoteError>,
    ) -> Result<usize, QuoteError> {
        admit()?;
        // Sequents contain Copy constraints. Their handles and borrowed Todo labels do not
        // own semantic children; only these vectors and boxed slices need to be released.
        self.sequents.iter().try_fold(
            self.sequents
                .len()
                .checked_add(self.pending.len())
                .ok_or(QuoteError::Overflow)?,
            |work, group| {
                admit()?;
                match group {
                    SequentGroup::Ungrouped(sequents) => work.checked_add(sequents.len()),
                    SequentGroup::Grouped {
                        leftwards,
                        rightwards,
                        ..
                    } => work
                        .checked_add(leftwards.len())
                        .and_then(|work| work.checked_add(rightwards.len())),
                }
                .ok_or(QuoteError::Overflow)
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::name::Name;
    use salsa::attempt_probe::{AttemptOutcome, try_with_attempt};
    use salsa::execution_probe::{ExecutionAdmission, ExecutionWork, RegistryBuilder, RunResult};
    use salsa::plumbing::function::IngredientImpl;

    use super::*;
    use crate::db::tests::{TestDb, setup_db};
    use crate::types::constraints::sequents::{
        Sequent, pair_sequent_ingredient, single_sequent_ingredient,
    };
    use crate::types::constraints::variables::{
        ConcreteEquivalenceBound, ConcreteLowerBound, ConcreteUpperBound, ConstraintProvenance,
        TypeVarEquivalenceBound, TypeVarRangeBound,
    };
    use crate::types::subclass_of::SubclassOfInner;
    use crate::types::{BoundTypeVarInstance, SubclassOfType, Type, TypeVarVariance, todo_type};

    fn variable<'db>(db: &'db TestDb, name: &'static str) -> BoundTypeVarInstance<'db> {
        BoundTypeVarInstance::synthetic(
            db,
            &db.program_environment(),
            Name::new_static(name),
            TypeVarVariance::Invariant,
        )
    }

    fn single_quote<'db, C>(
        _ingredient: &IngredientImpl<C>,
        input: (Program<'db>, Constraint<'db>),
    ) -> Option<usize>
    where
        C: InternedQueryConfiguration
            + for<'a> salsa::plumbing::interned::Configuration<
                Fields<'a> = (Program<'a>, Constraint<'a>),
            > + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    {
        <SingleSequentProfile as QueryKeyProfile<C>>::input_work(&input)
    }

    fn pair_quote<'db, C>(
        _ingredient: &IngredientImpl<C>,
        input: (Program<'db>, Constraint<'db>, Constraint<'db>),
    ) -> Option<usize>
    where
        C: InternedQueryConfiguration
            + for<'a> salsa::plumbing::interned::Configuration<
                Fields<'a> = (Program<'a>, Constraint<'a>, Constraint<'a>),
            > + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    {
        <PairSequentProfile as QueryKeyProfile<C>>::input_work(&input)
    }

    fn output_quote<C, P>(_ingredient: &IngredientImpl<C>, map: &SequentMap<'_>) -> Option<usize>
    where
        C: for<'a> Configuration<Output<'a> = SequentMap<'a>>,
        P: PassiveMemoProfile<C>,
    {
        P::retired_output_work(map)
    }

    #[test]
    fn concrete_and_typevar_fields_have_exact_inline_quotes() {
        let db = setup_db();
        let mut reader = db.clone();
        let env = db.program_environment();
        let program = env.program(&db);
        let t = variable(&db, "T");
        let u = variable(&db, "U");
        let Type::Dynamic(todo) = todo_type!("nested-label") else {
            panic!("Todo fixture must be dynamic");
        };
        let nested = SubclassOfType::from(&db, &env, SubclassOfInner::Dynamic(todo));
        let bounds = [
            (
                todo_type!("direct"),
                if cfg!(debug_assertions) { 6 } else { 0 },
            ),
            (nested, if cfg!(debug_assertions) { 12 } else { 0 }),
            (Type::unknown(), 0),
        ];
        let range = Constraint::from(TypeVarRangeBound::new(
            &db,
            ConstraintProvenance::Evidence,
            t,
            u,
        ));
        let equivalence = Constraint::from(TypeVarEquivalenceBound::new(
            &db,
            ConstraintProvenance::Validity,
            t,
            u,
        ));
        let single = single_sequent_ingredient(&db);
        let pair = pair_sequent_ingredient(&db);
        reader.clear_salsa_events();
        for (bound, expected) in bounds {
            for provenance in [
                ConstraintProvenance::Evidence,
                ConstraintProvenance::Validity,
            ] {
                for constraint in [
                    Constraint::from(ConcreteLowerBound::new(provenance, t, bound)),
                    Constraint::from(ConcreteUpperBound::new(provenance, t, bound)),
                    Constraint::from(ConcreteEquivalenceBound::new(provenance, t, bound)),
                ] {
                    assert_eq!(single_quote(single, (program, constraint)), Some(expected));
                    assert_eq!(
                        pair_quote(pair, (program, constraint, constraint)),
                        Some(expected * 2)
                    );
                    assert_eq!(
                        pair_quote(pair, (program, constraint, range)),
                        Some(expected)
                    );
                    assert_eq!(
                        pair_quote(pair, (program, equivalence, constraint)),
                        Some(expected)
                    );
                }
            }
        }
        assert_eq!(single_quote(single, (program, range)), Some(0));
        assert_eq!(single_quote(single, (program, equivalence)), Some(0));
        assert_eq!(pair_quote(pair, (program, range, equivalence)), Some(0));
        assert!(reader.take_salsa_events().is_empty());
    }

    #[test]
    fn retirement_quotes_all_owned_group_shapes_and_pending_sequents() {
        let db = setup_db();
        let mut reader = db.clone();
        let t = variable(&db, "T");
        let u = variable(&db, "U");
        let left = Constraint::from(ConcreteLowerBound::new(
            ConstraintProvenance::Evidence,
            t,
            Type::unknown(),
        ));
        let right = Constraint::from(ConcreteUpperBound::new(
            ConstraintProvenance::Validity,
            u,
            Type::unknown(),
        ));
        let equivalence = TypeVarEquivalenceBound::new(&db, ConstraintProvenance::Evidence, t, u);
        let sequents = [
            Sequent::SingleTautology { ante: left },
            Sequent::PairImpossibility {
                ante1: left,
                ante2: right,
            },
            Sequent::TripleImpossibility {
                ante1: left,
                ante2: right,
                ante3: left,
            },
            Sequent::SingleImplication {
                ante: left,
                post: right,
                fuel_cost: (),
            },
            Sequent::PairImplication {
                ante1: left,
                ante2: right,
                post: left,
                fuel_cost: (),
            },
        ];
        let ungrouped = || SequentGroup::Ungrouped(Box::from(&sequents[..2]));
        let grouped = || SequentGroup::Grouped {
            equivalence,
            leftwards: Box::from(&sequents[..1]),
            rightwards: Box::from(&sequents[1..3]),
        };
        let maps = [
            (SequentMap::default(), 0),
            (
                SequentMap {
                    sequents: vec![ungrouped()],
                    pending: Vec::new(),
                },
                3,
            ),
            (
                SequentMap {
                    sequents: vec![grouped()],
                    pending: Vec::new(),
                },
                4,
            ),
            (
                SequentMap {
                    sequents: Vec::new(),
                    pending: sequents.to_vec(),
                },
                5,
            ),
            (
                SequentMap {
                    sequents: vec![ungrouped(), grouped()],
                    pending: sequents.to_vec(),
                },
                12,
            ),
        ];
        let single = single_sequent_ingredient(&db);
        let pair = pair_sequent_ingredient(&db);
        reader.clear_salsa_events();
        for (map, expected) in maps {
            assert_eq!(map.retirement_work(), Some(expected));
            assert_eq!(
                output_quote::<_, SingleSequentProfile>(single, &map),
                Some(expected)
            );
            assert_eq!(
                output_quote::<_, PairSequentProfile>(pair, &map),
                Some(expected)
            );
        }
        assert!(reader.take_salsa_events().is_empty());
    }

    struct Admission;

    impl ExecutionAdmission for Admission {
        fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
            Ok(())
        }
    }

    #[test]
    fn actual_sequent_ingredients_register_with_both_profile_factories() {
        let db = setup_db();
        let mut reader = db.clone();
        let single = single_sequent_ingredient(&db);
        let pair = pair_sequent_ingredient(&db);
        let admission = Admission;
        reader.clear_salsa_events();
        for callable in [false, true] {
            let result = try_with_attempt(&db, usize::MAX, || {
                let mut registry = RegistryBuilder::new(&db, &admission)?;
                if callable {
                    let single = registry.reserve_callable(&db as &dyn Db, single)?;
                    let pair = registry.reserve_callable(&db as &dyn Db, pair)?;
                    let _single_keys =
                        registry.callable_query_keys::<_, SingleSequentProfile>(&single)?;
                    let _pair_keys =
                        registry.callable_query_keys::<_, PairSequentProfile>(&pair)?;
                } else {
                    let single = registry.reserve(&db as &dyn Db, single)?;
                    let pair = registry.reserve(&db as &dyn Db, pair)?;
                    let _single_keys = registry.query_keys::<_, SingleSequentProfile>(&single)?;
                    let _pair_keys = registry.query_keys::<_, PairSequentProfile>(&pair)?;
                }
                Ok::<(), salsa::execution_probe::RunError>(())
            });
            assert_eq!(result, Ok(AttemptOutcome::Complete(Ok(()))));
        }
        assert!(
            reader
                .take_salsa_events()
                .iter()
                .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
        );
    }
}
