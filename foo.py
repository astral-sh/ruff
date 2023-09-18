from typing import overload

from typing_extensions import override


class CannotModify:
    @overload
    def BadName(self, var: int) -> int:  # noqa: N802
        ...

    @overload
    def BadName(self, var: str) -> str:  # noqa: N802
        ...

    def BadName(self, var: int | str) -> int | str:  # noqa: N802
        return var


class MyClass(CannotModify):
    @overload
    @override
    def BadName(self, var: int) -> int:
        ...

    @overload
    @override
    def BadName(self, var: str) -> str:
        ...

    @override
    def BadName(self, var: int | str) -> int | str:
        return var * 2
