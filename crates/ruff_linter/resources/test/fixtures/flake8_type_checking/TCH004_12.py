from __future__ import annotations

from typing import Any, TYPE_CHECKING, TypeAlias

if TYPE_CHECKING:
    from collections.abc import Callable

AnyCallable: TypeAlias = Callable[..., Any]
