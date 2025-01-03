use super::{definition_expression_ty, Type};
use crate::Db;
use crate::{semantic_index::definition::Definition, types::todo_type};
use ruff_python_ast::{self as ast, name::Name};

/// A typed callable signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Signature<'db> {
    /// Parameters, in source order.
    ///
    /// The ordering of parameters in a valid signature must be: first positional-only parameters,
    /// then positional-or-keyword, then optionally the variadic parameter, then keyword-only
    /// parameters, and last, optionally the variadic keywords parameter. Parameters with defaults
    /// must come after parameters without defaults.
    parameters: Parameters<'db>,

    /// Annotated return type (Unknown if no annotation.)
    pub(crate) return_ty: Type<'db>,
}

impl<'db> Signature<'db> {
    /// Return a todo signature: (*args: Todo, **kwargs: Todo) -> Todo
    pub(crate) fn todo() -> Self {
        Self {
            parameters: Parameters::todo(),
            return_ty: todo_type!("return type"),
        }
    }

    /// Return a typed signature from a function definition.
    pub(super) fn from_function(
        db: &'db dyn Db,
        definition: Definition<'db>,
        function_node: &'db ast::StmtFunctionDef,
    ) -> Self {
        let return_ty = function_node
            .returns
            .as_ref()
            .map(|returns| {
                if function_node.is_async {
                    todo_type!("generic types.CoroutineType")
                } else {
                    definition_expression_ty(db, definition, returns.as_ref())
                }
            })
            .unwrap_or(Type::Unknown);

        Self {
            parameters: Parameters::from_parameters(
                db,
                definition,
                function_node.parameters.as_ref(),
            ),
            return_ty,
        }
    }

    /// Return number of parameters in this signature.
    pub(crate) fn parameter_count(&self) -> usize {
        self.parameters.0.len()
    }

    /// Return number of positional parameters this signature can accept.
    ///
    /// Doesn't account for variadic parameter.
    pub(crate) fn positional_parameter_count(&self) -> usize {
        self.parameters
            .into_iter()
            .take_while(|param| param.is_positional())
            .count()
    }

    pub(crate) fn parameter_at_index(&self, index: usize) -> Option<&Parameter<'db>> {
        self.parameters.0.get(index)
    }

    /// Return positional parameter at given index, or `None` if `index` is out of range.
    ///
    /// Does not return variadic parameter.
    pub(crate) fn positional_at_index(&self, index: usize) -> Option<&Parameter<'db>> {
        if let Some(candidate) = self.parameter_at_index(index) {
            if candidate.is_positional() {
                return Some(candidate);
            }
        }
        None
    }

    /// Return the variadic parameter (`*args`), if any, and its index, or `None`.
    pub(crate) fn variadic_parameter(&self) -> Option<(usize, &Parameter<'db>)> {
        self.parameters
            .into_iter()
            .enumerate()
            .find(|(_, parameter)| parameter.is_variadic())
    }

    /// Return parameter (with index) for given name, or `None` if no such parameter.
    ///
    /// Does not return keywords (`**kwargs`) parameter.
    pub(crate) fn keyword_by_name(
        &self,
        name: &ast::name::Name,
    ) -> Option<(usize, &Parameter<'db>)> {
        self.parameters
            .into_iter()
            .enumerate()
            .find(|(_, parameter)| parameter.callable_by_name(name))
    }

    /// Return the keywords parameter (`**kwargs`), if any, and its index, or `None`.
    pub(crate) fn keywords_parameter(&self) -> Option<(usize, &Parameter<'db>)> {
        self.parameters
            .into_iter()
            .enumerate()
            .find(|(_, parameter)| parameter.is_keywords())
    }
}

// TODO: use SmallVec here once invariance bug is fixed
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Parameters<'db>(Vec<Parameter<'db>>);

impl<'db> Parameters<'db> {
    /// Return todo parameters: (*args: Todo, **kwargs: Todo)
    fn todo() -> Self {
        Self(vec![
            Parameter::Variadic(Param {
                name: Some(Name::new_static("args")),
                annotated_ty: todo_type!("todo signature *args"),
            }),
            Parameter::Keywords(Param {
                name: Some(Name::new_static("kwargs")),
                annotated_ty: todo_type!("todo signature **kwargs"),
            }),
        ])
    }

