//! Stub content generation for synthesized types.
//!
//! This module generates Python stub file content (.pyi format) for types that
//! don't have source code declarations, such as:
//! - Dataclass-generated methods (__init__, __eq__, etc.)
//! - `NamedTuple` synthesized members (_make, _asdict, _fields, etc.)
//! - Built-in functions and types
//! - Synthesized protocols
//! - `TypedDict` types
//! - `NewType` declarations
//! - Generic specializations
//!
//! The approach is pragmatic: we use the display string and category from
//! `ty_ide`'s `TypeInfo` (which uses ty's Display implementation), then format
//! them as proper Python stubs. This avoids depending on ty's internal
//! Type<'db> representation.

use std::fmt::Write;

use ty_ide::TypeCategory;

/// Generate stub content for a synthesized type.
///
/// Takes a `TypeCategory` and display string (from `ty_ide::TypeInfo`) and
/// produces stub content if the type needs it.
///
/// Returns:
/// - The stub content as a string (Python .pyi format)
/// - The module name parts (e.g., `["typing", "Protocol"]`)
/// - The primary definition offset (character offset to the main definition)
///
/// Returns None if the type doesn't need stub generation.
pub(crate) fn generate_stub(
    category: TypeCategory,
    display: &str,
) -> Option<(String, Vec<String>, usize)> {
    // Check if this category needs synthesized stubs
    if !needs_stub(category) {
        return None;
    }

    // Generate appropriate stub based on category
    match category {
        TypeCategory::Function | TypeCategory::BoundMethod | TypeCategory::Callable => {
            generate_function_stub(display)
        }
        TypeCategory::Instance => Some(generate_class_instance_stub(display)),
        TypeCategory::Class | TypeCategory::SubclassOf => Some(generate_class_stub(display)),
        TypeCategory::NewType => Some(generate_newtype_stub(display)),
        TypeCategory::TypedDict => Some(generate_typeddict_stub(display)),
        TypeCategory::Literal => Some(generate_annotation_stub(display)),
        // For other categories that need stubs, try a generic approach
        _ => None,
    }
}

/// Check if a type category needs synthesized stubs.
///
/// Types that don't have source-location declarations need synthesized stubs
/// so `PyrightTypeFactory` can reconstruct them.
fn needs_stub(category: TypeCategory) -> bool {
    match category {
        // These categories always need stubs because ty-tsp doesn't (yet)
        // provide source-location declarations for them.
        TypeCategory::Instance | TypeCategory::Class | TypeCategory::SubclassOf => true,

        // Functions may need stubs (built-in functions, etc.)
        TypeCategory::Function | TypeCategory::BoundMethod | TypeCategory::Callable => true,

        // Special types that need stubs
        TypeCategory::NewType | TypeCategory::TypedDict => true,

        // Literal types need annotation-based stubs to preserve literal values
        TypeCategory::Literal => true,

        // These don't need stubs (handled as BuiltIn or other kinds)
        TypeCategory::Unknown
        | TypeCategory::Any
        | TypeCategory::Never
        | TypeCategory::None
        | TypeCategory::Dynamic
        | TypeCategory::Union
        | TypeCategory::Intersection
        | TypeCategory::Module
        | TypeCategory::TypeVar
        | TypeCategory::Tuple
        | TypeCategory::TypeAlias
        | TypeCategory::SpecialForm
        | TypeCategory::Property
        | TypeCategory::TypeGuard
        | TypeCategory::OverloadedFunction => false,
    }
}

/// Generate stub for a function type.
fn generate_function_stub(display: &str) -> Option<(String, Vec<String>, usize)> {
    // ty's display for functions is already in Python format like:
    // "def len(__obj: Sized) -> int"
    // We just need to wrap it as a stub with "..." body

    // Check if this looks like a function signature
    if !display.starts_with("def ") && !display.starts_with("async def ") {
        return None;
    }

    // Extract function name to determine module
    let function_name = extract_function_name(display)?;

    // For now, assume builtins module for common functions
    let module = if is_known_builtin_function(function_name) {
        vec!["builtins".to_string()]
    } else {
        // Unknown module - use empty for now
        vec![]
    };

    // Find type references in the signature and generate class stubs for them.
    // This ensures types like 'A' in `def foo(x: A) -> A` are defined in the stub.
    let preamble = generate_type_preamble(display);

    // Generate stub: add "..." body if not already present
    let func_stub = if display.ends_with("...") {
        display.to_string()
    } else {
        format!("{}: ...", display.trim_end_matches(':').trim())
    };

    let stub = format!("{preamble}{func_stub}");

    // Offset points to "def" keyword (after any preamble)
    let offset = stub.find("def ").unwrap_or(0);

    Some((stub, module, offset))
}

