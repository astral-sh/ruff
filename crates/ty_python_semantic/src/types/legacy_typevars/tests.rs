use std::cell::{Cell, RefCell};
use std::convert::Infallible;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::{PythonVersion, name::Name};
use salsa::Database as _;
use ty_python_core::{ProgramFile, definition::Definition};

use super::{
    InlineLegacyTypeVarEffects, LegacyTypeVarDependency, LegacyTypeVarEffects, LegacyTypeVarWork,
    collect_with_visitor, find_legacy_typevars_with,
};
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::place::global_symbol;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::instance::{NominalVisitorChildren, Protocol, ProtocolInterfaceSource};
use crate::types::known_instance::InternedType;
use crate::types::protocol_class::{LegacyProtocolTestMember, ProtocolClass, ProtocolInterface};
use crate::types::tuple::{Tuple, TupleSpec, TupleType, VariableSegment};
use crate::types::typevar::{
    BindingContext, ParamSpecAttrKind, TypeVarBoundOrConstraints, TypeVarDefaultEvaluation,
    TypeVarIdentity, TypeVarInstance, TypeVarKind, TypeVarNonce,
};
use crate::types::{
    BoundMethodType, BoundTypeVarInstance, CallableType, ClassLiteral, ClassType, DynamicType,
    FindLegacyTypeVarsVisitor, GenericAlias, GenericContext, IntersectionBuilder,
    KnownBoundMethodType, KnownInstanceType, MaterializationKind, Parameter, Parameters,
    ProtocolInstanceType, Signature, Specialization, StaticClassLiteral, SubclassOfInner,
    SubclassOfType, Type, TypeFormType, TypedDictType, UnionType,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

fn database(source: &str) -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/legacy.py", source)
        .build()
}

fn symbol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/legacy.py")?,
        env.program(db),
    );
    global_symbol(db, file, name)
        .place
        .ignore_possibly_undefined()
        .ok_or_else(|| anyhow::anyhow!("missing symbol {name}"))
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    match symbol(db, name)? {
        Type::ClassLiteral(ClassLiteral::Static(class)) => Ok(class),
        _ => anyhow::bail!("{name} is not a static class"),
    }
}

fn variable<'db>(
    db: &'db TestDb,
    name: &str,
    kind: TypeVarKind,
    context: BindingContext<'db>,
) -> BoundTypeVarInstance<'db> {
    BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
            db,
            TypeVarIdentity::new(db, Name::new(name), None, kind),
            None,
            None,
            None,
        ),
        context,
        None,
        TypeVarNonce::NONE,
    )
}

fn tuple<'db>(db: &'db TestDb, types: impl IntoIterator<Item = Type<'db>>) -> Type<'db> {
    Type::tuple(TupleType::heterogeneous(
        db,
        &db.program_environment(),
        types,
    ))
}

fn executions(db: &TestDb) -> Vec<String> {
    db.clone()
        .take_salsa_events()
        .into_iter()
        .filter_map(|event| {
            let salsa::EventKind::WillExecute { database_key } = event.kind else {
                return None;
            };
            Some(
                db.ingredient_debug_name(database_key.ingredient_index())
                    .into_owned(),
            )
        })
        .collect()
}

// Retain the original recursive decisions for these fixtures. Stored descendants recurse
// here, not through the extracted collector. Function signatures retain their ordinary query.
fn original<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    context: Option<Definition<'db>>,
    variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    visitor: &FindLegacyTypeVarsVisitor<'db>,
) -> anyhow::Result<()> {
    match ty {
        Type::TypeVar(variable) => {
            let matches_context = context.is_none_or(|definition| {
                variable.binding_context(db) == BindingContext::Definition(definition)
            });
            match variable.typevar(db).kind(db) {
                TypeVarKind::LegacyTypeVar
                | TypeVarKind::LegacyTypeVarTuple
                | TypeVarKind::TypingSelf
                | TypeVarKind::Pep613Alias
                    if matches_context =>
                {
                    variables.insert(variable);
                }
                TypeVarKind::LegacyParamSpec if matches_context => {
                    variables.insert(variable.without_paramspec_attr(db));
                }
                _ => {}
            }
        }
        Type::GenericAlias(alias) => original_specialization(
            db,
            env,
            alias.specialization(db),
            context,
            variables,
            visitor,
        )?,
        Type::NominalInstance(instance) => match instance.children_for_visitor(db) {
            NominalVisitorChildren::None => {}
            NominalVisitorChildren::Class(class) => {
                original(db, env, class, context, variables, visitor)?;
            }
            NominalVisitorChildren::Tuple(tuple) => {
                original_tuple(db, env, tuple, context, variables, visitor)?;
            }
        },
        Type::ProtocolInstance(protocol) => {
            original_protocol(db, env, protocol, context, variables, visitor)?;
        }
        Type::SubclassOf(subclass) => match subclass.subclass_of() {
            SubclassOfInner::Protocol(protocol) => {
                original_protocol(db, env, protocol, context, variables, visitor)?;
            }
            _ => anyhow::bail!("fixture outside the retained recursive oracle: {ty:?}"),
        },
        Type::TypeForm(form) => {
            original(db, env, form.type_argument(db), context, variables, visitor)?;
        }
        Type::TypeIs(inner) => original(
            db,
            env,
            inner.type_argument(db),
            context,
            variables,
            visitor,
        )?,
        Type::TypeGuard(inner) => {
            original(db, env, inner.return_type(db), context, variables, visitor)?;
        }
        Type::Union(union) => {
            for &child in union.elements(db) {
                original(db, env, child, context, variables, visitor)?;
            }
        }
        Type::Intersection(intersection) => {
            for &child in intersection
                .positive(db)
                .iter()
                .chain(intersection.negative(db))
            {
                original(db, env, child, context, variables, visitor)?;
            }
        }
        Type::Dynamic(DynamicType::UnknownGeneric(generic)) => {
            for variable in generic.variables(db) {
                original(
                    db,
                    env,
                    Type::TypeVar(variable),
                    context,
                    variables,
                    visitor,
                )?;
            }
        }
        Type::KnownInstance(
            KnownInstanceType::Annotated(inner)
            | KnownInstanceType::TypeGenericAlias(inner)
            | KnownInstanceType::LiteralStringAlias(inner),
        ) => {
            original(db, env, inner.inner(db), context, variables, visitor)?;
        }
        Type::Callable(callable) | Type::KnownInstance(KnownInstanceType::Callable(callable)) => {
            for signature in &callable.signatures(db).overloads {
                original_signature(db, env, signature, context, variables, visitor)?;
            }
        }
        Type::FunctionLiteral(function) => {
            let mut result: anyhow::Result<()> = Ok(());
            visitor.visit(db, ty, || {
                result = (|| {
                    for signature in &function.signature(db).overloads {
                        original_signature(db, env, signature, context, variables, visitor)?;
                    }
                    Ok(())
                })();
            });
            result?;
        }
        Type::BoundMethod(method) => {
            let mut result: anyhow::Result<()> = Ok(());
            visitor.visit(db, ty, || {
                result = original(
                    db,
                    env,
                    method.self_instance(db),
                    context,
                    variables,
                    visitor,
                )
                .and_then(|()| original(db, env, method.func(db), context, variables, visitor));
            });
            result?;
        }
        Type::KnownBoundMethod(KnownBoundMethodType::MethodTypeDunderGet(method)) => {
            let mut result: anyhow::Result<()> = Ok(());
            visitor.visit(db, ty, || {
                result = original(
                    db,
                    env,
                    Type::BoundMethod(method),
                    context,
                    variables,
                    visitor,
                );
            });
            result?;
        }
        Type::TypeAlias(alias) => {
            let mut result: anyhow::Result<()> = Ok(());
            visitor.visit(db, ty, || {
                result = original(db, env, alias.value_type(db), context, variables, visitor);
            });
            result?;
        }
        Type::ClassLiteral(_)
        | Type::LiteralValue(_)
        | Type::Never
        | Type::Dynamic(_)
        | Type::NewTypeInstance(_)
        | Type::TypedDict(TypedDictType::Synthesized(_))
        | Type::KnownInstance(
            KnownInstanceType::SubscriptedProtocol(_)
            | KnownInstanceType::SubscriptedGeneric(_)
            | KnownInstanceType::TypeVar(_)
            | KnownInstanceType::TypeAliasType(_),
        ) => {}
        _ => anyhow::bail!("fixture outside the retained recursive oracle: {ty:?}"),
    }
    Ok(())
}

