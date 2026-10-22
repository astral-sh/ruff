from __future__ import annotations

from datetime import date

from sqlalchemy.orm import Mapped, declared_attr, mapped_column, relationship

TYPE_CHECKING = False
if TYPE_CHECKING:
    from .models import Person


class PeopleMixin:

    @declared_attr
    def created(self) -> Mapped[date]:
        return mapped_column()

    @declared_attr
    def people(self) -> Mapped[list[Person]]:
        return relationship()
