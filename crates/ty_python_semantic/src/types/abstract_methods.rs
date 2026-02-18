use ruff_db::{
    diagnostic::{Annotation, Span, SubDiagnostic, SubDiagnosticSeverity},
    parsed::parsed_module,
};
use ruff_python_ast::name::Name;
use ty_module_resolver::{SearchPath, file_to_module};

use crate::{
    Db, FxIndexSet, TypeQualifiers,
    diagnostic::format_enumeration,
    place::{DefinedPlace, Place, place_from_bindings, place_from_declarations},
    semantic_index::{place::ScopedPlaceId, place_table, use_def_map},
    types::{
        ClassBase, ClassLiteral, ClassType, LintDiagnosticGuard, Parameters, Signature, Type,
        binding_type,
        function::{FunctionBodyKind, FunctionDecorators, FunctionType},
    },
};

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub(super) struct AbstractMethods<'db> {
    methods: &'db AbstractMethodsInner,
    class: ClassType<'db>,
}

impl<'db> AbstractMethods<'db> {
    /// Returns a set of methods on this class that were defined as abstract on a superclass
    /// and have not been overridden with a concrete implementation anywhere in the MRO
    pub(super) fn of_class(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        #[salsa::tracked(
            returns(ref),
            heap_size=ruff_memory_usage::heap_size,
            cycle_initial=|_, _, _| AbstractMethodsInner::default()
        )]
        fn of_class_inner<'db>(db: &'db dyn Db, class: ClassType<'db>) -> AbstractMethodsInner {
            let mut abstract_methods: FxIndexSet<&Name> = FxIndexSet::default();

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
                        .retain(|name| class.own_class_member(db, None, name).is_undefined());
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
                        .own_synthesized_member(db, None, None, name)
                        .is_some()
                    {
                        return false;
                    }

                    place_table.symbol_id(name).is_none_or(|symbol_id| {
                        let declarations = use_def_map.end_of_scope_symbol_declarations(symbol_id);
                        !place_from_declarations(db, declarations)
                            .ignore_conflicting_declarations()
                            .qualifiers
                            .contains(TypeQualifiers::CLASS_VAR)
                    })
                });

                for (symbol_id, bindings_iterator) in use_def_map.all_end_of_scope_symbol_bindings()
                {
                    let name = place_table.symbol(symbol_id).name();
                    let place_and_definition = place_from_bindings(db, bindings_iterator);
                    let Place::Defined(DefinedPlace { ty, .. }) = place_and_definition.place else {
                        continue;
                    };
                    if type_as_abstract_method(db, ty, class).is_some() {
                        abstract_methods.insert(name);
                    } else {
                        // If this method is concrete, remove it from the map of abstract methods.
                        abstract_methods.shift_remove(name);
                    }
                }
            }

            let total_abstract_methods = abstract_methods.len();

            match total_abstract_methods {
                0 => AbstractMethodsInner::Empty,
                1 => AbstractMethodsInner::One(abstract_methods[0].clone()),
                2 => AbstractMethodsInner::Two([
                    abstract_methods[0].clone(),
                    abstract_methods[1].clone(),
                ]),
                3 => AbstractMethodsInner::Three([
                    abstract_methods[0].clone(),
                    abstract_methods[1].clone(),
                    abstract_methods[2].clone(),
                ]),
                _ if db.verbose() => {
                    AbstractMethodsInner::Full(abstract_methods.into_iter().cloned().collect())
                }
                _ => AbstractMethodsInner::Truncated {
                    names: [
                        abstract_methods[0].clone(),
                        abstract_methods[1].clone(),
                        abstract_methods[2].clone(),
                    ],
                    full_length: total_abstract_methods,
                },
            }
        }

        let methods = of_class_inner(db, class);
        Self { methods, class }
    }

    /// Attach primary and secondary annotations to a passed in diagnostic that describe
    /// this set of abstract methods
    pub(super) fn annotate_diagnostic(
        &self,
        db: &'db dyn Db,
        diagnostic: &mut LintDiagnosticGuard,
    ) {
        let first_name =
            self.methods.iter().next().expect(
                "`annotate_diagnostic()` should not be called on an empty `AbstractMethods`",
            );

        let mut annotation_override = None;

        let (definition, kind, defining_class) = self
            .class
            .iter_mro(db)
            .filter_map(ClassBase::into_class)
            .find_map(|superclass| {
                let literal = superclass.class_literal(db).as_static()?;
                let scope = literal.body_scope(db);
                let symbol_id = place_table(db, scope).symbol_id(first_name)?;
                let use_def_map = use_def_map(db, literal.body_scope(db));
                let bindings = use_def_map.end_of_scope_bindings(ScopedPlaceId::Symbol(symbol_id));
                let place_and_def = place_from_bindings(db, bindings);

                let Some(ty) = place_and_def.place.ignore_possibly_undefined() else {
                    let declarations_iterator =
                        use_def_map.end_of_scope_symbol_declarations(symbol_id);
                    let declarations = place_from_declarations(db, declarations_iterator);
                    let first_declaration = declarations.first_declaration?;
                    debug_assert!(
                        !declarations
                            .ignore_conflicting_declarations()
                            .is_class_var()
                    );
                    annotation_override = Some((superclass, first_declaration));
                    return None;
                };

                let definition = place_and_def.first_definition?;
                let kind = type_as_abstract_method(db, ty, superclass)?;
                Some((definition, kind, superclass))
            })
            .expect(
                "Every name included in an `AbstractMethods` collection \
                should be defined on the associated class",
            );

        let module = parsed_module(db, definition.file(db)).load(db);
        let span = Span::from(definition.focus_range(db, &module));
        let secondary_annotation = Annotation::secondary(span);

        if self.len() == 1 {
            diagnostic.set_primary_message(format_args!(
                "Abstract method `{first_name}` is unimplemented"
            ));
            if defining_class == self.class {
                diagnostic.annotate(
                    secondary_annotation
                        .message(format_args!("`{first_name}` declared as abstract")),
                );
            } else {
                diagnostic.annotate(secondary_annotation.message(format_args!(
                    "`{first_name}` declared as abstract on superclass `{}`",
                    defining_class.name(db)
                )));
            }
        } else {
            let num_abstract_methods = self.len();
            let formatted_methods = self.formatted_names();

            if formatted_methods.truncation_occurred {
                diagnostic.set_primary_message(format_args!(
                    "{num_abstract_methods} abstract methods are unimplemented, \
                        including {formatted_methods}",
                ));
            } else {
                diagnostic.set_primary_message(format_args!(
                    "Abstract methods {formatted_methods} are unimplemented"
                ));
            }

            if defining_class == self.class {
                diagnostic.annotate(
                    secondary_annotation
                        .message(format_args!("`{first_name}` declared as abstract")),
                );
            } else {
                diagnostic.annotate(secondary_annotation.message(format_args!(
                    "`{first_name}` declared as abstract on superclass `{}`",
                    defining_class.name(db)
                )));
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

            // If the implicitly abstract method is defined in first-party code
            // and the return type is assignable to `None`, they may not have intended
            // for it to be implicitly abstract; add a clarificatory note:
            if kind.is_implicit_due_to_stub_body()
                && file_to_module(db, definition.file(db))
                    .and_then(|module| module.search_path(db))
                    .is_some_and(SearchPath::is_first_party)
            {
                let function_type_as_callable =
                    binding_type(db, definition).try_upcast_to_callable(db);

                if let Some(callables) = function_type_as_callable
                    && Type::function_like_callable(
                        db,
                        Signature::new(Parameters::gradual_form(), Type::none(db)),
                    )
                    .is_assignable_to(db, callables.into_type(db))
                {
                    diagnostic.help(format_args!(
                        "Change the body of `{first_name}` to `return` \
                        or `return None` if it was not intended to be abstract",
                    ));
                }
            }
        }

        if let Some((superclass, declaration)) = annotation_override {
            if superclass == self.class {
                diagnostic.info(format_args!(
                    "`{first_name}` is overridden \
                        with an instance-attribute annotation,",
                ));
            } else {
                diagnostic.info(format_args!(
                    "`{first_name}` is overridden on superclass `{}` \
                        with an instance-attribute annotation,",
                    superclass.name(db)
                ));
            }
            diagnostic.info(format_args!(
                "but this is insufficient to make `{}` non-abstract",
                self.class.name(db)
            ));

            let file = declaration.file(db);

            if file_to_module(db, file)
                .and_then(|module| module.search_path(db))
                .is_some_and(SearchPath::is_first_party)
            {
                let mut sub = SubDiagnostic::new(
                    SubDiagnosticSeverity::Help,
                    "Either assign a value or add `ClassVar` to this declaration",
                );
                let declaration_module = parsed_module(db, file).load(db);
                sub.annotate(
                    Annotation::secondary(Span::from(
                        declaration.focus_range(db, &declaration_module),
                    ))
                    .message("Instance-attribute declaration on superclass"),
                );
                diagnostic.sub(sub);
            }
        }
    }

    /// Return a string that contains a formatted subset of the abstract methods
    /// in this map.
    ///
    /// This is useful for diagnostics.
    pub(super) fn formatted_names(&self) -> FormattedAbstractMethods {
        FormattedAbstractMethods {
            inner: format_enumeration(self.methods),
            truncation_occurred: self.methods.is_truncated(),
        }
    }

    pub(super) fn first_name(&self) -> Option<&Name> {
        self.methods.iter().next()
    }

    pub(super) fn len(&self) -> usize {
        self.methods.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.methods.len() == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, salsa::Update, get_size2::GetSize)]
