from dataclasses import KW_ONLY, MISSING, InitVar, dataclass, field
from typing import Annotated, ClassVar, Final

from somewhere import A


class B:
    class_var_outermost: ClassVar[int]
    class_var_wrapped: Final[ClassVar[int]]
    class_var_inlegally_wrapped: Final[list[ClassVar[int]]]
    class_var_annotated: Annotated[ClassVar[int], 42]
    class_var_invalid_annotated: Annotated[ClassVar[int]]
    class_var.attribute: ClassVar[int]
    class_var[subscript]: ClassVar[int]

    if True:
        class_var_nested: ClassVar[int]


@dataclass
class C(B):
    # Errors
    no_annotation = r"foo"
    missing = MISSING
    field = field()

    class_var_invalid_annotated = 42
    attribute = 42
    subscript = 42
    class_var_inlegally_wrapped = 42

    # No errors
    __slots__ = ("foo", "bar")
    __radd__ = __add__
    _private_attr = 100

    with_annotation: str
    with_annotation_and_default: int = 42
    with_annotation_and_field_specifier: bytes = field()

    class_var_no_arguments: ClassVar = 42
    class_var_with_arguments: ClassVar[int] = 42

    class_var_outermost = 42
    class_var_wrapped = 42
    class_var_annotated = 42
    class_var_nested = 42

    init_var_no_arguments: InitVar = "lorem"
    init_var_with_arguments: InitVar[str] = "ipsum"

    kw_only: KW_ONLY
    tu, ple, [unp, ack, ing] = (0, 1, 2, [3, 4, 5])
    mul, [ti, ple] = (a, ssign), ment =  {1: b"3", "2": 4}, [6j, 5]

    @dataclass
    class D(A):
        class_var_unknown = 42
