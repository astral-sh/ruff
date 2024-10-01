# pylint: disable=missing-docstring, too-few-public-methods
from abc import ABCMeta, abstractmethod


class Cls:
    @property
    def attribute(self, param, param1):  # [property-with-parameters]
        return param + param1

    @property
    def attribute_keyword_only(self, *, param, param1):  # [property-with-parameters]
        return param + param1

    @property
    def attribute_positional_only(self, param, param1, /):  # [property-with-parameters]
        return param + param1


class MyClassBase(metaclass=ABCMeta):
    """MyClassBase."""

    @property
    @abstractmethod
    def example(self):
        """Getter."""

    @example.setter
    @abstractmethod
    def example(self, value):
        """Setter."""


class VariadicParameters:
    @property
    def attribute_var_args(self, *args):  # [property-with-parameters]
        return sum(args)

    @property
    def attribute_var_kwargs(self, **kwargs):  #[property-with-parameters]
        return {key: value * 2 for key, value in kwargs.items()}


from functools import cached_property


class Cached:
    @cached_property
    def cached_prop(self, value):  # [property-with-parameters]
        ...


import abc
import enum
import types


class Baz:
    @abc.abstractproperty
    def prop2(self, param) -> None:  # [property-with-parameters]
        return None

    @types.DynamicClassAttribute
    def prop3(self, param) -> None:  # [property-with-parameters]
        return None

    @enum.property
    def prop4(self, param) -> None:  # [property-with-parameters]
        return None
