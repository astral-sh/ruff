from typing import overload


class Foo:
    foo = 12

    def bar(a):
        pass

    def some():
        pass


    @overload
    def overloaded_method(self, a: int) -> str:
        ...

    @overload
    def overloaded_method(self, a: str) -> str:
        """Foo bar documentation."""
        ...

    def overloaded_method(self, a):
        """Foo bar documentation."""
        return str(a)