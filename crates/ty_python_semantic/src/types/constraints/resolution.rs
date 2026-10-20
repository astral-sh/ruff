//! Resolve dependencies between the selected bindings of one solution alternative.

mod equations;

use std::cell::{OnceCell, RefCell};

use equations::EquationGraph;
use rustc_hash::FxHashMap;

use super::{Solution, TypeVarSolution};
use crate::types::cyclic::CycleDetector;
use crate::types::function::FunctionType;
use crate::types::generics::{ApplySpecialization, GenericContext};
use crate::types::known_instance::walk_known_instance_type;
use crate::types::signatures::{Signature, walk_signature};
use crate::types::typevar::TypeVarSet;
use crate::types::visitor::{TypeKind, TypeVisitor, walk_non_atomic_type};
use crate::types::{
    BoundTypeVarInstance, CallableType, KnownInstanceType, RecursiveType, Type, TypeAliasType,
    TypeContext, TypeMapping,
};
use crate::{Db, FxOrderMap, FxOrderSet, ProgramEnvironment};

/// Whether a selected type is independent of the other inferable variables in its alternative.
///
/// This does not describe budget completeness or apply defaults to variables without evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum SolutionType<'db> {
    Resolved(Type<'db>),
    /// The original selected type, retained when dependencies are missing, recursion cannot be
    /// solved, or a type form preserves captured references during substitution.
    Unresolved(Type<'db>),
}

impl<'db> SolutionType<'db> {
    /// The selected type, including its original references if resolution was incomplete.
    pub(crate) fn ty(self) -> Type<'db> {
        match self {
            Self::Resolved(ty) | Self::Unresolved(ty) => ty,
        }
    }
}

/// Resolves each group of mutually dependent solutions once all of its dependencies are
/// resolved, by finding the strongly connected components of the dependency graph.
struct Resolver<'a, 'env, 'db> {
    env: &'a ProgramEnvironment<'db>,
    type_dependencies: &'a mut Dependencies<'env, 'db>,
    inferable: TypeVarSet<'db>,
    solution: &'a [TypeVarSolution<'db>],
    /// The solutions that each solution refers to, or `None` if one of them is missing.
    dependencies: Vec<Option<Vec<usize>>>,
    resolved: Vec<Option<Type<'db>>>,
    /// The order in which each solution was first visited.
    visited: Vec<Option<usize>>,
    /// Visited solutions whose group is not complete yet, in the order of their visits.
    pending: Vec<usize>,
    is_pending: Vec<bool>,
}

