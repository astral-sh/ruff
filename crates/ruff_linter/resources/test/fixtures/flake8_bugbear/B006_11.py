# Test nested function reading value from outer scope
def outer_function():
    my_list = [1, 2, 3]  # Assignment in outer function
    
    # Should trigger B006 - nested function using mutable from outer scope
    def inner_function(items=my_list):
        return items
    
    return inner_function


# Test that imports don't trigger B006 (assignment restriction)
from some_module import IMPORTED_LIST

# Should NOT trigger B006 - imported names are not assignments
def func_with_import(items=IMPORTED_LIST):
    return items


# Test that function parameters don't trigger B006 (assignment restriction)
def func_with_param(param_list):
    # Should NOT trigger B006 - function parameters are not assignments
    def nested_func(items=param_list):
        return items
    
    return nested_func


# Test module-level assignment that should trigger B006
module_list = [1, 2, 3]  # Module-level assignment

# Should trigger B006 - module-level assignment used as default
def func_with_module_var(items=module_list):
    return items


# Test that non-assignment bindings don't trigger B006
# (This would require a more complex setup to actually test, so we'll focus on the key cases above)

