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


@pytest.mark.parametrize(("param1, " "param2, " "param3"), [(1, 2, 3), (4, 5, 6)])
def test_implicit_str_concat_with_parens(param1, param2, param3):
    ...


@pytest.mark.parametrize("param1, " "param2, " "param3", [(1, 2, 3), (4, 5, 6)])
def test_implicit_str_concat_no_parens(param1, param2, param3):
    ...


@pytest.mark.parametrize((("param1, " "param2, " "param3")), [(1, 2, 3), (4, 5, 6)])
def test_implicit_str_concat_with_multi_parens(param1, param2, param3):
    ...


@pytest.mark.parametrize(("param1,param2"), [(1, 2), (3, 4)])
def test_csv_with_parens(param1, param2):
    ...


parametrize = pytest.mark.parametrize(("param1,param2"), [(1, 2), (3, 4)])

@parametrize
def test_not_decorator(param1, param2):
    ...


@pytest.mark.parametrize(argnames=("param1,param2"), argvalues=[(1, 2), (3, 4)])
def test_keyword_arguments(param1, param2):
    ...


@pytest.mark.parametrize(("param",), [(1,), (2,)])
def test_single_element_tuple(param):
    ...


@pytest.mark.parametrize(("param",), [[1], [2]])
def test_single_element_list(param):
    ...


@pytest.mark.parametrize(("param",), [[1], [2]])
def test_single_element_list(param):
    ...


# Unsafe fix
@pytest.mark.parametrize(
    (
        # comment
        "param",
    ),
    [[1], [2]],
)
def test_comment_in_argnames(param):
    ...

# Unsafe fix
@pytest.mark.parametrize(
    ("param",),
    [
        (
            # comment
            1,
        ),
        (2,),
    ],
)
def test_comment_in_argvalues(param):
    ...


# Safe fix
@pytest.mark.parametrize(
    ("param",),
    [
        (1,),
        # comment
        (2,),
    ],
)
def test_comment_between_argvalues_items(param):
    ...


# A fix should be suggested for `argnames`, but not for `argvalues`.
@pytest.mark.parametrize(
    ("param",),
    [
        (1,),
        (2, 3),
    ],
)
def test_invalid_argvalues(param):
    """
    pytest throws the following error for this test:
    ------------------------------------------------
    a.py::test_comment_between_argvalues_items: in "parametrize" the number of names (1):
        ('param',)
    must be equal to the number of values (2):
        (2, 3)
    ------------------------------------------------
    """
    ...
