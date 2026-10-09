//! Bounded projections of correlated constraint solutions.

use rustc_hash::FxHashSet;

use super::{
    CandidateSolutions, CandidateTypeVarSolution, ConstraintSet, PathBoundSolution, SolutionPaths,
    Solutions, TypeVarSolution,
};
use crate::types::typevar::TypeVarSet;
use crate::types::{BoundTypeVarIdentity, Type, TypeVarVariance};
use crate::{Db, FxOrderMap, ProgramEnvironment};

/// Limits for one projection, including preprocessing, path collection, and its result.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SolutionBudget {
    /// Satisfied paths collected before per-variable solution selection can reject them.
    pub(crate) paths: usize,
    /// Interior and terminal visits, shared by preprocessing and path collection.
    pub(crate) visits: usize,
    /// Set-theoretic terms contributed to the result, including terms exposed by aliases.
    /// Also bounds storage when retaining alternatives.
    pub(crate) type_terms: usize,
}

impl Default for SolutionBudget {
    fn default() -> Self {
        // Allow long, simple conjunctions and sizable existing unions without allowing their
        // alternatives to expand into an equally large family of specializations.
        Self {
            paths: 4_096,
            visits: 32_768,
            type_terms: 8_192,
        }
    }
}

/// Why an exact projection could not be completed.
///
/// None of these outcomes proves that the constraint set is unsatisfiable. In particular, a
/// caller must not use the prefix visited before a limit was reached as the complete answer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProjectionError {
    PathBudgetExceeded,
    TraversalBudgetExceeded,
    TypeBudgetExceeded,
    IncompleteSolution,
}

/// An exact projection of all retained solution paths.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum SolutionProjection<T> {
    Unsatisfiable,
    Unconstrained,
    Constrained(T),
}

/// A shared limit on the type terms consumed while constructing a projection.
///
/// Union projections charge each contribution before adding it. An intersection projection must
/// additionally use `IntersectionType::bounded_from_elements`, since distributing intersections
/// over unions can multiply, rather than add, the number of terms.
pub(crate) struct ProjectionTypeBudget {
    remaining: usize,
}

impl ProjectionTypeBudget {
    fn new(remaining: usize) -> Self {
        Self { remaining }
    }

    /// Charges the set-theoretic terms that a type constructor may flatten or inspect. Aliases
    /// are included so a large union cannot evade the limit by being hidden behind a name.
    pub(crate) fn charge_type<'db>(
        &mut self,
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> Result<(), ProjectionError> {
        self.charge_type_inner(db, ty, &mut FxHashSet::default())
    }

