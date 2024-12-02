from typing import Any, Literal, overload


class Everything:
    foo = 1

    def __init__(self):
        pass

    def _private(self):
        pass

    def method1(self):
        pass

    def method2(self):
        pass

    def method3(self):
        pass

    def method4(self):
        pass

    def method5(self):
        pass

    def method6(self):
        pass

    def method7(self):
        pass

    def method8(self):
        pass

    def method9(self):
        pass


class Small:
    def __init__(self):
        pass

    def _private(self):
        pass

    def method1(self):
        pass

    def method2(self):
        pass

    def method3(self):
        pass

    def method4(self):
        pass

    def method5(self):
        pass

    def method6(self):
        pass


class SmallWithOverload:
    @overload
    def method1(self, a: Literal[1]) -> None: ...
    @overload
    def method1(self, a: Literal[2]) -> None: ...
    @overload
    def method1(self, a: Literal[3]) -> None: ...
    @overload
    def method1(self, a: Literal[4]) -> None: ...
    @overload
    def method1(self, a: Literal[5]) -> None: ...
    @overload
    def method1(self, a: Literal[6]) -> None: ...
    @overload
    def method1(self, a: Literal[7]) -> None: ...
    @overload
    def method1(self, a: Literal[8]) -> None: ...
    @overload
    def method1(self, a: Literal[9]) -> None: ...

    def method1(self, a: Any) -> None: ...