/// Collects a protocol's legacy variables using the retained recursive rules.
/// Class-backed protocols traverse specialization arguments with the supplied visitor;
/// synthesized protocols use a fresh visitor for each stored member type.
fn original_protocol<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    protocol: ProtocolInstanceType<'db>,
    context: Option<Definition<'db>>,
    variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    visitor: &FindLegacyTypeVarsVisitor<'db>,
) -> anyhow::Result<()> {
    let class = match protocol.inner {
        Protocol::FromClass(class) => class,
        Protocol::Materialized(materialized) => materialized.origin(db),
        Protocol::Synthesized(synthesized) => {
            for ty in synthesized.interface().legacy_test_member_types(db) {
                original(
                    db,
                    env,
                    ty,
                    context,
                    variables,
                    &FindLegacyTypeVarsVisitor::default(),
                )?;
            }
            return Ok(());
        }
    };
    match *class {
        ClassType::NonGeneric(_) => Ok(()),
        ClassType::Generic(alias) => original_specialization(
            db,
            env,
            alias.specialization(db),
            context,
            variables,
            visitor,
        ),
    }
}

fn original_specialization<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    specialization: Specialization<'db>,
    context: Option<Definition<'db>>,
    variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    visitor: &FindLegacyTypeVarsVisitor<'db>,
) -> anyhow::Result<()> {
    if let Some(tuple) = specialization.tuple(db) {
        original_tuple(db, env, tuple, context, variables, visitor)
    } else {
        for &child in specialization.types(db) {
            original(db, env, child, context, variables, visitor)?;
        }
        Ok(())
    }
}

fn original_tuple<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    tuple: &TupleSpec<'db>,
    context: Option<Definition<'db>>,
    variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    visitor: &FindLegacyTypeVarsVisitor<'db>,
) -> anyhow::Result<()> {
    match tuple {
        Tuple::Fixed(tuple) => {
            for &child in tuple.all_elements() {
                original(db, env, child, context, variables, visitor)?;
            }
        }
        Tuple::Variable(tuple) => {
            for &child in tuple.prefix_elements() {
                original(db, env, child, context, variables, visitor)?;
            }
            let variable = match tuple.variable() {
                VariableSegment::Homogeneous(ty) => ty,
                VariableSegment::TypeVarTuple(variable) => Type::TypeVar(variable),
            };
            original(db, env, variable, context, variables, visitor)?;
            for &child in tuple.suffix_elements() {
                original(db, env, child, context, variables, visitor)?;
            }
        }
    }
    Ok(())
}

fn original_signature<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    signature: &Signature<'db>,
    context: Option<Definition<'db>>,
    variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    visitor: &FindLegacyTypeVarsVisitor<'db>,
) -> anyhow::Result<()> {
    for ty in signature.receiver_constraint_types() {
        original(db, env, ty, context, variables, visitor)?;
    }
    for parameter in signature.parameters() {
        original(
            db,
            env,
            parameter.annotated_type(),
            context,
            variables,
            visitor,
        )?;
        if let Some(default) = parameter.eager_default_type() {
            original(db, env, default, context, variables, visitor)?;
        }
    }
    original(db, env, signature.return_ty, context, variables, visitor)
}

fn collected<'db>(
    db: &'db TestDb,
    root: Type<'db>,
    context: Option<Definition<'db>>,
) -> Vec<BoundTypeVarInstance<'db>> {
    let mut variables = FxOrderSet::default();
    infallible(find_legacy_typevars_with(
        db,
        &db.program_environment(),
        root,
        context,
        &mut variables,
        &InlineLegacyTypeVarEffects,
    ));
    variables.into_iter().collect()
}

fn assert_original<'db>(
    db: &'db TestDb,
    root: Type<'db>,
    context: Option<Definition<'db>>,
    expected: &[BoundTypeVarInstance<'db>],
) -> anyhow::Result<()> {
    let mut reference = FxOrderSet::default();
    original(
        db,
        &db.program_environment(),
        root,
        context,
        &mut reference,
        &FindLegacyTypeVarsVisitor::default(),
    )?;
    assert_eq!(reference.into_iter().collect::<Vec<_>>(), expected);
    assert_eq!(collected(db, root, context), expected);
    Ok(())
}

#[test]
fn candidate_kinds_ownership_and_paramspec_attributes_preserve_order() -> anyhow::Result<()> {
    let db = database("class Owner: ...\nclass Other: ...\n")?;
    let env = db.program_environment();
    let owner = class(&db, "Owner")?.definition(&db);
    let other = class(&db, "Other")?.definition(&db);
    let kinds = [
        TypeVarKind::LegacyTypeVar,
        TypeVarKind::LegacyTypeVarTuple,
        TypeVarKind::TypingSelf,
        TypeVarKind::Pep613Alias,
        TypeVarKind::LegacyParamSpec,
        TypeVarKind::Pep695TypeVar,
        TypeVarKind::Pep695TypeVarTuple,
        TypeVarKind::Pep695ParamSpec,
    ];
    let variables: Vec<_> = kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| variable(&db, &format!("T{index}"), kind, owner.into()))
        .collect();
    let foreign = variable(&db, "Foreign", TypeVarKind::LegacyTypeVar, other.into());
    let synthetic = variable(
        &db,
        "Synthetic",
        TypeVarKind::LegacyTypeVar,
        BindingContext::Synthetic(env.program(&db)),
    );
    let args = variables[4].with_paramspec_attr(&db, ParamSpecAttrKind::Args);
    let kwargs = variables[4].with_paramspec_attr(&db, ParamSpecAttrKind::Kwargs);
    let root = tuple(
        &db,
        variables
            .iter()
            .copied()
            .map(Type::TypeVar)
            .chain([foreign, args, synthetic, kwargs, variables[0]].map(Type::TypeVar)),
    );
    assert_original(&db, root, Some(owner), &variables[..5])?;
    assert_original(&db, root, Some(other), &[foreign])?;
    let mut expected = variables[..5].to_vec();
    expected.extend([foreign, synthetic]);
    assert_original(&db, root, None, &expected)?;
    Ok(())
}

