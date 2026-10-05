use std::fmt::Write;
use std::sync::{Arc, Mutex};

use ruff_db::diagnostic::{Diagnostic, UnifiedFile};
use ruff_db::files::{FilePath, system_path_to_file};
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_source_file::SourceFileBuilder;
use salsa::{Database, EventKind};
use ty_python_core::semantic_index;

use crate::db::tests::{TestDb, TestDbBuilder};
use crate::types::infer::builder::{
    ClassType, Db, Definition, DynamicType, ExpressionInference, ExpressionInferenceExtra,
    ExpressionKind, ExpressionNodeKey, LiteralValueTypeKind, NodeWithScopeKind, ProgramEnvironment,
    PythonVersion, Ranged, Span, Type, TypeContext, ast, infer_expression_types,
};

const OPS: &str = "from typing import overload\n\n@overload\ndef choose(value: int, /) -> int: ...\n@overload\ndef choose(value: None, /) -> None: ...\n";

// Each record is JSON, with field order retained for a byte-for-byte comparison between the
// independent ordinary baseline and the extraction. Database keys never enter these records.
fn quoted(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            c if c < ' ' => {
                let _ = write!(output, "\\u{:04x}", u32::from(c));
            }
            c => output.push(c),
        }
    }
    output.push('"');
    output
}

fn object(fields: impl IntoIterator<Item = (&'static str, String)>) -> String {
    let fields: Vec<_> = fields
        .into_iter()
        .map(|(key, value)| format!("{}:{value}", quoted(key)))
        .collect();
    format!("{{{}}}", fields.join(","))
}

fn array(values: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", values.into_iter().collect::<Vec<_>>().join(","))
}

fn file_identity(db: &TestDb, file: ruff_db::files::File) -> String {
    let path = file.path(db);
    let kind = match path {
        FilePath::System(_) => "system",
        FilePath::SystemVirtual(_) => "virtual",
        FilePath::Vendored(_) => "vendored",
    };
    format!("{kind}:{}", path.as_str())
}

fn definition_identity<'db>(db: &'db TestDb, definition: Definition<'db>) -> String {
    let module = parsed_module(db, definition.python_file(db)).load(db);
    object([
        ("file", quoted(&file_identity(db, definition.file(db)))),
        (
            "full_range",
            quoted(&format!("{:?}", definition.full_range(db, &module).range())),
        ),
        (
            "focus_range",
            quoted(&format!(
                "{:?}",
                definition.focus_range(db, &module).range()
            )),
        ),
        (
            "name",
            definition
                .name(db)
                .as_deref()
                .map_or_else(|| "null".into(), quoted),
        ),
    ])
}

fn normalized_type<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> anyhow::Result<String> {
    let (kind, detail) = match ty {
        Type::FunctionLiteral(function) => (
            "FunctionLiteral",
            object([
                (
                    "definition",
                    definition_identity(db, function.definition(db)),
                ),
                (
                    "last_definition",
                    definition_identity(db, function.last_definition(db)),
                ),
            ]),
        ),
        Type::NominalInstance(instance) => {
            let ClassType::NonGeneric(class) = instance.class(db, env) else {
                anyhow::bail!("generic instance requires a richer baseline normalizer");
            };
            let definition = class
                .definition(db)
                .ok_or_else(|| anyhow::anyhow!("instance without source definition"))?;
            (
                "NominalInstance",
                object([
                    ("definition", definition_identity(db, definition)),
                    (
                        "inherits_explicit_any",
                        instance.inherits_from_explicit_any().to_string(),
                    ),
                ]),
            )
        }
        Type::LiteralValue(literal) if matches!(literal.kind(), LiteralValueTypeKind::Int(_)) => (
            "LiteralValueInt",
            // Integer literal Debug contains its value and all flags, with no database identity.
            quoted(&format!("{literal:?}")),
        ),
        Type::Union(union) => (
            "Union",
            object([
                (
                    "elements",
                    array(
                        union
                            .elements(db)
                            .iter()
                            .map(|element| normalized_type(db, env, *element))
                            .collect::<anyhow::Result<Vec<_>>>()?,
                    ),
                ),
                (
                    "recursively_defined",
                    quoted(&format!("{:?}", union.recursively_defined(db))),
                ),
            ]),
        ),
        Type::Dynamic(
            dynamic @ (DynamicType::Any
            | DynamicType::Unknown
            | DynamicType::UnspecializedTypeVar
            | DynamicType::UnknownLambdaParameter
            | DynamicType::InvalidConcatenateUnknown
            | DynamicType::AmbiguousOverload),
        ) => ("Dynamic", quoted(&format!("{dynamic:?}"))),
        Type::Never => ("Never", "null".into()),
        _ => anyhow::bail!(
            "unexpected Type variant in fixed ordinary fixture: {}",
            ty.display(db, env).preserve_long_unions()
        ),
    };
    Ok(object([
        ("kind", quoted(kind)),
        (
            "display",
            quoted(&ty.display(db, env).preserve_long_unions().to_string()),
        ),
        ("detail", detail),
    ]))
}

fn normalize_annotation(db: &TestDb, annotation: &mut ruff_db::diagnostic::Annotation) {
    let span = annotation.get_span();
    if let UnifiedFile::Ty(file) = span.file() {
        let source =
            SourceFileBuilder::new(file_identity(db, *file), source_text(db, *file).as_str())
                .finish();
        annotation.set_span(Span::from(source).with_optional_range(span.range()));
    }
}

fn normalized_diagnostic(db: &TestDb, diagnostic: &Diagnostic) -> Diagnostic {
    let mut diagnostic = diagnostic.clone();
    for annotation in diagnostic.annotations_mut() {
        normalize_annotation(db, annotation);
    }
    for subdiagnostic in diagnostic.sub_diagnostics_mut() {
        for annotation in subdiagnostic.annotations_mut() {
            normalize_annotation(db, annotation);
        }
    }
    diagnostic
}

fn normalized_payload<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    inference: &ExpressionInference<'db>,
    nodes: &[(ExpressionNodeKey, String)],
) -> anyhow::Result<String> {
    let ExpressionInference {
        expressions,
        extra,
        #[cfg(debug_assertions)]
        scope,
    } = inference;
    let node = |key| {
        nodes
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, identity)| identity.clone())
            .ok_or_else(|| anyhow::anyhow!("payload includes an unregistered source node"))
    };
    let typed_nodes = |values: &crate::types::infer::FrozenMap<ExpressionNodeKey, Type<'db>>| {
        values
            .iter()
            .map(|(key, ty)| {
                Ok(object([
                    ("node", node(*key)?),
                    ("type", normalized_type(db, env, *ty)?),
                ]))
            })
            .collect::<anyhow::Result<Vec<_>>>()
            .map(array)
    };
    let expressions = typed_nodes(expressions)?;
    let extra = if let Some(extra) = extra {
        let ExpressionInferenceExtra {
            implicit_aliases,
            string_annotations,
            expected_types,
            type_expression_flags,
            comparison_truthiness,
            collection_use_constraints,
            bindings,
            diagnostics,
            called_functions,
            cycle_recovery,
        } = extra.as_ref();
        anyhow::ensure!(implicit_aliases.is_empty(), "unexpected implicit aliases");
        anyhow::ensure!(
            string_annotations.iter().next().is_none(),
            "unexpected string annotations"
        );
        anyhow::ensure!(
            type_expression_flags.iter().next().is_none(),
            "unexpected type-expression flags"
        );
        anyhow::ensure!(
            comparison_truthiness.iter().next().is_none(),
            "unexpected comparison truthiness"
        );
        anyhow::ensure!(
            collection_use_constraints.is_empty(),
            "unexpected collection constraints"
        );
        anyhow::ensure!(bindings.is_empty(), "unexpected bindings");
        anyhow::ensure!(
            called_functions.is_empty(),
            "unexpected called-function metadata"
        );
        anyhow::ensure!(cycle_recovery.is_none(), "unexpected cycle recovery");
        anyhow::ensure!(diagnostics.used_len() == 0, "unexpected used suppressions");
        let diagnostics = diagnostics.into_iter().map(|diagnostic| {
            // Derived Debug retains every diagnostic field after its spans have been converted
            // to source identities, including fields without public getters.
            quoted(&format!("{:?}", normalized_diagnostic(db, diagnostic)))
        });
        object([
            ("implicit_aliases", "[]".into()),
            ("string_annotations", "[]".into()),
            ("expected_types", typed_nodes(expected_types)?),
            ("type_expression_flags", "[]".into()),
            ("comparison_truthiness", "[]".into()),
            ("collection_use_constraints", "[]".into()),
            ("bindings", "[]".into()),
            ("diagnostics", array(diagnostics)),
            ("used_suppressions", "[]".into()),
            ("called_functions", "[]".into()),
            ("cycle_recovery", "null".into()),
        ])
    } else {
        "null".into()
    };
    #[cfg(debug_assertions)]
    let scope = {
        anyhow::ensure!(
            matches!(scope.node(db), NodeWithScopeKind::Module),
            "unexpected scope"
        );
        object([
            ("file", quoted(&file_identity(db, scope.file(db)))),
            ("kind", quoted("Module")),
        ])
    };
    #[cfg(not(debug_assertions))]
    let scope = "null".into();
    Ok(object([
        ("expressions", expressions),
        ("extra", extra),
        ("debug_scope", scope),
    ]))
}

fn ordinary_fixture(literal: &str, expect_diagnostics: bool) -> anyhow::Result<()> {
    let source = format!("from ops import choose\n\nchoose(choose({literal}))\n");
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = events.clone();
    let db = TestDbBuilder::new()
        .with_salsa_event_callback(move |event| {
            if let EventKind::WillExecute { database_key } = event {
                observed.lock().unwrap().push(*database_key);
            }
        })
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/ops.pyi", OPS)
        .with_file("/src/main.py", &source)
        .build()?;
    let file = system_path_to_file(&db, "/src/main.py")?;
    let program_file = db.program_file(file);
    let module = parsed_module(&db, program_file.python_file(&db)).load(&db);
    let index = semantic_index(&db, program_file);
    let [ast::Stmt::ImportFrom(_), ast::Stmt::Expr(statement)] = module.suite().as_slice() else {
        anyhow::bail!("unexpected fixture statements");
    };
    let ast::Expr::Call(outer) = statement.value.as_ref() else {
        anyhow::bail!("missing outer call")
    };
    let [ast::Expr::Call(inner)] = outer.arguments.args.as_ref() else {
        anyhow::bail!("missing inner call")
    };
    let [number @ ast::Expr::NumberLiteral(_)] = inner.arguments.args.as_ref() else {
        anyhow::bail!("missing numeric argument")
    };
    anyhow::ensure!(outer.arguments.keywords.is_empty() && inner.arguments.keywords.is_empty());
    let outer_expression = index
        .try_expression(outer)
        .ok_or_else(|| anyhow::anyhow!("outer call is not canonical"))?;
    let callee_expression = index
        .try_expression(&outer.func)
        .ok_or_else(|| anyhow::anyhow!("outer callee is not canonical"))?;
    anyhow::ensure!(outer_expression.kind(&db) == ExpressionKind::Normal);
    anyhow::ensure!(callee_expression.kind(&db) == ExpressionKind::Callee);
    anyhow::ensure!(std::ptr::eq(
        outer_expression.node_ref(&db).node(&module),
        statement.value.as_ref()
    ));
    anyhow::ensure!(std::ptr::eq(
        callee_expression.node_ref(&db).node(&module),
        outer.func.as_ref()
    ));
    anyhow::ensure!(index.try_expression(inner).is_none());
    anyhow::ensure!(index.try_expression(&inner.func).is_none());
    anyhow::ensure!(index.try_expression(number).is_none());
    let nodes = [
        (ExpressionNodeKey::from(outer), "outer_call", outer.range()),
        (
            ExpressionNodeKey::from(&outer.func),
            "outer_callee",
            outer.func.range(),
        ),
        (ExpressionNodeKey::from(inner), "inner_call", inner.range()),
        (
            ExpressionNodeKey::from(&inner.func),
            "inner_callee",
            inner.func.range(),
        ),
        (ExpressionNodeKey::from(number), "number", number.range()),
    ]
    .map(|(key, role, range)| {
        (
            key,
            object([
                ("file", quoted("system:/src/main.py")),
                ("role", quoted(role)),
                ("start", u32::from(range.start()).to_string()),
                ("end", u32::from(range.end()).to_string()),
            ]),
        )
    });
    let env = ProgramEnvironment::from_file(program_file);
    let before_request = events.lock().unwrap().len();
    let inference = infer_expression_types(&db, outer_expression, TypeContext::default());
    let after_request = events.lock().unwrap().len();
    let diagnostic_count = inference
        .extra
        .as_ref()
        .map_or(0, |extra| (&extra.diagnostics).into_iter().count());
    anyhow::ensure!((diagnostic_count > 0) == expect_diagnostics);
    anyhow::ensure!(
        inference
            .expressions
            .get(&ExpressionNodeKey::from(outer))
            .is_some()
    );
    anyhow::ensure!(
        inference
            .expressions
            .get(&ExpressionNodeKey::from(inner))
            .is_some()
    );
    if !expect_diagnostics {
        assert_eq!(
            inference
                .expression_type(outer)
                .display(&db, &env)
                .to_string(),
            "int"
        );
        assert_eq!(
            inference
                .expression_type(inner)
                .display(&db, &env)
                .to_string(),
            "int"
        );
    }
    let payload = normalized_payload(&db, &env, inference, &nodes)?;
    let observed = events.lock().unwrap();
    let names = |start, end| {
        array(
            observed[start..end]
                .iter()
                .map(|key: &salsa::DatabaseKeyIndex| {
                    quoted(&db.ingredient_debug_name(key.ingredient_index()))
                }),
        )
    };
    let record = object([
        ("fixture", quoted(literal)),
        ("source", quoted(&source)),
        (
            "canonical_local_shape",
            array(nodes.iter().map(|(_, identity)| identity.clone())),
        ),
        ("preparation_queries", names(0, before_request)),
        ("request_queries", names(before_request, after_request)),
        (
            "normalization_queries",
            names(after_request, observed.len()),
        ),
        ("payload", payload),
    ]);
    println!("LOCAL_CALL_BASELINE_JSON {record}");
    Ok(())
}

#[test]
fn ordinary_nested_call_preserves_complete_payload() -> anyhow::Result<()> {
    ordinary_fixture("1", false)
}

#[test]
fn ordinary_nested_call_error_preserves_complete_payload() -> anyhow::Result<()> {
    ordinary_fixture("1.0", true)
}

mod cleanup {
    use std::cell::{Cell, RefCell};
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::{Rc, Weak};
    use std::task::{Context, Poll};

    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::{ParsedModuleRef, parsed_module};
    use salsa::attempt_probe::{AttemptOutcome, Incomplete, try_with_attempt};
    use salsa::execution_probe::{
        ExecutionAdmission, ExecutionWork, RegistryBuilder, RunError, RunResult, TaskEndpoint,
    };
    use ty_python_core::{global_scope, semantic_index};

    use super::super::{
        self as local, ActiveArgument, ArgumentStep, BuilderId, BuilderStore, CalleeState,
        CompletedArgument, CompletedArgumentLease, ExpressionMode, FinishedOwner, Frame,
        LocalEffects, LocalFacts, LocalInvocation, LocalOwners, OrdinaryLocalEffects,
        OrdinaryOwnedArgumentEffects, OwnedAction, OwnedArgumentEffects, OwnedPending, OwnedState,
        PendingArgument, Preparation, PreparationStep, PreparedCall, RootCheckpoint, Splat,
        SynchronousLocalEffects, SynchronousOwnedArgumentEffects, TakenArgument, Work, arguments,
        call,
    };
    use crate::db::tests::{TestDb, TestDbBuilder};
    use crate::types::infer::builder::{
        CallArguments, Definition, ExpressionCache, ExpressionCacheEntry, InferenceFlags,
        InferenceRegion, KnownClass, SpecialFormType, Type, TypeContext, TypeInferenceBuilder, ast,
        source_expression,
    };
    use crate::types::signatures::effects::try_poll_immediate;
    use crate::{Db, ProgramEnvironment};

    const PATH: &str = "/src/local_cleanup.py";

    fn database() -> anyhow::Result<TestDb> {
        TestDbBuilder::new()
            .with_file(PATH, "def marker(): ...\nouter(inner(1))\n")
            .build()
    }

    fn builder<'db, 'ast>(
        db: &'db TestDb,
        module: &'ast ParsedModuleRef,
        env: &'ast ProgramEnvironment<'db>,
    ) -> anyhow::Result<(
        TypeInferenceBuilder<'db, 'ast>,
        &'ast ast::ExprCall,
        Definition<'db>,
    )> {
        let file = db.program_file(system_path_to_file(db, PATH)?);
        let index = semantic_index(db, file);
        let [ast::Stmt::FunctionDef(marker), ast::Stmt::Expr(statement)] =
            module.suite().as_slice()
        else {
            anyhow::bail!("unexpected cleanup fixture");
        };
        let ast::Expr::Call(call) = statement.value.as_ref() else {
            anyhow::bail!("missing cleanup call");
        };
        let binding = index
            .try_definition(marker)
            .ok_or_else(|| anyhow::anyhow!("missing marker definition"))?;
        anyhow::ensure!(index.try_expression(&call.func).is_some());
        let mut builder = TypeInferenceBuilder::new(
            db,
            env,
            InferenceRegion::Scope(global_scope(db, file), TypeContext::default()),
            file.file(db),
            file,
            index,
            module,
        );
        builder.context.defuse();
        builder.typevar_binding_context = Some(binding);
        builder
            .context
            .inference_flags
            .set(InferenceFlags::CHECK_UNBOUND_TYPEVARS, false);
        builder
            .context
            .inference_flags
            .set(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, false);
        Ok((builder, call, binding))
    }

    #[test]
    fn store_abort_restores_temporary_root_state_and_cache_ownership() -> anyhow::Result<()> {
        fn assert_copy<T: Copy>() {}
        assert_copy::<RootCheckpoint<'_>>();
        let db = database()?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let env = ProgramEnvironment::from_file(file);
        for preexisting in [false, true] {
            let (mut root, _, binding) = builder(&db, &module, &env)?;
            if preexisting {
                root.setup_expression_cache();
            }
            let flags = root.context.inference_flags;
            let original_cache = root.expression_cache.as_ref().map(Rc::downgrade);
            let installed_cache;
            {
                let mut store = BuilderStore::new(&mut root);
                let Ok(saved) = OrdinaryLocalEffects::default().enter_callee(&mut store, BuilderId::ROOT);
                assert_eq!(saved.binding, Some(binding));
                assert!(!saved.check_unbound);
                let Ok(previous) =
                    OrdinaryLocalEffects::default().enter_paramspec(&mut store, BuilderId::ROOT);
                assert!(!previous);
                assert_eq!(store.setup_expression_cache(BuilderId::ROOT), !preexisting);
                installed_cache = store
                    .builder(BuilderId::ROOT)
                    .expression_cache
                    .as_ref()
                    .map(Rc::downgrade);
                let outer = store.speculate(BuilderId::ROOT, false);
                let inner = store.speculate(outer, false);
                assert_eq!(store.speculative.len(), 2);
                let Some(cache) = store.builder(BuilderId::ROOT).expression_cache.as_ref() else {
                    anyhow::bail!("root cache was not installed");
                };
                assert_eq!(Rc::strong_count(cache), 3);
                assert!(
                    store
                        .builder(outer)
                        .expression_cache
                        .as_ref()
                        .is_some_and(|outer_cache| Rc::ptr_eq(cache, outer_cache))
                );
                assert!(
                    store
                        .builder(inner)
                        .expression_cache
                        .as_ref()
                        .is_some_and(|inner_cache| Rc::ptr_eq(cache, inner_cache))
                );
                assert!(
                    store
                        .builder(BuilderId::ROOT)
                        .typevar_binding_context
                        .is_none()
                );
                assert!(
                    store
                        .builder(BuilderId::ROOT)
                        .context
                        .inference_flags
                        .contains(
                            InferenceFlags::CHECK_UNBOUND_TYPEVARS
                                | InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR
                        )
                );
            }
            assert_eq!(root.typevar_binding_context, Some(binding));
            assert_eq!(root.context.inference_flags, flags);
            match (original_cache, root.expression_cache.as_ref()) {
                (Some(original), Some(cache)) => {
                    assert!(Weak::ptr_eq(&original, &Rc::downgrade(cache)));
                    assert_eq!(Rc::strong_count(cache), 1);
                }
                (None, None) => {
                    assert!(installed_cache.and_then(|cache| cache.upgrade()).is_none())
                }
                _ => anyhow::bail!("cache ownership changed during abort cleanup"),
            }
            // No semantic operation ran. This checks temporary state, not rollback of inferred results.
            assert!(root.expressions.is_empty());
        }
        Ok(())
    }

