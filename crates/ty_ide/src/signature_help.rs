//! This module handles the "signature help" request in the language server
//! protocol. This request is typically issued by a client when the user types
//! an open parenthesis and starts to enter arguments for a function call.
//! The signature help provides information that the editor displays to the
//! user about the target function signature including parameter names,
//! types, and documentation. It supports multiple signatures for union types
//! and overloads.

use crate::Db;
use crate::docstring::Docstring;
use crate::goto::Definitions;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::find_node::covering_node;
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_python_semantic::ResolvedDefinition;
use ty_python_semantic::SemanticModel;
use ty_python_semantic::semantic_index::definition::Definition;
use ty_python_semantic::types::ide_support::{
    CallSignatureDetails, call_signature_details, find_active_signature_from_details,
};
use ty_python_semantic::types::{ParameterKind, Type};

// TODO: We may want to add special-case handling for calls to constructors
// so the class docstring is used in place of (or inaddition to) any docstring
// associated with the __new__ or __init__ call.

/// Information about a function parameter
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterDetails<'db> {
    /// The parameter name (e.g., "param1")
    pub name: String,
    /// The parameter label in the signature (e.g., "param1: str")
    pub label: String,
    /// The annotated type of the parameter, if any
    pub ty: Option<Type<'db>>,
    /// Documentation specific to the parameter, typically extracted from the
    /// function's docstring
    pub documentation: Option<String>,
    /// True if the parameter is positional-only.
    pub is_positional_only: bool,
}

/// Information about a function signature
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureDetails<'db> {
    /// Text representation of the full signature (including input parameters and return type).
    pub label: String,
    /// Documentation for the signature, typically from the function's docstring.
    pub documentation: Option<Docstring>,
    /// Information about each of the parameters in left-to-right order.
    pub parameters: Vec<ParameterDetails<'db>>,
    /// Index of the parameter that corresponds to the argument where the
    /// user's cursor is currently positioned.
    pub active_parameter: Option<usize>,
}

/// Signature help information for function calls
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureHelpInfo<'db> {
    /// Information about each of the signatures for the function call. We
    /// need to handle multiple because of unions, overloads, and composite
    /// calls like constructors (which invoke both __new__ and __init__).
    pub signatures: Vec<SignatureDetails<'db>>,
    /// Index of the "active signature" which is the first signature where
    /// all arguments that are currently present in the code map to parameters.
    pub active_signature: Option<usize>,
}

/// Signature help information for function calls at the given position
pub fn signature_help(db: &dyn Db, file: File, offset: TextSize) -> Option<SignatureHelpInfo<'_>> {
    let parsed = parsed_module(db, file).load(db);

    // Get the call expression at the given position.
    let (call_expr, current_arg_index) = get_call_expr(&parsed, offset)?;

    let model = SemanticModel::new(db, file);

    // Get signature details from the semantic analyzer.
    let signature_details: Vec<CallSignatureDetails<'_>> =
        call_signature_details(&model, call_expr);

    if signature_details.is_empty() {
        return None;
    }

    // Find the active signature - the first signature where all arguments map to parameters.
    let active_signature_index = find_active_signature_from_details(&signature_details);

    // Convert to SignatureDetails objects.
    let signatures: Vec<SignatureDetails> = signature_details
        .into_iter()
        .map(|details| {
            create_signature_details_from_call_signature_details(db, &details, current_arg_index)
        })
        .collect();

    Some(SignatureHelpInfo {
        signatures,
        active_signature: active_signature_index,
    })
}

