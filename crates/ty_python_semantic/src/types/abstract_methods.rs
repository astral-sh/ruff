//! Abstract-method discovery and diagnostics for class validation.

use ruff_db::diagnostic::{Annotation, SubDiagnostic, SubDiagnosticSeverity};
use ruff_python_ast::name::Name;
use ty_python_core::{definition::Definition, place_table, use_def_map};

use crate::{
    Db, FxIndexMap, ProgramEnvironment, TypeQualifiers,
    diagnostic::format_enumeration,
    place::{DefinedPlace, Place, place_from_bindings, place_from_declarations},
    types::{
        ClassBase, ClassLiteral, ClassType, LintDiagnosticGuard, Parameters, Signature, Type,
        binding_type,
        diagnostic::{AbstractMethodAnnotationPolicy, abstract_method_span},
        function::AbstractMethodKind,
        infer::infer_definition_types,
    },
};

#[derive(Debug, Clone, Copy)]
pub(super) struct AbstractMethods<'db> {
    class: ClassType<'db>,
    methods: &'db FxIndexMap<Name, AbstractMethod<'db>>,
}

impl<'db> AbstractMethods<'db> {
    /// Find methods that remain abstract after applying overrides in MRO order.
    pub(super) fn of_class(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        Self {
            class,
            methods: class.abstract_methods(db),
        }
    }

    /// Annotate a diagnostic with the unimplemented methods and their declarations.
    pub(super) fn annotate_diagnostic(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        diagnostic: &mut LintDiagnosticGuard,
    ) {
        let Some((first_method_name, abstract_method)) = self.methods.iter().next() else {
            return;
        };
        let num_abstract_methods = self.len();
        if num_abstract_methods == 1 {
            diagnostic.set_primary_annotation_message(format_args!(
                "`{first_method_name}` is unimplemented"
            ));
        } else {
            let formatted_methods = self.formatted_names(db);
            if formatted_methods.truncation_occurred {
                diagnostic.set_primary_annotation_message(format_args!(
                    "{num_abstract_methods} abstract methods are unimplemented, including {formatted_methods}",
                ));
                diagnostic.info(format_args!(
                    "Use `--verbose` to see all {num_abstract_methods} unimplemented abstract methods",
                ));
            } else {
                diagnostic.set_primary_annotation_message(format_args!(
                    "Abstract methods {formatted_methods} are unimplemented"
                ));
            }
        }

        let AbstractMethod {
            defining_class,
            definition,
            kind,
        } = abstract_method;

        let defining_class_name = defining_class.name(db);

        if let Type::FunctionLiteral(function) = binding_type(db, *definition) {
            let policy = if kind.is_explicit() {
                AbstractMethodAnnotationPolicy::ExcludeVerboseBody
            } else {
                AbstractMethodAnnotationPolicy::AlwaysIncludeBody
            };
            let secondary_span = abstract_method_span(db, function, policy);
            let mut secondary_annotation = Annotation::secondary(secondary_span);
            secondary_annotation = if defining_class.class_literal(db)
                == self.class.class_literal(db)
            {
                secondary_annotation
                    .message(format_args!("`{first_method_name}` declared as abstract"))
            } else {
                secondary_annotation.message(format_args!(
                    "`{first_method_name}` declared as abstract on superclass `{defining_class_name}`",
                ))
            };
            diagnostic.annotate(secondary_annotation);
        }

        if !kind.is_explicit() {
            let mut sub = SubDiagnostic::new(
                SubDiagnosticSeverity::Info,
                format_args!(
                    "`{defining_class_name}.{first_method_name}` is implicitly abstract \
                        because `{defining_class_name}` is a `Protocol` class \
                        and `{first_method_name}` lacks an implementation",
                ),
            );
            sub.annotate(
                Annotation::secondary(defining_class.definition_span(db))
                    .message(format_args!("`{defining_class_name}` declared here")),
            );
            diagnostic.sub(sub);

            // If the implicitly abstract method is defined in first-party code
            // and the return type is assignable to `None`, they may not have intended
            // for it to be implicitly abstract; add a clarificatory note:
            if kind.is_implicit_due_to_stub_body() && db.should_check_file(definition.file(db)) {
                let function_type_as_callable = infer_definition_types(db, *definition)
                    .binding_type(*definition)
                    .try_upcast_to_callable(db, env);

                if let Some(callables) = function_type_as_callable
                    && Type::function_like_callable(
                        db,
                        Signature::new(Parameters::gradual_form(), Type::none(db, env)),
                    )
                    .is_assignable_to(db, env, callables.into_type(db, env))
                {
                    diagnostic.help(format_args!(
                        "Change the body of `{first_method_name}` to `return` \
                            or `return None` if it was not intended to be abstract"
                    ));
                }
            }
        }
    }

    /// Unless `--verbose` was specified on the command line,
    /// we will only print this number of abstract methods in diagnostics
    /// complaining about abstract class instantiation (and similar)
    const DEFAULT_METHOD_NUMBER_TO_PRINT: usize = 3;

