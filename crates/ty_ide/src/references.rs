//! This module implements the core functionality of the "references" and
//! "rename" language server features. It locates all references to a named
//! symbol. Unlike a simple text search for the symbol's name, this is
//! a "semantic search" where the text and the semantic meaning must match.
//!
//! Some symbols (such as parameters and local variables) are visible only
//! within their scope. All other symbols, such as those defined at the global
//! scope or within classes, are visible outside of the module. Finding
//! all references to these externally-visible symbols therefore requires
//! an expensive search of all source files in the workspace.

use crate::find_node::CoveringNode;
use crate::goto::{GotoTarget, find_goto_target};
use crate::{Db, NavigationTarget, NavigationTargets, RangedValue};
use ruff_db::files::{File, FileRange};
use ruff_python_ast::{
    self as ast, AnyNodeRef,
    visitor::source_order::{SourceOrderVisitor, TraversalSignal},
};
use ruff_text_size::{Ranged, TextSize};

/// Find all references to a symbol at the given position.
/// Search for references across all files in `project_files`.
/// To search only within the current file, pass an empty iterator.
pub fn references(
    db: &dyn Db,
    file: File,
    offset: TextSize,
    include_declaration: bool,
    project_files: impl IntoIterator<Item = File>,
) -> Option<Vec<RangedValue<NavigationTargets>>> {
    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);

    // Get the definitions for the symbol at the cursor position
    let goto_target = find_goto_target(&module, offset)?;
    let target_definitions = goto_target.get_definition_targets(file, db, None)?;

    // Extract the target text from the goto target for fast comparison
    let target_text = goto_target.to_string()?;

    // Find all of the references to the symbol within this file
    let mut references = Vec::new();
    references_for_file(
        db,
        file,
        &target_definitions,
        include_declaration,
        &target_text,
        &mut references,
    );

    // Check if the symbol is potentially visible outside of this module
    if is_symbol_externally_visible(&goto_target) {
        // Look for references in all other files within the workspace
        for other_file in project_files {
            // Skip the current file as we already processed it
            if other_file == file {
                continue;
            }

            // First do a simple text search to see if there is a potential match in the file
            let source = ruff_db::source::source_text(db, other_file);
            if !source.as_str().contains(target_text.as_ref()) {
                continue;
            }

            // If the target text is found, do the more expensive semantic analysis
            references_for_file(
                db,
                other_file,
                &target_definitions,
                false, // Don't include declarations from other files
                &target_text,
                &mut references,
            );
        }
    }

    if references.is_empty() {
        None
    } else {
        Some(references)
    }
}

/// Find all references to a local symbol within the current file. If
/// `include_declaration` is true, return the original declaration for symbols
/// such as functions or classes that have a single declaration location.
fn references_for_file(
    db: &dyn Db,
    file: File,
    target_definitions: &NavigationTargets,
    include_declaration: bool,
    target_text: &str,
    references: &mut Vec<RangedValue<NavigationTargets>>,
) {
    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);

    let mut finder = LocalReferencesFinder {
        db,
        file,
        target_definitions,
        references,
        include_declaration,
        target_text,
        ancestors: Vec::new(),
    };

    AnyNodeRef::from(module.syntax()).visit_source_order(&mut finder);
}

/// Determines whether a symbol is potentially visible outside of the current module.
fn is_symbol_externally_visible(goto_target: &GotoTarget<'_>) -> bool {
    match goto_target {
        GotoTarget::Parameter(_)
        | GotoTarget::ExceptVariable(_)
        | GotoTarget::TypeParamTypeVarName(_)
        | GotoTarget::TypeParamParamSpecName(_)
        | GotoTarget::TypeParamTypeVarTupleName(_) => false,

        // Assume all other goto target types are potentially visible.

        // TODO: For local variables, we should be able to return false
        // except in cases where the variable is in the global scope
        // or uses a "global" binding.
        _ => true,
    }
}

