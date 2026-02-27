import pytest


# Error: boolean value without ids
@pytest.mark.parametrize("flag", [True, False])
def test_error_simple(flag):
    ...


# Error: boolean values in tuples without ids
@pytest.mark.parametrize(("x", "flag"), [(1, True), (2, False)])
def test_error_multi_param(x, flag):
    ...


# Error: boolean value in list without ids
@pytest.mark.parametrize("enabled", [True])
def test_error_single_bool(enabled):
    ...


# Error: multiple boolean values without ids
@pytest.mark.parametrize("x", [True, True, False])
def test_error_multiple_bools(x):
    ...


# Error: nested booleans without ids
@pytest.mark.parametrize(
    ("a", "b", "c"),
    [
        (1, True, "foo"),
        (2, False, "bar"),
    ],
)
def test_error_nested(a, b, c):
    ...


# OK: has ids keyword argument
@pytest.mark.parametrize("flag", [True, False], ids=["enabled", "disabled"])
def test_ok_with_ids(flag):
    ...


# OK: no boolean values
@pytest.mark.parametrize("x", [1, 2, 3])
def test_ok_no_bools(x):
    ...


# OK: strings, not booleans
@pytest.mark.parametrize("x", ["True", "False"])
def test_ok_strings(x):
    ...


# OK: multiple params, no booleans
@pytest.mark.parametrize(("x", "y"), [(1, 2), (3, 4)])
def test_ok_multi_no_bools(x, y):
    ...


# OK: has ids even with nested booleans
@pytest.mark.parametrize(
    ("x", "flag"),
    [(1, True), (2, False)],
    ids=["case1", "case2"],
)
def test_ok_nested_with_ids(x, flag):
    ...
