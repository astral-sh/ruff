use std::cell::RefCell;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::ops::ControlFlow;

use crate::types::constraints::paths::PathAssignments;
use crate::types::constraints::projection::ProjectionTypeBudget;
use crate::types::constraints::{
    ALWAYS_FALSE, ALWAYS_TRUE, ConstraintBound, ConstraintBoundsBuilder, ConstraintId,
    ConstraintSetBuilder, ConstraintSetStorage, NodeId, PathBound, PathBounds, ProjectionError,
    SolutionBudget, SolutionLimits, Solutions, TypeVarSolution,
};
use crate::types::graph::DependencyGraph;
use crate::types::typevar::TypeVarSet;
use crate::types::{
    BoundTypeVarInstance, GenericContext, IntersectionBuilder, IntersectionType, RecursiveType,
    Specialization, Type, UnionType, any_over_type_including_alias_arguments,
};
use crate::{Db, FxIndexMap, FxIndexSet, ProgramEnvironment};

impl<'db> TypeVarSolution<'db> {
    /// Record dependencies in selected solution types, including alias arguments.
    fn dependency_graph(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        solution: &[TypeVarSolution<'db>],
    ) -> DependencyGraph {
        let variables: FxIndexMap<_, _> = solution
            .iter()
            .enumerate()
            .map(|(index, binding)| (binding.bound_typevar.identity(db), index))
            .collect();
        let mut dependencies = Vec::with_capacity(solution.len());
        for binding in solution {
            let found = RefCell::new(FxIndexSet::default());
            any_over_type_including_alias_arguments(db, env, binding.solution, |ty| {
                if let Type::TypeVar(typevar) = ty
                    && let Some(dependency) = variables.get(&typevar.identity(db))
                {
                    found.borrow_mut().insert(*dependency);
                }
                false
            });
            let found = found.into_inner();
            dependencies.push(found.into_iter().collect());
        }
        DependencyGraph::new(dependencies)
    }

    /// Solve one defining equation per variable using the shared path solver.
    /// The equations already form one conjunction; discovering alternative constraint
    /// paths would only derive redundant consequences before closing their cycles.
    pub(in crate::types) fn solve_equations(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        equations: &[Self],
    ) -> Result<Solutions<'db>, ProjectionError> {
        let inferable =
            TypeVarSet::from_typevars(db, equations.iter().map(|equation| equation.bound_typevar));
        let bounds = PathBounds::Constrained(
            Box::new([equations
                .iter()
                .map(|equation| PathBound::exact(equation.bound_typevar, equation.solution))
                .collect()]),
            inferable,
        );
        let builder = ConstraintSetBuilder::new();
        let mut budget = ProjectionTypeBudget::new(SolutionBudget::default().type_terms);
        bounds.try_solve_with(
            db,
            env,
            |_variance, bound| PathBounds::default_solve(db, env, &builder, bound),
            |solution| {
                for binding in solution {
                    budget.charge_type(db, binding.solution)?;
                }
                Ok(())
            },
        )
    }

