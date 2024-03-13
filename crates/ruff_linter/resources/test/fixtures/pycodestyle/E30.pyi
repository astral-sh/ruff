import json

from typing import Any, Sequence

class MissingCommand(TypeError): ...
class AnoherClass: ...

def a(): ...

@overload
def a(arg: int): ...
 
@overload
def a(arg: int, name: str): ...


def grouped1(): ...
def grouped2(): ...
def grouped3( ): ...


class BackendProxy:
    backend_module: str
    backend_object: str | None
    backend: Any
    
    def grouped1(): ...
    def grouped2(): ...
    def grouped3( ): ...
    @decorated
    
    def with_blank_line(): ...
    
    
    def ungrouped(): ...
a = "test"

def function_def():
    pass
b = "test"


def outer():
     def inner():
         pass
     def inner2():
         pass
         
class Foo: ...
class Bar: ... 
