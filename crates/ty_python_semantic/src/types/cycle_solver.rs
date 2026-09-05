//! Solves the equations that define the cycle variables mentioned by a query's outputs.
//!
//! A query's output can mention cycle markers whose values are defined elsewhere: the root marker
//! of another cycle participant stands for that query's current result, and a derived marker
//! stands for an operation the owning query deferred. Gathering those definitions yields a system
//! of equations. Solving it replaces the markers whose values the current cycle iteration already
//! determines, instead of leaving them for later iterations of the Salsa fixed point, which cannot
//! resolve a marker nested inside a constructed type at all.
//!
//! The solver never substitutes a value into a nested position while iterating: a marker nested
//! inside a type stays a reference to its variable, and only a marker that is the whole input, or
//! a top-level element of a union input, is replaced by the variable's current value. Values
//! therefore never grow in depth, so the iteration terminates without a budget. The final
//! expansion substitutes nested references once, keeping the markers of mutually recursive
//! variables as the cut points that cycle recovery already knows how to bound.

use std::cell::RefCell;

use ruff_python_ast::Operator;
use rustc_hash::{FxHashMap, FxHashSet};
use ty_python_core::scope::ScopeKind;

use crate::Db;
use crate::types::call::{CallArguments, CallDunderError};
use crate::types::class::implicit_attributes::{
    implicit_attribute_equations, implicit_attribute_value,
};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::cycle_equations::{CycleEquations, Operation};
use crate::types::cycle_variable::{CycleOwner, CycleVariable};
use crate::types::infer::{
    InferenceRegion, infer_deferred_types, infer_definition_types, infer_expression_types,
    infer_function_default_types, infer_scope_types, infer_statement_types_impl,
    infer_unpack_types,
};
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::visitor::{
    any_over_type_for_cycle_markers, nesting_depth, visit_types_for_cycle_markers,
};
use crate::types::{
    DivergentType, MemberLookupPolicy, ProgramEnvironment, Type, TypeContext, TypeMapping,
    UnionBuilder, UnionType, member_lookup_value,
};

/// The resolved values of cycle variables, ready to substitute for their markers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CycleSolution<'db> {
    /// The values to substitute for the markers of the variables they resolve.
    values: FxHashMap<CycleVariable<'db>, Type<'db>>,
    /// The values of recursive variables, to be replaced by the markers of their variables
    /// where they occur.
    ///
    /// A recursive value mentions its own variable, so substituting it would unfold the
    /// recursion by one level. A query that infers such a value again from a result in which
    /// it was unfolded nests it one level deeper on every cycle iteration; folding the value
    /// back into its marker keeps the result at the depth the query inferred.
    cuts: FxHashMap<Type<'db>, CycleVariable<'db>>,
}

impl<'db> CycleSolution<'db> {
    pub(crate) fn get(&self, variable: CycleVariable<'db>) -> Option<Type<'db>> {
        self.values.get(&variable).copied()
    }

    /// The recursive variable whose value `ty` is, if any.
    pub(crate) fn cut(&self, ty: Type<'db>) -> Option<CycleVariable<'db>> {
        self.cuts.get(&ty).copied()
    }

    fn has_cuts(&self) -> bool {
        !self.cuts.is_empty()
    }

    fn is_empty(&self) -> bool {
        self.values.is_empty() && self.cuts.is_empty()
    }
}

/// Substitutes the resolved values of `solution` for the markers in `ty`.
pub(crate) fn resolve_cycle_variables<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    solution: &CycleSolution<'db>,
    ty: Type<'db>,
) -> Type<'db> {
    // A value to cut is a union, which the search does not present as a whole; a solution
    // with cuts is applied to every type.
    let mentions_resolved = solution.has_cuts()
        || any_over_type_for_cycle_markers(db, env, ty, |ty| {
            matches!(
                ty,
                Type::Divergent(marker)
                    if marker.variable().is_some_and(|variable| solution.get(variable).is_some())
            )
        });
    if !mentions_resolved {
        return ty;
    }
    ty.apply_type_mapping(
        db,
        env,
        &TypeMapping::ResolveCycleVariables(solution),
        TypeContext::default(),
    )
}