impl<'a, 'env, 'db> Resolver<'a, 'env, 'db> {
    /// Resolves dependencies between the selected solutions. Solutions that depend on each other, such
    /// as `T = int | tuple[T]`, are closed together as recursive types. References outside
    /// `inferable` retain their original bound-variable identity.
    fn resolve(
        db: &'db dyn Db,
        type_dependencies: &'a mut Dependencies<'env, 'db>,
        inferable: TypeVarSet<'db>,
        solution: &'a [TypeVarSolution<'db>],
    ) -> Box<[SolutionType<'db>]> {
        let env = type_dependencies.env;
        // Variadic bindings are still selected by compatibility inference after ordinary
        // solutions. Keep them symbolic here, including in a recursive constructor's arguments.
        let inferable = TypeVarSet::from_typevars(
            db,
            inferable
                .iter(db)
                .filter(|variable| !variable.is_paramspec(db) && !variable.is_typevartuple(db)),
        );
        // Most solutions mention no other variable, so the index is built on the first reference.
        let indices = OnceCell::new();
        let index_of = |variable: BoundTypeVarInstance<'db>| {
            let indices: &FxHashMap<_, _> = indices.get_or_init(|| {
                solution
                    .iter()
                    .enumerate()
                    .map(|(index, binding)| (binding.bound_typevar.identity(db), index))
                    .collect()
            });
            indices.get(&variable.identity(db)).copied()
        };
        let dependencies: Vec<_> = solution
            .iter()
            .map(|binding| {
                // A dependency without a selected solution cannot be substituted.
                type_dependencies
                    .get(db, binding.solution.ty())
                    .iter()
                    .copied()
                    .filter(|variable| variable.is_inferable(db, inferable))
                    .map(index_of)
                    .collect::<Option<Vec<_>>>()
            })
            .collect();
        let is_independent =
            |dependencies: &Option<Vec<usize>>| dependencies.as_ref().is_some_and(Vec::is_empty);
        if dependencies.iter().all(is_independent) {
            return solution
                .iter()
                .map(|binding| SolutionType::Resolved(binding.solution.ty()))
                .collect();
        }

        let mut resolver = Resolver {
            env,
            type_dependencies,
            inferable,
            solution,
            dependencies,
            resolved: vec![None; solution.len()],
            visited: vec![None; solution.len()],
            is_pending: vec![false; solution.len()],
            pending: Vec::new(),
        };
        for index in 0..solution.len() {
            resolver.visit(db, index);
        }
        solution
            .iter()
            .zip(resolver.resolved)
            .map(|(binding, resolved)| {
                resolved.map_or(
                    SolutionType::Unresolved(binding.solution.ty()),
                    SolutionType::Resolved,
                )
            })
            .collect()
    }

    /// Returns the earliest visited solution that `index` can reach among the pending ones.
    /// `index` completes a group if that is `index` itself.
    fn visit(&mut self, db: &'db dyn Db, index: usize) -> usize {
        if let Some(order) = self.visited[index] {
            return order;
        }
        // Every solution is visited once, and stays pending until its group is complete.
        let order = self.visited.iter().flatten().count();
        self.visited[index] = Some(order);
        self.pending.push(index);
        self.is_pending[index] = true;
        let mut earliest = order;
        for dependency in self.dependencies[index].clone().unwrap_or_default() {
            let reached = self.visit(db, dependency);
            if self.is_pending[dependency] {
                earliest = earliest.min(reached);
            }
        }
        if earliest == order {
            let start = self.pending.iter().rposition(|pending| *pending == index);
            let group = self.pending.split_off(start.unwrap_or_default());
            for member in &group {
                self.is_pending[*member] = false;
            }
            self.resolve_group(db, &group);
        }
        earliest
    }

    fn resolve_group(&mut self, db: &'db dyn Db, group: &[usize]) {
        // Most solutions mention no other variable, and need no substitution or verification.
        if let [index] = group
            && self.dependencies[*index]
                .as_ref()
                .is_some_and(Vec::is_empty)
        {
            self.resolved[*index] = Some(self.solution[*index].solution.ty());
            return;
        }
        let Some(equations) = group
            .iter()
            .map(|index| {
                Some((
                    self.solution[*index].bound_typevar,
                    self.substitute(db, *index, group)?,
                ))
            })
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };
        let is_recursive = group.iter().any(|index| {
            self.dependencies[*index]
                .iter()
                .flatten()
                .any(|dependency| group.contains(dependency))
        });
        let types = if is_recursive {
            let Some(types) = EquationGraph::solve(db, self.env, &equations) else {
                return;
            };
            types
        } else {
            equations.into_iter().map(|(_, ty)| ty).collect()
        };
        // Some type forms preserve captured variables when specialized. For example, an alias
        // changes its explicit arguments but can retain a free variable in its body. Verify
        // closure on the actual results without performing further substitutions.
        if self
            .type_dependencies
            .is_closed(db, self.inferable, types.iter().copied())
        {
            for (index, ty) in group.iter().zip(types) {
                self.resolved[*index] = Some(ty);
            }
        }
    }

    /// Substitutes the resolved dependencies outside of `group`. Every one of them is already
    /// closed, so one simultaneous substitution suffices.
    fn substitute(&self, db: &'db dyn Db, index: usize, group: &[usize]) -> Option<Type<'db>> {
        let original = self.solution[index].solution.ty();
        let mut replacements = FxOrderMap::default();
        for dependency in self.dependencies[index].as_ref()? {
            if !group.contains(dependency) {
                replacements.insert(*dependency, self.resolved[*dependency]?);
            }
        }
        if replacements.is_empty() {
            return Some(original);
        }
        let context = GenericContext::from_typevar_instances(
            db,
            self.env,
            replacements
                .keys()
                .map(|index| self.solution[*index].bound_typevar),
        );
        let types: Vec<_> = replacements.values().copied().collect();
        Some(original.apply_type_mapping(
            db,
            self.env,
            &TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
                generic_context: context,
                types: &types,
                skip: None,
            }),
            TypeContext::default(),
        ))
    }
}