    /// Solve dependencies within one path, preserving correlations between paths.
    /// Acyclic bindings are substituted in dependency order. Remaining equations
    /// are closed together as shared recursive graphs.
    /// Returns whether equality simplification or recursive closure requires revalidating bounds.
    pub(super) fn resolve_dependencies(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        solution: &mut [Self],
        bounds: &[PathBound<'db>],
        inferable: TypeVarSet<'db>,
    ) -> bool {
        let bounds: FxIndexMap<_, _> = bounds
            .iter()
            .map(|bound| (bound.bound_typevar, bound))
            .collect();
        // Pin equalities before substitution, including finite dependents of a cycle.
        // Otherwise equal variables can retain different expansions of the same equation.
        let mut simplified = false;
        for binding in solution.iter_mut() {
            if binding.bound_typevar.is_inferable(db, inferable)
                && let Some(bound) = bounds.get(&binding.bound_typevar)
            {
                let equation = bound.simplify_equation(db, env, binding.solution);
                simplified |= equation != binding.solution;
                binding.solution = equation;
            }
        }
        let graph = Self::dependency_graph(db, env, solution);
        let mut unresolved: Vec<_> = graph.dependencies.iter().map(Vec::len).collect();

        let mut ready: VecDeque<_> = unresolved
            .iter()
            .enumerate()
            .filter_map(|(index, count)| (*count == 0).then_some(index))
            .collect();
        while let Some(index) = ready.pop_front() {
            if graph.dependents[index].is_empty() {
                continue;
            }
            // Specialize stored alias arguments without expanding recursive alias bodies.
            let context =
                GenericContext::from_typevar_instances(db, env, [solution[index].bound_typevar]);
            let specialization = context.specialize(db, &[solution[index].solution]);
            for &dependent in &graph.dependents[index] {
                solution[dependent].solution = solution[dependent]
                    .solution
                    .apply_specialization(db, specialization);
                unresolved[dependent] -= 1;
                if unresolved[dependent] == 0 {
                    ready.push_back(dependent);
                }
            }
        }

        let mut recursive = false;
        let remaining = unresolved
            .iter()
            .enumerate()
            .filter_map(|(index, count)| (*count != 0).then_some(index));
        for mut component in graph.components(remaining) {
            component.sort_unstable();
            if component
                .iter()
                .any(|index| !solution[*index].bound_typevar.is_inferable(db, inferable))
            {
                continue;
            }
            let mut candidate: Vec<_> = component
                .iter()
                .map(|index| solution[*index].clone())
                .collect();
            let is_cycle = graph.is_cyclic(&component);
            let newly_recursive = is_cycle && Self::close_component(db, env, &mut candidate);
            if is_cycle && !newly_recursive {
                // Pure type-variable relationships retain their inference policy
                // and free parameters; they do not introduce a recursive type.
                continue;
            }
            recursive |= newly_recursive;
            let context = GenericContext::from_typevar_instances(
                db,
                env,
                candidate.iter().map(|binding| binding.bound_typevar),
            );
            let types: Vec<_> = candidate.iter().map(|binding| binding.solution).collect();
            let specialization = context.specialize(db, &types);
            for (index, binding) in component.iter().zip(candidate) {
                solution[*index] = binding;
            }
            // All remaining components see this component's closed solutions.
            for (index, binding) in solution.iter_mut().enumerate() {
                if unresolved[index] != 0
                    && !component.contains(&index)
                    && binding.bound_typevar.is_inferable(db, inferable)
                {
                    binding.solution = binding.solution.apply_specialization(db, specialization);
                }
            }
        }
        if recursive {
            // Fold finite dependents into the same graph when their structure also
            // occurs inside a recursive component, independently of equation ordering.
            let equations: Vec<_> = solution
                .iter()
                .map(|binding| (binding.bound_typevar, binding.solution))
                .collect();
            if let Some(types) = RecursiveType::from_equations(db, env, &equations) {
                for (binding, ty) in solution.iter_mut().zip(types) {
                    binding.solution = ty;
                }
            }
        }
        recursive || simplified
    }

    /// Simplify Boolean dependencies, then bind constructor edges simultaneously.
    /// The caller publishes the component only after every equation is closed.
    fn close_component(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        solution: &mut [Self],
    ) -> bool {
        let mut equations = solution.to_vec();
        Self::normalize_equations(db, env, &mut equations);
        let equations: Vec<_> = equations
            .iter()
            .map(|binding| (binding.bound_typevar, binding.solution))
            .collect();
        let Some(types) = RecursiveType::from_equations(db, env, &equations) else {
            return false;
        };
        for (binding, ty) in solution.iter_mut().zip(types) {
            binding.solution = ty;
        }
        true
    }

    /// Eliminate Boolean cycles without substituting references below constructors.
    /// The normalized equations can be closed as recursive types or unfolded symbolically.
    pub(in crate::types) fn normalize_equations(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        equations: &mut [Self],
    ) {
        for index in 0..equations.len() {
            let variable = equations[index].bound_typevar;
            let equation = equations[index].without_self_constraint(db, env);
            equations[index].solution = equation;
            for (dependent, binding) in equations.iter_mut().enumerate() {
                if dependent != index {
                    binding.solution =
                        Self::substitute_unguarded(db, env, binding.solution, variable, equation);
                }
            }
        }
        for binding in equations {
            binding.solution = binding.without_self_constraint(db, env);
        }
    }

    /// Substitute within Boolean expressions; constructor edges stay shared.
    fn substitute_unguarded(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        variable: BoundTypeVarInstance<'db>,
        replacement: Type<'db>,
    ) -> Type<'db> {
        match ty {
            Type::TypeVar(found) if found.identity(db) == variable.identity(db) => replacement,
            Type::Union(union) => UnionType::from_elements(
                db,
                env,
                union.elements(db).iter().map(|element| {
                    Self::substitute_unguarded(db, env, *element, variable, replacement)
                }),
            ),
            Type::Intersection(intersection) => {
                let mut builder = IntersectionBuilder::new(db, env);
                for element in intersection.positive(db) {
                    builder.add_positive_in_place(Self::substitute_unguarded(
                        db,
                        env,
                        *element,
                        variable,
                        replacement,
                    ));
                }
                for element in intersection.negative(db) {
                    builder.add_negative_in_place(Self::substitute_unguarded(
                        db,
                        env,
                        *element,
                        variable,
                        replacement,
                    ));
                }
                builder.build()
            }
            _ => ty,
        }
    }

