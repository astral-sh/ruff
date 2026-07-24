from typing import List

from sqlalchemy import Integer, ForeignKey, String, func
from sqlalchemy.ext.associationproxy import association_proxy, AssociationProxy
from sqlalchemy.orm import (
    Mapped,
    mapped_column,
    DeclarativeBase,
    relationship,
    column_property,
    MappedSQLExpression,
    synonym,
    Synonym,
)


class Base(DeclarativeBase):
    pass


class Parent(Base):
    __tablename__ = "parent"

    id: Mapped[int] = mapped_column(primary_key=True)
    children: Mapped[List["Child"]] = relationship(back_populates="parent")

    name: Mapped[str]

    parent_name: Synonym[str] = synonym("name")


class Child(Base):
    __tablename__ = "child"

    id: Mapped[int] = mapped_column(primary_key=True)
    parent_id: Mapped[int] = mapped_column(ForeignKey("parent.id"))
    parent: Mapped["Parent"] = relationship(back_populates="children")

    parent_name: AssociationProxy[str] = association_proxy("parent", "name")

    first_name: Mapped[str] = mapped_column(String)
    last_name: Mapped[str] = mapped_column()

    name_length: MappedSQLExpression[int] = column_property(
        func.length(first_name + last_name)
    )


class Company(Base):
    __tablename__ = "company"

    id = mapped_column(Integer, primary_key=True)
    employees = relationship("Employee", back_populates="company")

    name = mapped_column(String)
    company_name = synonym("name")


class Employee(Base):
    __tablename__ = "employee"

    id = mapped_column(Integer, primary_key=True)
    company_id = mapped_column(ForeignKey("company.id"))
    company = relationship("Company", back_populates="employees")

    company_name = association_proxy("company", "name")

    first_name = mapped_column(String)
    last_name = mapped_column(String)

    name_length = column_property(func.length(first_name + last_name))