    #[test]
    fn real_speculative_slots_retire_in_innermost_order() -> anyhow::Result<()> {
        let db = database()?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let env = ProgramEnvironment::from_file(file);
        let (mut root, _, _) = builder(&db, &module, &env)?;
        root.setup_expression_cache();
        let mut store = BuilderStore::new(&mut root);
        let outer = store.speculate(BuilderId::ROOT, false);
        let inner = store.speculate(outer, true);
        let inner_builder = store.take_speculative(inner);
        assert_eq!(store.speculative.len(), 1);
        assert!(std::ptr::eq(
            inner_builder.module(),
            store.builder(outer).module()
        ));
        drop(inner_builder);
        assert_eq!(
            store
                .builder(outer)
                .expression_cache
                .as_ref()
                .map(Rc::strong_count),
            Some(2)
        );
        drop(store.take_speculative(outer));
        assert!(store.speculative.is_empty());
        assert_eq!(
            store
                .builder(BuilderId::ROOT)
                .expression_cache
                .as_ref()
                .map(Rc::strong_count),
            Some(1)
        );
        store.complete();
        Ok(())
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum Refused {
        Push,
        ArgumentBoundary,
        Unavailable(&'static str),
    }

    #[derive(Default)]
    struct Journal {
        events: RefCell<Vec<&'static str>>,
        drops: RefCell<Vec<&'static str>>,
        children: Cell<usize>,
        pending_with_child: Cell<usize>,
        speculative_at_child: Cell<usize>,
        resumed: Cell<bool>,
        scope_states: RefCell<Vec<(&'static str, InferenceFlags, local::DeferredExpressionState)>>,
        refuse_store: Cell<bool>,
        refuse_after_store: Cell<bool>,
        complete_annotation: Cell<bool>,
        scope_entries: Cell<usize>,
    }

    pub(super) struct Effects<'call, 'run, 'db: 'run> {
        endpoint: Option<&'call TaskEndpoint<'run, 'db>>,
        refuse_push: bool,
        journal: Rc<Journal>,
        argument_control: Option<Rc<ArgumentControl>>,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum ArgumentBoundary {
        NarrowCache,
        Committed,
        Contextual,
    }

    pub(super) struct ArgumentControl {
        pub(super) boundary: ArgumentBoundary,
        pub(super) before: arguments::Observation,
        pub(super) request: arguments::PreparedRequest,
        pub(super) suspend: bool,
        pub(super) reached: Cell<bool>,
        pub(super) argument_steps: Cell<usize>,
        pub(super) argument_frames: Cell<usize>,
        pub(super) attempted_argument_frames: Cell<usize>,
        pub(super) refuse_argument_push: bool,
        pub(super) cache_child: Cell<Option<BuilderId>>,
        pub(super) pending: RefCell<Option<arguments::Observation>>,
        pub(super) completed: Cell<bool>,
        pub(super) resumed: Cell<bool>,
    }

    impl ArgumentControl {
        pub(super) fn new(
            boundary: ArgumentBoundary,
            before: arguments::Observation,
            request: arguments::PreparedRequest,
            suspend: bool,
        ) -> Self {
            Self {
                boundary,
                before,
                request,
                suspend,
                reached: Cell::new(false),
                argument_steps: Cell::new(0),
                argument_frames: Cell::new(0),
                attempted_argument_frames: Cell::new(0),
                refuse_argument_push: false,
                cache_child: Cell::new(None),
                pending: RefCell::new(None),
                completed: Cell::new(false),
                resumed: Cell::new(false),
            }
        }

        fn inspect(
            &self,
            builders: &BuilderStore<'_, '_, '_>,
            id: BuilderId,
        ) -> Result<(), Refused> {
            let pending = self.pending.borrow();
            let Some(pending) = pending.as_ref() else {
                return Err(Refused::Unavailable("driver has no pending argument"));
            };
            assert_eq!(self.argument_steps.get(), 1);
            assert_eq!(self.argument_frames.get(), 1);
            let expected_slots = match self.boundary {
                ArgumentBoundary::NarrowCache => {
                    assert!(pending.narrow.is_some());
                    assert!(pending.contextual_child.is_none());
                    assert_eq!(self.cache_child.get(), Some(id));
                    3
                }
                ArgumentBoundary::Committed => {
                    assert!(pending.predecessor.is_some());
                    assert!(pending.contextual_child.is_none());
                    assert_eq!(id, BuilderId::ROOT);
                    assert!(self.cache_child.get().is_none());
                    1
                }
                ArgumentBoundary::Contextual => {
                    assert!(pending.narrow.is_none());
                    assert!(pending.predecessor.is_none());
                    assert!(pending.contextual_child.is_some());
                    assert_eq!(self.cache_child.get(), Some(id));
                    3
                }
            };
            assert_eq!(builders.speculative.len(), expected_slots);
            let root_cache = builders.builder(BuilderId::ROOT).expression_cache.as_ref();
            assert!(root_cache.is_some_and(|cache| Rc::strong_count(cache) == expected_slots + 1));
            for speculative in &builders.speculative {
                assert!(
                    root_cache
                        .zip(speculative.expression_cache.as_ref())
                        .is_some_and(|(root, selected)| Rc::ptr_eq(root, selected))
                );
            }
            Ok(())
        }

        async fn stop<T>(&self) -> Result<T, Refused> {
            assert!(!self.reached.replace(true));
            if self.suspend {
                std::future::pending().await
            } else {
                Err(Refused::ArgumentBoundary)
            }
        }
    }

    impl Effects<'_, '_, '_> {
        pub(super) fn argument_control(control: Rc<ArgumentControl>) -> Self {
            Self {
                endpoint: None,
                refuse_push: false,
                journal: Rc::new(Journal::default()),
                argument_control: Some(control),
            }
        }
    }

    impl<'db, 'ast> source_expression::ContextualExpressionEffects<'db, 'ast> for Effects<'_, '_, 'db> {
        type Error = Refused;

        async fn contextual(
            &self,
            _builder: &mut TypeInferenceBuilder<'db, 'ast>,
            _expression: &ast::Expr,
            _target: Type<'db>,
        ) -> Result<Option<Type<'db>>, Refused> {
            if let Some(control) = &self.argument_control
                && matches!(
                    control.boundary,
                    ArgumentBoundary::NarrowCache | ArgumentBoundary::Contextual
                )
            {
                return control.stop().await;
            }
            Err(Refused::Unavailable("contextual"))
        }

        async fn store(
            &self,
            _builder: &mut TypeInferenceBuilder<'db, 'ast>,
            _expression: &ast::Expr,
            _ty: Type<'db>,
        ) -> Result<(), Refused> {
            Err(Refused::Unavailable("store"))
        }
    }

    struct Child(Rc<Journal>);
    impl Child {
        fn new(journal: Rc<Journal>) -> Self {
            journal.children.set(journal.children.get() + 1);
            Self(journal)
        }
    }
    impl Drop for Child {
        fn drop(&mut self) {
            assert!(self.0.drops.borrow().is_empty());
            self.0.children.set(self.0.children.get() - 1);
            self.0.drops.borrow_mut().push("child");
        }
    }

    struct CanonicalBorrow<'borrow, 'root, 'db, 'ast> {
        builders: &'borrow mut BuilderStore<'root, 'db, 'ast>,
        id: BuilderId,
        journal: Rc<Journal>,
    }
    impl Drop for CanonicalBorrow<'_, '_, '_, '_> {
        fn drop(&mut self) {
            assert_eq!(self.journal.children.get(), 0);
            let builder = self.builders.builder(self.id);
            assert!(builder.typevar_binding_context.is_none());
            assert!(
                builder
                    .context
                    .inference_flags
                    .contains(InferenceFlags::CHECK_UNBOUND_TYPEVARS)
            );
            assert!(builder.expressions.is_empty());
            assert!(!builder.module().suite().is_empty());
            let root_cache = self
                .builders
                .builder(BuilderId::ROOT)
                .expression_cache
                .as_ref();
            assert!(
                root_cache
                    .zip(builder.expression_cache.as_ref())
                    .is_some_and(|(root, selected)| {
                        Rc::ptr_eq(root, selected)
                            && Rc::strong_count(root) == self.builders.speculative.len() + 1
                    })
            );
            self.journal.drops.borrow_mut().push("canonical borrow");
        }
    }

    impl<'db, 'ast> OwnedArgumentEffects<'db, 'ast> for Effects<'_, '_, 'db> {
        type Error = Refused;
        type Builder = crate::types::constraints::ConstraintSetBuilder<'db>;
        type CustomSpecializationTarget = std::convert::Infallible;

        async fn prepared<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _data: call::CallData<'db, 'expr>,
            _arguments: CallArguments<'expr, 'db>,
        ) -> Result<call::Prepared<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("prepared"))
        }
        async fn install_prepared<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            id: BuilderId,
            prepared: call::Prepared<'db, 'expr>,
        ) -> Result<PreparedCall<'db>, Refused> {
            let Ok(result) =
                OrdinaryOwnedArgumentEffects::default().install_prepared(owners, id, prepared);
            Ok(result)
        }
        async fn take_active<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            owner: ActiveArgument,
        ) -> Result<TakenArgument<'db, 'expr, OwnedState<'db, 'expr>>, Refused> {
            let state = owners
                .active(&owner)
                .ok_or(Refused::Unavailable("active owner"))?;
            let control = self
                .argument_control
                .as_ref()
                .ok_or(Refused::Unavailable("argument control"))?;
            assert_eq!(state.observe(), Some(control.before));
            assert_eq!(state.prepared_request(), Some(control.request));
            let Ok(taken) = OrdinaryOwnedArgumentEffects::default().take_active(owners, owner);
            Ok(taken)
        }
        async fn advance<'expr>(
            &self,
            taken: TakenArgument<'db, 'expr, OwnedState<'db, 'expr>>,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<TakenArgument<'db, 'expr, OwnedAction<'db, 'expr>>, Refused> {
            let control = self
                .argument_control
                .as_ref()
                .ok_or(Refused::Unavailable("argument control"))?;
            assert_eq!(taken.payload.phase.observe(), Some(control.before));
            assert_eq!(
                taken.payload.phase.prepared_request(),
                Some(control.request)
            );
            assert_eq!(control.argument_steps.replace(1), 0);
            // This reached phase only moves its local cursor and pending owner. Binding checks,
            // context collection and expression inference are outside this transition.
            let Ok(advanced) = OrdinaryOwnedArgumentEffects::default().advance(taken, builders);
            Ok(advanced)
        }
        async fn install_action<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            taken: TakenArgument<'db, 'expr, OwnedAction<'db, 'expr>>,
        ) -> Result<ArgumentStep<'db, 'expr>, Refused> {
            let Ok(step) = OrdinaryOwnedArgumentEffects::default().install_action(owners, taken);
            Ok(step)
        }
        async fn take_pending<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            owner: PendingArgument,
        ) -> Result<TakenArgument<'db, 'expr, OwnedPending<'db, 'expr>>, Refused> {
            let Ok(taken) = OrdinaryOwnedArgumentEffects::default().take_pending(owners, owner);
            Ok(taken)
        }
        async fn resume<'expr>(
            &self,
            _taken: TakenArgument<'db, 'expr, OwnedPending<'db, 'expr>>,
            _ty: Type<'db>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<TakenArgument<'db, 'expr, OwnedState<'db, 'expr>>, Refused> {
            Err(Refused::Unavailable("resume"))
        }
        async fn install_active<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            taken: TakenArgument<'db, 'expr, OwnedState<'db, 'expr>>,
        ) -> Result<ActiveArgument, Refused> {
            let Ok(owner) = OrdinaryOwnedArgumentEffects::default().install_active(owners, taken);
            Ok(owner)
        }
        async fn take_completed<'owner, 'expr>(
            &self,
            owners: &'owner mut LocalOwners<'db, 'expr>,
            owner: CompletedArgument,
        ) -> Result<CompletedArgumentLease<'owner, 'db, 'expr>, Refused> {
            let Ok(taken) = OrdinaryOwnedArgumentEffects::default().take_completed(owners, owner);
            Ok(taken)
        }
        async fn finish<'expr>(
            &self,
            _lease: CompletedArgumentLease<'_, 'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<FinishedOwner<'db>, Refused> {
            Err(Refused::Unavailable("finish"))
        }
        async fn retire<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            finished: FinishedOwner<'db>,
        ) -> Result<Type<'db>, Refused> {
            let Ok(ty) = OrdinaryOwnedArgumentEffects::default().retire(owners, finished);
            Ok(ty)
        }
    }

    impl<'db: 'run, 'ast, 'run> LocalEffects<'db, 'ast> for Effects<'_, 'run, 'db> {
        type Error = Refused;
        type Builder = crate::types::constraints::ConstraintSetBuilder<'db>;
        type CustomSpecializationTarget = std::convert::Infallible;
        type StringAnnotations = local::string_annotation::OrdinaryStorage;

        async fn parse_string_annotation<'expr>(
            &self,
            builders: &BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            string: &ast::ExprStringLiteral,
            storage: &'expr Self::StringAnnotations,
        ) -> Result<Option<&'expr ast::Expr>, Refused> {
            let Ok(parsed) = OrdinaryLocalEffects::default()
                .parse_string_annotation(builders, id, string, storage);
            Ok(parsed)
        }

        async fn prepare_string_annotation<'expr>(
            &self,
            builders: &BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            string: &'expr ast::ExprStringLiteral,
            parsed: &'expr ast::Expr,
        ) -> Result<local::string_annotation::Scope<'expr>, Refused> {
            let Ok(scope) = OrdinaryLocalEffects::default()
                .prepare_string_annotation(builders, id, string, parsed);
            Ok(scope)
        }

        async fn enter_string_annotation(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            scope: local::string_annotation::Scope<'_>,
        ) -> Result<(), Refused> {
            let Ok(()) =
                OrdinaryLocalEffects::default().enter_string_annotation(builders, id, scope);
            Ok(())
        }

        async fn finish_string_annotation(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            scope: local::string_annotation::Scope<'_>,
        ) -> Result<(), Refused> {
            let Ok(()) =
                OrdinaryLocalEffects::default().finish_string_annotation(builders, id, scope);
            Ok(())
        }

        async fn next<'expr>(
            &self,
            work: &mut Option<Work<'db, 'expr>>,
        ) -> Result<Option<Work<'db, 'expr>>, Refused> {
            self.journal.events.borrow_mut().push("next");
            Ok(work.take())
        }
        async fn continue_with<'expr>(
            &self,
            work: &mut Option<Work<'db, 'expr>>,
            next: Work<'db, 'expr>,
        ) -> Result<(), Refused> {
            self.journal.events.borrow_mut().push("continue");
            *work = Some(next);
            Ok(())
        }
        async fn push<'expr>(
            &self,
            invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr>,
            frame: Frame<'db, 'expr>,
        ) -> Result<(), Refused> {
            self.journal.events.borrow_mut().push("push");
            if self.refuse_push {
                return Err(Refused::Push);
            }
            if let Some(control) = &self.argument_control {
                match &frame {
                    Frame::Argument(owner) => {
                        control
                            .attempted_argument_frames
                            .set(control.attempted_argument_frames.get() + 1);
                        let pending = invocation
                            .owners
                            .pending(owner)
                            .ok_or(Refused::Unavailable("pending owner missing before push"))?;
                        assert_eq!(*control.pending.borrow(), Some(pending.observe()));
                        assert_eq!(owner.0 + 1, invocation.owners.slots.len());
                        let expected_builders = match control.boundary {
                            ArgumentBoundary::NarrowCache | ArgumentBoundary::Contextual => 2,
                            ArgumentBoundary::Committed => 1,
                        };
                        assert_eq!(invocation.builders.speculative.len(), expected_builders);
                        assert_eq!(
                            invocation
                                .builders
                                .builder(BuilderId::ROOT)
                                .expression_cache
                                .as_ref()
                                .map(Rc::strong_count),
                            Some(expected_builders + 1)
                        );
                        if control.refuse_argument_push {
                            assert_eq!(control.argument_frames.get(), 0);
                            return Err(Refused::Push);
                        }
                        control
                            .argument_frames
                            .set(control.argument_frames.get() + 1);
                    }
                    Frame::Cache(parent, child, _, _) => {
                        let pending = control.pending.borrow();
                        let Some(pending) = pending.as_ref() else {
                            return Err(Refused::Unavailable("cache without pending argument"));
                        };
                        assert_eq!(*parent, pending.contextual_child.unwrap_or(pending.pass));
                        assert!(control.cache_child.replace(Some(*child)).is_none());
                    }
                    _ => return Err(Refused::Unavailable("unexpected argument frame")),
                }
            }
            invocation.frames.push(frame);
            Ok(())
        }
        async fn pop<'expr>(
            &self,
            frames: &mut Vec<Frame<'db, 'expr>>,
        ) -> Result<Option<Frame<'db, 'expr>>, Refused> {
            Ok(frames.pop())
        }
        async fn push_annotation<'expr>(&self, _invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr>, _continuation: local::AnnotationContinuation<'expr>) -> Result<(), Refused> {
            Err(Refused::Unavailable("push_annotation"))
        }

        async fn pop_annotation<'expr>(&self, invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr>) -> Result<Option<local::AnnotationContinuation<'expr>>, Refused> {
            Ok(invocation.annotations.pop())
        }

        async fn canonical(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            expression: &ast::Expr,
            _tcx: TypeContext<'db>,
        ) -> Result<Option<Type<'db>>, Refused> {
            self.journal.events.borrow_mut().push("canonical");
            assert!(
                builders
                    .builder(id)
                    .index
                    .try_expression(expression)
                    .is_some()
            );
            self.journal
                .speculative_at_child
                .set(builders.speculative.len());
            let _borrowed = CanonicalBorrow {
                builders,
                id,
                journal: self.journal.clone(),
            };
            let Some(endpoint) = self.endpoint else {
                return Err(Refused::Unavailable("canonical"));
            };
            endpoint
                .child_call(|| async {
                    let child = Child::new(self.journal.clone());
                    endpoint
                        .demand(move || async move {
                            let _child = child;
                            Err::<Result<Option<Type<'db>>, Refused>, _>(RunError::Refused(
                                Incomplete::Allowance,
                            ))
                        })?
                        .await
                })
                .await
        }
        async fn existing(
            &self,
            _builders: &BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _expression: &ast::Expr,
        ) -> Result<Option<Type<'db>>, Refused> {
            Err(Refused::Unavailable("existing"))
        }
        async fn cache_enabled(
            &self,
            builders: &BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
        ) -> Result<bool, Refused> {
            Ok(builders.builder(id).expression_cache.is_some())
        }
        async fn cache_lookup(
            &self,
            builders: &BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            expression: &ast::Expr,
            tcx: TypeContext<'db>,
        ) -> Result<Option<ExpressionCacheEntry<'db>>, Refused> {
            let Ok(entry) =
                OrdinaryLocalEffects::default().cache_lookup(builders, id, expression, tcx);
            if let Some(control) = &self.argument_control {
                assert_eq!(
                    entry.is_some(),
                    control.boundary == ArgumentBoundary::Committed,
                    "argument fixture did not reach its intended cache hit or miss"
                );
            }
            Ok(entry)
        }
        async fn cache_hit(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            _expression: &ast::Expr,
            _entry: ExpressionCacheEntry<'db>,
        ) -> Result<Type<'db>, Refused> {
            if let Some(control) = &self.argument_control
                && control.boundary == ArgumentBoundary::Committed
            {
                control.inspect(builders, id)?;
                return control.stop().await;
            }
            Err(Refused::Unavailable("cache_hit"))
        }
        async fn speculate(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
        ) -> Result<BuilderId, Refused> {
            Ok(builders.speculate(id, false))
        }
        async fn cache_commit(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _parent: BuilderId,
            _child: BuilderId,
            _expression: &ast::Expr,
            _tcx: TypeContext<'db>,
            _ty: Type<'db>,
        ) -> Result<(), Refused> {
            Err(Refused::Unavailable("cache_commit"))
        }
        async fn contextual_dispatch(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            expression: &ast::Expr,
            tcx: TypeContext<'db>,
        ) -> Result<source_expression::ContextualExpressionResult<'db>, Refused> {
            if let Some(control) = &self.argument_control {
                control.inspect(builders, id)?;
                assert!(tcx.annotation.is_some());
            }
            source_expression::contextual_expression_with(
                builders.get_mut(id),
                expression,
                tcx,
                self,
            )
            .await
        }
        async fn other_expression(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _expression: &ast::Expr,
            _tcx: TypeContext<'db>,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("other_expression"))
        }
        async fn finish_expression(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _expression: &ast::Expr,
            _ty: Type<'db>,
            _tcx: TypeContext<'db>,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("finish_expression"))
        }
        async fn enter_callee(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
        ) -> Result<CalleeState<'db>, Refused> {
            self.journal.events.borrow_mut().push("enter callee");
            let Ok(saved) = OrdinaryLocalEffects::default().enter_callee(builders, id);
            assert!(saved.binding.is_some());
            assert!(!saved.check_unbound);
            assert!(builders.builder(id).typevar_binding_context.is_none());
            Ok(saved)
        }
        async fn restore_callee(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _state: CalleeState<'db>,
        ) -> Result<(), Refused> {
            Err(Refused::Unavailable("restore_callee"))
        }
        async fn prepare_annotation_scope(
            &self,
            builders: &BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            state: local::DeferredExpressionState,
        ) -> Result<local::AnnotationScope, Refused> {
            let Ok(scope) =
                OrdinaryLocalEffects::default().prepare_annotation_scope(builders, id, state);
            Ok(scope)
        }
        async fn enter_annotation_scope(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            scope: local::AnnotationScope,
        ) -> Result<(), Refused> {
            self.journal
                .scope_entries
                .set(self.journal.scope_entries.get() + 1);
            let Ok(()) =
                OrdinaryLocalEffects::default().enter_annotation_scope(builders, id, scope);
            Ok(())
        }
        async fn restore_annotation_scope(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            scope: local::AnnotationScope,
        ) -> Result<(), Refused> {
            let Ok(()) =
                OrdinaryLocalEffects::default().restore_annotation_scope(builders, id, scope);
            Ok(())
        }
        async fn store_annotation_qualifiers(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            expression: &ast::Expr,
            qualifiers: local::TypeQualifiers,
        ) -> Result<(), Refused> {
            let Ok(()) = OrdinaryLocalEffects::default()
                .store_annotation_qualifiers(builders, id, expression, qualifiers);
            Ok(())
        }
        async fn store_type_expression(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            expression: &ast::Expr,
            ty: Type<'db>,
        ) -> Result<(), Refused> {
            let builder = builders.builder(id);
            self.journal.scope_states.borrow_mut().push((
                "store",
                builder.context.inference_flags,
                builder.deferred_state,
            ));
            if self.journal.refuse_store.get() {
                return Err(Refused::Unavailable("store_type_expression"));
            }
            let Ok(()) =
                OrdinaryLocalEffects::default().store_type_expression(builders, id, expression, ty);
            Ok(())
        }
        async fn prepare_type_expression_scope(
            &self,
            builders: &BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            mode: local::TypeExpressionMode,
        ) -> Result<Option<local::TypeExpressionScope>, Refused> {
            let Ok(scope) =
                OrdinaryLocalEffects::default().prepare_type_expression_scope(builders, id, mode);
            Ok(scope)
        }
        async fn enter_type_expression_scope(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            scope: local::TypeExpressionScope,
        ) -> Result<(), Refused> {
            self.journal
                .scope_entries
                .set(self.journal.scope_entries.get() + 1);
            let Ok(()) =
                OrdinaryLocalEffects::default().enter_type_expression_scope(builders, id, scope);
            Ok(())
        }
        async fn restore_type_expression_before_store(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            scope: local::TypeExpressionScope,
        ) -> Result<(), Refused> {
            let Ok(()) = OrdinaryLocalEffects::default()
                .restore_type_expression_before_store(builders, id, scope);
            Ok(())
        }
        async fn restore_type_expression_after_store(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            scope: local::TypeExpressionScope,
        ) -> Result<(), Refused> {
            if self.journal.refuse_after_store.get() {
                return Err(Refused::Unavailable("restore_type_expression_after_store"));
            }
            let Ok(()) = OrdinaryLocalEffects::default()
                .restore_type_expression_after_store(builders, id, scope);
            Ok(())
        }
        async fn start_annotation<'expr>(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            _annotation: &'expr ast::Expr,
            _policy: local::PEP613Policy,
        ) -> Result<local::AnnotationStep<'db, 'expr>, Refused> {
            let builder = builders.builder(id);
            self.journal.scope_states.borrow_mut().push((
                "annotation",
                builder.context.inference_flags,
                builder.deferred_state,
            ));
            Err(Refused::Unavailable("start_annotation"))
        }
        async fn resume_annotation<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _pending: local::AnnotationPending<'expr>,
            _ty: Type<'db>,
        ) -> Result<local::AnnotationStep<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("resume_annotation"))
        }
        async fn resume_qualifier<'expr>(&self, _builders: &mut BuilderStore<'_, 'db, 'ast>, _root: &local::AnnotationRoot<'expr>, _pending: local::QualifierPending<'expr>, _ty: local::TypeAndQualifiers<'db>) -> Result<local::AnnotationStep<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("resume_qualifier"))
        }


        async fn start_tuple_value<'expr>(
            &self,
            _owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>,
            _id: BuilderId,
            _tuple: &'expr ast::ExprTuple,
            _context: TypeContext<'db>,
        ) -> Result<local::tuple_expression::Active, Refused> {
            Err(Refused::Unavailable("start_tuple_value"))
        }

        async fn tuple_value_step<'expr>(
            &self,
            _owner: local::tuple_expression::Active,
            _owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<local::tuple_expression::Step<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("tuple_value_step"))
        }

        async fn resume_tuple_value<'expr>(
            &self,
            _owner: local::tuple_expression::Waiting,
            _owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>,
        ) -> Result<local::tuple_expression::Active, Refused> {
            Err(Refused::Unavailable("resume_tuple_value"))
        }

        async fn finish_tuple_value<'expr>(
            &self,
            _owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>,
            _owner: local::tuple_expression::Finished,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("finish_tuple_value"))
        }

        async fn start_type_expression<'expr>(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            _request: local::TypeExpressionRequest<'db, 'expr>,
        ) -> Result<local::TypeExpressionStep<'db, 'expr>, Refused> {
            let builder = builders.builder(id);
            self.journal.scope_states.borrow_mut().push((
                "type",
                builder.context.inference_flags,
                builder.deferred_state,
            ));
            Err(Refused::Unavailable("start_type_expression"))
        }
        async fn resume_type_expression<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _pending: local::TypeExpressionPending<'db, 'expr>,
            _ty: Type<'db>,
        ) -> Result<local::TypeExpressionStep<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("resume_type_expression"))
        }
        async fn start_assignment<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _target: &'expr ast::Expr,
            _call: &'expr ast::ExprCall,
            _definition: Definition<'db>,
            _callable_type: Type<'db>,
        ) -> Result<local::assignment::Start<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("start_assignment"))
        }
        async fn finish_assignment(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _target: &ast::Expr,
            _call: &ast::ExprCall,
            _callable_type: Type<'db>,
            _ty: Type<'db>,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("finish_assignment"))
        }
        async fn legacy_typevar<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _state: local::legacy::State<'db, 'expr>,
        ) -> Result<local::legacy::Action<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("legacy_typevar"))
        }
        async fn resume_legacy_typevar<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _pending: local::legacy::Pending<'db, 'expr>,
            _ty: Type<'db>,
        ) -> Result<local::legacy::State<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("resume_legacy_typevar"))
        }
        async fn subscript_receiver<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _subscript: &'expr ast::ExprSubscript,
            _ty: Type<'db>,
        ) -> Result<local::subscript::SubscriptStart<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("subscript_receiver"))
        }
        async fn subscript_slice<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _pending: local::subscript::SubscriptPending<'db, 'expr>,
            _ty: Type<'db>,
        ) -> Result<Result<Type<'db>, Type<'db>>, Refused> {
            Err(Refused::Unavailable("subscript_slice"))
        }
        async fn start_call<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _expression: &'expr ast::ExprCall,
            _ty: Type<'db>,
            _tcx: TypeContext<'db>,
        ) -> Result<call::Start<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("start_call"))
        }
        async fn prepare<'expr>(
            &self,
            _id: BuilderId,
            _arguments: &'expr ast::Arguments,
        ) -> Result<Preparation<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("prepare"))
        }
        async fn preparation_step<'expr>(
            &self,
            _preparation: Preparation<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<PreparationStep<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("preparation_step"))
        }
        async fn resume_splat<'expr>(
            &self,
            _splat: Splat<'db, 'expr>,
            _ty: Type<'db>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<Preparation<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("resume_splat"))
        }
        async fn prepared_call<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _owners: &mut LocalOwners<'db, 'expr>,
            _id: BuilderId,
            _data: call::CallData<'db, 'expr>,
            _arguments: CallArguments<'expr, 'db>,
        ) -> Result<PreparedCall<'db>, Refused> {
            Err(Refused::Unavailable("prepared_call"))
        }
        async fn argument_step<'expr>(
            &self,
            owner: ActiveArgument,
            owners: &mut LocalOwners<'db, 'expr>,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<ArgumentStep<'db, 'expr>, Refused> {
            let step = local::owned_argument_step(owner, owners, builders, self).await?;
            let ArgumentStep::Infer {
                pending, builder, ..
            } = &step
            else {
                return Err(Refused::Unavailable(
                    "prepared phase did not yield an argument",
                ));
            };
            let control = self
                .argument_control
                .as_ref()
                .ok_or(Refused::Unavailable("argument control"))?;
            let observed = owners
                .pending(pending)
                .ok_or(Refused::Unavailable("pending argument"))?
                .observe();
            match control.request {
                arguments::PreparedRequest::Unique => {
                    assert_eq!(observed, control.before);
                    assert_eq!(*builder, observed.pass);
                }
                arguments::PreparedRequest::Contextual => {
                    assert!(control.before.contextual_child.is_none());
                    assert_eq!(observed.contextual_child, Some(*builder));
                    assert_ne!(*builder, observed.pass);
                    assert_eq!(
                        arguments::Observation {
                            contextual_child: None,
                            ..observed
                        },
                        control.before
                    );
                }
            }
            *control.pending.borrow_mut() = Some(observed);
            Ok(step)
        }
        async fn resume_argument<'expr>(
            &self,
            _owner: PendingArgument,
            _ty: Type<'db>,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<ActiveArgument, Refused> {
            if let Some(control) = &self.argument_control {
                control.resumed.set(true);
            }
            Err(Refused::Unavailable("resume_argument"))
        }
        async fn finish_call<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _owners: &mut LocalOwners<'db, 'expr>,
            _owner: CompletedArgument,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("finish_call"))
        }
        async fn start_callable_annotation<'expr>(
            &self,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _request: local::callable_annotation::Request<'expr>,
        ) -> Result<local::callable_annotation::Active, Refused> {
            Err(Refused::Unavailable("start_callable_annotation"))
        }
        async fn callable_annotation_step<'expr>(
            &self,
            _owner: local::callable_annotation::Active,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<local::callable_annotation::Step<'expr>, Refused> {
            Err(Refused::Unavailable("callable_annotation_step"))
        }
        async fn resume_callable_annotation<'expr>(
            &self,
            _owner: local::callable_annotation::Waiting,
            _ty: Type<'db>,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<local::callable_annotation::Active, Refused> {
            Err(Refused::Unavailable("resume_callable_annotation"))
        }
        async fn finish_callable_annotation<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _owners: &mut LocalOwners<'db, 'expr>,
            _owner: local::callable_annotation::Finished,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("finish_callable_annotation"))
        }
        async fn start_tuple_annotation<'expr>(
            &self,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _request: local::tuple_annotation::Request<'expr>,
        ) -> Result<local::tuple_annotation::Active, Refused> {
            Err(Refused::Unavailable("start_tuple_annotation"))
        }
        async fn tuple_annotation_step<'expr>(
            &self,
            _owner: local::tuple_annotation::Active,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<local::tuple_annotation::Step<'expr>, Refused> {
            Err(Refused::Unavailable("tuple_annotation_step"))
        }
        async fn resume_tuple_annotation<'expr>(
            &self,
            _owner: local::tuple_annotation::Waiting,
            _ty: Type<'db>,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<local::tuple_annotation::Active, Refused> {
            Err(Refused::Unavailable("resume_tuple_annotation"))
        }
        async fn finish_tuple_annotation<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _owners: &mut LocalOwners<'db, 'expr>,
            _owner: local::tuple_annotation::Finished,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("finish_tuple_annotation"))
        }
        async fn start_specialization<'expr>(
            &self,
            _owners: &mut LocalOwners<'db, 'expr>,
            _id: BuilderId,
            _subscript: &'expr ast::ExprSubscript,
            _value_ty: Type<'db>,
            _class: crate::types::StaticClassLiteral<'db>,
            _generic_context: crate::types::generics::GenericContext<'db>,
            _kind: local::ClassSpecializationKind,
        ) -> Result<local::ActiveSpecialization, Refused> {
            Err(Refused::Unavailable("start_specialization"))
        }
        async fn specialization_step<'expr>(
            &self,
            _owner: local::ActiveSpecialization,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<local::SpecializationStep<'expr>, Refused> {
            Err(Refused::Unavailable("specialization_step"))
        }
        async fn resume_specialization<'expr>(
            &self,
            _owner: local::PendingSpecialization,
            _ty: Type<'db>,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<local::ActiveSpecialization, Refused> {
            Err(Refused::Unavailable("resume_specialization"))
        }
        async fn finish_specialization<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _owners: &mut LocalOwners<'db, 'expr>,
            _owner: local::CompletedSpecialization,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("finish_specialization"))
        }
        async fn enter_paramspec(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
        ) -> Result<bool, Refused> {
            Err(Refused::Unavailable("enter_paramspec"))
        }
        async fn restore_paramspec(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _previous: bool,
        ) -> Result<(), Refused> {
            Err(Refused::Unavailable("restore_paramspec"))
        }
        async fn permit_paramspec(
            &self,
            policy: arguments::ArgumentPolicy,
            _expression: &ast::Expr,
        ) -> Result<bool, Refused> {
            if self.argument_control.is_some()
                && matches!(policy, arguments::ArgumentPolicy::Ordinary)
            {
                return Ok(false);
            }
            Err(Refused::Unavailable("permit_paramspec"))
        }
        async fn complete<'expr>(
            &self,
            invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr>,
        ) -> Result<(), Refused> {
            if self.journal.complete_annotation.get() {
                let Ok(()) = OrdinaryLocalEffects::default().complete(invocation);
                return Ok(());
            }
            if let Some(control) = &self.argument_control {
                control.completed.set(true);
            }
            Err(Refused::Unavailable("complete"))
        }
    }

    #[test]
    fn actual_drive_restores_callee_state_when_frame_push_refuses() -> anyhow::Result<()> {
        let db = database()?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let env = ProgramEnvironment::from_file(file);
        for assignment in [false, true] {
            let (mut root, call, binding) = builder(&db, &module, &env)?;
            let continuation = if assignment {
                local::CalleeContinuation::Assignment {
                    target: &call.func,
                    call,
                    definition: binding,
                    context: TypeContext::default(),
                }
            } else {
                local::CalleeContinuation::Return
            };
            let flags = root.context.inference_flags;
            let journal = Rc::new(Journal::default());
            let effects = Effects {
                endpoint: None,
                refuse_push: true,
                journal: journal.clone(),
                argument_control: None,
            };
            let syntax_storage = local::string_annotation::OrdinaryStorage::new();
            let syntax = &syntax_storage;
            let mut invocation = LocalInvocation::new(&mut root);
            let result = try_poll_immediate(local::drive(
                &mut Some(Work::Callee(BuilderId::ROOT, &call.func, continuation)),
                &mut invocation,
                LocalFacts,
                &effects,
                syntax,
            ));
            drop(invocation);
            assert_eq!(result, Poll::Ready(Err(Refused::Push)));
            assert_eq!(*journal.events.borrow(), ["next", "enter callee", "push"]);
            assert_eq!(root.typevar_binding_context, Some(binding));
            assert_eq!(root.context.inference_flags, flags);
            assert!(root.expression_cache.is_none());
            assert!(root.expressions.is_empty());
        }
        Ok(())
    }

    #[test]
    fn assignment_header_and_subscript_resume_refusals_restore_the_root() -> anyhow::Result<()> {
        let db = database()?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let env = ProgramEnvironment::from_file(file);
        let parsed = ruff_python_parser::parse_expression("Generic[marker]")?;
        let ast::Expr::Subscript(subscript) = parsed.expr() else {
            anyhow::bail!("missing subscript fixture");
        };
        for operation in [
            "finish_assignment",
            "resume_legacy_typevar",
            "subscript_receiver",
            "subscript_slice",
        ] {
            let (mut root, call, binding) = builder(&db, &module, &env)?;
            let flags = root.context.inference_flags;
            let frame = match operation {
                "finish_assignment" => {
                    Frame::AssignmentFinish(BuilderId::ROOT, &call.func, call, Type::unknown())
                }
                "resume_legacy_typevar" => {
                    let state = local::legacy::new(&call.func, call, binding, KnownClass::TypeVar);
                    let Ok(action) = local::legacy::advance_sync(
                        state,
                        &mut root,
                        &local::legacy::OrdinaryLegacyTypeVarEffects,
                    );
                    let local::legacy::Action::Infer { pending, .. } = action else {
                        anyhow::bail!("header fixture did not request its name expression");
                    };
                    Frame::LegacyTypeVar(BuilderId::ROOT, pending)
                }
                "subscript_receiver" => Frame::SubscriptReceiver(BuilderId::ROOT, subscript),
                "subscript_slice" => {
                    let Ok(start) = local::subscript::subscript_after_receiver_sync(
                        &mut root,
                        subscript,
                        Type::SpecialForm(SpecialFormType::Generic),
                        &local::subscript::OrdinarySubscriptEffects,
                    );
                    let local::subscript::SubscriptStart::Slice(pending) = start else {
                        anyhow::bail!("subscript fixture did not request its slice");
                    };
                    Frame::SubscriptSlice(BuilderId::ROOT, pending)
                }
                _ => anyhow::bail!("unknown continuation fixture"),
            };
            let syntax_storage = local::string_annotation::OrdinaryStorage::new();
            let syntax = &syntax_storage;
            let mut invocation = LocalInvocation::new(&mut root);
            invocation.frames.push(frame);
            let active = invocation.builders.get_mut(BuilderId::ROOT);
            active.typevar_binding_context = None;
            active
                .context
                .inference_flags
                .insert(InferenceFlags::CHECK_UNBOUND_TYPEVARS);
            active.setup_expression_cache();
            let effects = Effects {
                endpoint: None,
                refuse_push: false,
                journal: Rc::new(Journal::default()),
                argument_control: None,
            };
            assert_eq!(
                try_poll_immediate(local::drive(
                    &mut Some(Work::Return(Type::unknown())),
                    &mut invocation,
                    LocalFacts,
                    &effects,
                    syntax,
                )),
                Poll::Ready(Err(Refused::Unavailable(operation))),
            );
            drop(invocation);
            assert_eq!(root.typevar_binding_context, Some(binding));
            assert_eq!(root.context.inference_flags, flags);
            assert!(root.expression_cache.is_none());
            assert!(root.expressions.is_empty());
        }
        Ok(())
    }

    fn assert_deferred_state(
        actual: local::DeferredExpressionState,
        expected: local::DeferredExpressionState,
    ) {
        fn identity(state: local::DeferredExpressionState) -> (u8, Option<local::NodeKey>) {
            match state {
                local::DeferredExpressionState::None => (0, None),
                local::DeferredExpressionState::Deferred => (1, None),
                local::DeferredExpressionState::InStringAnnotation(key) => (2, Some(key)),
            }
        }
        assert_eq!(identity(actual), identity(expected));
    }

    #[test]
    fn annotation_entries_restore_state_on_refusal() -> anyhow::Result<()> {
        let db = database()?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let env = ProgramEnvironment::from_file(file);
        for string_state in [false, true] {
            for entry in [
                "annotation",
                "body",
                "scoped",
                "with_state",
                "no_store",
                "push",
            ] {
                let (mut root, call, binding) = builder(&db, &module, &env)?;
                root.context
                    .inference_flags
                    .insert(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT);
                root.context
                    .inference_flags
                    .set(InferenceFlags::IN_TYPE_EXPRESSION, string_state);
                root.context
                    .inference_flags
                    .set(InferenceFlags::IN_NESTED_TYPE_EXPRESSION, !string_state);
                root.deferred_state = if string_state {
                    local::DeferredExpressionState::InStringAnnotation(local::NodeKey::from_node(
                        call,
                    ))
                } else {
                    local::DeferredExpressionState::Deferred
                };
                let flags = root.context.inference_flags;
                let deferred = root.deferred_state;
                let (work, refusal) = match entry {
                    "annotation" => (
                        Work::AnnotationStart(
                            BuilderId::ROOT,
                            &call.func,
                            local::DeferredExpressionState::None,
                            local::PEP613Policy::Disallowed,
                        ),
                        Refused::Unavailable("start_annotation"),
                    ),
                    "body" => (
                        Work::AnnotationBody(
                            local::AnnotationRoot {
                                builder: BuilderId::ROOT,
                                annotation: &call.func,
                                saved: None,
                            },
                            local::PEP613Policy::Disallowed,
                        ),
                        Refused::Unavailable("start_annotation"),
                    ),
                    _ => {
                        let mode = match entry {
                            "with_state" => local::TypeExpressionMode::ScopedWithState(
                                local::DeferredExpressionState::None,
                            ),
                            "no_store" => local::TypeExpressionMode::NoStore,
                            _ => local::TypeExpressionMode::Scoped,
                        };
                        (
                            Work::TypeExpression(
                                BuilderId::ROOT,
                                local::TypeExpressionRequest::Expression {
                                    expression: &call.func,
                                    mode,
                                },
                            ),
                            if entry == "push" {
                                Refused::Push
                            } else {
                                Refused::Unavailable("start_type_expression")
                            },
                        )
                    }
                };
                let effects = Effects {
                    endpoint: None,
                    refuse_push: entry == "push",
                    journal: Rc::new(Journal::default()),
                    argument_control: None,
                };
                let syntax_storage = local::string_annotation::OrdinaryStorage::new();
                let syntax = &syntax_storage;
                let mut invocation = LocalInvocation::new(&mut root);
                assert_eq!(
                    try_poll_immediate(local::drive(
                        &mut Some(work),
                        &mut invocation,
                        LocalFacts,
                        &effects,
                        syntax,
                    )),
                    Poll::Ready(Err(refusal))
                );
                drop(invocation);
                assert_eq!(
                    effects.journal.scope_entries.get(),
                    usize::from(!matches!(entry, "body" | "no_store" | "push"))
                );
                let states = effects.journal.scope_states.borrow();
                if entry == "push" {
                    assert!(states.is_empty());
                } else {
                    let [(_, body_flags, body_deferred)] = states.as_slice() else {
                        anyhow::bail!("missing annotation body observation");
                    };
                    let mut expected_flags = flags;
                    if entry == "annotation" {
                        expected_flags.insert(InferenceFlags::CHECK_UNBOUND_TYPEVARS);
                    } else if matches!(entry, "scoped" | "with_state") {
                        expected_flags.insert(
                            InferenceFlags::IN_TYPE_EXPRESSION
                                | InferenceFlags::IN_NESTED_TYPE_EXPRESSION,
                        );
                    }
                    assert_eq!(*body_flags, expected_flags);
                    let requested_none = matches!(entry, "annotation" | "with_state");
                    assert_deferred_state(
                        *body_deferred,
                        if requested_none && !string_state {
                            local::DeferredExpressionState::None
                        } else {
                            deferred
                        },
                    );
                }
                assert_eq!(root.typevar_binding_context, Some(binding));
                assert_eq!(root.context.inference_flags, flags);
                assert_deferred_state(root.deferred_state, deferred);
                assert!(root.expressions.is_empty());
                assert!(root.qualifiers.is_empty());
                assert!(root.expression_cache.is_none());
            }
        }
        Ok(())
    }

    #[test]
    fn stub_annotation_entries_defer_without_losing_string_identity() -> anyhow::Result<()> {
        let path = "/src/local_annotation.pyi";
        let db = TestDbBuilder::new().with_file(path, "marker\n").build()?;
        let file = db.program_file(system_path_to_file(&db, path)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let env = ProgramEnvironment::from_file(file);
        let [ast::Stmt::Expr(statement)] = module.suite().as_slice() else {
            anyhow::bail!("missing annotation fixture");
        };
        let expression = statement.value.as_ref();
        for annotation in [false, true] {
            for string_state in [false, true] {
                let mut root = TypeInferenceBuilder::new(
                    &db,
                    &env,
                    InferenceRegion::Scope(global_scope(&db, file), TypeContext::default()),
                    file.file(&db),
                    file,
                    semantic_index(&db, file),
                    &module,
                );
                root.context.defuse();
                let initial = root.deferred_state;
                let flags = root.context.inference_flags;
                let state = if string_state {
                    local::DeferredExpressionState::InStringAnnotation(local::NodeKey::from_node(
                        expression,
                    ))
                } else {
                    local::DeferredExpressionState::None
                };
                let work = if annotation {
                    Work::AnnotationStart(
                        BuilderId::ROOT,
                        expression,
                        state,
                        local::PEP613Policy::Disallowed,
                    )
                } else {
                    Work::TypeExpression(
                        BuilderId::ROOT,
                        local::TypeExpressionRequest::Expression {
                            expression,
                            mode: local::TypeExpressionMode::ScopedWithState(state),
                        },
                    )
                };
                let effects = Effects {
                    endpoint: None,
                    refuse_push: false,
                    journal: Rc::new(Journal::default()),
                    argument_control: None,
                };
                let syntax_storage = local::string_annotation::OrdinaryStorage::new();
                let syntax = &syntax_storage;
                let mut invocation = LocalInvocation::new(&mut root);
                let result = try_poll_immediate(local::drive(
                    &mut Some(work),
                    &mut invocation,
                    LocalFacts,
                    &effects,
                    syntax,
                ));
                drop(invocation);
                assert_eq!(
                    result,
                    Poll::Ready(Err(Refused::Unavailable(if annotation {
                        "start_annotation"
                    } else {
                        "start_type_expression"
                    })))
                );
                let states = effects.journal.scope_states.borrow();
                let [(_, _, active)] = states.as_slice() else {
                    anyhow::bail!("missing stub annotation state");
                };
                assert_deferred_state(
                    *active,
                    if string_state {
                        state
                    } else {
                        local::DeferredExpressionState::Deferred
                    },
                );
                assert_deferred_state(root.deferred_state, initial);
                assert_eq!(root.context.inference_flags, flags);
            }
        }
        Ok(())
    }

    #[test]
    fn annotation_completion_keeps_qualifiers_and_storage_disposition() -> anyhow::Result<()> {
        let db = database()?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let env = ProgramEnvironment::from_file(file);
        for scoped in [false, true] {
            for already_stored in [false, true] {
                let (mut root, call, _) = builder(&db, &module, &env)?;
                root.deferred_state = local::DeferredExpressionState::InStringAnnotation(
                    local::NodeKey::from_node(call),
                );
                root.context
                    .inference_flags
                    .insert(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT);
                let deferred = root.deferred_state;
                let flags = root.context.inference_flags;
                let annotation_ty = local::TypeAndQualifiers::declared(Type::unknown())
                    .with_qualifier(local::TypeQualifiers::FINAL);
                let syntax_storage = local::string_annotation::OrdinaryStorage::new();
                let syntax = &syntax_storage;
                let mut invocation = LocalInvocation::new(&mut root);
                let saved = if scoped {
                    let Ok(scope) = OrdinaryLocalEffects::default().prepare_annotation_scope(
                        &invocation.builders,
                        BuilderId::ROOT,
                        local::DeferredExpressionState::Deferred,
                    );
                    let Ok(()) = OrdinaryLocalEffects::default().enter_annotation_scope(
                        &mut invocation.builders,
                        BuilderId::ROOT,
                        scope,
                    );
                    Some(scope)
                } else {
                    None
                };
                let storage = if already_stored {
                    invocation
                        .builders
                        .get_mut(BuilderId::ROOT)
                        .store_expression_type(&call.func, Type::Never);
                    local::AnnotationStorage::AlreadyStored
                } else {
                    local::AnnotationStorage::Store {
                        expression_ty: Type::Never,
                    }
                };
                let effects = Effects {
                    endpoint: None,
                    refuse_push: false,
                    journal: Rc::new(Journal::default()),
                    argument_control: None,
                };
                effects.journal.complete_annotation.set(true);
                let result = try_poll_immediate(local::drive(
                    &mut Some(Work::FinishAnnotation(
                        local::AnnotationRoot {
                            builder: BuilderId::ROOT,
                            annotation: &call.func,
                            saved,
                        },
                        local::AnnotationExpressionInference {
                            annotation_ty,
                            storage,
                        },
                    )),
                    &mut invocation,
                    LocalFacts,
                    &effects,
                    syntax,
                ));
                drop(invocation);
                assert_eq!(
                    result,
                    Poll::Ready(Ok(Some(local::LocalResult::Annotation(annotation_ty))))
                );
                let states = effects.journal.scope_states.borrow();
                if already_stored {
                    assert!(states.is_empty());
                } else {
                    let [("store", store_flags, store_deferred)] = states.as_slice() else {
                        anyhow::bail!("annotation was not stored exactly once");
                    };
                    let mut expected_flags = flags;
                    if scoped {
                        expected_flags.insert(InferenceFlags::CHECK_UNBOUND_TYPEVARS);
                    }
                    assert_eq!(*store_flags, expected_flags);
                    assert_deferred_state(*store_deferred, deferred);
                }
                assert_eq!(root.context.inference_flags, flags);
                assert_deferred_state(root.deferred_state, deferred);
                assert_eq!(root.expressions.len(), 1);
                assert_eq!(root.try_expression_type(&call.func), Some(Type::Never));
                assert_eq!(
                    root.qualifiers
                        .get(&local::ExpressionNodeKey::from(&call.func))
                        .copied(),
                    if already_stored {
                        None
                    } else {
                        Some(local::TypeQualifiers::FINAL)
                    }
                );
            }
        }
        Ok(())
    }

    #[test]
    fn annotation_finish_refusals_restore_the_root() -> anyhow::Result<()> {
        let db = database()?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let env = ProgramEnvironment::from_file(file);
        for annotation in [false, true] {
            let (mut root, call, _) = builder(&db, &module, &env)?;
            root.deferred_state = local::DeferredExpressionState::Deferred;
            root.context
                .inference_flags
                .insert(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT);
            let deferred = root.deferred_state;
            let flags = root.context.inference_flags;
            let syntax_storage = local::string_annotation::OrdinaryStorage::new();
            let syntax = &syntax_storage;
            let mut invocation = LocalInvocation::new(&mut root);
            let (work, refusal) = if annotation {
                let Ok(saved) = OrdinaryLocalEffects::default().prepare_annotation_scope(
                    &invocation.builders,
                    BuilderId::ROOT,
                    local::DeferredExpressionState::None,
                );
                let Ok(()) = OrdinaryLocalEffects::default().enter_annotation_scope(
                    &mut invocation.builders,
                    BuilderId::ROOT,
                    saved,
                );
                (
                    Work::FinishAnnotation(
                        local::AnnotationRoot {
                            builder: BuilderId::ROOT,
                            annotation: &call.func,
                            saved: Some(saved),
                        },
                        local::AnnotationExpressionInference {
                            annotation_ty: local::TypeAndQualifiers::declared(Type::unknown()),
                            storage: local::AnnotationStorage::Store {
                                expression_ty: Type::unknown(),
                            },
                        },
                    ),
                    "store_type_expression",
                )
            } else {
                let Ok(Some(scope)) = OrdinaryLocalEffects::default().prepare_type_expression_scope(
                    &invocation.builders,
                    BuilderId::ROOT,
                    local::TypeExpressionMode::ScopedWithState(
                        local::DeferredExpressionState::None,
                    ),
                ) else {
                    anyhow::bail!("missing type-expression scope");
                };
                invocation.frames.push(Frame::TypeExpressionFinish(
                    BuilderId::ROOT,
                    &call.func,
                    scope,
                ));
                let Ok(()) = OrdinaryLocalEffects::default().enter_type_expression_scope(
                    &mut invocation.builders,
                    BuilderId::ROOT,
                    scope,
                );
                (Work::Return(Type::unknown()), "store_type_expression")
            };
            let effects = Effects {
                endpoint: None,
                refuse_push: false,
                journal: Rc::new(Journal::default()),
                argument_control: None,
            };
            effects.journal.refuse_store.set(true);
            assert_eq!(
                try_poll_immediate(local::drive(
                    &mut Some(work),
                    &mut invocation,
                    LocalFacts,
                    &effects,
                    syntax,
                )),
                Poll::Ready(Err(Refused::Unavailable(refusal)))
            );
            drop(invocation);
            assert_eq!(root.context.inference_flags, flags);
            assert_deferred_state(root.deferred_state, deferred);
            assert!(root.expressions.is_empty());
            assert!(root.qualifiers.is_empty());
        }
        Ok(())
    }

    #[test]
    fn type_expression_storage_restores_inner_then_outer_state() -> anyhow::Result<()> {
        let db = database()?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let env = ProgramEnvironment::from_file(file);
        for string_state in [false, true] {
            for explicit_state in [false, true] {
                for refuse_after_store in [false, true] {
                    let (mut root, call, _) = builder(&db, &module, &env)?;
                    root.deferred_state = if string_state {
                        local::DeferredExpressionState::InStringAnnotation(
                            local::NodeKey::from_node(call),
                        )
                    } else {
                        local::DeferredExpressionState::Deferred
                    };
                    root.context.inference_flags.insert(
                        InferenceFlags::IN_TYPE_EXPRESSION
                            | InferenceFlags::IN_UNPACK_TYPE_ARGUMENT
                            | InferenceFlags::CHECK_UNBOUND_TYPEVARS,
                    );
                    root.context
                        .inference_flags
                        .remove(InferenceFlags::IN_NESTED_TYPE_EXPRESSION);
                    let deferred = root.deferred_state;
                    let flags = root.context.inference_flags;
                    let mode = if explicit_state {
                        local::TypeExpressionMode::ScopedWithState(
                            local::DeferredExpressionState::None,
                        )
                    } else {
                        local::TypeExpressionMode::Scoped
                    };
                    let syntax_storage = local::string_annotation::OrdinaryStorage::new();
                    let syntax = &syntax_storage;
                    let mut invocation = LocalInvocation::new(&mut root);
                    let Ok(Some(scope)) = OrdinaryLocalEffects::default().prepare_type_expression_scope(
                        &invocation.builders,
                        BuilderId::ROOT,
                        mode,
                    ) else {
                        anyhow::bail!("missing type-expression scope");
                    };
                    invocation.frames.push(Frame::TypeExpressionFinish(
                        BuilderId::ROOT,
                        &call.func,
                        scope,
                    ));
                    let Ok(()) = OrdinaryLocalEffects::default().enter_type_expression_scope(
                        &mut invocation.builders,
                        BuilderId::ROOT,
                        scope,
                    );
                    let effects = Effects {
                        endpoint: None,
                        refuse_push: false,
                        journal: Rc::new(Journal::default()),
                        argument_control: None,
                    };
                    effects.journal.complete_annotation.set(true);
                    effects.journal.refuse_after_store.set(refuse_after_store);
                    let result = try_poll_immediate(local::drive(
                        &mut Some(Work::Return(Type::Never)),
                        &mut invocation,
                        LocalFacts,
                        &effects,
                        syntax,
                    ));
                    drop(invocation);
                    assert_eq!(
                        result,
                        Poll::Ready(if refuse_after_store {
                            Err(Refused::Unavailable("restore_type_expression_after_store"))
                        } else {
                            Ok(Some(local::LocalResult::Type(Type::Never)))
                        })
                    );
                    let states = effects.journal.scope_states.borrow();
                    let [("store", store_flags, store_deferred)] = states.as_slice() else {
                        anyhow::bail!("type expression was not stored exactly once");
                    };
                    assert_eq!(*store_flags, flags);
                    assert_deferred_state(
                        *store_deferred,
                        if explicit_state && !string_state {
                            local::DeferredExpressionState::None
                        } else {
                            deferred
                        },
                    );
                    assert_eq!(root.context.inference_flags, flags);
                    assert_deferred_state(root.deferred_state, deferred);
                    assert_eq!(root.try_expression_type(&call.func), Some(Type::Never));
                    assert!(root.qualifiers.is_empty());
                }
            }
        }
        Ok(())
    }

    struct Observed<F> {
        future: Option<Pin<Box<F>>>,
        journal: Rc<Journal>,
    }
    impl<F: Future> Future for Observed<F> {
        type Output = F::Output;
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = self.get_mut();
            let Some(future) = this.future.as_mut() else {
                return Poll::Pending;
            };
            let result = future.as_mut().poll(cx);
            if result.is_pending() && this.journal.children.get() > 0 {
                this.journal
                    .pending_with_child
                    .set(this.journal.pending_with_child.get() + 1);
            }
            result
        }
    }
    impl<F> Drop for Observed<F> {
        fn drop(&mut self) {
            assert_eq!(self.journal.children.get(), 0);
            drop(self.future.take());
            self.journal.drops.borrow_mut().push("drive");
        }
    }

    struct RootOwner<'db, 'ast> {
        builder: TypeInferenceBuilder<'db, 'ast>,
        binding: Definition<'db>,
        flags: InferenceFlags,
        cache: Weak<RefCell<ExpressionCache<'db>>>,
        journal: Rc<Journal>,
    }
    impl Drop for RootOwner<'_, '_> {
        fn drop(&mut self) {
            assert_eq!(self.journal.children.get(), 0);
            assert_eq!(self.builder.typevar_binding_context, Some(self.binding));
            assert_eq!(self.builder.context.inference_flags, self.flags);
            assert!(self.builder.expression_cache.as_ref().is_some_and(|cache| {
                Weak::ptr_eq(&self.cache, &Rc::downgrade(cache)) && Rc::strong_count(cache) == 1
            }));
            assert!(self.builder.expressions.is_empty());
            self.journal.drops.borrow_mut().push("root");
        }
    }

    struct Admission;
    impl ExecutionAdmission for Admission {
        fn admit(&self, _: ExecutionWork) -> RunResult<()> {
            Ok(())
        }
    }

    #[test]
    fn real_canonical_child_retires_before_actual_drive_and_root() -> anyhow::Result<()> {
        let db = database()?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let env = ProgramEnvironment::from_file(file);
        let Some(ast::Stmt::Expr(statement)) = module.suite().as_slice().last() else {
            anyhow::bail!("missing cleanup expression");
        };
        let outer_expression = statement.value.as_ref();
        for cached_call in [false, true] {
            let (mut root, call, binding) = builder(&db, &module, &env)?;
            root.setup_expression_cache();
            let journal = Rc::new(Journal::default());
            let owner = RootOwner {
                flags: root.context.inference_flags,
                cache: root
                    .expression_cache
                    .as_ref()
                    .map(Rc::downgrade)
                    .ok_or_else(|| anyhow::anyhow!("missing root cache"))?,
                builder: root,
                binding,
                journal: journal.clone(),
            };
            let observed = journal.clone();
            let admission = Admission;
            let outcome = try_with_attempt(&db, 100_000, || {
                RegistryBuilder::new(&db, &admission)?
                    .seal()?
                    .run(move |endpoint| async move {
                        let mut owner = owner;
                        let effects = Effects {
                            endpoint: Some(&endpoint),
                            refuse_push: false,
                            journal: observed.clone(),
                            argument_control: None,
                        };
                        let work = if cached_call {
                            Work::Expression(
                                BuilderId::ROOT,
                                outer_expression,
                                TypeContext::default(),
                                ExpressionMode::Cached,
                            )
                        } else {
                            Work::Callee(BuilderId::ROOT, &call.func, local::CalleeContinuation::Return)
                        };
                        let syntax_storage = local::string_annotation::OrdinaryStorage::new();
                        let syntax = &syntax_storage;
                        let mut invocation = LocalInvocation::new(&mut owner.builder);
                        let result = Observed {
                            future: Some(Box::pin(local::drive(
                                &mut Some(work),
                                &mut invocation,
                                LocalFacts,
                                &effects,
                                syntax,
                            ))),
                            journal: observed.clone(),
                        }
                        .await;
                        drop(invocation);
                        observed.resumed.set(true);
                        result.map_err(|_| RunError::Contract("unexpected local return"))
                    })
            });
            assert!(
                matches!(
                    outcome,
                    Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
                ),
                "{outcome:?}"
            );
            assert!(journal.pending_with_child.get() > 0);
            assert!(!journal.resumed.get());
            assert_eq!(journal.children.get(), 0);
            assert_eq!(journal.speculative_at_child.get(), usize::from(cached_call));
            assert_eq!(
                *journal.drops.borrow(),
                ["child", "canonical borrow", "drive", "root"]
            );
        }
        Ok(())
    }
}

