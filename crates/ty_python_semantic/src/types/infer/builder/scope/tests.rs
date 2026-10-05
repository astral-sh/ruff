use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use ruff_db::files::system_path_to_file;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_text_size::{Ranged, TextRange};
use salsa::attempt_probe::{AttemptOutcome, Incomplete, try_with_attempt};
use salsa::execution_probe::{
    ExecutionAdmission, ExecutionWork, RegistryBuilder, RunError, RunResult, TaskEndpoint,
};
use ty_python_core::{SemanticIndex, global_scope, place_table, semantic_index, use_def_map};

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::diagnostic::INVALID_ASSIGNMENT;
use crate::types::infer::{infer_definition_types, infer_scope_types};
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::{SpecialFormType, TypeAndQualifiers};
use crate::{Db, ProgramEnvironment};

const PATH: &str = "/src/scope.py";
const SOURCE: &str = "from typing import TypeAlias\ndef f(x: int = 1) -> int: return x\nclass C: pass\nAlias: TypeAlias = int\n";

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new().with_file(PATH, SOURCE).build()
}

struct Fixture<'db> {
    db: &'db TestDb,
    scope: ScopeId<'db>,
    index: &'db SemanticIndex<'db>,
    node: &'db NodeWithScopeKind,
    definitions: [Definition<'db>; 3],
    kinds: [&'db DefinitionKind<'db>; 3],
    function: FunctionType<'db>,
    class_type: Type<'db>,
    inference: &'db DefinitionInference<'db>,
}

impl<'db> Fixture<'db> {
    fn new(db: &'db TestDb) -> anyhow::Result<Self> {
        let file = db.program_file(system_path_to_file(db, PATH)?);
        let scope = global_scope(db, file);
        let places = place_table(db, scope);
        let uses = use_def_map(db, scope);
        let definition = |name| -> anyhow::Result<_> {
            let symbol = places
                .symbol_id(name)
                .ok_or_else(|| anyhow::anyhow!("missing symbol {name}"))?;
            uses.end_of_scope_symbol_bindings(symbol)
                .find_map(|binding| binding.binding.definition())
                .ok_or_else(|| anyhow::anyhow!("missing definition {name}"))
        };
        let definitions = [definition("f")?, definition("C")?, definition("Alias")?];
        let function_type = global_symbol(db, file, "f")
            .place
            .ignore_possibly_undefined()
            .ok_or_else(|| anyhow::anyhow!("missing function type"))?;
        let Type::FunctionLiteral(function) = function_type else {
            anyhow::bail!("expected function literal");
        };
        let class_type = global_symbol(db, file, "C")
            .place
            .ignore_possibly_undefined()
            .ok_or_else(|| anyhow::anyhow!("missing class type"))?;
        Ok(Self {
            db,
            scope,
            index: semantic_index(db, file),
            node: scope.node(db),
            kinds: definitions.map(|definition| definition.kind(db)),
            inference: infer_definition_types(db, definitions[1]),
            definitions,
            function,
            class_type,
        })
    }

    fn module(&self) -> ParsedModuleRef {
        parsed_module(
            self.db,
            self.scope.program_file(self.db).python_file(self.db),
        )
        .load(self.db)
    }