/// How a system defines the value of one variable.
#[derive(Clone, Debug)]
enum SystemEquation<'db> {
    /// The value the owning query has inferred for its root. It can mention markers, including
    /// the root's own.
    Value(Type<'db>),
    /// An operation the owning query deferred.
    Operation(Operation<'db>),
}

impl<'db> SystemEquation<'db> {
    fn inputs(&self, db: &'db dyn Db) -> Vec<Type<'db>> {
        match self {
            SystemEquation::Value(ty) => vec![*ty],
            SystemEquation::Operation(operation) => operation.inputs(db).to_vec(),
        }
    }
}

/// Resolves the cycle markers in the outputs of one query.
pub(crate) struct CycleResolver<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    /// The query whose outputs are resolved.
    owner: CycleOwner<'db>,
    /// The operations the query has deferred so far.
    local: &'a FxHashMap<CycleVariable<'db>, Operation<'db>>,
    /// The value the query's root marker stands for, for a query with one main output.
    root: Option<Type<'db>>,
    /// Another query whose root marker stands for the same value as `root`.
    alias: Option<CycleOwner<'db>>,
}

impl<'a, 'db> CycleResolver<'a, 'db> {
    pub(crate) fn new(
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        owner: CycleOwner<'db>,
        local: &'a FxHashMap<CycleVariable<'db>, Operation<'db>>,
        root: Option<Type<'db>>,
    ) -> Self {
        Self {
            db,
            env,
            owner,
            local,
            root,
            alias: None,
        }
    }

    /// Lets the root marker of `owner` stand for the same value as the query's own root.
    ///
    /// The value of an assignment `name = expression` is the value of the expression, so the
    /// definition's root is known to the expression's query before the definition's own result
    /// is, which still holds the previous cycle iteration's value.
    pub(crate) fn with_alias(mut self, owner: CycleOwner<'db>) -> Self {
        self.alias = Some(owner);
        self
    }

    /// Solves the equations reachable from `outputs`.
    ///
    /// Root markers are never part of the solution. A root stands for a query's whole result,
    /// and substituting that result where its marker is nested would unfold the recursion by
    /// one level on every cycle iteration, without the marker that lets cycle recovery cut the
    /// growth. Root values only feed the evaluation of the derived variables.
    ///
    /// Returns `None` when no marker in `outputs` can be resolved.
    pub(crate) fn solve(
        &self,
        outputs: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<CycleSolution<'db>> {
        let db = self.db;
        let mut system = System::default();
        let mut worklist = unmaterialized_markers_of(db, self.env, outputs);
        if worklist.is_empty() {
            return None;
        }
        let mut visited = FxHashSet::default();
        while let Some(variable) = worklist.pop() {
            if !visited.insert(variable) {
                continue;
            }
            let Some(equation) = self.equation(variable) else {
                continue;
            };
            worklist.extend(
                equation
                    .inputs(db)
                    .into_iter()
                    .flat_map(|input| unmaterialized_markers(db, self.env, input)),
            );
            system.insert(variable, equation);
        }
        if system.is_empty() {
            return None;
        }
        let mut solution = system.solve(db, self.env);
        solution.values.retain(|variable, _| !variable.is_root(db));
        (!solution.is_empty()).then_some(solution)
    }

    /// The definition of `variable`, if the query that owns it has recorded one.
    fn equation(&self, variable: CycleVariable<'db>) -> Option<SystemEquation<'db>> {
        let db = self.db;
        let owner = variable.owner(db);
        let is_root = variable.is_root(db);
        if is_root && self.alias == Some(owner) {
            return self.root.map(SystemEquation::Value);
        }
        if owner == self.owner {
            return if is_root {
                self.root.map(SystemEquation::Value)
            } else {
                self.local
                    .get(&variable)
                    .cloned()
                    .map(SystemEquation::Operation)
            };
        }
        if is_root {
            owner
                .root_value(db)
                // A query that has only its cycle-initial marker so far defines nothing.
                .filter(|value| *value != Type::divergent_variable(variable))
                .map(SystemEquation::Value)
        } else {
            owner
                .equations(db)
                .and_then(|equations| equations.get(&variable).cloned())
                .map(SystemEquation::Operation)
        }
    }
}

impl<'db> CycleOwner<'db> {
    /// The current result that this owner's root marker stands for.
    ///
    /// Reading the result of a query that is still being inferred yields its provisional value,
    /// which is how a cycle participant learns what the other participants know so far.
    fn root_value(self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            CycleOwner::Region(InferenceRegion::Definition(definition)) => {
                infer_definition_types(db, definition).root_value(definition)
            }
            CycleOwner::Region(InferenceRegion::Deferred(definition)) => {
                infer_deferred_types(db, definition).root_value(definition)
            }
            CycleOwner::Region(InferenceRegion::Expression(expression, tcx)) => {
                infer_expression_types(db, expression, tcx)
                    .try_expression_type(expression.node_ref(db))
            }
            CycleOwner::Attribute(attribute) => implicit_attribute_value(db, attribute),
            CycleOwner::Member(key, receiver) => member_lookup_value(db, key, receiver),
            // These queries have no single main output.
            CycleOwner::Region(
                InferenceRegion::Statement(_)
                | InferenceRegion::Scope(..)
                | InferenceRegion::FunctionDecorators(_)
                | InferenceRegion::FunctionDefaults(_),
            )
            | CycleOwner::Unpack(_)
            | CycleOwner::Query(_) => None,
        }
    }

    /// The operations this owner has deferred, as recorded by its current result.
    fn equations(self, db: &'db dyn Db) -> Option<&'db CycleEquations<'db>> {
        match self {
            CycleOwner::Region(InferenceRegion::Definition(definition)) => {
                infer_definition_types(db, definition).equations()
            }
            CycleOwner::Region(InferenceRegion::Deferred(definition)) => {
                infer_deferred_types(db, definition).equations()
            }
            CycleOwner::Region(InferenceRegion::FunctionDefaults(definition)) => {
                infer_function_default_types(db, definition).equations()
            }
            CycleOwner::Region(InferenceRegion::Expression(expression, tcx)) => {
                infer_expression_types(db, expression, tcx).equations()
            }
            CycleOwner::Region(InferenceRegion::Statement(statement)) => {
                infer_statement_types_impl(db, statement).equations()
            }
            CycleOwner::Unpack(unpack) => Some(infer_unpack_types(db, unpack).equations()),
            CycleOwner::Attribute(attribute) => Some(implicit_attribute_equations(db, attribute)),
            // The expression that contains a comprehension already depends on the inference of
            // the comprehension's scope.
            CycleOwner::Region(InferenceRegion::Scope(scope, tcx))
                if scope.scope(db).kind() == ScopeKind::Comprehension =>
            {
                infer_scope_types(db, scope, tcx).equations()
            }
            // Depending on a whole scope's inference from another query would create needless
            // cycles, and the remaining owners never defer operations.
            CycleOwner::Region(
                InferenceRegion::Scope(..) | InferenceRegion::FunctionDecorators(_),
            )
            | CycleOwner::Member(..)
            | CycleOwner::Query(_) => None,
        }
    }
}

/// The variables appearing in any of `types` whose markers still stand for values to be
/// resolved.
///
/// The outputs of a query share most of their types, such as the type of a name and of every
/// expression that reads it; each is walked once.
fn unmaterialized_markers_of<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    types: impl IntoIterator<Item = Type<'db>>,
) -> Vec<CycleVariable<'db>> {
    let found = RefCell::new(Vec::new());
    visit_types_for_cycle_markers(db, env, types, |ty| {
        if let Type::Divergent(marker) = ty
            && marker.materialization_kind().is_none()
            && let Some(variable) = marker.variable()
        {
            found.borrow_mut().push(variable);
        }
    });
    found.into_inner()
}

