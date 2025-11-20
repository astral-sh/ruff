"""
Should emit:
RUF066 - on lines 11, 22, 47
"""

import abc


class User:
    @property
    def name(self):  # No return
        f"{self.first_name} {self.last_name}"

    @property
    def age(self):
        return 100

    def method(self):
        x = 1

    @property
    def nested(self):  # No return
        def inner():
            return 0

    @property
    def stub(self): ...


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
    def prop2(self):  # No return
        50

    def method(self):
        x = 1


def func():
    x = 1