    fn from_parameters(
        db: &'db dyn Db,
        definition: Definition<'db>,
        parameters: &'db ast::Parameters,
    ) -> Self {
        let ast::Parameters {
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
            range: _,
        } = parameters;
        let positional_only = posonlyargs
            .iter()
            .map(|arg| Parameter::PositionalOnly(ParamWithDefault::from_node(db, definition, arg)));
        let positional_or_keyword = args.iter().map(|arg| {
            Parameter::PositionalOrKeyword(ParamWithDefault::from_node(db, definition, arg))
        });
        let variadic = vararg
            .as_ref()
            .map(|arg| Parameter::Variadic(Param::from_node(db, definition, arg)));
        let keyword_only = kwonlyargs
            .iter()
            .map(|arg| Parameter::KeywordOnly(ParamWithDefault::from_node(db, definition, arg)));
        let keywords = kwarg
            .as_ref()
            .map(|arg| Parameter::Keywords(Param::from_node(db, definition, arg)));
        Self(
            positional_only
                .chain(positional_or_keyword)
                .chain(variadic)
                .chain(keyword_only)
                .chain(keywords)
                .collect(),
        )
    }
}

impl<'db, 'a> IntoIterator for &'a Parameters<'db> {
    type Item = &'a Parameter<'db>;
    type IntoIter = std::slice::Iter<'a, Parameter<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Parameter<'db> {
    /// Positional-only parameter, e.g. `def f(x, /): ...`
    PositionalOnly(ParamWithDefault<'db>),
    /// Positional-or-keyword parameter, e.g. `def f(x): ...`
    PositionalOrKeyword(ParamWithDefault<'db>),
    /// Variadic parameter, e.g. `def f(*args): ...`
    Variadic(Param<'db>),
    /// Keyword-only parameter, e.g. `def f(*, x): ...`
    KeywordOnly(ParamWithDefault<'db>),
    /// Variadic keywords parameter, e.g. `def f(**kwargs): ...`
    Keywords(Param<'db>),
}

impl<'db> Parameter<'db> {
    pub(crate) fn is_variadic(&self) -> bool {
        matches!(self, Self::Variadic(_))
    }

    pub(crate) fn is_keywords(&self) -> bool {
        matches!(self, Self::Keywords(_))
    }

    pub(crate) fn is_positional(&self) -> bool {
        matches!(self, Self::PositionalOnly(_) | Self::PositionalOrKeyword(_))
    }

    pub(crate) fn callable_by_name(&self, name: &ast::name::Name) -> bool {
        match self {
            Self::PositionalOrKeyword(param) | Self::KeywordOnly(param) => param
                .param
                .name
                .as_ref()
                .is_some_and(|param_name| param_name == name),
            _ => false,
        }
    }

    /// Annotated type of the parameter.
    pub(crate) fn annotated_ty(&self) -> Type<'db> {
        self.param().annotated_ty
    }

    /// Name of the parameter (if it has one).
    pub(crate) fn name(&self) -> Option<&ast::name::Name> {
        self.param().name.as_ref()
    }

    /// Display name of the parameter, with fallback if it doesn't have a name.
    pub(crate) fn display_name(&self, index: usize) -> ast::name::Name {
        self.name()
            .cloned()
            .unwrap_or_else(|| ast::name::Name::new(format!("positional parameter {index}")))
    }

    /// Default-value type of the parameter, if any.
    pub(crate) fn default_ty(&self) -> Option<Type<'db>> {
        match self {
            Self::PositionalOnly(ParamWithDefault { default_ty, .. }) => *default_ty,
            Self::PositionalOrKeyword(ParamWithDefault { default_ty, .. }) => *default_ty,
            Self::Variadic(_) => None,
            Self::KeywordOnly(ParamWithDefault { default_ty, .. }) => *default_ty,
            Self::Keywords(_) => None,
        }
    }

    fn param(&self) -> &Param<'db> {
        let (Self::PositionalOnly(ParamWithDefault { param, .. })
        | Self::PositionalOrKeyword(ParamWithDefault { param, .. })
        | Self::Variadic(param)
        | Self::KeywordOnly(ParamWithDefault { param, .. })
        | Self::Keywords(param)) = self;
        param
    }
}

/// A single parameter of a typed signature, with optional default value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParamWithDefault<'db> {
    param: Param<'db>,

    /// Type of the default value, if any.
    default_ty: Option<Type<'db>>,
}

impl<'db> ParamWithDefault<'db> {
    fn from_node(
        db: &'db dyn Db,
        definition: Definition<'db>,
        parameter_with_default: &'db ast::ParameterWithDefault,
    ) -> Self {
        Self {
            default_ty: parameter_with_default
                .default
                .as_deref()
                .map(|default| definition_expression_ty(db, definition, default)),
            param: Param::from_node(db, definition, &parameter_with_default.parameter),
        }
    }
}

/// A single parameter of a typed signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Param<'db> {
    /// Parameter name.
    ///
    /// It is possible for signatures to be defined in ways that leave positional-only parameters
    /// nameless (e.g. via `Callable` annotations).
    name: Option<Name>,

    /// Annotated type of the parameter (Unknown if no annotation.)
    annotated_ty: Type<'db>,
}

impl<'db> Param<'db> {
    fn from_node(
        db: &'db dyn Db,
        definition: Definition<'db>,
        parameter: &'db ast::Parameter,
    ) -> Self {
        Param {
            name: Some(parameter.name.id.clone()),
            annotated_ty: parameter
                .annotation
                .as_deref()
                .map(|annotation| definition_expression_ty(db, definition, annotation))
                .unwrap_or(Type::Unknown),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::{setup_db, TestDb};
    use crate::types::{global_symbol, FunctionType, KnownClass};
    use ruff_db::system::DbWithTestSystem;

    #[track_caller]
    fn get_function_f<'db>(db: &'db TestDb, file: &'static str) -> FunctionType<'db> {
        let module = ruff_db::files::system_path_to_file(db, file).unwrap();
        global_symbol(db, module, "f")
            .expect_type()
            .expect_function_literal()
    }

    #[track_caller]
    fn assert_params<'db>(signature: &Signature<'db>, expected: &[Parameter<'db>]) {
        assert_eq!(signature.parameters.0.as_slice(), expected);
    }

    #[test]
    fn empty() {
        let mut db = setup_db();
        db.write_dedented("/src/a.py", "def f(): ...").unwrap();
        let func = get_function_f(&db, "/src/a.py");

        let sig = func.internal_signature(&db);

        assert_eq!(sig.return_ty.display(&db).to_string(), "Unknown");
        assert_params(&sig, &[]);
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn full() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            def f(a, b: int, c = 1, d: int = 2, /,
                  e = 3, f: Literal[4] = 4, *args: object,
                  g = 5, h: Literal[6] = 6, **kwargs: str) -> bytes: ...
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.py");

        let sig = func.internal_signature(&db);

        assert_eq!(sig.return_ty.display(&db).to_string(), "bytes");
        assert_params(
            &sig,
            &[
                Parameter::PositionalOnly(ParamWithDefault {
                    param: Param {
                        name: Some(Name::new_static("a")),
                        annotated_ty: Type::Unknown,
                    },
                    default_ty: None,
                }),
                Parameter::PositionalOnly(ParamWithDefault {
                    param: Param {
                        name: Some(Name::new_static("b")),
                        annotated_ty: KnownClass::Int.to_instance(&db),
                    },
                    default_ty: None,
                }),
                Parameter::PositionalOnly(ParamWithDefault {
                    param: Param {
                        name: Some(Name::new_static("c")),
                        annotated_ty: Type::Unknown,
                    },
                    default_ty: Some(Type::IntLiteral(1)),
                }),
                Parameter::PositionalOnly(ParamWithDefault {
                    param: Param {
                        name: Some(Name::new_static("d")),
                        annotated_ty: KnownClass::Int.to_instance(&db),
                    },
                    default_ty: Some(Type::IntLiteral(2)),
                }),
                Parameter::PositionalOrKeyword(ParamWithDefault {
                    param: Param {
                        name: Some(Name::new_static("e")),
                        annotated_ty: Type::Unknown,
                    },
                    default_ty: Some(Type::IntLiteral(3)),
                }),
                Parameter::PositionalOrKeyword(ParamWithDefault {
                    param: Param {
                        name: Some(Name::new_static("f")),
                        annotated_ty: Type::IntLiteral(4),
                    },
                    default_ty: Some(Type::IntLiteral(4)),
                }),
                Parameter::Variadic(Param {
                    name: Some(Name::new_static("args")),
                    annotated_ty: KnownClass::Object.to_instance(&db),
                }),
                Parameter::KeywordOnly(ParamWithDefault {
                    param: Param {
                        name: Some(Name::new_static("g")),
                        annotated_ty: Type::Unknown,
                    },
                    default_ty: Some(Type::IntLiteral(5)),
                }),
                Parameter::KeywordOnly(ParamWithDefault {
                    param: Param {
                        name: Some(Name::new_static("h")),
                        annotated_ty: Type::IntLiteral(6),
                    },
                    default_ty: Some(Type::IntLiteral(6)),
                }),
                Parameter::Keywords(Param {
                    name: Some(Name::new_static("kwargs")),
                    annotated_ty: KnownClass::Str.to_instance(&db),
                }),
            ],
        );
    }

    #[test]
    fn not_deferred() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            class A: ...
            class B: ...

            alias = A

            def f(a: alias): ...

            alias = B
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.py");

        let sig = func.internal_signature(&db);

        let [Parameter::PositionalOrKeyword(ParamWithDefault {
            param:
                Param {
                    name: Some(name),
                    annotated_ty,
                },
            ..
        })] = &sig.parameters.0[..]
        else {
            panic!("expected one positional-or-keyword parameter");
        };
        assert_eq!(name, "a");
        // Parameter resolution not deferred; we should see A not B
        assert_eq!(annotated_ty.display(&db).to_string(), "A");
    }

    #[test]
    fn deferred_in_stub() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.pyi",
            "
            class A: ...
            class B: ...

            alias = A

            def f(a: alias): ...

            alias = B
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.pyi");

        let sig = func.internal_signature(&db);

        let [Parameter::PositionalOrKeyword(ParamWithDefault {
            param:
                Param {
                    name: Some(name),
                    annotated_ty,
                },
            default_ty: None,
        })] = &sig.parameters.0[..]
        else {
            panic!("expected one positional-or-keyword parameter");
        };
        assert_eq!(name, "a");
        // Parameter resolution deferred; we should see B
        assert_eq!(annotated_ty.display(&db).to_string(), "B");
    }