/// The variables appearing in `ty` whose markers still stand for values to be resolved.
fn unmaterialized_markers<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> Vec<CycleVariable<'db>> {
    let found = RefCell::new(Vec::new());
    any_over_type_for_cycle_markers(db, env, ty, |ty| {
        if let Type::Divergent(marker) = ty
            && marker.materialization_kind().is_none()
            && let Some(variable) = marker.variable()
        {
            found.borrow_mut().push(variable);
        }
        false
    });
    found.into_inner()
}

/// The equations gathered for one resolution, indexed for solving.
#[derive(Default)]
struct System<'db> {
    variables: Vec<CycleVariable<'db>>,
    index: FxHashMap<CycleVariable<'db>, usize>,
    equations: Vec<SystemEquation<'db>>,
}

impl<'db> System<'db> {
    fn is_empty(&self) -> bool {
        self.variables.is_empty()
    }

    fn insert(&mut self, variable: CycleVariable<'db>, equation: SystemEquation<'db>) {
        self.index.insert(variable, self.variables.len());
        self.variables.push(variable);
        self.equations.push(equation);
    }

    fn index_of(&self, marker: DivergentType<'db>) -> Option<usize> {
        if marker.materialization_kind().is_some() {
            return None;
        }
        self.index.get(&marker.variable()?).copied()
    }

    /// The variables of this system mentioned anywhere in `ty`.
    fn mentioned(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Vec<usize> {
        let found = RefCell::new(Vec::new());
        any_over_type_for_cycle_markers(db, env, ty, |ty| {
            if let Type::Divergent(marker) = ty
                && let Some(index) = self.index_of(marker)
            {
                found.borrow_mut().push(index);
            }
            false
        });
        found.into_inner()
    }

    /// The variables no evaluation can give a value: an input of their equation is nothing but
    /// the marker of a variable the system does not define, of one that is itself stuck, or of
    /// the variable itself.
    ///
    /// Such a variable is, for this solve, like a marker the system does not define: what
    /// mentions it stays as it is, with the marker as an unresolved reference, rather than
    /// being dropped as if the variable's value were still to come.
    fn stuck(&self, db: &'db dyn Db) -> Vec<bool> {
        let bare: Vec<Vec<Option<usize>>> = self
            .equations
            .iter()
            .map(|equation| {
                equation
                    .inputs(db)
                    .into_iter()
                    .filter_map(|input| match input {
                        Type::Divergent(marker)
                            if marker.materialization_kind().is_none()
                                && marker.variable().is_some() =>
                        {
                            Some(self.index_of(marker))
                        }
                        _ => None,
                    })
                    .collect()
            })
            .collect();
        let mut evaluable = vec![false; self.variables.len()];
        let mut changed = true;
        while changed {
            changed = false;
            for (variable, bare) in bare.iter().enumerate() {
                if !evaluable[variable]
                    && bare.iter().all(|input| {
                        input.is_some_and(|input| input != variable && evaluable[input])
                    })
                {
                    evaluable[variable] = true;
                    changed = true;
                }
            }
        }
        evaluable.into_iter().map(|evaluable| !evaluable).collect()
    }

    fn solve(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> CycleSolution<'db> {
        let dependencies: Vec<Vec<usize>> = self
            .equations
            .iter()
            .map(|equation| {
                let mut dependencies: Vec<_> = equation
                    .inputs(db)
                    .into_iter()
                    .flat_map(|input| self.mentioned(db, env, input))
                    .collect();
                dependencies.sort_unstable();
                dependencies.dedup();
                dependencies
            })
            .collect();
        let mut dependents = vec![Vec::new(); self.variables.len()];
        for (variable, dependencies) in dependencies.iter().enumerate() {
            for &dependency in dependencies {
                dependents[dependency].push(variable);
            }
        }

        let stuck = self.stuck(db);
        let mut solver = Solver {
            db,
            env,
            system: &self,
            values: vec![None; self.variables.len()],
            stuck,
        };
        // Components come out with their dependencies first, so every variable a component reads
        // from outside itself already has its final value.
        for component in strongly_connected_components(&dependencies) {
            solver.solve_component(&component, &dependents);
        }
        solver.expand()
    }
}

/// How many times one variable's value can change while its strongly connected component is
/// iterated.
///
/// The iteration is bounded by the widenings applied to each variable's value: literal families
/// are widened to their instance types, and a value that keeps nesting deeper, such as the result
/// of a method `__iadd__` returning `Grow[list[T]]` for a `Grow[T]`, is frozen at its first
/// depth. This cap guards against growth those widenings do not cover. What a solve leaves
/// unresolved is refined by the next cycle iteration, where cycle recovery bounds the growth of
/// the query results as it does without the solver.
const MAX_EVALUATIONS_PER_VARIABLE: usize = 8;

/// Where a type is resolved: the value of a root, an input of an operation, or the result of
/// evaluating one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    Value,
    Operand,
    Result,
}

