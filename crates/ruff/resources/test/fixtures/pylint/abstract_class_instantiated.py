import abc
import weakref
from lala import Bala


class GoodClass(metaclass=abc.ABCMeta):
    pass


class SecondGoodClass(metaclass=abc.ABCMeta):
    def test(self):
        """do nothing."""


class ThirdGoodClass(metaclass=abc.ABCMeta):
    """This should not raise the warning."""

    def test(self):
        raise NotImplementedError()


class BadClass(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def test(self):
        """do nothing."""


class SecondBadClass(metaclass=abc.ABCMeta):
    @property
    @abc.abstractmethod
    def test(self):
        """do nothing."""


class ThirdBadClass(SecondBadClass):
    pass


class Structure(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def __iter__(self):
        pass

    @abc.abstractmethod
    def __len__(self):
        pass

    @abc.abstractmethod
    def __contains__(self, _):
        pass

    @abc.abstractmethod
    def __hash__(self):
        pass


class Container(Structure):
    def __contains__(self, _):
        pass


class Sizable(Structure):
    def __len__(self):
        return 42


class Hashable(Structure):
    __hash__ = 42


class Iterator(Structure):
    def keys(self):
        return iter([1, 2, 3])

    __iter__ = keys


class AbstractSizable(Structure):
    @abc.abstractmethod
    def length(self):
        pass

    __len__ = length


class NoMroAbstractMethods(Container, Iterator, Sizable, Hashable):
    pass


class BadMroAbstractMethods(Container, Iterator, AbstractSizable):
    pass


class SomeMetaclass(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def prop(self):
        pass


class FourthGoodClass(SomeMetaclass):
    """Don't consider this abstract if some attributes are
    there, but can't be inferred.
    """

    prop = Bala  # missing


if 1:  # pylint: disable=using-constant-test

    class FourthBadClass(metaclass=abc.ABCMeta):
        def test(self):
            pass

else:

    class FourthBadClass(metaclass=abc.ABCMeta):
        @abc.abstractmethod
        def test(self):
            pass


class FifthBadClass(abc.ABC):
    """
    Check that instantiating a class with `abc.ABCMeta` as ancestor fails if it
    defines abstract methods.
    """

    @abc.abstractmethod
    def test(self):
        pass


def main():
    """do nothing"""
    GoodClass()
    SecondGoodClass()
    ThirdGoodClass()
    FourthGoodClass()
    weakref.WeakKeyDictionary()
    weakref.WeakValueDictionary()
    NoMroAbstractMethods()

    BadMroAbstractMethods()  # [abstract-class-instantiated]
    BadClass()  # [abstract-class-instantiated]
    SecondBadClass()  # [abstract-class-instantiated]
    ThirdBadClass()  # [abstract-class-instantiated]
    FourthBadClass()  # [abstract-class-instantiated]
    FifthBadClass()  # [abstract-class-instantiated]
