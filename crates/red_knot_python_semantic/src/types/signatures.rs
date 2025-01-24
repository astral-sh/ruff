use super::{definition_expression_type, Type};
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
    ///
    /// We may get invalid signatures, though, and need to handle them without panicking.
    parameters: Parameters<'db>,

    /// Annotated return type, if any.
    pub(crate) return_ty: Option<Type<'db>>,
}

impl<'db> Signature<'db> {
    /// Return a todo signature: (*args: Todo, **kwargs: Todo) -> Todo
    pub(crate) fn todo() -> Self {
        Self {
            parameters: Parameters::todo(),
            return_ty: Some(todo_type!("return type")),
        }
    }

    /// Return a typed signature from a function definition.
    pub(super) fn from_function(
        db: &'db dyn Db,
        definition: Definition<'db>,
        function_node: &'db ast::StmtFunctionDef,
    ) -> Self {
        let return_ty = function_node.returns.as_ref().map(|returns| {
            if function_node.is_async {
                todo_type!("generic types.CoroutineType")
            } else {
                definition_expression_type(db, definition, returns.as_ref())
            }
        });

        Self {
            parameters: Parameters::from_parameters(
                db,
                definition,
                function_node.parameters.as_ref(),
            ),
            return_ty,
        }
    }

    /// Return the parameters in this signature.
    pub(crate) fn parameters(&self) -> &Parameters<'db> {
        &self.parameters
    }
}

// TODO: use SmallVec here once invariance bug is fixed
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Parameters<'db>(Vec<Parameter<'db>>);

impl<'db> Parameters<'db> {
    /// Return todo parameters: (*args: Todo, **kwargs: Todo)
    fn todo() -> Self {
        Self(vec![
            Parameter {
                name: Some(Name::new_static("args")),
                annotated_ty: Some(todo_type!("todo signature *args")),
                kind: ParameterKind::Variadic,
            },
            Parameter {
                name: Some(Name::new_static("kwargs")),
                annotated_ty: Some(todo_type!("todo signature **kwargs")),
                kind: ParameterKind::KeywordVariadic,
            },
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
        let default_ty = |parameter_with_default: &ast::ParameterWithDefault| {
            parameter_with_default
                .default
                .as_deref()
                .map(|default| definition_expression_type(db, definition, default))
        };
        let positional_only = posonlyargs.iter().map(|arg| {
            Parameter::from_node_and_kind(
                db,
                definition,
                &arg.parameter,
                ParameterKind::PositionalOnly {
                    default_ty: default_ty(arg),
                },
            )
        });
        let positional_or_keyword = args.iter().map(|arg| {
            Parameter::from_node_and_kind(
                db,
                definition,
                &arg.parameter,
                ParameterKind::PositionalOrKeyword {
                    default_ty: default_ty(arg),
                },
            )
        });
        let variadic = vararg
            .as_ref()
            .map(|arg| Parameter::from_node_and_kind(db, definition, arg, ParameterKind::Variadic));
        let keyword_only = kwonlyargs.iter().map(|arg| {
            Parameter::from_node_and_kind(
                db,
                definition,
                &arg.parameter,
                ParameterKind::KeywordOnly {
                    default_ty: default_ty(arg),
                },
            )
        });
        let keywords = kwarg.as_ref().map(|arg| {
            Parameter::from_node_and_kind(db, definition, arg, ParameterKind::KeywordVariadic)
        });
        Self(
            positional_only
                .chain(positional_or_keyword)
                .chain(variadic)
                .chain(keyword_only)
                .chain(keywords)
                .collect(),
        )
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<Parameter<'db>> {
        self.0.iter()
    }

    /// Iterate initial positional parameters, not including variadic parameter, if any.
    ///
    /// For a valid signature, this will be all positional parameters. In an invalid signature,
    /// there could be non-initial positional parameters; effectively, we just won't consider those
    /// to be positional, which is fine.
    pub(crate) fn positional(&self) -> impl Iterator<Item = &Parameter<'db>> {
        self.iter().take_while(|param| param.is_positional())
    }

    /// Return parameter at given index, or `None` if index is out-of-range.
    pub(crate) fn get(&self, index: usize) -> Option<&Parameter<'db>> {
        self.0.get(index)
    }

