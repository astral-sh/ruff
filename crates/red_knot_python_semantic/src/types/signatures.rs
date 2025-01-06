#![allow(dead_code)]
use super::{definition_expression_ty, Type};
use crate::Db;
use crate::{semantic_index::definition::Definition, types::todo_type};
use ruff_python_ast::{self as ast, name::Name};

/// A typed callable signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Signature<'db> {
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
}

/// The parameters portion of a typed signature.
///
/// The ordering of parameters is always as given in this struct: first positional-only parameters,
/// then positional-or-keyword, then optionally the variadic parameter, then keyword-only
/// parameters, and last, optionally the variadic keywords parameter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Parameters<'db> {
    /// Parameters which may only be filled by positional arguments.
    positional_only: Box<[ParameterWithDefault<'db>]>,

    /// Parameters which may be filled by positional or keyword arguments.
    positional_or_keyword: Box<[ParameterWithDefault<'db>]>,

    /// The `*args` variadic parameter, if any.
    variadic: Option<Parameter<'db>>,

    /// Parameters which may only be filled by keyword arguments.
    keyword_only: Box<[ParameterWithDefault<'db>]>,

    /// The `**kwargs` variadic keywords parameter, if any.
    keywords: Option<Parameter<'db>>,
}

impl<'db> Parameters<'db> {
    /// Return todo parameters: (*args: Todo, **kwargs: Todo)
    fn todo() -> Self {
        Self {
            variadic: Some(Parameter {
                name: Some(Name::new_static("args")),
                annotated_ty: todo_type!(),
            }),
            keywords: Some(Parameter {
                name: Some(Name::new_static("kwargs")),
                annotated_ty: todo_type!(),
            }),
            ..Default::default()
        }
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
            .map(|arg| ParameterWithDefault::from_node(db, definition, arg))
            .collect();
        let positional_or_keyword = args
            .iter()
            .map(|arg| ParameterWithDefault::from_node(db, definition, arg))
            .collect();
        let variadic = vararg
            .as_ref()
            .map(|arg| Parameter::from_node(db, definition, arg));
        let keyword_only = kwonlyargs
            .iter()
            .map(|arg| ParameterWithDefault::from_node(db, definition, arg))
            .collect();
        let keywords = kwarg
            .as_ref()
            .map(|arg| Parameter::from_node(db, definition, arg));
        Self {
            positional_only,
            positional_or_keyword,
            variadic,
            keyword_only,
            keywords,
        }
    }
}

/// A single parameter of a typed signature, with optional default value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ParameterWithDefault<'db> {
    parameter: Parameter<'db>,

    /// Type of the default value, if any.
    default_ty: Option<Type<'db>>,
}

impl<'db> ParameterWithDefault<'db> {
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
            parameter: Parameter::from_node(db, definition, &parameter_with_default.parameter),
        }
    }
}

/// A single parameter of a typed signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Parameter<'db> {
    /// Parameter name.
    ///
    /// It is possible for signatures to be defined in ways that leave positional-only parameters
    /// nameless (e.g. via `Callable` annotations).
    name: Option<Name>,

    /// Annotated type of the parameter (Unknown if no annotation.)
    annotated_ty: Type<'db>,
}

