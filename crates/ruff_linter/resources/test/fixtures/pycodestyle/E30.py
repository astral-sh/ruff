"""Fixtures for the errors E301, E302, E303, E304, E305 and E306.
Since these errors are about new lines, each test starts with either "No error" or "# E30X".
Each test's end is signaled by a "# end" line.
There should be no E30X error outside of a test's bound.
"""


# No error
class Class:
    pass
# end


# No error
class Class:
    """Docstring"""
    def __init__(self) -> None:
        pass
# end


# No error
def func():
    pass
# end


# No error
# comment
class Class:
    pass
# end


# No error
# comment
def func():
    pass
# end


# no error
def foo():
    pass


def bar():
    pass


class Foo(object):
    pass


class Bar(object):
    pass
# end


# No error
class Class(object):

    def func1():
        pass

    def func2():
        pass
# end


# No error
class Class(object):

    def func1():
        pass

# comment
    def func2():
        pass
# end


# No error
class Class:

    def func1():
        pass

    # comment
    def func2():
        pass

    # This is a
    # ... multi-line comment

    def func3():
        pass


# This is a
# ... multi-line comment

@decorator
class Class:

    def func1():
        pass

    # comment

    def func2():
        pass

    @property
    def func3():
        pass

# end


# No error
try:
    from nonexistent import Bar
except ImportError:
    class Bar(object):
        """This is a Bar replacement"""
# end


# No error
def with_feature(f):
    """Some decorator"""
    wrapper = f
    if has_this_feature(f):
        def wrapper(*args):
            call_feature(args[0])
            return f(*args)
    return wrapper
# end


# No error
try:
    next
except NameError:
    def next(iterator, default):
        for item in iterator:
            return item
        return default
# end


# No error
def fn():
    pass


class Foo():
    """Class Foo"""

    def fn():

        pass
# end


# No error
# comment
def c():
    pass


# comment


def d():
    pass

# This is a
# ... multi-line comment

# And this one is
# ... a second paragraph
# ... which spans on 3 lines


# Function `e` is below
# NOTE: Hey this is a testcase

def e():
    pass


def fn():
    print()

    # comment

    print()

    print()

# Comment 1

# Comment 2


# Comment 3

def fn2():

    pass
# end


# no error
if __name__ == '__main__':
    foo()
# end


# no error
defaults = {}
defaults.update({})
# end


# no error
def foo(x):
    classification = x
    definitely = not classification
# end


# no error
def bar(): pass
def baz(): pass
# end


# no error
def foo():
    def bar(): pass
    def baz(): pass
# end


# no error
from typing import overload
from typing import Union
# end


# no error
@overload
def f(x: int) -> int: ...
@overload
def f(x: str) -> str: ...
# end


# no error
def f(x: Union[int, str]) -> Union[int, str]:
    return x
# end


# no error
from typing import Protocol


class C(Protocol):
    @property
    def f(self) -> int: ...
    @property
    def g(self) -> str: ...
# end


# no error
def f(
    a,
):
    pass
# end


# no error
if True:
    class Class:
        """Docstring"""

        def function(self):
            ...
# end


# no error
if True:
    def function(self):
        ...
# end


# no error
@decorator
# comment
@decorator
def function():
    pass
# end


# no error
class Class:
    def method(self):
        if True:
            def function():
                pass
# end


# no error
@decorator
async def function(data: None) -> None:
    ...
# end


# no error
class Class:
    def method():
        """docstring"""
        # comment
        def function():
            pass
# end


# no error
try:
    if True:
        # comment
        class Class:
            pass

except:
    pass
# end


# no error
def f():
    def f():
        pass
# end


# no error
class MyClass:
    # comment
    def method(self) -> None:
        pass
# end


# no error
def function1():
    # Comment
    def function2():
        pass
# end


# no error
async def function1():
    await function2()
    async with function3():
        pass
# end


# no error
if (
    cond1




    and cond2
):
    pass
#end


# no error
async def function1():
  await function2()
  async with function3():
    pass
# end


# no error
async def function1():
	await function2()
	async with function3():
		pass
# end


# no error
async def function1():
    await function2()
    async with function3():
        pass
# end


# no error
class Test:
    def a():
        pass
# wrongly indented comment

    def b():
        pass
# end


# no error
def test():
    pass

  # Wrongly indented comment
    pass
# end


# no error
class Foo:
    """Demo."""

    @overload
    def bar(self, x: int) -> int: ...
    @overload
    def bar(self, x: str) -> str: ...
    def bar(self, x: int | str) -> int | str:
        return x
# end


# no error
@overload
def foo(x: int) -> int: ...
@overload
def foo(x: str) -> str: ...
def foo(x: int | str) -> int | str:
    if not isinstance(x, (int, str)):
        raise TypeError
    return x