/// Returns the innermost call expression that contains the specified offset
/// and the index of the argument that the offset maps to.
fn get_call_expr(
    parsed: &ruff_db::parsed::ParsedModuleRef,
    offset: TextSize,
) -> Option<(&ast::ExprCall, usize)> {
    let root_node: AnyNodeRef = parsed.syntax().into();

    // Find the token under the cursor and use its offset to find the node
    let token = parsed
        .tokens()
        .at_offset(offset)
        .max_by_key(|token| match token.kind() {
            TokenKind::Name
            | TokenKind::String
            | TokenKind::Complex
            | TokenKind::Float
            | TokenKind::Int => 1,
            _ => 0,
        })?;

    // Find the covering node at the given position that is a function call.
    // Note that we are okay with the range being anywhere within a call
    // expression, even if it's not in the arguments portion of the call
    // expression. This is because, e.g., a user can request signature
    // information at a call site, and this should ideally work anywhere
    // within the call site, even at the function name.
    let call = covering_node(root_node, token.range())
        .find_first(|node| {
            if !node.is_expr_call() {
                return false;
            }

            // Close the signature help if the cursor is at the closing parenthesis
            if token.kind() == TokenKind::Rpar && node.end() == token.end() && offset == token.end()
            {
                return false;
            }

            if token.range().is_empty() && node.end() == token.end() {
                return false;
            }

            true
        })
        .ok()?;

    // Get the function call expression.
    let AnyNodeRef::ExprCall(call_expr) = call.node() else {
        return None;
    };

    // Determine which argument corresponding to the current cursor location.
    let current_arg_index = get_argument_index(call_expr, offset);

    Some((call_expr, current_arg_index))
}

/// Determine which argument is associated with the specified offset.
/// Returns zero if not within any argument.
fn get_argument_index(call_expr: &ast::ExprCall, offset: TextSize) -> usize {
    let mut current_arg = 0;

    for (i, arg) in call_expr.arguments.arguments_source_order().enumerate() {
        if offset <= arg.end() {
            return i;
        }
        current_arg = i + 1;
    }

    current_arg
}

/// Create signature details from `CallSignatureDetails`.
fn create_signature_details_from_call_signature_details<'db>(
    db: &dyn crate::Db,
    details: &CallSignatureDetails<'db>,
    current_arg_index: usize,
) -> SignatureDetails<'db> {
    let signature_label = details.label.clone();

    let documentation = get_callable_documentation(db, details.definition);

    // Translate the argument index to parameter index using the mapping.
    let active_parameter =
        if details.argument_to_parameter_mapping.is_empty() && current_arg_index == 0 {
            Some(0)
        } else {
            details
                .argument_to_parameter_mapping
                .get(current_arg_index)
                .and_then(|mapping| mapping.parameters.first().copied())
                .or({
                    // If we can't find a mapping for this argument, but we have a current
                    // argument index, use that as the active parameter if it's within bounds.
                    if current_arg_index < details.parameter_label_offsets.len() {
                        Some(current_arg_index)
                    } else {
                        None
                    }
                })
        };

    let parameters = create_parameters_from_offsets(
        &details.parameter_label_offsets,
        &signature_label,
        documentation.as_ref(),
        &details.parameter_names,
        &details.parameter_kinds,
        &details.parameter_types,
    );
    SignatureDetails {
        label: signature_label,
        documentation,
        parameters,
        active_parameter,
    }
}

/// Determine appropriate documentation for a callable type based on its original type.
fn get_callable_documentation(
    db: &dyn crate::Db,
    definition: Option<Definition>,
) -> Option<Docstring> {
    Definitions(vec![ResolvedDefinition::Definition(definition?)]).docstring(db)
}

