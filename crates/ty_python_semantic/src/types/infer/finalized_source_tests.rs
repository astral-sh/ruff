use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, PythonVersion};
use salsa::attempt_probe::{AttemptOutcome, try_with_attempt};
use salsa::execution_probe::{
    ExecutionAdmission, ExecutionWork, FinalSourceError, FinalSourceMemo, RegistryBuilder,
    RunResult,
};
use salsa::plumbing::{AsId, ZalsaDatabase};
use salsa::{Database, DatabaseKeyIndex, Event, EventKind};
use ty_python_core::definition::{Definition, DefinitionState};
use ty_python_core::finalized_sources::{place_table_ingredient, use_def_map_ingredient};
use ty_python_core::scope::ScopeId;
use ty_python_core::{
    PlaceTable, UseDefMap, global_scope, place_table, semantic_index, use_def_map,
};

use super::{DefinitionInference, infer_definition_types};
use crate::Db;
use crate::db::tests::TestDbBuilder;
use crate::types::class::{
    explicit_bases_ingredient, known_class_to_class_literal_ingredient,
    known_class_to_class_literal_key, pep695_generic_context_ingredient,
    static_class_generic_context_ingredient,
};
use crate::types::{
    ClassLiteral, GenericContext, KnownClass, KnownInstanceType, SpecialFormType,
    StaticClassLiteral, Type, TypeAndQualifiers,
};

const PATH: &str = "/src/declarations.py";
const PEP695_SOURCE: &str = "\
from typing import Protocol

class C[T]:
    child: T

class P[T](Protocol):
    child: T
";
const LEGACY_SOURCE: &str = "\
from typing import Generic, Protocol, TypeVar
T = TypeVar(\"T\")

class C(Generic[T]):
    child: T

class P(Protocol[T]):
    child: T
";

#[derive(Clone, Copy, Debug)]
enum Syntax {
    Pep695,
    Legacy,
}

struct PreparedClass<'db> {
    definitions: [Definition<'db>; 3],
    class: StaticClassLiteral<'db>,
    context: GenericContext<'db>,
    pep695_context: Option<GenericContext<'db>>,
    explicit_bases: &'db [Type<'db>],
    body: ScopeId<'db>,
    places: &'db PlaceTable,
    uses: &'db UseDefMap<'db>,
    inference: &'db DefinitionInference<'db>,
    declared: TypeAndQualifiers<'db>,
}

struct Admit;

impl ExecutionAdmission for Admit {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}

fn single_definition<'db>(states: impl Iterator<Item = DefinitionState<'db>>) -> Definition<'db> {
    let mut definitions = states.filter_map(|state| match state {
        DefinitionState::Defined(definition) => Some(definition),
        DefinitionState::Undefined | DefinitionState::Deleted => None,
    });
    let definition = definitions
        .next()
        .expect("the fixture declares this symbol");
    assert!(
        definitions.next().is_none(),
        "the fixture has one definition"
    );
    definition
}

fn declaration<'db>(
    inference: &DefinitionInference<'db>,
    definition: Definition<'db>,
) -> TypeAndQualifiers<'db> {
    inference
        .inferred_declaration(definition)
        .declared()
        .expect("the fixture has a valid declared type")
}

fn child_definition<'db>(places: &PlaceTable, uses: &UseDefMap<'db>) -> Definition<'db> {
    let symbol = places.symbol_id("child").expect("the class declares child");
    single_definition(
        uses.end_of_scope_symbol_declarations(symbol)
            .map(|declaration| declaration.declaration),
    )
}

fn executed(events: &[Event]) -> Vec<DatabaseKeyIndex> {
    events
        .iter()
        .filter_map(|event| match event.kind {
            EventKind::WillExecute { database_key } => Some(database_key),
            _ => None,
        })
        .collect()
}

#[test]
fn pep695_declaration_inference_is_a_finalized_source() -> anyhow::Result<()> {
    declaration_inference_is_a_finalized_source(Syntax::Pep695)
}

