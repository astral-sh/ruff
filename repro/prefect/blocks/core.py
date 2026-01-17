from __future__ import annotations

from abc import ABC
from typing import Optional

from pydantic import BaseModel
from typing_extensions import Self

from prefect._internal.compatibility.async_dispatch import async_dispatch
from prefect.client.utilities import inject_client


class Block(BaseModel, ABC):
    @classmethod
    @inject_client
    async def aload(
        cls,
        name: str,
        validate: bool = True,
        client: Optional[object] = None,
    ) -> Self:
        return cls()  # type: ignore

    @classmethod
    @async_dispatch(aload)
    def load(
        cls,
        name: str,
        validate: bool = True,
        client: Optional[object] = None,
    ) -> Self:
        return cls()  # type: ignore