mod argument_interruption {
    use std::cell::RefCell;
    use std::rc::{Rc, Weak};
    use std::task::Poll;

    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::{ParsedModuleRef, parsed_module};
    use ty_python_core::{global_scope, semantic_index};

    use super::super::{
        self as local, BuilderId, BuilderStore, LocalFacts, LocalInvocation, LocalOwners,
        ObservedOwnerPhase, OwnedState, OwnershipEvent, Work, arguments, call,
    };
    use super::cleanup::{ArgumentBoundary as Boundary, ArgumentControl, Effects, Refused};
    use crate::db::tests::{TestDb, TestDbBuilder};
    use crate::types::infer::builder::{
        ArgumentsIter, CallErrorKind, Definition, InferenceFlags, InferenceRegion, PythonVersion,
        TypeContext, TypeInferenceBuilder, ast,
    };
    use crate::types::signatures::effects::try_poll_immediate;
    use crate::{Db, ProgramEnvironment};

    pub(super) const PATH: &str = "/src/argument_interruption.py";
    const IDENTITY: &str = "def identity[T](value: T, /) -> T: ...\n";
    const CHOOSE: &str = "from typing import overload\n@overload\ndef choose(value: int, /) -> int: ...\n@overload\ndef choose(value: None, /) -> None: ...\n";

    pub(super) fn database(boundary: Boundary, literal: &str) -> anyhow::Result<TestDb> {
        let (name, stub) = match boundary {
            Boundary::NarrowCache => ("identity", IDENTITY),
            Boundary::Committed | Boundary::Contextual => ("choose", CHOOSE),
        };
        let call = match boundary {
            Boundary::NarrowCache => format!("{name}({name}({literal}))"),
            Boundary::Committed | Boundary::Contextual => format!("{name}({literal})"),
        };
        let source =
            format!("from ops import {name}\ndef marker(): ...\nresult: int | None = {call}\n");
        TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file("/src/ops.pyi", stub)
            .with_file(PATH, &source)
            .build()
    }

