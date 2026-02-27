# OK: `del` after use in stub file should not trigger F821
class MyClass: ...
def f(cls: MyClass) -> None: ...
del MyClass

# OK: same pattern with a variable
_T = int
x: _T
del _T

# OK: used in base class
class Base: ...
class Child(Base): ...
del Base

# Error: `del` before use should still trigger F821
class EarlyDel: ...
del EarlyDel
def f2(cls: EarlyDel) -> None: ...  # F821

# Error: name that was never defined
def g(x: Undefined) -> None: ...  # F821