impl<'db> Parameter<'db> {
    fn from_node(
        db: &'db dyn Db,
        definition: Definition<'db>,
        parameter: &'db ast::Parameter,
    ) -> Self {
        Parameter {
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
    use crate::types::{global_symbol, FunctionType};
    use ruff_db::system::DbWithTestSystem;

    #[track_caller]
    fn get_function_f<'db>(db: &'db TestDb, file: &'static str) -> FunctionType<'db> {
        let module = ruff_db::files::system_path_to_file(db, file).unwrap();
        global_symbol(db, module, "f")
            .expect_type()
            .expect_function_literal()
    }

    #[track_caller]
    fn assert_param_with_default<'db>(
        db: &'db TestDb,
        param_with_default: &ParameterWithDefault<'db>,
        expected_name: &'static str,
        expected_annotation_ty_display: &'static str,
        expected_default_ty_display: Option<&'static str>,
    ) {
        assert_eq!(
            param_with_default
                .default_ty
                .map(|ty| ty.display(db).to_string()),
            expected_default_ty_display.map(ToString::to_string)
        );
        assert_param(
            db,
            &param_with_default.parameter,
            expected_name,
            expected_annotation_ty_display,
        );
    }

    #[track_caller]
    fn assert_param<'db>(
        db: &'db TestDb,
        param: &Parameter<'db>,
        expected_name: &'static str,
        expected_annotation_ty_display: &'static str,
    ) {
        assert_eq!(param.name.as_ref().unwrap(), expected_name);
        assert_eq!(
            param.annotated_ty.display(db).to_string(),
            expected_annotation_ty_display
        );
    }

    #[test]
    fn empty() {
        let mut db = setup_db();
        db.write_dedented("/src/a.py", "def f(): ...").unwrap();
        let func = get_function_f(&db, "/src/a.py");

        let sig = func.internal_signature(&db);

        assert_eq!(sig.return_ty.display(&db).to_string(), "Unknown");
        let params = sig.parameters;
        assert!(params.positional_only.is_empty());
        assert!(params.positional_or_keyword.is_empty());
        assert!(params.variadic.is_none());
        assert!(params.keyword_only.is_empty());
        assert!(params.keywords.is_none());
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
        let params = sig.parameters;
        let [a, b, c, d] = &params.positional_only[..] else {
            panic!("expected four positional-only parameters");
        };
        let [e, f] = &params.positional_or_keyword[..] else {
            panic!("expected two positional-or-keyword parameters");
        };
        let Some(args) = params.variadic else {
            panic!("expected a variadic parameter");
        };
        let [g, h] = &params.keyword_only[..] else {
            panic!("expected two keyword-only parameters");
        };
        let Some(kwargs) = params.keywords else {
            panic!("expected a kwargs parameter");
        };

        assert_param_with_default(&db, a, "a", "Unknown", None);
        assert_param_with_default(&db, b, "b", "int", None);
        assert_param_with_default(&db, c, "c", "Unknown", Some("Literal[1]"));
        assert_param_with_default(&db, d, "d", "int", Some("Literal[2]"));
        assert_param_with_default(&db, e, "e", "Unknown", Some("Literal[3]"));
        assert_param_with_default(&db, f, "f", "Literal[4]", Some("Literal[4]"));
        assert_param_with_default(&db, g, "g", "Unknown", Some("Literal[5]"));
        assert_param_with_default(&db, h, "h", "Literal[6]", Some("Literal[6]"));
        assert_param(&db, &args, "args", "object");
        assert_param(&db, &kwargs, "kwargs", "str");
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

        let [a] = &sig.parameters.positional_or_keyword[..] else {
            panic!("expected one positional-or-keyword parameter");
        };
        // Parameter resolution not deferred; we should see A not B
        assert_param_with_default(&db, a, "a", "A", None);
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

        let [a] = &sig.parameters.positional_or_keyword[..] else {
            panic!("expected one positional-or-keyword parameter");
        };
        // Parameter resolution deferred; we should see B
        assert_param_with_default(&db, a, "a", "B", None);
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

        let [a, b] = &sig.parameters.positional_or_keyword[..] else {
            panic!("expected two positional-or-keyword parameters");
        };
        // TODO resolution should not be deferred; we should see A not B
        assert_param_with_default(&db, a, "a", "B", None);
        assert_param_with_default(&db, b, "b", "T", None);
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

        let [a, b] = &sig.parameters.positional_or_keyword[..] else {
            panic!("expected two positional-or-keyword parameters");
        };
        // Parameter resolution deferred; we should see B
        assert_param_with_default(&db, a, "a", "B", None);
        assert_param_with_default(&db, b, "b", "T", None);
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
