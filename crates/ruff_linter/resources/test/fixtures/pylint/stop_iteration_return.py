"""Test cases for PLR1708 stop-iteration-return."""


# Valid cases - should not trigger the rule
def normal_function():
    raise StopIteration  # Not a generator, should not trigger


def normal_function_with_value():
    raise StopIteration("value")  # Not a generator, should not trigger


def generator_with_return():
    yield 1
    yield 2
    return "finished"  # This is the correct way


def generator_with_yield_from():
    yield from [1, 2, 3]


def generator_without_stop_iteration():
    yield 1
    yield 2
    # No explicit termination


def generator_with_other_exception():
    yield 1
    raise ValueError("something else")  # Different exception


# Invalid cases - should trigger the rule
def generator_with_stop_iteration():
    yield 1
    yield 2
    raise StopIteration  # Should trigger


def generator_with_stop_iteration_value():
    yield 1
    yield 2
    raise StopIteration("finished")  # Should trigger


def generator_with_stop_iteration_expr():
    yield 1
    yield 2
    raise StopIteration(1 + 2)  # Should trigger


def async_generator_with_stop_iteration():
    yield 1
    yield 2
    raise StopIteration("async")  # Should trigger


def nested_generator():
    def inner_gen():
        yield 1
        raise StopIteration("inner")  # Should trigger

    yield from inner_gen()


def generator_in_class():
    class MyClass:
        def generator_method(self):
            yield 1
            raise StopIteration("method")  # Should trigger

    return MyClass


# Complex cases
def complex_generator():
    try:
        yield 1
        yield 2
        raise StopIteration("complex")  # Should trigger
    except ValueError:
        yield 3
    finally:
        pass


def generator_with_conditional_stop_iteration(condition):
    yield 1
    if condition:
        raise StopIteration("conditional")  # Should trigger
    yield 2


# Edge cases
def generator_with_bare_stop_iteration():
    yield 1
    raise StopIteration  # Should trigger (no arguments)


def generator_with_stop_iteration_in_loop():
    for i in range(5):
        yield i
        if i == 3:
            raise StopIteration("loop")  # Should trigger


# Should not trigger - different exceptions
def generator_with_runtime_error():
    yield 1
    raise RuntimeError("not StopIteration")  # Should not trigger


def generator_with_custom_exception():
    yield 1
    raise CustomException("custom")  # Should not trigger


class CustomException(Exception):
    pass


# Generator comprehensions should not be affected
list_comp = [x for x in range(10)]  # Should not trigger


# Lambda in generator context
def generator_with_lambda():
    yield 1
    func = lambda x: x  # Just a regular lambda
    yield 2

# See: https://github.com/astral-sh/ruff/issues/21162
def foo():
    def g():
        yield 1
    raise StopIteration  # Should not trigger


def foo():
    def g():
        raise StopIteration  # Should not trigger
    yield 1

# https://github.com/astral-sh/ruff/pull/21177#pullrequestreview-3430209718
def foo():
    yield 1
    class C:
        raise StopIteration  # Should trigger
    yield C

# https://github.com/astral-sh/ruff/pull/21177#discussion_r2539702728
def foo():
    raise StopIteration((yield 1))  # Should trigger