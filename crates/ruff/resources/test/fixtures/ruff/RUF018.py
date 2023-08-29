# Ok - abstractmethod not imported
class Foo:
    @abstractmethod
    @property
    def foo(self) -> int:
        ...

# Since we ensure that the potentially problematic decorators are imported, let's import them now
from abc import abstractmethod
from collections.abc import Iterator
from contextlib import contextmanager

# RUF018
class Foo:
    @abstractmethod
    @property
    def foo(self) -> int:
        ...

class Bar:
    @contextmanager
    @staticmethod
    def bar() -> Iterator[None]:
        yield None

    def bar_main(self):
        with self.bar():
            ...

# OK
class FooSafe:
    @property
    @abstractmethod
    def foo(self) -> int:
        ...

class BarSafe:
    @staticmethod
    @contextmanager
    def bar() -> Iterator[None]:
        yield None

    def bar_main(self):
        with self.bar():
            ...
