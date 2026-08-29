//! Abstract-method discovery and diagnostics shared by class validation and constructor calls.

use ruff_db::{
    diagnostic::{Annotation, Span, SubDiagnostic, SubDiagnosticSeverity},
    parsed::parsed_module,
};
use ruff_python_ast::name::Name;
use ty_python_core::{place_table, use_def_map};

use crate::{
    Db, FxIndexSet, ProgramEnvironment, TypeQualifiers,
    diagnostic::format_enumeration,
    place::{DefinedPlace, Place, place_from_bindings, place_from_declarations},
    types::{
        ClassBase, ClassLiteral, ClassType, LintDiagnosticGuard, Parameters, Signature, Type,
        binding_type,
        diagnostic::{AbstractMethodAnnotationPolicy, abstract_method_span},
        function::AbstractMethodKind,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
pub(super) struct AbstractMethods<'db> {
    class: ClassType<'db>,
    is_empty: bool,
}

impl<'db> AbstractMethods<'db> {
    /// Find methods that remain abstract after applying overrides in MRO order.
    ///
    /// Cache only whether the set is empty for the common case of a concrete constructor call.
    /// Retain names and recover diagnostic locations only when a caller needs them.
    pub(super) fn of_class(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        // Inferring class members can call constructors that query abstractness again.
        // Start with no abstract methods while resolving these cycles.
        #[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size, cycle_initial=|_, _, _| true)]
        fn abstract_methods_is_empty<'db>(db: &'db dyn Db, class: ClassType<'db>) -> bool {
            abstract_methods_of_class(db, class).is_empty()
        }

        Self {
            class,
            is_empty: abstract_methods_is_empty(db, class),
        }
    }

    fn cached_methods(&self, db: &'db dyn Db) -> &'db FxIndexSet<Name> {
        #[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
        fn cached_abstract_methods<'db>(
            db: &'db dyn Db,
            class: ClassType<'db>,
        ) -> FxIndexSet<Name> {
            abstract_methods_of_class(db, class)
        }

        cached_abstract_methods(db, self.class)
    }

    /// Annotate a diagnostic with the unimplemented methods and their declarations.
    pub(super) fn annotate_diagnostic(&self, db: &dyn Db, diagnostic: &mut LintDiagnosticGuard) {
        let Some(first_name) = self.first_name(db) else {
            return;
        };
        let env = &ProgramEnvironment::from_file(self.class.class_literal(db).program_file(db));

        let mut annotation_override = None;

        let Some((definition, kind, defining_class)) = self
            .class
            .iter_mro(db)
            .filter_map(ClassBase::into_class)
            .find_map(|superclass| {
                let literal = superclass.class_literal(db).as_static()?;
                let scope = literal.body_scope(db);
                let symbol_id = place_table(db, scope).symbol_id(first_name)?;
                let use_def_map = use_def_map(db, literal.body_scope(db));
                let bindings = use_def_map.end_of_scope_symbol_bindings(symbol_id);
                let place_and_def = place_from_bindings(db, env, bindings);

                let Some(ty) = place_and_def.place.ignore_possibly_undefined() else {
                    let declarations_iterator =
                        use_def_map.end_of_scope_symbol_declarations(symbol_id);
                    let declarations = place_from_declarations(db, env, declarations_iterator);
                    let first_declaration = declarations.first_declaration?;
                    if !declarations
                        .ignore_conflicting_declarations()
                        .qualifiers
                        .contains(TypeQualifiers::CLASS_VAR)
                    {
                        annotation_override = Some((superclass, first_declaration));
                    }
                    return None;
                };

                let definition = place_and_def.first_definition?;
                let kind = type_as_abstract_method(db, ty, superclass)?;
                Some((definition, kind, superclass))
            })
        else {
            return;
        };

        let span = if let Type::FunctionLiteral(function) = binding_type(db, definition) {
            let policy = if kind.is_explicit() {
                AbstractMethodAnnotationPolicy::ExcludeVerboseBody
            } else {
                AbstractMethodAnnotationPolicy::AlwaysIncludeBody
            };
            abstract_method_span(db, function, policy)
        } else {
            let module = parsed_module(db, definition.python_file(db)).load(db);
            Span::from(definition.focus_range(db, &module))
        };
        let secondary_annotation = Annotation::secondary(span);
        diagnostic.annotate(if defining_class == self.class {
            secondary_annotation.message(format_args!("`{first_name}` declared as abstract"))
        } else {
            secondary_annotation.message(format_args!(
                "`{first_name}` declared as abstract on superclass `{}`",
                defining_class.name(db)
            ))
        });
        let num_abstract_methods = self.len(db);

        if num_abstract_methods == 1 {
            diagnostic
                .set_primary_annotation_message(format_args!("`{first_name}` is unimplemented"));
        } else {
            let formatted_methods = self.formatted_names(db);

            if formatted_methods.truncation_occurred {
                diagnostic.set_primary_annotation_message(format_args!(
                    "{num_abstract_methods} abstract methods are unimplemented, \
                        including {formatted_methods}",
                ));
            } else {
                diagnostic.set_primary_annotation_message(format_args!(
                    "Abstract methods {formatted_methods} are unimplemented"
                ));
            }

            if formatted_methods.truncation_occurred {
                diagnostic.info(format_args!(
                    "Use `--verbose` to see all {num_abstract_methods} \
                    unimplemented abstract methods",
                ));
            }
        }

        // If this method was implicitly abstract (due to being a method with an
        // empty body in a `Protocol` class), we attach additional annotations
        // that explain this feature of the type system.
        if !kind.is_explicit() {
            let defining_class_name = defining_class.name(db);
            let mut sub = SubDiagnostic::new(
                SubDiagnosticSeverity::Info,
                format_args!(
                    "`{defining_class_name}.{first_name}` is implicitly abstract \
                because `{defining_class_name}` is a `Protocol` class \
                and `{first_name}` lacks an implementation",
                ),
            );
            sub.annotate(
                Annotation::secondary(defining_class.definition_span(db))
                    .message(format_args!("`{defining_class_name}` declared here")),
            );
            diagnostic.sub(sub);

            // If an implicitly abstract method in checked code can return `None`,
            // suggest a concrete no-op body.
            if kind.is_implicit_due_to_stub_body()
                && db.should_check_file(definition.file(db))
                && let Some(callables) =
                    binding_type(db, definition).try_upcast_to_callable(db, env)
                && Type::function_like_callable(
                    db,
                    Signature::new(Parameters::gradual_form(), Type::none(db, env)),
                )
                .is_assignable_to(db, env, callables.into_type(db, env))
            {
                diagnostic.help(format_args!(
                    "Change the body of `{first_name}` to `return` \
                    or `return None` if it was not intended to be abstract",
                ));
            }
        }

        if let Some((overriding_class, declaration)) = annotation_override {
            if overriding_class == self.class {
                diagnostic.info(format_args!(
                    "The instance-attribute annotation for `{first_name}` \
                    does not override the abstract method",
                ));
            } else {
                diagnostic.info(format_args!(
                    "The instance-attribute annotation for `{first_name}` on superclass `{}` \
                    does not override the abstract method",
                    overriding_class.name(db)
                ));
            }

            let file = declaration.file(db);

            if db.should_check_file(file) {
                let mut sub = SubDiagnostic::new(
                    SubDiagnosticSeverity::Help,
                    "Either assign a value or add `ClassVar` to this declaration",
                );
                let declaration_module = parsed_module(db, declaration.python_file(db)).load(db);
                sub.annotate(
                    Annotation::secondary(Span::from(
                        declaration.focus_range(db, &declaration_module),
                    ))
                    .message("Instance-attribute declaration"),
                );
                diagnostic.sub(sub);
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
        let len = self.cached_methods(db).len();
        let max_abstract_methods_to_print = if db.verbose() {
            len
        } else {
            AbstractMethods::DEFAULT_METHOD_NUMBER_TO_PRINT
        };
        let truncation_occurred = max_abstract_methods_to_print < len;
        FormattedAbstractMethods {
            inner: format_enumeration(
                self.cached_methods(db)
                    .iter()
                    .take(max_abstract_methods_to_print),
            ),
            truncation_occurred,
        }
    }

    pub(super) fn first_name(&self, db: &'db dyn Db) -> Option<&Name> {
        if self.is_empty {
            None
        } else {
            self.cached_methods(db).first()
        }
    }

    pub(super) fn len(&self, db: &'db dyn Db) -> usize {
        if self.is_empty {
            0
        } else {
            self.cached_methods(db).len()
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.is_empty
    }
}

fn abstract_methods_of_class<'db>(db: &'db dyn Db, class: ClassType<'db>) -> FxIndexSet<Name> {
    let mut abstract_methods: FxIndexSet<Name> = FxIndexSet::default();
    let env = &ProgramEnvironment::from_file(class.class_literal(db).program_file(db));

    // Iterate through the MRO in reverse order,
    // skipping `object` (we know it doesn't define any abstract methods)
    for supercls in class.iter_mro(db).rev().skip(1) {
        let ClassBase::Class(class) = supercls else {
            continue;
        };

        // Currently we do not recognize dynamic classes as being able to define abstract methods,
        // but we do recognise them as being able to override abstract methods defined in static classes.
        let ClassLiteral::Static(class_literal) = class.class_literal(db) else {
            abstract_methods
                .retain(|name| class.own_class_member(db, env, None, name).is_undefined());
            continue;
        };

        let scope = class_literal.body_scope(db);
        let place_table = place_table(db, scope);
        let use_def_map = use_def_map(db, class_literal.body_scope(db));

        // Treat abstract methods from superclasses as having been overridden
        // if this class has a synthesized method by that name,
        // or this class has a `ClassVar` declaration by that name
        abstract_methods.retain(|name| {
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
            if type_as_abstract_method(db, ty, class).is_some() {
                abstract_methods.insert(name.clone());
            } else {
                // If this method is concrete, remove it from the set of abstract methods.
                abstract_methods.shift_remove(name);
            }
        }
    }

    abstract_methods.shrink_to_fit();
    abstract_methods
}

fn type_as_abstract_method<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    defining_class: ClassType<'db>,
) -> Option<AbstractMethodKind> {
    match ty {
        Type::FunctionLiteral(function) => function.as_abstract_method(db, defining_class),
        Type::BoundMethod(method) => method.function(db).as_abstract_method(db, defining_class),
        Type::PropertyInstance(property) => {
            // A property is abstract if any of its accessors is abstract.
            property
                .getter(db)
                .and_then(|getter| type_as_abstract_method(db, getter, defining_class))
                .or_else(|| {
                    property
                        .setter(db)
                        .and_then(|setter| type_as_abstract_method(db, setter, defining_class))
                })
                .or_else(|| {
                    property
                        .deleter(db)
                        .and_then(|deleter| type_as_abstract_method(db, deleter, defining_class))
                })
        }
        _ => None,
    }
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