    /// Return a string that contains a formatted subset of the abstract methods
    /// in this collection.
    ///
    /// This is useful for diagnostics.
    pub(super) fn formatted_names(&self, db: &'db dyn Db) -> FormattedAbstractMethods {
        let len = self.methods.len();
        let max_abstract_methods_to_print = if db.verbose() {
            len
        } else {
            AbstractMethods::DEFAULT_METHOD_NUMBER_TO_PRINT
        };
        let truncation_occurred = max_abstract_methods_to_print < len;
        FormattedAbstractMethods {
            inner: format_enumeration(self.methods.keys().take(max_abstract_methods_to_print)),
            truncation_occurred,
        }
    }

    pub(super) fn first_name(&self) -> Option<&Name> {
        self.methods.keys().next()
    }

    pub(super) fn len(&self) -> usize {
        self.methods.len()
    }
}

#[salsa::tracked]
impl<'db> ClassType<'db> {
    /// Returns a map of methods on this class that were defined as abstract on a superclass
    /// and have not been overridden with a concrete implementation anywhere in the MRO
    ///
    /// The value of the map is a struct containing information about the abstract method.
    #[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
    pub(in crate::types) fn abstract_methods(
        self,
        db: &'db dyn Db,
    ) -> FxIndexMap<Name, AbstractMethod<'db>> {
        fn type_as_abstract_method<'db>(
            db: &'db dyn Db,
            ty: Type<'db>,
            defining_class: ClassType<'db>,
        ) -> Option<AbstractMethodKind> {
            match ty {
                Type::FunctionLiteral(function) => function.as_abstract_method(db, defining_class),
                Type::BoundMethod(method) => {
                    type_as_abstract_method(db, method.func(db), defining_class)
                }
                Type::PropertyInstance(property) => {
                    // A property is abstract if any of its accessors is abstract.
                    property
                        .getter(db)
                        .and_then(|getter| type_as_abstract_method(db, getter, defining_class))
                        .or_else(|| {
                            property.setter(db).and_then(|setter| {
                                type_as_abstract_method(db, setter, defining_class)
                            })
                        })
                        .or_else(|| {
                            property.deleter(db).and_then(|deleter| {
                                type_as_abstract_method(db, deleter, defining_class)
                            })
                        })
                }
                _ => None,
            }
        }

        let mut abstract_methods: FxIndexMap<Name, _> = FxIndexMap::default();
        let env = &ProgramEnvironment::from_file(self.class_literal(db).program_file(db));

        // Iterate through the MRO in reverse order,
        // skipping `object` (we know it doesn't define any abstract methods)
        for supercls in self.iter_mro(db).rev().skip(1) {
            let ClassBase::Class(class) = supercls else {
                continue;
            };

            // Currently we do not recognize dynamic classes as being able to define abstract methods,
            // but we do recognise them as being able to override abstract methods defined in static classes.
            let ClassLiteral::Static(class_literal) = class.class_literal(db) else {
                abstract_methods
                    .retain(|name, _| class.own_class_member(db, env, None, name).is_undefined());
                continue;
            };

            let scope = class_literal.body_scope(db);
            let place_table = place_table(db, scope);
            let use_def_map = use_def_map(db, class_literal.body_scope(db));

            // Treat abstract methods from superclasses as having been overridden
            // if this class has a synthesized method by that name,
            // or this class has a `ClassVar` declaration by that name
            abstract_methods.retain(|name, _| {
                if class_literal
                    .own_synthesized_member(db, env, None, None, name)
                    .is_some()
                {
                    return false;
                }

                place_table.symbol_id(name).is_none_or(|symbol_id| {
                    let declarations = use_def_map.end_of_scope_symbol_declarations(symbol_id);
                    !place_from_declarations(db, env, declarations)
                        .ignore_conflicting_declarations()
                        .qualifiers
                        .contains(TypeQualifiers::CLASS_VAR)
                })
            });

            for (symbol_id, bindings_iterator) in use_def_map.all_end_of_scope_symbol_bindings() {
                let name = place_table.symbol(symbol_id).name();
                let place_and_definition = place_from_bindings(db, env, bindings_iterator);
                let Place::Defined(DefinedPlace { ty, .. }) = place_and_definition.place else {
                    continue;
                };
                let Some(definition) = place_and_definition.first_definition else {
                    continue;
                };
                if let Some(kind) = type_as_abstract_method(db, ty, class) {
                    let abstract_method = AbstractMethod {
                        defining_class: class,
                        definition,
                        kind,
                    };
                    abstract_methods.insert(name.clone(), abstract_method);
                } else {
                    // If this method is concrete, remove it from the map of abstract methods.
                    abstract_methods.shift_remove(name);
                }
            }
        }

        abstract_methods.shrink_to_fit();

        abstract_methods
    }
}

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct AbstractMethod<'db> {
    pub(super) defining_class: ClassType<'db>,
    pub(super) definition: Definition<'db>,
    pub(super) kind: AbstractMethodKind,
}

#[derive(Debug)]
pub(super) struct FormattedAbstractMethods {
    inner: String,

    /// Boolean flag that indicates whether the wrapped string is an exhaustive
    /// enumeration of *all* abstract methods on a class, or only an enumeration
    /// of a truncated subset
    pub(super) truncation_occurred: bool,
}

impl std::fmt::Display for FormattedAbstractMethods {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}
