# flags: --pyi
from typing import Union

@bird
def zoo(): ...

class A: ...
@bar
class B:
    def BMethod(self) -> None: ...
    @overload
    def BMethod(self, arg : List[str]) -> None: ...

class C: ...
@hmm
class D: ...
class E: ...

@baz
def foo() -> None:
    ...

class F (A , C): ...
def spam() -> None: ...

@overload
def spam(arg: str) -> str: ...

var  : int = 1

def eggs() -> Union[str, int]: ...