impl<'db> Solution<'db> {
    /// Compares both sides under this alternative's simultaneous substitution. Only inferable
    /// variables are substituted; outer variables remain rigid even if the path mentions them.
    pub(crate) fn are_types_assignable(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
        comparisons: &[(Type<'db>, Type<'db>)],
    ) -> bool {
        let closed: FxOrderMap<_, _> = self
            .solved_typevars
            .iter()
            .filter_map(|binding| {
                if !binding.bound_typevar.is_inferable(db, inferable) {
                    return None;
                }
                match binding.solution {
                    SolutionType::Resolved(ty) => Some((
                        binding.bound_typevar.identity(db),
                        (binding.bound_typevar, ty),
                    )),
                    SolutionType::Unresolved(_) => None,
                }
            })
            .collect();
        let context = GenericContext::from_typevar_instances(
            db,
            env,
            closed.values().map(|(variable, _)| *variable),
        );
        let types: Vec<_> = closed.values().map(|(_, ty)| *ty).collect();
        let mapping = TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
            generic_context: context,
            types: &types,
            skip: None,
        });
        comparisons.iter().all(|(inferred, preferred)| {
            let inferred = inferred.apply_type_mapping(db, env, &mapping, TypeContext::default());
            let preferred = preferred.apply_type_mapping(db, env, &mapping, TypeContext::default());
            inferred.is_assignable_to(db, env, preferred)
        })
    }

    /// Resolves each selected binding against the other bindings of this path.
    pub(super) fn resolve(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
    ) {
        let mut dependencies = Dependencies::new(env);
        let resolved = Resolver::resolve(db, &mut dependencies, inferable, &self.solved_typevars);
        for (binding, resolved) in self.solved_typevars.iter_mut().zip(resolved) {
            binding.solution = resolved;
        }
    }
}

struct VisitDependencies;

/// Shares complete type-variable dependency sets within one solution alternative.
struct Dependencies<'a, 'db> {
    env: &'a ProgramEnvironment<'db>,
    types: FxHashMap<Type<'db>, Box<[BoundTypeVarInstance<'db>]>>,
}

impl<'a, 'db> Dependencies<'a, 'db> {
    /// Starts dependency analysis for one solution alternative.
    fn new(env: &'a ProgramEnvironment<'db>) -> Self {
        Self {
            env,
            types: FxHashMap::default(),
        }
    }

    fn get(&mut self, db: &'db dyn Db, ty: Type<'db>) -> &[BoundTypeVarInstance<'db>] {
        self.types.entry(ty).or_insert_with(|| {
            let visitor = DependencyCollector {
                env: self.env,
                found: RefCell::default(),
                visited: CycleDetector::new(()),
            };
            visitor.visit_type(db, ty);
            // A nested visit can omit dependencies through an active recursion guard. Cache only
            // the complete traversal of a root, never such a partial result.
            visitor.found.into_inner().into_iter().collect()
        })
    }

    /// Whether the types contain no occurrences of inferable variables.
    fn is_closed(
        &mut self,
        db: &'db dyn Db,
        inferable: TypeVarSet<'db>,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> bool {
        types.into_iter().all(|ty| {
            self.get(db, ty)
                .iter()
                .all(|variable| !variable.is_inferable(db, inferable))
        })
    }
}