    fn charge_type_inner<'db>(
        &mut self,
        db: &'db dyn Db,
        ty: Type<'db>,
        seen_aliases: &mut FxHashSet<Type<'db>>,
    ) -> Result<(), ProjectionError> {
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or(ProjectionError::TypeBudgetExceeded)?;
        match ty {
            Type::Union(union) => {
                for element in union.elements(db) {
                    self.charge_type_inner(db, *element, seen_aliases)?;
                }
            }
            Type::Intersection(intersection) => {
                for element in intersection
                    .iter_positive(db)
                    .chain(intersection.iter_negative(db))
                {
                    self.charge_type_inner(db, element, seen_aliases)?;
                }
            }
            Type::TypeAlias(alias) if seen_aliases.insert(ty) => {
                self.charge_type_inner(db, alias.value_type(db), seen_aliases)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl<'db> ConstraintSet<'db, '_> {
    /// Finds equalities that hold in every specialization still permitted by this proof.
    ///
    /// An unresolved relation can restrict a variable to one exact type without proving that a
    /// valid specialization exists. Projecting its possible bound retains these necessary
    /// equalities while leaving the original relation responsible for checking validity.
    pub(in crate::types) fn necessary_bindings(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
    ) -> Result<Box<[TypeVarSolution<'db>]>, ProjectionError> {
        let budget = SolutionBudget::default();
        let candidates = CandidateSolutions::compute_bounded(
            db,
            env,
            &mut self.builder.storage.borrow_mut(),
            self.possible_node,
            inferable,
            self.source_order,
            budget,
        )?;
        let candidates = match candidates {
            CandidateSolutions::Constrained(candidates)
            | CandidateSolutions::Incomplete(candidates) => candidates,
            CandidateSolutions::Unsatisfiable | CandidateSolutions::Unconstrained => {
                return Ok(Box::default());
            }
        };
        // Candidate validation may remain unresolved. We retain every candidate here, including
        // candidates marked invalid, and require a positive equality proof on every path. Thus
        // validity cannot remove an alternative and manufacture an apparent consensus.
        let mut common: Option<FxOrderMap<BoundTypeVarIdentity<'db>, TypeVarSolution<'db>>> = None;
        let mut type_budget = ProjectionTypeBudget::new(budget.type_terms);
        for candidate in &candidates {
            let mut exact = FxOrderMap::default();
            for bounds in &candidate.typevars {
                if let Some(solution) = bounds.as_exact(db, env, self.builder) {
                    type_budget.charge_type(db, solution)?;
                    exact.insert(
                        bounds.bound_typevar.identity(db),
                        TypeVarSolution {
                            bound_typevar: bounds.bound_typevar,
                            solution,
                        },
                    );
                }
            }
            match &mut common {
                Some(common) => common.retain(|identity, binding| {
                    exact
                        .get(identity)
                        .is_some_and(|other| other.solution == binding.solution)
                }),
                None => common = Some(exact),
            }
        }
        Ok(common
            .into_iter()
            .flat_map(FxOrderMap::into_values)
            .collect())
    }

    /// Computes default solutions for each BDD path within the default projection budget.
    pub(crate) fn solutions(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
    ) -> Result<Solutions<'db>, ProjectionError> {
        let builder = self.builder;
        self.solutions_with(
            db,
            env,
            inferable,
            SolutionBudget::default(),
            |_variance, path_bound| CandidateSolutions::default_solve(db, env, builder, path_bound),
        )
    }

    fn bounded_path_bounds(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
        budget: SolutionBudget,
    ) -> Result<CandidateSolutions<'db>, ProjectionError> {
        let candidates = self.inference_path_bounds(db, env, inferable, budget)?;
        match candidates {
            CandidateSolutions::Incomplete(_) => Err(ProjectionError::IncompleteSolution),
            candidates => Ok(candidates),
        }
    }

    fn inference_path_bounds(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
        budget: SolutionBudget,
    ) -> Result<CandidateSolutions<'db>, ProjectionError> {
        let mut storage = self.builder.storage.borrow_mut();
        let incomplete_before = self.builder.relation_session.incomplete_epoch();
        let result = CandidateSolutions::compute_bounded(
            db,
            env,
            &mut storage,
            self.possible_node,
            inferable,
            self.source_order,
            budget,
        )?;
        if !self.is_complete()
            || self.builder.relation_session.incomplete_epoch() != incomplete_before
        {
            Ok(result.with_incomplete_validity())
        } else {
            Ok(result)
        }
    }

    /// Computes solutions using a caller-provided selector within the given projection budget.
    ///
    /// The selector receives the typevar's variance and explicit lower and upper bounds. Its
    /// outcome distinguishes missing evidence, invalid paths, and exhausted solution budgets.
    /// The caller is responsible for combining the resulting paths (typically via union).
    ///
    /// Per-variable budget exhaustion preserves available fallback bindings and marks the path
    /// family as [`SolutionPaths::BudgetExceeded`](super::SolutionPaths::BudgetExceeded).
    /// Exhausting a limit in the supplied [`SolutionBudget`] instead returns an error without a
    /// partial path family.
    pub(crate) fn solutions_with(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
        budget: SolutionBudget,
        choose: impl FnMut(TypeVarVariance, &CandidateTypeVarSolution<'db>) -> PathBoundSolution<'db>,
    ) -> Result<Solutions<'db>, ProjectionError> {
        let solutions = self.inference_solutions_with(db, env, inferable, budget, choose)?;
        match solutions {
            Solutions::Constrained(SolutionPaths::Incomplete(_))
            | Solutions::Unsatisfiable(SolutionPaths::Incomplete(_)) => {
                Err(ProjectionError::IncompleteSolution)
            }
            solutions => Ok(solutions),
        }
    }

    /// Retains selected types even when checking the candidate paths leaves a recursive proof
    /// unresolved. Such paths remain explicitly incomplete and cannot establish compatibility
    /// or an exact projection. This lets ordinary inference preserve its original evidence.
    pub(crate) fn inference_solutions_with(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
        budget: SolutionBudget,
        choose: impl FnMut(TypeVarVariance, &CandidateTypeVarSolution<'db>) -> PathBoundSolution<'db>,
    ) -> Result<Solutions<'db>, ProjectionError> {
        let path_bounds = self.inference_path_bounds(db, env, inferable, budget)?;
        let mut type_budget = ProjectionTypeBudget::new(budget.type_terms);
        let incomplete_before = self.builder.relation_session.incomplete_epoch();
        let result = path_bounds.try_solve_with(choose, |solution| {
            for violation in solution.violations() {
                for evidence in violation.evidence_types() {
                    type_budget.charge_type(db, *evidence)?;
                }
            }
            for binding in &solution.solved_typevars {
                type_budget.charge_type(db, binding.solution)?;
            }
            Ok(())
        })?;
        if self.builder.relation_session.incomplete_epoch() != incomplete_before {
            Ok(result.with_incomplete_validity())
        } else {
            Ok(result)
        }
    }

    /// Folds complete, correlated solutions without first allocating every solved path.
    ///
    /// Raw paths are collected within the traversal limits and sorted in the same source order
    /// as [`Self::solutions_with`]. The storage borrow is released before invoking either
    /// callback, so they can safely use the constraint builder. Each call to `fold` receives the
    /// complete bindings for one retained path, including an empty slice for a valid path on
    /// which no variable was solved.
    ///
    /// The accumulator is returned only if the entire projection succeeds. `fold` must charge
    /// newly accumulated types to its supplied budget and use bounded constructors for operations
    /// that can expand them. It should combine alternatives commutatively when their order is not
    /// meaningful to its consumer. Existing limitations in solution extraction still apply; this
    /// API does not make an order-sensitive selector or fold order-independent.
    #[expect(clippy::too_many_arguments)]
    pub(crate) fn try_fold_solutions<T>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
        budget: SolutionBudget,
        choose: impl FnMut(TypeVarVariance, &CandidateTypeVarSolution<'db>) -> PathBoundSolution<'db>,
        initial: T,
        fold: impl FnMut(
            T,
            &[TypeVarSolution<'db>],
            &mut ProjectionTypeBudget,
        ) -> Result<T, ProjectionError>,
    ) -> Result<SolutionProjection<T>, ProjectionError> {
        let path_bounds = self.bounded_path_bounds(db, env, inferable, budget)?;

        let incomplete_before = self.builder.relation_session.incomplete_epoch();
        let result = path_bounds.try_fold_with(
            choose,
            initial,
            &mut ProjectionTypeBudget::new(budget.type_terms),
            fold,
        )?;
        if self.builder.relation_session.incomplete_epoch() != incomplete_before {
            return Err(ProjectionError::IncompleteSolution);
        }
        Ok(result)
    }
}