#[test]
fn specialization_arguments_and_tuple_segments_skip_declared_variables() -> anyhow::Result<()> {
    let db = database("class Pair[A, B]: ...\n")?;
    let env = db.program_environment();
    let context = BindingContext::Synthetic(env.program(&db));
    let vars: Vec<_> = ["T", "U", "V", "Declared"]
        .into_iter()
        .map(|name| variable(&db, name, TypeVarKind::LegacyTypeVar, context))
        .collect();
    let pack = variable(&db, "Ts", TypeVarKind::LegacyTypeVarTuple, context);
    let declared = GenericContext::from_typevar_instances(
        &db,
        &env,
        [
            vars[3],
            variable(&db, "Declared2", TypeVarKind::LegacyTypeVar, context),
        ],
    );
    let origin = class(&db, "Pair")?;
    let first = GenericAlias::new(
        &db,
        origin,
        declared.specialize(&db, vec![Type::TypeVar(vars[1]), Type::TypeVar(vars[0])]),
    );
    let second = GenericAlias::new(
        &db,
        origin,
        declared.specialize(&db, vec![Type::TypeVar(vars[0]), Type::TypeVar(vars[2])]),
    );
    let nominal = Type::instance(&db, &env, ClassType::Generic(first));
    let segmented = Type::tuple(TupleType::mixed_with_segment(
        &db,
        &env,
        [Type::GenericAlias(first)],
        VariableSegment::TypeVarTuple(pack),
        [Type::GenericAlias(second), nominal],
    ));
    assert_original(&db, segmented, None, &[vars[1], vars[0], pack, vars[2]])?;
    let mut across_bases = FxOrderSet::default();
    for base in [Type::GenericAlias(first), Type::GenericAlias(second)] {
        infallible(find_legacy_typevars_with(
            &db,
            &env,
            base,
            None,
            &mut across_bases,
            &InlineLegacyTypeVarEffects,
        ));
    }
    assert_eq!(
        across_bases.into_iter().collect::<Vec<_>>(),
        [vars[1], vars[0], vars[2]]
    );
    let unknown = Type::Dynamic(DynamicType::UnknownGeneric(
        GenericContext::from_typevar_instances(&db, &env, [vars[2], vars[0]]),
    ));
    let wrappers = tuple(
        &db,
        [
            Type::KnownInstance(KnownInstanceType::Annotated(InternedType::new(
                &db, unknown,
            ))),
            Type::TypeForm(TypeFormType::new(&db, Type::TypeVar(vars[1]))),
            Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(declared)),
        ],
    );
    assert_original(&db, wrappers, None, &[vars[2], vars[0], vars[1]])?;
    let union =
        UnionType::from_elements(&db, &env, [Type::TypeVar(vars[1]), Type::TypeVar(vars[0])]);
    let intersection = IntersectionBuilder::new(&db, &env)
        .add_positive(Type::TypeVar(vars[2]))
        .add_negative(Type::TypeVar(vars[3]))
        .build();
    assert!(matches!(union, Type::Union(_)));
    assert!(matches!(intersection, Type::Intersection(_)));
    assert_original(
        &db,
        tuple(&db, [union, intersection]),
        None,
        &[vars[1], vars[0], vars[2], vars[3]],
    )?;
    Ok(())
}

#[test]
fn variable_bounds_and_defaults_are_not_collected_or_read() -> anyhow::Result<()> {
    let db =
        database("class Bound: ...\nclass Default: ...\nclass Scope[T: Bound = Default]: ...\n")?;
    let env = db.program_environment();
    let context = BindingContext::Synthetic(env.program(&db));
    let hidden = variable(&db, "Hidden", TypeVarKind::LegacyTypeVar, context);
    let eager = BoundTypeVarInstance::new(
        &db,
        TypeVarInstance::new(
            &db,
            TypeVarIdentity::new(
                &db,
                Name::new_static("Eager"),
                None,
                TypeVarKind::LegacyTypeVar,
            ),
            Some(TypeVarBoundOrConstraints::UpperBound(Type::TypeVar(hidden)).into()),
            None,
            Some(TypeVarDefaultEvaluation::Eager(Type::TypeVar(hidden))),
        ),
        context,
        None,
        TypeVarNonce::NONE,
    );
    let changed_bound = eager.map_bound_or_constraints(&db, |_| {
        Some(TypeVarBoundOrConstraints::UpperBound(Type::unknown()))
    });
    assert_ne!(changed_bound, eager);
    assert_eq!(changed_bound.identity(&db), eager.identity(&db));
    let declaration = class(&db, "Scope")?
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("missing context"))?;
    let lazy = declaration
        .variables(&db)
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing lazy variable"))?;
    assert!(lazy.typevar(&db).bounds_for_visitor(&db, &env, false).1);
    assert!(lazy.typevar(&db).default_for_visitor(&db, &env, false).1);
    let root = tuple(
        &db,
        [
            Type::TypeVar(eager),
            Type::TypeVar(lazy),
            Type::TypeVar(changed_bound),
            Type::TypeVar(eager),
        ],
    );
    executions(&db);
    assert_original(&db, root, None, &[eager, changed_bound])?;
    assert!(executions(&db).is_empty());
    let _ = lazy.typevar(&db).upper_bound(&db, &env);
    let _ = lazy.typevar(&db).default_type(&db, &env);
    let reads = executions(&db);
    assert!(
        reads
            .iter()
            .any(|name| name.contains("lazy_bound_unchecked")),
        "{reads:?}"
    );
    assert!(
        reads
            .iter()
            .any(|name| name.contains("lazy_default_unchecked")),
        "{reads:?}"
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event<'db> {
    Work(LegacyTypeVarWork),
    Normalize(BoundTypeVarInstance<'db>),
    Deferred,
}

struct RecordingEffects<'db> {
    events: RefCell<Vec<Event<'db>>>,
    refuse_at: Option<usize>,
    refuse_dependencies: bool,
    stop_after_dependency: bool,
    stop_after_normalize: bool,
    stopped: Cell<bool>,
}

impl RecordingEffects<'_> {
    fn new() -> Self {
        Self {
            events: RefCell::default(),
            refuse_at: None,
            refuse_dependencies: true,
            stop_after_dependency: false,
            stop_after_normalize: false,
            stopped: Cell::new(false),
        }
    }

    fn admit_event(&self) -> Result<(), ()> {
        let index = self.events.borrow().len();
        if self.stopped.get() || self.refuse_at == Some(index) {
            Err(())
        } else {
            Ok(())
        }
    }
}