    /// Return positional parameter at given index, or `None` if `index` is out of range.
    ///
    /// Does not return variadic parameter.
    pub(crate) fn get_positional(&self, index: usize) -> Option<&Parameter<'db>> {
        self.get(index)
            .and_then(|parameter| parameter.is_positional().then_some(parameter))
    }

    /// Return the variadic parameter (`*args`), if any, and its index, or `None`.
    pub(crate) fn variadic(&self) -> Option<(usize, &Parameter<'db>)> {
        self.iter()
            .enumerate()
            .find(|(_, parameter)| parameter.is_variadic())
    }

    /// Return parameter (with index) for given name, or `None` if no such parameter.
    ///
    /// Does not return keywords (`**kwargs`) parameter.
    ///
    /// In an invalid signature, there could be multiple parameters with the same name; we will
    /// just return the first that matches.
    pub(crate) fn keyword_by_name(&self, name: &str) -> Option<(usize, &Parameter<'db>)> {
        self.iter()
            .enumerate()
            .find(|(_, parameter)| parameter.callable_by_name(name))
    }

    /// Return the keywords parameter (`**kwargs`), if any, and its index, or `None`.
    pub(crate) fn keyword_variadic(&self) -> Option<(usize, &Parameter<'db>)> {
        self.iter()
            .enumerate()
            .rfind(|(_, parameter)| parameter.is_keyword_variadic())
    }
}

impl<'db, 'a> IntoIterator for &'a Parameters<'db> {
    type Item = &'a Parameter<'db>;
    type IntoIter = std::slice::Iter<'a, Parameter<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'db> std::ops::Index<usize> for Parameters<'db> {
    type Output = Parameter<'db>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Parameter<'db> {
    /// Parameter name.
    ///
    /// It is possible for signatures to be defined in ways that leave positional-only parameters
    /// nameless (e.g. via `Callable` annotations).
    name: Option<Name>,

    /// Annotated type of the parameter.
    annotated_ty: Option<Type<'db>>,

    kind: ParameterKind<'db>,
}

impl<'db> Parameter<'db> {
    fn from_node_and_kind(
        db: &'db dyn Db,
        definition: Definition<'db>,
        parameter: &'db ast::Parameter,
        kind: ParameterKind<'db>,
    ) -> Self {
        Self {
            name: Some(parameter.name.id.clone()),
            annotated_ty: parameter
                .annotation
                .as_deref()
                .map(|annotation| definition_expression_type(db, definition, annotation)),
            kind,
        }
    }

    pub(crate) fn is_variadic(&self) -> bool {
        matches!(self.kind, ParameterKind::Variadic)
    }

    pub(crate) fn is_keyword_variadic(&self) -> bool {
        matches!(self.kind, ParameterKind::KeywordVariadic)
    }

    pub(crate) fn is_positional(&self) -> bool {
        matches!(
            self.kind,
            ParameterKind::PositionalOnly { .. } | ParameterKind::PositionalOrKeyword { .. }
        )
    }

    pub(crate) fn callable_by_name(&self, name: &str) -> bool {
        match self.kind {
            ParameterKind::PositionalOrKeyword { .. } | ParameterKind::KeywordOnly { .. } => self
                .name
                .as_ref()
                .is_some_and(|param_name| param_name == name),
            _ => false,
        }
    }

    /// Annotated type of the parameter, if annotated.
    pub(crate) fn annotated_type(&self) -> Option<Type<'db>> {
        self.annotated_ty
    }

    /// Name of the parameter (if it has one).
    pub(crate) fn name(&self) -> Option<&ast::name::Name> {
        self.name.as_ref()
    }

    /// Display name of the parameter, if it has one.
    pub(crate) fn display_name(&self) -> Option<ast::name::Name> {
        self.name().map(|name| match self.kind {
            ParameterKind::Variadic => ast::name::Name::new(format!("*{name}")),
            ParameterKind::KeywordVariadic => ast::name::Name::new(format!("**{name}")),
            _ => name.clone(),
        })
    }

