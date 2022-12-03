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
