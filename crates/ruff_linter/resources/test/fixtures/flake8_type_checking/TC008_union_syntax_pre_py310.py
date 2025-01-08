from __future__ import annotations

from typing import Annotated, TypeAlias, TYPE_CHECKING

a: TypeAlias = 'int | None'  # OK
b: TypeAlias = 'Annotated[int, 1 | 2]'  # False negative in runtime context

if TYPE_CHECKING:
    c: TypeAlias = 'int | None'  # OK
    d: TypeAlias = 'Annotated[int, 1 | 2]'  # TC008
    e: TypeAlias = 'Annotated[int, 1 + 2]'  # TC008
    f: TypeAlias = 'dict[str, int | None]'  # OK