struct Solver<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    system: &'a System<'db>,
    /// The current value of each variable; `None` until an evaluation produces one.
    values: Vec<Option<Type<'db>>>,
    /// The variables no evaluation can give a value; see [`System::stuck`].
    stuck: Vec<bool>,
}

impl<'db> Solver<'_, 'db> {
    /// Iterates the variables of one strongly connected component to a fixed point.
    ///
    /// Each value only ever gains union elements drawn from the inputs of the component's
    /// equations and the results of its operations on them, and nested markers are never
    /// expanded, so the iteration terminates.
    fn solve_component(&mut self, component: &[usize], dependents: &[Vec<usize>]) {
        let in_component: FxHashSet<usize> = component.iter().copied().collect();
        let mut queued: Vec<bool> = vec![false; self.system.variables.len()];
        let mut evaluations: Vec<usize> = vec![0; self.system.variables.len()];
        let mut diverging: Vec<bool> = vec![false; self.system.variables.len()];
        let mut worklist: Vec<usize> = component.to_vec();
        for &variable in component {
            queued[variable] = true;
        }
        while let Some(variable) = worklist.pop() {
            queued[variable] = false;
            if diverging[variable] || evaluations[variable] >= MAX_EVALUATIONS_PER_VARIABLE {
                continue;
            }
            let evaluated = self.evaluate(variable);
            let Some(new) = evaluated else {
                // A value computed earlier from inputs that are pending now, or by an operation
                // whose result turned out to pass a provisional query result through, is not a
                // value of the equation anymore; it is withdrawn, and its dependents follow.
                if self.values[variable].take().is_some() {
                    evaluations[variable] += 1;
                    for &dependent in &dependents[variable] {
                        if in_component.contains(&dependent) && !queued[dependent] {
                            queued[dependent] = true;
                            worklist.push(dependent);
                        }
                    }
                }
                continue;
            };
            let new = match self.values[variable] {
                None => new,
                // A root's value keeps the markers nested in it as references, so it does not
                // change when the variables it references do. The operations that read it do
                // see their new values, so they are re-evaluated all the same. Other roots are
                // not: two roots that mention each other would re-evaluate each other forever.
                Some(old)
                    if old == new
                        && matches!(self.system.equations[variable], SystemEquation::Value(_)) =>
                {
                    for &dependent in &dependents[variable] {
                        if in_component.contains(&dependent)
                            && !queued[dependent]
                            && matches!(
                                self.system.equations[dependent],
                                SystemEquation::Operation(_)
                            )
                        {
                            queued[dependent] = true;
                            worklist.push(dependent);
                        }
                    }
                    continue;
                }
                Some(old) if old == new => continue,
                Some(old) => {
                    // A value that keeps growing is one a loop keeps widening, like a counter
                    // incremented on every iteration; literal unions are widened as they are for
                    // loop-carried bindings so the fixed point stays finite.
                    let mut joined = UnionBuilder::new(self.db, self.env)
                        .cycle_recovery(true)
                        .recursively_defined(RecursivelyDefined::Yes);
                    joined.add_in_place(old);
                    joined.add_in_place(new);
                    let joined = joined.build();
                    if joined == old {
                        continue;
                    }
                    // A root's value grows as the markers nested in it are resolved; only the
                    // result of an operation can nest deeper each time it is re-evaluated. A
                    // result that mentions its own variable is a recursive type, which the
                    // expansion cuts at that mention.
                    if matches!(
                        self.system.equations[variable],
                        SystemEquation::Operation(_)
                    ) && nesting_depth(self.db, self.env, joined)
                        > nesting_depth(self.db, self.env, old)
                        && !self
                            .system
                            .mentioned(self.db, self.env, joined)
                            .contains(&variable)
                    {
                        // A recursive alias and its expansion nest differently but describe the
                        // same type.
                        if joined.is_equivalent_to(self.db, self.env, old) {
                            continue;
                        }
                        // Re-evaluating an operation on a value that includes its own result
                        // nests the result deeper, such as `__iadd__` returning `Grow[list[T]]`
                        // for a `Grow[T]`; that has no finite fixed point. The variable stays
                        // unresolved: its marker remains for cycle recovery to bound, as without
                        // the solver.
                        diverging[variable] = true;
                        self.values[variable] = None;
                        continue;
                    }
                    joined
                }
            };
            self.values[variable] = Some(new);
            evaluations[variable] += 1;
            for &dependent in &dependents[variable] {
                if in_component.contains(&dependent) && !queued[dependent] {
                    queued[dependent] = true;
                    worklist.push(dependent);
                }
            }
        }
    }

    /// The value of `variable`'s equation on the values found so far, or `None` while an input,
    /// or every element of the result, has no value yet.
    fn evaluate(&self, variable: usize) -> Option<Type<'db>> {
        match &self.system.equations[variable] {
            SystemEquation::Value(ty) => self.resolve_top(variable, *ty, Position::Value),
            SystemEquation::Operation(operation) => {
                let operation = operation.map_inputs(self.db, |input| {
                    self.resolve_top(variable, input, Position::Operand)
                })?;
                self.resolve_top(
                    variable,
                    operation.evaluate(self.db, self.env),
                    Position::Result,
                )
            }
        }
    }

    /// Replaces the markers at the top level of `ty` by the values of their variables.
    ///
    /// A marker of the variable being evaluated adds nothing at the top level and is dropped.
    /// A union element that mentions a variable without a value yet is dropped as well, so the
    /// remaining elements form the productive base; the element returns once the variable has a
    /// value and the equation is re-evaluated. Returns `None` when nothing remains.
    ///
    /// A marker at the top level that stands for a value the system does not define, such as
    /// the provisional result of a query that has produced nothing but its cycle-initial marker
    /// yet, stands for no value either. In a value or an operand it is dropped like the marker
    /// of a variable without a value yet, and a bare one leaves the operation pending: applying
    /// the operation to the marker would pass it through like a dynamic type, and the result
    /// would then name the operand instead of the operation's result. In the result of an
    /// operation it means exactly that such a marker was passed through, by a query the
    /// operation called that is still being evaluated in the cycle, so the whole result is
    /// pending: a later cycle iteration re-evaluates the operation once the value is known.
    fn resolve_top(&self, variable: usize, ty: Type<'db>, position: Position) -> Option<Type<'db>> {
        let pending_outside = |marker: DivergentType<'db>| {
            marker.materialization_kind().is_none() && marker.variable().is_some()
        };
        match ty {
            Type::Divergent(marker) => match self.system.index_of(marker) {
                Some(index) if index == variable => None,
                Some(index) => self.values[index],
                None if pending_outside(marker) => None,
                None => Some(ty),
            },
            Type::Union(union) => {
                let mut elements = Vec::new();
                let mut changed = false;
                for &element in union.elements(self.db) {
                    match element {
                        Type::Divergent(marker)
                            if let Some(index) = self.system.index_of(marker) =>
                        {
                            changed = true;
                            if index != variable
                                && let Some(value) = self.values[index]
                            {
                                elements.push(value);
                            }
                        }
                        Type::Divergent(marker) if pending_outside(marker) => {
                            if position == Position::Result {
                                return None;
                            }
                            changed = true;
                        }
                        _ => {
                            if self.mentions_pending(variable, element) {
                                changed = true;
                            } else {
                                elements.push(element);
                            }
                        }
                    }
                }
                // A union that keeps every element as it is stays as it is: rebuilding it
                // compares each of its elements with every other one again, on every
                // evaluation of a root whose markers are all nested.
                if !changed {
                    Some(ty)
                } else if elements.is_empty() {
                    None
                } else {
                    // A loop-carried union stays marked as recursively defined, so the literal
                    // widening that bounds loop-carried literals applies to the resolved values.
                    let mut builder = UnionBuilder::new(self.db, self.env)
                        .cycle_recovery(true)
                        .recursively_defined(union.recursively_defined(self.db));
                    for element in elements {
                        builder.add_in_place(element);
                    }
                    Some(builder.build())
                }
            }
            _ => Some(ty),
        }
    }

    /// Whether `ty` mentions a variable other than `variable` whose value is still to come.
    ///
    /// A nested mention of `variable` itself is a recursive reference, kept as it is: it becomes
    /// the cut point that bounds the value's depth. A mention of a stuck variable is kept too,
    /// as an unresolved reference: dropping it would commit a value that lacks the part
    /// depending on the variable, and with it the recursion through that part.
    fn mentions_pending(&self, variable: usize, ty: Type<'db>) -> bool {
        any_over_type_for_cycle_markers(self.db, self.env, ty, |ty| {
            if let Type::Divergent(marker) = ty
                && let Some(index) = self.system.index_of(marker)
            {
                index != variable && self.values[index].is_none() && !self.stuck[index]
            } else {
                false
            }
        })
    }

    /// Substitutes the nested references to derived variables in every resolved value.
    ///
    /// References between mutually recursive values are left as markers, so the expansion is
    /// finite and the markers remain for cycle recovery to cut. References to roots are left as
    /// markers too, for the reason given on [`CycleResolver::solve`].
    fn expand(self) -> CycleSolution<'db> {
        let references: Vec<Vec<usize>> = self
            .values
            .iter()
            .map(|value| {
                value.map_or_else(Vec::new, |value| {
                    let mut references: Vec<_> = self
                        .system
                        .mentioned(self.db, self.env, value)
                        .into_iter()
                        .filter(|&index| {
                            self.values[index].is_some()
                                && !self.system.variables[index].is_root(self.db)
                        })
                        .collect();
                    references.sort_unstable();
                    references.dedup();
                    references
                })
            })
            .collect();
        let mut solution = CycleSolution::default();
        for component in strongly_connected_components(&references) {
            // The values of mutually recursive variables, and of a variable whose value
            // mentions itself, are recursive types. Their markers stay as they are, as the cut
            // points of the recursion, and their values fold back into the markers.
            if component.len() > 1
                || component
                    .iter()
                    .any(|&index| references[index].contains(&index))
            {
                for &index in &component {
                    let variable = self.system.variables[index];
                    if let Some(value) = self.values[index]
                        && !variable.is_root(self.db)
                    {
                        solution.cuts.insert(value, variable);
                    }
                }
                continue;
            }
            let expanded: Vec<_> = component
                .iter()
                .filter_map(|&index| {
                    let value = self.values[index]?;
                    Some((
                        self.system.variables[index],
                        resolve_cycle_variables(self.db, self.env, &solution, value),
                    ))
                })
                .collect();
            solution.values.extend(expanded);
        }
        solution
    }
}

