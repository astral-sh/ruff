@pytest.mark.foo(scope="module")
def ok_due_to_missing_import():
    pass


import pytest


@pytest.mark.foo(scope="module")
def ok_with_parameters_regardless_of_config():
    pass


# Without parentheses


@pytest.mark.foo
def test_something():
    pass


@pytest.mark.foo
class TestClass:
    def test_something():
        pass


class TestClass:
    @pytest.mark.foo
    def test_something():
        pass


class TestClass:
    @pytest.mark.foo
    class TestNestedClass:
        def test_something():
            pass


class TestClass:
    class TestNestedClass:
        @pytest.mark.foo
        def test_something():
            pass


# With parentheses


@pytest.mark.foo()
def test_something():
    pass


@pytest.mark.foo()
class TestClass:
    def test_something():
        pass


class TestClass:
    @pytest.mark.foo()
    def test_something():
        pass


class TestClass:
    @pytest.mark.foo()
    class TestNestedClass:
        def test_something():
            pass


class TestClass:
    class TestNestedClass:
        @pytest.mark.foo()
        def test_something():
            pass

# https://github.com/astral-sh/ruff/issues/18770
@pytest.mark.parametrize(
    # TODO: fix later
    # ("param1", "param2"),
    # (
    #     (1, 2),
    #     (3, 4),
    # ),
)
def test_bar(param1, param2): ...


@(pytest.mark.foo())
def test_outer_paren_mark_function():
    pass


class TestClass:
    @(pytest.mark.foo())
    def test_method_outer_paren():
        pass