    /// Drop the tautological part of `T >= T | F(T)`.
    /// References inside constructors remain, and an identity equation stays free.
    fn without_self_constraint(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        self.normalize_equation(db, env, self.solution, &mut FxIndexSet::default())
    }

    /// Aliases and recursive binders are transparent at the root of an equation.
    /// Unfold them before removing tautologies, but never expand below a constructor.
    fn normalize_equation(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        active: &mut FxIndexSet<Type<'db>>,
    ) -> Type<'db> {
        if !active.insert(ty) {
            return ty;
        }
        let is_self = |ty: Type<'db>| matches!(ty, Type::TypeVar(variable) if variable.identity(db) == self.bound_typevar.identity(db));
        let result = match ty {
            Type::TypeAlias(alias) => {
                self.normalize_equation(db, env, alias.value_type(db), active)
            }
            Type::Recursive(recursive) => recursive.map_type(db, env, |unfolded| {
                self.normalize_equation(db, env, unfolded, active)
            }),
            Type::Union(union) => UnionType::from_elements(
                db,
                env,
                union
                    .elements(db)
                    .iter()
                    .copied()
                    .map(|ty| self.normalize_equation(db, env, ty, active))
                    .filter(|ty| !is_self(*ty)),
            ),
            _ => ty,
        };
        active.swap_remove(&ty);
        result
    }
}

impl<'db> PathBound<'db> {
    /// Bare subtype edges form equality classes when they are mutually reachable.
    /// Only inferable variables can be substituted; outer parameters keep their identities.
    fn dependency_graph(
        db: &'db dyn Db,
        bounds: &[PathBound<'db>],
        inferable: TypeVarSet<'db>,
    ) -> DependencyGraph {
        let mut dependencies = vec![Vec::new(); bounds.len()];
        let mut variables = FxIndexMap::default();
        for (index, bound) in bounds.iter().enumerate() {
            if bound.bound_typevar.is_inferable(db, inferable) {
                let previous = *variables
                    .entry(bound.bound_typevar.identity(db))
                    .or_insert(index);
                // Materialized occurrences can carry different declarations for the same variable.
                if previous != index {
                    dependencies[previous].push(index);
                    dependencies[index].push(previous);
                }
            }
        }
        for (index, bound) in bounds.iter().enumerate() {
            if !bound.bound_typevar.is_inferable(db, inferable) {
                continue;
            }
            for lower in bound
                .evidence_lower
                .into_iter()
                .chain([bound.validity_lower])
            {
                let elements = match lower {
                    Type::Union(union) => union.elements(db),
                    _ => std::slice::from_ref(&lower),
                };
                for element in elements {
                    if let Type::TypeVar(variable) = element
                        && let Some(&source) = variables.get(&variable.identity(db))
                    {
                        dependencies[source].push(index);
                    }
                }
            }
            for upper in bound.upper.iter_clauses() {
                if let Type::TypeVar(variable) = upper.ty()
                    && let Some(&target) = variables.get(&variable.identity(db))
                {
                    dependencies[index].push(target);
                }
            }
        }
        DependencyGraph::new(dependencies)
    }

    /// Variables related in both subtype directions must satisfy their declarations jointly,
    /// including when no constructor or concrete bound fixes their shared type yet.
    pub(super) fn equal_variables(
        db: &'db dyn Db,
        path: &[Self],
        inferable: TypeVarSet<'db>,
    ) -> Vec<BoundTypeVarInstance<'db>> {
        let graph = Self::dependency_graph(db, path, inferable);
        graph
            .components(0..path.len())
            .into_iter()
            .filter(|component| component.len() > 1)
            .flat_map(|mut component| {
                component.sort_unstable();
                component
            })
            .map(|index| path[index].bound_typevar)
            .collect()
    }

