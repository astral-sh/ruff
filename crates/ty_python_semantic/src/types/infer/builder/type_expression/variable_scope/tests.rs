use std::cell::RefCell;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::name::Name;
use ty_python_core::{global_scope, semantic_index};

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::lint::{LintId, RuleSelection};
use crate::types::diagnostic::INVALID_TYPE_FORM;
use crate::types::infer::InferenceRegion;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::typevar::{TypeVarIdentity, TypeVarNonce};
use crate::types::{BindingContext, TypeContext};
use crate::{ProgramEnvironment, default_lint_registry};

const PATH: &str = "/src/variable_scope.py";

fn database() -> anyhow::Result<TestDb> {
    let mut rules = RuleSelection::from_registry(default_lint_registry());
    for lint in [
        &INVALID_INIT_TYPE_VARIABLE,
        &INVALID_TYPE_FORM,
        &UNBOUND_TYPE_VARIABLE,
    ] {
        rules.disable(LintId::of(lint));
    }
    TestDbBuilder::new()
        .with_file(PATH, "class Owner: ...\nAlias: int = int\nT\n")
        .with_rule_selection(rules)
        .build()
}

struct Fixture<'db, 'ast> {
    builder: TypeInferenceBuilder<'db, 'ast>,
    expression: &'ast ast::Expr,
    owner: Definition<'db>,
    alias: Definition<'db>,
    raw: TypeVarInstance<'db>,
    bound: BoundTypeVarInstance<'db>,
}

fn fixture<'db, 'ast>(
    db: &'db TestDb,
    module: &'ast ParsedModuleRef,
    env: &'ast ProgramEnvironment<'db>,
) -> anyhow::Result<Fixture<'db, 'ast>> {
    let file = db.program_file(system_path_to_file(db, PATH)?);
    let index = semantic_index(db, file);
    let [
        ast::Stmt::ClassDef(owner),
        ast::Stmt::AnnAssign(alias),
        ast::Stmt::Expr(expression),
    ] = module.suite().as_slice()
    else {
        anyhow::bail!("unexpected variable scope fixture");
    };
    let owner = index
        .try_definition(owner)
        .ok_or_else(|| anyhow::anyhow!("missing owner definition"))?;
    let alias = index
        .try_definition(alias)
        .ok_or_else(|| anyhow::anyhow!("missing alias definition"))?;
    let raw = TypeVarInstance::new(
        db,
        TypeVarIdentity::new(db, Name::new("T"), None, TypeVarKind::LegacyTypeVar),
        None,
        None,
        None,
    );
    let bound = raw.with_binding_context(db, owner);
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
    builder.context.inference_flags = InferenceFlags::CHECK_UNBOUND_TYPEVARS;
    Ok(Fixture {
        builder,
        expression: &expression.value,
        owner,
        alias,
        raw,
        bound,
    })
}

struct Sequence<'db> {
    db: &'db TestDb,
    events: RefCell<Vec<&'static str>>,
    refuse_at: Option<usize>,
}

impl Sequence<'_> {
    fn record(&self, event: &'static str) -> Result<(), &'static str> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.refuse_at == Some(index) {
            Err(event)
        } else {
            Ok(())
        }
    }
}

