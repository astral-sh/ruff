from typing import Any, Tuple


def f(
    a,  # type: int
):
    pass


# test type comments
def f(a, b, c, d, e, f, g, h, i):
    # type: (int, int, int, int, int, int, int, int, int) -> None
    pass


def f(
    a,  # type: int
    b,  # type: int
    c,  # type: int
    d,  # type: int
    e,  # type: int
    f,  # type: int
    g,  # type: int
    h,  # type: int
    i,  # type: int
):
    # type: (...) -> None
    pass


def f(
    arg,  # type: int
    *args,  # type: *Any
    default=False,  # type: bool
    **kwargs,  # type: **Any
):
    # type: (...) -> None
    pass


def f(
    a,  # type: int
    b,  # type: int
    c,  # type: int
    d,  # type: int
):
    # type: (...) -> None

    element = 0  # type: int
    another_element = 1  # type: float
    another_element_with_long_name = 2  # type: int
    another_really_really_long_element_with_a_unnecessarily_long_name_to_describe_what_it_does_enterprise_style = (
        3
    )  # type: int
    an_element_with_a_long_value = calls() or more_calls() and more()  # type: bool

    tup = (
        another_element,
        another_really_really_long_element_with_a_unnecessarily_long_name_to_describe_what_it_does_enterprise_style,
    )  # type: Tuple[int, int]

    a = (
        element
        + another_element
        + another_element_with_long_name
        + element
        + another_element
        + another_element_with_long_name
    )  # type: int


def f(
    x,  # not a type comment
    y,  # type: int
):
    # type: (...) -> None
    pass


def f(
    x,  # not a type comment
):  # type: (int) -> None
    pass


def func(
    a=some_list[0],  # type: int
):  # type: () -> int
    c = call(
        0.0123,
        0.0456,
        0.0789,
        0.0123,
        0.0456,
        0.0789,
        0.0123,
        0.0456,
        0.0789,
        a[-1],  # type: ignore
    )

    c = call(
        "aaaaaaaa", "aaaaaaaa", "aaaaaaaa", "aaaaaaaa", "aaaaaaaa", "aaaaaaaa", "aaaaaaaa"  # type: ignore
    )


result = (  # aaa
    "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
)

AAAAAAAAAAAAA = [AAAAAAAAAAAAA] + SHARED_AAAAAAAAAAAAA + USER_AAAAAAAAAAAAA + AAAAAAAAAAAAA  # type: ignore

call_to_some_function_asdf(
    foo,
    [AAAAAAAAAAAAAAAAAAAAAAA, AAAAAAAAAAAAAAAAAAAAAAA, AAAAAAAAAAAAAAAAAAAAAAA, BBBBBBBBBBBB],  # type: ignore
)

aaaaaaaaaaaaa, bbbbbbbbb = map(list, map(itertools.chain.from_iterable, zip(*items)))  # type: ignore[arg-type]
