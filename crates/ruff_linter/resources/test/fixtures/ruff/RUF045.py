from dataclasses import InitVar, KW_ONLY, MISSING, dataclass, field
from typing import ClassVar


@dataclass
class C:
    # Errors
    no_annotation = r"foo"
    missing = MISSING
    field = field()

    # No errors
    __slots__ = ("foo", "bar")
    __radd__ = __add__
    _private_attr = 100

    with_annotation: str
    with_annotation_and_default: int = 42
    with_annotation_and_field_specifier: bytes = field()

    class_var_no_arguments: ClassVar = 42
    class_var_with_arguments: ClassVar[int] = 42

    init_var_no_arguments: InitVar = "lorem"
    init_var_with_arguments: InitVar[str] = "ipsum"

    kw_only: KW_ONLY
    tu, ple, [unp, ack, ing] = (0, 1, 2, [3, 4, 5])
    mul, [ti, ple] = (a, ssign), ment =  {1: b"3", "2": 4}, [6j, 5]