macro_rules! sequence_effects {
    ($trait_name:ident $($asynchronous:ident)?) => {
        impl<'db, 'ast> $trait_name<'db, 'ast> for Sequence<'db> {
            type Error = &'static str;

            $($asynchronous)? fn dispatch(&self) -> Result<(), Self::Error> {
                self.record("dispatch")
            }
            $($asynchronous)? fn bound_typevar(&self, typevar: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Self::Error> {
                self.record("bound_typevar")?;
                Ok(typevar.typevar(self.db))
            }
            $($asynchronous)? fn kind(&self, typevar: TypeVarInstance<'db>) -> Result<TypeVarKind, Self::Error> {
                self.record("kind")?;
                Ok(typevar.kind(self.db))
            }
            $($asynchronous)? fn in_init_receiver(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error> {
                self.record("init_flag")?;
                Ok(builder.inference_flags().contains(InferenceFlags::IN_INIT_RECEIVER_ANNOTATION))
            }
            $($asynchronous)? fn bound_owner(&self, typevar: BoundTypeVarInstance<'db>) -> Result<Option<Definition<'db>>, Self::Error> {
                self.record("owner")?;
                Ok(typevar.binding_context(self.db).definition())
            }
            $($asynchronous)? fn binding_definition(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Option<Definition<'db>>, Self::Error> {
                self.record("binding")?;
                Ok(builder.typevar_binding_context)
            }
            $($asynchronous)? fn in_type_alias(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error> {
                self.record("alias_flag")?;
                Ok(builder.inference_flags().contains(InferenceFlags::IN_TYPE_ALIAS))
            }
            $($asynchronous)? fn definition_is_annotated_assignment(&self, definition: Definition<'db>) -> Result<bool, Self::Error> {
                self.record("annotated_assignment")?;
                Ok(matches!(definition.kind(self.db), DefinitionKind::AnnotatedAssignment(_)))
            }
            $($asynchronous)? fn definition_is_class(&self, definition: Definition<'db>) -> Result<bool, Self::Error> {
                self.record("class")?;
                Ok(matches!(definition.kind(self.db), DefinitionKind::Class(_)))
            }
            $($asynchronous)? fn check_unbound(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error> {
                self.record("unbound_flag")?;
                Ok(builder.inference_flags().contains(InferenceFlags::CHECK_UNBOUND_TYPEVARS))
            }
            $($asynchronous)? fn report_init_receiver(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, _expression: &ast::Expr, _typevar: BoundTypeVarInstance<'db>, _owner: Definition<'db>) -> Result<(), Self::Error> {
                self.record("report_init")
            }
            $($asynchronous)? fn report_alias_capture(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, _expression: &ast::Expr, _typevar: BoundTypeVarInstance<'db>) -> Result<(), Self::Error> {
                self.record("report_alias")
            }
            $($asynchronous)? fn report_unbound(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, _expression: &ast::Expr, _typevar: TypeVarInstance<'db>) -> Result<(), Self::Error> {
                self.record("report_unbound")
            }
        }
    };
}

sequence_effects!(SynchronousTypeVariableScopeEffects);
sequence_effects!(TypeVariableScopeEffects async);

fn assert_sequence<'db>(
    db: &'db TestDb,
    fixture: &Fixture<'db, '_>,
    ty: Type<'db>,
    expected: Type<'db>,
    sequence: &[&'static str],
) {
    for refusal in std::iter::once(None).chain((0..sequence.len()).map(Some)) {
        let effects = Sequence {
            db,
            events: RefCell::default(),
            refuse_at: refusal,
        };
        let expected_result = refusal.map_or(Ok(expected), |index| Err(sequence[index]));
        let expected_events = refusal.map_or(sequence, |index| &sequence[..=index]);
        assert_eq!(
            check_type_variable_scope_sync(
                &fixture.builder,
                fixture.expression,
                ty,
                TypeVariableScopeFacts,
                &effects
            ),
            expected_result,
        );
        assert_eq!(*effects.events.borrow(), expected_events);
        effects.events.borrow_mut().clear();
        assert_eq!(
            try_poll_immediate(check_type_variable_scope_with(
                &fixture.builder,
                fixture.expression,
                ty,
                TypeVariableScopeFacts,
                &effects
            )),
            Poll::Ready(expected_result),
        );
        assert_eq!(*effects.events.borrow(), expected_events);
    }
}

#[test]
fn self_check_precedes_init_flag_and_scope_effect_refusals() -> anyhow::Result<()> {
    let db = database()?;
    let module = parsed_module(
        &db,
        db.program_file(system_path_to_file(&db, PATH)?)
            .python_file(&db),
    )
    .load(&db);
    let env = db.program_environment();
    let mut fixture = fixture(&db, &module, &env)?;
    let bound = Type::TypeVar(fixture.bound);
    assert_sequence(
        &db,
        &fixture,
        bound,
        bound,
        &[
            "dispatch",
            "bound_typevar",
            "kind",
            "init_flag",
            "alias_flag",
            "unbound_flag",
        ],
    );

    fixture.builder.context.inference_flags |= InferenceFlags::IN_INIT_RECEIVER_ANNOTATION;
    let self_var = TypeVarInstance::new(
        &db,
        TypeVarIdentity::new(&db, Name::new("Self"), None, TypeVarKind::TypingSelf),
        None,
        None,
        None,
    )
    .with_binding_context(&db, fixture.owner);
    let self_ty = Type::TypeVar(self_var);
    assert_sequence(
        &db,
        &fixture,
        self_ty,
        self_ty,
        &[
            "dispatch",
            "bound_typevar",
            "kind",
            "alias_flag",
            "unbound_flag",
        ],
    );

    fixture.builder.typevar_binding_context = Some(fixture.owner);
    assert_sequence(
        &db,
        &fixture,
        bound,
        bound,
        &[
            "dispatch",
            "bound_typevar",
            "kind",
            "init_flag",
            "owner",
            "binding",
            "alias_flag",
            "unbound_flag",
        ],
    );

    let synthetic = Type::TypeVar(BoundTypeVarInstance::new(
        &db,
        fixture.raw,
        BindingContext::Synthetic(env.program(&db)),
        None,
        TypeVarNonce::NONE,
    ));
    assert_sequence(
        &db,
        &fixture,
        synthetic,
        synthetic,
        &[
            "dispatch",
            "bound_typevar",
            "kind",
            "init_flag",
            "owner",
            "alias_flag",
            "unbound_flag",
        ],
    );
    Ok(())
}

#[test]
fn init_report_precedes_alias_capture_and_its_early_return() -> anyhow::Result<()> {
    let db = database()?;
    let module = parsed_module(
        &db,
        db.program_file(system_path_to_file(&db, PATH)?)
            .python_file(&db),
    )
    .load(&db);
    let env = db.program_environment();
    let mut fixture = fixture(&db, &module, &env)?;
    fixture.builder.context.inference_flags |=
        InferenceFlags::IN_INIT_RECEIVER_ANNOTATION | InferenceFlags::IN_TYPE_ALIAS;
    fixture.builder.typevar_binding_context = Some(fixture.alias);
    let ty = Type::TypeVar(fixture.bound);
    assert_sequence(
        &db,
        &fixture,
        ty,
        ty,
        &[
            "dispatch",
            "bound_typevar",
            "kind",
            "init_flag",
            "owner",
            "binding",
            "report_init",
            "alias_flag",
            "binding",
            "annotated_assignment",
            "owner",
            "class",
            "report_alias",
        ],
    );
    Ok(())
}

#[test]
fn alias_capture_preserves_definition_and_owner_guards() -> anyhow::Result<()> {
    let db = database()?;
    let module = parsed_module(
        &db,
        db.program_file(system_path_to_file(&db, PATH)?)
            .python_file(&db),
    )
    .load(&db);
    let env = db.program_environment();
    let mut fixture = fixture(&db, &module, &env)?;
    fixture.builder.context.inference_flags |= InferenceFlags::IN_TYPE_ALIAS;
    let ty = Type::TypeVar(fixture.bound);
    assert_sequence(
        &db,
        &fixture,
        ty,
        ty,
        &[
            "dispatch",
            "bound_typevar",
            "kind",
            "init_flag",
            "alias_flag",
            "binding",
            "unbound_flag",
        ],
    );
    fixture.builder.typevar_binding_context = Some(fixture.owner);
    assert_sequence(
        &db,
        &fixture,
        ty,
        ty,
        &[
            "dispatch",
            "bound_typevar",
            "kind",
            "init_flag",
            "alias_flag",
            "binding",
            "annotated_assignment",
            "unbound_flag",
        ],
    );
    fixture.builder.typevar_binding_context = Some(fixture.alias);
    let synthetic = Type::TypeVar(BoundTypeVarInstance::new(
        &db,
        fixture.raw,
        BindingContext::Synthetic(env.program(&db)),
        None,
        TypeVarNonce::NONE,
    ));
    assert_sequence(
        &db,
        &fixture,
        synthetic,
        synthetic,
        &[
            "dispatch",
            "bound_typevar",
            "kind",
            "init_flag",
            "alias_flag",
            "binding",
            "annotated_assignment",
            "owner",
            "unbound_flag",
        ],
    );
    let alias_owned = Type::TypeVar(fixture.raw.with_binding_context(&db, fixture.alias));
    assert_sequence(
        &db,
        &fixture,
        alias_owned,
        alias_owned,
        &[
            "dispatch",
            "bound_typevar",
            "kind",
            "init_flag",
            "alias_flag",
            "binding",
            "annotated_assignment",
            "owner",
            "class",
            "unbound_flag",
        ],
    );
    Ok(())
}

#[test]
fn unbound_scope_gate_and_report_refusal_preserve_fallback_order() -> anyhow::Result<()> {
    let db = database()?;
    let module = parsed_module(
        &db,
        db.program_file(system_path_to_file(&db, PATH)?)
            .python_file(&db),
    )
    .load(&db);
    let env = db.program_environment();
    let mut fixture = fixture(&db, &module, &env)?;
    let ty = Type::KnownInstance(KnownInstanceType::TypeVar(fixture.raw));
    assert_sequence(
        &db,
        &fixture,
        ty,
        Type::unknown(),
        &["dispatch", "unbound_flag", "report_unbound"],
    );
    assert_sequence(
        &db,
        &fixture,
        Type::Never,
        Type::Never,
        &["dispatch", "unbound_flag"],
    );
    fixture
        .builder
        .context
        .inference_flags
        .remove(InferenceFlags::CHECK_UNBOUND_TYPEVARS);
    assert_sequence(&db, &fixture, ty, ty, &["dispatch", "unbound_flag"]);
    Ok(())
}

#[test]
fn disabled_scope_lints_retain_bound_types_and_unbound_unknown() -> anyhow::Result<()> {
    let db = database()?;
    let module = parsed_module(
        &db,
        db.program_file(system_path_to_file(&db, PATH)?)
            .python_file(&db),
    )
    .load(&db);
    let env = db.program_environment();
    let mut fixture = fixture(&db, &module, &env)?;
    let raw = Type::KnownInstance(KnownInstanceType::TypeVar(fixture.raw));
    assert_eq!(
        fixture
            .builder
            .check_type_variable_scope(fixture.expression, raw),
        Type::unknown()
    );
    fixture
        .builder
        .context
        .inference_flags
        .remove(InferenceFlags::CHECK_UNBOUND_TYPEVARS);
    assert_eq!(
        fixture
            .builder
            .check_type_variable_scope(fixture.expression, raw),
        raw
    );
    fixture.builder.context.inference_flags |= InferenceFlags::CHECK_UNBOUND_TYPEVARS
        | InferenceFlags::IN_INIT_RECEIVER_ANNOTATION
        | InferenceFlags::IN_TYPE_ALIAS;
    fixture.builder.typevar_binding_context = Some(fixture.alias);
    let bound = Type::TypeVar(fixture.bound);
    assert_eq!(
        fixture
            .builder
            .check_type_variable_scope(fixture.expression, bound),
        bound
    );
    assert!(fixture.builder.context.finish().is_empty());
    Ok(())
}
