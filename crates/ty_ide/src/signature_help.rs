use crate::{Db, docstring::get_parameter_documentation, find_node::covering_node};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_python_semantic::semantic_index::definition::Definition;
use ty_python_semantic::types::{
    CallSignatureDetails, ParameterLabelOffset, call_signature_details,
    get_docstring_for_definition,
};

// This module handles the "signature help" request in the language server
// protocol. This request is typically issued by a client when the user types
// an open parenthesis and starts to enter arguments for a function call.
// The signature help provides information that the editor displays to the
// user about the target function signature including parameter names,
// types, and documentation. It supports multiple signatures for union types
// and overloads.

// Limitations of the current implementation:

// TODO - If the target function is declared in a stub file but defined (implemented)
// in a source file, the documentation will not reflect the a docstring that appears
// only in the implementation. To do this, we'll need to map the function or
// method in the stub to the implementation and extract the docstring from there.

/// Information about a function parameter.
#[derive(Debug, Clone)]
pub struct ParameterInfo {
    /// A simple string label for the parameter including its name and type
    pub label: ParameterLabel,
    /// Documentation specific to the parameter, typically extracted from the
    /// function's docstring
    pub documentation: Option<String>,
}

/// Label information for a parameter, supporting both string labels and offset-based labels.
#[derive(Debug, Clone)]
pub enum ParameterLabel {
    /// A simple string label for the parameter including its name and type
    String(String),
    /// An offset-based label indicating the parameter's range within the
    /// full signature string
    Offset { start: usize, length: usize },
}

/// Information about a function signature.
#[derive(Debug, Clone)]
pub struct SignatureInfo {
    /// Text representation of the full signature (including input parameters and return type).
    pub label: String,
    /// Documentation for the signature, typically from the function's docstring.
    pub documentation: Option<String>,
    /// Information about each of the parameters in left-to-right order.
    pub parameters: Vec<ParameterInfo>,
    /// Index of the parameter that corresponds to the argument where the
    /// user's cursor is currently positioned.
    pub active_parameter: Option<usize>,
}

/// Signature help information for function calls.
#[derive(Debug, Clone)]
pub struct SignatureHelpInfo {
    /// Information about each of the signatures for the function call. We
    /// need to handle multiple because of unions, overloads, and composite
    /// calls like constructors (which invoke both __new__ and __init__).
    pub signatures: Vec<SignatureInfo>,
    /// Index of the "active signature" which is the first signature where
    /// all arguments that are currently present in the code map to parameters.
    pub active_signature: Option<usize>,
    /// Index of the active parameter within the active signature.
    pub active_parameter: Option<usize>,
}

/// Client capabilities for signature help feature.
#[derive(Debug, Clone, Default)]
pub struct SignatureHelpClientCapabilities {
    /// Does the client support signature label offsets?
    pub signature_label_offset_support: bool,

    /// Does the client support per-signature active parameter?
    pub active_parameter_support: bool,
}

/// Provides signature help information for function calls at the given position.
pub fn signature_help(
    db: &dyn Db,
    file: File,
    offset: TextSize,
    client_capabilities: &SignatureHelpClientCapabilities,
) -> Option<SignatureHelpInfo> {
    let parsed = parsed_module(db, file).load(db);

    // Get the call expression at the given position.
    let (call_expr, current_arg_index) = get_call_expr(&parsed, offset)?;

    // Get signature details from the semantic analyzer.
    let signature_details: Vec<CallSignatureDetails<'_>> =
        call_signature_details(db, file, call_expr);

    if signature_details.is_empty() {
        return None;
    }

    // Find the active signature - the first signature where all arguments map to parameters.
    let active_signature_index = find_active_signature_from_details(&signature_details);

    // Convert to SignatureInfo objects.
    let signatures: Vec<SignatureInfo> = signature_details
        .into_iter()
        .map(|details| {
            create_signature_info_from_details(db, &details, current_arg_index, client_capabilities)
        })
        .collect();

    // Get the active parameter for the active signature.
    let active_parameter_index = active_signature_index
        .and_then(|index| signatures.get(index))
        .and_then(|signature| signature.active_parameter);

    Some(SignatureHelpInfo {
        signatures,
        active_signature: active_signature_index,
        active_parameter: active_parameter_index,
    })
}