impl<'db> Operation<'db> {
    /// Evaluates the operation on its inputs, whose top-level markers stand for no value.
    ///
    /// Errors are not reported: the query that recorded the operation reports them once it
    /// re-infers the operation on the resolved operand.
    fn evaluate(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match self {
            Operation::Subscript { value, key } => value
                .subscript(db, env, *key, ruff_python_ast::ExprContext::Load)
                .unwrap_or_else(|_| Type::unknown()),
            Operation::Member {
                value,
                name,
                policy,
            } => value
                .member_lookup_with_policy(db, env, name, *policy)
                .place
                .ignore_possibly_undefined()
                .unwrap_or_else(Type::unknown),
            Operation::Iterate { value, mode } => value
                .try_iterate_with_mode(db, env, *mode)
                .map(|tuple| tuple.homogeneous_element_type(db, env))
                .unwrap_or_else(|error| error.fallback_element_type(db, env)),
            Operation::Unpack {
                value,
                length,
                index,
            } => value
                .unpacked_target(db, env, *length, *index)
                .unwrap_or_else(Type::unknown),
            Operation::Enter { value, mode } => value
                .try_enter_with_mode(db, env, *mode)
                .unwrap_or_else(|error| error.fallback_enter_type(db, env)),
            Operation::Await { value } => {
                value.try_await(db, env).unwrap_or_else(|_| Type::unknown())
            }
            Operation::Binary {
                left,
                right,
                operator,
                context,
            } => context
                .evaluate(db, env, *left, *right, *operator)
                .return_type
                .unwrap_or_else(Type::unknown),
            Operation::Unary { operand, operator } => operand
                .try_unary_operation(db, env, *operator, &mut |_| {})
                .unwrap_or_else(|fallback| fallback),
            Operation::Augmented {
                left,
                right,
                operator,
                context,
            } => {
                if let Type::Union(union) = left {
                    return union.map(db, env, |left| {
                        Operation::Augmented {
                            left: *left,
                            right: *right,
                            operator: *operator,
                            context: *context,
                        }
                        .evaluate(db, env)
                    });
                }
                // Updating a typed dictionary preserves its schema, including on invalid updates;
                // the query that recorded the operation validates the update.
                if matches!((operator, left), (Operator::BitOr, Type::TypedDict(_))) {
                    return *left;
                }
                let binary = || {
                    context
                        .evaluate(db, env, *left, *right, *operator)
                        .return_type
                        .unwrap_or_else(Type::unknown)
                };
                match left.try_call_dunder_with_policy(
                    db,
                    env,
                    operator.in_place_dunder(),
                    &mut CallArguments::positional([*right]),
                    TypeContext::default(),
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                ) {
                    Ok(bindings) => bindings.return_type(db, env),
                    Err(CallDunderError::MethodNotAvailable) => binary(),
                    Err(CallDunderError::PossiblyUnbound { bindings, .. }) => {
                        UnionType::from_two_elements(
                            db,
                            env,
                            bindings.return_type(db, env),
                            binary(),
                        )
                    }
                    Err(CallDunderError::CallError(..)) => Type::unknown(),
                }
            }
            Operation::Call {
                callable,
                arguments,
                tcx,
            } => {
                let arguments = arguments.to_arguments(db);
                let constraints = ConstraintSetBuilder::new();
                let result = callable
                    .bindings(db, env)
                    .match_parameters(db, env, &arguments)
                    .check_types(db, env, &constraints, &arguments, *tcx, &[]);
                result.map_or_else(
                    |_| Type::unknown(),
                    |bindings| bindings.return_type(db, env),
                )
            }
            Operation::Narrow { value, narrowing } => narrowing.apply(db, env, *value),
            Operation::MappingKey { value } => value
                .unpack_keys_and_items(db, env)
                .map(|(key, _)| key)
                .unwrap_or_else(Type::unknown),
            Operation::MappingValue { value } => value
                .unpack_keys_and_items(db, env)
                .map(|(_, value)| value)
                .unwrap_or_else(Type::unknown),
            Operation::TypeArgument { value, parameter } => {
                let argument =
                    |value: Type<'db>| value.class_specialization(db, env)?.1.get(db, *parameter);
                match value {
                    Type::Union(union) => union.try_map(db, env, |element| argument(*element)),
                    value => argument(*value),
                }
                .unwrap_or_else(Type::unknown)
            }
            Operation::Tuple { elements } => elements.build(db, env),
            Operation::ScopeExpression {
                scope,
                tcx,
                expression,
            } => infer_scope_types(db, *scope, *tcx).expression_type(*expression),
        }
    }
}

