from abc import abstractmethod
from typing import TYPE_CHECKING, Any, Protocol, Union

from faststream._internal.broker import BrokerUsecase

if TYPE_CHECKING:
    from faststream._internal.broker import BrokerUsecase
    from faststream.specification.schema import Contact, License


class SpecificationFactory(Protocol):
    title: str
    description: str | None
    version: str | None
    contact: Union["Contact", dict[str, Any]] | None
    license: Union["License", dict[str, Any]] | None

    @abstractmethod
    def add_broker(
        self,
        broker: "BrokerUsecase[Any, Any]",
        /,
    ) -> "SpecificationFactory":
        raise NotImplementedError