/// Returns the innermost call expression that contains the specified offset
/// and the index of the argument that the offset maps to.
fn get_call_expr(
    parsed: &ruff_db::parsed::ParsedModuleRef,
    offset: TextSize,
) -> Option<(&ast::ExprCall, usize)> {
    // Create a range from the offset for the covering_node function.
    let range = TextRange::new(offset, offset);

    // Find the covering node at the given position that is a function call.
    let covering_node = covering_node(parsed.syntax().into(), range)
        .find_first(|node| matches!(node, AnyNodeRef::ExprCall(_)))
        .ok()?;

    // Get the function call expression.
    let AnyNodeRef::ExprCall(call_expr) = covering_node.node() else {
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

    // Check positional arguments.
    for (i, arg) in call_expr.arguments.args.iter().enumerate() {
        if offset <= arg.end() {
            return i;
        }
        current_arg = i + 1;
    }

    // Check keyword arguments.
    for (i, keyword) in call_expr.arguments.keywords.iter().enumerate() {
        if offset <= keyword.end() {
            return current_arg + i;
        }
    }

    current_arg
}

/// Determine appropriate documentation for a callable type based on its original type.
fn get_callable_documentation(db: &dyn crate::Db, definition: Option<Definition>) -> String {
    // TODO: If the definition is located within a stub file and no docstring
    // is present, try to map the symbol to an implementation file and extract
    // the docstring from that location.
    if let Some(definition) = definition {
        get_docstring_for_definition(db, definition).unwrap_or_default()
    } else {
        String::new()
    }
}

/// Create signature information from `CallSignatureDetails`.
fn create_signature_info_from_details(
    db: &dyn crate::Db,
    details: &CallSignatureDetails,
    current_arg_index: usize,
    client_capabilities: &SignatureHelpClientCapabilities,
) -> SignatureInfo {
    let signature_label = details.label.clone();

    let documentation = get_callable_documentation(db, details.definition);

    // Translate the argument index to parameter index using the mapping.
    let active_parameter = if client_capabilities.active_parameter_support {
        // If there are no arguments yet, but we have a current argument index,
        // use that as the active parameter.
        if details.argument_to_parameter_mapping.is_empty() && current_arg_index == 0 {
            Some(0)
        } else {
            details
                .argument_to_parameter_mapping
                .get(current_arg_index)
                .and_then(|&param_index| param_index)
                .or({
                    // If we can't find a mapping for this argument, but we have a current
                    // argument index, use that as the active parameter if it's within bounds.
                    if current_arg_index < details.parameter_label_offsets.len() {
                        Some(current_arg_index)
                    } else {
                        None
                    }
                })
        }
    } else {
        None
    };

    SignatureInfo {
        label: signature_label.clone(),
        documentation: Some(documentation),
        parameters: create_parameters_from_offsets(
            &details.parameter_label_offsets,
            &signature_label,
            client_capabilities,
            details.definition,
            db,
            &details.parameter_names,
        ),
        active_parameter,
    }
}

/// Create `ParameterInfo` objects from parameter label offsets.
fn create_parameters_from_offsets(
    parameter_offsets: &[ParameterLabelOffset],
    signature_label: &str,
    client_capabilities: &SignatureHelpClientCapabilities,
    definition: Option<Definition>,
    db: &dyn crate::Db,
    parameter_names: &[String],
) -> Vec<ParameterInfo> {
    // Extract parameter documentation from the function's docstring if available.
    let param_docs = if let Some(definition) = definition {
        let docstring = get_docstring_for_definition(db, definition);
        docstring
            .map(|doc| get_parameter_documentation(&doc))
            .unwrap_or_default()
    } else {
        std::collections::HashMap::new()
    };

    parameter_offsets
        .iter()
        .enumerate()
        .map(|(i, offset)| {
            let label = if client_capabilities.signature_label_offset_support {
                // Use offset-based label when supported by the client.
                ParameterLabel::Offset {
                    start: offset.start,
                    length: offset.length,
                }
            } else {
                // Extract the parameter name from the signature string.
                let param_name = signature_label
                    .get(offset.start..offset.start + offset.length)
                    .unwrap_or("unknown")
                    .to_string();
                ParameterLabel::String(param_name)
            };

            // Get the parameter name for documentation lookup.
            let param_name = parameter_names.get(i).map(String::as_str).unwrap_or("");

            ParameterInfo {
                label,
                documentation: param_docs.get(param_name).cloned(),
            }
        })
        .collect()
}

/// Find the active signature index from `CallSignatureDetails`.
/// The active signature is the first signature where all arguments present in the call
/// have valid mappings to parameters (i.e., none of the mappings are None).
fn find_active_signature_from_details(signature_details: &[CallSignatureDetails]) -> Option<usize> {
    if signature_details.is_empty() {
        return None;
    }

    // If there are no arguments in the mapping, just return the first signature.
    if signature_details[0]
        .argument_to_parameter_mapping
        .is_empty()
    {
        return Some(0);
    }

    // First, try to find a signature where all arguments have valid parameter mappings.
    let perfect_match = signature_details.iter().position(|details| {
        // Check if all arguments have valid parameter mappings (i.e., are not None).
        details
            .argument_to_parameter_mapping
            .iter()
            .all(Option::is_some)
    });

    if let Some(index) = perfect_match {
        return Some(index);
    }

    // If no perfect match, find the signature with the most valid argument mappings.
    let (best_index, _) = signature_details
        .iter()
        .enumerate()
        .max_by_key(|(_, details)| {
            details
                .argument_to_parameter_mapping
                .iter()
                .filter(|mapping| mapping.is_some())
                .count()
        })?;

    Some(best_index)
}

#[cfg(test)]
mod tests {
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
            "            \n",
            "            Args:\n",
            "                param1: The first parameter as a string\n",
            "                param2: The second parameter as an integer\n",
            "            \n",
            "            Returns:\n",
            "                A formatted string combining both parameters\n",
            "            "
        );
        assert_eq!(
            signature.documentation,
            Some(expected_docstring.to_string())
        );

        assert_eq!(result.active_parameter, Some(0));
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
        assert_eq!(result.active_parameter, Some(0));
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
        assert_eq!(result.active_parameter, Some(0));
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
        match &param.label {
            crate::signature_help::ParameterLabel::Offset { start, length } => {
                let param_spec = &signature.label[*start..*start + *length];
                assert_eq!(param_spec, "x: int");
            }
            crate::signature_help::ParameterLabel::String(name) => {
                assert_eq!(name, "x: int");
            }
        }

        // Validate the second signature (from func_b)
        let signature_b = &result.signatures[1];
        assert_eq!(signature_b.label, "(y: str) -> str");
        assert_eq!(signature_b.parameters.len(), 1);

        // Check parameter information for the second signature
        let param_b = &signature_b.parameters[0];
        match &param_b.label {
            crate::signature_help::ParameterLabel::Offset { start, length } => {
                let param_spec = &signature_b.label[*start..*start + *length];
                assert_eq!(param_spec, "y: str");
            }
            crate::signature_help::ParameterLabel::String(name) => {
                assert_eq!(name, "y: str");
            }
        }

        assert_eq!(result.active_signature, Some(0));
        assert_eq!(result.active_parameter, Some(0));
    }

    #[test]
    fn signature_help_client_capabilities() {
        use crate::signature_help::SignatureHelpClientCapabilities;

        let test = cursor_test(
            r#"
        def test_function(param1: str, param2: int) -> str:
            return f"{param1}: {param2}"

        result = test_function(<CURSOR>
        "#,
        );

        // Test with active parameter support disabled
        let client_capabilities_no_active = SignatureHelpClientCapabilities {
            signature_label_offset_support: false,
            active_parameter_support: false,
        };
        let result_no_active = crate::signature_help::signature_help(
            &test.db,
            test.cursor.file,
            test.cursor.offset,
            &client_capabilities_no_active,
        )
        .expect("Should have signature help");

        // Should not have per-signature active parameter
        assert_eq!(result_no_active.signatures.len(), 1);
        let signature = &result_no_active.signatures[0];
        assert_eq!(signature.active_parameter, None);

        // Test with active parameter support enabled
        let client_capabilities_with_active = SignatureHelpClientCapabilities {
            signature_label_offset_support: false,
            active_parameter_support: true,
        };
        let result_with_active = crate::signature_help::signature_help(
            &test.db,
            test.cursor.file,
            test.cursor.offset,
            &client_capabilities_with_active,
        )
        .expect("Should have signature help");

        // Should have per-signature active parameter
        assert_eq!(result_with_active.signatures.len(), 1);
        let signature = &result_with_active.signatures[0];
        assert_eq!(signature.active_parameter, Some(0));
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
        assert_eq!(result.active_parameter, Some(0));

        // Validate the first overload: process(value: int) -> str
        let signature1 = &result.signatures[0];
        assert_eq!(signature1.label, "(value: int) -> str");
        assert_eq!(signature1.parameters.len(), 1);

        let param1 = &signature1.parameters[0];
        match &param1.label {
            crate::signature_help::ParameterLabel::Offset { start, length } => {
                let param_spec = &signature1.label[*start..*start + *length];
                assert_eq!(param_spec, "value: int");
            }
            crate::signature_help::ParameterLabel::String(name) => {
                assert_eq!(name, "value: int");
            }
        }

        // Validate the second overload: process(value: str) -> int
        let signature2 = &result.signatures[1];
        assert_eq!(signature2.label, "(value: str) -> int");
        assert_eq!(signature2.parameters.len(), 1);

        let param2 = &signature2.parameters[0];
        match &param2.label {
            crate::signature_help::ParameterLabel::Offset { start, length } => {
                let param_spec = &signature2.label[*start..*start + *length];
                assert_eq!(param_spec, "value: str");
            }
            crate::signature_help::ParameterLabel::String(name) => {
                assert_eq!(name, "value: str");
            }
        }
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
        match &param_x.label {
            crate::signature_help::ParameterLabel::Offset { start, length } => {
                let param_spec = &signature.label[*start..*start + *length];
                assert_eq!(param_spec, "x: int");
            }
            crate::signature_help::ParameterLabel::String(name) => {
                assert_eq!(name, "x: int");
            }
        }
        assert_eq!(param_x.documentation, Some("The x-coordinate".to_string()));

        // Validate the second parameter (y: int)
        let param_y = &signature.parameters[1];
        match &param_y.label {
            crate::signature_help::ParameterLabel::Offset { start, length } => {
                let param_spec = &signature.label[*start..*start + *length];
                assert_eq!(param_spec, "y: int");
            }
            crate::signature_help::ParameterLabel::String(name) => {
                assert_eq!(name, "y: int");
            }
        }
        assert_eq!(param_y.documentation, Some("The y-coordinate".to_string()));

        // Should have the __init__ method docstring as documentation (not the class docstring)
        let expected_docstring = "Initialize a point with x and y coordinates.\n                \n                Args:\n                    x: The x-coordinate\n                    y: The y-coordinate\n                ";
        assert_eq!(
            signature.documentation,
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
        assert_eq!(signature.documentation, Some(String::new()));
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

        // Check that we have offset-based labels when offset support is enabled
        for (i, param) in signature.parameters.iter().enumerate() {
            let expected_param_spec = match i {
                0 => "param1: str",
                1 => "param2: int",
                2 => "param3: bool",
                _ => panic!("Unexpected parameter index"),
            };
            match &param.label {
                crate::signature_help::ParameterLabel::Offset { start, length } => {
                    // Verify that the offset points to a valid position in the signature
                    assert!(*start < signature.label.len());
                    assert!(*start + *length <= signature.label.len());

                    // Extract the parameter specification using the offset
                    let param_spec = &signature.label[*start..*start + *length];
                    assert_eq!(param_spec, expected_param_spec);
                }
                crate::signature_help::ParameterLabel::String(name) => {
                    // If using string labels, they should match expected parameter specifications
                    assert_eq!(name, expected_param_spec);
                }
            }
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

        // The second overload should be active because the first doesn't have two parameters.
        assert_eq!(result.active_parameter, Some(1));
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

    impl CursorTest {
        fn signature_help(&self) -> Option<SignatureHelpInfo> {
            use crate::signature_help::SignatureHelpClientCapabilities;
            // Enable all capabilities for testing
            let client_capabilities = SignatureHelpClientCapabilities {
                signature_label_offset_support: true,
                active_parameter_support: true,
            };
            crate::signature_help::signature_help(
                &self.db,
                self.cursor.file,
                self.cursor.offset,
                &client_capabilities,
            )
        }
    }
}