    // These owners are obtained through ordinary inference. This is intentionally semantic
    // preparation for a cleanup control, rather than preparation for a cold controlled run.
    pub(super) fn prepare<'db, 'ast>(
        db: &'db TestDb,
        module: &'ast ParsedModuleRef,
        env: &'ast ProgramEnvironment<'db>,
    ) -> anyhow::Result<(
        TypeInferenceBuilder<'db, 'ast>,
        call::CallData<'db, 'ast>,
        arguments::OwnedArguments<'ast, 'db>,
        Definition<'db>,
    )> {
        let file = db.program_file(system_path_to_file(db, PATH)?);
        let index = semantic_index(db, file);
        let [
            ast::Stmt::ImportFrom(_),
            ast::Stmt::FunctionDef(marker),
            ast::Stmt::AnnAssign(assignment),
        ] = module.suite().as_slice()
        else {
            anyhow::bail!("unexpected argument-control fixture");
        };
        let Some(ast::Expr::Call(expression)) = assignment.value.as_deref() else {
            anyhow::bail!("missing annotated call");
        };
        let binding = index
            .try_definition(marker)
            .ok_or_else(|| anyhow::anyhow!("missing marker definition"))?;
        let mut root = TypeInferenceBuilder::new(
            db,
            env,
            InferenceRegion::Scope(global_scope(db, file), TypeContext::default()),
            file.file(db),
            file,
            index,
            module,
        );
        root.context.defuse();
        let tcx = TypeContext::new(Some(root.infer_type_expression(&assignment.annotation)));
        let callable = root.infer_callee(&expression.func);
        let Ok(start) = call::start_sync(
            &mut root,
            expression,
            callable,
            tcx,
            call::CallFacts,
            &call::OrdinaryCallEffects,
        );
        let call::Start::Prepare(data) = start else {
            anyhow::bail!("fixture did not enter ordinary argument preparation");
        };
        let arguments = root.prepare_call_arguments(&expression.arguments);
        let Ok(prepared) = call::prepared_sync(
            &mut root,
            data,
            arguments,
            call::CallFacts,
            &call::OrdinaryCallEffects,
        );
        let call::Prepared::Arguments(data, storage, None) = prepared else {
            anyhow::bail!("fixture did not produce actual matched bindings");
        };
        root.typevar_binding_context = Some(binding);
        root.context
            .inference_flags
            .set(InferenceFlags::CHECK_UNBOUND_TYPEVARS, false);
        root.context
            .inference_flags
            .set(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, false);
        anyhow::ensure!(root.expression_cache.is_none());
        Ok((root, data, storage, binding))
    }

    fn control(
        boundary: Boundary,
        literal: &str,
        semantic_error: bool,
        preexisting: bool,
        suspend: bool,
        refuse_push: bool,
    ) -> anyhow::Result<()> {
        let db = database(boundary, literal)?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let env = ProgramEnvironment::from_file(file);
        let (mut root, data, storage, binding) = prepare(&db, &module, &env)?;
        if preexisting {
            root.setup_expression_cache();
        }
        let initial_flags = root.context.inference_flags;
        let initial_cache = root.expression_cache.as_ref().map(Rc::downgrade);
        let mut builders = BuilderStore::new(&mut root);
        let mut state: OwnedState<'_, '_> = arguments::State::new(
            BuilderId::ROOT,
            ArgumentsIter::from_ast(&data.call.arguments),
            storage,
            arguments::ArgumentPolicy::Ordinary,
            data.tcx,
        );
        let mut steps = 0;
        let mut ordinary_requests = 0;
        let mut predecessor = None;
        let (state, observation, request) = loop {
            steps += 1;
            anyhow::ensure!(steps <= 256, "fixture did not reach {boundary:?}");
            if let Some(observed) = state.observe() {
                if matches!(
                    observed.kind,
                    arguments::ObservedPass::SimpleSpeculative | arguments::ObservedPass::Unified
                ) {
                    predecessor = Some((
                        observed.pass,
                        std::ptr::from_ref(builders.builder(observed.pass)),
                    ));
                }
                if let Some(request) = state.prepared_request() {
                    let reached = match boundary {
                        Boundary::NarrowCache => {
                            observed.narrow.is_some()
                                && request == arguments::PreparedRequest::Unique
                        }
                        Boundary::Committed => {
                            observed.predecessor.is_some()
                                && request == arguments::PreparedRequest::Unique
                        }
                        Boundary::Contextual => request == arguments::PreparedRequest::Contextual,
                    };
                    if reached {
                        break (state, observed, request);
                    }
                }
            }
            match arguments::advance(state, &mut builders) {
                arguments::Action::Continue(next) => state = next,
                arguments::Action::Complete { result, .. } => {
                    anyhow::bail!("fixture completed before {boundary:?}: {result:?}");
                }
                arguments::Action::Infer {
                    pending,
                    builder,
                    argument,
                    policy,
                } => {
                    anyhow::ensure!(matches!(policy, arguments::ArgumentPolicy::Ordinary));
                    let (_, expression, tcx) = argument;
                    let ty = builders.get_mut(builder).infer_expression(expression, tcx);
                    ordinary_requests += 1;
                    state = arguments::resume(pending, ty, &mut builders);
                }
            }
        };
        assert_eq!(observation.input, BuilderId::ROOT);
        assert!(observation.contextual_child.is_none());
        let installed_cache = Rc::downgrade(
            builders
                .builder(BuilderId::ROOT)
                .expression_cache
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("argument strategy did not install its cache"))?,
        );
        match boundary {
            Boundary::NarrowCache => {
                let narrow = observation
                    .narrow
                    .ok_or_else(|| anyhow::anyhow!("narrow boundary was not reached"))?;
                assert_ne!(narrow, BuilderId::ROOT);
                assert_ne!(observation.pass, narrow);
                assert_eq!(observation.active, narrow);
                assert_eq!(observation.kind, arguments::ObservedPass::Unified);
                assert_eq!(builders.speculative.len(), 2);
                assert_eq!(ordinary_requests, 0);
                assert!(observation.semantic_result.is_none());
            }
            Boundary::Committed => {
                assert!(observation.narrow.is_none());
                assert_eq!(observation.active, BuilderId::ROOT);
                assert_eq!(observation.kind, arguments::ObservedPass::SimpleCommitted);
                assert_eq!(observation.pass, BuilderId::ROOT);
                assert!(
                    ordinary_requests > 0,
                    "committed control requires ordinary speculative inference"
                );
                let retained = observation
                    .predecessor
                    .ok_or_else(|| anyhow::anyhow!("committed pass has no predecessor"))?;
                let (previous, pointer) = predecessor
                    .ok_or_else(|| anyhow::anyhow!("speculative pass was not observed"))?;
                assert_eq!(retained, previous);
                assert_eq!(std::ptr::from_ref(builders.builder(retained)), pointer);
                assert_eq!(builders.speculative.len(), 1);
                let semantic_result = observation
                    .semantic_result
                    .ok_or_else(|| anyhow::anyhow!("ordinary binding check did not complete"))?;
                assert_eq!(
                    semantic_result,
                    if semantic_error {
                        Err(CallErrorKind::BindingError)
                    } else {
                        Ok(())
                    }
                );
            }
            Boundary::Contextual => {
                assert!(observation.narrow.is_none());
                assert!(observation.predecessor.is_none());
                assert!(observation.semantic_result.is_none());
                assert_eq!(observation.active, BuilderId::ROOT);
                assert_eq!(observation.kind, arguments::ObservedPass::SimpleSpeculative);
                assert_ne!(observation.pass, BuilderId::ROOT);
                assert_eq!(builders.speculative.len(), 1);
                assert_eq!(ordinary_requests, 0);
            }
        }
        let mut control = ArgumentControl::new(boundary, observation, request, suspend);
        control.refuse_argument_push = refuse_push;
        let control = Rc::new(control);
        let effects = Effects::argument_control(control.clone());
        let events = Rc::new(RefCell::new(Vec::new()));
        let syntax_storage = local::string_annotation::OrdinaryStorage::new();
        let syntax = &syntax_storage;
        let mut invocation = LocalInvocation {
            builders,
            owners: LocalOwners::default(),
            frames: Vec::new(),
            annotations: Vec::new(),
        };
        let observed = events.clone();
        invocation.observe_ownership(Rc::new(move |event| observed.borrow_mut().push(event)));
        let owner = invocation.owners.push(BuilderId::ROOT, data, state, None);
        let result = try_poll_immediate(local::drive(
            &mut Some(Work::Arguments(owner)),
            &mut invocation,
            LocalFacts,
            &effects,
            syntax,
        ));
        drop(invocation);
        assert_eq!(
            result,
            if refuse_push {
                Poll::Ready(Err(Refused::Push))
            } else if suspend {
                Poll::Pending
            } else {
                Poll::Ready(Err(Refused::ArgumentBoundary))
            }
        );
        assert_eq!(
            control.reached.get(),
            !refuse_push,
            "driver reached the wrong argument boundary"
        );
        assert_eq!(control.argument_steps.get(), 1);
        assert_eq!(control.argument_frames.get(), usize::from(!refuse_push));
        assert_eq!(control.attempted_argument_frames.get(), 1);
        assert!(!control.completed.get());
        assert!(!control.resumed.get());
        let pending = control.pending.borrow();
        let pending = pending
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("driver did not produce a pending argument"))?;
        if boundary == Boundary::Contextual {
            assert!(pending.contextual_child.is_some());
            assert_ne!(pending.contextual_child, Some(pending.pass));
            assert_eq!(control.cache_child.get().is_some(), !refuse_push);
            assert_ne!(pending.contextual_child, control.cache_child.get());
        }
        assert_eq!(root.typevar_binding_context, Some(binding));
        assert_eq!(root.context.inference_flags, initial_flags);
        match (initial_cache, root.expression_cache.as_ref()) {
            (Some(initial), Some(cache)) => {
                assert!(Weak::ptr_eq(&initial, &Rc::downgrade(cache)));
                assert_eq!(Rc::strong_count(cache), 1);
            }
            (None, None) => assert!(installed_cache.upgrade().is_none()),
            _ => anyhow::bail!("argument interruption changed root cache ownership"),
        }
        let events = events.borrow();
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, OwnershipEvent::PayloadRetired { .. }))
                .count(),
            1
        );
        let pending_installed = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    OwnershipEvent::Installed {
                        phase: ObservedOwnerPhase::Pending,
                        ..
                    }
                )
            })
            .ok_or_else(|| anyhow::anyhow!("pending payload was not installed"))?;
        let retired = events
            .iter()
            .position(|event| *event == OwnershipEvent::PayloadRetired { identity: 0 })
            .ok_or_else(|| anyhow::anyhow!("payload did not retire"))?;
        assert!(pending_installed < retired);
        assert!(
            events[retired + 1..]
                .iter()
                .any(|event| matches!(event, OwnershipEvent::SpeculativeRetired(_)))
        );
        assert!(!events[..retired].iter().any(|event| matches!(
            event,
            OwnershipEvent::SpeculativeRetired(_) | OwnershipEvent::RootRestored
        )));
        assert_eq!(events.last(), Some(&OwnershipEvent::RootRestored));
        println!(
            "ARGUMENT_INTERRUPTION boundary={boundary:?} literal={literal} preexisting={preexisting} suspend={suspend} refuse_push={refuse_push} semantic_error={semantic_error} steps={steps} ordinary_requests={ordinary_requests}"
        );
        Ok(())
    }

    #[test]
    fn narrow_argument_trial_retires_with_a_real_nested_cache_miss() -> anyhow::Result<()> {
        for preexisting in [false, true] {
            for suspend in [false, true] {
                control(
                    Boundary::NarrowCache,
                    "1",
                    false,
                    preexisting,
                    suspend,
                    false,
                )?;
            }
        }
        Ok(())
    }

    #[test]
    fn committed_argument_pass_retires_its_retained_speculative_predecessor() -> anyhow::Result<()>
    {
        for preexisting in [false, true] {
            for suspend in [false, true] {
                control(Boundary::Committed, "1", false, preexisting, suspend, false)?;
                control(
                    Boundary::Committed,
                    "1.0",
                    true,
                    preexisting,
                    suspend,
                    false,
                )?;
            }
        }
        Ok(())
    }

    #[test]
    fn contextual_argument_pending_retires_its_real_child_before_resume() -> anyhow::Result<()> {
        for preexisting in [false, true] {
            for suspend in [false, true] {
                control(
                    Boundary::Contextual,
                    "1",
                    false,
                    preexisting,
                    suspend,
                    false,
                )?;
            }
        }
        Ok(())
    }
    #[test]
    fn pending_argument_is_owned_before_its_continuation_push() -> anyhow::Result<()> {
        for boundary in [Boundary::NarrowCache, Boundary::Contextual] {
            for preexisting in [false, true] {
                control(boundary, "1", false, preexisting, false, true)?;
            }
        }
        Ok(())
    }
}

mod argument_storage {
    use std::cell::RefCell;
    use std::mem::size_of;
    use std::rc::Rc;

    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_text_size::{Ranged, TextRange};
    use test_case::test_case;
    use ty_python_core::{global_scope, semantic_index};

    use super::super::{
        self as local, ActiveArgument, ActiveSpecialization, ArgumentStep, BuilderId,
        CompletedArgument, CompletedArgumentLease, CompletedSpecialization, ExpressionMode,
        FinishedOwner, Frame, LocalFacts, LocalInvocation, ObservedOwnerPhase,
        OrdinaryLocalEffects, OwnedPending, OwnedState, OwnerKind, OwnershipEvent, PendingArgument,
        PendingSpecialization, PreparedCall, SpecializationStep, Work,
    };
    use super::{ExpressionNodeKey, OPS};
    use crate::db::tests::TestDbBuilder;
    use crate::types::infer::builder::{
        GenericContext, InferenceFlags, InferenceRegion, PythonVersion, Type, TypeContext,
        TypeInferenceBuilder, ast,
    };
    use crate::{Db, ProgramEnvironment};

    struct LiveOwner {
        identity: usize,
        call: TextRange,
        phase: Option<ObservedOwnerPhase>,
        taken_from: Option<ObservedOwnerPhase>,
        payload_retired: bool,
    }

    // The trace follows actual payload identities: a vector move can change their addresses,
    // and a later sibling can reuse a slot only after its previous payload has finished.
    fn assert_nested_lifetimes(
        events: &[OwnershipEvent],
        outer: TextRange,
        children: [TextRange; 2],
    ) -> anyhow::Result<()> {
        let mut live: Vec<LiveOwner> = Vec::new();
        let mut created = Vec::new();
        let mut retired = Vec::new();
        let mut parent_identity = None;
        let mut awaiting_resume = None;
        let mut resumed_after_children = Vec::new();
        let mut initial_capacity = None;
        let mut maximum_capacity = 0;
        let mut maximum_live = 0;
        let mut completed = false;

        for event in events {
            anyhow::ensure!(
                !completed,
                "ownership event after invocation completion: {event:?}"
            );
            match *event {
                OwnershipEvent::Created {
                    kind: _,
                    index,
                    identity,
                    call,
                    len,
                    capacity,
                } => {
                    assert_eq!(index, live.len());
                    assert_eq!(len, live.len() + 1);
                    assert!(!created.iter().any(|(_, previous, _)| *previous == identity));
                    assert!(
                        awaiting_resume.is_none(),
                        "child created before parent resumed"
                    );
                    if let Some(parent) = live.last() {
                        assert_eq!(live.len(), 1, "fixture created an unexpected call depth");
                        assert_eq!(parent.call, outer);
                        assert_eq!(Some(parent.identity), parent_identity);
                        assert_eq!(parent.phase, Some(ObservedOwnerPhase::Pending));
                        assert!(!parent.payload_retired);
                        assert!(children.contains(&call), "unexpected nested call");
                        assert!(capacity > initial_capacity.unwrap_or_default());
                    } else {
                        assert_eq!(call, outer);
                        assert!(parent_identity.replace(identity).is_none());
                        initial_capacity = Some(capacity);
                        assert_eq!(capacity, 1);
                    }
                    created.push((call, identity, index));
                    live.push(LiveOwner {
                        identity,
                        call,
                        phase: Some(ObservedOwnerPhase::Active),
                        taken_from: None,
                        payload_retired: false,
                    });
                    maximum_live = maximum_live.max(len);
                    maximum_capacity = maximum_capacity.max(capacity);
                }
                OwnershipEvent::Taken { index, identity } => {
                    assert_eq!(index + 1, live.len());
                    let owner = live
                        .last_mut()
                        .ok_or_else(|| anyhow::anyhow!("take without an owner"))?;
                    assert_eq!(owner.identity, identity);
                    assert!(!owner.payload_retired);
                    anyhow::ensure!(owner.phase.is_some(), "second take of a taken payload");
                    owner.taken_from = owner.phase.take();
                }
                OwnershipEvent::Installed {
                    index,
                    identity,
                    phase,
                } => {
                    assert_eq!(index + 1, live.len());
                    let owner = live
                        .last_mut()
                        .ok_or_else(|| anyhow::anyhow!("installation without an owner"))?;
                    assert_eq!(owner.identity, identity);
                    assert!(owner.phase.is_none());
                    assert!(!owner.payload_retired);
                    let before = owner
                        .taken_from
                        .take()
                        .ok_or_else(|| anyhow::anyhow!("installation without a take"))?;
                    if before == ObservedOwnerPhase::Pending {
                        assert_eq!(phase, ObservedOwnerPhase::Active);
                        if let Some((parent, child)) = awaiting_resume.take() {
                            assert_eq!(owner.identity, parent);
                            assert_eq!(owner.call, outer);
                            resumed_after_children.push(child);
                        }
                    } else {
                        assert_eq!(before, ObservedOwnerPhase::Active);
                    }
                    owner.phase = Some(phase);
                }
                OwnershipEvent::PayloadRetired { identity } => {
                    let owner = live
                        .last_mut()
                        .ok_or_else(|| anyhow::anyhow!("payload retired without an owner"))?;
                    assert_eq!(owner.identity, identity);
                    assert!(owner.phase.is_none());
                    assert_eq!(owner.taken_from, Some(ObservedOwnerPhase::Completed));
                    assert!(!std::mem::replace(&mut owner.payload_retired, true));
                }
                OwnershipEvent::SlotRetired {
                    index,
                    len,
                    capacity,
                } => {
                    assert_eq!(index + 1, live.len());
                    let owner = live
                        .pop()
                        .ok_or_else(|| anyhow::anyhow!("slot retired without an owner"))?;
                    assert_eq!(len, live.len());
                    assert!(owner.payload_retired);
                    assert_eq!(owner.taken_from, Some(ObservedOwnerPhase::Completed));
                    assert!(owner.phase.is_none());
                    assert_eq!(capacity, maximum_capacity);
                    retired.push((owner.call, owner.identity, index));
                    if let Some(parent) = live.last() {
                        assert_eq!(len, 1);
                        assert_eq!(parent.phase, Some(ObservedOwnerPhase::Pending));
                        assert_eq!(Some(parent.identity), parent_identity);
                        assert!(
                            awaiting_resume
                                .replace((parent.identity, owner.call))
                                .is_none()
                        );
                    } else {
                        assert_eq!(owner.call, outer);
                        assert!(awaiting_resume.is_none());
                    }
                }
                OwnershipEvent::InvocationCompleted {
                    arguments,
                    frames,
                    speculative,
                    capacity,
                } => {
                    assert!(live.is_empty());
                    assert!(awaiting_resume.is_none());
                    assert_eq!((arguments, frames, speculative), (0, 0, 0));
                    assert_eq!(capacity, maximum_capacity);
                    completed = true;
                }
                OwnershipEvent::SpeculativeRetired(_) | OwnershipEvent::RootRestored => {
                    anyhow::bail!("ordinary completion used abort cleanup: {event:?}");
                }
            }
        }

        assert!(completed, "ordinary driver did not complete its invocation");
        assert_eq!(maximum_live, 2);
        assert!(maximum_capacity > 1);
        assert_eq!(created.len(), retired.len());
        for created_owner in &created {
            assert_eq!(
                retired
                    .iter()
                    .filter(|retired_owner| *retired_owner == created_owner)
                    .count(),
                1
            );
        }
        let child_creations: Vec<_> = created
            .iter()
            .filter(|(call, _, _)| children.contains(call))
            .collect();
        assert_eq!(
            child_creations.len(),
            2,
            "fixture did not create both sibling owners once"
        );
        assert_eq!(child_creations[0].0, children[0]);
        assert_eq!(child_creations[1].0, children[1]);
        assert_eq!(child_creations[0].2, child_creations[1].2);
        assert_eq!(child_creations[0].2, 1);
        assert_ne!(child_creations[0].1, child_creations[1].1);
        assert_eq!(resumed_after_children, children);
        println!(
            "ARGUMENT_STORAGE_LIFETIMES created={} retired={} maximum_live={maximum_live} initial_capacity={} maximum_capacity={maximum_capacity} final_live={}",
            created.len(),
            retired.len(),
            initial_capacity.unwrap_or_default(),
            live.len(),
        );
        Ok(())
    }

