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
