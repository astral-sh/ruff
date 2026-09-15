//! Variance inference separates constructing equations from evaluating them. Recursive types
//! contribute named variables, so references such as `P[list[T]]` do not expand indefinitely.
//!
//! Ordinary evaluation honors explicit protocol declarations. To validate a declaration, we
//! instead solve the equations in that parameter's strongly connected component, starting at
//! bivariance. Declarations outside the component still apply: referencing an independent
//! protocol does not make its declared variance part of the validation problem.
//!
//! Dependencies follow variance composition, not just syntax. An argument erased by a bivariant
//! parameter cannot connect two components. Conversely, reaching invariance does not erase
//! later dependencies, even though ordinary evaluation can stop there.

use std::collections::VecDeque;

use rustc_hash::{FxHashMap, FxHashSet};
use salsa::plumbing::AsId;

use crate::types::{
    BoundTypeVarIdentity, ClassType, FunctionType, GenericAlias, StaticClassLiteral, TypeAliasType,
    TypeVarVariance, TypedDictType,
};
use crate::{Db, ProgramEnvironment};

/// A variance expression whose recursive references name equations rather than expand types.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum VarianceTerm<'db> {
    Constant(TypeVarVariance),
    Variable(VarianceVariable<'db>),
    Join(VarianceSum<'db>),
    Compose(VarianceProduct<'db>),
}

impl<'db> VarianceTerm<'db> {
    pub(crate) const BIVARIANT: Self = Self::Constant(TypeVarVariance::Bivariant);

    pub(crate) fn variable(
        db: &'db dyn Db,
        origin: VarianceOrigin<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> Self {
        Self::Variable(VarianceVariable::new(db, origin, typevar))
    }

    /// Combine occurrences without discarding symbolic dependencies at `Invariant`.
    /// Only evaluation can short-circuit there: later terms can still connect a recursive group.
    pub(crate) fn join(db: &'db dyn Db, terms: impl IntoIterator<Item = Self>) -> Self {
        let mut constant = TypeVarVariance::Bivariant;
        let mut symbolic = Vec::new();
        for term in terms {
            match term {
                Self::Constant(variance) => constant = constant.join(variance),
                _ => symbolic.push(term),
            }
        }
        if constant != TypeVarVariance::Bivariant {
            symbolic.push(Self::Constant(constant));
        }
        match symbolic.as_slice() {
            [] => Self::BIVARIANT,
            [term] => *term,
            _ => Self::Join(VarianceSum::new(db, symbolic.into_boxed_slice())),
        }
    }

    /// Compose definition-site and use-site variance, preserving erasure in either position.
    pub(crate) fn compose_thunk(self, db: &'db dyn Db, other: impl FnOnce() -> Self) -> Self {
        if self == Self::BIVARIANT {
            return self;
        }
        let other = other();
        match (self, other) {
            (Self::Constant(left), Self::Constant(right)) => left.compose(right).into(),
            (_, Self::Constant(TypeVarVariance::Bivariant)) => Self::BIVARIANT,
            (Self::Constant(TypeVarVariance::Covariant), _) => other,
            (_, Self::Constant(TypeVarVariance::Covariant)) => self,
            _ => Self::Compose(VarianceProduct::new(db, self, other)),
        }
    }

    /// Evaluate an expression using declared variance at protocol-parameter references.
    pub(crate) fn evaluate(self, db: &'db dyn Db) -> TypeVarVariance {
        self.evaluate_with(db, &|variable| variable.effective_variance(db))
    }

    /// Substitute the supplied variable values; the component solver supplies its current
    /// approximations for members and effective variance for references outside the component.
    fn evaluate_with(
        self,
        db: &'db dyn Db,
        lookup: &impl Fn(VarianceVariable<'db>) -> TypeVarVariance,
    ) -> TypeVarVariance {
        match self {
            Self::Constant(variance) => variance,
            Self::Variable(variable) => lookup(variable),
            Self::Join(sum) => sum
                .terms(db)
                .iter()
                .map(|term| term.evaluate_with(db, lookup))
                .collect(),
            Self::Compose(product) => product
                .left(db)
                .evaluate_with(db, lookup)
                .compose_thunk(|| product.right(db).evaluate_with(db, lookup)),
        }
    }

    /// Visit only references that survive composition. For example, the equation for the
    /// parameter in `type Ignore[T] = int` is bivariant, so `Ignore[P[T]]` adds no edge to `P`.
    fn visit_live_variables(self, db: &'db dyn Db, mut visit: impl FnMut(VarianceVariable<'db>)) {
        let mut pending = vec![self];
        let mut visited = FxHashSet::default();
        while let Some(term) = pending.pop() {
            if !visited.insert(term) || term.evaluate(db) == TypeVarVariance::Bivariant {
                continue;
            }
            match term {
                Self::Constant(_) => {}
                Self::Variable(variable) => visit(variable),
                Self::Join(sum) => pending.extend(sum.terms(db).iter().copied()),
                Self::Compose(product) => pending.extend([product.left(db), product.right(db)]),
            }
        }
    }
}

impl From<TypeVarVariance> for VarianceTerm<'_> {
    fn from(variance: TypeVarVariance) -> Self {
        Self::Constant(variance)
    }
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct VarianceSum<'db> {
    #[returns(ref)]
    terms: Box<[VarianceTerm<'db>]>,
}

impl get_size2::GetSize for VarianceSum<'_> {}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct VarianceProduct<'db> {
    #[returns(copy)]
    left: VarianceTerm<'db>,
    #[returns(copy)]
    right: VarianceTerm<'db>,
}

impl get_size2::GetSize for VarianceProduct<'_> {}

/// Definition bodies that can occur recursively in variance expressions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum VarianceOrigin<'db> {
    Class(StaticClassLiteral<'db>),
    /// A use of an explicitly declared protocol parameter, distinct from inferring its body.
    ProtocolParameter(StaticClassLiteral<'db>, TypeVarVariance),
    GenericAlias(GenericAlias<'db>),
    TypeAlias(TypeAliasType<'db>),
    Function(FunctionType<'db>),
    TypedDict(ClassType<'db>),
}

/// One unknown in the equation graph. Generic references use the origin's formal parameters;
/// specialization arguments contribute separate terms instead of expanding definition bodies.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct VarianceVariable<'db> {
    #[returns(copy)]
    origin: VarianceOrigin<'db>,
    #[returns(copy)]
    typevar: BoundTypeVarIdentity<'db>,
}

impl get_size2::GetSize for VarianceVariable<'_> {}

#[salsa::tracked]
impl<'db> VarianceVariable<'db> {
    /// Honor the protocol declaration attached to this reference even when its equation infers
    /// a different variance. References without a declaration are evaluated from their equations.
    #[salsa::tracked(returns(copy), cycle_initial=|_, _, _| TypeVarVariance::Bivariant, heap_size=ruff_memory_usage::heap_size)]
    fn effective_variance(self, db: &'db dyn Db) -> TypeVarVariance {
        if let VarianceOrigin::ProtocolParameter(_, declared) = self.origin(db) {
            declared
        } else {
            self.equation(db).evaluate(db)
        }
    }

    /// Return the defining expression, or declared variance for an unsupported protocol.
    /// Recursive references remain symbolic, allowing the same equation to serve
    /// ordinary evaluation and declaration validation.
    #[salsa::tracked(returns(copy), cycle_initial=|_, _, _| VarianceTerm::BIVARIANT, heap_size=ruff_memory_usage::heap_size)]
    fn equation(self, db: &'db dyn Db) -> VarianceTerm<'db> {
        let typevar = self.typevar(db);
        match self.origin(db) {
            // Checking structural support opens the protocol's interface. Defer that work until
            // validation needs the equation; ordinary evaluation only needs the declaration.
            VarianceOrigin::ProtocolParameter(class, declared)
                if class
                    .into_protocol_class(db)
                    .is_none_or(|protocol| !protocol.supports_variance_inference(db)) =>
            {
                declared.into()
            }
            VarianceOrigin::Class(class) | VarianceOrigin::ProtocolParameter(class, _) => {
                class.variance_equation(db, typevar)
            }
            VarianceOrigin::GenericAlias(alias) => alias.variance_equation(db, typevar),
            VarianceOrigin::TypeAlias(alias) => alias.variance_equation(db, typevar),
            VarianceOrigin::Function(function) => function.variance_equation(db, typevar),
            VarianceOrigin::TypedDict(class) => {
                let env = ProgramEnvironment::from_file(class.class_literal(db).program_file(db));
                TypedDictType::new(class).variance_of_items(db, &env, typevar)
            }
        }
    }

    /// Return unique references that survive composition under ordinary, declaration-honoring
    /// evaluation. Component discovery and the solver's work queue share these cached edges.
    #[salsa::tracked(returns(ref), cycle_initial=|_, _, _| Box::default(), heap_size=ruff_memory_usage::heap_size)]
    fn dependencies(self, db: &'db dyn Db) -> Box<[Self]> {
        let mut dependencies = Vec::new();
        self.equation(db)
            .visit_live_variables(db, |dependency| dependencies.push(dependency));
        dependencies.into_boxed_slice()
    }
}

/// A recursive component, ordered by variable ID so every member uses the same cached solution.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct VarianceComponent<'db> {
    #[returns(ref)]
    variables: Box<[VarianceVariable<'db>]>,
}

impl get_size2::GetSize for VarianceComponent<'_> {}

#[salsa::tracked]
impl<'db> VarianceComponent<'db> {
    /// Solve all equations together, revisiting only the dependents of a changed value.
    ///
    /// Results follow the component's canonical variable order. The empty Salsa cycle seed
    /// represents bivariance for every member until the solution is available.
    #[salsa::tracked(returns(ref), cycle_initial=|_, _, _| Box::default(), heap_size=ruff_memory_usage::heap_size)]
    fn solution(self, db: &'db dyn Db) -> Box<[TypeVarVariance]> {
        let variables = self.variables(db);
        let indices: FxHashMap<_, _> = variables
            .iter()
            .enumerate()
            .map(|(index, variable)| (*variable, index))
            .collect();
        let equations: Vec<_> = variables
            .iter()
            .map(|variable| variable.equation(db))
            .collect();
        let mut dependents = vec![Vec::new(); variables.len()];
        for (index, variable) in variables.iter().enumerate() {
            for dependency in variable.dependencies(db) {
                if let Some(&dependency_index) = indices.get(dependency) {
                    dependents[dependency_index].push(index);
                }
            }
        }

        let mut values = vec![TypeVarVariance::Bivariant; variables.len()];
        let mut pending: VecDeque<_> = (0..variables.len()).collect();
        let mut queued = vec![true; variables.len()];
        // Join and composition are monotone. Each value can move from bivariance to a polarity
        // and then to invariance, so even negative cycles need only finitely many updates.
        while let Some(index) = pending.pop_front() {
            queued[index] = false;
            let variance = equations[index].evaluate_with(db, &|dependency| {
                indices.get(&dependency).map_or_else(
                    || dependency.effective_variance(db),
                    |&dependency_index| values[dependency_index],
                )
            });
            if values[index] != variance {
                values[index] = variance;
                for &dependent in &dependents[index] {
                    if !queued[dependent] {
                        queued[dependent] = true;
                        pending.push_back(dependent);
                    }
                }
            }
        }
        values.into_boxed_slice()
    }
}

/// Infer the root protocol parameter together with its mutually dependent parameters.
/// Declarations outside that component remain authoritative. Equations and effective values are
/// cached separately, so dependency discovery does not repeat the semantic type traversal.
///
/// For example, this infers contravariance for `T_co` despite its declaration:
///
/// ```python
/// from typing import Protocol, TypeVar
///
/// T_co = TypeVar("T_co", covariant=True)
/// class Sink(Protocol[T_co]):
///     def write(self, value: T_co) -> None: ...
///     def next(self) -> "Sink[T_co]": ...
/// ```
///
/// Callers select supported protocol parameters and normalize bivariance to covariance only
/// after inference, so unused parameters do not introduce constraints into a recursive component.
#[salsa::tracked(returns(copy), cycle_initial=|_, _, _, _, _| TypeVarVariance::Bivariant, heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn infer_protocol_variance<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    typevar: BoundTypeVarIdentity<'db>,
    declared: TypeVarVariance,
) -> TypeVarVariance {
    let root = VarianceVariable::new(
        db,
        VarianceOrigin::ProtocolParameter(class, declared),
        typevar,
    );
    let mut pending = vec![root];
    let mut visited = FxHashSet::default();
    let mut incoming: FxHashMap<_, Vec<_>> = FxHashMap::default();
    let mut variables = Vec::new();

    while let Some(variable) = pending.pop() {
        if !visited.insert(variable) {
            continue;
        }
        for &dependency in variable.dependencies(db) {
            incoming.entry(dependency).or_default().push(variable);
            pending.push(dependency);
        }
        variables.push(variable);
    }

    // Every visited variable is reachable from the root. Those with a path back to the root
    // therefore form exactly its strongly connected component.
    let mut component = FxHashSet::default();
    pending.push(root);
    while let Some(variable) = pending.pop() {
        if component.insert(variable)
            && let Some(predecessors) = incoming.get(&variable)
        {
            pending.extend(predecessors.iter().copied());
        }
    }
    variables.retain(|variable| component.contains(variable));
    variables.sort_unstable_by_key(AsId::as_id);
    let root_index = variables.binary_search_by_key(&root.as_id(), AsId::as_id);
    let component = VarianceComponent::new(db, variables.into_boxed_slice());
    root_index
        .ok()
        .and_then(|index| component.solution(db).get(index).copied())
        .unwrap_or(TypeVarVariance::Bivariant)
}
