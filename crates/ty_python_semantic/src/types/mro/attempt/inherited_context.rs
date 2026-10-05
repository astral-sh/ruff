use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use salsa::Database as _;
use salsa::prepared_source_probe::Stamp;
use ty_python_core::definition::Definition;

use super::AttemptMroEffects;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class::context::inherited::{
    InheritedContextEffects, InheritedContextWork, inherited_context_with,
};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::source_read::read_source;
use crate::types::{
    BindingContext, BoundTypeVarInstance, ClassLiteral, GenericContext, KnownClass,
    StaticClassLiteral, Type,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

const SOURCE: &str = r#"
from typing import Generic, TypeVar

T = TypeVar("T")
U = TypeVar("U")
class First(Generic[T, U]): ...
class Second(Generic[T, U]): ...
class Inherited(First[U, T], Second[T, U]): ...
class Fixed(First[int, str]): ...
class Plain: ...
"#;

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/inherited.py", SOURCE)
        .build()
}

fn request<'db>(
    db: &'db TestDb,
    name: &str,
) -> anyhow::Result<(StaticClassLiteral<'db>, Option<GenericContext<'db>>)> {
    let env = db.program_environment();
    let class = if name == "Sequence" {
        KnownClass::Sequence.try_to_class_literal(db, &env)
    } else {
        let file = db.program_file(system_path_to_file(db, "/src/inherited.py")?);
        global_symbol(db, file, name)
            .place
            .ignore_possibly_undefined()
            .and_then(Type::as_class_literal)
            .and_then(ClassLiteral::as_static)
    }
    .ok_or_else(|| anyhow::anyhow!("missing class {name}"))?;
    let context = if expansion_probe::active() {
        read_source(&AttemptMroEffects::new(db), || class.generic_context(db))
            .map_err(|error| anyhow::anyhow!("{error:?}"))?
    } else {
        class.generic_context(db)
    };
    Ok((class, context))
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

fn assert_variables<'db>(
    db: &'db TestDb,
    class: StaticClassLiteral<'db>,
    context: Option<GenericContext<'db>>,
    names: &[&str],
) {
    let variables = context
        .into_iter()
        .flat_map(|context| context.variables(db))
        .collect::<Vec<_>>();
    assert_eq!(
        variables
            .iter()
            .map(|variable| variable.name(db).as_str())
            .collect::<Vec<_>>(),
        names
    );
    for variable in variables {
        assert_eq!(
            variable.binding_context(db),
            BindingContext::Definition(class.definition(db))
        );
    }
}

struct PublicationControl<'db> {
    inner: AttemptMroEffects<'db>,
    refuse: Cell<bool>,
    published: Cell<bool>,
    built: Cell<Option<GenericContext<'db>>>,
}

impl<'db> InheritedContextEffects<'db> for PublicationControl<'db> {
    type Error = Incomplete;

    fn checkpoint(&self, work: InheritedContextWork) -> Result<(), Incomplete> {
        if work == InheritedContextWork::Publish {
            self.published.set(true);
            if self.refuse.get() {
                self.inner.admit(usize::MAX)?;
            }
        }
        InheritedContextEffects::checkpoint(&self.inner, work)
    }

    fn definition(&self, class: StaticClassLiteral<'db>) -> Result<Definition<'db>, Incomplete> {
        InheritedContextEffects::definition(&self.inner, class)
    }

    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Incomplete> {
        InheritedContextEffects::explicit_bases(&self.inner, class)
    }

    fn find_variables(
        &self,
        env: &ProgramEnvironment<'db>,
        definition: Definition<'db>,
        base: Type<'db>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<(), Incomplete> {
        self.inner.find_variables(env, definition, base, variables)
    }

    fn build_context(
        &self,
        env: &ProgramEnvironment<'db>,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<GenericContext<'db>, Incomplete> {
        let context = self.inner.build_context(env, variables)?;
        self.built.set(Some(context));
        Ok(context)
    }
}

#[test]
fn empty_and_nonempty_contexts_refuse_publication_and_retry() -> anyhow::Result<()> {
    for name in ["Plain", "Fixed", "Inherited", "Sequence"] {
        let db = database()?;
        let (class, expected) = request(&db, name)?;
        let stamp = Stamp::current(&db);
        let effects = PublicationControl {
            inner: AttemptMroEffects::new(&db),
            refuse: Cell::new(true),
            published: Cell::new(false),
            built: Cell::new(None),
        };
        let build = || inherited_context_with(&db, class, &effects);
        let (limited, _) = expansion_probe::run_mro_observed(&db, 100_000, build);
        assert_eq!(limited, Err(Incomplete::Allowance));
        assert!(effects.published.get());
        assert_eq!(effects.built.get(), expected);
        effects.refuse.set(false);
        for _ in 0..2 {
            effects.published.set(false);
            let (retried, _) = expansion_probe::run_mro_observed(&db, 100_000, build);
            assert_eq!(retried, Ok(Ok(expected)));
            assert!(effects.published.get());
            assert_eq!(Stamp::current(&db), stamp);
            assert!(!expansion_probe::active());
        }
    }
    Ok(())
}

#[test]
fn cold_inherited_contexts_preserve_variables_and_source_order() -> anyhow::Result<()> {
    for (name, names) in [
        ("Inherited", &["U", "T"][..]),
        ("Fixed", &[][..]),
        ("Plain", &[][..]),
        ("Sequence", &["_T_co"][..]),
    ] {
        let mut ordinary = database()?;
        ordinary.clear_salsa_events();
        let (class, context) = request(&ordinary, name)?;
        let original_reads = executions(&ordinary);
        let expected = context.map(|context| {
            context
                .variables(&ordinary)
                .map(|variable| variable.name(&ordinary).to_string())
                .collect::<Vec<_>>()
        });
        assert_variables(&ordinary, class, context, names);

        let mut db = database()?;
        db.clear_salsa_events();
        let (result, _) = expansion_probe::run_mro_observed(&db, 100_000, || request(&db, name));
        let actual_reads = executions(&db);
        let (class, context) = result.map_err(|error| anyhow::anyhow!("{name}: {error:?}"))??;
        assert_variables(&db, class, context, names);
        assert_eq!(
            context.map(|context| context
                .variables(&db)
                .map(|variable| variable.name(&db).to_string())
                .collect::<Vec<_>>()),
            expected
        );
        assert_eq!(request(&db, name)?.1, context);
        assert_eq!(actual_reads, original_reads, "{name}");
    }
    Ok(())
}

#[test]
fn cold_inherited_context_refusal_retries_without_an_edit() -> anyhow::Result<()> {
    for name in ["Inherited", "Fixed", "Sequence"] {
        let db = database()?;
        let stamp = Stamp::current(&db);
        let (limited, _) = expansion_probe::run_mro_observed(&db, 1, || request(&db, name));
        assert!(matches!(limited, Err(Incomplete::Allowance)), "{limited:?}");
        for _ in 0..2 {
            let (result, _) =
                expansion_probe::run_mro_observed(&db, 100_000, || request(&db, name));
            let (class, context) =
                result.map_err(|error| anyhow::anyhow!("{name}: {error:?}"))??;
            let names = match name {
                "Inherited" => &["U", "T"][..],
                "Sequence" => &["_T_co"][..],
                _ => &[][..],
            };
            assert_variables(&db, class, context, names);
            assert_eq!(Stamp::current(&db), stamp);
            assert!(!expansion_probe::active());
        }
    }
    Ok(())
}
use std::cell::Cell;
