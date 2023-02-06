import pytest


@pytest.mark.parametrize("param", (1, 2))
def test_tuple(param):
    ...


@pytest.mark.parametrize(
    ("param1", "param2"),
    (
        (1, 2),
        (3, 4),
    ),
)
def test_tuple_of_tuples(param1, param2):
    ...


@pytest.mark.parametrize(
    ("param1", "param2"),
    (
        [1, 2],
        [3, 4],
    ),
)
def test_tuple_of_lists(param1, param2):
    ...


@pytest.mark.parametrize("param", [1, 2])
def test_list(param):
    ...


@pytest.mark.parametrize(
    ("param1", "param2"),
    [
        (1, 2),
        (3, 4),
    ],
)
def test_list_of_tuples(param1, param2):
    ...


@pytest.mark.parametrize(
    ("param1", "param2"),
    [
        [1, 2],
        [3, 4],
    ],
)
def test_list_of_lists(param1, param2):
    ...


@pytest.mark.parametrize(
    "param1,param2",
    [
        [1, 2],
        [3, 4],
    ],
)
def test_csv_name_list_of_lists(param1, param2):
    ...


@pytest.mark.parametrize(
    "param",
    [
        [1, 2],
        [3, 4],
    ],
)
def test_single_list_of_lists(param):
    ...
