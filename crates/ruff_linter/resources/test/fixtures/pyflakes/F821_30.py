# Regression tests for:
# https://github.com/astral-sh/ruff/issues/10812

from typing import Annotated, Literal, TypedDict


# No errors
single: TypedDict[{"foo": int}]

# Error at `qux`
multiple: TypedDict[{
    "bar": str,
    "baz": list["qux"],
}]

# Error at `dolor`
nested: TypedDict[
    "lorem": TypedDict[{
        "ipsum": "dolor"
    }],
    "sit": Literal["amet"]
]

# Error at `adipiscing`, `eiusmod`, `tempor`
unpack: TypedDict[{
    "consectetur": Annotated["adipiscing", "elit"]
    **{"sed do": str, int: "eiusmod", **tempor}
}]