/// Generate class stub preamble for type references in a function signature.
///
/// Extracts type annotation names from the signature and generates minimal
/// `class X: ...` stubs for names that aren't Python builtins, so Pyright
/// can resolve them when evaluating the function stub.
fn generate_type_preamble(display: &str) -> String {
    let type_refs = extract_type_references(display);
    if type_refs.is_empty() {
        return String::new();
    }

    let mut preamble = String::new();
    for name in type_refs {
        let _ = writeln!(preamble, "class {name}: ...");
    }
    preamble
}

/// Extract type reference names from a function signature's annotations.
///
/// Parses `def foo(x: A, y: B) -> C` to find annotation type names.
/// Filters out Python builtin type names since they don't need stubs.
fn extract_type_references(display: &str) -> Vec<String> {
    let mut refs = Vec::new();

    // Extract the part between '(' and ')' for params, and after '->' for return
    let Some(params_start) = display.find('(') else {
        return refs;
    };
    let Some(params_end) = display.rfind(')') else {
        return refs;
    };

    let params_str = &display[params_start + 1..params_end];
    let return_str = display[params_end + 1..]
        .strip_prefix(" -> ")
        .or_else(|| display[params_end + 1..].strip_prefix("-> "));

    // Extract type annotations from parameters
    for param in split_params(params_str) {
        if let Some(colon_pos) = param.find(':') {
            let type_str = param[colon_pos + 1..].trim();
            collect_simple_type_names(type_str, &mut refs);
        }
    }

    // Extract return type
    if let Some(ret) = return_str {
        let ret = ret.trim();
        collect_simple_type_names(ret, &mut refs);
    }

    // Deduplicate
    refs.sort();
    refs.dedup();

    refs
}

/// Split a parameter list string on top-level commas (respecting bracket nesting).
fn split_params(s: &str) -> Vec<&str> {
    let mut params = Vec::new();
    let mut depth = 0;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                params.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }

    let last = s[start..].trim();
    if !last.is_empty() {
        params.push(last);
    }

    params
}

