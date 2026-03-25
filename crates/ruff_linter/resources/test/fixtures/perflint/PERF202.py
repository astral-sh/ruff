import os

GLOBAL_VAR = 42
ANOTHER_GLOBAL = "hello"
GLOBAL_DICT = {"key": 1}
MY_GLOBAL_CONSTANT_C = 1234
MY_GLOBAL_CONSTANT_A = 3.14
MY_GLOBAL_VARIABLE = 1234


# Errors

# perflint: tests/test_loop_invariant.py::test_global_in_for_loop (line 85)
# Global variable accessed in a for loop
def test_global_in_for_loop():
    items = (1, 2, 3, 4)
    for item in list(items):
        GLOBAL_VAR  # PERF202


# perflint: tests/functional/global_usage.py::global_constant_in_loop (line 7)
# Global constant used in arithmetic inside loop
def global_constant_in_loop():
    total = MY_GLOBAL_CONSTANT_A
    for i in range(10_000):
        total += i * MY_GLOBAL_CONSTANT_C  # PERF202


# perflint: tests/functional/global_usage.py::global_variable_in_loop (line 26)
# global statement + global variable accessed in loop
def global_variable_in_loop():
    global MY_GLOBAL_VARIABLE
    total = MY_GLOBAL_CONSTANT_A
    for i in range(10_000):
        MY_GLOBAL_VARIABLE += total  # PERF202
        total += i


# Global variable in while loop
def while_loop():
    while True:
        print(GLOBAL_VAR)  # PERF202
        break


# Multiple globals in same loop
def multiple_globals():
    for i in range(10):
        a = GLOBAL_VAR  # PERF202
        b = ANOTHER_GLOBAL  # PERF202


# Global in nested loops
def nested_loops():
    for i in range(10):
        for j in range(10):
            print(GLOBAL_VAR)  # PERF202


# Async for loop
async def async_loop():
    async for item in some_aiter():
        print(GLOBAL_VAR)  # PERF202


# Global inside control flow inside loop
def control_flow_in_loop():
    for i in range(10):
        if i > 5:
            print(GLOBAL_VAR)  # PERF202
        elif GLOBAL_VAR:  # PERF202
            pass
        try:
            print(GLOBAL_VAR)  # PERF202
        except Exception:
            pass
        with open("f") as fh:
            print(GLOBAL_VAR)  # PERF202


# Global used as subscript target
def subscript_global():
    for i in range(10):
        x = GLOBAL_DICT[i]  # PERF202


# Global in comprehension inside loop
def comprehension_in_loop():
    for i in range(10):
        x = [GLOBAL_VAR for _ in range(5)]  # PERF202


# OK

# perflint: tests/test_loop_invariant.py::test_assigned_global_in_for_loop (line 101)
# Loop target shadows a module-level global name
def test_assigned_global_in_for_loop():
    items = (1, 2, 3, 4)
    for GLOBAL_VAR in list(items):
        GLOBAL_VAR


# perflint: tests/test_loop_invariant.py::test_self_assignment_for_loop (line 117)
# self.n used as loop target and body
class Foo:
    n = 1

    def loop(self):
        for self.n in range(4):
            print(self.n)


# perflint: tests/functional/global_usage.py::local_constant_in_loop (line 15)
# Only local/literal values used in loop
def local_constant_in_loop():
    total = 3.14
    for i in range(10_000):
        total += i * 1234


# perflint: tests/functional/global_usage.py::recursive_example (line 36)
# Recursive function call and calling another function in loop
def recursive_example(x):
    for i in range(100):
        recursive_example(i)

    for i in range(100):
        global_constant_in_loop()


# Builtins should not be flagged
def uses_builtins(seq):
    for item in seq:
        print(len(seq))
        x = range(10)
        y = list(seq)


# Function parameters should not be flagged
def uses_params(param):
    for i in range(10):
        print(param)


# Local variables should not be flagged
def uses_locals():
    local_var = 10
    for i in range(10):
        print(local_var)


# Variables assigned in loop body should not be flagged
def assigns_in_loop():
    for i in range(10):
        x = 10
        print(x)


# Loop target variable should not be flagged
def loop_target():
    for item in range(10):
        print(item)


# Module-level loop should not be flagged
for i in range(10):
    print(GLOBAL_VAR)


# Cached as local before loop should not be flagged
def cached_local():
    local_copy = GLOBAL_VAR
    for i in range(10):
        print(local_copy)


# Module-level imports should not be flagged
def uses_import():
    for i in range(10):
        os.path.join("a", "b")


# Module-level function definitions should not be flagged
def helper():
    return 42


def calls_function_in_loop():
    for i in range(10):
        helper()


# Module-level class used in loop should not be flagged
class MyClass:
    pass


def uses_class_in_loop():
    for i in range(10):
        obj = MyClass()


# Nonlocal variable should not be flagged
def outer():
    x = 10
    def inner():
        nonlocal x
        for i in range(10):
            print(x)
    inner()


# OK (cases that perflint flags but ruff intentionally does not)

# del uses DELETE_GLOBAL, not LOAD_GLOBAL - no performance gain from caching
def del_in_loop():
    for i in range(10):
        del GLOBAL_VAR


# Loop else clause runs once after the loop, not a hot path
def loop_else():
    for i in range(10):
        pass
    else:
        print(GLOBAL_VAR)


# OK (cases that perflint flags but ruff cannot due to deferred
# function/class/lambda body analysis)

# Nested function inside loop
def nested_func_in_loop():
    for i in range(10):
        def inner():
            print(GLOBAL_VAR)


# Lambda inside loop
def lambda_in_loop():
    for i in range(10):
        fn = lambda: GLOBAL_VAR


# Class inside loop
def class_in_loop():
    for i in range(10):
        class InnerClass:
            x = GLOBAL_VAR
