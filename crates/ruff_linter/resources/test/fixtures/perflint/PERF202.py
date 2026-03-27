import os

GLOBAL_VAR = 42
ANOTHER_GLOBAL = "hello"
GLOBAL_DICT = {"key": 1}
MY_GLOBAL_CONSTANT_C = 1234
MY_GLOBAL_CONSTANT_A = 3.14
MY_GLOBAL_VARIABLE = 1234
THRESHOLD = 100
SENTINEL = object()


# Errors

# 5+ direct reads of globals in binary/comparison expressions
def global_in_binary_exprs():
    total = 0
    for i in range(10_000):  # PERF202
        total += i * MY_GLOBAL_CONSTANT_C
        total += i + MY_GLOBAL_CONSTANT_C
        total -= MY_GLOBAL_CONSTANT_C
        if total > MY_GLOBAL_CONSTANT_C:
            total = total % MY_GLOBAL_CONSTANT_C


# Augmented assignment values count as direct reads
def augmented_assign_in_loop():
    total = 0
    for i in range(10_000):  # PERF202
        total += MY_GLOBAL_CONSTANT_C
        total -= MY_GLOBAL_CONSTANT_C
        total *= MY_GLOBAL_CONSTANT_C
        if total > MY_GLOBAL_CONSTANT_C:
            total //= MY_GLOBAL_CONSTANT_C


# Multiple different globals, all in binary expressions
def multiple_globals_in_exprs():
    total = 0
    for i in range(10_000):  # PERF202
        total += i * MY_GLOBAL_CONSTANT_C
        total += i * MY_GLOBAL_CONSTANT_A
        if total > THRESHOLD:
            total -= THRESHOLD
        if i % 2 == GLOBAL_VAR:
            total += GLOBAL_VAR


# Comparison and boolean expressions
def comparison_and_bool_exprs():
    results = []
    for item in range(10_000):  # PERF202
        if item > THRESHOLD and item < THRESHOLD * 2:
            pass
        if item is not SENTINEL:
            pass
        if not SENTINEL:
            pass
        if item + THRESHOLD > 0:
            pass


# While loop with 5+ direct reads
def while_loop_exprs():
    total = 0
    i = 0
    while i < 10_000:  # PERF202
        total += i * THRESHOLD
        total -= THRESHOLD
        if total > THRESHOLD:
            total = total % THRESHOLD
        if total == THRESHOLD:
            break
        i += 1


# While condition itself counts (evaluated every iteration)
def while_condition_global():
    total = 0
    i = 0
    while total < THRESHOLD:  # PERF202
        total += i * THRESHOLD
        total -= THRESHOLD
        if total > THRESHOLD:
            total = total % THRESHOLD
        i += 1


# Async for loop with 5+ direct reads
async def async_loop_exprs():
    total = 0
    async for item in some_aiter():  # PERF202
        total += item * THRESHOLD
        total -= THRESHOLD
        if total > THRESHOLD:
            total = total % THRESHOLD
        if total == THRESHOLD:
            break


# Global in comprehension inside loop, in binary expr
def comprehension_in_loop():
    for i in range(10):  # PERF202
        x = [j + THRESHOLD for j in range(5)]
        y = [j * THRESHOLD for j in range(5)]
        z = [j - THRESHOLD for j in range(5)]
        a = [j > THRESHOLD for j in range(5)]
        b = [j < THRESHOLD for j in range(5)]


# OK (fewer than 5 direct reads - not worth optimizing)

# Single global access - below threshold
def single_global_access():
    items = (1, 2, 3, 4)
    for item in list(items):
        GLOBAL_VAR


# Only one direct read per loop
def global_constant_once():
    total = MY_GLOBAL_CONSTANT_A
    for i in range(10_000):
        total += i * MY_GLOBAL_CONSTANT_C


# 4 references - just below threshold
def just_below_threshold():
    total = 0
    for i in range(10_000):
        total += i * MY_GLOBAL_CONSTANT_C
        total -= MY_GLOBAL_CONSTANT_C
        if total > MY_GLOBAL_CONSTANT_C:
            total = total % MY_GLOBAL_CONSTANT_C


# Global in method call - not a direct read.
# 5 call-based refs
def global_in_method_call():
    total = 0
    for i in range(10):
        print(GLOBAL_VAR)
        print(GLOBAL_VAR)
        print(GLOBAL_VAR)
        print(GLOBAL_VAR)
        print(GLOBAL_VAR)


# Global as subscript - not a direct read.
# 5 subscript-based refs
def global_as_subscript():
    total = 0
    for i in range(10):
        GLOBAL_DICT[i]
        GLOBAL_DICT[i]
        GLOBAL_DICT[i]
        GLOBAL_DICT[i]
        GLOBAL_DICT[i]


# Logger in loop - not a direct read (attribute access).
# 5 attribute-based refs
def logger_in_loop():
    import logging
    log = logging.getLogger(__name__)
    for i in range(10):
        log.info("msg")
        log.info("msg")
        log.info("msg")
        log.info("msg")
        log.info("msg")


# OK (general non-flagged cases)