/// Create `ParameterDetails` objects from parameter label offsets.
fn create_parameters_from_offsets<'db>(
    parameter_offsets: &[TextRange],
    signature_label: &str,
    docstring: Option<&Docstring>,
    parameter_names: &[String],
    parameter_kinds: &[ParameterKind],
    parameter_types: &[Option<Type<'db>>],
) -> Vec<ParameterDetails<'db>> {
    // Extract parameter documentation from the function's docstring if available.
    let param_docs = if let Some(docstring) = docstring {
        docstring.parameter_documentation()
    } else {
        std::collections::HashMap::new()
    };

    parameter_offsets
        .iter()
        .enumerate()
        .map(|(i, offset)| {
            // Extract the parameter label from the signature string.
            let start = usize::from(offset.start());
            let end = usize::from(offset.end());
            let label = signature_label
                .get(start..end)
                .unwrap_or("unknown")
                .to_string();

            // Get the parameter name for documentation lookup.
            let param_name = parameter_names.get(i).map(String::as_str).unwrap_or("");
            let is_positional_only = matches!(
                parameter_kinds.get(i),
                Some(ParameterKind::PositionalOnly { .. })
            );
            let ty = parameter_types.get(i).copied().flatten();

            ParameterDetails {
                name: param_name.to_string(),
                label,
                ty,
                documentation: param_docs.get(param_name).cloned(),
                is_positional_only,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::MarkupKind;
    use crate::docstring::Docstring;
    use crate::signature_help::SignatureHelpInfo;
    use crate::tests::{CursorTest, cursor_test};

    #[test]
    fn signature_help_basic_function_call() {
        let test = cursor_test(
            r#"
        def example_function(param1: str, param2: int) -> str:
            """This is a docstring for the example function.

            Args:
                param1: The first parameter as a string
                param2: The second parameter as an integer

            Returns:
                A formatted string combining both parameters
            """
            return f"{param1}: {param2}"

        result = example_function(<CURSOR>
        "#,
        );

        // Test that signature help is provided
        let result = test.signature_help().expect("Should have signature help");
        assert_eq!(result.signatures.len(), 1);

        let signature = &result.signatures[0];
        assert!(signature.label.contains("param1") && signature.label.contains("param2"));

        // Verify that the docstring is extracted and included in the documentation
        let expected_docstring = concat!(
            "This is a docstring for the example function.\n",
            "\n",
            "Args:\n",
            "    param1: The first parameter as a string\n",
            "    param2: The second parameter as an integer\n",
            "\n",
            "Returns:\n",
            "    A formatted string combining both parameters\n",
        );
        assert_eq!(
            signature
                .documentation
                .as_ref()
                .map(Docstring::render_plaintext),
            Some(expected_docstring.to_string())
        );

        assert_eq!(result.active_signature, Some(0));
        assert_eq!(signature.active_parameter, Some(0));
    }

    #[test]
    fn signature_help_method_call() {
        let test = cursor_test(
            r#"
        class MyClass:
            def my_method(self, arg1: str, arg2: bool) -> None:
                pass

        obj = MyClass()
        obj.my_method(arg2=True, arg1=<CURSOR>
        "#,
        );

        // Test that signature help is provided for method calls
        let result = test.signature_help().expect("Should have signature help");
        assert_eq!(result.signatures.len(), 1);

        let signature = &result.signatures[0];
        assert!(signature.label.contains("arg1") && signature.label.contains("arg2"));
        assert_eq!(result.active_signature, Some(0));

        // Check the active parameter from the active signature
        if let Some(active_sig_index) = result.active_signature {
            let active_signature = &result.signatures[active_sig_index];
            assert_eq!(active_signature.active_parameter, Some(0));
        }
    }

    #[test]
    fn signature_help_nested_function_calls() {
        let test = cursor_test(
            r#"
        def outer(a: int) -> int:
            return a * 2

        def inner(b: str) -> str:
            return b.upper()

        result = outer(inner(<CURSOR>
        "#,
        );

        // Test that signature help focuses on the innermost function call
        let result = test.signature_help().expect("Should have signature help");
        assert_eq!(result.signatures.len(), 1);

        let signature = &result.signatures[0];
        assert!(signature.label.contains("str") || signature.label.contains("->"));
        assert_eq!(result.active_signature, Some(0));
        assert_eq!(signature.active_parameter, Some(0));
    }

    #[test]
    fn signature_help_union_callable() {
        let test = cursor_test(
            r#"
        import random
        def func_a(x: int) -> int:
            return x

        def func_b(y: str) -> str:
            return y

        if random.random() > 0.5:
            f = func_a
        else:
            f = func_b

        f(<CURSOR>
        "#,
        );

        let result = test.signature_help().expect("Should have signature help");

        assert_eq!(result.signatures.len(), 2);

        let signature = &result.signatures[0];
        assert_eq!(signature.label, "(x: int) -> int");
        assert_eq!(signature.parameters.len(), 1);

        // Check parameter information
        let param = &signature.parameters[0];
        assert_eq!(param.label, "x: int");
        assert_eq!(param.name, "x");

        // Validate the second signature (from func_b)
        let signature_b = &result.signatures[1];
        assert_eq!(signature_b.label, "(y: str) -> str");
        assert_eq!(signature_b.parameters.len(), 1);

        // Check parameter information for the second signature
        let param_b = &signature_b.parameters[0];
        assert_eq!(param_b.label, "y: str");
        assert_eq!(param_b.name, "y");

        assert_eq!(result.active_signature, Some(0));

        // Check the active parameter from the active signature
        if let Some(active_sig_index) = result.active_signature {
            let active_signature = &result.signatures[active_sig_index];
            assert_eq!(active_signature.active_parameter, Some(0));
        }
    }

    #[test]
    fn signature_help_overloaded_function() {
        let test = cursor_test(
            r#"
        from typing import overload

        @overload
        def process(value: int) -> str: ...

        @overload
        def process(value: str) -> int: ...

        def process(value):
            if isinstance(value, int):
                return str(value)
            else:
                return len(value)

        result = process(<CURSOR>
        "#,
        );

        // Test that signature help is provided for overloaded functions
        let result = test.signature_help().expect("Should have signature help");

        // We should have signatures for the overloads
        assert_eq!(result.signatures.len(), 2);
        assert_eq!(result.active_signature, Some(0));

        // Check the active parameter from the active signature
        if let Some(active_sig_index) = result.active_signature {
            let active_signature = &result.signatures[active_sig_index];
            assert_eq!(active_signature.active_parameter, Some(0));
        }

        // Validate the first overload: process(value: int) -> str
        let signature1 = &result.signatures[0];
        assert_eq!(signature1.label, "(value: int) -> str");
        assert_eq!(signature1.parameters.len(), 1);

        let param1 = &signature1.parameters[0];
        assert_eq!(param1.label, "value: int");
        assert_eq!(param1.name, "value");

        // Validate the second overload: process(value: str) -> int
        let signature2 = &result.signatures[1];
        assert_eq!(signature2.label, "(value: str) -> int");
        assert_eq!(signature2.parameters.len(), 1);

        let param2 = &signature2.parameters[0];
        assert_eq!(param2.label, "value: str");
        assert_eq!(param2.name, "value");
    }

    #[test]
    fn signature_help_overload_type_disambiguated1() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

ab(1<CURSOR>)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int):
    """the int overload"""

@overload
def ab(a: str): ...
    """the str overload"""
"#,
            )
            .build();

        assert_snapshot!(test.signature_help_render(), @r"
        ============== active signature =============
        (a: int) -> Unknown
        ---------------------------------------------
        the int overload

        -------------- active parameter -------------
        a: int
        ---------------------------------------------

        =============== other signature =============
        (a: str) -> Unknown
        ---------------------------------------------
        the real implementation!

        -------------- active parameter -------------
        a: str
        ---------------------------------------------
        ");
    }

    #[test]
    fn signature_help_overload_type_disambiguated2() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
from mymodule import ab

ab("hello"<CURSOR>)
"#,
            )
            .source(
                "mymodule.py",
                r#"
def ab(a):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int):
    """the int overload"""

@overload
def ab(a: str):
    """the str overload"""
"#,
            )
            .build();

        assert_snapshot!(test.signature_help_render(), @r"
        ============== active signature =============
        (a: int) -> Unknown
        ---------------------------------------------
        the int overload

        -------------- active parameter -------------
        a: int
        ---------------------------------------------

        =============== other signature =============
        (a: str) -> Unknown
        ---------------------------------------------
        the str overload

        -------------- active parameter -------------
        a: str
        ---------------------------------------------
        ");
    }

    #[test]
    fn signature_help_overload_arity_disambiguated1() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

ab(1, 2<CURSOR>)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a, b = None):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int, b: int):
    """the two arg overload"""

@overload
def ab(a: int):
    """the one arg overload"""
"#,
            )
            .build();

        assert_snapshot!(test.signature_help_render(), @r"
        ============== active signature =============
        (a: int, b: int) -> Unknown
        ---------------------------------------------
        the two arg overload

        -------------- active parameter -------------
        b: int
        ---------------------------------------------

        =============== other signature =============
        (a: int) -> Unknown
        ---------------------------------------------
        the one arg overload

        (no active parameter specified)
        ");
    }

    #[test]
    fn signature_help_overload_arity_disambiguated2() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

ab(1<CURSOR>)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a, b = None):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int, b: int):
    """the two arg overload"""