/// Collects type-variable occurrences without visiting their declarations' bounds and defaults.
struct DependencyCollector<'a, 'db> {
    env: &'a ProgramEnvironment<'db>,
    found: RefCell<FxOrderSet<BoundTypeVarInstance<'db>>>,
    visited: CycleDetector<'db, VisitDependencies, Type<'db>, (), 3>,
}

impl<'db> DependencyCollector<'_, 'db> {
    fn signature(&self, db: &'db dyn Db, signature: &Signature<'db>) {
        walk_signature(db, signature, self);
        for parameter in signature.parameters() {
            if let Some(default) = parameter.eager_default_type() {
                self.visit_type(db, default);
            }
        }
    }
}

impl<'db> TypeVisitor<'db> for DependencyCollector<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        // Recursive specialization can introduce dependencies in an alias's changing arguments.
        // Inspect them even when the recursion guard skips another visit to the alias's body.
        let specialization = match ty {
            Type::TypeAlias(alias) => alias.specialization(db),
            Type::Recursive(recursive) => recursive.arguments(db),
            _ => None,
        };
        if let Some(specialization) = specialization {
            for argument in specialization.types(db) {
                self.visit_type(db, *argument);
            }
        }
        if let Type::TypeVar(typevar) = ty {
            self.found.borrow_mut().insert(typevar);
        } else if let TypeKind::NonAtomic(non_atomic) = TypeKind::from(ty) {
            // Revisiting a recursive structural type adds no new dependencies. Binding cycles
            // are handled separately by Resolver, where their fallback is unresolved.
            self.visited
                .visit(db, ty, || walk_non_atomic_type(db, non_atomic, self));
        }
    }

    // Generic declarations are not dependencies of their specialized arguments. Actual typevar
    // occurrences enter through `visit_type`, and their bounds and defaults remain untouched.
    fn visit_bound_type_var_type(&self, _db: &'db dyn Db, _typevar: BoundTypeVarInstance<'db>) {}

    fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
        self.visit_type(db, alias.value_type(db));
    }

    fn visit_recursive_type(&self, db: &'db dyn Db, recursive: RecursiveType<'db>) {
        self.visit_type(db, recursive.unfold(db, self.env).into_type());
    }

    fn visit_function_type(&self, db: &'db dyn Db, function: FunctionType<'db>) {
        for signature in &function.signature(db).overloads {
            self.signature(db, signature);
        }
        if function.literal(db).has_separate_implementation(db) {
            for callable in function.implementation_callables(db).iter() {
                self.visit_callable_type(db, *callable);
            }
        }
    }

    fn visit_callable_type(&self, db: &'db dyn Db, callable: CallableType<'db>) {
        for signature in &callable.signatures(db).overloads {
            self.signature(db, signature);
        }
    }

    fn visit_known_instance_type(&self, db: &'db dyn Db, known: KnownInstanceType<'db>) {
        match known {
            KnownInstanceType::TypeAliasType(alias) => {
                self.visit_type(db, Type::TypeAlias(alias));
            }
            KnownInstanceType::FunctoolsPartial(partial)
            | KnownInstanceType::FunctoolsPartialCall(partial) => {
                self.visit_type(db, partial.wrapped(db).inner(db));
                self.visit_callable_type(db, partial.partial(db));
            }
            _ => walk_known_instance_type(db, known, self),
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use ruff_python_ast::name::Name;
    use ty_python_core::ProgramFile;

    use super::{Dependencies, Resolver, SolutionType};
    use crate::db::tests::{TestDb, setup_db};
    use crate::place::global_symbol;
    use crate::types::constraints::TypeVarSolution;
    use crate::types::tuple::TupleType;
    use crate::types::typevar::TypeVarSet;
    use crate::types::{
        BoundTypeVarInstance, KnownClass, KnownInstanceType, Type, TypeVarVariance,
    };

    fn create_typevar<'db>(db: &'db TestDb, name: &str) -> BoundTypeVarInstance<'db> {
        BoundTypeVarInstance::synthetic(
            db,
            &db.program_environment(),
            Name::new(name),
            TypeVarVariance::Invariant,
        )
    }

    fn binding<'db>(
        bound_typevar: BoundTypeVarInstance<'db>,
        solution: Type<'db>,
    ) -> TypeVarSolution<'db> {
        TypeVarSolution {
            bound_typevar,
            solution: SolutionType::Resolved(solution),
        }
    }

    #[test]
    fn captured_alias_dependency_is_not_closed_by_its_argument() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
            class C[U]:
                type Alias[V] = tuple[V, U]
                type RecursiveAlias[V] = int | list[C.RecursiveAlias[tuple[V, U]]]
            "#,
        )?;
        let db = &db;
        let env = db.program_environment();
        let file = system_path_to_file(db, "/src/a.py")?;
        let file = ProgramFile::new(db, file, env.program(db));
        let class = global_symbol(db, file, "C")
            .place
            .expect_type()
            .as_class_literal()
            .ok_or_else(|| anyhow::anyhow!("expected C"))?;
        let u = class
            .generic_context(db)
            .and_then(|context| context.variables(db).next())
            .ok_or_else(|| anyhow::anyhow!("expected C's U"))?;
        let t = create_typevar(db, "T");
        let int = KnownClass::Int.to_instance(db, &env);
        // RecursiveAlias first exposes U in a recursive specialization's argument.
        for (name, argument) in [("Alias", Type::TypeVar(u)), ("RecursiveAlias", int)] {
            let alias = Type::instance(db, &env, class.identity_specialization(db))
                .member(db, &env, name)
                .place
                .expect_type();
            let Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) = alias else {
                anyhow::bail!("expected a type alias, got {alias:?}");
            };
            let alias =
                alias.apply_specialization(db, |context| context.specialize(db, vec![argument]));
            for alias in [
                Type::TypeAlias(alias),
                Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)),
            ] {
                let resolved = Resolver::resolve(
                    db,
                    &mut Dependencies::new(&env),
                    TypeVarSet::from_typevars(db, [t, u]),
                    &[binding(t, alias), binding(u, int)],
                );
                assert_eq!(
                    resolved.as_ref(),
                    [SolutionType::Unresolved(alias), SolutionType::Resolved(int)],
                    "{name}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn recursive_alias_arguments_resolve_selected_dependencies() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
            from typing import TypeVar

            V = TypeVar("V")
            Tree = tuple[V, "Tree[V] | None"]
            Growing = tuple[V, "Growing[list[V]] | None"]
            type ExplicitTree[V] = tuple[V, ExplicitTree[V] | None]
            type ExplicitGrowing[V] = tuple[V, ExplicitGrowing[list[V]] | None]

            class Source[U]:
                explicit: ExplicitTree[U]
                implicit: Tree[U]
                explicit_growing: ExplicitGrowing[U]
                implicit_growing: Growing[U]

            explicit_expected: ExplicitTree[int]
            implicit_expected: Tree[int]
            explicit_growing_expected: ExplicitGrowing[int]
            implicit_growing_expected: Growing[int]
            "#,
        )?;
        let db = &db;
        let env = db.program_environment();
        let file = system_path_to_file(db, "/src/a.py")?;
        let file = ProgramFile::new(db, file, env.program(db));
        let class = global_symbol(db, file, "Source")
            .place
            .expect_type()
            .as_class_literal()
            .ok_or_else(|| anyhow::anyhow!("expected Source"))?;
        let u = class
            .generic_context(db)
            .and_then(|context| context.variables(db).next())
            .ok_or_else(|| anyhow::anyhow!("expected Source's U"))?;
        let source = Type::instance(db, &env, class.identity_specialization(db));
        let t = create_typevar(db, "T");
        let int = KnownClass::Int.to_instance(db, &env);
        for name in [
            "explicit",
            "implicit",
            "explicit_growing",
            "implicit_growing",
        ] {
            let alias = source.member(db, &env, name).place.expect_type();
            let expected = global_symbol(db, file, &format!("{name}_expected"))
                .place
                .expect_type();
            let inferable = TypeVarSet::from_typevars(db, [t, u]);
            assert_eq!(
                Resolver::resolve(
                    db,
                    &mut Dependencies::new(&env),
                    inferable,
                    &[binding(t, alias)]
                )
                .as_ref(),
                [SolutionType::Unresolved(alias)],
                "{name}: missing dependency"
            );
            let resolved = Resolver::resolve(
                db,
                &mut Dependencies::new(&env),
                inferable,
                &[binding(t, alias), binding(u, int)],
            );
            let [
                SolutionType::Resolved(mapped),
                SolutionType::Resolved(resolved_u),
            ] = resolved.as_ref()
            else {
                anyhow::bail!("{name}: expected both dependencies to resolve, got {resolved:?}");
            };
            assert!(mapped.is_equivalent_to(db, &env, expected), "{name}");
            assert_eq!(*resolved_u, int, "{name}");
            assert_eq!(
                Resolver::resolve(
                    db,
                    &mut Dependencies::new(&env),
                    TypeVarSet::from_typevars(db, [t]),
                    &[binding(t, alias)]
                )
                .as_ref(),
                [SolutionType::Resolved(alias)],
                "{name}: non-inferable variable"
            );
        }
        Ok(())
    }

    #[test]
    fn recursive_alias_bodies_retain_captured_dependencies() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
            from typing import TypeVar

            V = TypeVar("V")

            class C[U]:
                Growing = tuple[int, "C.Growing[tuple[V, U]] | None"]
                type ExplicitGrowing[V] = tuple[int, C.ExplicitGrowing[tuple[V, U]] | None]

            def source(
                explicit_growing: C.ExplicitGrowing[int],
                implicit_growing: C.Growing[int],
            ) -> None: ...
            "#,
        )?;
        let db = &db;
        let env = db.program_environment();
        let file = system_path_to_file(db, "/src/a.py")?;
        let file = ProgramFile::new(db, file, env.program(db));
        let u = global_symbol(db, file, "C")
            .place
            .expect_type()
            .as_class_literal()
            .and_then(|class| class.generic_context(db))
            .and_then(|context| context.variables(db).next())
            .ok_or_else(|| anyhow::anyhow!("expected C's U"))?;
        let source = global_symbol(db, file, "source")
            .place
            .expect_type()
            .as_function_literal()
            .ok_or_else(|| anyhow::anyhow!("expected source"))?;
        let signature = source
            .signature(db)
            .overloads
            .first()
            .ok_or_else(|| anyhow::anyhow!("expected source's signature"))?;
        let t = create_typevar(db, "T");
        let int = KnownClass::Int.to_instance(db, &env);
        for parameter in signature.parameters() {
            let alias = parameter.annotated_type();
            let inferable = TypeVarSet::from_typevars(db, [t, u]);
            assert_eq!(
                Resolver::resolve(
                    db,
                    &mut Dependencies::new(&env),
                    inferable,
                    &[binding(t, alias)]
                )
                .as_ref(),
                [SolutionType::Unresolved(alias)],
                "{parameter:?}: missing captured dependency"
            );
            // Specializing an alias's arguments cannot replace a variable captured in its body.
            assert_eq!(
                Resolver::resolve(
                    db,
                    &mut Dependencies::new(&env),
                    inferable,
                    &[binding(t, alias), binding(u, int)]
                )
                .as_ref(),
                [SolutionType::Unresolved(alias), SolutionType::Resolved(int)],
                "{parameter:?}: retained captured dependency"
            );
        }
        Ok(())
    }

    #[test]
    fn closed_recursive_alias_retains_its_identity() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
            type Tree = int | list[Tree]
            value: Tree
            type Growing[V] = V | list[Growing[list[V]]]
            growing: Growing[int]
            "#,
        )?;
        let db = &db;
        let env = db.program_environment();
        let file = system_path_to_file(db, "/src/a.py")?;
        let file = ProgramFile::new(db, file, env.program(db));
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let int = KnownClass::Int.to_instance(db, &env);
        for name in ["value", "growing"] {
            let tree = global_symbol(db, file, name).place.expect_type();
            let resolved = Resolver::resolve(
                db,
                &mut Dependencies::new(&env),
                TypeVarSet::from_typevars(db, [t]),
                &[binding(t, tree)],
            );
            assert_eq!(resolved.as_ref(), [SolutionType::Resolved(tree)]);

            // A closed recursive alias does not prevent resolving an independent tuple element.
            let pair = Type::tuple(TupleType::heterogeneous(db, &env, [tree, Type::TypeVar(u)]));
            let expected = Type::tuple(TupleType::heterogeneous(db, &env, [tree, int]));
            let resolved = Resolver::resolve(
                db,
                &mut Dependencies::new(&env),
                TypeVarSet::from_typevars(db, [t, u]),
                &[binding(t, pair), binding(u, int)],
            );
            assert_eq!(
                resolved.as_ref(),
                [
                    SolutionType::Resolved(expected),
                    SolutionType::Resolved(int)
                ]
            );
        }
        Ok(())
    }

    #[test]
    fn partial_resolves_dependencies_in_its_wrapped_callable() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
            from typing import Callable

            def wrapped[U](value: U) -> int: ...
            reduced: Callable[[], int]
            "#,
        )?;
        let db = &db;
        let env = db.program_environment();
        let file = system_path_to_file(db, "/src/a.py")?;
        let file = ProgramFile::new(db, file, env.program(db));
        let wrapped = global_symbol(db, file, "wrapped").place.expect_type();
        let u = wrapped
            .as_function_literal()
            .and_then(|function| function.signature(db).overloads.first()?.generic_context)
            .and_then(|context| context.variables(db).next())
            .ok_or_else(|| anyhow::anyhow!("expected wrapped's U"))?;
        let reduced = global_symbol(db, file, "reduced")
            .place
            .expect_type()
            .as_callable()
            .ok_or_else(|| anyhow::anyhow!("expected reduced callable"))?;
        let Type::KnownInstance(KnownInstanceType::FunctoolsPartial(partial)) =
            reduced.into_precise_functools_partial_instance(db, wrapped)
        else {
            anyhow::bail!("expected a precise partial instance");
        };
        let t = create_typevar(db, "T");
        let int = KnownClass::Int.to_instance(db, &env);

        // Binding the parameter removes U from the reduced signature, but .func still exposes it.
        for partial in [
            KnownInstanceType::FunctoolsPartial(partial),
            KnownInstanceType::FunctoolsPartialCall(partial),
        ] {
            let resolved = Resolver::resolve(
                db,
                &mut Dependencies::new(&env),
                TypeVarSet::from_typevars(db, [t, u]),
                &[binding(t, Type::KnownInstance(partial)), binding(u, int)],
            );
            let [
                SolutionType::Resolved(Type::KnownInstance(
                    KnownInstanceType::FunctoolsPartial(mapped)
                    | KnownInstanceType::FunctoolsPartialCall(mapped),
                )),
                SolutionType::Resolved(resolved_u),
            ] = resolved.as_ref()
            else {
                anyhow::bail!("expected resolved partial and U");
            };
            let parameter = mapped
                .wrapped(db)
                .inner(db)
                .as_function_literal()
                .and_then(|function| function.signature(db).overloads.first())
                .and_then(|signature| signature.parameters().iter().next())
                .ok_or_else(|| anyhow::anyhow!("expected wrapped callable's parameter"))?;
            assert_eq!(parameter.annotated_type(), int);
            assert_eq!(mapped.partial(db), reduced);
            assert_eq!(*resolved_u, int);
        }
        Ok(())
    }
}
