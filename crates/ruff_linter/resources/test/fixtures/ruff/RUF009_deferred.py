from dataclasses import dataclass 
from typing import TYPE_CHECKING


def default_function() ->list[int]:
    return []

@dataclass()
class A:
    hidden_mutable_default: list[int] = default_function()
    class_variable: typing.ClassVar[list[int]] = default_function()
    another_class_var: ClassVar[list[int]] = default_function()

if TYPE_CHECKING:
    from typing import ClassVar