impl<'db> LegacyTypeVarEffects<'db> for RecordingEffects<'db> {
    type Error = ();

    fn checkpoint(&self, work: LegacyTypeVarWork) -> Result<(), ()> {
        let result = self.admit_event();
        self.events.borrow_mut().push(Event::Work(work));
        result
    }

    fn normalize_paramspec(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, ()> {
        let result = self.admit_event();
        self.events.borrow_mut().push(Event::Normalize(variable));
        result?;
        let normalized = variable.without_paramspec_attr(db);
        self.stopped.set(self.stop_after_normalize);
        Ok(normalized)
    }

    fn deferred(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        context: Option<Definition<'db>>,
        dependency: LegacyTypeVarDependency<'db>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) -> Result<(), ()> {
        let result = self.admit_event();
        self.events.borrow_mut().push(Event::Deferred);
        result?;
        if self.refuse_dependencies {
            return Err(());
        }
        infallible(
            InlineLegacyTypeVarEffects.deferred(db, env, context, dependency, variables, visitor),
        );
        self.stopped.set(self.stop_after_dependency);
        Ok(())
    }
}

#[test]
fn every_local_checkpoint_and_normalization_can_refuse_before_later_work() {
    let db = setup_db();
    let env = db.program_environment();
    let context = BindingContext::Synthetic(env.program(&db));
    let first = variable(&db, "First", TypeVarKind::LegacyTypeVar, context);
    let second = variable(&db, "Second", TypeVarKind::LegacyTypeVar, context);
    let paramspec = variable(&db, "P", TypeVarKind::LegacyParamSpec, context);
    let args = paramspec.with_paramspec_attr(&db, ParamSpecAttrKind::Args);
    let kwargs = paramspec.with_paramspec_attr(&db, ParamSpecAttrKind::Kwargs);
    let root = tuple(&db, [first, args, first, kwargs, second].map(Type::TypeVar));
    let complete = RecordingEffects::new();
    let mut expected = FxOrderSet::default();
    assert_eq!(
        find_legacy_typevars_with(&db, &env, root, None, &mut expected, &complete),
        Ok(())
    );
    assert_eq!(
        expected.iter().copied().collect::<Vec<_>>(),
        [first, paramspec, second]
    );
    let trace = complete.events.into_inner();
    assert_eq!(trace.last(), Some(&Event::Work(LegacyTypeVarWork::Publish)));
    assert_eq!(
        trace
            .iter()
            .filter(|event| matches!(event, Event::Work(LegacyTypeVarWork::Insert { .. })))
            .count(),
        5
    );
    assert_eq!(
        trace
            .iter()
            .filter(|event| matches!(event, Event::Normalize(_)))
            .count(),
        2
    );
    for index in 0..trace.len() {
        let mut effects = RecordingEffects::new();
        effects.refuse_at = Some(index);
        let mut partial = FxOrderSet::default();
        assert_eq!(
            find_legacy_typevars_with(&db, &env, root, None, &mut partial, &effects),
            Err(())
        );
        assert_eq!(*effects.events.borrow(), trace[..=index]);
        assert_eq!(
            partial.iter().copied().collect::<Vec<_>>(),
            expected
                .iter()
                .take(partial.len())
                .copied()
                .collect::<Vec<_>>()
        );
    }
    for _ in 0..2 {
        let mut retry = FxOrderSet::default();
        assert_eq!(
            find_legacy_typevars_with(&db, &env, root, None, &mut retry, &RecordingEffects::new()),
            Ok(())
        );
        assert_eq!(retry, expected);
    }
    let mut effects = RecordingEffects::new();
    effects.stop_after_normalize = true;
    let mut partial = FxOrderSet::default();
    assert_eq!(
        find_legacy_typevars_with(&db, &env, Type::TypeVar(args), None, &mut partial, &effects),
        Err(())
    );
    assert!(partial.is_empty());
    assert_eq!(
        effects.events.borrow().last(),
        Some(&Event::Work(LegacyTypeVarWork::Resume))
    );
}

#[test]
fn foreign_paramspec_does_not_reach_normalization() -> anyhow::Result<()> {
    let db = database("class Owner: ...\nclass Other: ...\n")?;
    let env = db.program_environment();
    let owner = class(&db, "Owner")?.definition(&db);
    let foreign = variable(
        &db,
        "P",
        TypeVarKind::LegacyParamSpec,
        class(&db, "Other")?.definition(&db).into(),
    );
    let args = foreign.with_paramspec_attr(&db, ParamSpecAttrKind::Args);
    let effects = RecordingEffects::new();
    let mut variables = FxOrderSet::default();
    assert_eq!(
        find_legacy_typevars_with(
            &db,
            &env,
            Type::TypeVar(args),
            Some(owner),
            &mut variables,
            &effects
        ),
        Ok(())
    );
    assert!(variables.is_empty());
    assert!(!effects.events.borrow().iter().any(|event| matches!(
        event,
        Event::Normalize(_)
            | Event::Work(LegacyTypeVarWork::NormalizeParamSpec | LegacyTypeVarWork::Insert { .. })
    )));
    assert_eq!(
        effects.events.borrow().last(),
        Some(&Event::Work(LegacyTypeVarWork::Publish))
    );
    Ok(())
}

/// Selects the protocol entry form traversed by a test.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProtocolEntry {
    Instance,
    Subclass,
}

impl ProtocolEntry {
    const fn wrap(self, protocol: ProtocolInstanceType<'_>) -> Type<'_> {
        match self {
            Self::Instance => Type::ProtocolInstance(protocol),
            Self::Subclass => SubclassOfType::from_protocol(protocol),
        }
    }
}

/// Selects whether a class-backed protocol retains a pending materialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProtocolClassRepresentation {
    Plain,
    Top,
    Bottom,
}

impl ProtocolClassRepresentation {
    fn wrap<'db>(self, db: &'db dyn Db, class: ClassType<'db>) -> ProtocolInstanceType<'db> {
        let origin = ProtocolClass::from_class(class);
        let source = match self {
            Self::Plain => ProtocolInterfaceSource::Class(origin),
            Self::Top => ProtocolInterfaceSource::Materialized {
                origin,
                kind: MaterializationKind::Top,
            },
            Self::Bottom => ProtocolInterfaceSource::Materialized {
                origin,
                kind: MaterializationKind::Bottom,
            },
        };
        ProtocolInstanceType::from_interface_source_for_test(db, source)
    }
}

/// Creates a synthesized protocol with exactly the supplied stored members.
fn synthesized_protocol<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    members: impl IntoIterator<Item = (&'static str, LegacyProtocolTestMember<'db>)>,
) -> ProtocolInstanceType<'db> {
    let interface = ProtocolInterface::for_legacy_test(
        db,
        env,
        members
            .into_iter()
            .map(|(name, member)| (Name::new_static(name), member)),
    );
    ProtocolInstanceType::from_interface_source_for_test(
        db,
        ProtocolInterfaceSource::Synthesized(interface),
    )
}