# end


# no error
def foo(self, x: int) -> int: ...
def bar(self, x: str) -> str: ...
def baz(self, x: int | str) -> int | str:
    return x
# end


# E301
class Class(object):

    def func1():
        pass
    def func2():
        pass
# end


# E301
class Class:

    def fn1():
        pass
    # comment
    def fn2():
        pass
# end


# E301
class Class:
    """Class for minimal repo."""

    columns = []
    @classmethod
    def cls_method(cls) -> None:
        pass
# end


# E301
class Class:
    """Class for minimal repo."""

    def method(cls) -> None:
        pass
    @classmethod
    def cls_method(cls) -> None:
        pass
# end


# E301
class Foo:
    """Demo."""

    @overload
    def bar(self, x: int) -> int: ...
    @overload
    def bar(self, x: str) -> str:
        ...
    def bar(self, x: int | str) -> int | str:
        return x
# end


# E302
"""Main module."""
def fn():
    pass
# end


# E302
import sys
def get_sys_path():
    return sys.path
# end


# E302
def a():
    pass

def b():
    pass
# end


# E302
def a():
    pass

# comment

def b():
    pass
# end


# E302
def a():
    pass

async def b():
    pass
# end


# E302
async  def x():
    pass

async  def x(y: int = 1):
    pass
# end


# E302
def bar():
    pass
def baz(): pass
# end


# E302
def bar(): pass
def baz():
    pass
# end


# E302
def f():
    pass

# comment
@decorator
def g():
    pass
# end


# E302
class Test:

	pass

	def method1():
		return 1


	def method2():
		return 22
# end


# E302
class A:...
class B: ...
# end


# E302
@overload
def fn(a: int) -> int: ...
@overload
def fn(a: str) -> str: ...

def fn(a: int | str) -> int | str:
    ...
# end


# E303
def fn():
    _ = None


    # arbitrary comment

    def inner():  # E306 not expected (pycodestyle detects E306)
        pass
# end


# E303
def fn():
    _ = None


    # arbitrary comment
    def inner():  # E306 not expected (pycodestyle detects E306)
        pass
# end


# E303
print()



print()
# end


# E303:5:1
print()



# comment

print()
# end


# E303:5:5 E303:8:5
def a():
    print()


    # comment


    # another comment

    print()
# end


# E303
#!python



"""This class docstring comes on line 5.
It gives error E303: too many blank lines (3)
"""
# end


# E303
class Class:
    def a(self):
        pass


    def b(self):
        pass
# end


# E303
if True:
    a = 1


    a = 2
# end


# E303
class Test:


    # comment


    # another comment

    def test(self): pass
# end


# E303
class Test:
    def a(self):
        pass

# wrongly indented comment


    def b(self):
        pass
# end


# E303
def fn():
    pass


    pass
# end


# E304
@decorator

def function():
    pass
# end


# E304
@decorator

# comment    E304 not expected
def function():
    pass
# end


# E304
@decorator

# comment  E304 not expected


# second comment  E304 not expected
def function():
    pass
# end


# E305:7:1
def fn():
    print()

    # comment

    # another comment
fn()
# end


# E305
class Class():
    pass

    # comment

    # another comment
a = 1
# end


# E305:8:1
def fn():
    print()

    # comment

    # another comment

try:
    fn()
except Exception:
    pass
# end


# E305:5:1
def a():
    print()

# Two spaces before comments, too.
if a():
    a()
# end


#: E305:8:1
# Example from https://github.com/PyCQA/pycodestyle/issues/400
import stuff


def main():
    blah, blah

if __name__ == '__main__':
    main()
# end


# E306:3:5
def a():
    x = 1
    def b():
        pass
# end


#: E306:3:5
async def a():
    x = 1
    def b():
        pass
# end


#: E306:3:5 E306:5:9
def a():
    x = 2
    def b():
        x = 1
        def c():
            pass
# end


# E306:3:5 E306:6:5
def a():
    x = 1
    class C:
        pass
    x = 2
    def b():
        pass
# end


# E306
def foo():
    def bar():
        pass
    def baz(): pass
# end


# E306:3:5
def foo():
    def bar(): pass
    def baz():
        pass
# end


# E306
def a():
    x = 2
    @decorator
    def b():
        pass
# end


# E306
def a():
    x = 2
    @decorator
    async def b():
        pass
# end


# E306
def a():
    x = 2
    async def b():
        pass
# end


# no error
@overload
def arrow_strip_whitespace(obj: Table, /, *cols: str) -> Table: ...
@overload
def arrow_strip_whitespace(obj: Array, /, *cols: str) -> Array: ...  # type: ignore[misc]
def arrow_strip_whitespace(obj, /, *cols):
    ...
# end