    /// Combine the bounds of variables proven equal on this path before selecting their types.
    /// Constructor dependencies do not imply equality and remain available for recursive closure.
    pub(super) fn merge_equalities(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        path: &[Self],
        inferable: TypeVarSet<'db>,
    ) -> Option<EqualityClasses<'db>> {
        let graph = Self::dependency_graph(db, path, inferable);
        let components = graph.components(0..path.len());
        let mut representatives: Vec<_> = path.iter().map(|bound| bound.bound_typevar).collect();
        let mut groups = Vec::with_capacity(components.len());
        for mut component in components {
            component.sort_unstable();
            if component.len() == 1 {
                groups.push(component);
                continue;
            }
            let representative = path[component[0]].bound_typevar;
            let context = GenericContext::from_typevar_instances(
                db,
                env,
                component
                    .iter()
                    .map(|index| path[*index].bound_typevar)
                    .filter(|variable| variable.identity(db) != representative.identity(db)),
            );
            let types = vec![Type::TypeVar(representative); context.len(db)];
            let merged =
                Self::merge_bounds(db, env, path, &component, context.specialize(db, &types));
            if merged.evidence_lower.is_none()
                && merged.validity_lower.is_never()
                && merged.upper.is_empty()
            {
                // A bare equality does not choose a free parameter's binding context or
                // declaration. Keep those identities until the class acquires a bound.
                groups.extend(component.into_iter().map(|index| vec![index]));
                continue;
            }
            for &index in &component {
                representatives[index] = representative;
            }
            groups.push(component);
        }
        if groups.iter().all(|component| component.len() == 1) {
            return None;
        }
        let substitutions: FxIndexMap<_, _> = path
            .iter()
            .zip(&representatives)
            .filter(|(bound, representative)| {
                bound.bound_typevar.identity(db) != representative.identity(db)
            })
            .map(|(bound, representative)| (bound.bound_typevar.identity(db), *representative))
            .collect();
        let context = GenericContext::from_typevar_instances(
            db,
            env,
            path.iter()
                .map(|bound| bound.bound_typevar)
                .filter(|variable| substitutions.contains_key(&variable.identity(db))),
        );
        let types: Vec<_> = context
            .variables(db)
            .map(|variable| Type::TypeVar(substitutions[&variable.identity(db)]))
            .collect();
        let specialization = context.specialize(db, &types);
        let mut result = path.to_vec();
        for component in &groups {
            let representative = representatives[component[0]];
            let mut merged = Self::merge_bounds(db, env, path, component, specialization);
            if merged.evidence_lower.is_none()
                && merged.validity_lower.is_never()
                && merged.upper.is_empty()
            {
                // An identity equation remains a free parameter.
                merged = Self::exact(representative, Type::TypeVar(representative));
            }
            for &index in component {
                result[index] = Self {
                    bound_typevar: path[index].bound_typevar,
                    ..merged.clone()
                };
            }
        }
        Some(EqualityClasses {
            bounds: result,
            groups,
        })
    }

    /// Substitute class representatives and conjoin their bounds, omitting self tautologies.
    fn merge_bounds(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        path: &[Self],
        component: &[usize],
        specialization: Specialization<'db>,
    ) -> Self {
        let representative = path[component[0]].bound_typevar;
        let mut merged = ConstraintBoundsBuilder::default();
        for &index in component {
            let bound = &path[index];
            for lower in bound
                .evidence_lower
                .map(ConstraintBound::Evidence)
                .into_iter()
                .chain([ConstraintBound::Validity(bound.validity_lower)])
            {
                let mapped = lower.ty().apply_specialization(db, specialization);
                // L(Q) <= Q is equivalent to L(Never) <= Q for Boolean occurrences
                // of Q. References inside constructors must remain recursive.
                let ty = TypeVarSolution::substitute_unguarded(
                    db,
                    env,
                    mapped,
                    representative,
                    Type::Never,
                );
                if ty != mapped && ty.is_never() {
                    continue;
                }
                merged.add_lower(db, env, lower.with_type(ty));
            }
            for upper in bound.upper.iter_clauses() {
                let mapped = upper.ty().apply_specialization(db, specialization);
                // Dually, Q <= U(Q) only constrains the Boolean region where Q holds.
                let ty = TypeVarSolution::substitute_unguarded(
                    db,
                    env,
                    mapped,
                    representative,
                    Type::object(),
                );
                if ty != mapped && ty.is_object() {
                    continue;
                }
                merged.add_upper(db, env, upper.with_type(ty));
            }
        }
        merged.finish(db, env, representative)
    }

    /// Prefer a pinned equality to a union of its consequences before elimination.
    /// The original bounds remain available for validating the simultaneous solution.
    fn simplify_equation(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        selected: Type<'db>,
    ) -> Type<'db> {
        let Type::Union(lower) = selected else {
            return selected;
        };
        if selected != self.effective_lower(db, env) || !selected.is_fully_static(db, env) {
            return selected;
        }
        // If A is included in the lower bound and is also an upper bound, A <= T <= A.
        // Union bounds are flattened when consequences are added. Compare their elements
        // so finite expansions cannot obscure the original recursive equation.
        self.upper
            .iter_clauses()
            .map(ConstraintBound::ty)
            .find(|upper| match upper {
                Type::Union(upper) => upper
                    .elements(db)
                    .iter()
                    .all(|element| lower.elements(db).contains(element)),
                _ => lower.elements(db).contains(upper),
            })
            .unwrap_or(selected)
    }
}

