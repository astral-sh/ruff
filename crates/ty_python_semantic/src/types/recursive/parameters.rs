//! Reduce constructor parameters over the finite bodies of nested recursive binders.
//!
//! A parameter is live if it occurs outside recursive arguments, or flows to another live
//! parameter. Equal arguments can share a parameter only when every entry and recursive call
//! preserves that equality. Partition refinement proves this without unfolding recursive types.

use std::cell::RefCell;

use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    RecursiveCycle, RecursiveMapping, RecursiveMappingReference, RecursiveSubstitution,
    RecursiveType, RecursiveTypeMapping,
};
use crate::types::generics::{ApplySpecialization, Specialization, walk_specialization_types};
use crate::types::visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard};
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, BoundTypeVarInstance, GenericContext, Type,
    TypeAliasType, TypeContext, TypeMapping,
};
use crate::{Db, FxOrderMap, FxOrderSet, ProgramEnvironment};

struct ParameterBinder<'db> {
    original: Option<RecursiveType<'db>>,
    cycle: RecursiveCycle,
    parent: Option<usize>,
    children: Vec<usize>,
    body: Type<'db>,
    reference: RecursiveMappingReference<'db>,
    renaming: Specialization<'db>,
    variables: Vec<BoundTypeVarInstance<'db>>,
}

#[derive(Clone, Copy)]
enum ParameterBinderSource<'db> {
    Root {
        body: Type<'db>,
        reference: RecursiveMappingReference<'db>,
    },
    Nested(RecursiveType<'db>),
}

