from __future__ import annotations
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import AsyncIterator, List


class Example:
    async def example(self) -> AsyncIterator[List[str]]:
        yield 0