/// Collect simple (non-builtin) type names from a type annotation string.
///
/// For simple names like `A`, adds them. For complex types like `list[A]`,
/// extracts `A`. Skips Python builtins like `int`, `str`, `list`, etc.
fn collect_simple_type_names(type_str: &str, refs: &mut Vec<String>) {
    // Split on '[', ']', ',', '|' to get individual name tokens
    let tokens: Vec<&str> = type_str
        .split(['[', ']', ',', '|', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    for token in tokens {
        // Skip known builtins and typing constructs
        if is_builtin_or_typing_name(token) {
            continue;
        }
        // Skip names starting with '*' (star params)
        if token.starts_with('*') {
            continue;
        }
        // Skip if not a valid identifier
        if !token.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        // Skip if starts with lowercase (likely a parameter name that leaked through)
        // Type names in Python conventionally start uppercase
        if token.starts_with(|c: char| c.is_lowercase()) {
            continue;
        }
        refs.push(token.to_string());
    }
}

/// Check if a name is a Python builtin type or typing construct.
fn is_builtin_or_typing_name(name: &str) -> bool {
    matches!(
        name,
        "int"
            | "str"
            | "float"
            | "bool"
            | "bytes"
            | "None"
            | "none"
            | "list"
            | "dict"
            | "set"
            | "tuple"
            | "frozenset"
            | "object"
            | "type"
            | "complex"
            | "range"
            | "bytearray"
            | "memoryview"
            | "Any"
            | "Never"
            | "Unknown"
            | "NoReturn"
            | "Optional"
            | "Union"
            | "Callable"
            | "Literal"
            | "Sequence"
            | "Mapping"
            | "Iterable"
            | "Iterator"
            | "Generator"
            | "Awaitable"
            | "Coroutine"
            | "AsyncGenerator"
            | "AsyncIterator"
            | "AsyncIterable"
            | "ClassVar"
            | "Final"
            | "TypeVar"
            | "TypeAlias"
            | "Protocol"
            | "TypedDict"
            | "NamedTuple"
            | "Self"
            | "Unpack"
            | "TypeVarTuple"
            | "ParamSpec"
            | "Sized"
            | "Hashable"
            | "Reversible"
            | "SupportsInt"
            | "SupportsFloat"
            | "SupportsComplex"
            | "SupportsBytes"
            | "SupportsAbs"
            | "SupportsRound"
    )
}

/// Collect all type names from an annotation string that should get class stubs.
///
/// Unlike `collect_simple_type_names` (used for function preambles), this includes
/// builtin type names like `int`, `str`, `list`, `dict`, etc. because stub files
/// bound by the `ExternalProgram`'s `BindingService` don't have access to builtinsScope.
///
/// Excludes:
/// - Typing special forms (`Literal`, `Optional`, `type`, etc.) handled via imports
/// - Non-identifier tokens (numbers, operators, etc.)
/// - `None`, `Any`, `Never`, `Unknown` — these are special forms in Pyright
fn collect_annotation_type_names(display: &str) -> Vec<String> {
    let mut refs = Vec::new();

    // Split on delimiters to get individual name tokens
    let tokens: Vec<&str> = display
        .split(['[', ']', ',', '|', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    for token in tokens {
        // Skip if not a valid Python identifier
        if !token.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        // Skip special forms and typing constructs (handled by imports or Pyright internals)
        if is_annotation_special_form(token) {
            continue;
        }
        // Skip numeric literals (e.g., "1" in Literal[1])
        if token.chars().next().is_none_or(|c| c.is_ascii_digit()) {
            continue;
        }
        // Skip `x` itself (the variable name in `x: <type>`)
        if token == "x" {
            continue;
        }
        refs.push(token.to_string());
    }

    // Deduplicate while preserving order
    refs.sort();
    refs.dedup();

    refs
}

/// Check if a name is a special form that should NOT get a class stub in annotations.
///
/// These are typing constructs, special forms, or names that Pyright handles
/// intrinsically — adding `class <name>: ...` would shadow or break them.
fn is_annotation_special_form(name: &str) -> bool {
    matches!(
        name,
        // Special forms handled by Pyright without needing class defs
        "type" | "None" | "True" | "False"
        // Typing module constructs (handled via `from typing import ...`)
        | "Any" | "Never" | "Unknown" | "NoReturn"
        | "Literal" | "Optional" | "Union" | "Callable"
        | "ClassVar" | "Final" | "TypeGuard" | "TypeAlias"
        | "Self" | "Unpack" | "TypeVarTuple" | "ParamSpec"
        | "Generic" | "Protocol" | "TypeVar"
        | "TypedDict" | "NamedTuple"
    )
}

/// Check if a function name is a known built-in.
fn is_known_builtin_function(name: &str) -> bool {
    matches!(
        name,
        "len"
            | "print"
            | "isinstance"
            | "issubclass"
            | "type"
            | "id"
            | "abs"
            | "all"
            | "any"
            | "ascii"
            | "bin"
            | "bool"
            | "breakpoint"
            | "bytearray"
            | "bytes"
            | "callable"
            | "chr"
            | "classmethod"
            | "compile"
            | "complex"
            | "delattr"
            | "dict"
            | "dir"
            | "divmod"
            | "enumerate"
            | "eval"
            | "exec"
            | "filter"
            | "float"
            | "format"
            | "frozenset"
            | "getattr"
            | "globals"
            | "hasattr"
            | "hash"
            | "help"
            | "hex"
            | "input"
            | "int"
            | "iter"
            | "list"
            | "locals"
            | "map"
            | "max"
            | "memoryview"
            | "min"
            | "next"
            | "object"
            | "oct"
            | "open"
            | "ord"
            | "pow"
            | "property"
            | "range"
            | "repr"
            | "reversed"
            | "round"
            | "set"
            | "setattr"
            | "slice"
            | "sorted"
            | "staticmethod"
            | "str"
            | "sum"
            | "super"
            | "tuple"
            | "vars"
            | "zip"
            | "__import__"
    )
}

/// Extract function name from display string.
fn extract_function_name(display: &str) -> Option<&str> {
    // Simple heuristic: look for "def name(" pattern
    if let Some(start) = display.find("def ") {
        let name_start = start + 4;
        if let Some(end) = display[name_start..].find('(') {
            return Some(&display[name_start..name_start + end]);
        }
    }

    // Also handle "async def name(" pattern
    if let Some(start) = display.find("async def ") {
        let name_start = start + 10;
        if let Some(end) = display[name_start..].find('(') {
            return Some(&display[name_start..name_start + end]);
        }
    }

    // Fallback: extract from "<function name>" format
    if display.starts_with("<function ") && display.ends_with('>') {
        let name = &display[10..display.len() - 1];
        return Some(name);
    }

    None
}

/// Extract class name from display string.
fn extract_class_name(display: &str) -> &str {
    // Handle generic syntax like "list[int]" -> "list"
    if let Some(bracket_pos) = display.find('[') {
        return &display[..bracket_pos];
    }

    // Handle "type[ClassName]" syntax
    if display.starts_with("type[") && display.ends_with(']') {
        return &display[5..display.len() - 1];
    }

    // Otherwise use the whole string
    display
}

/// Extract a class name from ty's `<class 'X'>` repr format.
///
/// Returns `Some("X")` for `<class 'X'>`, `None` for other formats.
fn extract_angle_bracket_class_name(display: &str) -> Option<&str> {
    let inner = display.strip_prefix("<class '")?.strip_suffix("'>")?;
    Some(inner)
}

/// Generate an annotation-based stub for types expressible as Python annotations.
///
/// Produces a `TypeAnnotation` statement `x: <type>` that Pyright can evaluate.
/// Used for generic types, literal types, and other types that are better
/// expressed as annotations than as class/function definitions.
///
/// If the display string references `typing` constructs (e.g., `Literal[1]`),
/// the necessary imports are prepended.
fn generate_annotation_stub(display: &str) -> (String, Vec<String>, usize) {
    let mut preamble = String::new();

    // Detect typing imports needed
    let typing_names = detect_typing_imports(display);
    if !typing_names.is_empty() {
        let imports = typing_names.join(", ");
        let _ = writeln!(preamble, "from typing import {imports}");
    }

    // Generate class stubs for all type names referenced in the annotation.
    // This makes the stub self-contained so the evaluator can resolve types
    // without needing builtinsScope (which is unavailable for stub files
    // in the ExternalProgram context).
    let type_names = collect_annotation_type_names(display);
    for name in &type_names {
        let _ = writeln!(preamble, "class {name}: ...");
    }

    let stub = format!("{preamble}x: {display}\n");
    // Offset points to the 'x' variable (after any preamble)
    let offset = preamble.len();

    (stub, vec![], offset)
}

/// Detect typing module names referenced in a display string.
fn detect_typing_imports(display: &str) -> Vec<&str> {
    let typing_names = [
        "Literal",
        "Optional",
        "Union",
        "Callable",
        "ClassVar",
        "Final",
        "TypeGuard",
        "TypeAlias",
    ];

    typing_names
        .iter()
        .filter(|name| display.contains(**name))
        .copied()
        .collect()
}

/// Generate stub for a class instance.
///
/// For simple class names (e.g., `MyClass`), produces a minimal class definition.
/// For generic types (e.g., `list[int]`), produces an annotation-based stub
/// to preserve type arguments.
fn generate_class_instance_stub(display: &str) -> (String, Vec<String>, usize) {
    // For generic types (contain '['), use annotation stub to preserve type args
    if display.contains('[') {
        return generate_annotation_stub(display);
    }

    // Simple class name — generate a minimal class definition
    let class_name = extract_class_name(display);

    let stub = format!("class {class_name}:\n    pass\n");
    let module = vec![];
    let offset = 0;

    (stub, module, offset)
}

/// Generate stub for a class type.
///
/// For `type[X]` where X is a builtin type, uses an annotation-based stub
/// to avoid shadowing the builtin. For other class types, generates a
/// minimal class definition.
fn generate_class_stub(display: &str) -> (String, Vec<String>, usize) {
    // For type[X] syntax, use annotation stub to preserve the exact type
    // (creating `class int: ...` would shadow the builtin)
    if display.starts_with("type[") && display.ends_with(']') {
        return generate_annotation_stub(display);
    }

    // Handle ty's `<class 'X'>` repr format by extracting the class name
    // and generating an annotation stub `x: type[X]`.
    if let Some(class_name) = extract_angle_bracket_class_name(display) {
        let type_display = format!("type[{class_name}]");
        return generate_annotation_stub(&type_display);
    }

    // For other class types, generate simple class stub
    let stub = format!("class {display}:\n    ...");
    let module = vec![];
    let offset = 0;

    (stub, module, offset)
}

/// Generate stub for a `NewType`.
fn generate_newtype_stub(display: &str) -> (String, Vec<String>, usize) {
    // ty's display for NewType might show something like:
    // "UserId" (the NewType name)
    // We'll generate a simple NewType stub

    let stub = format!("{display} = ...");
    let module = vec!["typing".to_string()];
    let offset = 0;

    (stub, module, offset)
}

/// Generate stub for a `TypedDict`.
fn generate_typeddict_stub(display: &str) -> (String, Vec<String>, usize) {
    // ty's display for TypedDict might show the dict structure
    // We'll generate a TypedDict class stub

    let dict_name = extract_class_name(display);

    let stub = format!("class {dict_name}(TypedDict):\n    ...");
    let module = vec!["typing".to_string()];
    let offset = 0;

    (stub, module, offset)
}

/// Generate stub for a protocol.
#[allow(dead_code)]
fn generate_protocol_stub(display: &str) -> (String, Vec<String>, usize) {
    // ty's display for protocols might show the protocol structure
    // We'll generate a Protocol class stub

    let protocol_name = extract_class_name(display);

    let stub = format!("class {protocol_name}(Protocol):\n    ...");
    let module = vec!["typing".to_string()];
    let offset = 0;

    (stub, module, offset)
}

/// Generate stub for a generic specialization (e.g., Box[int], list[str]).
#[allow(dead_code)]
fn generate_generic_specialization_stub(display: &str) -> (String, Vec<String>, usize) {
    // ty's display already shows the specialized form like "list[int]"
    // We can use that directly as a type alias

    let stub = format!("{display} = ...");
    let module = vec![];
    let offset = 0;

    (stub, module, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_function_name() {
        assert_eq!(extract_function_name("def foo(x: int) -> int"), Some("foo"));
        assert_eq!(extract_function_name("<function len>"), Some("len"));
        assert_eq!(extract_function_name("not a function"), None);
    }

    #[test]
    fn test_annotation_stub_for_literal() {
        let result = generate_stub(TypeCategory::Literal, "Literal[1]");
        assert!(result.is_some());
        let (stub, _, offset) = result.unwrap();
        assert!(stub.contains("from typing import Literal"));
        assert!(stub.contains("x: Literal[1]"));
        // No class stubs needed — "1" is a numeric literal, "Literal" is a special form
        assert!(!stub.contains("class "));
        // Offset should point to 'x' after the import line
        assert_eq!(&stub[offset..offset + 1], "x");
    }

    #[test]
    fn test_annotation_stub_for_generic_instance() {
        let result = generate_stub(TypeCategory::Instance, "list[int]");
        assert!(result.is_some());
        let (stub, _, offset) = result.unwrap();
        // Should include class stubs for builtin types
        assert!(stub.contains("class int: ..."));
        assert!(stub.contains("class list: ..."));
        assert!(stub.contains("x: list[int]"));
        assert_eq!(&stub[offset..offset + 1], "x");
    }

    #[test]
    fn test_class_stub_for_type_of_builtin() {
        let result = generate_stub(TypeCategory::Class, "type[int]");
        assert!(result.is_some());
        let (stub, _, offset) = result.unwrap();
        // Should include class stub for 'int' but not 'type' (special form)
        assert!(stub.contains("class int: ..."));
        assert!(!stub.contains("class type:"));
        assert!(stub.contains("x: type[int]"));
        assert_eq!(&stub[offset..offset + 1], "x");
    }

    #[test]
    fn test_class_stub_for_angle_bracket_format() {
        // ty uses `<class 'int'>` repr format for class types
        let result = generate_stub(TypeCategory::Class, "<class 'int'>");
        assert!(result.is_some());
        let (stub, _, offset) = result.unwrap();
        assert!(stub.contains("class int: ..."));
        assert!(stub.contains("x: type[int]"));
        assert_eq!(&stub[offset..offset + 1], "x");
    }

    #[test]
    fn test_function_stub_with_type_preamble() {
        let result = generate_stub(TypeCategory::Function, "def foo(x: A) -> A");
        assert!(result.is_some());
        let (stub, _, offset) = result.unwrap();
        assert!(stub.contains("class A: ..."));
        assert!(stub.contains("def foo(x: A) -> A: ..."));
        // Offset should point to 'def' keyword
        assert_eq!(&stub[offset..offset + 3], "def");
    }

    #[test]
    fn test_function_stub_no_preamble_for_builtins() {
        let result = generate_stub(TypeCategory::Function, "def len(__obj: Sized) -> int");
        assert!(result.is_some());
        let (stub, _, offset) = result.unwrap();
        // Sized and int are builtins, no preamble needed
        assert!(!stub.contains("class "));
        assert!(stub.contains("def len(__obj: Sized) -> int: ..."));
        assert_eq!(&stub[offset..offset + 3], "def");
    }

    #[test]
    fn test_extract_type_references() {
        let refs = extract_type_references("def foo(x: A) -> A");
        assert_eq!(refs, vec!["A"]);

        let refs = extract_type_references("def bar(x: int, y: MyClass) -> None");
        assert_eq!(refs, vec!["MyClass"]);

        let refs = extract_type_references("def baz(x: list[int]) -> str");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_simple_class_instance_stub() {
        let result = generate_stub(TypeCategory::Instance, "MyClass");
        assert!(result.is_some());
        let (stub, _, _) = result.unwrap();
        assert!(stub.contains("class MyClass:"));
    }
}