#[test]
fn legacy_declaration_inference_is_a_finalized_source() -> anyhow::Result<()> {
    declaration_inference_is_a_finalized_source(Syntax::Legacy)
}

fn declaration_inference_is_a_finalized_source(syntax: Syntax) -> anyhow::Result<()> {
    let source = match syntax {
        Syntax::Pep695 => PEP695_SOURCE,
        Syntax::Legacy => LEGACY_SOURCE,
    };
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(PATH, source)
        .build()?;
    let file = system_path_to_file(&db, PATH)?;
    let program_file = db.program_file(file);
    let module = parsed_module(&db, program_file.python_file(&db)).load(&db);
    let index = semantic_index(&db, program_file);
    let scope = global_scope(&db, program_file);
    let places = place_table(&db, scope);
    let uses = use_def_map(&db, scope);

    let prepared = ["C", "P"].map(|name| {
        let symbol = places.symbol_id(name).expect("the class is in the module");
        let class_definition = single_definition(
            uses.end_of_scope_symbol_bindings(symbol)
                .map(|binding| binding.binding),
        );
        let class_node = module
            .suite()
            .iter()
            .find_map(|statement| match statement {
                ast::Stmt::ClassDef(class) if class.name.id.as_str() == name => Some(class),
                _ => None,
            })
            .expect("the source contains this class");
        assert_eq!(index.expect_single_definition(class_node), class_definition);
        let parameter_definition = match syntax {
            Syntax::Pep695 => {
                let Some(parameters) = &class_node.type_params else {
                    panic!("the class has PEP 695 parameters");
                };
                let [ast::TypeParam::TypeVar(parameter)] = parameters.type_params.as_slice() else {
                    panic!("the class declares one TypeVar");
                };
                index.expect_single_definition(parameter)
            }
            Syntax::Legacy => {
                assert!(class_node.type_params.is_none());
                let symbol = places.symbol_id("T").expect("the module binds the TypeVar");
                single_definition(
                    uses.end_of_scope_symbol_bindings(symbol)
                        .map(|binding| binding.binding),
                )
            }
        };
        let class_inference = infer_definition_types(&db, class_definition);
        let Type::ClassLiteral(ClassLiteral::Static(class)) =
            class_inference.binding_type(class_definition)
        else {
            panic!("the undecorated class retains its static class literal");
        };
        let body = class.body_scope(&db);
        let body_places = place_table(&db, body);
        let body_uses = use_def_map(&db, body);
        let child_definition = child_definition(body_places, body_uses);
        let child_inference = infer_definition_types(&db, child_definition);
        let child_type = declaration(child_inference, child_definition);
        let Type::TypeVar(bound) = child_type.inner_type() else {
            panic!("child is declared with the class's bound TypeVar");
        };
        assert!(child_type.qualifiers().is_empty());
        assert_eq!(
            bound.binding_context(&db).definition(),
            Some(class_definition)
        );

        let parameter_inference = infer_definition_types(&db, parameter_definition);
        let parameter_type = match syntax {
            Syntax::Pep695 => {
                let declared = declaration(parameter_inference, parameter_definition);
                assert!(declared.qualifiers().is_empty());
                declared.inner_type()
            }
            Syntax::Legacy => parameter_inference.binding_type(parameter_definition),
        };
        let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = parameter_type else {
            panic!("the parameter definition produces the actual TypeVar instance");
        };
        assert_eq!(bound.typevar(&db), typevar);
        assert_eq!(typevar.definition(&db), Some(parameter_definition));
        let context = class.generic_context(&db).expect("the class is generic");
        assert_eq!(context.variables(&db).collect::<Vec<_>>(), [bound]);
        let pep695_context = class.pep695_generic_context(&db);
        let explicit_bases = class.explicit_bases(&db);
        match syntax {
            Syntax::Pep695 => {
                assert!(class.has_type_params(&db));
                assert_eq!(pep695_context, Some(context));
                if name == "C" {
                    assert!(!class.has_explicit_bases(&db));
                    assert!(explicit_bases.is_empty());
                } else {
                    assert!(class.has_explicit_bases(&db));
                    assert_eq!(
                        explicit_bases,
                        [Type::SpecialForm(SpecialFormType::Protocol)]
                    );
                }
            }
            Syntax::Legacy => {
                assert!(!class.has_type_params(&db));
                assert!(class.has_explicit_bases(&db));
                assert_eq!(pep695_context, None);
                let base = if name == "C" {
                    KnownInstanceType::SubscriptedGeneric(context)
                } else {
                    KnownInstanceType::SubscriptedProtocol(context)
                };
                assert_eq!(explicit_bases, [Type::KnownInstance(base)]);
            }
        }
        PreparedClass {
            definitions: [class_definition, parameter_definition, child_definition],
            class,
            context,
            pep695_context,
            explicit_bases,
            body,
            places: body_places,
            uses: body_uses,
            inference: child_inference,
            declared: child_type,
        }
    });
    assert_ne!(
        prepared[0].declared.inner_type(),
        prepared[1].declared.inner_type()
    );
    let expected_keys = match syntax {
        Syntax::Pep695 => {
            assert_ne!(prepared[0].definitions[1], prepared[1].definitions[1]);
            6
        }
        Syntax::Legacy => {
            assert_eq!(prepared[0].definitions[1], prepared[1].definitions[1]);
            5
        }
    };

    let ingredient = infer_definition_types::fn_ingredient_(&db, db.zalsa());
    let mut definitions = prepared
        .iter()
        .flat_map(|class| class.definitions)
        .collect::<Vec<_>>();
    definitions.sort_by_key(|definition| definition.as_id());
    definitions.dedup();
    let certificates = definitions
        .iter()
        .map(|definition| {
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, definition.as_id()).unwrap_or_else(
                |error| {
                    panic!(
                        "infer_definition_types({:?}) is ineligible: {error:?}",
                        definition.as_id()
                    )
                },
            )
        })
        .collect::<Vec<_>>();
    let mut keys = certificates
        .iter()
        .map(FinalSourceMemo::database_key)
        .collect::<Vec<_>>();
    assert_eq!(keys.len(), expected_keys);

    let places_ingredient = place_table_ingredient(&db);
    let uses_ingredient = use_def_map_ingredient(&db);
    let bodies = [prepared[0].body, prepared[1].body];
    assert_ne!(bodies[0], bodies[1]);
    let mut sorted_bodies = bodies;
    sorted_bodies.sort_by_key(|body| body.as_id());
    let places_certificates = sorted_bodies.map(|body| {
        FinalSourceMemo::certify(
            &db as &dyn ty_python_core::Db,
            places_ingredient,
            body.as_id(),
        )
        .unwrap_or_else(|error| panic!("place_table({:?}) is ineligible: {error:?}", body.as_id()))
    });
    let uses_certificates = sorted_bodies.map(|body| {
        FinalSourceMemo::certify(
            &db as &dyn ty_python_core::Db,
            uses_ingredient,
            body.as_id(),
        )
        .unwrap_or_else(|error| panic!("use_def_map({:?}) is ineligible: {error:?}", body.as_id()))
    });
    for (position, body) in sorted_bodies.into_iter().enumerate() {
        assert_eq!(
            places_certificates[position].database_key(),
            places_ingredient.database_key_index(body.as_id()),
        );
        assert_eq!(
            uses_certificates[position].database_key(),
            uses_ingredient.database_key_index(body.as_id()),
        );
    }
    keys.extend(
        places_certificates
            .iter()
            .map(FinalSourceMemo::database_key),
    );
    keys.extend(uses_certificates.iter().map(FinalSourceMemo::database_key));
    assert_eq!(keys.len(), expected_keys + 4);

    let context_ingredient = static_class_generic_context_ingredient(&db);
    let classes = [prepared[0].class, prepared[1].class];
    let mut sorted_classes = classes;
    sorted_classes.sort_by_key(|class| class.as_id());
    let context_certificates = sorted_classes.map(|class| {
        FinalSourceMemo::certify(&db as &dyn Db, context_ingredient, class.as_id()).unwrap_or_else(
            |error| {
                panic!(
                    "generic_context({:?}) is ineligible: {error:?}",
                    class.as_id()
                )
            },
        )
    });
    let context_keys = context_certificates
        .each_ref()
        .map(FinalSourceMemo::database_key);
    for (class, key) in sorted_classes.into_iter().zip(context_keys) {
        assert_eq!(key, context_ingredient.database_key_index(class.as_id()));
    }
    keys.extend(context_keys);
    assert_eq!(keys.len(), expected_keys + 6);

    let pep695_ingredient = pep695_generic_context_ingredient(&db);
    let bases_ingredient = explicit_bases_ingredient(&db);
    let mut pep695_certificates = Vec::new();
    let mut bases_certificates = Vec::new();
    let mut absent_keys = Vec::new();
    for class in sorted_classes {
        let certificate =
            FinalSourceMemo::certify(&db as &dyn Db, pep695_ingredient, class.as_id());
        let key = pep695_ingredient.database_key_index(class.as_id());
        if class.has_type_params(&db) {
            let certificate = certificate.unwrap_or_else(|error| {
                panic!(
                    "pep695_generic_context_inner({:?}) is ineligible: {error:?}",
                    class.as_id()
                )
            });
            assert_eq!(certificate.database_key(), key);
            pep695_certificates.push(certificate);
        } else {
            assert!(
                matches!(certificate, Err(FinalSourceError::MissingMemo)),
                "{certificate:?}"
            );
            absent_keys.push(key);
        }
        let certificate = FinalSourceMemo::certify(&db as &dyn Db, bases_ingredient, class.as_id());
        let key = bases_ingredient.database_key_index(class.as_id());
        if class.has_explicit_bases(&db) {
            let certificate = certificate.unwrap_or_else(|error| {
                panic!(
                    "explicit_bases_inner({:?}) is ineligible: {error:?}",
                    class.as_id()
                )
            });
            assert_eq!(certificate.database_key(), key);
            bases_certificates.push(certificate);
        } else {
            assert!(
                matches!(certificate, Err(FinalSourceError::MissingMemo)),
                "{certificate:?}"
            );
            absent_keys.push(key);
        }
    }
    assert_eq!(
        (
            pep695_certificates.len(),
            bases_certificates.len(),
            absent_keys.len()
        ),
        match syntax {
            Syntax::Pep695 => (2, 1, 1),
            Syntax::Legacy => (0, 2, 2),
        },
    );

    let env = db.program_environment();
    let known_classes = [KnownClass::Object, KnownClass::Int];
    let known_literals = known_classes.map(|known| {
        let class = known
            .try_to_class_literal(&db, &env)
            .expect("the built-in class exists");
        assert_eq!(class.known(&db), Some(known));
        class
    });
    let known_ingredient = known_class_to_class_literal_ingredient(&db);
    let known_ids = known_classes.map(|known| {
        let id = known_class_to_class_literal_key(&db, known, env.program(&db));
        assert_eq!(
            id,
            known_class_to_class_literal_key(&db, known, env.program(&db))
        );
        id
    });
    assert_ne!(known_ids[0], known_ids[1]);
    let mut sorted_known_ids = known_ids;
    sorted_known_ids.sort();
    let known_certificates = sorted_known_ids.map(|id| {
        let certificate = FinalSourceMemo::certify(&db as &dyn Db, known_ingredient, id)
            .unwrap_or_else(|error| {
                panic!("known_class_to_class_literal({id:?}) is ineligible: {error:?}")
            });
        assert_eq!(
            certificate.database_key(),
            known_ingredient.database_key_index(id)
        );
        certificate
    });
    let mut declaration_keys = context_keys.to_vec();
    for key in pep695_certificates
        .iter()
        .map(FinalSourceMemo::database_key)
        .chain(bases_certificates.iter().map(FinalSourceMemo::database_key))
        .chain(known_certificates.iter().map(FinalSourceMemo::database_key))
    {
        keys.push(key);
        declaration_keys.push(key);
    }
    assert_eq!(
        keys.len(),
        match syntax {
            Syntax::Pep695 => 17,
            Syntax::Legacy => 15,
        }
    );

    // Keep the setup footprint: ordinary declaration preparation can execute other queries.
    let mut event_reader = db.clone();
    let preparation_events = event_reader.take_salsa_events();
    let preparation_queries = executed(&preparation_events);
    for key in &keys {
        assert!(
            preparation_queries.contains(key),
            "source {key:?} was not prepared"
        );
    }
    for key in declaration_keys {
        assert_eq!(
            preparation_queries
                .iter()
                .filter(|executed| **executed == key)
                .count(),
            1,
            "the canonical declaration body executes once for {key:?}",
        );
    }
    for key in &absent_keys {
        assert!(
            !preparation_queries.contains(key),
            "shortcut unexpectedly executed {key:?}"
        );
    }
    for class in &prepared {
        assert_eq!(class.class.generic_context(&db), Some(class.context));
        assert_eq!(
            class.class.pep695_generic_context(&db),
            class.pep695_context
        );
        let bases = class.class.explicit_bases(&db);
        assert_eq!(bases, class.explicit_bases);
        if class.class.has_explicit_bases(&db) {
            assert!(std::ptr::eq(bases, class.explicit_bases));
        }
    }
    for (known, expected) in known_classes.into_iter().zip(known_literals) {
        assert_eq!(known.try_to_class_literal(&db, &env), Some(expected));
    }
    let reuse_events = event_reader.take_salsa_events();
    let reuse_queries = executed(&reuse_events);
    assert!(
        reuse_queries.is_empty(),
        "ordinary declaration reuse executed {reuse_queries:?}"
    );
    let revision = db.zalsa().current_revision();
    let db_view = &db as &dyn Db;

    for root in 0..2 {
        let result = try_with_attempt(&db, 100_000, || {
            let mut registry = RegistryBuilder::new(&db, &Admit)?;
            let route =
                registry.register_final_source(&db as &dyn Db, ingredient, &certificates)?;
            let places_route = registry.register_final_source(
                &db as &dyn ty_python_core::Db,
                places_ingredient,
                &places_certificates,
            )?;
            let uses_route = registry.register_final_source(
                &db as &dyn ty_python_core::Db,
                uses_ingredient,
                &uses_certificates,
            )?;
            let context_route = registry.register_final_source(
                &db as &dyn Db,
                context_ingredient,
                &context_certificates,
            )?;
            // A shortcut has no memo to register or read, including no empty route.
            let pep695_route = if pep695_certificates.is_empty() {
                None
            } else {
                Some(registry.register_final_source(
                    db_view,
                    pep695_ingredient,
                    &pep695_certificates,
                )?)
            };
            let bases_route =
                registry.register_final_source(db_view, bases_ingredient, &bases_certificates)?;
            let known_route =
                registry.register_final_source(db_view, known_ingredient, &known_certificates)?;
            registry.seal()?.run(move |endpoint| async move {
                let read_child = async |body: ScopeId<'_>, class: StaticClassLiteral<'_>| {
                    let places = endpoint
                        .read_final_source(&places_route, body.as_id())
                        .await;
                    let uses = endpoint.read_final_source(&uses_route, body.as_id()).await;
                    let child = child_definition(places, uses);
                    let inference = endpoint.read_final_source(&route, child.as_id()).await;
                    let context = endpoint
                        .read_final_source(&context_route, class.as_id())
                        .await;
                    let pep695_context = if class.has_type_params(db_view) {
                        let route = pep695_route
                            .as_ref()
                            .expect("PEP 695 declarations have a route");
                        Some(endpoint.read_final_source(route, class.as_id()).await)
                    } else {
                        None
                    };
                    let bases = if class.has_explicit_bases(db_view) {
                        Some(
                            endpoint
                                .read_final_source(&bases_route, class.as_id())
                                .await,
                        )
                    } else {
                        None
                    };
                    (
                        places,
                        uses,
                        child,
                        inference,
                        declaration(inference, child),
                        context,
                        pep695_context,
                        bases,
                    )
                };
                let children = [
                    read_child(bodies[0], classes[0]).await,
                    read_child(bodies[1], classes[1]).await,
                ];
                let known = [
                    endpoint.read_final_source(&known_route, known_ids[0]).await,
                    endpoint.read_final_source(&known_route, known_ids[1]).await,
                ];
                Ok((children, known))
            })
        });
        let root_events = event_reader.take_salsa_events();
        let root_queries = executed(&root_events);
        assert!(
            root_queries.is_empty(),
            "root {root} executed query bodies: {root_queries:?}"
        );
        assert_eq!(db.zalsa().current_revision(), revision);
        let Ok(AttemptOutcome::Complete(Ok((actual, known)))) = result else {
            panic!("root {root} failed: {result:?}");
        };
        for (
            position,
            (places, uses, child, inference, declared, context, pep695_context, bases),
        ) in actual.into_iter().enumerate()
        {
            assert!(std::ptr::eq(places.as_ref(), prepared[position].places));
            assert!(std::ptr::eq(uses.as_ref(), prepared[position].uses));
            assert_eq!(child, prepared[position].definitions[2]);
            assert!(std::ptr::eq(inference, prepared[position].inference));
            assert_eq!(declared, prepared[position].declared);
            assert_eq!(*context, Some(prepared[position].context));
            match syntax {
                Syntax::Pep695 => {
                    assert_eq!(
                        *pep695_context.expect("the inner context memo exists"),
                        prepared[position].pep695_context
                    );
                }
                Syntax::Legacy => assert!(pep695_context.is_none()),
            }
            if let Some(bases) = bases {
                assert_eq!(bases.as_ref(), prepared[position].explicit_bases);
                assert!(std::ptr::eq(
                    bases.as_ref(),
                    prepared[position].explicit_bases
                ));
            } else {
                assert!(!prepared[position].class.has_explicit_bases(&db));
                assert!(prepared[position].explicit_bases.is_empty());
            }
            let Type::TypeVar(bound) = declared.inner_type() else {
                panic!("the selected declaration retains its TypeVar");
            };
            assert_eq!(
                bound.binding_context(&db).definition(),
                Some(prepared[position].definitions[0]),
            );
            assert_eq!(
                context
                    .expect("the selected context is generic")
                    .variables(&db)
                    .collect::<Vec<_>>(),
                [bound],
            );
        }
        for (stored, expected) in known.into_iter().zip(known_literals) {
            // The public Option also accepts a possibly unbound class, so compare the stored Result.
            assert_eq!(*stored, Ok(Some(expected)));
        }
        for class in classes {
            if !class.has_type_params(&db) {
                assert!(matches!(
                    FinalSourceMemo::certify(db_view, pep695_ingredient, class.as_id()),
                    Err(FinalSourceError::MissingMemo),
                ));
            }
            if !class.has_explicit_bases(&db) {
                assert!(matches!(
                    FinalSourceMemo::certify(db_view, bases_ingredient, class.as_id()),
                    Err(FinalSourceError::MissingMemo),
                ));
            }
        }
    }
    let preparation_names = preparation_queries
        .iter()
        .map(|key| (db.ingredient_debug_name(key.ingredient_index()), *key))
        .collect::<Vec<_>>();
    eprintln!(
        "FINAL_SOURCE_DECLARATIONS syntax={syntax:?} preparation={preparation_names:?} certified={keys:?} absent={absent_keys:?}"
    );
    Ok(())
}