# Loop target shadows a module-level global name
def test_assigned_global_in_for_loop():
    items = (1, 2, 3, 4)
    for GLOBAL_VAR in list(items):
        GLOBAL_VAR


# self.n used as loop target and body
class Foo:
    n = 1

    def loop(self):
        for self.n in range(4):
            print(self.n)
            print(self.n)
            print(self.n)
            print(self.n)
            print(self.n)


# Only local/literal values used in loop
def local_constant_in_loop():
    total = 3.14
    for i in range(10_000):
        total += i * 1234
        total += i * 1234
        total += i * 1234
        total += i * 1234
        total += i * 1234


# Recursive function call and calling another function in loop
def recursive_example(x):
    for i in range(100):
        recursive_example(i)

    for i in range(100):
        global_constant_once()


# Builtins should not be flagged
def uses_builtins(seq):
    for item in seq:
        print(len(seq))
        print(len(seq))
        print(len(seq))
        print(len(seq))
        print(len(seq))


# Function parameters should not be flagged
def uses_params(param):
    for i in range(10):
        print(param)
        print(param)
        print(param)
        print(param)
        print(param)


# Local variables should not be flagged
def uses_locals():
    local_var = 10
    for i in range(10):
        print(local_var)
        print(local_var)
        print(local_var)
        print(local_var)
        print(local_var)


# Variables assigned in loop body should not be flagged
def assigns_in_loop():
    for i in range(10):
        x = 10
        print(x)
        x = 10
        print(x)
        x = 10
        print(x)
        x = 10
        print(x)
        x = 10
        print(x)


# Loop target variable should not be flagged
def loop_target():
    for item in range(10):
        print(item)
        print(item)
        print(item)
        print(item)
        print(item)


# Module-level loop should not be flagged
for i in range(10):
    print(GLOBAL_VAR)
    print(GLOBAL_VAR)
    print(GLOBAL_VAR)
    print(GLOBAL_VAR)
    print(GLOBAL_VAR)


# Cached as local before loop should not be flagged
def cached_local():
    local_copy = GLOBAL_VAR
    for i in range(10):
        print(local_copy)
        print(local_copy)
        print(local_copy)
        print(local_copy)
        print(local_copy)


# Module-level imports should not be flagged
def uses_import():
    for i in range(10):
        os.path.join("a", "b")
        os.path.join("a", "b")
        os.path.join("a", "b")
        os.path.join("a", "b")
        os.path.join("a", "b")


# Module-level function definitions should not be flagged
def helper():
    return 42


def calls_function_in_loop():
    for i in range(10):
        helper()
        helper()
        helper()
        helper()
        helper()


# Module-level class used in loop should not be flagged
class MyClass:
    pass


def uses_class_in_loop():
    for i in range(10):
        obj = MyClass()
        obj = MyClass()
        obj = MyClass()
        obj = MyClass()
        obj = MyClass()


# Nonlocal variable should not be flagged
def outer():
    x = 10
    def inner():
        nonlocal x
        for i in range(10):
            print(x)
            print(x)
            print(x)
            print(x)
            print(x)
    inner()


# global statement + augmented assignment - AugAssign targets with global
# binding are not resolved in ruff's semantic model
def global_variable_in_loop():
    global MY_GLOBAL_VARIABLE
    for i in range(10_000):
        MY_GLOBAL_VARIABLE += i
        MY_GLOBAL_VARIABLE -= i
        MY_GLOBAL_VARIABLE *= i
        if MY_GLOBAL_VARIABLE > 0:
            MY_GLOBAL_VARIABLE //= 2


# del uses DELETE_GLOBAL, not LOAD_GLOBAL - no performance gain from caching
def del_in_loop():
    for i in range(10):
        del GLOBAL_VAR
        del GLOBAL_VAR
        del GLOBAL_VAR
        del GLOBAL_VAR
        del GLOBAL_VAR


# Loop else clause runs once after the loop, not a hot path
def loop_else():
    for i in range(10):
        pass
    else:
        x = 5 * GLOBAL_VAR
        x = 5 * GLOBAL_VAR
        x = 5 * GLOBAL_VAR
        x = 5 * GLOBAL_VAR
        x = 5 * GLOBAL_VAR


# OK (cases that perflint flags but ruff cannot due to deferred
# function/class/lambda body analysis)

# Nested function inside loop
def nested_func_in_loop():
    for i in range(10):
        def inner():
            print(GLOBAL_VAR)
            print(GLOBAL_VAR)
            print(GLOBAL_VAR)
            print(GLOBAL_VAR)
            print(GLOBAL_VAR)


# Lambda inside loop
def lambda_in_loop():
    for i in range(10):
        fn = lambda: GLOBAL_VAR
        fn = lambda: GLOBAL_VAR
        fn = lambda: GLOBAL_VAR
        fn = lambda: GLOBAL_VAR
        fn = lambda: GLOBAL_VAR


# Class inside loop
def class_in_loop():
    for i in range(10):
        class InnerClass:
            x = GLOBAL_VAR
            x = GLOBAL_VAR
            x = GLOBAL_VAR
            x = GLOBAL_VAR
            x = GLOBAL_VAR
