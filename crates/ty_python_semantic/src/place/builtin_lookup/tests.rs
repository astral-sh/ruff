use std::cell::{Cell, RefCell};

use anyhow::Context;
use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ty_python_core::definition::DefinitionNodeKey;
use ty_python_core::semantic_index;

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::{DefinedPlace, Definedness, Provenance};
use crate::types::Type;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Namespace(BuiltinNamespace),
    Checkpoint,
    Resolve,
    Scope,
    Symbol,
    PublicType,
    Fallback,
    Combine,
    Visibility,
}

struct Recording<'db> {
    files: [ProgramFile<'db>; 2],
    symbols: [PlaceAndQualifiers<'db>; 2],
    fallbacks: [PlaceAndQualifiers<'db>; 2],
    project_present: bool,
    runtime_visible: bool,
    current: Cell<usize>,
    events: RefCell<Vec<Event>>,
    refuse_at: Option<usize>,
}

impl<'db> Recording<'db> {
    fn new(files: [ProgramFile<'db>; 2]) -> Self {
        Self {
            files,
            symbols: [PlaceAndQualifiers::default(); 2],
            fallbacks: [PlaceAndQualifiers::default(); 2],
            project_present: true,
            runtime_visible: true,
            current: Cell::new(0),
            events: RefCell::default(),
            refuse_at: None,
        }
    }

    fn record(&self, event: Event) -> Result<(), Event> {
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

impl<'db> SynchronousBuiltinLookupEffects<'db> for Recording<'db> {
    type Error = Event;

    fn checkpoint(&self, _name: &str) -> Result<(), Event> {
        self.record(Event::Checkpoint)
    }

    fn namespace_symbol(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        visibility: BuiltinVisibility,
        namespace: BuiltinNamespace,
    ) -> Result<Option<(ScopeId<'db>, PlaceAndQualifiers<'db>)>, Event> {
        self.record(Event::Namespace(namespace))?;
        self.current.set(match namespace {
            BuiltinNamespace::Project => 0,
            BuiltinNamespace::Standard => 1,
        });
        builtin_namespace_symbol_sync(
            db,
            env,
            name,
            visibility,
            namespace,
            BuiltinLookupFacts,
            self,
        )
    }

    fn resolve_namespace(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        namespace: BuiltinNamespace,
    ) -> Result<Option<ProgramFile<'db>>, Event> {
        self.record(Event::Resolve)?;
        Ok(
            if namespace == BuiltinNamespace::Project && !self.project_present {
                None
            } else {
                Some(self.files[self.current.get()])
            },
        )
    }

    fn global_scope(&self, db: &'db dyn Db, file: ProgramFile<'db>) -> Result<ScopeId<'db>, Event> {
        self.record(Event::Scope)?;
        Ok(global_scope(db, file))
    }

    fn symbol(
        &self,
        _db: &'db dyn Db,
        _scope: ScopeId<'db>,
        _name: &str,
        reexport: RequiresExplicitReExport,
        considered: ConsideredDefinitions,
    ) -> Result<PlaceAndQualifiers<'db>, Event> {
        self.record(Event::Symbol)?;
        assert_eq!(reexport, RequiresExplicitReExport::Yes);
        assert_eq!(considered, ConsideredDefinitions::EndOfScope);
        Ok(self.symbols[self.current.get()])
    }

    fn lookup_result(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        symbol: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Event> {
        self.record(Event::PublicType)?;
        Ok(symbol.into_lookup_result(db, env))
    }

    fn implicit_global_symbol(
        &self,
        _db: &'db dyn Db,
        _file: ProgramFile<'db>,
        _name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Event> {
        self.record(Event::Fallback)?;
        Ok(self.fallbacks[self.current.get()])
    }

    fn combine_fallback(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        prior: LookupError<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Event> {
        self.record(Event::Combine)?;
        Ok(prior.or_fall_back_to(db, env, fallback))
    }

    fn runtime_visibility(&self, _definition: Definition<'db>) -> Result<bool, Event> {
        self.record(Event::Visibility)?;
        Ok(self.runtime_visible)
    }
}

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file("/src/project.py", "value = 1")
        .with_file("/src/standard.py", "value = 2")
        .build()
}

fn files(db: &TestDb) -> anyhow::Result<[ProgramFile<'_>; 2]> {
    let program = db.program_environment().program(db);
    Ok([
        ProgramFile::new(db, system_path_to_file(db, "/src/project.py")?, program),
        ProgramFile::new(db, system_path_to_file(db, "/src/standard.py")?, program),
    ])
}

fn definition<'db>(db: &'db dyn Db, file: ProgramFile<'db>) -> anyhow::Result<Definition<'db>> {
    let module = parsed_module(db, file.python_file(db)).load(db);
    let key = module
        .suite()
        .iter()
        .filter_map(|statement| statement.as_assign_stmt())
        .flat_map(DefinitionNodeKey::from_assignment)
        .next()
        .context("fixture assignment")?;
    Ok(semantic_index(db, file).expect_single_definition(key))
}

fn lookup<'db>(
    db: &'db TestDb,
    visibility: BuiltinVisibility,
    effects: &Recording<'db>,
) -> Result<Option<(ScopeId<'db>, PlaceAndQualifiers<'db>)>, Event> {
    builtins_symbol_sync(db, &db.program_environment(), "value", visibility, effects)
}

#[test]
fn project_precedence_keeps_module_fallback_and_standard_resolution_lazy() -> anyhow::Result<()> {
    let db = database()?;
    let mut effects = Recording::new(files(&db)?);
    effects.symbols = [
        Place::bound(Type::int_literal(1)).into(),
        Place::bound(Type::int_literal(2)).into(),
    ];
    assert_eq!(
        lookup(&db, BuiltinVisibility::RuntimeOnly, &effects),
        Ok(Some((
            global_scope(&db, effects.files[0]),
            effects.symbols[0]
        )))
    );
    assert_eq!(
        *effects.events.borrow(),
        [
            Event::Namespace(BuiltinNamespace::Project),
            Event::Checkpoint,
            Event::Resolve,
            Event::Scope,
            Event::Symbol,
            Event::PublicType,
        ]
    );
    Ok(())
}

#[test]
fn absent_or_hidden_project_symbol_allows_standard_builtins() -> anyhow::Result<()> {
    let db = database()?;
    let files = files(&db)?;
    let definition = definition(&db, files[0])?;
    for (project_present, project_symbol) in [
        (false, Place::Undefined.into()),
        (true, Place::Undefined.into()),
        (
            true,
            Place::bound(Type::int_literal(1))
                .with_definition(definition)
                .into(),
        ),
    ] {
        let mut effects = Recording::new(files);
        effects.project_present = project_present;
        effects.runtime_visible = false;
        effects.symbols = [project_symbol, Place::bound(Type::int_literal(2)).into()];
        assert_eq!(
            lookup(&db, BuiltinVisibility::RuntimeOnly, &effects),
            Ok(Some((global_scope(&db, files[1]), effects.symbols[1])))
        );
        assert!(
            effects
                .events
                .borrow()
                .contains(&Event::Namespace(BuiltinNamespace::Standard))
        );
        assert_eq!(
            effects.events.borrow().contains(&Event::Fallback),
            project_present && project_symbol.is_undefined()
        );
        assert_eq!(
            effects.events.borrow().contains(&Event::Visibility),
            !project_symbol.is_undefined()
        );
    }
    Ok(())
}

#[test]
fn possibly_undefined_builtin_uses_the_existing_fallback_combination() -> anyhow::Result<()> {
    let db = database()?;
    let mut effects = Recording::new(files(&db)?);
    effects.symbols[0] = Place::Defined(
        DefinedPlace::new(Type::int_literal(1)).with_definedness(Definedness::PossiblyUndefined),
    )
    .into();
    for fallback in [
        Place::Undefined.into(),
        Place::bound(Type::int_literal(2)).into(),
    ] {
        effects.fallbacks[0] = fallback;
        let expected =
            effects.symbols[0].or_fall_back_to(&db, &db.program_environment(), || fallback);
        assert_eq!(
            lookup(&db, BuiltinVisibility::RuntimeOnly, &effects),
            Ok(Some((global_scope(&db, effects.files[0]), expected)))
        );
        assert!(
            !effects
                .events
                .borrow()
                .contains(&Event::Namespace(BuiltinNamespace::Standard))
        );
    }
    Ok(())
}

#[test]
fn exhausted_namespaces_return_absence() -> anyhow::Result<()> {
    let db = database()?;
    let files = files(&db)?;
    let definition = definition(&db, files[1])?;
    for standard_symbol in [
        Place::Undefined.into(),
        Place::bound(Type::int_literal(2))
            .with_definition(definition)
            .into(),
    ] {
        let mut effects = Recording::new(files);
        effects.symbols[1] = standard_symbol;
        effects.runtime_visible = false;
        assert_eq!(
            lookup(&db, BuiltinVisibility::RuntimeOnly, &effects),
            Ok(None)
        );
    }
    Ok(())
}

#[test]
fn visibility_only_checks_single_definition_provenance_when_requested() -> anyhow::Result<()> {
    let db = database()?;
    let files = files(&db)?;
    let definition = definition(&db, files[0])?;
    for (visibility, provenance) in [
        (
            BuiltinVisibility::All,
            Provenance::SingleDefinition(definition),
        ),
        (BuiltinVisibility::RuntimeOnly, Provenance::Unknown),
        (
            BuiltinVisibility::RuntimeOnly,
            Provenance::MultipleDefinitions,
        ),
    ] {
        let mut effects = Recording::new(files);
        effects.runtime_visible = false;
        effects.symbols[0] = Place::bound(Type::int_literal(1))
            .with_provenance(provenance)
            .into();
        assert_eq!(
            lookup(&db, visibility, &effects),
            Ok(Some((global_scope(&db, files[0]), effects.symbols[0])))
        );
        assert!(!effects.events.borrow().contains(&Event::Visibility));
    }
    Ok(())
}

#[test]
fn refusals_stop_before_later_namespaces_or_effects() -> anyhow::Result<()> {
    let db = database()?;
    let files = files(&db)?;
    let definition = definition(&db, files[1])?;
    let mut effects = Recording::new(files);
    effects.symbols[1] = Place::bound(Type::int_literal(2))
        .with_definition(definition)
        .into();
    assert!(matches!(
        lookup(&db, BuiltinVisibility::RuntimeOnly, &effects),
        Ok(Some(_))
    ));
    let events = effects.events.take();
    for (index, event) in events.iter().copied().enumerate() {
        effects.refuse_at = Some(index);
        assert_eq!(
            lookup(&db, BuiltinVisibility::RuntimeOnly, &effects),
            Err(event)
        );
        assert_eq!(*effects.events.borrow(), events[..=index]);
        effects.events.borrow_mut().clear();
    }
    Ok(())
}