    #[test]
    fn ordinary_nested_owners_grow_resume_and_reuse_sibling_slot() -> anyhow::Result<()> {
        const PATH: &str = "/src/argument_storage.py";
        let db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file(
                "/src/ops.pyi",
                &format!("{OPS}def combine(left: int, right: int, /) -> int: ...\n"),
            )
            .with_file(
                PATH,
                "from ops import choose, combine\ncombine(choose(1), choose(1))\n",
            )
            .build()?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let index = semantic_index(&db, file);
        let env = ProgramEnvironment::from_file(file);
        let [ast::Stmt::ImportFrom(_), ast::Stmt::Expr(statement)] = module.suite().as_slice()
        else {
            anyhow::bail!("unexpected storage fixture statements");
        };
        let ast::Expr::Call(outer) = statement.value.as_ref() else {
            anyhow::bail!("missing combine call");
        };
        let [ast::Expr::Call(left), ast::Expr::Call(right)] = outer.arguments.args.as_ref() else {
            anyhow::bail!("missing choose sibling calls");
        };
        assert!(index.try_expression(outer).is_some());
        assert!(index.try_expression(&outer.func).is_some());
        for child in [left, right] {
            assert!(index.try_expression(child).is_none());
            assert!(index.try_expression(&child.func).is_none());
        }

        let mut root = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Scope(global_scope(&db, file), TypeContext::default()),
            file.file(&db),
            file,
            index,
            &module,
        );
        root.context.defuse();
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = events.clone();
        let syntax_storage = local::string_annotation::OrdinaryStorage::new();
        let syntax = &syntax_storage;
        let mut invocation = LocalInvocation::new(&mut root);
        assert!(invocation.owners.slots.is_empty());
        assert_eq!(invocation.owners.slots.capacity(), 0);
        invocation.owners.slots = Vec::with_capacity(1);
        invocation.observe_ownership(Rc::new(move |event| observed.borrow_mut().push(event)));
        let Ok(result) = local::drive_sync(
            &mut Some(Work::Expression(
                BuilderId::ROOT,
                statement.value.as_ref(),
                TypeContext::default(),
                ExpressionMode::Value,
            )),
            &mut invocation,
            LocalFacts,
            &OrdinaryLocalEffects::default(),
            syntax,
        );
        drop(invocation);
        let Some(local::LocalResult::Type(ty)) = result else {
            anyhow::bail!("ordinary driver returned no type");
        };
        assert_eq!(ty.display(&db, &env).to_string(), "int");
        assert!(
            !root.context.has_diagnostics(),
            "ordinary nested calls produced diagnostics"
        );
        for child in [left, right] {
            let child_type = root
                .expressions
                .get(&ExpressionNodeKey::from(child))
                .ok_or_else(|| anyhow::anyhow!("nested call has no inferred result"))?;
            assert_eq!(child_type.display(&db, &env).to_string(), "int");
        }
        assert_nested_lifetimes(
            &events.borrow(),
            outer.range(),
            [left.range(), right.range()],
        )
    }

    fn assert_mixed_lifetimes(
        events: &[OwnershipEvent],
        expected: &[(OwnerKind, TextRange)],
    ) -> anyhow::Result<()> {
        let mut live: Vec<LiveOwner> = Vec::new();
        let mut created = Vec::new();
        let mut retired = Vec::new();
        let mut maximum_live = 0;
        let mut maximum_capacity = 0;
        let mut completed = false;
        for event in events {
            assert!(!completed, "event after completion: {event:?}");
            match *event {
                OwnershipEvent::Created {
                    kind,
                    call,
                    index,
                    identity,
                    len,
                    capacity,
                } => {
                    assert_eq!(expected.get(created.len()), Some(&(kind, call)));
                    assert_eq!(index, live.len());
                    assert_eq!(len, index + 1);
                    if let Some(parent) = live.last() {
                        assert_eq!(parent.phase, Some(ObservedOwnerPhase::Pending));
                        assert!(!parent.payload_retired);
                    } else {
                        assert_eq!(capacity, 1);
                    }
                    created.push(identity);
                    live.push(LiveOwner {
                        identity,
                        call,
                        phase: Some(ObservedOwnerPhase::Active),
                        taken_from: None,
                        payload_retired: false,
                    });
                    maximum_live = maximum_live.max(len);
                    maximum_capacity = maximum_capacity.max(capacity);
                }
                OwnershipEvent::Taken { index, identity } => {
                    assert_eq!(index + 1, live.len());
                    let owner = live
                        .last_mut()
                        .ok_or_else(|| anyhow::anyhow!("take without a mixed owner"))?;
                    assert_eq!(owner.identity, identity);
                    assert!(!owner.payload_retired);
                    assert!(owner.phase.is_some());
                    owner.taken_from = owner.phase.take();
                }
                OwnershipEvent::Installed {
                    index,
                    identity,
                    phase,
                } => {
                    assert_eq!(index + 1, live.len());
                    let owner = live
                        .last_mut()
                        .ok_or_else(|| anyhow::anyhow!("install without a mixed owner"))?;
                    assert_eq!(owner.identity, identity);
                    assert!(owner.phase.is_none());
                    assert!(!owner.payload_retired);
                    match owner.taken_from.take() {
                        Some(ObservedOwnerPhase::Pending) => {
                            assert_eq!(phase, ObservedOwnerPhase::Active);
                        }
                        Some(ObservedOwnerPhase::Active) => {}
                        _ => anyhow::bail!("invalid mixed-owner installation"),
                    }
                    owner.phase = Some(phase);
                }
                OwnershipEvent::PayloadRetired { identity } => {
                    let owner = live
                        .last_mut()
                        .ok_or_else(|| anyhow::anyhow!("retirement without a mixed owner"))?;
                    assert_eq!(owner.identity, identity);
                    assert!(owner.phase.is_none());
                    assert_eq!(owner.taken_from, Some(ObservedOwnerPhase::Completed));
                    assert!(!std::mem::replace(&mut owner.payload_retired, true));
                }
                OwnershipEvent::SlotRetired {
                    index,
                    len,
                    capacity,
                } => {
                    assert_eq!(index + 1, live.len());
                    let owner = live
                        .pop()
                        .ok_or_else(|| anyhow::anyhow!("slot retirement without a mixed owner"))?;
                    assert!(owner.payload_retired);
                    assert_eq!(len, live.len());
                    assert_eq!(capacity, maximum_capacity);
                    retired.push(owner.identity);
                }
                OwnershipEvent::InvocationCompleted {
                    arguments,
                    frames,
                    speculative,
                    capacity,
                } => {
                    assert!(live.is_empty());
                    assert_eq!((arguments, frames, speculative), (0, 0, 0));
                    assert_eq!(capacity, maximum_capacity);
                    completed = true;
                }
                OwnershipEvent::SpeculativeRetired(_) | OwnershipEvent::RootRestored => {
                    anyhow::bail!("mixed-owner completion used abort cleanup: {event:?}");
                }
            }
        }
        assert!(completed);
        assert_eq!(maximum_live, expected.len());
        assert!(maximum_capacity > 1);
        assert_eq!(created.len(), expected.len());
        assert_eq!(retired, created.into_iter().rev().collect::<Vec<_>>());
        Ok(())
    }

    fn mixed_owners(expression: &str, explicit_root: bool) -> anyhow::Result<()> {
        const PATH: &str = "/src/mixed_owner_storage.py";
        let db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file(
                "/src/ops.pyi",
                "class Leaf: ...\nclass Box[T]: ...\ndef keep(value: object, /) -> int: ...\n",
            )
            .with_file(
                PATH,
                &format!("from ops import Box, Leaf, keep\n{expression}\n"),
            )
            .build()?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let index = semantic_index(&db, file);
        let env = ProgramEnvironment::from_file(file);
        let [ast::Stmt::ImportFrom(_), ast::Stmt::Expr(statement)] = module.suite().as_slice()
        else {
            anyhow::bail!("unexpected mixed-owner fixture");
        };
        let mut expected = Vec::new();
        let call = if let ast::Expr::Subscript(outer) = statement.value.as_ref() {
            expected.push((OwnerKind::Specialization, outer.range()));
            outer.slice.as_call_expr()
        } else {
            statement.value.as_call_expr()
        }
        .ok_or_else(|| anyhow::anyhow!("missing keep call"))?;
        let [ast::Expr::Subscript(inner)] = call.arguments.args.as_ref() else {
            anyhow::bail!("missing Box specialization");
        };
        expected.extend([
            (OwnerKind::Argument, call.range()),
            (OwnerKind::Specialization, inner.range()),
        ]);
        let mut root = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Scope(global_scope(&db, file), TypeContext::default()),
            file.file(&db),
            file,
            index,
            &module,
        );
        root.context.defuse();
        let work = if explicit_root {
            let ast::Expr::Subscript(outer) = statement.value.as_ref() else {
                anyhow::bail!("missing explicit root specialization");
            };
            let value_ty = root.infer_expression(&outer.value, TypeContext::default());
            let class = value_ty
                .as_class_literal()
                .and_then(|class| class.as_static())
                .ok_or_else(|| anyhow::anyhow!("root receiver is not a static class"))?;
            Work::StartSpecialization {
                builder: BuilderId::ROOT,
                subscript: outer,
                value_ty,
                class,
                generic_context: GenericContext::from_typevar_instances(&db, &env, []),
                kind: local::ClassSpecializationKind::ClassObject,
            }
        } else {
            Work::Expression(
                BuilderId::ROOT,
                statement.value.as_ref(),
                TypeContext::default(),
                ExpressionMode::Value,
            )
        };
        let initial_flags = root.context.inference_flags;
        let initial_binding = root.typevar_binding_context;
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = events.clone();
        let syntax_storage = local::string_annotation::OrdinaryStorage::new();
        let syntax = &syntax_storage;
        let mut invocation = LocalInvocation::new(&mut root);
        invocation.owners.slots = Vec::with_capacity(1);
        invocation.observe_ownership(Rc::new(move |event| observed.borrow_mut().push(event)));
        let Ok(result) = local::drive_sync(
            &mut Some(work),
            &mut invocation,
            LocalFacts,
            &OrdinaryLocalEffects::default(),
            syntax,
        );
        drop(invocation);
        let Some(local::LocalResult::Type(ty)) = result else {
            anyhow::bail!("mixed-owner driver returned no type");
        };
        if explicit_root {
            assert_eq!(ty, Type::unknown());
        } else {
            assert_eq!(ty.display(&db, &env).to_string(), "int");
        }
        assert_eq!(root.context.has_diagnostics(), explicit_root);
        assert_eq!(root.context.inference_flags, initial_flags);
        assert_eq!(root.typevar_binding_context, initial_binding);
        let inner_ty = root
            .expressions
            .get(&ExpressionNodeKey::from(ast::ExprRef::Subscript(inner)))
            .ok_or_else(|| anyhow::anyhow!("nested specialization has no inferred type"))?;
        assert!(matches!(inner_ty, Type::GenericAlias(_)));
        assert_eq!(
            inner_ty.display(&db, &env).to_string(),
            "<class 'Box[Leaf]'>"
        );
        assert_mixed_lifetimes(&events.borrow(), &expected)
    }

    #[test]
    fn argument_and_specialization_share_the_owner_stack() -> anyhow::Result<()> {
        mixed_owners("keep(Box[Leaf])", false)
    }

    #[test]
    fn specialization_value_argument_nests_call_and_specialization_owners() -> anyhow::Result<()> {
        // An empty generic context infers the slice as a value before reporting that the
        // receiver is not generic. That value can own both call and specialization continuations.
        mixed_owners("Leaf[keep(Box[Leaf])]", true)
    }

    /// Checks that nested class-object and subclass inference retires inner owners first and
    /// restores flags, including the subclass receiver and result-store continuations.
    /// This exercises the synchronous shared driver; it does not test bounded admission.
    #[test_case(local::ClassSpecializationKind::ClassObject, false; "class_object_default_flags")]
    #[test_case(local::ClassSpecializationKind::ClassObject, true; "class_object_preexisting_flags")]
    #[test_case(local::ClassSpecializationKind::Subclass, false; "subclass_default_flags")]
    #[test_case(local::ClassSpecializationKind::Subclass, true; "subclass_preexisting_flags")]
    fn nested_type_specializations_restore_flags_and_retire_inner_first(
        kind: local::ClassSpecializationKind,
        preexisting: bool,
    ) -> anyhow::Result<()> {
        const PATH: &str = "/src/nested_specialization.pyi";
        let db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file(
                PATH,
                "from typing import Generic, TypeVar\n\
                 T = TypeVar(\"T\")\n\
                 class Box(Generic[T]): ...\n\
                 class Leaf: ...\n\
                 left = right = Box[Box[Leaf]]\n",
            )
            .build()?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let index = semantic_index(&db, file);
        let env = ProgramEnvironment::from_file(file);
        let Some(ast::Stmt::Assign(assignment)) = module.suite().last() else {
            anyhow::bail!("fixture must end with an assignment");
        };
        let ast::Expr::Subscript(outer) = &*assignment.value else {
            anyhow::bail!("fixture must specialize Box");
        };
        let ast::Expr::Subscript(inner) = &*outer.slice else {
            anyhow::bail!("fixture must contain a nested specialization");
        };
        let mut root = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Scope(global_scope(&db, file), TypeContext::default()),
            file.file(&db),
            file,
            index,
            &module,
        );
        root.context.defuse();
        root.context
            .inference_flags
            .set(InferenceFlags::DISABLE_INT_FLOAT_SPECIAL_CASE, preexisting);
        root.context
            .inference_flags
            .set(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, preexisting);
        root.context
            .inference_flags
            .set(InferenceFlags::IN_VALID_UNPACK_CONTEXT, preexisting);
        let initial_flags = root.context.inference_flags;
        let initial_binding = root.typevar_binding_context;
        assert!(matches!(
            root.deferred_state,
            local::DeferredExpressionState::None
        ));
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = events.clone();
        let syntax_storage = local::string_annotation::OrdinaryStorage::new();
        let syntax = &syntax_storage;
        let mut invocation = LocalInvocation::new(&mut root);
        invocation.owners.slots = Vec::with_capacity(1);
        invocation.observe_ownership(Rc::new(move |event| observed.borrow_mut().push(event)));
        let work = match kind {
            local::ClassSpecializationKind::ClassObject => Work::Expression(
                BuilderId::ROOT,
                &assignment.value,
                TypeContext::default(),
                ExpressionMode::Value,
            ),
            local::ClassSpecializationKind::Subclass => Work::TypeExpression(
                BuilderId::ROOT,
                local::TypeExpressionRequest::SubclassArgument {
                    slice: &assignment.value,
                },
            ),
        };
        let Ok(result) = local::drive_sync(
            &mut Some(work),
            &mut invocation,
            LocalFacts,
            &OrdinaryLocalEffects::default(),
            syntax,
        );
        drop(invocation);
        let Some(local::LocalResult::Type(ty)) = result else {
            anyhow::bail!("nested specialization returned no type");
        };
        match kind {
            local::ClassSpecializationKind::ClassObject => {
                assert_eq!(ty.display(&db, &env).to_string(), "<class 'Box[Box[Leaf]]'>");
            }
            local::ClassSpecializationKind::Subclass => {
                assert_eq!(ty.display(&db, &env).to_string(), "type[Box[Box[Leaf]]]");
                let stored = root
                    .expressions
                    .get(&ExpressionNodeKey::from(ast::ExprRef::Subscript(outer)))
                    .ok_or_else(|| anyhow::anyhow!("outer subclass expression was not stored"))?;
                assert_eq!(*stored, ty);
                let receiver = root
                    .expressions
                    .get(&ExpressionNodeKey::from(&*outer.value))
                    .ok_or_else(|| anyhow::anyhow!("outer subclass receiver was not stored"))?;
                assert!(matches!(receiver, Type::ClassLiteral(_)));
            }
        }
        assert!(!root.context.has_diagnostics());
        assert_eq!(root.context.inference_flags, initial_flags);
        assert_eq!(root.typevar_binding_context, initial_binding);
        assert!(matches!(
            root.deferred_state,
            local::DeferredExpressionState::None
        ));
        let inner_ty = root
            .expressions
            .get(&ExpressionNodeKey::from(ast::ExprRef::Subscript(inner)))
            .ok_or_else(|| anyhow::anyhow!("inner type expression was not stored"))?;
        assert!(matches!(inner_ty, Type::NominalInstance(_)));
        assert_eq!(inner_ty.display(&db, &env).to_string(), "Box[Leaf]");
        assert_mixed_lifetimes(
            &events.borrow(),
            &[
                (OwnerKind::Specialization, outer.range()),
                (OwnerKind::Specialization, inner.range()),
            ],
        )?;
        Ok(())
    }

    #[test]
    fn owned_argument_driver_boundaries_have_small_values() {
        let word = size_of::<usize>();
        for size in [
            size_of::<ActiveArgument>(),
            size_of::<PendingArgument>(),
            size_of::<CompletedArgument>(),
            size_of::<ActiveSpecialization>(),
            size_of::<PendingSpecialization>(),
            size_of::<CompletedSpecialization>(),
            size_of::<local::tuple_annotation::Active>(),
            size_of::<local::tuple_annotation::Waiting>(),
            size_of::<local::tuple_annotation::Finished>(),
        ] {
            assert_eq!(size, word);
        }
        let sizes = [
            ("Work", size_of::<Work<'_, '_>>(), 512),
            ("TupleStep", size_of::<local::tuple_annotation::Step<'_>>(), 128),
            ("Frame", size_of::<Frame<'_, '_>>(), 512),
            ("PreparedCall", size_of::<PreparedCall<'_>>(), 128),
            ("ArgumentStep", size_of::<ArgumentStep<'_, '_>>(), 128),
            ("SpecializationStep", size_of::<SpecializationStep<'_>>(), 128),
            ("FinishedOwner", size_of::<FinishedOwner<'_>>(), 128),
            ("CompletedArgumentLease", size_of::<CompletedArgumentLease<'_, '_, '_>>(), 128),
        ];
        for (name, size, limit) in sizes {
            assert!(
                size <= limit,
                "{name} retains {size} bytes, above its {limit}-byte limit"
            );
            println!("ARGUMENT_STORAGE_SIZE name={name} bytes={size} limit={limit}");
        }
        println!("TUPLE_STORAGE_SIZE owner_slot={} phase={} active={} pending={} completed={}",
            size_of::<local::OwnerSlot<'_, '_>>(),
            size_of::<local::tuple_annotation::Phase<'_, '_>>(),
            size_of::<local::tuple_annotation::State<'_, '_>>(),
            size_of::<local::tuple_annotation::Pending<'_, '_>>(),
            size_of::<local::tuple_annotation::Completed<'_, '_>>());
        println!(
            "ARGUMENT_STORAGE_PAYLOAD_SIZE active={} pending={}",
            size_of::<OwnedState<'_, '_>>(),
            size_of::<OwnedPending<'_, '_>>(),
        );
    }
}

mod taken_argument_controls {
    use std::cell::{Cell, RefCell};
    use std::convert::Infallible;
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::{Rc, Weak};
    use std::task::{Context, Poll};

    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use salsa::attempt_probe::{AttemptOutcome, Incomplete, try_with_attempt};
    use salsa::execution_probe::{
        ExecutionAdmission, ExecutionWork, RegistryBuilder, RunError, RunResult, TaskEndpoint,
    };

    use super::super::{
        self as local, ActiveArgument, ActiveSpecialization, ArgumentPhase, ArgumentStep,
        BuilderId, BuilderStore, CalleeState, CompletedArgument, CompletedArgumentLease,
        CompletedSpecialization, FinishedOwner, Frame, LocalEffects, LocalFacts, LocalInvocation,
        LocalOwners, OrdinaryLocalEffects, OrdinaryOwnedArgumentEffects, OwnedAction,
        OwnedArgumentEffects, OwnedPending, OwnedSpecializationEffects, OwnedState, OwnerKind,
        OwnerLifetime, OwnerSlot, OwnershipEvent, PendingArgument, PendingSpecialization,
        Preparation, PreparationStep, PreparedCall, SpecializationPhase, SpecializationStep, Splat,
        SynchronousLocalEffects, SynchronousOwnedArgumentEffects,
        SynchronousOwnedSpecializationEffects, TakenArgument, TakenSpecialization, Work, arguments,
        call, specialization,
    };
    use super::argument_interruption::{PATH, database, prepare};
    use super::cleanup::ArgumentBoundary;
    use crate::db::tests::{TestDb, TestDbBuilder};
    use crate::types::constraints::ConstraintSetBuilder;
    use crate::types::infer::builder::{
        CallArguments, CallErrorKind, Definition, ExpressionCache, ExpressionCacheEntry,
        InferenceFlags, PythonVersion, Type, TypeContext, TypeInferenceBuilder, ast,
        source_expression,
    };
    use crate::types::signatures::effects::try_poll_immediate;
    use crate::{Db, ProgramEnvironment};

    type SpecializationState<'db, 'expr> =
        local::SpecializationState<'db, 'expr, ConstraintSetBuilder<'db>, Infallible>;
    type SpecializationPending<'db, 'expr> =
        local::SpecializationPending<'db, 'expr, ConstraintSetBuilder<'db>, Infallible>;
    type SpecializationAction<'db, 'expr> =
        local::SpecializationAction<'db, 'expr, ConstraintSetBuilder<'db>, Infallible>;
    type SpecializationCompleted<'db, 'expr> =
        local::SpecializationCompleted<'db, 'expr, ConstraintSetBuilder<'db>, Infallible>;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Kind {
        Advance,
        Resume,
        Finish,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Mode {
        Refuse,
        Pending,
        QueuedChild,
        RefuseRetirement,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Refused {
        AfterTake,
        Unavailable(&'static str),
        UnexpectedChildReturn,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Event {
        Ownership(OwnershipEvent),
        ControlledTake {
            kind: Kind,
            index: usize,
            identity: usize,
        },
        ChildRetired,
        RootReleased,
    }

    #[derive(Default)]
    struct Journal {
        events: RefCell<Vec<Event>>,
        semantic_entries: Cell<usize>,
        taken_index: Cell<Option<usize>>,
        taken_identity: Cell<Option<usize>>,
        completed_bindings: Cell<Option<usize>>,
        child_live: Cell<bool>,
        child_queued: Cell<bool>,
        parent_pending_with_child: Cell<bool>,
        parent_resumed: Cell<bool>,
        driver_advance: Cell<usize>,
        driver_resume: Cell<usize>,
        driver_finish: Cell<usize>,
        driver_complete: Cell<bool>,
    }

    impl Journal {
        fn observe(&self, event: OwnershipEvent) {
            self.events.borrow_mut().push(Event::Ownership(event));
        }
    }

    struct Effects<'call, 'run, 'db: 'run> {
        kind: Kind,
        mode: Mode,
        expected_result: Option<Result<(), CallErrorKind>>,
        reply: Option<Type<'db>>,
        endpoint: Option<&'call TaskEndpoint<'run, 'db>>,
        journal: Rc<Journal>,
    }

    struct Child(Rc<Journal>);
    impl Drop for Child {
        fn drop(&mut self) {
            assert!(self.0.child_live.replace(false));
            assert!(!self.0.events.borrow().iter().any(|event| matches!(
                event,
                Event::Ownership(
                    OwnershipEvent::PayloadRetired { .. } | OwnershipEvent::RootRestored
                )
            )));
            self.0.events.borrow_mut().push(Event::ChildRetired);
        }
    }

    impl Effects<'_, '_, '_> {
        fn taken<P>(
            &self,
            kind: Kind,
            owners: &LocalOwners<'_, '_>,
            taken: &TakenArgument<'_, '_, P>,
        ) -> Result<(), Refused> {
            self.record_taken(kind, owners, taken.index, taken.payload.lifetime.as_ref())
        }

        fn specialization_taken<P>(
            &self,
            kind: Kind,
            owners: &LocalOwners<'_, '_>,
            taken: &TakenSpecialization<P>,
        ) -> Result<(), Refused> {
            self.record_taken(kind, owners, taken.index, taken.payload.lifetime.as_ref())
        }

        fn record_taken(
            &self,
            kind: Kind,
            owners: &LocalOwners<'_, '_>,
            index: usize,
            lifetime: Option<&OwnerLifetime>,
        ) -> Result<(), Refused> {
            assert_eq!(kind, self.kind);
            assert_eq!(index + 1, owners.slots.len());
            assert!(matches!(owners.slots.last(), Some(OwnerSlot::Taken)));
            assert!(
                owners.slots[..index]
                    .iter()
                    .all(|slot| matches!(slot, OwnerSlot::Argument(_)))
            );
            self.record_handoff(kind, index, lifetime)
        }

        fn record_handoff(
            &self,
            kind: Kind,
            index: usize,
            lifetime: Option<&OwnerLifetime>,
        ) -> Result<(), Refused> {
            assert_eq!(kind, self.kind);
            let Some(lifetime) = lifetime else {
                return Err(Refused::Unavailable("unobserved actual payload"));
            };
            assert_eq!(self.journal.taken_index.replace(Some(index)), None);
            assert_eq!(
                self.journal.taken_identity.replace(Some(lifetime.identity)),
                None
            );
            assert_eq!(
                self.journal.events.borrow().last(),
                Some(&Event::Ownership(OwnershipEvent::Taken {
                    index,
                    identity: lifetime.identity,
                }))
            );
            self.journal
                .events
                .borrow_mut()
                .push(Event::ControlledTake {
                    kind,
                    index,
                    identity: lifetime.identity,
                });
            Ok(())
        }

        async fn interrupt(&self) -> Result<(), Refused> {
            assert_eq!(self.journal.semantic_entries.replace(1), 0);
            match self.mode {
                Mode::Refuse | Mode::RefuseRetirement => Err(Refused::AfterTake),
                Mode::Pending => std::future::pending().await,
                Mode::QueuedChild => {
                    let Some(endpoint) = self.endpoint else {
                        return Err(Refused::Unavailable("missing real endpoint"));
                    };
                    endpoint
                        .child_call(|| async {
                            assert!(!self.journal.child_live.replace(true));
                            self.journal.child_queued.set(true);
                            let child = Child(self.journal.clone());
                            endpoint
                                .demand(move || async move {
                                    let _child = child;
                                    Err::<Result<(), Refused>, _>(RunError::Refused(
                                        Incomplete::Allowance,
                                    ))
                                })?
                                .await
                        })
                        .await
                }
            }
        }

        // The actual envelope remains owned by this future across the selected interruption.
        // Its lifetime event runs only after its real state or pending value drops.
        async fn hold<P, R>(&self, taken: TakenArgument<'_, '_, P>) -> Result<R, Refused> {
            assert_eq!(Some(taken.index), self.journal.taken_index.get());
            assert!(!taken.payload.data.call.arguments.args.is_empty());
            let outcome = self.interrupt().await;
            drop(taken);
            match outcome {
                Err(error) => Err(error),
                Ok(()) => Err(Refused::UnexpectedChildReturn),
            }
        }

        async fn hold_specialization<P, R>(
            &self,
            taken: TakenSpecialization<P>,
        ) -> Result<R, Refused> {
            assert_eq!(Some(taken.index), self.journal.taken_index.get());
            let outcome = self.interrupt().await;
            drop(taken);
            match outcome {
                Err(error) => Err(error),
                Ok(()) => Err(Refused::UnexpectedChildReturn),
            }
        }
    }

    impl<'db: 'run, 'ast, 'run> OwnedArgumentEffects<'db, 'ast> for Effects<'_, 'run, 'db> {
        type Error = Refused;
        type Builder = crate::types::constraints::ConstraintSetBuilder<'db>;
        type CustomSpecializationTarget = std::convert::Infallible;

        async fn prepared<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _data: call::CallData<'db, 'expr>,
            _arguments: CallArguments<'expr, 'db>,
        ) -> Result<call::Prepared<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("prepared"))
        }

        async fn install_prepared<'expr>(
            &self,
            _owners: &mut LocalOwners<'db, 'expr>,
            _id: BuilderId,
            _prepared: call::Prepared<'db, 'expr>,
        ) -> Result<PreparedCall<'db>, Refused> {
            Err(Refused::Unavailable("install_prepared"))
        }

        async fn take_active<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            owner: ActiveArgument,
        ) -> Result<TakenArgument<'db, 'expr, OwnedState<'db, 'expr>>, Refused> {
            let state = owners
                .active(&owner)
                .ok_or(Refused::Unavailable("missing active owner"))?;
            if self.kind == Kind::Finish {
                assert_eq!(state.finished_root_result(), self.expected_result);
            } else {
                assert_eq!(self.kind, Kind::Advance);
                assert!(state.prepared_request().is_some());
            }
            let Ok(taken) = OrdinaryOwnedArgumentEffects::default().take_active(owners, owner);
            if self.kind == Kind::Advance {
                self.taken(Kind::Advance, owners, &taken)?;
            }
            Ok(taken)
        }

        async fn advance<'expr>(
            &self,
            taken: TakenArgument<'db, 'expr, OwnedState<'db, 'expr>>,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<TakenArgument<'db, 'expr, OwnedAction<'db, 'expr>>, Refused> {
            if self.kind == Kind::Finish {
                assert_eq!(
                    taken.payload.phase.finished_root_result(),
                    self.expected_result
                );
                // The reached root-finished phase only tears down its local cache and moves
                // actual completed storage. Binding checks and narrowing have already finished.
                let Ok(advanced) = OrdinaryOwnedArgumentEffects::default().advance(taken, builders);
                return Ok(advanced);
            }
            assert_eq!(self.kind, Kind::Advance);
            assert!(taken.payload.phase.prepared_request().is_some());
            self.hold(taken).await
        }

        async fn install_action<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            taken: TakenArgument<'db, 'expr, OwnedAction<'db, 'expr>>,
        ) -> Result<ArgumentStep<'db, 'expr>, Refused> {
            if self.kind != Kind::Finish {
                return Err(Refused::Unavailable(
                    "install_action after interrupted advance",
                ));
            }
            let Ok(step) = OrdinaryOwnedArgumentEffects::default().install_action(owners, taken);
            Ok(step)
        }

        async fn take_pending<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            owner: PendingArgument,
        ) -> Result<TakenArgument<'db, 'expr, OwnedPending<'db, 'expr>>, Refused> {
            assert!(owners.pending(&owner).is_some());
            let Ok(taken) = OrdinaryOwnedArgumentEffects::default().take_pending(owners, owner);
            self.taken(Kind::Resume, owners, &taken)?;
            Ok(taken)
        }

        async fn resume<'expr>(
            &self,
            taken: TakenArgument<'db, 'expr, OwnedPending<'db, 'expr>>,
            ty: Type<'db>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<TakenArgument<'db, 'expr, OwnedState<'db, 'expr>>, Refused> {
            assert_eq!(self.kind, Kind::Resume);
            assert_eq!(self.reply, Some(ty));
            self.hold(taken).await
        }

        async fn install_active<'expr>(
            &self,
            _owners: &mut LocalOwners<'db, 'expr>,
            _taken: TakenArgument<'db, 'expr, OwnedState<'db, 'expr>>,
        ) -> Result<ActiveArgument, Refused> {
            Err(Refused::Unavailable(
                "install_active after interrupted resume",
            ))
        }

        async fn take_completed<'owner, 'expr>(
            &self,
            owners: &'owner mut LocalOwners<'db, 'expr>,
            owner: CompletedArgument,
        ) -> Result<CompletedArgumentLease<'owner, 'db, 'expr>, Refused> {
            assert_eq!(owner.0 + 1, owners.slots.len());
            assert!(
                owners.slots[..owner.0]
                    .iter()
                    .all(|slot| matches!(slot, OwnerSlot::Argument(_)))
            );
            let completed = owners
                .completed(&owner)
                .ok_or(Refused::Unavailable("missing completed owner"))?;
            assert_eq!(Some(completed.result), self.expected_result);
            let address = std::ptr::from_ref(&completed.storage.bindings).addr();
            assert_eq!(self.journal.completed_bindings.replace(Some(address)), None);
            let Ok(lease) = OrdinaryOwnedArgumentEffects::default().take_completed(owners, owner);
            let OwnerSlot::Argument(payload) = &*lease.slot else {
                return Err(Refused::Unavailable("completed lease moved its payload"));
            };
            assert!(matches!(&payload.phase, ArgumentPhase::Completed(_)));
            self.record_handoff(Kind::Finish, lease.index, payload.lifetime.as_ref())?;
            Ok(lease)
        }

        async fn finish<'expr>(
            &self,
            mut lease: CompletedArgumentLease<'_, 'db, 'expr>,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<FinishedOwner<'db>, Refused> {
            assert_eq!(self.kind, Kind::Finish);
            assert_eq!(Some(lease.index), self.journal.taken_index.get());
            {
                let (_, data, storage, result) = lease.parts_mut();
                assert_eq!(Some(result), self.expected_result);
                assert_eq!(storage.arguments.len(), 1);
                assert!(!data.call.arguments.args.is_empty());
                assert_eq!(
                    Some(std::ptr::from_ref(&*storage.bindings).addr()),
                    self.journal.completed_bindings.get(),
                );
            }
            if self.mode == Mode::RefuseRetirement {
                let Ok(finished) = OrdinaryOwnedArgumentEffects::default().finish(lease, builders);
                return Ok(finished);
            }
            let outcome = self.interrupt().await;
            drop(lease);
            match outcome {
                Err(error) => Err(error),
                Ok(()) => Err(Refused::UnexpectedChildReturn),
            }
        }

        async fn retire<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            finished: FinishedOwner<'db>,
        ) -> Result<Type<'db>, Refused> {
            if self.mode != Mode::RefuseRetirement {
                return Err(Refused::Unavailable("retire after interrupted finish"));
            }
            assert_eq!(finished.index + 1, owners.slots.len());
            assert_eq!(Some(finished.index), self.journal.taken_index.get());
            assert!(matches!(owners.slots.last(), Some(OwnerSlot::Taken)));
            assert!(
                owners.slots[..finished.index]
                    .iter()
                    .all(|slot| matches!(slot, OwnerSlot::Argument(_)))
            );
            {
                let events = self.journal.events.borrow();
                let retired: Vec<_> = events
                    .iter()
                    .filter_map(|event| match event {
                        Event::Ownership(OwnershipEvent::PayloadRetired { identity }) => {
                            Some(*identity)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    retired.as_slice(),
                    self.journal.taken_identity.get().as_slice()
                );
                assert!(matches!(
                    events.last(),
                    Some(Event::Ownership(OwnershipEvent::PayloadRetired { .. }))
                ));
            }
            self.interrupt().await?;
            Err(Refused::UnexpectedChildReturn)
        }
    }

    impl<'db: 'run, 'ast, 'run> OwnedSpecializationEffects<'db, 'ast> for Effects<'_, 'run, 'db> {
        type Error = Refused;
        type Builder = ConstraintSetBuilder<'db>;
        type CustomSpecializationTarget = Infallible;

        async fn take_active<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            owner: ActiveSpecialization,
        ) -> Result<TakenSpecialization<SpecializationState<'db, 'expr>>, Refused> {
            let taken = owners.take_specialization_active(owner);
            if self.kind == Kind::Advance {
                assert!(matches!(
                    taken.payload.phase,
                    specialization::State::Active(_, specialization::Phase::LocateVariadic(..))
                ));
                self.specialization_taken(Kind::Advance, owners, &taken)?;
            } else {
                assert_eq!(self.kind, Kind::Finish);
                assert!(matches!(
                    taken.payload.phase,
                    specialization::State::Active(_, specialization::Phase::Finalize)
                ));
            }
            Ok(taken)
        }

        async fn advance<'expr>(
            &self,
            taken: TakenSpecialization<SpecializationState<'db, 'expr>>,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<TakenSpecialization<SpecializationAction<'db, 'expr>>, Refused> {
            if self.kind == Kind::Finish {
                let Ok(advanced) = SynchronousOwnedSpecializationEffects::advance(
                    &OrdinaryLocalEffects::default(),
                    taken,
                    builders,
                );
                return Ok(advanced);
            }
            assert_eq!(self.kind, Kind::Advance);
            self.hold_specialization(taken).await
        }

        async fn install_action<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            taken: TakenSpecialization<SpecializationAction<'db, 'expr>>,
        ) -> Result<SpecializationStep<'expr>, Refused> {
            assert_eq!(self.kind, Kind::Finish);
            let step = owners.install_specialization_action(taken);
            assert!(matches!(step, SpecializationStep::Complete(_)));
            Ok(step)
        }

        async fn take_pending<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            owner: PendingSpecialization,
        ) -> Result<TakenSpecialization<SpecializationPending<'db, 'expr>>, Refused> {
            let taken = owners.take_specialization_pending(owner);
            self.specialization_taken(Kind::Resume, owners, &taken)?;
            Ok(taken)
        }

        async fn resume<'expr>(
            &self,
            taken: TakenSpecialization<SpecializationPending<'db, 'expr>>,
            ty: Type<'db>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<TakenSpecialization<SpecializationState<'db, 'expr>>, Refused> {
            assert_eq!(self.kind, Kind::Resume);
            assert_eq!(self.reply, Some(ty));
            self.hold_specialization(taken).await
        }

        async fn install_active<'expr>(
            &self,
            _owners: &mut LocalOwners<'db, 'expr>,
            _taken: TakenSpecialization<SpecializationState<'db, 'expr>>,
        ) -> Result<ActiveSpecialization, Refused> {
            Err(Refused::Unavailable(
                "install_active after interrupted specialization",
            ))
        }

        async fn take_completed<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            owner: CompletedSpecialization,
        ) -> Result<TakenSpecialization<SpecializationCompleted<'db, 'expr>>, Refused> {
            let taken = owners.take_specialization_completed(owner);
            self.specialization_taken(Kind::Finish, owners, &taken)?;
            Ok(taken)
        }

        async fn finish<'expr>(
            &self,
            taken: TakenSpecialization<SpecializationCompleted<'db, 'expr>>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<FinishedOwner<'db>, Refused> {
            assert_eq!(self.kind, Kind::Finish);
            self.hold_specialization(taken).await
        }

        async fn retire<'expr>(
            &self,
            _owners: &mut LocalOwners<'db, 'expr>,
            _finished: FinishedOwner<'db>,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable(
                "retire after interrupted specialization",
            ))
        }
    }

    impl<'db: 'run, 'ast, 'run> LocalEffects<'db, 'ast> for Effects<'_, 'run, 'db> {
        type Error = Refused;
        type Builder = crate::types::constraints::ConstraintSetBuilder<'db>;
        type CustomSpecializationTarget = std::convert::Infallible;
        type StringAnnotations = local::string_annotation::OrdinaryStorage;

        async fn parse_string_annotation<'expr>(
            &self,
            builders: &BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            string: &ast::ExprStringLiteral,
            storage: &'expr Self::StringAnnotations,
        ) -> Result<Option<&'expr ast::Expr>, Refused> {
            let Ok(parsed) = OrdinaryLocalEffects::default()
                .parse_string_annotation(builders, id, string, storage);
            Ok(parsed)
        }

        async fn prepare_string_annotation<'expr>(
            &self,
            builders: &BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            string: &'expr ast::ExprStringLiteral,
            parsed: &'expr ast::Expr,
        ) -> Result<local::string_annotation::Scope<'expr>, Refused> {
            let Ok(scope) = OrdinaryLocalEffects::default()
                .prepare_string_annotation(builders, id, string, parsed);
            Ok(scope)
        }

        async fn enter_string_annotation(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            scope: local::string_annotation::Scope<'_>,
        ) -> Result<(), Refused> {
            let Ok(()) =
                OrdinaryLocalEffects::default().enter_string_annotation(builders, id, scope);
            Ok(())
        }

        async fn finish_string_annotation(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            scope: local::string_annotation::Scope<'_>,
        ) -> Result<(), Refused> {
            let Ok(()) =
                OrdinaryLocalEffects::default().finish_string_annotation(builders, id, scope);
            Ok(())
        }

        async fn next<'expr>(
            &self,
            work: &mut Option<Work<'db, 'expr>>,
        ) -> Result<Option<Work<'db, 'expr>>, Refused> {
            Ok(work.take())
        }
        async fn continue_with<'expr>(
            &self,
            work: &mut Option<Work<'db, 'expr>>,
            next: Work<'db, 'expr>,
        ) -> Result<(), Refused> {
            *work = Some(next);
            Ok(())
        }
        async fn push<'expr>(
            &self,
            _invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr>,
            _frame: Frame<'db, 'expr>,
        ) -> Result<(), Refused> {
            Err(Refused::Unavailable("push"))
        }
        async fn pop<'expr>(
            &self,
            frames: &mut Vec<Frame<'db, 'expr>>,
        ) -> Result<Option<Frame<'db, 'expr>>, Refused> {
            Ok(frames.pop())
        }
        async fn push_annotation<'expr>(&self, _invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr>, _continuation: local::AnnotationContinuation<'expr>) -> Result<(), Refused> {
            Err(Refused::Unavailable("push_annotation"))
        }

        async fn pop_annotation<'expr>(&self, invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr>) -> Result<Option<local::AnnotationContinuation<'expr>>, Refused> {
            Ok(invocation.annotations.pop())
        }

        async fn canonical(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _expression: &ast::Expr,
            _tcx: TypeContext<'db>,
        ) -> Result<Option<Type<'db>>, Refused> {
            Err(Refused::Unavailable("canonical"))
        }
        async fn existing(
            &self,
            _builders: &BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _expression: &ast::Expr,
        ) -> Result<Option<Type<'db>>, Refused> {
            Err(Refused::Unavailable("existing"))
        }
        async fn cache_enabled(
            &self,
            _builders: &BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
        ) -> Result<bool, Refused> {
            Err(Refused::Unavailable("cache_enabled"))
        }
        async fn cache_lookup(
            &self,
            _builders: &BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _expression: &ast::Expr,
            _tcx: TypeContext<'db>,
        ) -> Result<Option<ExpressionCacheEntry<'db>>, Refused> {
            Err(Refused::Unavailable("cache_lookup"))
        }
        async fn cache_hit(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _expression: &ast::Expr,
            _entry: ExpressionCacheEntry<'db>,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("cache_hit"))
        }
        async fn speculate(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
        ) -> Result<BuilderId, Refused> {
            Err(Refused::Unavailable("speculate"))
        }
        async fn cache_commit(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _parent: BuilderId,
            _child: BuilderId,
            _expression: &ast::Expr,
            _tcx: TypeContext<'db>,
            _ty: Type<'db>,
        ) -> Result<(), Refused> {
            Err(Refused::Unavailable("cache_commit"))
        }
        async fn contextual_dispatch(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _expression: &ast::Expr,
            _tcx: TypeContext<'db>,
        ) -> Result<source_expression::ContextualExpressionResult<'db>, Refused> {
            Err(Refused::Unavailable("contextual_dispatch"))
        }
        async fn other_expression(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _expression: &ast::Expr,
            _tcx: TypeContext<'db>,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("other_expression"))
        }
        async fn finish_expression(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _expression: &ast::Expr,
            _ty: Type<'db>,
            _tcx: TypeContext<'db>,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("finish_expression"))
        }
        async fn enter_callee(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
        ) -> Result<CalleeState<'db>, Refused> {
            Err(Refused::Unavailable("enter_callee"))
        }
        async fn restore_callee(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _state: CalleeState<'db>,
        ) -> Result<(), Refused> {
            Err(Refused::Unavailable("restore_callee"))
        }
        async fn prepare_annotation_scope(
            &self,
            builders: &BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            state: local::DeferredExpressionState,
        ) -> Result<local::AnnotationScope, Refused> {
            let Ok(scope) =
                OrdinaryLocalEffects::default().prepare_annotation_scope(builders, id, state);
            Ok(scope)
        }
        async fn enter_annotation_scope(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            scope: local::AnnotationScope,
        ) -> Result<(), Refused> {
            let Ok(()) =
                OrdinaryLocalEffects::default().enter_annotation_scope(builders, id, scope);
            Ok(())
        }
        async fn restore_annotation_scope(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            scope: local::AnnotationScope,
        ) -> Result<(), Refused> {
            let Ok(()) =
                OrdinaryLocalEffects::default().restore_annotation_scope(builders, id, scope);
            Ok(())
        }
        async fn store_annotation_qualifiers(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            expression: &ast::Expr,
            qualifiers: local::TypeQualifiers,
        ) -> Result<(), Refused> {
            let Ok(()) = OrdinaryLocalEffects::default()
                .store_annotation_qualifiers(builders, id, expression, qualifiers);
            Ok(())
        }
        async fn store_type_expression(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            expression: &ast::Expr,
            ty: Type<'db>,
        ) -> Result<(), Refused> {
            let Ok(()) =
                OrdinaryLocalEffects::default().store_type_expression(builders, id, expression, ty);
            Ok(())
        }
        async fn prepare_type_expression_scope(
            &self,
            builders: &BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            mode: local::TypeExpressionMode,
        ) -> Result<Option<local::TypeExpressionScope>, Refused> {
            let Ok(scope) =
                OrdinaryLocalEffects::default().prepare_type_expression_scope(builders, id, mode);
            Ok(scope)
        }
        async fn enter_type_expression_scope(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            scope: local::TypeExpressionScope,
        ) -> Result<(), Refused> {
            let Ok(()) =
                OrdinaryLocalEffects::default().enter_type_expression_scope(builders, id, scope);
            Ok(())
        }
        async fn restore_type_expression_before_store(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            scope: local::TypeExpressionScope,
        ) -> Result<(), Refused> {
            let Ok(()) = OrdinaryLocalEffects::default()
                .restore_type_expression_before_store(builders, id, scope);
            Ok(())
        }
        async fn restore_type_expression_after_store(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            id: BuilderId,
            scope: local::TypeExpressionScope,
        ) -> Result<(), Refused> {
            let Ok(()) = OrdinaryLocalEffects::default()
                .restore_type_expression_after_store(builders, id, scope);
            Ok(())
        }
        async fn start_annotation<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _annotation: &'expr ast::Expr,
            _policy: local::PEP613Policy,
        ) -> Result<local::AnnotationStep<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("start_annotation"))
        }
        async fn resume_annotation<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _pending: local::AnnotationPending<'expr>,
            _ty: Type<'db>,
        ) -> Result<local::AnnotationStep<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("resume_annotation"))
        }
        async fn resume_qualifier<'expr>(&self, _builders: &mut BuilderStore<'_, 'db, 'ast>, _root: &local::AnnotationRoot<'expr>, _pending: local::QualifierPending<'expr>, _ty: local::TypeAndQualifiers<'db>) -> Result<local::AnnotationStep<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("resume_qualifier"))
        }


        async fn start_tuple_value<'expr>(
            &self,
            _owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>,
            _id: BuilderId,
            _tuple: &'expr ast::ExprTuple,
            _context: TypeContext<'db>,
        ) -> Result<local::tuple_expression::Active, Refused> {
            Err(Refused::Unavailable("start_tuple_value"))
        }

        async fn tuple_value_step<'expr>(
            &self,
            _owner: local::tuple_expression::Active,
            _owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<local::tuple_expression::Step<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("tuple_value_step"))
        }

        async fn resume_tuple_value<'expr>(
            &self,
            _owner: local::tuple_expression::Waiting,
            _owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>,
        ) -> Result<local::tuple_expression::Active, Refused> {
            Err(Refused::Unavailable("resume_tuple_value"))
        }

        async fn finish_tuple_value<'expr>(
            &self,
            _owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>,
            _owner: local::tuple_expression::Finished,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("finish_tuple_value"))
        }

        async fn start_type_expression<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _request: local::TypeExpressionRequest<'db, 'expr>,
        ) -> Result<local::TypeExpressionStep<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("start_type_expression"))
        }
        async fn resume_type_expression<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _pending: local::TypeExpressionPending<'db, 'expr>,
            _ty: Type<'db>,
        ) -> Result<local::TypeExpressionStep<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("resume_type_expression"))
        }
        async fn start_assignment<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _target: &'expr ast::Expr,
            _call: &'expr ast::ExprCall,
            _definition: Definition<'db>,
            _callable_type: Type<'db>,
        ) -> Result<local::assignment::Start<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("start_assignment"))
        }
        async fn finish_assignment(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _target: &ast::Expr,
            _call: &ast::ExprCall,
            _callable_type: Type<'db>,
            _ty: Type<'db>,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("finish_assignment"))
        }
        async fn legacy_typevar<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _state: local::legacy::State<'db, 'expr>,
        ) -> Result<local::legacy::Action<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("legacy_typevar"))
        }
        async fn resume_legacy_typevar<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _pending: local::legacy::Pending<'db, 'expr>,
            _ty: Type<'db>,
        ) -> Result<local::legacy::State<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("resume_legacy_typevar"))
        }
        async fn subscript_receiver<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _subscript: &'expr ast::ExprSubscript,
            _ty: Type<'db>,
        ) -> Result<local::subscript::SubscriptStart<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("subscript_receiver"))
        }
        async fn subscript_slice<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _pending: local::subscript::SubscriptPending<'db, 'expr>,
            _ty: Type<'db>,
        ) -> Result<Result<Type<'db>, Type<'db>>, Refused> {
            Err(Refused::Unavailable("subscript_slice"))
        }
        async fn start_call<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _expression: &'expr ast::ExprCall,
            _ty: Type<'db>,
            _tcx: TypeContext<'db>,
        ) -> Result<call::Start<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("start_call"))
        }
        async fn prepare<'expr>(
            &self,
            _id: BuilderId,
            _arguments: &'expr ast::Arguments,
        ) -> Result<Preparation<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("prepare"))
        }
        async fn preparation_step<'expr>(
            &self,
            _preparation: Preparation<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<PreparationStep<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("preparation_step"))
        }
        async fn resume_splat<'expr>(
            &self,
            _splat: Splat<'db, 'expr>,
            _ty: Type<'db>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<Preparation<'db, 'expr>, Refused> {
            Err(Refused::Unavailable("resume_splat"))
        }
        async fn prepared_call<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _owners: &mut LocalOwners<'db, 'expr>,
            _id: BuilderId,
            _data: call::CallData<'db, 'expr>,
            _arguments: CallArguments<'expr, 'db>,
        ) -> Result<PreparedCall<'db>, Refused> {
            Err(Refused::Unavailable("prepared_call"))
        }
        async fn argument_step<'expr>(
            &self,
            owner: ActiveArgument,
            owners: &mut LocalOwners<'db, 'expr>,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<ArgumentStep<'db, 'expr>, Refused> {
            self.journal
                .driver_advance
                .set(self.journal.driver_advance.get() + 1);
            local::owned_argument_step(owner, owners, builders, self).await
        }
        async fn resume_argument<'expr>(
            &self,
            owner: PendingArgument,
            ty: Type<'db>,
            owners: &mut LocalOwners<'db, 'expr>,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<ActiveArgument, Refused> {
            self.journal
                .driver_resume
                .set(self.journal.driver_resume.get() + 1);
            local::owned_resume_argument(owner, ty, owners, builders, self).await
        }
        async fn finish_call<'expr>(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            owners: &mut LocalOwners<'db, 'expr>,
            owner: CompletedArgument,
        ) -> Result<Type<'db>, Refused> {
            self.journal
                .driver_finish
                .set(self.journal.driver_finish.get() + 1);
            local::owned_finish_call(builders, owners, owner, self).await
        }
        async fn start_callable_annotation<'expr>(
            &self,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _request: local::callable_annotation::Request<'expr>,
        ) -> Result<local::callable_annotation::Active, Refused> {
            Err(Refused::Unavailable("start_callable_annotation"))
        }
        async fn callable_annotation_step<'expr>(
            &self,
            _owner: local::callable_annotation::Active,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<local::callable_annotation::Step<'expr>, Refused> {
            Err(Refused::Unavailable("callable_annotation_step"))
        }
        async fn resume_callable_annotation<'expr>(
            &self,
            _owner: local::callable_annotation::Waiting,
            _ty: Type<'db>,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<local::callable_annotation::Active, Refused> {
            Err(Refused::Unavailable("resume_callable_annotation"))
        }
        async fn finish_callable_annotation<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _owners: &mut LocalOwners<'db, 'expr>,
            _owner: local::callable_annotation::Finished,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("finish_callable_annotation"))
        }
        async fn start_tuple_annotation<'expr>(
            &self,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _request: local::tuple_annotation::Request<'expr>,
        ) -> Result<local::tuple_annotation::Active, Refused> {
            Err(Refused::Unavailable("start_tuple_annotation"))
        }
        async fn tuple_annotation_step<'expr>(
            &self,
            _owner: local::tuple_annotation::Active,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<local::tuple_annotation::Step<'expr>, Refused> {
            Err(Refused::Unavailable("tuple_annotation_step"))
        }
        async fn resume_tuple_annotation<'expr>(
            &self,
            _owner: local::tuple_annotation::Waiting,
            _ty: Type<'db>,
            _owners: &mut LocalOwners<'db, 'expr>,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<local::tuple_annotation::Active, Refused> {
            Err(Refused::Unavailable("resume_tuple_annotation"))
        }
        async fn finish_tuple_annotation<'expr>(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _owners: &mut LocalOwners<'db, 'expr>,
            _owner: local::tuple_annotation::Finished,
        ) -> Result<Type<'db>, Refused> {
            Err(Refused::Unavailable("finish_tuple_annotation"))
        }
        async fn start_specialization<'expr>(
            &self,
            owners: &mut LocalOwners<'db, 'expr>,
            id: BuilderId,
            subscript: &'expr ast::ExprSubscript,
            value_ty: Type<'db>,
            class: crate::types::StaticClassLiteral<'db>,
            generic_context: crate::types::generics::GenericContext<'db>,
            kind: local::ClassSpecializationKind,
        ) -> Result<local::ActiveSpecialization, Refused> {
            let Ok(owner) = OrdinaryLocalEffects::default().start_specialization(
                owners,
                id,
                subscript,
                value_ty,
                class,
                generic_context,
                kind,
            );
            Ok(owner)
        }
        async fn specialization_step<'expr>(
            &self,
            owner: local::ActiveSpecialization,
            owners: &mut LocalOwners<'db, 'expr>,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<local::SpecializationStep<'expr>, Refused> {
            self.journal
                .driver_advance
                .set(self.journal.driver_advance.get() + 1);
            local::owned_specialization_step(owner, owners, builders, self).await
        }
        async fn resume_specialization<'expr>(
            &self,
            owner: local::PendingSpecialization,
            ty: Type<'db>,
            owners: &mut LocalOwners<'db, 'expr>,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
        ) -> Result<local::ActiveSpecialization, Refused> {
            self.journal
                .driver_resume
                .set(self.journal.driver_resume.get() + 1);
            local::owned_resume_specialization(owner, ty, owners, builders, self).await
        }
        async fn finish_specialization<'expr>(
            &self,
            builders: &mut BuilderStore<'_, 'db, 'ast>,
            owners: &mut LocalOwners<'db, 'expr>,
            owner: local::CompletedSpecialization,
        ) -> Result<Type<'db>, Refused> {
            self.journal
                .driver_finish
                .set(self.journal.driver_finish.get() + 1);
            local::owned_finish_specialization(builders, owners, owner, self).await
        }
        async fn enter_paramspec(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
        ) -> Result<bool, Refused> {
            Err(Refused::Unavailable("enter_paramspec"))
        }
        async fn restore_paramspec(
            &self,
            _builders: &mut BuilderStore<'_, 'db, 'ast>,
            _id: BuilderId,
            _previous: bool,
        ) -> Result<(), Refused> {
            Err(Refused::Unavailable("restore_paramspec"))
        }
        async fn permit_paramspec(
            &self,
            _policy: arguments::ArgumentPolicy,
            _expression: &ast::Expr,
        ) -> Result<bool, Refused> {
            Err(Refused::Unavailable("permit_paramspec"))
        }
        async fn complete<'expr>(
            &self,
            _invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr>,
        ) -> Result<(), Refused> {
            self.journal.driver_complete.set(true);
            Err(Refused::Unavailable("complete"))
        }
    }

    enum Operation<'db> {
        Advance(ActiveArgument),
        Resume(PendingArgument, Type<'db>),
        Finish(ActiveArgument),
        SpecAdvance(ActiveSpecialization),
        SpecResume(PendingSpecialization, Type<'db>),
        SpecFinish(ActiveSpecialization),
    }

    // The invocation outlives the real driver. Resume enters its return-frame branch with a reply
    // produced by ordinary inference; finish enters its argument or specialization branch at a reached result.
    async fn run<'root, 'db: 'run, 'ast, 'expr, 'run>(
        mut invocation: LocalInvocation<'root, 'db, 'ast, 'expr>,
        operation: Operation<'db>,
        effects: &Effects<'_, 'run, 'db>,
        syntax: &'expr local::string_annotation::OrdinaryStorage,
    ) -> Result<(), Refused> {
        let work = match operation {
            Operation::Advance(owner) | Operation::Finish(owner) => Work::Arguments(owner),
            Operation::Resume(owner, ty) => {
                invocation.frames.push(Frame::Argument(owner));
                Work::Return(ty)
            }
            Operation::SpecAdvance(owner) | Operation::SpecFinish(owner) => {
                Work::Specialization(owner)
            }
            Operation::SpecResume(owner, ty) => {
                invocation.frames.push(Frame::Specialization(owner));
                Work::Return(ty)
            }
        };
        local::drive(&mut Some(work), &mut invocation, LocalFacts, effects, syntax)
            .await
            .map(|_| ())
    }

    fn install<'db, 'expr>(
        owners: &mut LocalOwners<'db, 'expr>,
        builder: BuilderId,
        data: call::CallData<'db, 'expr>,
        storage: arguments::OwnedArguments<'expr, 'db>,
    ) -> anyhow::Result<ActiveArgument> {
        let Ok(prepared) = OrdinaryOwnedArgumentEffects::default().install_prepared(
            owners,
            builder,
            call::Prepared::Arguments(data, storage, None),
        );
        let PreparedCall::Arguments(owner) = prepared else {
            anyhow::bail!("actual matched arguments did not install an owner");
        };
        Ok(owner)
    }

    // Every semantic operation in this preparation is ordinary synchronous inference.
    // The result is a real phase token and, for resume, its actual inferred expression reply.
    fn reach<'db, 'ast, 'expr>(
        invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr>,
        mut owner: ActiveArgument,
        kind: Kind,
    ) -> anyhow::Result<Operation<'db>> {
        for _ in 0..256 {
            if kind == Kind::Finish
                && invocation
                    .owners
                    .active(&owner)
                    .is_some_and(|state| state.finished_root_result().is_some())
            {
                return Ok(Operation::Finish(owner));
            }
            if kind == Kind::Advance
                && invocation
                    .owners
                    .active(&owner)
                    .is_some_and(|state| state.prepared_request().is_some())
            {
                return Ok(Operation::Advance(owner));
            }
            let Ok(step) = local::owned_argument_step_sync(
                owner,
                &mut invocation.owners,
                &mut invocation.builders,
                &OrdinaryOwnedArgumentEffects::default(),
            );
            match step {
                ArgumentStep::Continue(next) => owner = next,
                ArgumentStep::Infer {
                    pending,
                    builder,
                    expression,
                    tcx,
                    policy,
                } => {
                    anyhow::ensure!(matches!(policy, arguments::ArgumentPolicy::Ordinary));
                    let ty = invocation
                        .builders
                        .get_mut(builder)
                        .infer_expression(expression, tcx);
                    if kind == Kind::Resume {
                        return Ok(Operation::Resume(pending, ty));
                    }
                    let Ok(next) = local::owned_resume_argument_sync(
                        pending,
                        ty,
                        &mut invocation.owners,
                        &mut invocation.builders,
                        &OrdinaryOwnedArgumentEffects::default(),
                    );
                    owner = next;
                }
                ArgumentStep::Complete(_) => {
                    anyhow::bail!("fixture completed before the required reached phase")
                }
            }
        }
        anyhow::bail!("fixture did not reach {kind:?}")
    }

    fn specialization_database() -> anyhow::Result<TestDb> {
        TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file(
                "/src/ops.pyi",
                "class Leaf: ...\nclass Box[T]: ...\ndef keep(value: object, /) -> int: ...\n",
            )
            .with_file(
                PATH,
                "from ops import Box, Leaf, keep\ndef marker(): ...\nresult: int = keep(Box[Leaf])\n",
            )
            .build()
    }

    fn nested_specialization<'db, 'ast, 'expr>(
        invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr>,
        mut outer: ActiveArgument,
    ) -> anyhow::Result<ActiveSpecialization> {
        for _ in 0..256 {
            let Ok(step) = local::owned_argument_step_sync(
                outer,
                &mut invocation.owners,
                &mut invocation.builders,
                &OrdinaryOwnedArgumentEffects::default(),
            );
            match step {
                ArgumentStep::Continue(next) => outer = next,
                ArgumentStep::Infer {
                    pending,
                    builder,
                    expression,
                    policy,
                    ..
                } => {
                    anyhow::ensure!(matches!(policy, arguments::ArgumentPolicy::Ordinary));
                    let ast::Expr::Subscript(subscript) = expression else {
                        anyhow::bail!("outer argument is not the nested specialization");
                    };
                    anyhow::ensure!(invocation.owners.pending(&pending).is_some());
                    invocation.frames.push(Frame::Argument(pending));
                    let selected = invocation.builders.get_mut(builder);
                    let value_ty =
                        selected.infer_expression(&subscript.value, TypeContext::default());
                    let class = value_ty
                        .as_class_literal()
                        .and_then(|class| class.as_static())
                        .ok_or_else(|| anyhow::anyhow!("specialization receiver is not a class"))?;
                    let generic_context = class
                        .generic_context(selected.db())
                        .ok_or_else(|| anyhow::anyhow!("specialization receiver is not generic"))?;
                    let Ok(owner) = OrdinaryLocalEffects::default().start_specialization(
                        &mut invocation.owners,
                        builder,
                        subscript,
                        value_ty,
                        class,
                        generic_context,
                        local::ClassSpecializationKind::ClassObject,
                    );
                    assert_eq!(invocation.owners.slots.len(), 2);
                    assert!(matches!(
                        invocation.owners.slots.first(),
                        Some(OwnerSlot::Argument(payload))
                            if matches!(payload.phase, ArgumentPhase::Pending(_))
                    ));
                    return Ok(owner);
                }
                ArgumentStep::Complete(_) => {
                    anyhow::bail!("outer call completed before its specialization")
                }
            }
        }
        anyhow::bail!("outer call did not request its specialization")
    }

    fn reach_specialization<'db, 'ast, 'expr>(
        invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr>,
        mut owner: ActiveSpecialization,
        kind: Kind,
    ) -> anyhow::Result<Operation<'db>> {
        for _ in 0..256 {
            let Some(OwnerSlot::Specialization(payload)) = invocation.owners.slots.last() else {
                anyhow::bail!("specialization owner is missing");
            };
            if kind == Kind::Advance
                && matches!(
                    payload.phase,
                    SpecializationPhase::Active(specialization::State::Active(_, specialization::Phase::LocateVariadic(..)))
                )
            {
                assert!(
                    invocation
                        .builders
                        .builder(payload.builder)
                        .inference_flags()
                        .contains(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR)
                );
                return Ok(Operation::SpecAdvance(owner));
            }
            if kind == Kind::Finish
                && matches!(
                    payload.phase,
                    SpecializationPhase::Active(specialization::State::Active(_, specialization::Phase::Finalize))
                )
            {
                return Ok(Operation::SpecFinish(owner));
            }
            let Ok(step) = local::owned_specialization_step_sync(
                owner,
                &mut invocation.owners,
                &mut invocation.builders,
                &OrdinaryLocalEffects::default(),
            );
            match step {
                SpecializationStep::Continue(next) => owner = next,
                SpecializationStep::Infer {
                    pending,
                    builder,
                    request,
                } => {
                    let specialization::ChildRequest::TypeExpression(expression) = request else {
                        anyhow::bail!("generic class requested a value expression");
                    };
                    let selected = invocation.builders.get_mut(builder);
                    assert!(
                        selected
                            .inference_flags()
                            .contains(InferenceFlags::IN_VALID_UNPACK_CONTEXT)
                    );
                    let ty = selected.infer_type_expression(expression);
                    if kind == Kind::Resume {
                        return Ok(Operation::SpecResume(pending, ty));
                    }
                    let Ok(next) = local::owned_resume_specialization_sync(
                        pending,
                        ty,
                        &mut invocation.owners,
                        &mut invocation.builders,
                        &OrdinaryLocalEffects::default(),
                    );
                    owner = next;
                }
                SpecializationStep::Complete(_) => {
                    anyhow::bail!("specialization completed before {kind:?}")
                }
            }
        }
        anyhow::bail!("specialization did not reach {kind:?}")
    }

    struct Root<'db, 'ast> {
        builder: Option<TypeInferenceBuilder<'db, 'ast>>,
        binding: Definition<'db>,
        flags: InferenceFlags,
        original_cache: Option<Weak<RefCell<ExpressionCache<'db>>>>,
        journal: Rc<Journal>,
    }

    impl<'db, 'ast> Root<'db, 'ast> {
        fn new(
            builder: TypeInferenceBuilder<'db, 'ast>,
            binding: Definition<'db>,
            journal: Rc<Journal>,
        ) -> Self {
            Self {
                flags: builder.context.inference_flags,
                original_cache: builder.expression_cache.as_ref().map(Rc::downgrade),
                builder: Some(builder),
                binding,
                journal,
            }
        }

        fn get_mut(&mut self) -> anyhow::Result<&mut TypeInferenceBuilder<'db, 'ast>> {
            self.builder
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("root was already released"))
        }
    }

    impl Drop for Root<'_, '_> {
        fn drop(&mut self) {
            if let Some(builder) = self.builder.take() {
                assert!(!self.journal.child_live.get());
                assert_eq!(builder.typevar_binding_context, Some(self.binding));
                assert_eq!(builder.context.inference_flags, self.flags);
                let cache_matches = match (&self.original_cache, builder.expression_cache.as_ref())
                {
                    (Some(original), Some(cache)) => {
                        Weak::ptr_eq(original, &Rc::downgrade(cache))
                            && Rc::strong_count(cache) == 1
                    }
                    (None, None) => true,
                    _ => false,
                };
                assert!(cache_matches, "root cache ownership was not restored");
                drop(builder);
                self.journal.events.borrow_mut().push(Event::RootReleased);
            }
        }
    }

    fn observe(invocation: &mut LocalInvocation<'_, '_, '_, '_>, journal: &Rc<Journal>) {
        let observed = journal.clone();
        invocation.observe_ownership(Rc::new(move |event| observed.observe(event)));
    }

    fn assert_retirement(journal: &Journal, owners: usize, builders: usize, queued: bool) {
        let events = journal.events.borrow();
        let created: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Ownership(OwnershipEvent::Created {
                    kind: _,
                    index, identity, ..
                }) => Some((*index, *identity)),
                _ => None,
            })
            .collect();
        assert_eq!(created.len(), owners);
        assert_eq!(
            created.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
            (0..owners).collect::<Vec<_>>()
        );
        let retired: Vec<_> = events
            .iter()
            .copied()
            .filter(|event| {
                matches!(
                    event,
                    Event::ChildRetired
                        | Event::RootReleased
                        | Event::Ownership(
                            OwnershipEvent::PayloadRetired { .. }
                                | OwnershipEvent::SpeculativeRetired(_)
                                | OwnershipEvent::RootRestored
                        )
                )
            })
            .collect();
        let mut expected = Vec::new();
        if queued {
            expected.push(Event::ChildRetired);
        }
        expected.extend(created.iter().rev().map(|(_, identity)| {
            Event::Ownership(OwnershipEvent::PayloadRetired {
                identity: *identity,
            })
        }));
        expected.extend(
            (1..=builders).rev().map(|index| {
                Event::Ownership(OwnershipEvent::SpeculativeRetired(BuilderId(index)))
            }),
        );
        expected.push(Event::Ownership(OwnershipEvent::RootRestored));
        expected.push(Event::RootReleased);
        assert_eq!(retired, expected);
        assert_eq!(journal.taken_index.get(), Some(owners - 1));
        assert_eq!(
            journal.taken_identity.get(),
            created.last().map(|(_, identity)| *identity)
        );
        assert_eq!(journal.semantic_entries.get(), 1);
        assert!(!journal.parent_resumed.get());
        assert!(!journal.driver_complete.get());
        assert_eq!(journal.child_queued.get(), queued);
        assert_eq!(journal.parent_pending_with_child.get(), queued);
        assert!(!journal.child_live.get());
        assert!(!events.iter().any(|event| matches!(
            event,
            Event::Ownership(
                OwnershipEvent::SlotRetired { .. } | OwnershipEvent::InvocationCompleted { .. }
            )
        )));
    }

    fn interrupted(
        kind: Kind,
        mode: Mode,
        literal: &str,
        error: bool,
        preexisting: bool,
    ) -> anyhow::Result<()> {
        let db = database(ArgumentBoundary::Committed, literal)?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let env = ProgramEnvironment::from_file(file);
        let (mut builder, data, storage, binding) = prepare(&db, &module, &env)?;
        if preexisting {
            builder.setup_expression_cache();
        }
        let journal = Rc::new(Journal::default());
        let mut root = Root::new(builder, binding, journal.clone());
        let syntax_storage = local::string_annotation::OrdinaryStorage::new();
        let syntax = &syntax_storage;
        let mut invocation = LocalInvocation::new(root.get_mut()?);
        observe(&mut invocation, &journal);
        let owner = install(&mut invocation.owners, BuilderId::ROOT, data, storage)?;
        let operation = reach(&mut invocation, owner, kind)?;
        let reply = match &operation {
            Operation::Resume(_, ty) => Some(*ty),
            _ => None,
        };
        let expected_result = (kind == Kind::Finish).then_some(if error {
            Err(CallErrorKind::BindingError)
        } else {
            Ok(())
        });
        let builders = invocation.builders.speculative.len();
        let temporary_cache = invocation
            .builders
            .builder(BuilderId::ROOT)
            .expression_cache
            .as_ref()
            .map(Rc::downgrade);
        let effects = Effects {
            kind,
            mode,
            expected_result,
            reply,
            endpoint: None,
            journal: journal.clone(),
        };
        let outcome = try_poll_immediate(run(invocation, operation, &effects, syntax));
        assert_eq!(
            outcome,
            if mode == Mode::Pending {
                Poll::Pending
            } else {
                Poll::Ready(Err(Refused::AfterTake))
            }
        );
        drop(root);
        if !preexisting {
            assert!(temporary_cache.and_then(|cache| cache.upgrade()).is_none());
        }
        assert_retirement(&journal, 1, builders, false);
        assert_eq!(
            journal.driver_advance.get(),
            usize::from(kind != Kind::Resume)
        );
        assert_eq!(
            journal.driver_resume.get(),
            usize::from(kind == Kind::Resume)
        );
        assert_eq!(
            journal.driver_finish.get(),
            usize::from(kind == Kind::Finish)
        );
        println!(
            "TAKEN_ARGUMENT_CONTROL kind={kind:?} mode={mode:?} error={error} preexisting={preexisting} builders={builders}"
        );
        Ok(())
    }

    #[test]
    fn taken_advance_resume_and_finish_retire_actual_payload_before_the_invocation()
    -> anyhow::Result<()> {
        for preexisting in [false, true] {
            for mode in [Mode::Refuse, Mode::Pending] {
                interrupted(Kind::Advance, mode, "1", false, preexisting)?;
                interrupted(Kind::Resume, mode, "1", false, preexisting)?;
                interrupted(Kind::Finish, mode, "1", false, preexisting)?;
                interrupted(Kind::Finish, mode, "1.0", true, preexisting)?;
            }
            interrupted(Kind::Finish, Mode::RefuseRetirement, "1", false, preexisting)?;
            interrupted(Kind::Finish, Mode::RefuseRetirement, "1.0", true, preexisting)?;
        }
        Ok(())
    }

    fn interrupted_specialization(kind: Kind, mode: Mode, preexisting: bool) -> anyhow::Result<()> {
        let db = specialization_database()?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let env = ProgramEnvironment::from_file(file);
        let (mut builder, data, storage, binding) = prepare(&db, &module, &env)?;
        if preexisting {
            builder.setup_expression_cache();
        }
        builder
            .context
            .inference_flags
            .set(InferenceFlags::DISABLE_INT_FLOAT_SPECIAL_CASE, preexisting);
        builder
            .context
            .inference_flags
            .set(InferenceFlags::IN_VALID_UNPACK_CONTEXT, false);
        assert!(
            !builder
                .inference_flags()
                .contains(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR)
        );
        let journal = Rc::new(Journal::default());
        let mut root = Root::new(builder, binding, journal.clone());
        let syntax_storage = local::string_annotation::OrdinaryStorage::new();
        let syntax = &syntax_storage;
        let mut invocation = LocalInvocation::new(root.get_mut()?);
        observe(&mut invocation, &journal);
        let outer = install(&mut invocation.owners, BuilderId::ROOT, data, storage)?;
        let inner = nested_specialization(&mut invocation, outer)?;
        let operation = reach_specialization(&mut invocation, inner, kind)?;
        let Some(OwnerSlot::Specialization(payload)) = invocation.owners.slots.last() else {
            anyhow::bail!("missing retained specialization before interruption");
        };
        let live_flags = invocation
            .builders
            .builder(payload.builder)
            .inference_flags();
        assert!(live_flags.contains(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR));
        assert_eq!(
            live_flags.contains(InferenceFlags::DISABLE_INT_FLOAT_SPECIAL_CASE),
            preexisting,
        );
        assert_eq!(
            live_flags.contains(InferenceFlags::IN_VALID_UNPACK_CONTEXT),
            kind == Kind::Resume,
        );
        let reply = match &operation {
            Operation::SpecResume(_, ty) => Some(*ty),
            _ => None,
        };
        let builders_at_stop = invocation.builders.speculative.len();
        let temporary_cache = invocation
            .builders
            .builder(BuilderId::ROOT)
            .expression_cache
            .as_ref()
            .map(Rc::downgrade);
        assert_eq!(
            journal
                .events
                .borrow()
                .iter()
                .filter_map(|event| match event {
                    Event::Ownership(OwnershipEvent::Created { kind, .. }) => Some(*kind),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [OwnerKind::Argument, OwnerKind::Specialization],
        );
        if mode == Mode::QueuedChild {
            let observed = journal.clone();
            let admission = Admission;
            let outcome = try_with_attempt(&db, 100_000, || {
                RegistryBuilder::new(&db, &admission)?
                    .seal()?
                    .run(move |endpoint| async move {
                        let effects = Effects {
                            kind,
                            mode,
                            expected_result: None,
                            reply,
                            endpoint: Some(&endpoint),
                            journal: observed.clone(),
                        };
                        let result = Observed {
                            future: Some(Box::pin(run(invocation, operation, &effects, syntax))),
                            journal: observed.clone(),
                        }
                        .await;
                        observed.parent_resumed.set(true);
                        result.map_err(|_| {
                            RunError::Contract("taken specialization unexpectedly returned")
                        })
                    })
            });
            assert!(
                matches!(
                    outcome,
                    Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
                ),
                "{outcome:?}"
            );
        } else {
            let effects = Effects {
                kind,
                mode,
                expected_result: None,
                reply,
                endpoint: None,
                journal: journal.clone(),
            };
            assert_eq!(
                try_poll_immediate(run(invocation, operation, &effects, syntax)),
                if mode == Mode::Pending {
                    Poll::Pending
                } else {
                    Poll::Ready(Err(Refused::AfterTake))
                },
            );
        }
        drop(root);
        if !preexisting {
            assert!(temporary_cache.and_then(|cache| cache.upgrade()).is_none());
        }
        assert_retirement(&journal, 2, builders_at_stop, mode == Mode::QueuedChild);
        assert_eq!(
            journal.driver_advance.get(),
            usize::from(kind != Kind::Resume)
        );
        assert_eq!(
            journal.driver_resume.get(),
            usize::from(kind == Kind::Resume)
        );
        assert_eq!(
            journal.driver_finish.get(),
            usize::from(kind == Kind::Finish)
        );
        Ok(())
    }

    #[test]
    fn taken_specialization_phases_retire_before_pending_call_and_root() -> anyhow::Result<()> {
        for preexisting in [false, true] {
            for mode in [Mode::Refuse, Mode::Pending] {
                for kind in [Kind::Advance, Kind::Resume, Kind::Finish] {
                    interrupted_specialization(kind, mode, preexisting)?;
                }
            }
        }
        Ok(())
    }

    #[test]
    fn queued_child_retires_before_taken_specialization_and_pending_call() -> anyhow::Result<()> {
        for preexisting in [false, true] {
            interrupted_specialization(Kind::Resume, Mode::QueuedChild, preexisting)?;
        }
        Ok(())
    }

    struct Admission;
    impl ExecutionAdmission for Admission {
        fn admit(&self, _: ExecutionWork) -> RunResult<()> {
            Ok(())
        }
    }

    // The outer call reaches its actual pending nested-Call argument before the inner owner is
    // created. This preparation supplies no semantic result to the controlled taken operation.
    fn nested<'db, 'ast, 'expr>(
        invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr>,
        mut outer: ActiveArgument,
    ) -> anyhow::Result<ActiveArgument> {
        for _ in 0..256 {
            let Ok(step) = local::owned_argument_step_sync(
                outer,
                &mut invocation.owners,
                &mut invocation.builders,
                &OrdinaryOwnedArgumentEffects::default(),
            );
            match step {
                ArgumentStep::Continue(next) => outer = next,
                ArgumentStep::Infer {
                    pending,
                    builder,
                    expression,
                    tcx,
                    policy,
                } => {
                    anyhow::ensure!(matches!(policy, arguments::ArgumentPolicy::Ordinary));
                    let ast::Expr::Call(inner) = expression else {
                        anyhow::bail!("outer argument is not the real nested call");
                    };
                    anyhow::ensure!(invocation.owners.pending(&pending).is_some());
                    invocation.frames.push(Frame::Argument(pending));
                    let selected = invocation.builders.get_mut(builder);
                    let callable = selected.infer_callee(&inner.func);
                    let Ok(start) = call::start_sync(
                        selected,
                        inner,
                        callable,
                        tcx,
                        call::CallFacts,
                        &call::OrdinaryCallEffects,
                    );
                    let call::Start::Prepare(data) = start else {
                        anyhow::bail!("nested call did not require arguments");
                    };
                    let arguments = selected.prepare_call_arguments(&inner.arguments);
                    let Ok(prepared) = local::owned_prepared_call_sync(
                        &mut invocation.builders,
                        &mut invocation.owners,
                        builder,
                        data,
                        arguments,
                        &OrdinaryOwnedArgumentEffects::default(),
                    );
                    let PreparedCall::Arguments(inner) = prepared else {
                        anyhow::bail!("nested owner was not installed");
                    };
                    let Operation::Advance(inner) = reach(invocation, inner, Kind::Advance)? else {
                        anyhow::bail!("nested active phase missing");
                    };
                    assert_eq!(invocation.owners.slots.len(), 2);
                    assert!(
                        matches!(invocation.owners.slots.first(), Some(OwnerSlot::Argument(payload)) if matches!(payload.phase, ArgumentPhase::Pending(_)))
                    );
                    return Ok(inner);
                }
                ArgumentStep::Complete(_) => {
                    anyhow::bail!("outer call completed without its nested argument")
                }
            }
        }
        anyhow::bail!("outer argument owner did not become pending")
    }

    struct Observed<F> {
        future: Option<Pin<Box<F>>>,
        journal: Rc<Journal>,
    }

    impl<F: Future> Future for Observed<F> {
        type Output = F::Output;
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = self.get_mut();
            let Some(future) = this.future.as_mut() else {
                return Poll::Pending;
            };
            let result = future.as_mut().poll(cx);
            if result.is_pending() && this.journal.child_live.get() {
                this.journal.parent_pending_with_child.set(true);
            }
            result
        }
    }

    impl<F> Drop for Observed<F> {
        fn drop(&mut self) {
            assert!(!self.journal.child_live.get());
            drop(self.future.take());
        }
    }

    #[test]
    fn queued_child_retires_before_taken_inner_outer_payloads_and_root() -> anyhow::Result<()> {
        let db = database(ArgumentBoundary::NarrowCache, "1")?;
        let file = db.program_file(system_path_to_file(&db, PATH)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let env = ProgramEnvironment::from_file(file);
        for (preexisting, kind) in [
            (false, Kind::Advance),
            (true, Kind::Advance),
            (false, Kind::Finish),
            (true, Kind::Finish),
        ] {
            let (mut builder, data, storage, binding) = prepare(&db, &module, &env)?;
            if preexisting {
                builder.setup_expression_cache();
            }
            let journal = Rc::new(Journal::default());
            let mut root = Root::new(builder, binding, journal.clone());
            let syntax_storage = local::string_annotation::OrdinaryStorage::new();
            let syntax = &syntax_storage;
            let mut invocation = LocalInvocation::new(root.get_mut()?);
            observe(&mut invocation, &journal);
            // All semantic preparation finishes before creating the controlled registry parent.
            let outer = install(&mut invocation.owners, BuilderId::ROOT, data, storage)?;
            let inner = nested(&mut invocation, outer)?;
            let operation = if kind == Kind::Finish {
                reach(&mut invocation, inner, kind)?
            } else {
                Operation::Advance(inner)
            };
            let builders_at_stop = invocation.builders.speculative.len();
            let observed = journal.clone();
            let admission = Admission;
            let outcome = try_with_attempt(&db, 100_000, || {
                RegistryBuilder::new(&db, &admission)?
                    .seal()?
                    .run(move |endpoint| async move {
                        let effects = Effects {
                            kind,
                            mode: Mode::QueuedChild,
                            expected_result: (kind == Kind::Finish).then_some(Ok(())),
                            reply: None,
                            endpoint: Some(&endpoint),
                            journal: observed.clone(),
                        };
                        let result = Observed {
                            future: Some(Box::pin(run(
                                invocation,
                                operation,
                                &effects,
                                syntax,
                            ))),
                            journal: observed.clone(),
                        }
                        .await;
                        observed.parent_resumed.set(true);
                        result.map_err(|_| RunError::Contract("taken parent unexpectedly returned"))
                    })
            });
            assert!(
                matches!(
                    outcome,
                    Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
                ),
                "{outcome:?}"
            );
            drop(root);
            assert!(builders_at_stop >= 2);
            assert_eq!(journal.driver_advance.get(), 1);
            assert_eq!(journal.driver_resume.get(), 0);
            assert_eq!(
                journal.driver_finish.get(),
                usize::from(kind == Kind::Finish)
            );
            assert_retirement(&journal, 2, builders_at_stop, true);
        }
        Ok(())
    }
}