impl<'db> ParameterBinder<'db> {
    fn new(
        db: &'db dyn Db,
        mapping: &RecursiveTypeMapping<'_, 'db>,
        env: &ProgramEnvironment<'db>,
        source: ParameterBinderSource<'db>,
        parent: Option<(usize, Specialization<'db>)>,
    ) -> Self {
        let (body, cycle, parameters, original) = match source {
            ParameterBinderSource::Root { body, reference } => (
                body,
                reference.cycle(db),
                reference
                    .arguments(db)
                    .map(|arguments| arguments.generic_context(db)),
                None,
            ),
            ParameterBinderSource::Nested(recursive) => (
                recursive.body(db),
                recursive.cycle(db),
                recursive.parameters(db),
                Some(recursive),
            ),
        };
        let index = mapping.next_binder.get();
        mapping.next_binder.set(index + 1);
        let variables = parameters
            .into_iter()
            .flat_map(|parameters| parameters.variables(db))
            .map(|variable| variable.with_name_suffix(db, &format!("$reduced{index}")))
            .collect::<Vec<_>>();
        let mut renaming = FxOrderMap::default();
        if let Some((_, parent)) = parent {
            renaming.extend(
                parent
                    .generic_context(db)
                    .variables(db)
                    .zip(parent.types(db))
                    .map(|(variable, ty)| (variable.identity(db), (variable, *ty))),
            );
        }
        if let Some(parameters) = parameters {
            renaming.extend(
                parameters
                    .variables(db)
                    .zip(&variables)
                    .map(|(old, new)| (old.identity(db), (old, Type::TypeVar(*new)))),
            );
        }
        let renaming = Specialization::new(
            db,
            GenericContext::from_typevar_instances(db, env, renaming.values().map(|(old, _)| *old)),
            renaming.values().map(|(_, new)| *new).collect::<Box<[_]>>(),
            None,
            None,
        );
        let arguments = parameters.map(|_| {
            GenericContext::from_typevar_instances(db, env, variables.iter().copied())
                .identity_specialization(db)
        });
        Self {
            original,
            cycle,
            parent: parent.map(|(index, _)| index),
            children: Vec::new(),
            body,
            reference: RecursiveMappingReference::new(db, mapping.scope, index, arguments, None),
            renaming,
            variables,
        }
    }
}

pub(super) struct RecursiveParameters<'a, 'db> {
    mapping: &'a RecursiveTypeMapping<'a, 'db>,
    visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
    binders: Vec<ParameterBinder<'db>>,
}

impl<'db> RecursiveParameters<'_, 'db> {
    pub(super) fn reduce(
        db: &'db dyn Db,
        mapping: &RecursiveTypeMapping<'_, 'db>,
        body: Type<'db>,
        reference: RecursiveMappingReference<'db>,
        arguments: Specialization<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> (
        Type<'db>,
        RecursiveMappingReference<'db>,
        Option<Specialization<'db>>,
    ) {
        let mut reduction = RecursiveParameters {
            mapping,
            visitor,
            binders: vec![ParameterBinder::new(
                db,
                mapping,
                visitor.env,
                ParameterBinderSource::Root { body, reference },
                None,
            )],
        };
        reduction.abstract_binders(db);
        let variables = reduction
            .binders
            .iter()
            .flat_map(|binder| binder.variables.iter().copied())
            .collect::<Vec<_>>();
        let indices = variables
            .iter()
            .enumerate()
            .map(|(index, variable)| (variable.identity(db), index))
            .collect::<FxHashMap<_, _>>();
        let targets = reduction
            .binders
            .iter()
            .enumerate()
            .map(|(index, binder)| (binder.reference.cycle(db), index))
            .collect::<FxHashMap<_, _>>();
        let entry = reduction.binders[0]
            .reference
            .rebind_arguments(db, Some(arguments));
        let used = RefCell::new(FxHashSet::default());
        let calls = RefCell::new(FxOrderSet::from_iter([(0, entry.arguments(db))]));
        {
            let references = mapping.references.borrow();
            let uses = ParameterUses {
                env: visitor.env,
                references: &references,
                targets: &targets,
                indices: &indices,
                used: &used,
                calls: &calls,
                seen: TypeCollector::default(),
            };
            for binder in &reduction.binders {
                uses.visit_type(db, binder.body);
            }
            loop {
                let previous = (used.borrow().len(), calls.borrow().len());
                let pending = calls.borrow().iter().copied().collect::<Vec<_>>();
                for (target, arguments) in pending {
                    let Some(arguments) = arguments else {
                        continue;
                    };
                    for (variable, argument) in reduction.binders[target]
                        .variables
                        .iter()
                        .zip(arguments.types(db))
                    {
                        if used.borrow().contains(&indices[&variable.identity(db)]) {
                            uses.visit_type(db, *argument);
                        }
                    }
                }
                if previous == (used.borrow().len(), calls.borrow().len()) {
                    break;
                }
            }
        }
        let used = used.into_inner();
        let calls = calls.into_inner();
        let mut representatives = (0..variables.len()).collect::<Vec<_>>();
        for binder in &reduction.binders {
            for (position, variable) in binder.variables.iter().enumerate() {
                let index = indices[&variable.identity(db)];
                if used.contains(&index)
                    && let Some(previous) = binder.variables[..position].iter().find(|previous| {
                        used.contains(&indices[&previous.identity(db)])
                            && variable.is_paramspec(db) == previous.is_paramspec(db)
                            && variable.is_typevartuple(db) == previous.is_typevartuple(db)
                    })
                {
                    representatives[index] = representatives[indices[&previous.identity(db)]];
                }
            }
        }
        let context =
            GenericContext::from_typevar_instances(db, visitor.env, variables.iter().copied());
        loop {
            let substitution = Specialization::new(
                db,
                context,
                representatives
                    .iter()
                    .map(|index| Type::TypeVar(variables[*index]))
                    .collect::<Box<[_]>>(),
                None,
                None,
            );
            let mapping =
                TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(substitution));
            let mut transitions = vec![Vec::new(); variables.len()];
            for (target, arguments) in &calls {
                let Some(arguments) = arguments else {
                    continue;
                };
                let mapping_visitor = reduction.mapping.visitor(visitor);
                for (variable, argument) in reduction.binders[*target]
                    .variables
                    .iter()
                    .zip(arguments.types(db))
                {
                    let index = indices[&variable.identity(db)];
                    if used.contains(&index) {
                        transitions[index].push(argument.apply_type_mapping_impl(
                            db,
                            &mapping,
                            TypeContext::default(),
                            &mapping_visitor,
                        ));
                    }
                }
            }
            let mut refined = (0..variables.len()).collect::<Vec<_>>();
            for index in 0..variables.len() {
                if used.contains(&index)
                    && let Some(previous) = (0..index).find(|previous| {
                        representatives[*previous] == representatives[index]
                            && transitions[*previous] == transitions[index]
                    })
                {
                    refined[index] = refined[previous];
                }
            }
            if refined == representatives {
                break;
            }
            representatives = refined;
        }
        if (0..variables.len())
            .all(|index| used.contains(&index) && representatives[index] == index)
        {
            return (body, reference, Some(arguments));
        }
        let substitution = Specialization::new(
            db,
            context,
            representatives
                .iter()
                .map(|index| Type::TypeVar(variables[*index]))
                .collect::<Box<[_]>>(),
            None,
            None,
        );
        let substitution =
            TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(substitution));
        let mut restored = FxHashMap::default();
        for index in (0..reduction.binders.len()).rev() {
            let binder = &reduction.binders[index];
            let retained = binder
                .variables
                .iter()
                .copied()
                .filter(|variable| {
                    let index = indices[&variable.identity(db)];
                    used.contains(&index) && representatives[index] == index
                })
                .collect::<Vec<_>>();
            let parameters = (!retained.is_empty())
                .then(|| GenericContext::from_typevar_instances(db, visitor.env, retained));
            let reference = binder.reference.with_arguments(
                db,
                parameters.map(|parameters| parameters.identity_specialization(db)),
            );
            let mut body = binder.body.apply_type_mapping_impl(
                db,
                &substitution,
                TypeContext::default(),
                &reduction.mapping.visitor(visitor),
            );
            for child in &binder.children {
                if let Some((reference, source)) = restored.get(child).copied() {
                    let restore = TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                        RecursiveSubstitution::Restore {
                            placeholder: reference,
                            source,
                        },
                    ));
                    body = body.apply_type_mapping_impl(
                        db,
                        &restore,
                        TypeContext::default(),
                        &reduction.mapping.visitor(visitor),
                    );
                }
            }
            let Some(original) = binder.original else {
                return (
                    body,
                    reference,
                    reference.project_arguments(db, entry.arguments(db)),
                );
            };
            let bind = TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::BindFresh(reference),
            ));
            let body = body.apply_type_mapping_impl(
                db,
                &bind,
                TypeContext::default(),
                &reduction.mapping.visitor(visitor),
            );
            let recursive = RecursiveType::new_internal(
                db,
                original.origin(db),
                reference.cycle(db),
                body,
                reference.arguments(db),
                None,
            );
            restored.insert(index, (reference, Type::Recursive(recursive)));
        }
        (body, reference, Some(arguments))
    }

    /// Close and name each stored body once; recursive calls remain opaque references.
    fn abstract_binders(&mut self, db: &'db dyn Db) {
        let mut index = 0;
        while index < self.binders.len() {
            let mut body = self.binders[index].body;
            let mut scope = Some(index);
            while let Some(current) = scope {
                let binder = &self.binders[current];
                let close = TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                    RecursiveSubstitution::Close {
                        cycle: binder.cycle,
                        placeholder: binder.reference,
                    },
                ));
                body = body.apply_type_mapping_impl(
                    db,
                    &close,
                    TypeContext::default(),
                    &self.mapping.visitor(self.visitor),
                );
                scope = binder.parent;
            }
            let children = {
                let references = self.mapping.references.borrow();
                let children = ChildBinders {
                    env: self.visitor.env,
                    references: &references,
                    found: RefCell::default(),
                    seen: TypeCollector::default(),
                };
                children.visit_type(db, body);
                children.found.into_inner()
            };
            for original in children {
                let child = self.binders.len();
                let binder = ParameterBinder::new(
                    db,
                    self.mapping,
                    self.visitor.env,
                    ParameterBinderSource::Nested(original),
                    Some((index, self.binders[index].renaming)),
                );
                let abstract_child = TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                    RecursiveSubstitution::Abstract {
                        constructor: original,
                        placeholder: binder.reference,
                    },
                ));
                self.binders.push(binder);
                self.binders[index].children.push(child);
                body = body.apply_type_mapping_impl(
                    db,
                    &abstract_child,
                    TypeContext::default(),
                    &self.mapping.visitor(self.visitor),
                );
            }
            self.binders[index].body = body.apply_type_mapping_impl(
                db,
                &TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(
                    self.binders[index].renaming,
                )),
                TypeContext::default(),
                &self.mapping.visitor(self.visitor),
            );
            index += 1;
        }
    }
}

