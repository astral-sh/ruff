from __future__ import annotations

from datetime import date

from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


class Birthday(DeclarativeBase):

    __tablename__ = "birthday"
    id: Mapped[int] = mapped_column(primary_key=True)
    day: Mapped[date]