#[cfg(feature = "experimental-analysis")]
pub(super) mod resume_allocation {
    use std::cell::{Cell, RefCell};
    use std::panic::AssertUnwindSafe;
    use std::rc::Rc;

    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use ruff_db::testing::{
        assert_function_query_was_not_run_by_name, find_will_execute_event_by_name,
    };
    use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};
    use salsa::plumbing::AsId;
    use salsa::prepared_source_probe::assert_no_active_attempt;

    use super::super::{
        ArgumentPhase, LocalInvocation, LocalOwners, OwnerKind, OwnerSlot, OwnershipEvent,
        PendingArgument, PendingSpecialization, SpecializationPhase,
    };
    use crate::analysis::{
        AnalysisIncomplete, AnalysisOutcome, AnalysisPolicy, PreparedAnalysisFile,
        expression_type_with_policy, prepare_file,
    };
    use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
    use crate::types::ClassLiteral;
    use crate::types::constraints::ConstraintSetBuilder;
    use crate::types::infer::builder::source_definition::controlled::observations;
    use crate::types::infer::builder::{
        Db, ExpressionNodeKey, ProgramEnvironment, PythonVersion, Type, TypeContext, ast,
        infer_expression_types,
    };
    use crate::types::infer::{
        InferExpression, definition_inference_ingredient, expression_inference_ingredient,
        infer_definition_types,
    };
    use crate::types::relation::source::resources::observations as invocation_observations;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Event {
        Ownership(OwnershipEvent),
        Before { identity: usize },
        Admitted { remaining_work: usize },
        SpecializationBefore { identity: usize, builder: usize },
        SpecializationAdmitted { remaining_work: usize },
        SpecializationResumed { builder: usize },
        SpecializationFinishing { builder: usize },
        SpecializationFinished { builder: usize },
    }

    thread_local! {
        static EVENTS: RefCell<Option<Rc<RefCell<Vec<Event>>>>> = const { RefCell::new(None) };
        static CANCEL_SPECIALIZATION: Cell<bool> = const { Cell::new(false) };
    }

    struct Observation(Rc<RefCell<Vec<Event>>>);

    impl Observation {
        fn new() -> Self {
            let events = Rc::new(RefCell::new(Vec::new()));
            EVENTS.with_borrow_mut(|current| {
                assert!(current.replace(events.clone()).is_none());
            });
            Self(events)
        }

        fn admitted_work(&self) -> Option<usize> {
            self.0.borrow().iter().find_map(|event| match event {
                Event::Admitted { remaining_work } => {
                    Some(funded().semantic_work_limit - remaining_work)
                }
                _ => None,
            })
        }

        fn specialization_admitted_work(&self) -> Option<usize> {
            self.0.borrow().iter().find_map(|event| match event {
                Event::SpecializationAdmitted { remaining_work } => {
                    Some(funded().semantic_work_limit - remaining_work)
                }
                _ => None,
            })
        }
    }

    impl Drop for Observation {
        fn drop(&mut self) {
            EVENTS.with_borrow_mut(|current| *current = None);
            CANCEL_SPECIALIZATION.set(false);
        }
    }

    pub(in crate::types::infer::builder::local) fn observe_invocation<
        'root,
        'db,
        'ast,
        'expr,
        B,
    >(
        mut invocation: LocalInvocation<'root, 'db, 'ast, 'expr, B>,
    ) -> LocalInvocation<'root, 'db, 'ast, 'expr, B> {
        EVENTS.with_borrow(|events| {
            if let Some(events) = events {
                let events = events.clone();
                invocation.observe_ownership(Rc::new(move |event| {
                    events.borrow_mut().push(Event::Ownership(event));
                }));
            }
        });
        invocation
    }

    pub(in crate::types::infer::builder::local) fn before<B>(
        owners: &LocalOwners<'_, '_, B>,
        owner: &PendingArgument,
    ) {
        EVENTS.with_borrow(|events| {
            if let Some(events) = events {
                let Some(OwnerSlot::Argument(payload)) = owners.slots.get(owner.0) else {
                    panic!("resume allocation must begin with an initialized owner");
                };
                assert!(matches!(payload.phase, ArgumentPhase::Pending(_)));
                let Some(lifetime) = &payload.lifetime else {
                    panic!("the actual pending payload must be observed");
                };
                events.borrow_mut().push(Event::Before {
                    identity: lifetime.identity,
                });
            }
        });
    }

    pub(in crate::types::infer::builder::local) fn admitted(db: &dyn Db) {
        EVENTS.with_borrow(|events| {
            if let Some(events) = events {
                let Some(remaining_work) =
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(db)
                else {
                    panic!("the resume factory must run in a funded attempt");
                };
                events.borrow_mut().push(Event::Admitted { remaining_work });
            }
        });
    }

    pub(in crate::types::infer::builder::local) fn before_specialization<
        'db,
        B: std::borrow::Borrow<ConstraintSetBuilder<'db>>,
    >(
        owners: &LocalOwners<'db, '_, B>,
        owner: &PendingSpecialization,
    ) {
        EVENTS.with_borrow(|events| {
            if let Some(events) = events {
                let Some(OwnerSlot::Specialization(payload)) = owners.slots.get(owner.0) else {
                    panic!("resume allocation must begin with an initialized specialization");
                };
                let SpecializationPhase::Pending(pending) = &payload.phase else {
                    panic!("resume allocation must begin with a pending specialization");
                };
                let Some(lifetime) = &payload.lifetime else {
                    panic!("the actual pending payload must be observed");
                };
                let constraints = std::borrow::Borrow::<ConstraintSetBuilder<'db>>::borrow(
                    pending.constraint_builder(),
                );
                events.borrow_mut().push(Event::SpecializationBefore {
                    identity: lifetime.identity,
                    builder: std::ptr::from_ref(constraints).addr(),
                });
            }
        });
    }

    pub(in crate::types::infer::builder::local) fn admitted_specialization(db: &dyn Db) {
        EVENTS.with_borrow(|events| {
            if let Some(events) = events {
                let Some(remaining_work) =
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(db)
                else {
                    panic!("the resume factory must run in a funded attempt");
                };
                events
                    .borrow_mut()
                    .push(Event::SpecializationAdmitted { remaining_work });
            }
        });
        if CANCEL_SPECIALIZATION.replace(false) {
            db.cancellation_token().cancel();
        }
    }

    pub(in crate::types::infer::builder::local) fn specialization_resumed(
        builder: &ConstraintSetBuilder<'_>,
    ) {
        specialization_event(Event::SpecializationResumed {
            builder: std::ptr::from_ref(builder).addr(),
        });
    }

    pub(in crate::types::infer::builder::local) fn specialization_finishing(
        builder: &ConstraintSetBuilder<'_>,
    ) {
        specialization_event(Event::SpecializationFinishing {
            builder: std::ptr::from_ref(builder).addr(),
        });
    }

    pub(in crate::types::infer::builder::local) fn specialization_finished(
        builder: &ConstraintSetBuilder<'_>,
    ) {
        specialization_event(Event::SpecializationFinished {
            builder: std::ptr::from_ref(builder).addr(),
        });
    }

    fn specialization_event(event: Event) {
        EVENTS.with_borrow(|events| {
            if let Some(events) = events {
                events.borrow_mut().push(event);
            }
        });
    }

    fn funded() -> AnalysisPolicy {
        AnalysisPolicy {
            semantic_work_limit: 1_000_000,
            requested_bytes_limit: 16 * 1024 * 1024,
        }
    }

    fn fixture() -> anyhow::Result<TestDb> {
        let mut db = setup_db();
        db.write_file(
            "src/main.py",
            "def choose(value):\n    return value\nchoose(True)\n",
        )?;
        Ok(db)
    }

    fn prepare(db: &TestDb) -> anyhow::Result<PreparedAnalysisFile<'_>> {
        prepare_file(db, system_path_to_file(db, "src/main.py")?)
            .map_err(|error| anyhow::anyhow!("{error:?}"))
    }

    fn expression_key(prepared: &PreparedAnalysisFile<'_>) -> ExpressionNodeKey {
        match prepared.parsed_module().syntax().body.last() {
            Some(ast::Stmt::Expr(statement)) => statement.value.as_ref().into(),
            Some(ast::Stmt::Assign(assignment)) => assignment.value.as_ref().into(),
            _ => panic!("fixture must end with an expression or assignment"),
        }
    }

    fn infer<'db>(
        prepared: &PreparedAnalysisFile<'db>,
        policy: &AnalysisPolicy,
    ) -> anyhow::Result<AnalysisOutcome<Type<'db>>> {
        expression_type_with_policy(prepared, expression_key(prepared), policy)
            .map_err(|error| anyhow::anyhow!("{error:?}"))
    }

    fn cold_admission(policy: AnalysisPolicy) -> anyhow::Result<Option<usize>> {
        let db = fixture()?;
        let prepared = prepare(&db)?;
        observations::reset(None);
        let observed = Observation::new();
        let result = infer(&prepared, &policy)?;
        if policy == funded() {
            assert_eq!(result, AnalysisOutcome::Complete(Type::unknown()));
            assert!(observed.admitted_work().is_some());
        } else {
            let expected_reason = if policy.semantic_work_limit < funded().semantic_work_limit {
                AnalysisIncomplete::WorkLimit
            } else {
                AnalysisIncomplete::RequestedAllocationLimit
            };
            match result {
                AnalysisOutcome::Complete(ty) => assert_eq!(ty, Type::unknown()),
                AnalysisOutcome::Incomplete { reason, .. } => assert_eq!(reason, expected_reason),
            }
        }
        if observed.admitted_work().is_some() {
            assert_eq!(
                observed
                    .0
                    .borrow()
                    .iter()
                    .filter(|event| matches!(event, Event::Before { .. }))
                    .count(),
                1,
            );
            assert_eq!(
                observed
                    .0
                    .borrow()
                    .iter()
                    .filter(|event| matches!(event, Event::Admitted { .. }))
                    .count(),
                1,
            );
        }
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        Ok(observed.admitted_work())
    }

    fn specialization_fixture() -> anyhow::Result<TestDb> {
        specialization_fixture_with_argument("Leaf")
    }

    fn specialization_fixture_with_argument(argument: &str) -> anyhow::Result<TestDb> {
        Ok(TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file(
                "src/main.pyi",
                &format!("from typing import Generic, TypeVar\n\
                 T = TypeVar(\"T\")\n\
                 class Box(Generic[T]): ...\n\
                 class Leaf: ...\n\
                 left = right = Box[{argument}]\n"),
            )
            .build()?)
    }

    fn prepare_specialization(
        db: &TestDb,
        preceding_attempts: usize,
    ) -> anyhow::Result<PreparedAnalysisFile<'_>> {
        let prepared = prepare_file(db, system_path_to_file(db, "src/main.pyi")?)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        let revision = salsa::plumbing::current_revision(db);
        for _ in 0..preceding_attempts {
            observations::reset(None);
            assert_eq!(
                infer(&prepared, &funded())?,
                AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                },
            );
            assert_eq!(observations::counts().0, 0);
            assert_no_active_attempt();
            assert_eq!(salsa::plumbing::current_revision(db), revision);
        }
        Ok(prepared)
    }

    fn assert_specialization_builder(observed: &Observation) {
        let events = observed.0.borrow();
        let Some((position, identity, builder)) =
            events
                .iter()
                .enumerate()
                .find_map(|(position, event)| match event {
                    Event::SpecializationBefore { identity, builder } => {
                        Some((position, *identity, *builder))
                    }
                    _ => None,
                })
        else {
            panic!("specialization must reach its resume allocation");
        };
        let invocations = invocation_observations::invocation_snapshot();
        assert!(invocations.count <= invocations.events.len());
        assert_eq!(
            invocations
                .events
                .iter()
                .flatten()
                .filter(|event| {
                    event.stage == invocation_observations::InvocationStage::Allocated
                        && event.builder == builder
                })
                .count(),
            1,
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::SpecializationBefore { .. }
                        | Event::SpecializationResumed { .. }
                        | Event::SpecializationFinishing { .. }
                        | Event::SpecializationFinished { .. }
                ))
                .copied()
                .collect::<Vec<_>>(),
            [
                Event::SpecializationBefore { identity, builder },
                Event::SpecializationResumed { builder },
                Event::SpecializationFinishing { builder },
                Event::SpecializationFinished { builder },
            ],
        );
        let remaining = &events[position + 1..];
        assert_eq!(
            remaining
                .iter()
                .filter(|event| matches!(event, Event::SpecializationAdmitted { .. }))
                .count(),
            1,
        );
        let Some(finished) = remaining
            .iter()
            .position(|event| *event == Event::SpecializationFinished { builder })
        else {
            panic!("specialization must finish before its payload is retired");
        };
        assert_eq!(
            remaining.get(finished + 1),
            Some(&Event::Ownership(OwnershipEvent::PayloadRetired {
                identity
            })),
        );
        assert_eq!(
            remaining
                .iter()
                .filter(|event| {
                    **event == Event::Ownership(OwnershipEvent::PayloadRetired { identity })
                })
                .count(),
            1,
        );
    }

    fn assert_nested_specialization_builders(
        observed: &Observation,
        completed: bool,
        cancel: bool,
    ) {
        let events = observed.0.borrow();
        let Some(start) = events.iter().position(|event| {
            matches!(
                event,
                Event::Ownership(OwnershipEvent::Created {
                    kind: OwnerKind::Specialization,
                    ..
                })
            )
        }) else {
            panic!("nested specializations must create their owners: events={events:?}");
        };
        let events = &events[start..];
        let created = events
            .iter()
            .filter_map(|event| match event {
                Event::Ownership(OwnershipEvent::Created {
                    kind: OwnerKind::Specialization,
                    identity,
                    index,
                    ..
                }) => Some((*identity, *index)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(outer, 0), (inner, 1)] = created.as_slice() else {
            panic!(
                "nested specializations must share one owner stack: cancel={cancel}, completed={completed}, events={events:?}"
            );
        };
        assert_ne!(outer, inner);
        let retired = events
            .iter()
            .filter_map(|event| match event {
                Event::Ownership(OwnershipEvent::PayloadRetired { identity }) => Some(*identity),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            retired,
            [*inner, *outer],
            "cancel={cancel}, completed={completed}, events={events:?}"
        );
        if !completed {
            let retired_outer = events
                .iter()
                .position(|event| {
                    *event == Event::Ownership(OwnershipEvent::PayloadRetired { identity: *outer })
                })
                .unwrap();
            assert!(
                events[retired_outer + 1..]
                    .contains(&Event::Ownership(OwnershipEvent::RootRestored)),
                "interrupted nested owner journal: cancel={cancel}, events={events:?}"
            );
            return;
        }
        assert!(
            events.iter().any(|event| matches!(
                event,
                Event::Ownership(OwnershipEvent::InvocationCompleted {
                    arguments: 0,
                    frames: 0,
                    speculative: 0,
                    ..
                })
            )),
            "completed nested owner journal: cancel={cancel}, events={events:?}"
        );
        let before = events
            .iter()
            .filter_map(|event| match event {
                Event::SpecializationBefore { identity, builder } => Some((*identity, *builder)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(first, inner_builder), (second, outer_builder)] = before.as_slice() else {
            panic!("both retained builders must resume: cancel={cancel}, events={events:?}");
        };
        assert_eq!((first, second), (inner, outer));
        assert_ne!(inner_builder, outer_builder);
        let invocations = invocation_observations::invocation_snapshot();
        assert!(invocations.count <= invocations.events.len());
        for &(identity, builder) in &before {
            assert_eq!(
                invocations
                    .events
                    .iter()
                    .flatten()
                    .filter(|event| {
                        event.stage == invocation_observations::InvocationStage::Allocated
                            && event.builder == builder
                    })
                    .count(),
                1
            );
            let phases = events
                .iter()
                .filter(|event| match event {
                    Event::SpecializationBefore { builder: value, .. }
                    | Event::SpecializationResumed { builder: value }
                    | Event::SpecializationFinishing { builder: value }
                    | Event::SpecializationFinished { builder: value } => *value == builder,
                    _ => false,
                })
                .copied()
                .collect::<Vec<_>>();
            assert_eq!(
                phases,
                [
                    Event::SpecializationBefore { identity, builder },
                    Event::SpecializationResumed { builder },
                    Event::SpecializationFinishing { builder },
                    Event::SpecializationFinished { builder },
                ]
            );
        }
        let inner_finished = events
            .iter()
            .position(|event| {
                *event
                    == Event::SpecializationFinished {
                        builder: *inner_builder,
                    }
            })
            .unwrap();
        let outer_resumed = events
            .iter()
            .position(|event| {
                *event
                    == Event::SpecializationBefore {
                        identity: *outer,
                        builder: *outer_builder,
                    }
            })
            .unwrap();
        assert!(inner_finished < outer_resumed);
    }

    #[test]
    fn production_nested_specialization_resume_refusal_drains_both_owners_and_retries()
    -> anyhow::Result<()> {
        let measured_db = specialization_fixture_with_argument("Box[Leaf]")?;
        let measured_prepared = prepare_specialization(&measured_db, 0)?;
        let measured_revision = salsa::plumbing::current_revision(&measured_db);
        let mut measured = None;
        for preceding_attempts in 0..4 {
            observations::reset(None);
            invocation_observations::reset_invocations();
            let observed = Observation::new();
            match infer(&measured_prepared, &funded())? {
                AnalysisOutcome::Complete(Type::GenericAlias(_)) => {
                    assert_nested_specialization_builders(&observed, true, false);
                    measured = Some((
                        preceding_attempts,
                        observed.specialization_admitted_work().unwrap(),
                    ));
                }
                AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    ..
                } => {}
                other => anyhow::bail!("unexpected nested specialization result: {other:?}"),
            }
            assert_eq!(observations::counts().0, 0);
            assert_no_active_attempt();
            assert_eq!(
                salsa::plumbing::current_revision(&measured_db),
                measured_revision
            );
            if measured.is_some() {
                break;
            }
        }
        let Some((preceding_attempts, work)) = measured else {
            anyhow::bail!(
                "nested specialization did not complete within four funded caller attempts"
            );
        };
        assert!(work > 0);
        for cancel in [false, true] {
            let db = specialization_fixture_with_argument("Box[Leaf]")?;
            let prepared = prepare_specialization(&db, preceding_attempts)?;
            let revision = salsa::plumbing::current_revision(&db);
            let expression = prepared
                .semantic_index()
                .expression(expression_key(&prepared));
            observations::reset(None);
            invocation_observations::reset_invocations();
            let completed_before_cancellation;
            {
                let observed = Observation::new();
                CANCEL_SPECIALIZATION.set(cancel);
                let policy = if cancel {
                    funded()
                } else {
                    AnalysisPolicy {
                        semantic_work_limit: work - 1,
                        ..funded()
                    }
                };
                let result =
                    salsa::Cancelled::catch(AssertUnwindSafe(|| infer(&prepared, &policy)));
                match result {
                    Err(salsa::Cancelled::Local) if cancel => {}
                    Ok(Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        completed: (),
                    })) if !cancel => {}
                    other => anyhow::bail!("cancel={cancel}: {other:?}"),
                }
                // Cycle masking can defer native cancellation until the local driver completes.
                completed_before_cancellation = cancel && {
                    let events = observed.0.borrow();
                    events
                        .iter()
                        .rposition(|event| {
                            matches!(
                                event,
                                Event::Ownership(OwnershipEvent::PayloadRetired { .. })
                            )
                        })
                        .is_some_and(|retired| {
                            events[retired + 1..].iter().any(|event| {
                                matches!(
                                    event,
                                    Event::Ownership(OwnershipEvent::InvocationCompleted {
                                        arguments: 0,
                                        frames: 0,
                                        speculative: 0,
                                        ..
                                    })
                                )
                            })
                        })
                };
                assert_nested_specialization_builders(
                    &observed,
                    completed_before_cancellation,
                    cancel,
                );
                assert_eq!(
                    observed.specialization_admitted_work().is_some(),
                    cancel,
                    "cancel={cancel}, events={:?}",
                    observed.0.borrow()
                );
            }
            assert_eq!(observations::counts().0, 0);
            assert_no_active_attempt();
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            if !cancel {
                assert_eq!(
                    FinalSourceMemo::certify(
                        &db as &dyn Db,
                        expression_inference_ingredient(&db),
                        InferExpression::Bare(expression).as_id(),
                    )
                    .map(|_| ()),
                    Err(FinalSourceError::MissingMemo)
                );
            }
            let published = completed_before_cancellation.then(|| {
                assert!(
                    FinalSourceMemo::certify(
                        &db as &dyn Db,
                        expression_inference_ingredient(&db),
                        InferExpression::Bare(expression).as_id(),
                    )
                    .is_ok(),
                    "a completed native-cancelled driver must retain its canonical result"
                );
                let canonical = infer_expression_types(&db, expression, TypeContext::default());
                let ty = canonical.expression_type(expression_key(&prepared));
                assert!(matches!(ty, Type::GenericAlias(_)));
                ty
            });
            let mut completed = None;
            for _ in 0..4 {
                observations::reset(None);
                invocation_observations::reset_invocations();
                let observed = Observation::new();
                match infer(&prepared, &funded())? {
                    AnalysisOutcome::Complete(ty @ Type::GenericAlias(_)) => {
                        if let Some(published) = published {
                            assert_eq!(ty, published);
                            assert!(observed.0.borrow().is_empty());
                        } else {
                            assert_nested_specialization_builders(&observed, true, false);
                        }
                        completed = Some(ty);
                    }
                    AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        ..
                    } => {}
                    other => anyhow::bail!("unexpected retry result: {other:?}"),
                }
                assert_eq!(observations::counts().0, 0);
                assert_no_active_attempt();
                assert_eq!(salsa::plumbing::current_revision(&db), revision);
                if completed.is_some() {
                    break;
                }
            }
            let Some(completed) = completed else {
                anyhow::bail!("funded retry did not complete nested specialization");
            };
            assert_eq!(
                infer(&prepared, &funded())?,
                AnalysisOutcome::Complete(completed)
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
        Ok(())
    }

    fn cold_specialization_admission(
        policy: AnalysisPolicy,
        preceding_attempts: usize,
    ) -> anyhow::Result<Option<usize>> {
        let db = specialization_fixture()?;
        let prepared = prepare_specialization(&db, preceding_attempts)?;
        observations::reset(None);
        invocation_observations::reset_invocations();
        let observed = Observation::new();
        let result = infer(&prepared, &policy)?;
        match result {
            AnalysisOutcome::Complete(Type::GenericAlias(_)) => {
                assert_specialization_builder(&observed);
                assert!(observed.specialization_admitted_work().is_some());
            }
            AnalysisOutcome::Incomplete { reason, .. } => {
                assert_eq!(
                    reason,
                    if policy.requested_bytes_limit < funded().requested_bytes_limit {
                        AnalysisIncomplete::RequestedAllocationLimit
                    } else {
                        AnalysisIncomplete::WorkLimit
                    },
                );
                if policy == funded() {
                    assert!(observed.specialization_admitted_work().is_none());
                }
            }
            other => anyhow::bail!("unexpected specialization result: {other:?}"),
        }
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        Ok(observed.specialization_admitted_work())
    }

    #[test]
    fn production_specialization_resume_allocation_preserves_builder_and_retries()
    -> anyhow::Result<()> {
        let mut measured = None;
        for preceding_attempts in 0..4 {
            if let Some(work) = cold_specialization_admission(funded(), preceding_attempts)? {
                measured = Some((preceding_attempts, work));
                break;
            }
        }
        let Some((preceding_attempts, work)) = measured else {
            anyhow::bail!("funded production inference did not resume its specialization");
        };
        // Replay the same preparation attempts before each cold probe so the search measures
        // admission of the resume future with its initialized pending owner.
        let mut lower = 0;
        let mut upper = funded().requested_bytes_limit;
        for _ in 0..usize::BITS {
            if lower == upper {
                break;
            }
            let middle = lower + (upper - lower) / 2;
            if cold_specialization_admission(
                AnalysisPolicy {
                    requested_bytes_limit: middle,
                    ..funded()
                },
                preceding_attempts,
            )?
            .is_some()
            {
                upper = middle;
            } else {
                lower = middle + 1;
            }
        }
        assert_eq!(lower, upper);
        assert!(work > 0 && upper > 0);
        for policy in [
            AnalysisPolicy {
                semantic_work_limit: work,
                ..funded()
            },
            AnalysisPolicy {
                requested_bytes_limit: upper,
                ..funded()
            },
        ] {
            assert!(cold_specialization_admission(policy, preceding_attempts)?.is_some());
        }

        for (policy, reason) in [
            (
                AnalysisPolicy {
                    semantic_work_limit: work - 1,
                    ..funded()
                },
                AnalysisIncomplete::WorkLimit,
            ),
            (
                AnalysisPolicy {
                    requested_bytes_limit: upper - 1,
                    ..funded()
                },
                AnalysisIncomplete::RequestedAllocationLimit,
            ),
        ] {
            let db = specialization_fixture()?;
            let prepared = prepare_specialization(&db, preceding_attempts)?;
            let revision = salsa::plumbing::current_revision(&db);
            let expression = prepared
                .semantic_index()
                .expression(expression_key(&prepared));
            let mut events_db = db.clone();
            observations::reset(None);
            invocation_observations::reset_invocations();
            {
                let observed = Observation::new();
                assert_eq!(
                    infer(&prepared, &policy)?,
                    AnalysisOutcome::Incomplete {
                        reason,
                        completed: ()
                    },
                );
                assert!(observed.specialization_admitted_work().is_none());
                let events = observed.0.borrow();
                let Some((position, identity, builder)) =
                    events
                        .iter()
                        .enumerate()
                        .find_map(|(position, event)| match event {
                            Event::SpecializationBefore { identity, builder } => {
                                Some((position, *identity, *builder))
                            }
                            _ => None,
                        })
                else {
                    anyhow::bail!("refusal occurred before the specialization resume allocation");
                };
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| matches!(
                            event,
                            Event::Ownership(OwnershipEvent::Created {
                                kind: OwnerKind::Specialization,
                                ..
                            })
                        ))
                        .count(),
                    1,
                );
                assert!(
                    invocation_observations::invocation_snapshot()
                        .events
                        .iter()
                        .flatten()
                        .any(|event| {
                            event.stage == invocation_observations::InvocationStage::Allocated
                                && event.builder == builder
                        })
                );
                // Refusing the future leaves the payload in its pending slot until cleanup.
                assert_eq!(
                    &events[position + 1..],
                    &[
                        Event::Ownership(OwnershipEvent::PayloadRetired { identity }),
                        Event::Ownership(OwnershipEvent::RootRestored),
                    ],
                );
            }
            assert_eq!(observations::counts().0, 0);
            assert_no_active_attempt();
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_eq!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    expression_inference_ingredient(&db),
                    InferExpression::Bare(expression).as_id(),
                )
                .map(|_| ()),
                Err(FinalSourceError::MissingMemo),
            );

            events_db.take_salsa_events();
            let children = ["Box", "Leaf"].map(|name| {
                let Some(class) = prepared
                    .parsed_module()
                    .syntax()
                    .body
                    .iter()
                    .filter_map(ast::Stmt::as_class_def_stmt)
                    .find(|class| class.name.as_str() == name)
                else {
                    panic!("fixture must define {name}");
                };
                let definition = prepared.semantic_index().expect_single_definition(class);
                assert!(
                    FinalSourceMemo::certify(
                        &db as &dyn Db,
                        definition_inference_ingredient(&db),
                        definition.as_id(),
                    )
                    .is_ok()
                );
                let inference = infer_definition_types(&db, definition);
                let Some(ClassLiteral::Static(class)) = inference.original_class_type(definition)
                else {
                    panic!("fixture must infer the class {name}");
                };
                (definition, inference, class)
            });
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_definition_types",
                None,
                &events_db.take_salsa_events(),
            );

            invocation_observations::reset_invocations();
            let alias = {
                let observed = Observation::new();
                let AnalysisOutcome::Complete(Type::GenericAlias(alias)) =
                    infer(&prepared, &funded())?
                else {
                    anyhow::bail!("funded retry must complete the specialization");
                };
                assert_specialization_builder(&observed);
                alias
            };
            let events = events_db.take_salsa_events();
            assert!(
                find_will_execute_event_by_name(&db, "infer_expression_types_impl", None, &events,)
                    .is_some()
            );
            for (definition, inference, _) in children {
                assert_function_query_was_not_run_by_name(
                    &db,
                    "infer_definition_types",
                    Some(definition.as_id()),
                    &events,
                );
                assert!(std::ptr::eq(
                    inference,
                    infer_definition_types(&db, definition)
                ));
            }
            assert_function_query_was_not_run_by_name(
                &db,
                "static_class_generic_context",
                Some(children[0].2.as_id()),
                &events,
            );
            assert_eq!(alias.origin(&db), children[0].2);
            let specialization = alias.specialization(&db);
            let [Type::NominalInstance(instance)] = specialization.types(&db) else {
                anyhow::bail!("specialization must contain its inferred Leaf argument");
            };
            let env = ProgramEnvironment::from_file(prepared.program_file());
            assert_eq!(
                instance.class_literal(&db, &env),
                ClassLiteral::Static(children[1].2)
            );
            assert_eq!(specialization.materialization_kind(&db), None);
            assert!(specialization.tuple(&db).is_none());
            assert_eq!(
                Some(specialization.generic_context(&db)),
                children[0].2.generic_context(&db)
            );
            let canonical = infer_expression_types(&db, expression, TypeContext::default());
            assert_eq!(
                canonical.expression_type(expression_key(&prepared)),
                Type::GenericAlias(alias),
            );
            assert_eq!(
                infer(&prepared, &funded())?,
                AnalysisOutcome::Complete(Type::GenericAlias(alias))
            );
            assert!(std::ptr::eq(
                canonical,
                infer_expression_types(&db, expression, TypeContext::default()),
            ));
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_expression_types_impl",
                None,
                &events_db.take_salsa_events(),
            );
            assert_eq!(observations::counts().0, 0);
            assert_no_active_attempt();
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
        Ok(())
    }

    #[test]
    fn production_resume_allocation_refusal_preserves_pending_owner_and_retries()
    -> anyhow::Result<()> {
        let Some(work) = cold_admission(funded())? else {
            anyhow::bail!("funded production inference did not resume its argument");
        };
        // Each probe starts cold. The predicate marks the admitted factory itself, so later
        // allocations and source-preparation retries cannot stand in for this boundary.
        let mut lower = 0;
        let mut upper = funded().requested_bytes_limit;
        for _ in 0..usize::BITS {
            if lower == upper {
                break;
            }
            let middle = lower + (upper - lower) / 2;
            if cold_admission(AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            })?
            .is_some()
            {
                upper = middle;
            } else {
                lower = middle + 1;
            }
        }
        assert_eq!(lower, upper);
        assert!(work > 0 && upper > 0);
        for policy in [
            AnalysisPolicy {
                semantic_work_limit: work,
                ..funded()
            },
            AnalysisPolicy {
                requested_bytes_limit: upper,
                ..funded()
            },
        ] {
            assert!(cold_admission(policy)?.is_some());
        }
        for (policy, reason) in [
            (
                AnalysisPolicy {
                    semantic_work_limit: work - 1,
                    ..funded()
                },
                AnalysisIncomplete::WorkLimit,
            ),
            (
                AnalysisPolicy {
                    requested_bytes_limit: upper - 1,
                    ..funded()
                },
                AnalysisIncomplete::RequestedAllocationLimit,
            ),
        ] {
            let db = fixture()?;
            let prepared = prepare(&db)?;
            let revision = salsa::plumbing::current_revision(&db);
            let mut events_db = db.clone();
            observations::reset(None);
            {
                let observed = Observation::new();
                assert_eq!(
                    infer(&prepared, &policy)?,
                    AnalysisOutcome::Incomplete {
                        reason,
                        completed: ()
                    },
                );
                assert!(observed.admitted_work().is_none());
                let events = observed.0.borrow();
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| matches!(
                            event,
                            Event::Ownership(OwnershipEvent::Created { .. })
                        ))
                        .count(),
                    1,
                );
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| matches!(
                            event,
                            Event::Ownership(OwnershipEvent::PayloadRetired { .. })
                        ))
                        .count(),
                    1,
                );
                let Some((position, identity)) =
                    events
                        .iter()
                        .enumerate()
                        .find_map(|(i, event)| match event {
                            Event::Before { identity } => Some((i, *identity)),
                            _ => None,
                        })
                else {
                    anyhow::bail!("refusal occurred before the resume allocation");
                };
                // The initialized slot owns the payload throughout refusal. No take or
                // reinstall occurs between this boundary and its one actual disposal.
                assert_eq!(
                    &events[position + 1..],
                    &[
                        Event::Ownership(OwnershipEvent::PayloadRetired { identity }),
                        Event::Ownership(OwnershipEvent::RootRestored),
                    ],
                );
            }
            assert_eq!(observations::counts().0, 0);
            assert_eq!(observations::argument_progress().0, 0);
            assert_eq!(observations::call_progress().0, 0);
            assert_eq!(observations::signature_ready().0, 1);
            assert_no_active_attempt();
            assert_eq!(salsa::plumbing::current_revision(&db), revision);

            let Some(ast::Stmt::Expr(statement)) = prepared.parsed_module().syntax().body.last()
            else {
                anyhow::bail!("fixture must end with a call expression");
            };
            let Some(call) = statement.value.as_call_expr() else {
                anyhow::bail!("fixture must end with a call expression");
            };
            let program_file = db.program_file(system_path_to_file(&db, "src/main.py")?);
            let callee =
                ty_python_core::semantic_index(&db, program_file).expression(call.func.as_ref());
            events_db.take_salsa_events();
            let child = infer_expression_types(&db, callee, TypeContext::default());
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_expression_types_impl",
                None,
                &events_db.take_salsa_events(),
            );

            let created = observations::counts().1;
            {
                let observed = Observation::new();
                assert_eq!(
                    infer(&prepared, &funded())?,
                    AnalysisOutcome::Complete(Type::unknown()),
                );
                assert!(observed.admitted_work().is_some());
            }
            // A refused parent has no partial canonical memo: retry must build it again.
            assert!(observations::counts().1 > created);
            assert_eq!(observations::argument_progress().0, 1);
            assert_eq!(observations::call_progress().0, 1);
            assert_eq!(observations::signature_ready().0, 1);
            let events = events_db.take_salsa_events();
            assert!(
                find_will_execute_event_by_name(&db, "infer_expression_types_impl", None, &events,)
                    .is_some()
            );
            assert_function_query_was_not_run_by_name(
                &db,
                "function_literal_signature",
                None,
                &events,
            );
            assert!(std::ptr::eq(
                child,
                infer_expression_types(&db, callee, TypeContext::default()),
            ));
            assert_eq!(
                infer(&prepared, &funded())?,
                AnalysisOutcome::Complete(Type::unknown()),
            );
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_expression_types_impl",
                None,
                &events_db.take_salsa_events(),
            );
            assert_eq!(observations::counts().0, 0);
            assert_no_active_attempt();
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
        Ok(())
    }
}

#[cfg(feature = "experimental-analysis")]
#[path = "tests/tuple_annotations.rs"]
pub(super) mod tuple_annotations;

#[cfg(feature = "experimental-analysis")]
#[path = "tests/annotation_qualifiers.rs"]
pub(super) mod annotation_qualifiers;

#[cfg(feature = "experimental-analysis")]
#[path = "tests/callable_annotations.rs"]
pub(super) mod callable_annotations;

#[cfg(feature = "experimental-analysis")]
#[path = "tests/annotated_values.rs"]
pub(super) mod annotated_values;

#[cfg(feature = "experimental-analysis")]
#[path = "tests/deferred_parameters.rs"]
pub(super) mod deferred_parameters;
