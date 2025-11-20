"""
Should emit:
RUF066 - on lines 11, 36
"""

import abc


class User:
    @property
    def name(self):
        f"{self.first_name} {self.last_name}"

    @property
    def age(self):
        return 100


class UserMeta(metaclass=abc.ABCMeta):
    @property
    @abc.abstractmethod
    def abstr_prop1(self): ...

    @property
    @abc.abstractmethod
    def abstr_prop2(self):
        """
        A cool docstring
        """

    @property
    def prop1(self):
        return 1

    @property
    def prop2(self):
        50