// Class-backed protocols contribute only their specialization arguments. Their declaration
// variables and member annotations stay unvisited, including under either materialization.
// The `Protocol[...]` declaration form also contributes nothing.
#[test_case::test_case(ProtocolEntry::Instance, ProtocolClassRepresentation::Plain; "instance class")]
#[test_case::test_case(ProtocolEntry::Subclass, ProtocolClassRepresentation::Plain; "subclass class")]
#[test_case::test_case(ProtocolEntry::Instance, ProtocolClassRepresentation::Top; "instance top")]
#[test_case::test_case(ProtocolEntry::Subclass, ProtocolClassRepresentation::Top; "subclass top")]
#[test_case::test_case(ProtocolEntry::Instance, ProtocolClassRepresentation::Bottom; "instance bottom")]
#[test_case::test_case(ProtocolEntry::Subclass, ProtocolClassRepresentation::Bottom; "subclass bottom")]
fn class_protocols_collect_specializations_without_reading_members(
    entry: ProtocolEntry,
    representation: ProtocolClassRepresentation,
) -> anyhow::Result<()> {
    let db = database(
        "from typing import Protocol, TypeVar\nHidden = TypeVar('Hidden')\nclass Plain(Protocol):\n    hidden: Hidden\nclass Pair[A, B](Protocol):\n    hidden: Hidden\n",
    )?;
    let env = db.program_environment();
    let context = BindingContext::Synthetic(env.program(&db));
    let first = variable(&db, "First", TypeVarKind::LegacyTypeVar, context);
    let second = variable(&db, "Second", TypeVarKind::LegacyTypeVar, context);
    let declared = GenericContext::from_typevar_instances(
        &db,
        &env,
        [
            variable(&db, "DeclaredA", TypeVarKind::LegacyTypeVar, context),
            variable(&db, "DeclaredB", TypeVarKind::LegacyTypeVar, context),
        ],
    );
    let alias = GenericAlias::new(
        &db,
        class(&db, "Pair")?,
        declared.specialize(&db, vec![Type::TypeVar(second), Type::TypeVar(first)]),
    );
    let root = tuple(
        &db,
        [
            entry.wrap(representation.wrap(
                &db,
                ClassType::NonGeneric(ClassLiteral::from(class(&db, "Plain")?)),
            )),
            entry.wrap(representation.wrap(&db, ClassType::Generic(alias))),
            Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(declared)),
        ],
    );
    executions(&db);
    assert_original(&db, root, None, &[second, first])?;
    assert!(executions(&db).is_empty());
    Ok(())
}

// Synthesized protocols visit names in sorted order and each property's read, write domain,
// and descriptor in that order. Getter/setter callables remain raw, so their annotations and
// eager defaults contribute alongside their returns. All members share the output set and filter.
#[test_case::test_case(ProtocolEntry::Instance; "protocol instance")]
#[test_case::test_case(ProtocolEntry::Subclass; "protocol subclass")]
fn synthesized_protocol_members_preserve_raw_type_order_and_binding_filter(
    entry: ProtocolEntry,
) -> anyhow::Result<()> {
    let db = database("class Owner: ...\nclass Other: ...\n")?;
    let env = db.program_environment();
    let owner = class(&db, "Owner")?.definition(&db);
    let other = class(&db, "Other")?.definition(&db);
    let variables: Vec<_> = [
        "Read",
        "Write",
        "Descriptor",
        "DescriptorOnly",
        "WriteOnly",
        "ReadOnly",
        "Attribute",
        "GetterAnnotation",
        "GetterDefault",
        "GetterReturn",
        "SetterAnnotation",
        "SetterReturn",
        "MethodAnnotation",
        "MethodReturn",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, name)| {
        let context = if index == 1 || index == 8 {
            other
        } else {
            owner
        };
        variable(
            &db,
            name,
            TypeVarKind::LegacyTypeVar,
            BindingContext::Definition(context),
        )
    })
    .collect();
    let getter = Type::single_callable(
        &db,
        Signature::new(
            Parameters::standard([Parameter::positional_only(None)
                .with_annotated_type(Type::TypeVar(variables[7]))
                .with_default_type(Type::TypeVar(variables[8]))]),
            Type::TypeVar(variables[9]),
        ),
    );
    let setter = Type::single_callable(
        &db,
        Signature::new(
            Parameters::standard([
                Parameter::positional_only(None).with_annotated_type(Type::TypeVar(variables[10]))
            ]),
            Type::TypeVar(variables[11]),
        ),
    );
    let method = Type::single_callable(
        &db,
        Signature::new(
            Parameters::standard([
                Parameter::positional_only(None).with_annotated_type(Type::TypeVar(variables[12]))
            ]),
            Type::TypeVar(variables[13]),
        ),
    );
    let root = entry.wrap(synthesized_protocol(
        &db,
        &env,
        [
            ("z_method", LegacyProtocolTestMember::Method(method)),
            (
                "g_accessors",
                LegacyProtocolTestMember::PropertyAccessors {
                    getter: Some(getter),
                    setter: Some(setter),
                },
            ),
            (
                "f_attribute",
                LegacyProtocolTestMember::Attribute(Type::TypeVar(variables[6])),
            ),
            (
                "a_property",
                LegacyProtocolTestMember::Property {
                    read: Some(Type::TypeVar(variables[0])),
                    write: Some(Type::TypeVar(variables[1])),
                    descriptor: Some(Type::TypeVar(variables[2])),
                },
            ),
            (
                "b_descriptor_only",
                LegacyProtocolTestMember::Property {
                    read: None,
                    write: None,
                    descriptor: Some(Type::TypeVar(variables[3])),
                },
            ),
            (
                "c_write_only",
                LegacyProtocolTestMember::Property {
                    read: None,
                    write: Some(Type::TypeVar(variables[4])),
                    descriptor: None,
                },
            ),
            (
                "d_read_only",
                LegacyProtocolTestMember::Property {
                    read: Some(Type::TypeVar(variables[5])),
                    write: None,
                    descriptor: None,
                },
            ),
            (
                "e_empty",
                LegacyProtocolTestMember::Property {
                    read: None,
                    write: None,
                    descriptor: None,
                },
            ),
            (
                "h_repeated",
                LegacyProtocolTestMember::Attribute(Type::TypeVar(variables[0])),
            ),
        ],
    ));
    assert_original(&db, root, None, &variables)?;
    let owner_variables: Vec<_> = variables
        .iter()
        .copied()
        .filter(|variable| variable.binding_context(&db) == BindingContext::Definition(owner))
        .collect();
    assert_original(&db, root, Some(owner), &owner_variables)?;
    assert_original(&db, root, Some(other), &[variables[1], variables[8]])?;
    let mut actual = FxOrderSet::default();
    let effects = RecordingEffects::new();
    assert_eq!(
        find_legacy_typevars_with(&db, &env, root, None, &mut actual, &effects),
        Ok(())
    );
    assert_eq!(actual.into_iter().collect::<Vec<_>>(), variables);
    assert!(!effects.events.borrow().contains(&Event::Deferred));
    Ok(())
}