@overload
def ab(a: int):
    """the one arg overload"""
"#,
            )
            .build();

        assert_snapshot!(test.signature_help_render(), @r"
        ============== active signature =============
        (a: int, b: int) -> Unknown
        ---------------------------------------------
        the two arg overload

        -------------- active parameter -------------
        a: int
        ---------------------------------------------

        =============== other signature =============
        (a: int) -> Unknown
        ---------------------------------------------
        the one arg overload

        -------------- active parameter -------------
        a: int
        ---------------------------------------------
        ");
    }

    #[test]
    fn signature_help_overload_keyword_disambiguated1() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

ab(1, b=2<CURSOR>)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a, *, b = None, c = None):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int):
    """keywordless overload"""

@overload
def ab(a: int, *, b: int):
    """b overload"""

@overload
def ab(a: int, *, c: int):
    """c overload"""
"#,
            )
            .build();

        assert_snapshot!(test.signature_help_render(), @r"
        ============== active signature =============
        (a: int, *, b: int) -> Unknown
        ---------------------------------------------
        b overload

        -------------- active parameter -------------
        b: int
        ---------------------------------------------

        =============== other signature =============
        (a: int) -> Unknown
        ---------------------------------------------
        keywordless overload

        (no active parameter specified)
        =============== other signature =============
        (a: int, *, c: int) -> Unknown
        ---------------------------------------------
        c overload

        -------------- active parameter -------------
        c: int
        ---------------------------------------------
        ");
    }

    #[test]
    fn signature_help_overload_keyword_disambiguated2() {
        let test = CursorTest::builder()
            .source(
                "main.py",
                "
from mymodule import ab

ab(1, c=2<CURSOR>)
",
            )
            .source(
                "mymodule.py",
                r#"
def ab(a, *, b = None, c = None):
    """the real implementation!"""
"#,
            )
            .source(
                "mymodule.pyi",
                r#"
from typing import overload

@overload
def ab(a: int):
    """keywordless overload"""

@overload
def ab(a: int, *, b: int):
    """b overload"""

@overload
def ab(a: int, *, c: int):
    """c overload"""
"#,
            )
            .build();

        assert_snapshot!(test.signature_help_render(), @r"
        ============== active signature =============
        (a: int, *, c: int) -> Unknown
        ---------------------------------------------
        c overload

        -------------- active parameter -------------
        c: int
        ---------------------------------------------

        =============== other signature =============
        (a: int) -> Unknown
        ---------------------------------------------
        keywordless overload

        (no active parameter specified)
        =============== other signature =============
        (a: int, *, b: int) -> Unknown
        ---------------------------------------------
        b overload

        -------------- active parameter -------------
        b: int
        ---------------------------------------------
        ");
    }

    #[test]
    fn signature_help_class_constructor() {
        let test = cursor_test(
            r#"
        class Point:
            """A simple point class representing a 2D coordinate."""

            def __init__(self, x: int, y: int):
                """Initialize a point with x and y coordinates.

                Args:
                    x: The x-coordinate
                    y: The y-coordinate
                """
                self.x = x
                self.y = y

        point = Point(<CURSOR>
        "#,
        );

        let result = test.signature_help().expect("Should have signature help");

        // Should have exactly one signature for the constructor
        assert_eq!(result.signatures.len(), 1);
        let signature = &result.signatures[0];

        // Validate the constructor signature
        assert_eq!(signature.label, "(x: int, y: int) -> Point");
        assert_eq!(signature.parameters.len(), 2);

        // Validate the first parameter (x: int)
        let param_x = &signature.parameters[0];
        assert_eq!(param_x.label, "x: int");
        assert_eq!(param_x.name, "x");
        assert_eq!(param_x.documentation, Some("The x-coordinate".to_string()));

        // Validate the second parameter (y: int)
        let param_y = &signature.parameters[1];
        assert_eq!(param_y.label, "y: int");
        assert_eq!(param_y.name, "y");
        assert_eq!(param_y.documentation, Some("The y-coordinate".to_string()));

        // Should have the __init__ method docstring as documentation (not the class docstring)
        let expected_docstring = "Initialize a point with x and y coordinates.\n\nArgs:\n    x: The x-coordinate\n    y: The y-coordinate\n";
        assert_eq!(
            signature
                .documentation
                .as_ref()
                .map(Docstring::render_plaintext),
            Some(expected_docstring.to_string())
        );
    }

    #[test]
    fn signature_help_callable_object() {
        let test = cursor_test(
            r#"
        class Multiplier:
            def __call__(self, x: int) -> int:
                return x * 2

        multiplier = Multiplier()
        result = multiplier(<CURSOR>
        "#,
        );

        let result = test.signature_help().expect("Should have signature help");

        // Should have a signature for the callable object
        assert!(!result.signatures.is_empty());
        let signature = &result.signatures[0];

        // Should provide signature help for the callable
        assert!(signature.label.contains("int") || signature.label.contains("->"));
    }

    #[test]
    fn signature_help_subclass_of_constructor() {
        let test = cursor_test(
            r#"
        from typing import Type

        def create_instance(cls: Type[list]) -> list:
            return cls(<CURSOR>
        "#,
        );

        let result = test.signature_help().expect("Should have signature help");

        // Should have a signature
        assert!(!result.signatures.is_empty());
        let signature = &result.signatures[0];

        // Should have empty documentation for now
        assert_eq!(
            signature
                .documentation
                .as_ref()
                .map(Docstring::render_plaintext),
            None
        );
    }

    #[test]
    fn signature_help_parameter_label_offsets() {
        let test = cursor_test(
            r#"
        def test_function(param1: str, param2: int, param3: bool) -> str:
            return f"{param1}: {param2}, {param3}"

        result = test_function(<CURSOR>
        "#,
        );

        let result = test.signature_help().expect("Should have signature help");
        assert_eq!(result.signatures.len(), 1);

        let signature = &result.signatures[0];
        assert_eq!(signature.parameters.len(), 3);

        // Check that we have parameter labels
        for (i, param) in signature.parameters.iter().enumerate() {
            let expected_param_spec = match i {
                0 => "param1: str",
                1 => "param2: int",
                2 => "param3: bool",
                _ => panic!("Unexpected parameter index"),
            };
            assert_eq!(param.label, expected_param_spec);
        }
    }

    #[test]
    fn signature_help_active_signature_selection() {
        // This test verifies that the algorithm correctly selects the first signature
        // where all arguments present in the call have valid parameter mappings.
        let test = cursor_test(
            r#"
        from typing import overload

        @overload
        def process(value: int) -> str: ...

        @overload
        def process(value: str, flag: bool) -> int: ...

        def process(value, flag=None):
            if isinstance(value, int):
                return str(value)
            elif flag is not None:
                return len(value) if flag else 0
            else:
                return len(value)

        # Call with two arguments - should select the second overload
        result = process("hello", True<CURSOR>)
        "#,
        );

        let result = test.signature_help().expect("Should have signature help");

        // Should have signatures for the overloads.
        assert!(!result.signatures.is_empty());

        // Check that we have an active signature and parameter
        if let Some(active_sig_index) = result.active_signature {
            let active_signature = &result.signatures[active_sig_index];
            assert_eq!(active_signature.active_parameter, Some(1));
        }
    }

    #[test]
    fn signature_help_parameter_documentation() {
        let test = cursor_test(
            r#"
        def documented_function(param1: str, param2: int) -> str:
            """This is a function with parameter documentation.

            Args:
                param1: The first parameter description
                param2: The second parameter description
            """
            return f"{param1}: {param2}"

        result = documented_function(<CURSOR>
        "#,
        );

        let result = test.signature_help().expect("Should have signature help");
        assert_eq!(result.signatures.len(), 1);

        let signature = &result.signatures[0];
        assert_eq!(signature.parameters.len(), 2);

        // Check that parameter documentation is extracted
        let param1 = &signature.parameters[0];
        assert_eq!(
            param1.documentation,
            Some("The first parameter description".to_string())
        );

        let param2 = &signature.parameters[1];
        assert_eq!(
            param2.documentation,
            Some("The second parameter description".to_string())
        );
    }

    #[test]
    fn signature_help_after_closing_paren() {
        let test = cursor_test(
            r#"
        def func1(v: str) -> str:
            return v

        r = func1("")<CURSOR>
        print(r)
        "#,
        );

        let result = test.signature_help();
        assert!(
            result.is_none(),
            "Signature help should return None after closing paren"
        );
    }

    #[test]
    fn signature_help_after_nested_closing_paren() {
        let test = cursor_test(
            r#"
        def inner_func(x: str) -> str:
            return x.upper()

        def outer_func(a: int, b: str) -> str:
            return f"{a}: {b}"

        result = outer_func(42, inner_func("hello")<CURSOR>
        "#,
        );

        // Should return signature help for the outer function call
        // even though cursor is after the closing paren of the inner call
        let result = test
            .signature_help()
            .expect("Should have signature help for outer function");
        assert_eq!(result.signatures.len(), 1);

        let signature = &result.signatures[0];
        assert!(signature.label.contains("a: int") && signature.label.contains("b: str"));

        // Should be on the second parameter (b: str) since we're after the inner call
        assert_eq!(signature.active_parameter, Some(1));
        assert_eq!(result.active_signature, Some(0));
    }

    #[test]
    fn signature_help_after_closing_paren_at_end_of_file() {
        let test = cursor_test(
            r#"
            def test(a: int) -> int:
                return 10

            test("test")<CURSOR>"#,
        );

        // Should not return a signature help
        assert_eq!(test.signature_help(), None);
    }

    #[test]
    fn signature_help_after_closing_paren_in_expression() {
        let test = cursor_test(
            r#"
            def test(a: int) -> int:
                return 10

            test("test")<CURSOR> + 10
        "#,
        );

        // Should not return a signature help
        assert_eq!(test.signature_help(), None);
    }

    #[test]
    fn signature_help_after_closing_paren_nested() {
        let test = cursor_test(
            r#"
        def inner(a: int) -> int:
            return 10

        def outer(a: int) -> None: ...

        outer(inner("test")<CURSOR> + 10)
        "#,
        );

        // Should return the outer signature help
        let help = test.signature_help().expect("Should have outer help");

        assert_eq!(help.signatures.len(), 1);

        let signature = &help.signatures[0];
        assert_eq!(signature.label, "(a: int) -> None");
    }

    #[test]
    fn signature_help_stub_to_implementation_mapping() {
        // Test that when a function is called from a stub file with no docstring,
        // the signature help includes the docstring from the corresponding implementation file
        let test = CursorTest::builder()
            .source(
                "main.py",
                r#"
                from lib import func
                result = func(<CURSOR>
"#,
            )
            .source(
                "lib.pyi",
                r#"
                def func() -> str: ...
"#,
            )
            .source(
                "lib.py",
                r#"
                def func() -> str:
                    """This function does something."""
                    return ""
"#,
            )
            .build();

        let result = test.signature_help().expect("Should have signature help");
        assert_eq!(result.signatures.len(), 1);

        let signature = &result.signatures[0];
        assert_eq!(signature.label, "() -> str");

        let expected_docstring = "This function does something.\n";
        assert_eq!(
            signature
                .documentation
                .as_ref()
                .map(Docstring::render_plaintext),
            Some(expected_docstring.to_string())
        );
    }

    impl CursorTest {
        fn signature_help(&self) -> Option<SignatureHelpInfo<'_>> {
            crate::signature_help::signature_help(&self.db, self.cursor.file, self.cursor.offset)
        }

        fn signature_help_render(&self) -> String {
            use std::fmt::Write;

            let Some(signature_help) = self.signature_help() else {
                return "Signature help found no signatures".to_string();
            };
            let active_sig_heading = "\n============== active signature =============\n";
            let second_sig_heading = "\n=============== other signature =============\n";
            let active_arg_heading = "\n-------------- active parameter -------------\n";

            let mut buf = String::new();
            if let Some(active_signature) = signature_help.active_signature {
                let signature = signature_help
                    .signatures
                    .get(active_signature)
                    .expect("failed to find active signature!");
                write!(
                    &mut buf,
                    "{heading}{label}{line}{docs}",
                    heading = active_sig_heading,
                    label = signature.label,
                    line = MarkupKind::PlainText.horizontal_line(),
                    docs = signature
                        .documentation
                        .as_ref()
                        .map(Docstring::render_plaintext)
                        .unwrap_or_default(),
                )
                .unwrap();
                if let Some(active_parameter) = signature.active_parameter {
                    let parameter = signature
                        .parameters
                        .get(active_parameter)
                        .expect("failed to find active parameter!");
                    write!(
                        &mut buf,
                        "{heading}{label}{line}{docs}",
                        heading = active_arg_heading,
                        label = parameter.label,
                        line = MarkupKind::PlainText.horizontal_line(),
                        docs = parameter.documentation.as_deref().unwrap_or_default(),
                    )
                    .unwrap();
                } else {
                    writeln!(&mut buf, "\n(no active parameter specified)").unwrap();
                }
            } else {
                writeln!(&mut buf, "\n(no active signature specified)").unwrap();
            }

            for (idx, signature) in signature_help.signatures.iter().enumerate() {
                if Some(idx) == signature_help.active_signature {
                    continue;
                }
                write!(
                    &mut buf,
                    "{heading}{label}{line}{docs}",
                    heading = second_sig_heading,
                    label = signature.label,
                    line = MarkupKind::PlainText.horizontal_line(),
                    docs = signature
                        .documentation
                        .as_ref()
                        .map(Docstring::render_plaintext)
                        .unwrap_or_default(),
                )
                .unwrap();
                if let Some(active_parameter) = signature.active_parameter {
                    let parameter = signature
                        .parameters
                        .get(active_parameter)
                        .expect("failed to find active parameter!");
                    write!(
                        &mut buf,
                        "{heading}{label}{line}{docs}",
                        heading = active_arg_heading,
                        label = parameter.label,
                        line = MarkupKind::PlainText.horizontal_line(),
                        docs = parameter.documentation.as_deref().unwrap_or_default(),
                    )
                    .unwrap();
                } else {
                    write!(&mut buf, "\n(no active parameter specified)").unwrap();
                }
            }

            buf
        }
    }
}