    fn builder<'ast>(
        &self,
        module: &'ast ParsedModuleRef,
        env: &'ast ProgramEnvironment<'db>,
    ) -> TypeInferenceBuilder<'db, 'ast> {
        let file = self.scope.program_file(self.db);
        TypeInferenceBuilder::new(
            self.db,
            env,
            InferenceRegion::Scope(self.scope, TypeContext::default()),
            file.file(self.db),
            file,
            self.index,
            module,
        )
    }

    // The supplied phase controls start with real storage and identities, without running a native
    // source checker inside the driver. These answers do not implement controlled source inference.
    fn seed(&self, builder: &mut TypeInferenceBuilder<'db, '_>) {
        builder
            .deferred
            .0
            .extend(self.definitions[..2].iter().copied());
        builder.declarations.0.extend([
            (
                self.definitions[0],
                TypeAndQualifiers::declared(Type::FunctionLiteral(self.function)),
            ),
            (
                self.definitions[1],
                TypeAndQualifiers::declared(self.class_type),
            ),
            (
                self.definitions[2],
                TypeAndQualifiers::declared(Type::unknown()),
            ),
        ]);
        builder.called_functions.insert(self.function);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event<'db> {
    Node(ScopeId<'db>),
    Body(&'static str, Option<TextRange>, Option<TypeContext<'db>>),
    Take,
    Deferred(usize, Definition<'db>),
    Kind(Definition<'db>),
    DeferredAnnotations(TextRange),
    ParameterDefaults(TextRange),
    DeferredTypes(Definition<'db>),
    FunctionDefaultTypes(Definition<'db>),
    Merge(Definition<'db>),
    DeferredEmpty,
    CheckFile,
    Seen,
    Declaration(usize, Definition<'db>, Type<'db>),
    FunctionDecorators(Definition<'db>, TextRange),
    FunctionDefinition(Definition<'db>),
    Overloaded(Type<'db>, Definition<'db>),
    TypeGuard(Type<'db>, TextRange),
    ClassDecorators(Definition<'db>, TextRange),
    OriginalClass(Definition<'db>),
    StaticClass(Type<'db>, TextRange),
    Annotation(TextRange),
    Alias(Definition<'db>),
    Dynamic(Definition<'db>),
    Called(usize, FunctionType<'db>),
    FunctionId(FunctionType<'db>),
    Final,
    Finish,
    Freeze(bool),
}

impl Event<'_> {
    fn local(&self) -> bool {
        matches!(
            self,
            Self::Take
                | Self::Deferred(..)
                | Self::DeferredAnnotations(..)
                | Self::ParameterDefaults(..)
                | Self::Declaration(..)
                | Self::Called(..)
                | Self::DeferredEmpty
                | Self::Seen
                | Self::Finish
                | Self::Freeze(..)
        )
    }
    fn charged(&self) -> bool {
        matches!(
            self,
            Self::Take
                | Self::Deferred(..)
                | Self::DeferredAnnotations(..)
                | Self::ParameterDefaults(..)
                | Self::Declaration(..)
                | Self::Called(..)
                | Self::Finish
                | Self::Freeze(..)
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Boundary<'db> {
    Refused(Event<'db>),
    Finalization,
    UnsupportedRegion,
}

#[derive(Default)]
struct Journal {
    owner_live: Cell<bool>,
    children: Cell<usize>,
    pending: Cell<usize>,
    drops: RefCell<Vec<&'static str>>,
    deferred_pointer: Cell<usize>,
    deferred_reads: Cell<usize>,
    commits: Cell<usize>,
    resumed: Cell<bool>,
}

struct Child(Rc<Journal>);
impl Drop for Child {
    fn drop(&mut self) {
        assert!(self.0.owner_live.get());
        self.0.children.set(self.0.children.get() - 1);
        self.0.drops.borrow_mut().push("child");
    }
}

struct BuilderBorrow<'a, 'db, 'ast> {
    builder: &'a TypeInferenceBuilder<'db, 'ast>,
    journal: Rc<Journal>,
    deferred_len: usize,
}
impl Drop for BuilderBorrow<'_, '_, '_> {
    fn drop(&mut self) {
        assert_eq!(self.journal.children.get(), 0);
        assert!(!self.builder.module().suite().is_empty());
        assert_eq!(self.builder.declarations.0.len(), 3);
        assert_eq!(self.builder.deferred.0.len(), self.deferred_len);
        self.journal.drops.borrow_mut().push("builder-borrow");
    }
}

struct DeferredBorrow<'a, 'db> {
    definitions: &'a [Definition<'db>],
    expected: [Definition<'db>; 2],
    journal: Rc<Journal>,
}
impl Drop for DeferredBorrow<'_, '_> {
    fn drop(&mut self) {
        assert_eq!(self.journal.children.get(), 0);
        assert_eq!(self.definitions, self.expected);
        assert_eq!(
            self.definitions.as_ptr() as usize,
            self.journal.deferred_pointer.get()
        );
        self.journal
            .deferred_reads
            .set(self.journal.deferred_reads.get() + 1);
        self.journal.drops.borrow_mut().push("deferred-borrow");
    }
}

struct Effects<'call, 'run, 'db: 'run> {
    fixture: &'call Fixture<'db>,
    endpoint: Option<&'call TaskEndpoint<'run, 'db>>,
    events: Rc<RefCell<Vec<Event<'db>>>>,
    reject_at: Option<usize>,
    check_file: bool,
    journal: Rc<Journal>,
    admission: Option<&'call Admission<'run, 'db>>,
}
impl<'call, 'run, 'db: 'run> Effects<'call, 'run, 'db> {
    fn new(fixture: &'call Fixture<'db>) -> Self {
        Self {
            fixture,
            endpoint: None,
            events: Rc::default(),
            reject_at: None,
            check_file: true,
            journal: Rc::default(),
            admission: None,
        }
    }
    fn before_sync(&self, event: Event<'db>) -> Result<(), Boundary<'db>> {
        let mut events = self.events.borrow_mut();
        let position = events.len();
        events.push(event.clone());
        if self.reject_at == Some(position) {
            Err(Boundary::Refused(event))
        } else {
            Ok(())
        }
    }
    async fn before(&self, event: Event<'db>) -> Result<(), Boundary<'db>> {
        let Some(endpoint) = self.endpoint else {
            return self.before_sync(event);
        };
        if event.local() {
            endpoint
                .local_call(|| {
                    let charged = event.charged();
                    let result = self.before_sync(event);
                    if charged {
                        if let Some(admission) = self.admission {
                            admission.active.set(true);
                        }
                        let admitted = endpoint.admit_work(1);
                        if let Some(admission) = self.admission {
                            admission.active.set(false);
                        }
                        admitted?;
                        endpoint.check_completion()?;
                    }
                    endpoint.check_completion()?;
                    Ok(result)
                })
                .await
        } else {
            endpoint
                .child_call(|| async {
                    let refused = self.before_sync(event).is_err();
                    let journal = self.journal.clone();
                    endpoint
                        .demand(move || {
                            assert!(journal.owner_live.get());
                            journal.children.set(journal.children.get() + 1);
                            let child = Child(journal);
                            async move {
                                let _child = child;
                                if refused {
                                    Err(RunError::Refused(Incomplete::Allowance))
                                } else {
                                    Ok(Ok(()))
                                }
                            }
                        })?
                        .await
                })
                .await
        }
    }
    fn builder_borrow<'a, 'ast>(
        &self,
        builder: &'a TypeInferenceBuilder<'db, 'ast>,
    ) -> Option<BuilderBorrow<'a, 'db, 'ast>> {
        self.endpoint.map(|_| BuilderBorrow {
            builder,
            journal: self.journal.clone(),
            deferred_len: builder.deferred.0.len(),
        })
    }
}

// Only the call/await spelling differs between these supplied providers. The production ordinary
// provider above still calls the original semantic owners; none of those owners runs here.
macro_rules! supplied_scope_effects {
    ($trait:ident, [$($async:tt)*], $before:ident, [$($await:tt)*]) => {
        impl<'db, 'ast, 'run> $trait<'db, 'ast> for Effects<'_, 'run, 'db> where 'db: 'run {
            type Error = Boundary<'db>;
            $($async)* fn scope_node(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, scope: ScopeId<'db>) -> Result<&'db NodeWithScopeKind, Self::Error> {
                self.$before(Event::Node(scope)) $($await)*?;
                Ok(self.fixture.node)
            }
            $($async)* fn infer_module(&self, _builder: &mut TypeInferenceBuilder<'db, 'ast>) -> Result<(), Self::Error> {
                self.$before(Event::Body("module", None, None)) $($await)*
            }
            $($async)* fn infer_function(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, node: &AstNodeRef<ast::StmtFunctionDef>) -> Result<(), Self::Error> {
                self.$before(Event::Body("function", Some(node.node(builder.module()).range()), None)) $($await)*
            }
            $($async)* fn infer_lambda(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, node: &AstNodeRef<ast::ExprLambda>, tcx: TypeContext<'db>) -> Result<(), Self::Error> {
                self.$before(Event::Body("lambda", Some(node.node(builder.module()).range()), Some(tcx))) $($await)*
            }
            $($async)* fn infer_class(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, node: &AstNodeRef<ast::StmtClassDef>) -> Result<(), Self::Error> {
                self.$before(Event::Body("class", Some(node.node(builder.module()).range()), None)) $($await)*
            }
            $($async)* fn infer_class_type_parameters(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, node: &AstNodeRef<ast::StmtClassDef>) -> Result<(), Self::Error> {
                self.$before(Event::Body("class parameters", Some(node.node(builder.module()).range()), None)) $($await)*
            }
            $($async)* fn infer_function_type_parameters(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, node: &AstNodeRef<ast::StmtFunctionDef>) -> Result<(), Self::Error> {
                self.$before(Event::Body("function parameters", Some(node.node(builder.module()).range()), None)) $($await)*
            }
            $($async)* fn infer_type_alias_type_parameters(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, node: &AstNodeRef<ast::StmtTypeAlias>) -> Result<(), Self::Error> {
                self.$before(Event::Body("alias parameters", Some(node.node(builder.module()).range()), None)) $($await)*
            }
            $($async)* fn infer_type_alias(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, node: &AstNodeRef<ast::StmtTypeAlias>) -> Result<(), Self::Error> {
                self.$before(Event::Body("alias", Some(node.node(builder.module()).range()), None)) $($await)*
            }
            $($async)* fn infer_list_comprehension(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, node: &AstNodeRef<ast::ExprListComp>, tcx: TypeContext<'db>) -> Result<(), Self::Error> {
                self.$before(Event::Body("list comprehension", Some(node.node(builder.module()).range()), Some(tcx))) $($await)*
            }
            $($async)* fn infer_set_comprehension(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, node: &AstNodeRef<ast::ExprSetComp>, tcx: TypeContext<'db>) -> Result<(), Self::Error> {
                self.$before(Event::Body("set comprehension", Some(node.node(builder.module()).range()), Some(tcx))) $($await)*
            }
            $($async)* fn infer_dict_comprehension(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, node: &AstNodeRef<ast::ExprDictComp>, tcx: TypeContext<'db>) -> Result<(), Self::Error> {
                self.$before(Event::Body("dict comprehension", Some(node.node(builder.module()).range()), Some(tcx))) $($await)*
            }
            $($async)* fn infer_generator(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, node: &AstNodeRef<ast::ExprGenerator>, tcx: TypeContext<'db>) -> Result<(), Self::Error> {
                self.$before(Event::Body("generator", Some(node.node(builder.module()).range()), Some(tcx))) $($await)*
            }
            $($async)* fn take_deferred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> Result<Vec<Definition<'db>>, Self::Error> {
                let retained = self.builder_borrow(builder);
                self.$before(Event::Take) $($await)*?;
                drop(retained);
                let definitions = std::mem::take(&mut builder.deferred.0);
                self.journal.commits.set(self.journal.commits.get() + 1);
                self.journal.deferred_pointer.set(definitions.as_ptr() as usize);
                Ok(definitions)
            }
            $($async)* fn next_deferred(&self, definitions: &[Definition<'db>], cursor: &mut usize) -> Result<Option<Definition<'db>>, Self::Error> {
                let Some(definition) = definitions.get(*cursor).copied() else { return Ok(None); };
                let retained = self.endpoint.map(|_| DeferredBorrow { definitions, expected: [self.fixture.definitions[0], self.fixture.definitions[1]], journal: self.journal.clone() });
                let position = CursorBorrow { cursor, expected: *cursor, journal: self.journal.clone() };
                self.$before(Event::Deferred(*cursor, definition)) $($await)*?;
                drop(retained);
                drop(position);
                *cursor += 1;
                self.journal.commits.set(self.journal.commits.get() + 1);
                Ok(Some(definition))
            }
            $($async)* fn definition_kind(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<&'db DefinitionKind<'db>, Self::Error> {
                self.$before(Event::Kind(definition)) $($await)*?;
                let position = self.fixture.definitions.iter().position(|candidate| *candidate == definition).ok_or(Boundary::UnsupportedRegion)?;
                Ok(self.fixture.kinds[position])
            }
            $($async)* fn has_deferred_annotations(&self, builder: &TypeInferenceBuilder<'db, 'ast>, function: &FunctionDefinitionKind) -> Result<bool, Self::Error> {
                let _retained = self.builder_borrow(builder);
                let function = function.node(builder.module());
                self.$before(Event::DeferredAnnotations(function.range())) $($await)*?;
                self.journal.commits.set(self.journal.commits.get() + 1);
                Ok(function_has_deferred_annotations(function))
            }
            $($async)* fn has_parameter_defaults(&self, builder: &TypeInferenceBuilder<'db, 'ast>, function: &FunctionDefinitionKind) -> Result<bool, Self::Error> {
                let _retained = self.builder_borrow(builder);
                let function = function.node(builder.module());
                self.$before(Event::ParameterDefaults(function.range())) $($await)*?;
                self.journal.commits.set(self.journal.commits.get() + 1);
                Ok(parameters_have_defaults(&function.parameters))
            }
            $($async)* fn function_default_types(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<&'db DefinitionInference<'db>, Self::Error> {
                self.$before(Event::FunctionDefaultTypes(definition)) $($await)*?;
                Ok(self.fixture.inference)
            }
            $($async)* fn deferred_types(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<&'db DefinitionInference<'db>, Self::Error> {
                self.$before(Event::DeferredTypes(definition)) $($await)*?;
                Ok(self.fixture.inference)
            }
            $($async)* fn extend_definition(&self, _builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, inferred: &DefinitionInference<'db>) -> Result<(), Self::Error> {
                assert!(std::ptr::eq(inferred, self.fixture.inference));
                self.$before(Event::Merge(definition)) $($await)*
            }
            $($async)* fn check_deferred_empty(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<(), Self::Error> {
                self.$before(Event::DeferredEmpty) $($await)*?;
                assert!(builder.deferred.is_empty());
                Ok(())
            }
            $($async)* fn should_check_file(&self, _builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error> {
                self.$before(Event::CheckFile) $($await)*?;
                Ok(self.check_file)
            }
            $($async)* fn seen_functions(&self) -> Result<SeenFunctions<'db>, Self::Error> {
                self.$before(Event::Seen) $($await)*?;
                Ok(SeenFunctions { overloaded_places: FxHashSet::default(), public_functions: FxHashSet::default() })
            }
            $($async)* fn next_declaration(&self, builder: &TypeInferenceBuilder<'db, 'ast>, cursor: &mut usize) -> Result<Option<(Definition<'db>, Type<'db>)>, Self::Error> {
                let Some((definition, ty)) = builder.declarations.0.get(*cursor).map(|(definition, ty)| (*definition, ty.inner_type())) else { return Ok(None); };
                let position = CursorBorrow { cursor, expected: *cursor, journal: self.journal.clone() };
                self.$before(Event::Declaration(*cursor, definition, ty)) $($await)*?;
                drop(position);
                *cursor += 1;
                self.journal.commits.set(self.journal.commits.get() + 1);
                Ok(Some((definition, ty)))
            }
            $($async)* fn function_decorators(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, function: &FunctionDefinitionKind) -> Result<(), Self::Error> {
                let _retained = self.builder_borrow(builder);
                self.$before(Event::FunctionDecorators(definition, function.node(builder.module()).range())) $($await)*
            }
            $($async)* fn function_definition(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<(), Self::Error> {
                self.$before(Event::FunctionDefinition(definition)) $($await)*
            }
            $($async)* fn overloaded_function(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, definition: Definition<'db>, seen: &mut SeenFunctions<'db>) -> Result<(), Self::Error> {
                assert!(seen.overloaded_places.is_empty() && seen.public_functions.is_empty());
                self.$before(Event::Overloaded(ty, definition)) $($await)*
            }
            $($async)* fn type_guard_definition(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, function: &FunctionDefinitionKind) -> Result<(), Self::Error> {
                self.$before(Event::TypeGuard(ty, function.node(builder.module()).range())) $($await)*
            }
            $($async)* fn class_decorators(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, class: &AstNodeRef<ast::StmtClassDef>) -> Result<(), Self::Error> {
                self.$before(Event::ClassDecorators(definition, class.node(builder.module()).range())) $($await)*
            }
            $($async)* fn original_class_type(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<Option<Type<'db>>, Self::Error> {
                self.$before(Event::OriginalClass(definition)) $($await)*?;
                Ok(Some(self.fixture.class_type))
            }
            $($async)* fn static_class(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, class: &AstNodeRef<ast::StmtClassDef>) -> Result<(), Self::Error> {
                self.$before(Event::StaticClass(ty, class.node(builder.module()).range())) $($await)*
            }
            $($async)* fn annotation_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, assignment: &AnnotatedAssignmentDefinitionKind) -> Result<Type<'db>, Self::Error> {
                self.$before(Event::Annotation(assignment.annotation(builder.module()).range())) $($await)*?;
                Ok(Type::SpecialForm(SpecialFormType::TypeAlias))
            }
            $($async)* fn mark_implicit_alias(&self, _builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<(), Self::Error> {
                // This supplied answer tests the branch, not controlled hash-set growth.
                self.$before(Event::Alias(definition)) $($await)*
            }
            $($async)* fn dynamic_class(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<(), Self::Error> {
                self.$before(Event::Dynamic(definition)) $($await)*
            }
            $($async)* fn next_called_function(&self, builder: &TypeInferenceBuilder<'db, 'ast>, cursor: &mut usize) -> Result<Option<FunctionType<'db>>, Self::Error> {
                let Some(function) = builder.called_functions.get_index(*cursor).copied() else { return Ok(None); };
                let position = CursorBorrow { cursor, expected: *cursor, journal: self.journal.clone() };
                self.$before(Event::Called(*cursor, function)) $($await)*?;
                drop(position);
                *cursor += 1;
                self.journal.commits.set(self.journal.commits.get() + 1);
                Ok(Some(function))
            }
            $($async)* fn function_definition_id(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, function: FunctionType<'db>) -> Result<Definition<'db>, Self::Error> {
                self.$before(Event::FunctionId(function)) $($await)*?;
                Ok(self.fixture.definitions[0])
            }
            $($async)* fn final_without_value(&self, _builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<(), Self::Error> {
                self.$before(Event::Final) $($await)*
            }
        }
    };
}
supplied_scope_effects!(SynchronousScopeEffects, [], before_sync, []);
supplied_scope_effects!(ScopeEffects, [async], before, [.await]);

impl<'db, 'ast, 'run> FinishScopeEffects<'db, 'ast> for Effects<'_, 'run, 'db>
where
    'db: 'run,
{
    type Error = Boundary<'db>;
    async fn infer_region(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<(), Self::Error> {
        let InferenceRegion::Scope(scope, tcx) = builder.region else {
            return Err(Boundary::UnsupportedRegion);
        };
        infer_scope_with(builder, scope, tcx, ScopeFacts, self).await
    }
    async fn finish_context(
        &self,
        builder: TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<ScopeFinishData<'db>, Self::Error> {
        let _retained = self.builder_borrow(&builder);
        self.before(Event::Finish).await?;
        Err(Boundary::Finalization)
    }
    async fn freeze_with_extra(
        &self,
        data: ScopeFinishData<'db>,
    ) -> Result<ScopeInference<'db>, Self::Error> {
        let _retained = DataBorrow {
            data: &data,
            journal: self.journal.clone(),
        };
        self.before(Event::Freeze(true)).await?;
        assert!(!data.expressions.is_empty());
        Err(Boundary::Finalization)
    }
    async fn freeze_without_extra(
        &self,
        data: ScopeFinishData<'db>,
    ) -> Result<ScopeInference<'db>, Self::Error> {
        let _retained = DataBorrow {
            data: &data,
            journal: self.journal.clone(),
        };
        self.before(Event::Freeze(false)).await?;
        assert!(!data.expressions.is_empty());
        Err(Boundary::Finalization)
    }
}

fn expected_events<'db>(
    fixture: &Fixture<'db>,
    module: &ParsedModuleRef,
    check_file: bool,
) -> anyhow::Result<Vec<Event<'db>>> {
    let [function, class, alias] = fixture.definitions;
    let [
        DefinitionKind::Function(function_kind),
        DefinitionKind::Class(class_kind),
        DefinitionKind::AnnotatedAssignment(assignment),
    ] = fixture.kinds
    else {
        anyhow::bail!("unexpected fixture definitions");
    };
    let function_range = function_kind.node(module).range();
    let class_range = class_kind.node(module).range();
    let function_type = Type::FunctionLiteral(fixture.function);
    let mut events = vec![
        Event::Node(fixture.scope),
        Event::Body("module", None, None),
        Event::Take,
        Event::Deferred(0, function),
        Event::Kind(function),
        Event::DeferredAnnotations(function_range),
        Event::DeferredTypes(function),
        Event::Merge(function),
        Event::ParameterDefaults(function_range),
        Event::FunctionDefaultTypes(function),
        Event::Merge(function),
        Event::Deferred(1, class),
        Event::Kind(class),
        Event::DeferredTypes(class),
        Event::Merge(class),
        Event::DeferredEmpty,
        Event::CheckFile,
    ];
    if check_file {
        events.extend([
            Event::Seen,
            Event::Declaration(0, function, function_type),
            Event::Kind(function),
            Event::FunctionDecorators(function, function_range),
            Event::FunctionDefinition(function),
            Event::Overloaded(function_type, function),
            Event::TypeGuard(function_type, function_range),
            Event::Declaration(1, class, fixture.class_type),
            Event::Kind(class),
            Event::ClassDecorators(class, class_range),
            Event::OriginalClass(class),
            Event::StaticClass(fixture.class_type, class_range),
            Event::Declaration(2, alias, Type::unknown()),
            Event::Kind(alias),
            Event::Annotation(assignment.annotation(module).range()),
            Event::Alias(alias),
            Event::Deferred(0, function),
            Event::Dynamic(function),
            Event::Deferred(1, class),
            Event::Dynamic(class),
            Event::Called(0, fixture.function),
            Event::FunctionId(fixture.function),
            Event::Overloaded(function_type, function),
            Event::Final,
        ]);
    }
    Ok(events)
}

/// Synchronous and asynchronous scope traversals request annotation inference and its merge before
/// defaults, and stop at each refused effect. Supplied effects record the order without merging types.
#[test]
fn shared_scope_phases_preserve_order_and_every_refusal_prefix() -> anyhow::Result<()> {
    let db = database()?;
    let fixture = Fixture::new(&db)?;
    let module = fixture.module();
    let env = ProgramEnvironment::from_file(fixture.scope.program_file(&db));
    for check_file in [false, true] {
        let expected = expected_events(&fixture, &module, check_file)?;
        for reject_at in (0..expected.len()).map(Some).chain([None]) {
            let mut synchronous = Effects::new(&fixture);
            synchronous.check_file = check_file;
            synchronous.reject_at = reject_at;
            let mut asynchronous = Effects::new(&fixture);
            asynchronous.check_file = check_file;
            asynchronous.reject_at = reject_at;
            let mut left = fixture.builder(&module, &env);
            let mut right = fixture.builder(&module, &env);
            fixture.seed(&mut left);
            fixture.seed(&mut right);
            left.context.defuse();
            right.context.defuse();
            let result = infer_scope_sync(
                &mut left,
                fixture.scope,
                TypeContext::default(),
                ScopeFacts,
                &synchronous,
            );
            let async_result = try_poll_immediate(infer_scope_with(
                &mut right,
                fixture.scope,
                TypeContext::default(),
                ScopeFacts,
                &asynchronous,
            ));
            assert_eq!(async_result, Poll::Ready(result));
            let end = reject_at.map_or(expected.len(), |position| position + 1);
            assert_eq!(*synchronous.events.borrow(), expected[..end]);
            assert_eq!(*asynchronous.events.borrow(), expected[..end]);
            assert_eq!(left.deferred.0, right.deferred.0);
        }
    }
    // Exhausted cursors retain even an intentionally out-of-range position and do no work.
    let mut builder = fixture.builder(&module, &env);
    builder.context.defuse();
    let effects = Effects::new(&fixture);
    let mut cursor = usize::MAX;
    assert_eq!(
        SynchronousScopeEffects::next_deferred(&effects, &[], &mut cursor),
        Ok(None)
    );
    assert_eq!(
        SynchronousScopeEffects::next_declaration(&effects, &builder, &mut cursor),
        Ok(None)
    );
    assert_eq!(
        SynchronousScopeEffects::next_called_function(&effects, &builder, &mut cursor),
        Ok(None)
    );
    assert_eq!(cursor, usize::MAX);
    assert!(effects.events.borrow().is_empty());
    Ok(())
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

fn payload<'db>(
    fixture: &Fixture<'db>,
    module: &ParsedModuleRef,
    env: &ProgramEnvironment<'db>,
    extra: bool,
) -> anyhow::Result<(ScopeFinishData<'db>, [ExpressionNodeKey; 2])> {
    let DefinitionKind::AnnotatedAssignment(assignment) = fixture.kinds[2] else {
        anyhow::bail!("missing annotated assignment");
    };
    let value = assignment
        .value(module)
        .ok_or_else(|| anyhow::anyhow!("missing alias value"))?;
    let keys = [
        ExpressionNodeKey::from(assignment.annotation(module)),
        ExpressionNodeKey::from(value),
    ];
    let mut builder = fixture.builder(module, env);
    builder.expressions.insert(keys[0], Type::int_literal(11));
    builder.expressions.insert(keys[1], Type::int_literal(11));
    if extra {
        builder
            .implicit_aliases
            .extend([fixture.definitions[2], fixture.definitions[0]]);
        builder.string_annotations.insert(keys[1]);
        builder.qualifiers.insert(keys[0], TypeQualifiers::FINAL);
        builder
            .expected_types
            .insert(keys[1], Type::int_literal(22));
        builder
            .type_expression_flags
            .insert(keys[0], TypeExpressionFlags::UNPACK);
        builder.collection_use_constraints.insert(
            fixture.definitions[1],
            [Type::int_literal(33)].into_iter().collect(),
        );
        builder.cycle_recovery = Some(Type::int_literal(44));
        if let Some(diagnostic) = builder.context.report_lint(&INVALID_ASSIGNMENT, value) {
            diagnostic.into_diagnostic("Retained scope diagnostic");
        }
        assert!(builder.context.has_diagnostics());
    }
    Ok((
        infallible(OrdinaryScopeEffects.finish_context(builder)),
        keys,
    ))
}

#[test]
fn ordinary_scope_finalizer_preserves_every_payload_field() -> anyhow::Result<()> {
    let db = database()?;
    let fixture = Fixture::new(&db)?;
    let module = fixture.module();
    let env = ProgramEnvironment::from_file(fixture.scope.program_file(&db));
    for extra in [false, true] {
        let (data, keys) = payload(&fixture, &module, &env, extra)?;
        let diagnostics = format!("{:?}", data.diagnostics);
        assert_eq!(FinishScopeFacts.has_extra(&data), extra);
        let result = if extra {
            infallible(OrdinaryScopeEffects.freeze_with_extra(data))
        } else {
            infallible(OrdinaryScopeEffects.freeze_without_extra(data))
        };
        assert_eq!(
            result.expressions.get(&keys[0]),
            Some(&Type::int_literal(11))
        );
        assert_eq!(
            result.expressions.get(&keys[1]),
            Some(&Type::int_literal(11))
        );
        assert_eq!(result.expressions.iter().count(), 2);
        assert!(result.expressions.iter().map(|(key, _)| key).is_sorted());
        if let Some(data) = result.extra {
            assert_eq!(
                &*data.implicit_aliases,
                &[fixture.definitions[2], fixture.definitions[0]]
            );
            assert!(data.string_annotations.contains(&keys[1]));
            assert_eq!(data.string_annotations.iter().count(), 1);
            assert_eq!(data.qualifiers.get(&keys[0]), Some(&TypeQualifiers::FINAL));
            assert_eq!(
                data.expected_types.get(&keys[1]),
                Some(&Type::int_literal(22))
            );
            assert_eq!(
                data.type_expression_flags.get(&keys[0]),
                Some(&TypeExpressionFlags::UNPACK)
            );
            assert_eq!(
                data.collection_use_constraints[&fixture.definitions[1]]
                    .iter()
                    .copied()
                    .collect::<Vec<_>>(),
                [Type::int_literal(33)]
            );
            assert_eq!(data.cycle_recovery, Some(Type::int_literal(44)));
            assert!(!data.diagnostics.is_empty());
            assert_eq!(format!("{:?}", data.diagnostics), diagnostics);
        } else {
            assert!(!extra);
        }
    }
    let result = infer_scope_types(&db, fixture.scope, TypeContext::default());
    assert!(result.expressions.iter().count() > 2);
    let extra = result
        .extra
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing inferred alias metadata"))?;
    assert!(extra.implicit_aliases.contains(&fixture.definitions[2]));
    assert!(extra.diagnostics.is_empty());
    Ok(())
}

struct Observed<F> {
    future: Option<Pin<Box<F>>>,
    journal: Rc<Journal>,
}
impl<F: Future> Future for Observed<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let Some(future) = this.future.as_mut() else {
            return Poll::Pending;
        };
        let result = future.as_mut().poll(context);
        if result.is_pending() {
            this.journal.pending.set(this.journal.pending.get() + 1);
        }
        result
    }
}
impl<F> Drop for Observed<F> {
    fn drop(&mut self) {
        assert_eq!(self.journal.children.get(), 0);
        drop(self.future.take());
        self.journal.drops.borrow_mut().push("body");
    }
}
struct ModuleOwner {
    module: ParsedModuleRef,
    journal: Rc<Journal>,
}
impl Drop for ModuleOwner {
    fn drop(&mut self) {
        assert_eq!(self.journal.children.get(), 0);
        assert!(!self.module.suite().is_empty());
        assert!(self.journal.owner_live.replace(false));
        self.journal.drops.borrow_mut().push("module");
    }
}
struct Admit;
impl ExecutionAdmission for Admit {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}

#[test]
fn scope_transaction_retains_builder_module_and_deferred_across_children() -> anyhow::Result<()> {
    let db = database()?;
    let fixture = Fixture::new(&db)?;
    let expected = expected_events(&fixture, &fixture.module(), true)?;
    let refusal = expected
        .iter()
        .position(|event| matches!(event, Event::FunctionDecorators(..)))
        .ok_or_else(|| anyhow::anyhow!("missing declaration check"))?;
    for reject_at in [Some(refusal), None] {
        let journal = Rc::new(Journal::default());
        let observed_events = Rc::new(RefCell::new(Vec::new()));
        let root_journal = journal.clone();
        let root_events = observed_events.clone();
        let module = fixture.module();
        let fixture = &fixture;
        let outcome = try_with_attempt(&db, 100_000, || {
            RegistryBuilder::new(&db, &Admit)?
                .seal()?
                .run(move |endpoint| async move {
                    let owner = ModuleOwner {
                        module,
                        journal: root_journal.clone(),
                    };
                    root_journal.owner_live.set(true);
                    let env = ProgramEnvironment::from_file(fixture.scope.program_file(fixture.db));
                    let mut builder = fixture.builder(&owner.module, &env);
                    fixture.seed(&mut builder);
                    builder.context.defuse();
                    let effects = Effects {
                        endpoint: Some(&endpoint),
                        journal: root_journal.clone(),
                        reject_at,
                        events: root_events,
                        ..Effects::new(fixture)
                    };
                    let result = Observed {
                        future: Some(Box::pin(finish_scope_with(
                            builder,
                            ScopeInferenceState::Uninferred,
                            FinishScopeFacts,
                            &effects,
                        ))),
                        journal: root_journal,
                    }
                    .await;
                    effects.journal.resumed.set(true);
                    assert_eq!(result, Err(Boundary::Finalization));
                    Ok(())
                })
        });
        assert_eq!(
            matches!(outcome, Ok(AttemptOutcome::Complete(Ok(())))),
            reject_at.is_none(),
            "{outcome:?}"
        );
        assert_eq!(journal.resumed.get(), reject_at.is_none());
        let mut trace = expected.clone();
        if let Some(index) = reject_at {
            trace.truncate(index + 1);
        } else {
            trace.push(Event::Finish);
        }
        assert_eq!(*observed_events.borrow(), trace);
        assert!(journal.pending.get() > 2);
        assert!(journal.deferred_reads.get() >= 2);
        let drops = journal.drops.borrow();
        assert_eq!(&drops[drops.len() - 2..], &["body", "module"]);
        assert!(
            drops
                .windows(2)
                .any(|pair| pair == ["child", "builder-borrow"])
        );
        assert!(!journal.owner_live.get());
    }
    // The supplied retry ends at finalization; the separate ordinary query still completes.
    assert!(
        infer_scope_types(&db, fixture.scope, TypeContext::default())
            .expressions
            .iter()
            .next()
            .is_some()
    );
    Ok(())
}

struct CursorBorrow<'a> {
    cursor: &'a usize,
    expected: usize,
    journal: Rc<Journal>,
}
impl Drop for CursorBorrow<'_> {
    fn drop(&mut self) {
        assert_eq!(self.journal.children.get(), 0);
        assert_eq!(*self.cursor, self.expected);
    }
}
struct DataBorrow<'a, 'db> {
    data: &'a ScopeFinishData<'db>,
    journal: Rc<Journal>,
}
impl Drop for DataBorrow<'_, '_> {
    fn drop(&mut self) {
        assert_eq!(self.journal.children.get(), 0);
        assert_eq!(self.data.expressions.len(), 2);
        assert!(
            self.data
                .expressions
                .values()
                .all(|ty| *ty == Type::int_literal(11))
        );
        self.journal.drops.borrow_mut().push("data-borrow");
    }
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    Refuse,
    QueueChild,
}
struct Admission<'run, 'db: 'run> {
    endpoint: RefCell<Option<TaskEndpoint<'run, 'db>>>,
    journal: Rc<Journal>,
    active: Cell<bool>,
    calls: Cell<usize>,
    at: usize,
    fault: Fault,
}
impl ExecutionAdmission for Admission<'_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if !self.active.replace(false) || !matches!(work, ExecutionWork::Work { .. }) {
            return Ok(());
        }
        let position = self.calls.get();
        self.calls.set(position + 1);
        if position != self.at {
            return Ok(());
        }
        match self.fault {
            Fault::Refuse => Err(RunError::Refused(Incomplete::Allowance)),
            Fault::QueueChild => {
                let endpoint = self.endpoint.borrow();
                let endpoint = endpoint
                    .as_ref()
                    .ok_or(RunError::Contract("missing observer endpoint"))?;
                self.journal.children.set(self.journal.children.get() + 1);
                let child = Child(self.journal.clone());
                let _demand = endpoint.demand(move || async move {
                    let _child = child;
                    Ok(())
                })?;
                Ok(())
            }
        }
    }
}
struct ClearEndpoint(&'static Admission<'static, 'static>);
impl Drop for ClearEndpoint {
    fn drop(&mut self) {
        self.0.active.set(false);
        self.0.endpoint.borrow_mut().take();
    }
}

#[test]
fn every_local_admission_preserves_storage_before_refusal_or_queued_child() -> anyhow::Result<()> {
    // Static observer storage follows the existing runtime injection pattern. ClearEndpoint
    // breaks its retained endpoint on every exit. This test measures caller-owned retirement,
    // not reclamation of fixture databases or observer storage.
    let db: &'static TestDb = Box::leak(Box::new(database()?));
    let prepared = Fixture::new(db)?;
    let mut expected = expected_events(&prepared, &prepared.module(), true)?;
    expected.push(Event::Finish);
    let positions = expected
        .iter()
        .enumerate()
        .filter_map(|(index, event)| event.charged().then_some(index))
        .collect::<Vec<_>>();
    for fault in [Fault::Refuse, Fault::QueueChild] {
        for (at, &trace_end) in positions.iter().enumerate() {
            let fixture = Fixture::new(db)?;
            let module = fixture.module();
            let journal = Rc::new(Journal::default());
            let admission: &'static Admission<'static, 'static> = Box::leak(Box::new(Admission {
                endpoint: RefCell::new(None),
                journal: journal.clone(),
                active: Cell::new(false),
                calls: Cell::new(0),
                at,
                fault,
            }));
            let clear = ClearEndpoint(admission);
            let root_journal = journal.clone();
            let events = Rc::new(RefCell::new(Vec::new()));
            let root_events = events.clone();
            let outcome = try_with_attempt(db, 100_000, || {
                RegistryBuilder::new(db, admission)?
                    .seal()?
                    .run(move |endpoint| async move {
                        *admission.endpoint.borrow_mut() = Some(endpoint.clone());
                        let owner = ModuleOwner {
                            module,
                            journal: root_journal.clone(),
                        };
                        root_journal.owner_live.set(true);
                        let env = ProgramEnvironment::from_file(fixture.scope.program_file(db));
                        let mut builder = fixture.builder(&owner.module, &env);
                        fixture.seed(&mut builder);
                        builder.context.defuse();
                        let effects = Effects {
                            endpoint: Some(&endpoint),
                            admission: Some(admission),
                            events: root_events,
                            journal: root_journal.clone(),
                            ..Effects::new(&fixture)
                        };
                        let _result = Observed {
                            future: Some(Box::pin(finish_scope_with(
                                builder,
                                ScopeInferenceState::Uninferred,
                                FinishScopeFacts,
                                &effects,
                            ))),
                            journal: root_journal,
                        }
                        .await;
                        effects.journal.resumed.set(true);
                        Err::<(), _>(RunError::Contract("refused transaction resumed"))
                    })
            });
            assert!(
                !matches!(outcome, Ok(AttemptOutcome::Complete(Ok(())))),
                "{outcome:?}"
            );
            assert_eq!(admission.calls.get(), at + 1);
            assert!(!journal.resumed.get());
            assert_eq!(*events.borrow(), expected[..=trace_end]);
            assert_eq!(journal.commits.get(), at);
            assert!(journal.pending.get() > 0);
            let drops = journal.drops.borrow();
            assert_eq!(&drops[drops.len() - 2..], &["body", "module"]);
            if matches!(fault, Fault::QueueChild) {
                assert!(drops.iter().any(|drop| *drop == "child"));
            }
            drop(clear);
            assert!(admission.endpoint.borrow().is_none());
        }
    }
    Ok(())
}

#[test]
fn finalizer_data_stays_owned_until_a_queued_child_retires() -> anyhow::Result<()> {
    let db: &'static TestDb = Box::leak(Box::new(database()?));
    for extra in [false, true] {
        let fixture = Fixture::new(db)?;
        let module = fixture.module();
        let env = ProgramEnvironment::from_file(fixture.scope.program_file(db));
        let (data, _) = payload(&fixture, &module, &env, extra)?;
        let journal = Rc::new(Journal::default());
        let admission: &'static Admission<'static, 'static> = Box::leak(Box::new(Admission {
            endpoint: RefCell::new(None),
            journal: journal.clone(),
            active: Cell::new(false),
            calls: Cell::new(0),
            at: 0,
            fault: Fault::QueueChild,
        }));
        let clear = ClearEndpoint(admission);
        let root_journal = journal.clone();
        let outcome = try_with_attempt(db, 100_000, || {
            RegistryBuilder::new(db, admission)?
                .seal()?
                .run(move |endpoint| async move {
                    *admission.endpoint.borrow_mut() = Some(endpoint.clone());
                    let owner = ModuleOwner {
                        module,
                        journal: root_journal.clone(),
                    };
                    root_journal.owner_live.set(true);
                    let effects = Effects {
                        endpoint: Some(&endpoint),
                        admission: Some(admission),
                        journal: root_journal.clone(),
                        ..Effects::new(&fixture)
                    };
                    let body = async {
                        if extra {
                            effects.freeze_with_extra(data).await
                        } else {
                            effects.freeze_without_extra(data).await
                        }
                    };
                    let _result = Observed {
                        future: Some(Box::pin(body)),
                        journal: root_journal,
                    }
                    .await;
                    effects.journal.resumed.set(true);
                    drop(owner);
                    Err::<(), _>(RunError::Contract("refused finalizer resumed"))
                })
        });
        assert!(
            !matches!(outcome, Ok(AttemptOutcome::Complete(Ok(())))),
            "{outcome:?}"
        );
        assert_eq!(journal.pending.get(), 1);
        assert!(!journal.resumed.get());
        assert_eq!(
            *journal.drops.borrow(),
            ["child", "data-borrow", "body", "module"]
        );
        drop(clear);
        assert!(admission.endpoint.borrow().is_none());
    }
    Ok(())
}
