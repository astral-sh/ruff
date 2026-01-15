from __future__ import annotations

from typing import Annotated

# OK
def okay_fn(
    a: "int", b: "str", c: "list[str]", d: "Annotated[list[str], 'yes']"
) -> list[int]: ...


# Error
def complex_argument(
    a: "list[dict[str | int, list[int]]]", d: "Annotated[dict[str, list[list[int] | str]], 'yes']"
) -> list[int]: ...


# Error
def complex_return(a: int) -> "dict[str, list[dict[str, dict[str, str] | int | None]]]": ...

# Error
variable_with_complex_ann: "dict[str, dict[str, dict[str, dict[str, str]]]]" = {}

def variable_with_complex_ann_in_fn() -> None:
    var: "dict[str, dict[str, dict[str, dict[str, str]]]]" = {}
    print(var)


class ClassWithComplexMemberFunction:
    def __init__(self, bad_arg: "dict[str, dict[str, dict[str, dict[str, str]]]]") -> None: ...
