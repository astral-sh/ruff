use crate::goto::find_goto_target;
use crate::references::{ReferencesMode, references};
use crate::{Db, ReferenceTarget};
use ruff_db::files::File;
use ruff_text_size::TextSize;

/// Find all references to a symbol at the given position.
/// Search for references across all files in the project.
pub fn goto_references(
    db: &dyn Db,
    file: File,
    offset: TextSize,
    include_declaration: bool,
) -> Option<Vec<ReferenceTarget>> {
    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);

    // Get the definitions for the symbol at the cursor position
    let goto_target = find_goto_target(&module, offset)?;

    let mode = if include_declaration {
        ReferencesMode::References
    } else {
        ReferencesMode::ReferencesSkipDeclaration
    };

    references(db, file, &goto_target, mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{CursorTest, IntoDiagnostic, cursor_test};
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};
    use ruff_db::files::FileRange;
    use ruff_text_size::Ranged;

    impl CursorTest {
        fn references(&self) -> String {
            let Some(mut reference_results) =
                goto_references(&self.db, self.cursor.file, self.cursor.offset, true)
            else {
                return "No references found".to_string();
            };

            if reference_results.is_empty() {
                return "No references found".to_string();
            }

            reference_results.sort_by_key(ReferenceTarget::file);

            self.render_diagnostics(reference_results.into_iter().enumerate().map(
                |(i, ref_item)| -> ReferenceResult {
                    ReferenceResult {
                        index: i,
                        file_range: FileRange::new(ref_item.file(), ref_item.range()),
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

        assert_snapshot!(test.references(), @r"
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

        info[references]: Reference 2
         --> main.py:5:21
          |
        3 |     x = 1 / 0
        4 | except ZeroDivisionError as err:
        5 |     print(f'Error: {err}')
          |                     ^^^
        6 |     return err
          |

        info[references]: Reference 3
         --> main.py:6:12
          |
        4 | except ZeroDivisionError as err:
        5 |     print(f'Error: {err}')
        6 |     return err
          |            ^^^
        7 |
        8 | try:
          |

        info[references]: Reference 4
          --> main.py:11:31
           |
         9 |     y = 2 / 0
        10 | except ValueError as err:
        11 |     print(f'Different error: {err}')
           |                               ^^^
           |
        ");
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
def fun<CURSOR>c(x):
    return x * 2
",
            )
            .source(
                "module.py",
                "
from utils import func

def process_data(data):
    return func(data)
",
            )
            .source(
                "app.py",
                "
from utils import func

class DataProcessor:
    def __init__(self):
        self.multiplier = func
    
    def process(self, value):
        return func(value)
",
            )
            .build();

        assert_snapshot!(test.references(), @r"
        info[references]: Reference 1
         --> utils.py:2:5
          |
        2 | def func(x):
          |     ^^^^
        3 |     return x * 2
          |

        info[references]: Reference 2
         --> module.py:2:19
          |
        2 | from utils import func
          |                   ^^^^
        3 |
        4 | def process_data(data):
          |

        info[references]: Reference 3
         --> module.py:5:12
          |
        4 | def process_data(data):
        5 |     return func(data)
          |            ^^^^
          |

        info[references]: Reference 4
         --> app.py:2:19
          |
        2 | from utils import func
          |                   ^^^^
        3 |
        4 | class DataProcessor:
          |

        info[references]: Reference 5
         --> app.py:6:27
          |
        4 | class DataProcessor:
        5 |     def __init__(self):
        6 |         self.multiplier = func
          |                           ^^^^
        7 |
        8 |     def process(self, value):
          |

        info[references]: Reference 6
         --> app.py:9:16
          |
        8 |     def process(self, value):
        9 |         return func(value)
          |                ^^^^
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

        assert_snapshot!(test.references(), @r"
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

    #[test]
    fn test_import_alias_references_should_not_resolve_to_original() {
        let test = CursorTest::builder()
            .source(
                "original.py",
                "
def func():
    pass

func()
",
            )
            .source(
                "importer.py",
                "
from original import func as func_alias

func<CURSOR>_alias()
",
            )
            .build();

        // When finding references to the alias, we should NOT find references
        // to the original function in the original module
        assert_snapshot!(test.references(), @r"
        info[references]: Reference 1
         --> importer.py:2:30
          |
        2 | from original import func as func_alias
          |                              ^^^^^^^^^^
        3 |
        4 | func_alias()
          |

        info[references]: Reference 2
         --> importer.py:4:1
          |
        2 | from original import func as func_alias
        3 |
        4 | func_alias()
          | ^^^^^^^^^^
          |
        ");
    }
}