/// AST visitor to find all references to a specific symbol by comparing semantic definitions
struct LocalReferencesFinder<'a> {
    db: &'a dyn Db,
    file: File,
    target_definitions: &'a NavigationTargets,
    references: &'a mut Vec<RangedValue<NavigationTargets>>,
    include_declaration: bool,
    target_text: &'a str,
    ancestors: Vec<AnyNodeRef<'a>>,
}

impl<'a> SourceOrderVisitor<'a> for LocalReferencesFinder<'a> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        self.ancestors.push(node);

        match node {
            AnyNodeRef::ExprName(name_expr) => {
                // If the name doesn't match our target text, this isn't a match
                if name_expr.id.as_str() != self.target_text {
                    return TraversalSignal::Traverse;
                }

                let covering_node = CoveringNode::from_ancestors(self.ancestors.clone());
                self.check_reference_from_covering_node(&covering_node);
            }
            AnyNodeRef::ExprAttribute(attr_expr) => {
                self.check_identifier_reference(&attr_expr.attr);
            }
            AnyNodeRef::StmtFunctionDef(func) if self.include_declaration => {
                self.check_identifier_reference(&func.name);
            }
            AnyNodeRef::StmtClassDef(class) if self.include_declaration => {
                self.check_identifier_reference(&class.name);
            }
            AnyNodeRef::Parameter(parameter) if self.include_declaration => {
                self.check_identifier_reference(&parameter.name);
            }
            AnyNodeRef::Keyword(keyword) => {
                if let Some(arg) = &keyword.arg {
                    self.check_identifier_reference(arg);
                }
            }
            AnyNodeRef::StmtGlobal(global_stmt) if self.include_declaration => {
                for name in &global_stmt.names {
                    self.check_identifier_reference(name);
                }
            }
            AnyNodeRef::StmtNonlocal(nonlocal_stmt) if self.include_declaration => {
                for name in &nonlocal_stmt.names {
                    self.check_identifier_reference(name);
                }
            }
            AnyNodeRef::ExceptHandlerExceptHandler(handler) if self.include_declaration => {
                if let Some(name) = &handler.name {
                    self.check_identifier_reference(name);
                }
            }
            AnyNodeRef::PatternMatchAs(pattern_as) if self.include_declaration => {
                if let Some(name) = &pattern_as.name {
                    self.check_identifier_reference(name);
                }
            }
            AnyNodeRef::PatternMatchMapping(pattern_mapping) if self.include_declaration => {
                if let Some(rest_name) = &pattern_mapping.rest {
                    self.check_identifier_reference(rest_name);
                }
            }
            _ => {}
        }

        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, node: AnyNodeRef<'a>) {
        debug_assert_eq!(self.ancestors.last(), Some(&node));
        self.ancestors.pop();
    }
}