/// Records active and cached guard counts before and after ordinary deferred traversal.
/// Each count pair is `(active, cached)`, matching the cycle detector's storage observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GuardObservation<'db> {
    key: Type<'db>,
    before: (usize, usize),
    after: (usize, usize),
}

/// Observes the supplied visitor while retaining ordinary guarded traversal and normalization.
#[derive(Debug, Default)]
struct VisitorRecordingEffects<'db> {
    guards: RefCell<Vec<GuardObservation<'db>>>,
}

impl<'db> LegacyTypeVarEffects<'db> for VisitorRecordingEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self, _work: LegacyTypeVarWork) -> Result<(), Infallible> {
        Ok(())
    }

    fn normalize_paramspec(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Infallible> {
        InlineLegacyTypeVarEffects.normalize_paramspec(db, variable)
    }

    fn deferred(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        context: Option<Definition<'db>>,
        dependency: LegacyTypeVarDependency<'db>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) -> Result<(), Infallible> {
        let LegacyTypeVarDependency::Guarded { key, .. } = dependency;
        let before = visitor.ownership_probe_counts();
        InlineLegacyTypeVarEffects.deferred(db, env, context, dependency, variables, visitor)?;
        self.guards.borrow_mut().push(GuardObservation {
            key,
            before,
            after: visitor.ownership_probe_counts(),
        });
        Ok(())
    }
}

// Class-backed specializations retain the caller's completed guards. Visiting the same function
// through either argument therefore skips its signature when that caller has already visited it.
#[test_case::test_case(ProtocolEntry::Instance, ProtocolClassRepresentation::Plain; "instance class")]
#[test_case::test_case(ProtocolEntry::Subclass, ProtocolClassRepresentation::Plain; "subclass class")]
#[test_case::test_case(ProtocolEntry::Instance, ProtocolClassRepresentation::Top; "instance top")]
#[test_case::test_case(ProtocolEntry::Subclass, ProtocolClassRepresentation::Top; "subclass top")]
#[test_case::test_case(ProtocolEntry::Instance, ProtocolClassRepresentation::Bottom; "instance bottom")]
#[test_case::test_case(ProtocolEntry::Subclass, ProtocolClassRepresentation::Bottom; "subclass bottom")]
fn class_protocol_specializations_reuse_the_supplied_visitor(
    entry: ProtocolEntry,
    representation: ProtocolClassRepresentation,
) -> anyhow::Result<()> {
    let db = database(
        "from typing import Protocol, TypeVar\nT = TypeVar('T')\ndef f(value: T) -> T: ...\nclass Pair[A, B](Protocol): ...\n",
    )?;
    let env = db.program_environment();
    let function = symbol(&db, "f")?;
    let origin = class(&db, "Pair")?;
    let context = origin
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("missing generic context"))?;
    let alias = GenericAlias::new(
        &db,
        origin,
        context.specialize(&db, vec![function, function]),
    );
    let root = entry.wrap(representation.wrap(&db, ClassType::Generic(alias)));
    let original_visitor = FindLegacyTypeVarsVisitor::default();
    let shared_visitor = FindLegacyTypeVarsVisitor::default();
    original_visitor.visit(&db, function, || ());
    shared_visitor.visit(&db, function, || ());
    let mut expected = FxOrderSet::default();
    original(&db, &env, root, None, &mut expected, &original_visitor)?;
    let effects = VisitorRecordingEffects::default();
    let mut actual = FxOrderSet::default();
    infallible(collect_with_visitor(
        &db,
        &env,
        root,
        None,
        &mut actual,
        &shared_visitor,
        &effects,
    ));
    assert!(expected.is_empty());
    assert_eq!(actual, expected);
    assert_eq!(
        effects.guards.into_inner(),
        [GuardObservation {
            key: function,
            before: (0, 1),
            after: (0, 1)
        }; 2],
    );
    assert_eq!(shared_visitor.ownership_probe_counts(), (0, 1));
    Ok(())
}

// Each synthesized raw type has an empty visitor, even when siblings contain the same guarded
// function. The nested protocol inside the callable's receiver bound uses fresh visitors, then
// traversal resumes with the enclosing callable's visitor. Its completed guard for `f` is reused
// by the rest of the bound, the annotation, and the return. Returning from the outer protocol
// restores the caller's already populated visitor.
#[test_case::test_case(ProtocolEntry::Instance; "protocol instance")]
#[test_case::test_case(ProtocolEntry::Subclass; "protocol subclass")]
fn synthesized_protocol_visitors_are_fresh_and_restore_nested_parents(
    entry: ProtocolEntry,
) -> anyhow::Result<()> {
    let db = database(
        "from typing import TypeVar\nT = TypeVar('T')\ndef f(value: T) -> T: ...\ndef seed() -> None: ...\n",
    )?;
    let env = db.program_environment();
    let function = symbol(&db, "f")?;
    let seed = symbol(&db, "seed")?;
    let receiver = variable(
        &db,
        "Receiver",
        TypeVarKind::LegacyTypeVar,
        BindingContext::Synthetic(env.program(&db)),
    );
    let nested = entry.wrap(synthesized_protocol(
        &db,
        &env,
        [(
            "member",
            LegacyProtocolTestMember::Property {
                read: Some(function),
                write: Some(function),
                descriptor: Some(function),
            },
        )],
    ));
    let constraints = ConstraintSetBuilder::new().into_owned(|builder| {
        ConstraintSet::constrain_typevar_lower_bound(
            &db,
            &env,
            builder,
            receiver,
            tuple(&db, [function, nested, function]),
        )
    });
    let callable = Type::single_callable(
        &db,
        Signature::new(
            Parameters::standard([Parameter::positional_only(None).with_annotated_type(function)]),
            function,
        )
        .with_probe_receiver_constraints(constraints),
    );
    let outer = entry.wrap(synthesized_protocol(
        &db,
        &env,
        [
            ("a_callable", LegacyProtocolTestMember::Attribute(callable)),
            ("b_sibling", LegacyProtocolTestMember::Attribute(function)),
        ],
    ));
    let root = tuple(&db, [function, outer, function]);
    let root_visitor = FindLegacyTypeVarsVisitor::default();
    let original_visitor = FindLegacyTypeVarsVisitor::default();
    root_visitor.visit(&db, seed, || ());
    original_visitor.visit(&db, seed, || ());
    let mut expected = FxOrderSet::default();
    original(&db, &env, root, None, &mut expected, &original_visitor)?;
    let mut actual = FxOrderSet::default();
    let effects = VisitorRecordingEffects::default();
    infallible(collect_with_visitor(
        &db,
        &env,
        root,
        None,
        &mut actual,
        &root_visitor,
        &effects,
    ));
    assert_eq!(actual, expected);
    assert_eq!(
        actual
            .iter()
            .map(|variable| variable.name(&db).as_str())
            .collect::<Vec<_>>(),
        ["T", "Receiver"],
    );
    let observations = effects.guards.into_inner();
    assert_eq!(
        observations
            .iter()
            .map(|observation| observation.key)
            .collect::<Vec<_>>(),
        [function; 10]
    );
    assert_eq!(
        observations
            .iter()
            .map(|observation| observation.before)
            .collect::<Vec<_>>(),
        [
            (0, 1), // Root before the outer protocol; only `seed` is cached.
            (0, 0), // First `f` in the callable's receiver bound.
            (0, 0), // Nested property's read type.
            (0, 0), // Nested property's write domain.
            (0, 0), // Nested property's descriptor type.
            (0, 1), // Rest of the receiver bound, after the nested protocol.
            (0, 1), // Callable annotation.
            (0, 1), // Callable return.
            (0, 0), // Outer protocol's sibling member.
            (0, 2), // Root after the outer protocol; `seed` and `f` remain cached.
        ],
    );
    // These are the same encounters after visiting `f`; only a visitor's first visit adds a guard.
    //
    assert_eq!(
        observations
            .iter()
            .map(|observation| observation.after)
            .collect::<Vec<_>>(),
        [
            (0, 2),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 2)
        ],
    );
    assert_eq!(root_visitor.ownership_probe_counts(), (0, 2));
    assert_eq!(
        root_visitor.ownership_probe_counts(),
        original_visitor.ownership_probe_counts()
    );
    Ok(())
}