/// Shared bounds and membership of equality classes on a single constraint path.
pub(super) struct EqualityClasses<'db> {
    pub(super) bounds: Vec<PathBound<'db>>,
    groups: Vec<Vec<usize>>,
}

impl<'db> EqualityClasses<'db> {
    /// Reconcile per-variable inference preferences into one choice for each equality class.
    /// Lower-bound evidence asks for a common supertype; upper-only evidence asks for a subtype.
    /// If an intersection exceeds its budget, omit that class's bindings and retain incompleteness.
    pub(super) fn merge_solutions(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        solutions: &mut Vec<TypeVarSolution<'db>>,
    ) -> bool {
        let mut exceeded_budget = false;
        let bindings: FxIndexMap<_, _> = solutions
            .iter()
            .enumerate()
            .map(|(index, binding)| (binding.bound_typevar, index))
            .collect();
        let mut omitted = FxIndexSet::default();
        for group in &self.groups {
            if group.len() == 1 {
                continue;
            }
            let indices: Vec<_> = group
                .iter()
                .filter_map(|index| bindings.get(&self.bounds[*index].bound_typevar).copied())
                .collect();
            let choices = indices.iter().map(|index| solutions[*index].solution);
            let choice = if self.bounds[group[0]].evidence_lower.is_some() {
                Some(UnionType::from_elements(db, env, choices))
            } else {
                IntersectionType::bounded_from_elements(db, env, choices)
            };
            if let Some(choice) = choice {
                for index in indices {
                    solutions[index].solution = choice;
                }
            } else {
                exceeded_budget = true;
                omitted.extend(indices);
            }
        }
        let mut index = 0;
        solutions.retain(|_| {
            let keep = !omitted.contains(&index);
            index += 1;
            keep
        });
        exceeded_budget
    }
}

pub(super) struct SolutionWalker<'db> {
    source_orders: FxIndexSet<ConstraintId>,
    sorted_paths: Vec<Vec<(ConstraintId, usize)>>,
    _phantom: PhantomData<&'db ()>,
}

impl<'db> SolutionWalker<'db> {
    pub(super) fn new(source_orders: FxIndexSet<ConstraintId>) -> Self {
        Self {
            source_orders,
            sorted_paths: Vec::default(),
            _phantom: PhantomData,
        }
    }