struct ChildBinders<'a, 'db> {
    env: &'a ProgramEnvironment<'db>,
    references: &'a FxHashMap<Type<'db>, RecursiveMappingReference<'db>>,
    found: RefCell<FxOrderSet<RecursiveType<'db>>>,
    seen: TypeCollector<'db>,
}

impl<'db> TypeVisitor<'db> for ChildBinders<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }
    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }
    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        if let Some(reference) = self.references.get(&ty) {
            if let Some(arguments) = reference.arguments(db) {
                walk_specialization_types(db, arguments, self);
            }
        } else if let Type::RecursiveVar(variable) = ty {
            if let Some(arguments) = variable.arguments(db) {
                walk_specialization_types(db, arguments, self);
            }
        } else {
            walk_type_with_recursion_guard(db, ty, self, &self.seen);
        }
    }
    fn visit_recursive_type(&self, db: &'db dyn Db, recursive: RecursiveType<'db>) {
        if recursive.alias(db).is_none() {
            self.found.borrow_mut().insert(recursive.constructor(db));
        }
        if let Some(arguments) = recursive.arguments(db) {
            walk_specialization_types(db, arguments, self);
        }
    }
    fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
        if let Some(arguments) = alias.specialization(db) {
            walk_specialization_types(db, arguments, self);
        }
    }
}