    /// Default-value type of the parameter, if any.
    pub(crate) fn default_type(&self) -> Option<Type<'db>> {
        match self.kind {
            ParameterKind::PositionalOnly { default_ty } => default_ty,
            ParameterKind::PositionalOrKeyword { default_ty } => default_ty,
            ParameterKind::Variadic => None,
            ParameterKind::KeywordOnly { default_ty } => default_ty,
            ParameterKind::KeywordVariadic => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ParameterKind<'db> {
    /// Positional-only parameter, e.g. `def f(x, /): ...`
    PositionalOnly { default_ty: Option<Type<'db>> },
    /// Positional-or-keyword parameter, e.g. `def f(x): ...`
    PositionalOrKeyword { default_ty: Option<Type<'db>> },
    /// Variadic parameter, e.g. `def f(*args): ...`
    Variadic,
    /// Keyword-only parameter, e.g. `def f(*, x): ...`
    KeywordOnly { default_ty: Option<Type<'db>> },
    /// Variadic keywords parameter, e.g. `def f(**kwargs): ...`
    KeywordVariadic,
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

        assert!(sig.return_ty.is_none());
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

        assert_eq!(sig.return_ty.unwrap().display(&db).to_string(), "bytes");
        assert_params(
            &sig,
            &[
                Parameter {
                    name: Some(Name::new_static("a")),
                    annotated_ty: None,
                    kind: ParameterKind::PositionalOnly { default_ty: None },
                },
                Parameter {
                    name: Some(Name::new_static("b")),
                    annotated_ty: Some(KnownClass::Int.to_instance(&db)),
                    kind: ParameterKind::PositionalOnly { default_ty: None },
                },
                Parameter {
                    name: Some(Name::new_static("c")),
                    annotated_ty: None,
                    kind: ParameterKind::PositionalOnly {
                        default_ty: Some(Type::IntLiteral(1)),
                    },
                },
                Parameter {
                    name: Some(Name::new_static("d")),
                    annotated_ty: Some(KnownClass::Int.to_instance(&db)),
                    kind: ParameterKind::PositionalOnly {
                        default_ty: Some(Type::IntLiteral(2)),
                    },
                },
                Parameter {
                    name: Some(Name::new_static("e")),
                    annotated_ty: None,
                    kind: ParameterKind::PositionalOrKeyword {
                        default_ty: Some(Type::IntLiteral(3)),
                    },
                },
                Parameter {
                    name: Some(Name::new_static("f")),
                    annotated_ty: Some(Type::IntLiteral(4)),
                    kind: ParameterKind::PositionalOrKeyword {
                        default_ty: Some(Type::IntLiteral(4)),
                    },
                },
                Parameter {
                    name: Some(Name::new_static("args")),
                    annotated_ty: Some(KnownClass::Object.to_instance(&db)),
                    kind: ParameterKind::Variadic,
                },
                Parameter {
                    name: Some(Name::new_static("g")),
                    annotated_ty: None,
                    kind: ParameterKind::KeywordOnly {
                        default_ty: Some(Type::IntLiteral(5)),
                    },
                },
                Parameter {
                    name: Some(Name::new_static("h")),
                    annotated_ty: Some(Type::IntLiteral(6)),
                    kind: ParameterKind::KeywordOnly {
                        default_ty: Some(Type::IntLiteral(6)),
                    },
                },
                Parameter {
                    name: Some(Name::new_static("kwargs")),
                    annotated_ty: Some(KnownClass::Str.to_instance(&db)),
                    kind: ParameterKind::KeywordVariadic,
                },
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

        let [Parameter {
            name: Some(name),
            annotated_ty,
            kind: ParameterKind::PositionalOrKeyword { .. },
        }] = &sig.parameters.0[..]
        else {
            panic!("expected one positional-or-keyword parameter");
        };
        assert_eq!(name, "a");
        // Parameter resolution not deferred; we should see A not B
        assert_eq!(annotated_ty.unwrap().display(&db).to_string(), "A");
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

        let [Parameter {
            name: Some(name),
            annotated_ty,
            kind: ParameterKind::PositionalOrKeyword { .. },
        }] = &sig.parameters.0[..]
        else {
            panic!("expected one positional-or-keyword parameter");
        };
        assert_eq!(name, "a");
        // Parameter resolution deferred; we should see B
        assert_eq!(annotated_ty.unwrap().display(&db).to_string(), "B");
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

        let [Parameter {
            name: Some(a_name),
            annotated_ty: a_annotated_ty,
            kind: ParameterKind::PositionalOrKeyword { .. },
        }, Parameter {
            name: Some(b_name),
            annotated_ty: b_annotated_ty,
            kind: ParameterKind::PositionalOrKeyword { .. },
        }] = &sig.parameters.0[..]
        else {
            panic!("expected two positional-or-keyword parameters");
        };
        assert_eq!(a_name, "a");
        assert_eq!(b_name, "b");
        // TODO resolution should not be deferred; we should see A not B
        assert_eq!(
            a_annotated_ty.unwrap().display(&db).to_string(),
            "Unknown | B"
        );
        assert_eq!(b_annotated_ty.unwrap().display(&db).to_string(), "T");
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

        let [Parameter {
            name: Some(a_name),
            annotated_ty: a_annotated_ty,
            kind: ParameterKind::PositionalOrKeyword { .. },
        }, Parameter {
            name: Some(b_name),
            annotated_ty: b_annotated_ty,
            kind: ParameterKind::PositionalOrKeyword { .. },
        }] = &sig.parameters.0[..]
        else {
            panic!("expected two positional-or-keyword parameters");
        };
        assert_eq!(a_name, "a");
        assert_eq!(b_name, "b");
        // Parameter resolution deferred; we should see B
        assert_eq!(
            a_annotated_ty.unwrap().display(&db).to_string(),
            "Unknown | B"
        );
        assert_eq!(b_annotated_ty.unwrap().display(&db).to_string(), "T");
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