    pub(super) fn visit_node<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        path: &mut PathAssignments,
        node: NodeId,
        limits: &mut L,
    ) -> ControlFlow<L::Break> {
        limits.visit_node()?;
        if node == ALWAYS_FALSE {
            return ControlFlow::Continue(());
        }

        // If the current node is ALWAYS_TRUE, we can immediately report the current solution.
        if node == ALWAYS_TRUE {
            limits.satisfied_path()?;
            self.found_satisfied_path(path);
            return ControlFlow::Continue(());
        }

        // At this point we actually have to walk the outgoing edges of this node.
        let interior = storage.interior_node_data(node);
        let constraint = interior.constraint;
        for (assignment, child) in [
            (constraint.when_true(), interior.if_true),
            (constraint.when_unconstrained(), interior.if_uncertain),
            (constraint.when_false(), interior.if_false),
        ] {
            path.walk_edge(
                db,
                env,
                storage,
                assignment,
                |storage, path, _new_range, found_conflict| {
                    if !found_conflict {
                        self.visit_node(db, env, storage, path, child, limits)?;
                    }
                    ControlFlow::Continue(())
                },
            )?;
        }
        ControlFlow::Continue(())
    }

    fn found_satisfied_path(&mut self, path: &PathAssignments) {
        let mut path: Vec<_> = path
            .positive_constraints()
            .map(|(constraint, source_constraint)| {
                let source_order = self
                    .source_orders
                    .get_index_of(&source_constraint)
                    .expect("every TDD constraint should have a source order");
                (constraint, source_order)
            })
            .collect();
        // Sort the constraints in each path by their `source_order`s, to ensure that we construct
        // any unions or intersections in our type mappings in a stable order. Constraints might
        // come out of `PathAssignments` with identical `source_order`s, but if they do, those
        // "tied" constraints will still be ordered in a stable way. So we need a stable sort to
        // retain that stable per-tie ordering.
        path.sort_by_key(|(_, source_order)| *source_order);
        self.sorted_paths.push(path);
    }

    pub(super) fn finish(
        mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        inferable: TypeVarSet<'db>,
    ) -> PathBounds<'db> {
        if self.sorted_paths.is_empty() {
            return PathBounds::Unsatisfiable;
        }

        self.sorted_paths.sort_by(|path1, path2| {
            let source_orders1 = path1.iter().map(|(_, source_order)| *source_order);
            let source_orders2 = path2.iter().map(|(_, source_order)| *source_order);
            source_orders1.cmp(source_orders2)
        });

        let mut result = Vec::with_capacity(self.sorted_paths.len());
        let mut mappings: FxIndexMap<BoundTypeVarInstance<'db>, ConstraintBoundsBuilder<'db>> =
            FxIndexMap::default();

        for path in self.sorted_paths {
            mappings.clear();
            for (constraint, _) in path {
                let constraint = storage.constraint_data(constraint);
                let typevar = constraint.typevar;
                if let Some(lower) = constraint.stored_lower_bound() {
                    let bounds = mappings.entry(typevar).or_default();
                    bounds.add_lower(db, env, lower);

                    if let Type::TypeVar(lower_bound_typevar) = lower.ty() {
                        let bounds = mappings.entry(lower_bound_typevar).or_default();
                        bounds.add_upper(db, env, lower.with_type(Type::TypeVar(typevar)));
                    }
                }

                if let Some(upper) = constraint.stored_upper_bound() {
                    let bounds = mappings.entry(typevar).or_default();
                    bounds.add_upper(db, env, upper);

                    if let Type::TypeVar(upper_bound_typevar) = upper.ty() {
                        let bounds = mappings.entry(upper_bound_typevar).or_default();
                        bounds.add_lower(db, env, upper.with_type(Type::TypeVar(typevar)));
                    }
                }
            }

            let path_bounds = mappings
                .drain(..)
                .map(|(bound_typevar, bounds)| bounds.finish(db, env, bound_typevar))
                .collect();
            result.push(path_bounds);
        }

        PathBounds::Constrained(result.into_boxed_slice(), inferable)
    }
}