/// Tarjan's algorithm, returning components with their dependencies first.
fn strongly_connected_components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct Tarjan<'a> {
        edges: &'a [Vec<usize>],
        index: Vec<Option<usize>>,
        low_link: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        next_index: usize,
        components: Vec<Vec<usize>>,
    }

    impl Tarjan<'_> {
        fn visit(&mut self, root: usize) {
            // An explicit stack of (node, next edge position) keeps deep systems off the call
            // stack.
            let mut call_stack = vec![(root, 0)];
            self.enter(root);
            while let Some(&mut (node, ref mut position)) = call_stack.last_mut() {
                if let Some(&successor) = self.edges[node].get(*position) {
                    *position += 1;
                    if self.index[successor].is_none() {
                        self.enter(successor);
                        call_stack.push((successor, 0));
                    } else if self.on_stack[successor] {
                        self.low_link[node] = self.low_link[node].min(self.low_link[successor]);
                    }
                    continue;
                }
                call_stack.pop();
                if let Some(&(parent, _)) = call_stack.last() {
                    self.low_link[parent] = self.low_link[parent].min(self.low_link[node]);
                }
                if Some(self.low_link[node]) == self.index[node] {
                    let mut component = Vec::new();
                    loop {
                        let member = self.stack.pop().expect("node is on the stack");
                        self.on_stack[member] = false;
                        component.push(member);
                        if member == node {
                            break;
                        }
                    }
                    component.sort_unstable();
                    self.components.push(component);
                }
            }
        }

        fn enter(&mut self, node: usize) {
            self.index[node] = Some(self.next_index);
            self.low_link[node] = self.next_index;
            self.next_index += 1;
            self.stack.push(node);
            self.on_stack[node] = true;
        }
    }

    let mut tarjan = Tarjan {
        edges,
        index: vec![None; edges.len()],
        low_link: vec![0; edges.len()],
        on_stack: vec![false; edges.len()],
        stack: Vec::new(),
        next_index: 0,
        components: Vec::new(),
    };
    for node in 0..edges.len() {
        if tarjan.index[node].is_none() {
            tarjan.visit(node);
        }
    }
    tarjan.components
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_db::system::DbWithWritableSystem;
    use ty_python_core::ExpressionNodeKey;

    use super::*;
    use crate::db::tests::{TestDb, setup_db};
    use crate::semantic_model::HasDefinition;
    use crate::types::CycleSlot;
    use crate::types::class::KnownClass;
    use crate::types::tuple::{TupleLength, TupleType};
    use crate::{ProgramFile, SemanticModel};

    /// A database with two definitions whose queries can own cycle variables.
    fn db_with_owners() -> TestDb {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
def first(): ...
def second(): ...
"#,
        )
        .unwrap();
        db
    }

    /// The query inferring the `index`th function definition of the test module.
    fn owner<'db>(db: &'db TestDb, env: &ProgramEnvironment<'db>, index: usize) -> CycleOwner<'db> {
        let file = system_path_to_file(db, "/src/a.py").unwrap();
        let program_file = ProgramFile::new(db, file, env.program(db));
        let module = parsed_module(db, program_file.python_file(db)).load(db);
        let function = module.syntax().body[index]
            .as_function_def_stmt()
            .expect("test module consists of function definitions");
        let model = SemanticModel::new(db, program_file);
        CycleOwner::Region(InferenceRegion::Definition(function.definition(&model)))
    }

    /// A slot for a derived variable; the expression it names does not matter here.
    fn expression_slot() -> CycleSlot {
        let parsed = ruff_python_parser::parse_expression("x").expect("valid expression");
        CycleSlot::Expression(ExpressionNodeKey::from(parsed.expr()))
    }

    #[test]
    fn components_come_after_their_dependencies() {
        // 0 -> 1 -> 2 -> 1, 0 -> 3
        let edges = vec![vec![1, 3], vec![2], vec![1], vec![]];
        let components = strongly_connected_components(&edges);
        let position = |node: usize| {
            components
                .iter()
                .position(|component| component.contains(&node))
                .unwrap()
        };
        assert_eq!(components.len(), 3);
        assert!(components.contains(&vec![1, 2]));
        assert!(position(1) < position(0));
        assert!(position(3) < position(0));
    }

    #[test]
    fn rebuilds_a_tuple_from_its_productive_alternative() {
        // x = (x[0],) or x = (1,): the marker nested in the first alternative is the value the
        // unpacking of `x` yields, which the productive alternative determines.
        let db = db_with_owners();
        let env = db.program_environment();
        let root = CycleVariable::root(&db, salsa::plumbing::Id::from_bits(1), owner(&db, &env, 0));
        let element = CycleVariable::derived(&db, owner(&db, &env, 1), expression_slot(), root);
        let int = KnownClass::Int.to_instance(&db, &env);
        let output = UnionType::from_elements(
            &db,
            &env,
            [
                Type::tuple(TupleType::heterogeneous(
                    &db,
                    &env,
                    [Type::divergent_variable(element)],
                )),
                Type::tuple(TupleType::heterogeneous(&db, &env, [int])),
            ],
        );
        let mut system = System::default();
        system.insert(root, SystemEquation::Value(output));
        system.insert(
            element,
            SystemEquation::Operation(Operation::Unpack {
                value: Type::divergent_variable(root),
                length: TupleLength::Fixed(1),
                index: 0,
            }),
        );
        let solution = system.solve(&db, &env);
        assert_eq!(solution.get(element), Some(int));
        assert_eq!(
            solution.get(root),
            Some(Type::tuple(TupleType::heterogeneous(&db, &env, [int])))
        );
    }

    #[test]
    fn keeps_a_recursive_reference_as_a_cut_point() {
        // x = [x] or x = 1: the recursive alternative is kept with its marker rather than
        // unfolded, so the result stays finite.
        let db = db_with_owners();
        let env = db.program_environment();
        let root = CycleVariable::root(&db, salsa::plumbing::Id::from_bits(1), owner(&db, &env, 0));
        let int = KnownClass::Int.to_instance(&db, &env);
        let list_of_root = KnownClass::List.to_specialized_instance(
            &db,
            &env,
            vec![Type::divergent_variable(root)],
        );
        let output = UnionType::from_elements(&db, &env, [int, list_of_root]);
        let mut system = System::default();
        system.insert(root, SystemEquation::Value(output));
        let solution = system.solve(&db, &env);
        assert_eq!(solution.get(root), Some(output));
    }

    #[test]
    fn leaves_an_unproductive_variable_unresolved() {
        // x = (x,): nothing determines the element, so no value is produced.
        let db = db_with_owners();
        let env = db.program_environment();
        let root = CycleVariable::root(&db, salsa::plumbing::Id::from_bits(1), owner(&db, &env, 0));
        let element = CycleVariable::derived(&db, owner(&db, &env, 1), expression_slot(), root);
        let output = Type::tuple(TupleType::heterogeneous(
            &db,
            &env,
            [Type::divergent_variable(element)],
        ));
        let mut system = System::default();
        system.insert(root, SystemEquation::Value(output));
        system.insert(
            element,
            SystemEquation::Operation(Operation::Unpack {
                value: Type::divergent_variable(root),
                length: TupleLength::Fixed(1),
                index: 0,
            }),
        );
        let solution = system.solve(&db, &env);
        assert!(solution.get(element).is_none());
        assert_eq!(solution.get(root), Some(output));
    }

    #[test]
    fn solves_mutually_dependent_values() {
        // a = int or b; b = str or a.
        let db = db_with_owners();
        let env = db.program_environment();
        let a = CycleVariable::root(&db, salsa::plumbing::Id::from_bits(1), owner(&db, &env, 0));
        let b = CycleVariable::root(&db, salsa::plumbing::Id::from_bits(2), owner(&db, &env, 1));
        let int = KnownClass::Int.to_instance(&db, &env);
        let str = KnownClass::Str.to_instance(&db, &env);
        let mut system = System::default();
        system.insert(
            a,
            SystemEquation::Value(UnionType::from_elements(
                &db,
                &env,
                [int, Type::divergent_variable(b)],
            )),
        );
        system.insert(
            b,
            SystemEquation::Value(UnionType::from_elements(
                &db,
                &env,
                [str, Type::divergent_variable(a)],
            )),
        );
        let solution = system.solve(&db, &env);
        let both = FxHashSet::from_iter([int, str]);
        assert_eq!(
            solution.get(a).map(|ty| elements(&db, ty)),
            Some(both.clone())
        );
        assert_eq!(solution.get(b).map(|ty| elements(&db, ty)), Some(both));
    }

    /// The elements of a union, ignoring their order.
    fn elements<'db>(db: &'db dyn Db, ty: Type<'db>) -> FxHashSet<Type<'db>> {
        match ty {
            Type::Union(union) => union.elements(db).iter().copied().collect(),
            ty => FxHashSet::from_iter([ty]),
        }
    }
}
