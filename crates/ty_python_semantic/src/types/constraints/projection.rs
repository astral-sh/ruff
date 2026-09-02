//! Bounded projections of correlated constraint solutions.

use rustc_hash::FxHashSet;

use super::{ConstraintSet, PathBound, PathBoundSolution, PathBounds, Solutions, TypeVarSolution};
use crate::types::typevar::TypeVarSet;
use crate::types::{Type, TypeVarVariance};
use crate::{Db, ProgramEnvironment};

/// Limits for one projection, including preprocessing, path collection, and its result.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SolutionBudget {
    /// Satisfied paths collected before per-variable solution selection can reject them.
    pub(crate) paths: usize,
    /// Interior and terminal visits, shared by preprocessing and path collection.
    pub(crate) visits: usize,
    /// Set-theoretic terms contributed to the result, including terms exposed by aliases.
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
            |_variance, path_bound| PathBounds::default_solve(db, env, builder, path_bound),
        )
    }

    fn bounded_path_bounds(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
        budget: SolutionBudget,
    ) -> Result<PathBounds<'db>, ProjectionError> {
        PathBounds::compute_bounded(
            db,
            env,
            &mut self.builder.storage.borrow_mut(),
            self.node,
            inferable,
            self.source_order,
            budget,
        )
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
        choose: impl FnMut(TypeVarVariance, &PathBound<'db>) -> PathBoundSolution<'db>,
    ) -> Result<Solutions<'db>, ProjectionError> {
        let path_bounds = self.bounded_path_bounds(db, env, inferable, budget)?;
        let mut type_budget = ProjectionTypeBudget::new(budget.type_terms);
        path_bounds.try_solve_with(choose, |solution| {
            for binding in solution {
                type_budget.charge_type(db, binding.solution)?;
            }
            Ok(())
        })
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
        choose: impl FnMut(TypeVarVariance, &PathBound<'db>) -> PathBoundSolution<'db>,
        initial: T,
        fold: impl FnMut(
            T,
            &[TypeVarSolution<'db>],
            &mut ProjectionTypeBudget,
        ) -> Result<T, ProjectionError>,
    ) -> Result<SolutionProjection<T>, ProjectionError> {
        let path_bounds = self.bounded_path_bounds(db, env, inferable, budget)?;

        path_bounds.try_fold_with(
            choose,
            initial,
            &mut ProjectionTypeBudget::new(budget.type_terms),
            fold,
        )
    }
}

impl<'db> PathBounds<'db> {
    fn try_fold_with<T>(
        &self,
        mut choose: impl FnMut(TypeVarVariance, &PathBound<'db>) -> PathBoundSolution<'db>,
        mut accumulated: T,
        budget: &mut ProjectionTypeBudget,
        mut fold: impl FnMut(
            T,
            &[TypeVarSolution<'db>],
            &mut ProjectionTypeBudget,
        ) -> Result<T, ProjectionError>,
    ) -> Result<SolutionProjection<T>, ProjectionError> {
        let paths = match self {
            Self::Unsatisfiable => return Ok(SolutionProjection::Unsatisfiable),
            Self::Unconstrained => return Ok(SolutionProjection::Unconstrained),
            Self::Constrained(paths) => paths,
        };

        let mut retained = false;
        for path in paths {
            let Some((solution, incomplete)) = Self::solve_path_with(path, &mut choose) else {
                continue;
            };
            if incomplete {
                return Err(ProjectionError::IncompleteSolution);
            }
            accumulated = fold(accumulated, &solution, budget)?;
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