impl LocalReferencesFinder<'_> {
    /// Helper method to check identifier references for declarations
    fn check_identifier_reference(&mut self, identifier: &ast::Identifier) {
        // Quick text-based check first
        if identifier.id != self.target_text {
            return;
        }

        let mut ancestors_with_identifier = self.ancestors.clone();
        ancestors_with_identifier.push(AnyNodeRef::from(identifier));
        let covering_node = CoveringNode::from_ancestors(ancestors_with_identifier);
        self.check_reference_from_covering_node(&covering_node);
    }

    /// Determines whether the given covering node is a reference to
    /// the symbol we are searching for
    fn check_reference_from_covering_node(
        &mut self,
        covering_node: &crate::find_node::CoveringNode<'_>,
    ) {
        // Use the start of the covering node as the offset. Any offset within
        // the node is fine here. Offsets matter only for import statements
        // where the identifier might be a multi-part module name.
        let offset = covering_node.node().range().start();

        if let Some(goto_target) = GotoTarget::from_covering_node(covering_node, offset) {
            // Use the range of the covering node (the identifier) rather than the goto target
            // This ensures we highlight just the identifier, not the entire expression
            let range = covering_node.node().range();

            // Get the definitions for this goto target
            if let Some(current_definitions) =
                goto_target.get_definition_targets(self.file, self.db, None)
            {
                // Check if any of the current definitions match our target definitions
                if self.navigation_targets_match(&current_definitions) {
                    let target = NavigationTarget::new(self.file, range);
                    self.references.push(RangedValue {
                        value: NavigationTargets::single(target),
                        range: FileRange::new(self.file, range),
                    });
                }
            }
        }
    }

    /// Check if `NavigationTargets` match our target definitions
    fn navigation_targets_match(&self, current_targets: &NavigationTargets) -> bool {
        // Since we're comparing the same symbol, all definitions should be equivalent
        // We only need to check against the first target definition
        if let Some(first_target) = self.target_definitions.iter().next() {
            for current_target in current_targets {
                if current_target.file == first_target.file
                    && current_target.focus_range == first_target.focus_range
                {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{CursorTest, IntoDiagnostic, cursor_test};
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};
    use ruff_text_size::Ranged;

    impl CursorTest {
        fn references(&self) -> String {
            let Some(reference_results) = references(
                &self.db,
                self.cursor.file,
                self.cursor.offset,
                true,
                std::iter::empty(),
            ) else {
                return "No references found".to_string();
            };

            if reference_results.is_empty() {
                return "No references found".to_string();
            }

            self.render_diagnostics(reference_results.into_iter().enumerate().map(
                |(i, ref_item)| -> ReferenceResult {
                    ReferenceResult {
                        index: i,
                        file_range: ref_item.range,
                    }
                },
            ))
        }

        fn references_with_project_files(&self, project_files: Vec<File>) -> String {
            let Some(reference_results) = references(
                &self.db,
                self.cursor.file,
                self.cursor.offset,
                true,
                project_files,
            ) else {
                return "No references found".to_string();
            };

            if reference_results.is_empty() {
                return "No references found".to_string();
            }

            self.render_diagnostics(reference_results.into_iter().enumerate().map(
                |(i, ref_item)| -> ReferenceResult {
                    ReferenceResult {
                        index: i,
                        file_range: ref_item.range,
                    }
                },
            ))
        }
    }

    struct ReferenceResult {
        index: usize,
        file_range: FileRange,
    }

    impl IntoDiagnostic for ReferenceResult {
        fn into_diagnostic(self) -> Diagnostic {
            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("references")),
                Severity::Info,
                format!("Reference {}", self.index + 1),
            );
            main.annotate(Annotation::primary(
                Span::from(self.file_range.file()).with_range(self.file_range.range()),
            ));

            main
        }
    }

    #[test]
    fn test_parameter_references_in_function() {
        let test = cursor_test(
            "
def calculate_sum(<CURSOR>value: int) -> int:
    doubled = value * 2
    result = value + doubled
    return value

# Call with keyword argument
result = calculate_sum(value=42)
",
        );

        assert_snapshot!(test.references(), @r###"
        info[references]: Reference 1
         --> main.py:2:19
          |
        2 | def calculate_sum(value: int) -> int:
          |                   ^^^^^
        3 |     doubled = value * 2
        4 |     result = value + doubled
          |

        info[references]: Reference 2
         --> main.py:3:15
          |
        2 | def calculate_sum(value: int) -> int:
        3 |     doubled = value * 2
          |               ^^^^^
        4 |     result = value + doubled
        5 |     return value
          |

        info[references]: Reference 3
         --> main.py:4:14
          |
        2 | def calculate_sum(value: int) -> int:
        3 |     doubled = value * 2
        4 |     result = value + doubled
          |              ^^^^^
        5 |     return value
          |

        info[references]: Reference 4
         --> main.py:5:12
          |
        3 |     doubled = value * 2
        4 |     result = value + doubled
        5 |     return value
          |            ^^^^^
        6 |
        7 | # Call with keyword argument
          |

        info[references]: Reference 5
         --> main.py:8:24
          |
        7 | # Call with keyword argument
        8 | result = calculate_sum(value=42)
          |                        ^^^^^
          |
        "###);
    }

    #[test]
    #[ignore] // TODO: Enable when nonlocal support is fully implemented in goto.rs
    fn test_nonlocal_variable_references() {
        let test = cursor_test(
            "
def outer_function():
    coun<CURSOR>ter = 0
    
    def increment():
        nonlocal counter
        counter += 1
        return counter
    
    def decrement():
        nonlocal counter
        counter -= 1
        return counter
    
    # Use counter in outer scope
    initial = counter
    increment()
    decrement()
    final = counter
    
    return increment, decrement
",
        );

        assert_snapshot!(test.references(), @r"
        info[references]: Reference 1
         --> main.py:3:5
          |
        2 | def outer_function():
        3 |     counter = 0
          |     ^^^^^^^
        4 |     
        5 |     def increment():
          |

        info[references]: Reference 2
         --> main.py:6:18
          |
        5 |     def increment():
        6 |         nonlocal counter
          |                  ^^^^^^^
        7 |         counter += 1
        8 |         return counter
          |

        info[references]: Reference 3
         --> main.py:7:9
          |
        5 |     def increment():
        6 |         nonlocal counter
        7 |         counter += 1
          |         ^^^^^^^
        8 |         return counter
          |

        info[references]: Reference 4
          --> main.py:8:16
           |
         6 |         nonlocal counter
         7 |         counter += 1
         8 |         return counter
           |                ^^^^^^^
         9 |     
        10 |     def decrement():
           |

        info[references]: Reference 5
          --> main.py:11:18
           |
        10 |     def decrement():
        11 |         nonlocal counter
           |                  ^^^^^^^
        12 |         counter -= 1
        13 |         return counter
           |

        info[references]: Reference 6
          --> main.py:12:9
           |
        10 |     def decrement():
        11 |         nonlocal counter
        12 |         counter -= 1
           |         ^^^^^^^
        13 |         return counter
           |

        info[references]: Reference 7
          --> main.py:13:16
           |
        11 |         nonlocal counter
        12 |         counter -= 1
        13 |         return counter
           |                ^^^^^^^
        14 |     
        15 |     # Use counter in outer scope
           |

        info[references]: Reference 8
          --> main.py:16:15
           |
        15 |     # Use counter in outer scope
        16 |     initial = counter
           |               ^^^^^^^
        17 |     increment()
        18 |     decrement()
           |

        info[references]: Reference 9
          --> main.py:19:13
           |
        17 |     increment()
        18 |     decrement()
        19 |     final = counter
           |             ^^^^^^^
        20 |     
        21 |     return increment, decrement
           |
        ");
    }

    #[test]
    #[ignore] // TODO: Enable when global support is fully implemented in goto.rs
    fn test_global_variable_references() {
        let test = cursor_test(
            "
glo<CURSOR>bal_counter = 0

def increment_global():
    global global_counter
    global_counter += 1
    return global_counter

def decrement_global():
    global global_counter
    global_counter -= 1
    return global_counter

# Use global_counter at module level
initial_value = global_counter
increment_global()
decrement_global()
final_value = global_counter
",
        );

        assert_snapshot!(test.references(), @r"
        info[references]: Reference 1
         --> main.py:2:1
          |
        2 | global_counter = 0
          | ^^^^^^^^^^^^^^
        3 |
        4 | def increment_global():
          |

        info[references]: Reference 2
         --> main.py:5:12
          |
        4 | def increment_global():
        5 |     global global_counter
          |            ^^^^^^^^^^^^^^
        6 |     global_counter += 1
        7 |     return global_counter
          |

        info[references]: Reference 3
         --> main.py:6:5
          |
        4 | def increment_global():
        5 |     global global_counter
        6 |     global_counter += 1
          |     ^^^^^^^^^^^^^^
        7 |     return global_counter
          |

        info[references]: Reference 4
         --> main.py:7:12
          |
        5 |     global global_counter
        6 |     global_counter += 1
        7 |     return global_counter
          |            ^^^^^^^^^^^^^^
        8 |
        9 | def decrement_global():
          |

        info[references]: Reference 5
          --> main.py:10:12
           |
         9 | def decrement_global():
        10 |     global global_counter
           |            ^^^^^^^^^^^^^^
        11 |     global_counter -= 1
        12 |     return global_counter
           |

        info[references]: Reference 6
          --> main.py:11:5
           |
         9 | def decrement_global():
        10 |     global global_counter
        11 |     global_counter -= 1
           |     ^^^^^^^^^^^^^^
        12 |     return global_counter
           |

        info[references]: Reference 7
          --> main.py:12:12
           |
        10 |     global global_counter
        11 |     global_counter -= 1
        12 |     return global_counter
           |            ^^^^^^^^^^^^^^
        13 |
        14 | # Use global_counter at module level
           |

        info[references]: Reference 8
          --> main.py:15:17
           |
        14 | # Use global_counter at module level
        15 | initial_value = global_counter
           |                 ^^^^^^^^^^^^^^
        16 | increment_global()
        17 | decrement_global()
           |

        info[references]: Reference 9
          --> main.py:18:15
           |
        16 | increment_global()
        17 | decrement_global()
        18 | final_value = global_counter
           |               ^^^^^^^^^^^^^^
           |
        ");
    }

    #[test]
    fn test_except_handler_variable_references() {
        let test = cursor_test(
            "
try:
    x = 1 / 0
except ZeroDivisionError as e<CURSOR>rr:
    print(f'Error: {err}')
    return err

try:
    y = 2 / 0
except ValueError as err:
    print(f'Different error: {err}')
",
        );

        // Note: Currently only finds the declaration, not the usages
        // This is because semantic analysis for except handler variables isn't fully implemented
        assert_snapshot!(test.references(), @r###"
        info[references]: Reference 1
         --> main.py:4:29
          |
        2 | try:
        3 |     x = 1 / 0
        4 | except ZeroDivisionError as err:
          |                             ^^^
        5 |     print(f'Error: {err}')
        6 |     return err
          |
        "###);
    }

    #[test]
    fn test_pattern_match_as_references() {
        let test = cursor_test(
            "
match x:
    case [a, b] as patter<CURSOR>n:
        print(f'Matched: {pattern}')
        return pattern
    case _:
        pass
",
        );

        assert_snapshot!(test.references(), @r###"
        info[references]: Reference 1
         --> main.py:3:20
          |
        2 | match x:
        3 |     case [a, b] as pattern:
          |                    ^^^^^^^
        4 |         print(f'Matched: {pattern}')
        5 |         return pattern
          |

        info[references]: Reference 2
         --> main.py:4:27
          |
        2 | match x:
        3 |     case [a, b] as pattern:
        4 |         print(f'Matched: {pattern}')
          |                           ^^^^^^^
        5 |         return pattern
        6 |     case _:
          |

        info[references]: Reference 3
         --> main.py:5:16
          |
        3 |     case [a, b] as pattern:
        4 |         print(f'Matched: {pattern}')
        5 |         return pattern
          |                ^^^^^^^
        6 |     case _:
        7 |         pass
          |
        "###);
    }

    #[test]
    fn test_pattern_match_mapping_rest_references() {
        let test = cursor_test(
            "
match data:
    case {'a': a, 'b': b, **re<CURSOR>st}:
        print(f'Rest data: {rest}')
        process(rest)
        return rest
",
        );

        assert_snapshot!(test.references(), @r###"
        info[references]: Reference 1
         --> main.py:3:29
          |
        2 | match data:
        3 |     case {'a': a, 'b': b, **rest}:
          |                             ^^^^
        4 |         print(f'Rest data: {rest}')
        5 |         process(rest)
          |

        info[references]: Reference 2
         --> main.py:4:29
          |
        2 | match data:
        3 |     case {'a': a, 'b': b, **rest}:
        4 |         print(f'Rest data: {rest}')
          |                             ^^^^
        5 |         process(rest)
        6 |         return rest
          |

        info[references]: Reference 3
         --> main.py:5:17
          |
        3 |     case {'a': a, 'b': b, **rest}:
        4 |         print(f'Rest data: {rest}')
        5 |         process(rest)
          |                 ^^^^
        6 |         return rest
          |

        info[references]: Reference 4
         --> main.py:6:16
          |
        4 |         print(f'Rest data: {rest}')
        5 |         process(rest)
        6 |         return rest
          |                ^^^^
          |
        "###);
    }

    #[test]
    fn test_function_definition_references() {
        let test = cursor_test(
            "
def my_func<CURSOR>tion():
    return 42

# Call the function multiple times
result1 = my_function()
result2 = my_function()

# Function passed as an argument
callback = my_function

# Function used in different contexts
print(my_function())
value = my_function
",
        );

        assert_snapshot!(test.references(), @r"
        info[references]: Reference 1
         --> main.py:2:5
          |
        2 | def my_function():
          |     ^^^^^^^^^^^
        3 |     return 42
          |

        info[references]: Reference 2
         --> main.py:6:11
          |
        5 | # Call the function multiple times
        6 | result1 = my_function()
          |           ^^^^^^^^^^^
        7 | result2 = my_function()
          |

        info[references]: Reference 3
         --> main.py:7:11
          |
        5 | # Call the function multiple times
        6 | result1 = my_function()
        7 | result2 = my_function()
          |           ^^^^^^^^^^^
        8 |
        9 | # Function passed as an argument
          |

        info[references]: Reference 4
          --> main.py:10:12
           |
         9 | # Function passed as an argument
        10 | callback = my_function
           |            ^^^^^^^^^^^
        11 |
        12 | # Function used in different contexts
           |

        info[references]: Reference 5
          --> main.py:13:7
           |
        12 | # Function used in different contexts
        13 | print(my_function())
           |       ^^^^^^^^^^^
        14 | value = my_function
           |

        info[references]: Reference 6
          --> main.py:14:9
           |
        12 | # Function used in different contexts
        13 | print(my_function())
        14 | value = my_function
           |         ^^^^^^^^^^^
           |
        ");
    }

    #[test]
    fn test_class_definition_references() {
        let test = cursor_test(
            "
class My<CURSOR>Class:
    def __init__(self):
        pass

# Create instances
obj1 = MyClass()
obj2 = MyClass()

# Use in type annotations
def process(instance: MyClass) -> MyClass:
    return instance

# Reference the class itself
cls = MyClass
",
        );

        assert_snapshot!(test.references(), @r"
        info[references]: Reference 1
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
        3 |     def __init__(self):
        4 |         pass
          |

        info[references]: Reference 2
         --> main.py:7:8
          |
        6 | # Create instances
        7 | obj1 = MyClass()
          |        ^^^^^^^
        8 | obj2 = MyClass()
          |

        info[references]: Reference 3
          --> main.py:8:8
           |
         6 | # Create instances
         7 | obj1 = MyClass()
         8 | obj2 = MyClass()
           |        ^^^^^^^
         9 |
        10 | # Use in type annotations
           |

        info[references]: Reference 4
          --> main.py:11:23
           |
        10 | # Use in type annotations
        11 | def process(instance: MyClass) -> MyClass:
           |                       ^^^^^^^
        12 |     return instance
           |

        info[references]: Reference 5
          --> main.py:11:35
           |
        10 | # Use in type annotations
        11 | def process(instance: MyClass) -> MyClass:
           |                                   ^^^^^^^
        12 |     return instance
           |

        info[references]: Reference 6
          --> main.py:15:7
           |
        14 | # Reference the class itself
        15 | cls = MyClass
           |       ^^^^^^^
           |
        ");
    }

    #[test]
    fn test_multi_file_function_references() {
        let test = CursorTest::builder()
            .source(
                "utils.py",
                "
def helper_fun<CURSOR>ction(x):
    return x * 2
",
            )
            .source(
                "module.py",
                "
from utils import helper_function

def process_data(data):
    return helper_function(data)

def double_process(data):
    result = helper_function(data)
    return helper_function(result)
",
            )
            .source(
                "app.py",
                "
from utils import helper_function

class DataProcessor:
    def __init__(self):
        self.multiplier = helper_function
    
    def process(self, value):
        return helper_function(value)
",
            )
            .build();

        assert_snapshot!(test.references_with_project_files(test.files.clone()), @r"
        info[references]: Reference 1
         --> utils.py:2:5
          |
        2 | def helper_function(x):
          |     ^^^^^^^^^^^^^^^
        3 |     return x * 2
          |

        info[references]: Reference 2
         --> module.py:5:12
          |
        4 | def process_data(data):
        5 |     return helper_function(data)
          |            ^^^^^^^^^^^^^^^
        6 |
        7 | def double_process(data):
          |

        info[references]: Reference 3
         --> module.py:8:14
          |
        7 | def double_process(data):
        8 |     result = helper_function(data)
          |              ^^^^^^^^^^^^^^^
        9 |     return helper_function(result)
          |

        info[references]: Reference 4
         --> module.py:9:12
          |
        7 | def double_process(data):
        8 |     result = helper_function(data)
        9 |     return helper_function(result)
          |            ^^^^^^^^^^^^^^^
          |

        info[references]: Reference 5
         --> app.py:6:27
          |
        4 | class DataProcessor:
        5 |     def __init__(self):
        6 |         self.multiplier = helper_function
          |                           ^^^^^^^^^^^^^^^
        7 |     
        8 |     def process(self, value):
          |

        info[references]: Reference 6
         --> app.py:9:16
          |
        8 |     def process(self, value):
        9 |         return helper_function(value)
          |                ^^^^^^^^^^^^^^^
          |
        ");
    }

    #[test]
    fn test_multi_file_class_attribute_references() {
        let test = CursorTest::builder()
            .source(
                "models.py",
                "
class MyModel:
    a<CURSOR>ttr = 42
        
    def get_attribute(self):
        return MyModel.attr
",
            )
            .source(
                "main.py",
                "
from models import MyModel

def process_model():
    model = MyModel()
    value = model.attr
    model.attr = 100
    return model.attr
",
            )
            .build();

        assert_snapshot!(test.references_with_project_files(test.files.clone()), @r"
        info[references]: Reference 1
         --> models.py:3:5
          |
        2 | class MyModel:
        3 |     attr = 42
          |     ^^^^
        4 |         
        5 |     def get_attribute(self):
          |

        info[references]: Reference 2
         --> models.py:6:24
          |
        5 |     def get_attribute(self):
        6 |         return MyModel.attr
          |                        ^^^^
          |

        info[references]: Reference 3
         --> main.py:6:19
          |
        4 | def process_model():
        5 |     model = MyModel()
        6 |     value = model.attr
          |                   ^^^^
        7 |     model.attr = 100
        8 |     return model.attr
          |

        info[references]: Reference 4
         --> main.py:7:11
          |
        5 |     model = MyModel()
        6 |     value = model.attr
        7 |     model.attr = 100
          |           ^^^^
        8 |     return model.attr
          |

        info[references]: Reference 5
         --> main.py:8:18
          |
        6 |     value = model.attr
        7 |     model.attr = 100
        8 |     return model.attr
          |                  ^^^^
          |
        ");
    }
}