enum AbstractMethodsInner {
    #[default]
    Empty,
    One(Name),
    Two([Name; 2]),
    Three([Name; 3]),
    Truncated {
        names: [Name; 3],
        full_length: usize,
    },
    Full(Box<[Name]>),
}

impl AbstractMethodsInner {
    const fn is_truncated(&self) -> bool {
        matches!(self, Self::Truncated { .. })
    }

    fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::One(_) => 1,
            Self::Two(..) => 2,
            Self::Three(..) => 3,
            Self::Truncated { full_length, .. } => *full_length,
            Self::Full(names) => names.len(),
        }
    }

    fn iter(&self) -> std::slice::Iter<'_, Name> {
        match self {
            AbstractMethodsInner::Empty => [].iter(),
            AbstractMethodsInner::One(single) => std::slice::from_ref(single).iter(),
            AbstractMethodsInner::Two(names) => names.iter(),
            AbstractMethodsInner::Three(names) => names.iter(),
            AbstractMethodsInner::Truncated { names, .. } => names.iter(),
            AbstractMethodsInner::Full(names) => names.iter(),
        }
    }
}

impl<'a> IntoIterator for &'a AbstractMethodsInner {
    type IntoIter = std::slice::Iter<'a, Name>;
    type Item = &'a Name;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

fn type_as_abstract_method<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    defining_class: ClassType<'db>,
) -> Option<AbstractMethodKind> {
    match ty {
        Type::FunctionLiteral(function) => {
            AbstractMethodKind::of_function(db, function, defining_class)
        }
        Type::BoundMethod(method) => {
            AbstractMethodKind::of_function(db, method.function(db), defining_class)
        }
        Type::PropertyInstance(property) => {
            // A property is abstract if either its getter or setter is abstract.
            property
                .getter(db)
                .and_then(|getter| type_as_abstract_method(db, getter, defining_class))
                .or_else(|| {
                    property
                        .setter(db)
                        .and_then(|setter| type_as_abstract_method(db, setter, defining_class))
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

/// Indicates whether a method is explicitly or implicitly abstract.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(super) enum AbstractMethodKind {
    /// The method is explicitly marked as abstract using `@abstractmethod`.
    Explicit,
    /// The method is implicitly abstract due to being in a `Protocol` class without an
    /// implementation.
    ImplicitDueToStubBody,
    /// The method is implicitly abstract due to being in a `Protocol` class with a body that
    /// solely consists of `raise NotImplementedError` statements.
    ImplicitDueToAlwaysRaising,
}

impl AbstractMethodKind {
    const fn is_explicit(self) -> bool {
        matches!(self, AbstractMethodKind::Explicit)
    }

    const fn is_implicit_due_to_stub_body(self) -> bool {
        matches!(self, AbstractMethodKind::ImplicitDueToStubBody)
    }

    /// Return `Some()` if the function passed in is an abstract method.
    ///
    /// A method can be abstract if it is explicitly decorated with `@abstractmethod`,
    /// or if it is an overloaded `Protocol` method without an implementation,
    /// or if it is a `Protocol` method with a body that solely consists of `pass`/`...`
    /// statements, or if it is a `Protocol` method that only has a docstring,
    /// or if it is a `Protocol` method whose body only consists of a single
    /// `raise NotImplementedError` statement.
    pub(super) fn of_function<'db>(
        db: &'db dyn Db,
        function: FunctionType<'db>,
        enclosing_class: ClassType<'db>,
    ) -> Option<Self> {
        if function.has_known_decorator(db, FunctionDecorators::ABSTRACT_METHOD) {
            return Some(AbstractMethodKind::Explicit);
        }
        if function.definition(db).file(db).is_stub(db) {
            return None;
        }
        if !enclosing_class.is_protocol(db) {
            return None;
        }
        match function.literal(db).body_kind(db) {
            FunctionBodyKind::Stub => Some(AbstractMethodKind::ImplicitDueToStubBody),
            FunctionBodyKind::AlwaysRaisesNotImplementedError => {
                Some(AbstractMethodKind::ImplicitDueToAlwaysRaising)
            }
            FunctionBodyKind::Regular => None,
        }
    }
}