// A guarded descendant can still refuse inside a synthesized member after earlier raw types
// have contributed variables. That refusal leaves later members unvisited; a fresh retry succeeds.
#[test_case::test_case(ProtocolEntry::Instance; "protocol instance")]
#[test_case::test_case(ProtocolEntry::Subclass; "protocol subclass")]
fn synthesized_protocol_descendant_refusal_preserves_the_collected_prefix(
    entry: ProtocolEntry,
) -> anyhow::Result<()> {
    let db = database("def f() -> int: ...\n")?;
    let env = db.program_environment();
    let context = BindingContext::Synthetic(env.program(&db));
    let first = variable(&db, "First", TypeVarKind::LegacyTypeVar, context);
    let later = variable(&db, "Later", TypeVarKind::LegacyTypeVar, context);
    let root = entry.wrap(synthesized_protocol(
        &db,
        &env,
        [
            (
                "a_property",
                LegacyProtocolTestMember::Property {
                    read: Some(Type::TypeVar(first)),
                    write: Some(symbol(&db, "f")?),
                    descriptor: Some(Type::TypeVar(later)),
                },
            ),
            (
                "b_attribute",
                LegacyProtocolTestMember::Attribute(Type::TypeVar(later)),
            ),
        ],
    ));
    let effects = RecordingEffects::new();
    let mut actual = FxOrderSet::default();
    assert_eq!(
        find_legacy_typevars_with(&db, &env, root, None, &mut actual, &effects),
        Err(())
    );
    assert_eq!(actual.into_iter().collect::<Vec<_>>(), [first]);
    assert_eq!(effects.events.borrow().last(), Some(&Event::Deferred));
    assert_original(&db, root, None, &[first, later])?;
    Ok(())
}

/// Selects the stored callable representation traversed by a test.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CallableRepresentation {
    Type,
    KnownInstance,
}

impl CallableRepresentation {
    const fn wrap(self, callable: CallableType<'_>) -> Type<'_> {
        match self {
            Self::Type => Type::Callable(callable),
            Self::KnownInstance => Type::KnownInstance(KnownInstanceType::Callable(callable)),
        }
    }
}

// Both callable representations visit annotations, eager defaults, and returns in order,
// while variables mentioned only in the generic declaration remain excluded.
#[test_case::test_case(CallableRepresentation::Type; "callable type")]
#[test_case::test_case(CallableRepresentation::KnownInstance; "known callable instance")]
fn callable_defaults_and_declarations_preserve_the_original_order(
    representation: CallableRepresentation,
) -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let context = BindingContext::Synthetic(env.program(&db));
    let vars: Vec<_> = ["Annotation", "Default", "Return", "Declaration"]
        .into_iter()
        .map(|name| variable(&db, name, TypeVarKind::LegacyTypeVar, context))
        .collect();
    let root = representation.wrap(CallableType::single(
        &db,
        Signature::new_generic(
            Some(GenericContext::from_typevar_instances(&db, &env, [vars[3]])),
            Parameters::standard([Parameter::positional_only(None)
                .with_annotated_type(Type::TypeVar(vars[0]))
                .with_default_type(Type::TypeVar(vars[1]))]),
            Type::TypeVar(vars[2]),
        ),
    ));
    assert_original(&db, root, None, &vars[..3])?;
    let mut variables = FxOrderSet::default();
    let effects = RecordingEffects::new();
    assert_eq!(
        find_legacy_typevars_with(&db, &env, root, None, &mut variables, &effects),
        Ok(())
    );
    assert_eq!(variables.into_iter().collect::<Vec<_>>(), vars[..3]);
    assert!(!effects.events.borrow().contains(&Event::Deferred));
    assert_eq!(
        effects.events.borrow().last(),
        Some(&Event::Work(LegacyTypeVarWork::Publish))
    );
    Ok(())
}

// Refusing the deferred dependency for a function inside a callable annotation preserves
// only the collected prefix. Refusing the next checkpoint after that dependency completes
// also prevents the eager-default and return-type visits.
#[test_case::test_case(CallableRepresentation::Type; "callable type")]
#[test_case::test_case(CallableRepresentation::KnownInstance; "known callable instance")]
fn callable_descendant_refusal_stops_before_eager_default_and_return(
    representation: CallableRepresentation,
) -> anyhow::Result<()> {
    let db = database("def f() -> int: ...\n")?;
    let env = db.program_environment();
    let context = BindingContext::Synthetic(env.program(&db));
    let vars: Vec<_> = ["Annotation", "Default", "Return"]
        .into_iter()
        .map(|name| variable(&db, name, TypeVarKind::LegacyTypeVar, context))
        .collect();
    let root = representation.wrap(CallableType::single(
        &db,
        Signature::new(
            Parameters::standard([Parameter::positional_only(None)
                .with_annotated_type(tuple(&db, [Type::TypeVar(vars[0]), symbol(&db, "f")?]))
                .with_default_type(Type::TypeVar(vars[1]))]),
            Type::TypeVar(vars[2]),
        ),
    ));
    assert_original(&db, root, None, &vars)?;
    let mut variables = FxOrderSet::default();
    let effects = RecordingEffects::new();
    assert_eq!(
        find_legacy_typevars_with(&db, &env, root, None, &mut variables, &effects),
        Err(())
    );
    assert_eq!(variables.iter().copied().collect::<Vec<_>>(), vars[..1]);
    assert_eq!(effects.events.borrow().last(), Some(&Event::Deferred));
    let mut effects = RecordingEffects::new();
    effects.refuse_dependencies = false;
    effects.stop_after_dependency = true;
    let mut variables = FxOrderSet::default();
    assert_eq!(
        find_legacy_typevars_with(&db, &env, root, None, &mut variables, &effects),
        Err(())
    );
    assert_eq!(variables.into_iter().collect::<Vec<_>>(), vars[..1]);
    assert_eq!(
        effects.events.borrow().last(),
        Some(&Event::Work(LegacyTypeVarWork::Resume))
    );
    Ok(())
}

