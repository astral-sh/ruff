import pytest


@pytest.mark.parametrize("param", [1, 2, 3])
def test_always_ok(param):
    ...


@pytest.mark.parametrize("param1,param2", [(1, 2), (3, 4)])
def test_csv(param1, param2):
    ...


@pytest.mark.parametrize("   param1,   ,    param2   , ", [(1, 2), (3, 4)])
def test_csv_with_whitespace(param1, param2):
    ...


@pytest.mark.parametrize("param1,param2", [(1, 2), (3, 4)])
def test_csv_bad_quotes(param1, param2):
    ...


@pytest.mark.parametrize(("param1", "param2"), [(1, 2), (3, 4)])
def test_tuple(param1, param2):
    ...


@pytest.mark.parametrize(("param1",), [1, 2, 3])
def test_tuple_one_elem(param1, param2):
    ...


@pytest.mark.parametrize(["param1", "param2"], [(1, 2), (3, 4)])
def test_list(param1, param2):
    ...


@pytest.mark.parametrize(["param1"], [1, 2, 3])
def test_list_one_elem(param1, param2):
    ...


@pytest.mark.parametrize([some_expr, another_expr], [1, 2, 3])
def test_list_expressions(param1, param2):
    ...


@pytest.mark.parametrize([some_expr, "param2"], [1, 2, 3])
def test_list_mixed_expr_literal(param1, param2):
    ...
