"""Regression tests for https://github.com/astral-sh/ruff/issues/15812"""


def f():
    from typing import Union

    from typing_extensions import TypeAliasType

    Json = TypeAliasType(
        "Json",
        "Union[dict[str, Json], list[Json], str, int, float, bool, None]",
    )


def f():
    from typing import Union

    from typing_extensions import TypeAliasType, TypeVar

    T = TypeVar("T")
    V = TypeVar("V")
    Json = TypeAliasType(
        "Json",
        "Union[dict[str, Json], list[Json], str, int, float, bool, T, V, None]",
        type_params=(T, V),
    )


def f():
    from typing import Union

    from typing_extensions import TypeAliasType

    Json = TypeAliasType(
        value="Union[dict[str, Json], list[Json], str, int, float, bool, None]",
        name="Json",
    )


# strictly speaking it's a false positive to emit F401 for both of these, but
# we can't really be expected to understand that the strings here are type
# expressions (and type checkers probably wouldn't understand them as type
# expressions either!)
def f():
    from typing import Union

    from typing_extensions import TypeAliasType

    args = [
        "Json",
        "Union[dict[str, Json], list[Json], str, int, float, bool, None]",
    ]

    Json = TypeAliasType(*args)


def f():
    from typing import Union

    from typing_extensions import TypeAliasType

    kwargs = {
        "name": "Json",
        "value": "Union[dict[str, Json], list[Json], str, int, float, bool, None]",
    }

    Json = TypeAliasType(**kwargs)