    #[test]
    fn generic_not_deferred() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            class A: ...
            class B: ...

            alias = A

            def f[T](a: alias, b: T) -> T: ...

            alias = B
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.py");

        let sig = func.internal_signature(&db);

        let [Parameter::PositionalOrKeyword(ParamWithDefault {
            param:
                Param {
                    name: Some(a_name),
                    annotated_ty: a_annotated_ty,
                },
            default_ty: None,
        }), Parameter::PositionalOrKeyword(ParamWithDefault {
            param:
                Param {
                    name: Some(b_name),
                    annotated_ty: b_annotated_ty,
                },
            default_ty: None,
        })] = &sig.parameters.0[..]
        else {
            panic!("expected two positional-or-keyword parameters");
        };
        assert_eq!(a_name, "a");
        assert_eq!(b_name, "b");
        // TODO resolution should not be deferred; we should see A not B
        assert_eq!(a_annotated_ty.display(&db).to_string(), "B");
        assert_eq!(b_annotated_ty.display(&db).to_string(), "T");
    }

    #[test]
    fn generic_deferred_in_stub() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.pyi",
            "
            class A: ...
            class B: ...

            alias = A

            def f[T](a: alias, b: T) -> T: ...

            alias = B
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.pyi");

        let sig = func.internal_signature(&db);

