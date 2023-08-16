import pytest


@pytest.mark.parametrize("x", [1, 1, 2])
def test_error_literal(x):
    ...


a = 1
b = 2
c = 3


@pytest.mark.parametrize("x", [a, a, b, b, b, c])
def test_error_expr_simple(x):
    ...


@pytest.mark.parametrize("x", [(a, b), (a, b), (b, c)])
def test_error_expr_complex(x):
    ...


@pytest.mark.parametrize("x", [1, 2])
def test_ok(x):
    ...