#[test]
fn guarded_wrappers_share_the_supplied_visitor_and_fresh_entries_do_not() -> anyhow::Result<()> {
    let db = database("from typing import TypeVar\nT = TypeVar('T')\ndef f(value: T) -> T: ...\n")?;
    let env = db.program_environment();
    let function = symbol(&db, "f")?;
    let receiver = variable(
        &db,
        "Receiver",
        TypeVarKind::LegacyTypeVar,
        BindingContext::Synthetic(env.program(&db)),
    );
    let method =
        BoundMethodType::from_callable(&db, function, env.program(&db), Type::TypeVar(receiver));
    let wrapper = Type::KnownBoundMethod(KnownBoundMethodType::MethodTypeDunderGet(method));
    let shared_visitor = FindLegacyTypeVarsVisitor::default();
    let original_visitor = FindLegacyTypeVarsVisitor::default();
    let mut first = Vec::new();
    for (index, root) in [
        wrapper,
        wrapper,
        Type::BoundMethod(method),
        Type::TypeVar(receiver),
    ]
    .into_iter()
    .enumerate()
    {
        let mut expected = FxOrderSet::default();
        original(&db, &env, root, None, &mut expected, &original_visitor)?;
        let mut actual = FxOrderSet::default();
        infallible(collect_with_visitor(
            &db,
            &env,
            root,
            None,
            &mut actual,
            &shared_visitor,
            &InlineLegacyTypeVarEffects,
        ));
        assert_eq!(actual, expected);
        match index {
            0 => {
                first = actual.into_iter().collect();
                assert_eq!(first.first(), Some(&receiver));
                assert_eq!(first.len(), 2);
                assert_eq!(first[1].name(&db).as_str(), "T");
            }
            1 | 2 => assert!(actual.is_empty()),
            _ => assert_eq!(actual.into_iter().collect::<Vec<_>>(), [receiver]),
        }
    }
    for _ in 0..2 {
        assert_eq!(collected(&db, wrapper, None), first);
    }
    Ok(())
}

fn cold_source_reads(shared: bool) -> anyhow::Result<Vec<String>> {
    let db = database(
        "from typing import TypeVar\nT = TypeVar('T')\nU = TypeVar('U')\ndef f(first: T, second: U) -> T: ...\n",
    )?;
    let env = db.program_environment();
    let root = symbol(&db, "f")?;
    executions(&db);
    let mut variables = FxOrderSet::default();
    if shared {
        infallible(find_legacy_typevars_with(
            &db,
            &env,
            root,
            None,
            &mut variables,
            &InlineLegacyTypeVarEffects,
        ));
    } else {
        original(
            &db,
            &env,
            root,
            None,
            &mut variables,
            &FindLegacyTypeVarsVisitor::default(),
        )?;
    }
    let reads = executions(&db);
    assert_eq!(
        variables
            .iter()
            .map(|variable| variable.name(&db).as_str())
            .collect::<Vec<_>>(),
        ["T", "U"]
    );
    Ok(reads)
}

#[test]
fn ordinary_deferred_function_reads_keep_independent_cold_order() -> anyhow::Result<()> {
    let expected = cold_source_reads(false)?;
    let actual = cold_source_reads(true)?;
    assert!(!expected.is_empty());
    assert!(
        expected.iter().any(|name| name.contains("signature")),
        "{expected:?}"
    );
    assert_eq!(actual, expected);
    Ok(())
}

// Deep and wide stored types collect on a small native stack, and pending traversal state is
// dropped on that same stack after success or refusal. Nested callable annotations leave outer
// return visits pending; refusals stop the trace and fresh retries complete.
#[test]
fn deep_and_wide_stored_shapes_collect_and_drop_on_a_small_stack() -> anyhow::Result<()> {
    std::thread::Builder::new().stack_size(256 * 1024).spawn(|| -> anyhow::Result<()> {
        let db = setup_db();
        let env = db.program_environment();
        let context = BindingContext::Synthetic(env.program(&db));
        let leaf = variable(&db, "Leaf", TypeVarKind::LegacyTypeVar, context);
        let mut deep = Type::TypeVar(leaf);
        for _ in 0..2048 {
            deep = Type::TypeForm(TypeFormType::new(&db, deep));
        }
        let mut deep_callable = Type::TypeVar(leaf);
        for index in 0..2048 {
            let callable = CallableType::single(
                &db,
                Signature::new(
                    Parameters::standard([Parameter::positional_only(None)
                        .with_annotated_type(deep_callable)]),
                    Type::TypeVar(leaf),
                ),
            );
            let representation = if index % 2 == 0 {
                CallableRepresentation::Type
            } else {
                CallableRepresentation::KnownInstance
            };
            deep_callable = representation.wrap(callable);
        }
        let wide = tuple(&db, (0..512).map(|index| Type::TypeVar(variable(&db, &format!("T{index}"), TypeVarKind::LegacyTypeVar, context))));
        let mut pending_growth = Type::TypeVar(leaf);
        for _ in 0..128 {
            pending_growth = tuple(&db, [pending_growth, Type::TypeVar(leaf)]);
        }
        for root in [deep, deep_callable, wide, pending_growth] {
            let effects = RecordingEffects::new();
            let mut expected = FxOrderSet::default();
            assert_eq!(find_legacy_typevars_with(&db, &env, root, None, &mut expected, &effects), Ok(()));
            assert_eq!(expected.len(), if root == wide { 512 } else { 1 });
            let trace = effects.events.into_inner();
            let mut positions = vec![trace.len() / 2, trace.len() - 1];
            for (index, event) in trace.iter().enumerate() {
                if matches!(event, Event::Work(LegacyTypeVarWork::Pending { len, capacity } | LegacyTypeVarWork::Insert { len, capacity }) if *len > 0 && len == capacity) {
                    positions.push(index);
                }
            }
            for index in positions {
                let mut refused = RecordingEffects::new();
                refused.refuse_at = Some(index);
                let mut partial = FxOrderSet::default();
                assert_eq!(find_legacy_typevars_with(&db, &env, root, None, &mut partial, &refused), Err(()));
                assert_eq!(*refused.events.borrow(), trace[..=index]);
            }
            for _ in 0..2 {
                let mut retry = FxOrderSet::default();
                assert_eq!(find_legacy_typevars_with(&db, &env, root, None, &mut retry, &RecordingEffects::new()), Ok(()));
                assert_eq!(retry, expected);
            }
        }
        Ok(())
    })?.join().map_err(|_| anyhow::anyhow!("collector worker panicked"))??;
    Ok(())
}
