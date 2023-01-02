import pytest


@pytest.mark.parametrize("param", [1, 2, 3])
def test_always_ok(param):
    ...


@pytest.mark.parametrize("param1,param2", [(1, 2), (3, 4)])
def test_csv(param1, param2):
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
