from typing import Any
import typing


class Bad:
    def __eq__(self, other: Any) -> bool: ...  # Y032
    def __ne__(self, other: typing.Any) -> typing.Any: ...  # Y032


class Good:
  def __eq__(self, other: object) -> bool: ...

  def __ne__(self, obj: object) -> int: ...


class WeirdButFine:
    def __eq__(self, other: Any, strange_extra_arg: list[str]) -> Any: ...
    def __ne__(self, *, kw_only_other: Any) -> bool: ...


class Unannotated:
  def __eq__(self) -> Any: ...
  def __ne__(self) -> bool: ...