impl<'db> CandidateSolutions<'db> {
    fn try_fold_with<T>(
        &self,
        mut choose: impl FnMut(
            TypeVarVariance,
            &CandidateTypeVarSolution<'db>,
        ) -> PathBoundSolution<'db>,
        mut accumulated: T,
        budget: &mut ProjectionTypeBudget,
        mut fold: impl FnMut(
            T,
            &[TypeVarSolution<'db>],
            &mut ProjectionTypeBudget,
        ) -> Result<T, ProjectionError>,
    ) -> Result<SolutionProjection<T>, ProjectionError> {
        let candidates = match self {
            Self::Incomplete(_) => return Err(ProjectionError::IncompleteSolution),
            Self::Unsatisfiable => return Ok(SolutionProjection::Unsatisfiable),
            Self::Unconstrained => return Ok(SolutionProjection::Unconstrained),
            Self::Constrained(candidates) => candidates,
        };

        let mut retained = false;
        for candidate in candidates {
            let Some((solution, incomplete)) = Self::solve_path_with(candidate, &mut choose) else {
                continue;
            };
            if !solution.is_valid() {
                continue;
            }
            if incomplete {
                return Err(ProjectionError::IncompleteSolution);
            }
            accumulated = fold(accumulated, &solution.solved_typevars, budget)?;
            retained = true;
        }

        Ok(if retained {
            SolutionProjection::Constrained(accumulated)
        } else {
            SolutionProjection::Unsatisfiable
        })
    }
}

#[cfg(test)]
mod tests;