struct ParameterUses<'a, 'db> {
    env: &'a ProgramEnvironment<'db>,
    references: &'a FxHashMap<Type<'db>, RecursiveMappingReference<'db>>,
    targets: &'a FxHashMap<RecursiveCycle, usize>,
    indices: &'a FxHashMap<BoundTypeVarIdentity<'db>, usize>,
    used: &'a RefCell<FxHashSet<usize>>,
    calls: &'a RefCell<FxOrderSet<(usize, Option<Specialization<'db>>)>>,
    seen: TypeCollector<'db>,
}

impl<'db> TypeVisitor<'db> for ParameterUses<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }
    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }
    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        if let Some(reference) = self.references.get(&ty) {
            if let Some(target) = self.targets.get(&reference.cycle(db)) {
                self.calls
                    .borrow_mut()
                    .insert((*target, reference.arguments(db)));
            } else if let Some(arguments) = reference.arguments(db) {
                walk_specialization_types(db, arguments, self);
            }
            return;
        }
        if let Type::TypeVar(variable) = ty {
            let mut identity = variable.identity(db);
            identity.paramspec_attr = None;
            if let Some(index) = self.indices.get(&identity) {
                self.used.borrow_mut().insert(*index);
            }
            return;
        }
        walk_type_with_recursion_guard(db, ty, self, &self.seen);
    }
    fn visit_recursive_type(&self, db: &'db dyn Db, recursive: RecursiveType<'db>) {
        if let Some(arguments) = recursive.arguments(db) {
            walk_specialization_types(db, arguments, self);
        }
    }
    fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
        if let Some(arguments) = alias.specialization(db) {
            walk_specialization_types(db, arguments, self);
        }
    }
}