        let [Parameter::PositionalOrKeyword(ParamWithDefault {
            param:
                Param {
                    name: Some(a_name),
                    annotated_ty: a_annotated_ty,
                },
            default_ty: None,
        }), Parameter::PositionalOrKeyword(ParamWithDefault {
            param:
                Param {
                    name: Some(b_name),
                    annotated_ty: b_annotated_ty,
                },
            default_ty: None,
        })] = &sig.parameters.0[..]
        else {
            panic!("expected two positional-or-keyword parameters");
        };
        assert_eq!(a_name, "a");
        assert_eq!(b_name, "b");
        // Parameter resolution deferred; we should see B
        assert_eq!(a_annotated_ty.display(&db).to_string(), "B");
        assert_eq!(b_annotated_ty.display(&db).to_string(), "T");
    }

    #[test]
    fn external_signature_no_decorator() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            def f(a: int) -> int: ...
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.py");

        let expected_sig = func.internal_signature(&db);

        // With no decorators, internal and external signature are the same
        assert_eq!(func.signature(&db), &expected_sig);
    }

    #[test]
    fn external_signature_decorated() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            def deco(func): ...

            @deco
            def f(a: int) -> int: ...
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.py");

        let expected_sig = Signature::todo();

        // With no decorators, internal and external signature are the same
        assert_eq!(func.signature(&db), &expected_sig);
    }
}
