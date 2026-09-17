//! Resolve dependencies between the selected bindings of one solution alternative.

use std::cell::{Cell, RefCell};

use rustc_hash::FxHashMap;

use super::TypeVarSolution;
use crate::types::cyclic::CycleDetector;
use crate::types::function::FunctionType;
use crate::types::generics::{ApplySpecialization, GenericContext};
use crate::types::known_instance::walk_known_instance_type;
use crate::types::signatures::{Signature, walk_signature};
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarSet};
use crate::types::visitor::{TypeKind, TypeVisitor, walk_non_atomic_type};
use crate::types::{
    BoundTypeVarInstance, CallableType, KnownInstanceType, Type, TypeAliasType, TypeContext,
    TypeMapping,
};
use crate::{Db, FxOrderMap, ProgramEnvironment};

/// Whether a selected type is independent of the other inferable variables in its alternative.
///
/// This does not describe budget completeness or apply defaults to variables without evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum SolutionType<'db> {
    Resolved(Type<'db>),
    /// The original selected type, retained when dependencies are missing, cyclic, or cannot be
    /// substituted through a type form that preserves captured references.
    Unresolved(Type<'db>),
}

/// Resolves only dependencies with selected, acyclic solutions. The result has the same order as
/// `solution`; references outside `inferable` retain their original bound-variable identity.
pub(crate) fn resolve_solution<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    inferable: TypeVarSet<'db>,
    solution: &[TypeVarSolution<'db>],
) -> Box<[SolutionType<'db>]> {
    let resolver = Resolver {
        env,
        inferable,
        solution,
        indices: solution
            .iter()
            .enumerate()
            .map(|(index, binding)| (binding.bound_typevar.identity(db), index))
            .collect(),
        resolved: CycleDetector::new(None),
    };
    solution
        .iter()
        .enumerate()
        .map(|(index, binding)| {
            resolver.resolve(db, index).map_or(
                SolutionType::Unresolved(binding.solution),
                SolutionType::Resolved,
            )
        })
        .collect()
}

struct ResolveBinding;

struct Resolver<'a, 'db> {
    env: &'a ProgramEnvironment<'db>,
    inferable: TypeVarSet<'db>,
    solution: &'a [TypeVarSolution<'db>],
    indices: FxHashMap<BoundTypeVarIdentity<'db>, usize>,
    resolved: CycleDetector<'db, ResolveBinding, Type<'db>, Option<Type<'db>>, 3>,
}

impl<'db> Resolver<'_, 'db> {
    fn resolve(&self, db: &'db dyn Db, index: usize) -> Option<Type<'db>> {
        let binding = &self.solution[index];
        self.resolved
            .visit(db, Type::TypeVar(binding.bound_typevar), || {
                let original = binding.solution;
                let replacements = RefCell::new(FxOrderMap::default());
                if !Dependencies::check(db, self.env, self.inferable, original, |dependency| {
                    let Some(&index) = self.indices.get(&dependency.identity(db)) else {
                        return false;
                    };
                    let Some(ty) = self.resolve(db, index) else {
                        return false;
                    };
                    replacements.borrow_mut().insert(index, ty);
                    true
                }) {
                    return None;
                }
                let replacements = replacements.into_inner();
                if replacements.is_empty() {
                    return Some(original);
                }

                // Every replacement is already closed. One simultaneous substitution therefore
                // suffices, and never substitutes a cycle with an arbitrary representative.
                let context = GenericContext::from_typevar_instances(
                    db,
                    self.env,
                    replacements
                        .keys()
                        .map(|index| self.solution[*index].bound_typevar),
                );
                let types: Vec<_> = replacements.values().copied().collect();
                let mapped = original.apply_type_mapping(
                    db,
                    self.env,
                    &TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
                        generic_context: context,
                        types: &types,
                        skip: None,
                    }),
                    TypeContext::default(),
                );
                // Some type forms preserve captured variables when specialized. For example, an
                // alias changes its explicit arguments but can retain a free variable in its body.
                // Verify closure on the actual result without performing further substitutions.
                Dependencies::check(db, self.env, self.inferable, mapped, |_| false)
                    .then_some(mapped)
            })
    }
}

struct VisitDependencies;

/// Visits occurrences of inferable variables, leaving their declarations' bounds and defaults alone.
struct Dependencies<'a, 'db> {
    env: &'a ProgramEnvironment<'db>,
    inferable: TypeVarSet<'db>,
    query: &'a dyn Fn(BoundTypeVarInstance<'db>) -> bool,
    satisfied: Cell<bool>,
    visited: CycleDetector<'db, VisitDependencies, Type<'db>, (), 3>,
}

impl<'db> Dependencies<'_, 'db> {
    fn check(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
        ty: Type<'db>,
        query: impl Fn(BoundTypeVarInstance<'db>) -> bool,
    ) -> bool {
        let visitor = Dependencies {
            env,
            inferable,
            query: &query,
            satisfied: Cell::new(true),
            visited: CycleDetector::new(()),
        };
        visitor.visit_type(db, ty);
        visitor.satisfied.get()
    }

    fn signature(&self, db: &'db dyn Db, signature: &Signature<'db>) {
        walk_signature(db, signature, self);
        for parameter in signature.parameters() {
            if let Some(default) = parameter.eager_default_type() {
                self.visit_type(db, default);
            }
        }
    }
}

impl<'db> TypeVisitor<'db> for Dependencies<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        if !self.satisfied.get() {
            return;
        }
        // Recursive specialization can introduce dependencies in an alias's changing arguments.
        // Inspect them even when the recursion guard skips another visit to the alias's body.
        if let Type::TypeAlias(alias) = ty
            && let Some(specialization) = alias.specialization(db)
        {
            for argument in specialization.types(db) {
                self.visit_type(db, *argument);
            }
        }
        if let Type::TypeVar(typevar) = ty {
            if typevar.is_inferable(db, self.inferable) {
                self.satisfied.set((self.query)(typevar));
            }
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

    use super::{SolutionType, resolve_solution};
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
            solution,
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
                let resolved = resolve_solution(
                    db,
                    &env,
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
            let resolved = resolve_solution(
                db,
                &env,
                TypeVarSet::from_typevars(db, [t]),
                &[binding(t, tree)],
            );
            assert_eq!(resolved.as_ref(), [SolutionType::Resolved(tree)]);

            // A closed recursive alias does not prevent resolving an independent tuple element.
            let pair = Type::tuple(TupleType::heterogeneous(db, &env, [tree, Type::TypeVar(u)]));
            let expected = Type::tuple(TupleType::heterogeneous(db, &env, [tree, int]));
            let resolved = resolve_solution(
                db,
                &env,
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
            let resolved = resolve_solution(
                db,
                &env,
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
