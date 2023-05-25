#: E301:5:5
class X:

    def a():
        pass
    def b():
        pass
#: E301:6:5
class X:

    def a():
        pass
    # comment
    def b():
        pass
#:


#: E302:2:1
"""Main module."""
def _main():
    pass
#: E302:2:1
import sys
def get_sys_path():
    return sys.path
#: E302:4:1
def a():
    pass

def b():
    pass
#: E302:6:1
def a():
    pass

# comment

def b():
    pass
#:
#: E302:4:1
def a():
    pass

async def b():
    pass
#:

#: E303:5:1
print



print
#: E303:5:1
print



# comment

print
#: E303:5:5 E303:8:5
def a():
    print


    # comment


    # another comment

    print
#:


#: E304:3:1
@decorator

def function():
    pass
#: E303:5:1
#!python



"""This class docstring comes on line 5.
It gives error E303: too many blank lines (3)
"""
#:

#: E305:7:1
def a():
    print

    # comment

    # another comment
a()
#: E305:8:1
def a():
    print

    # comment

    # another comment

try:
    a()
except Exception:
    pass
#: E305:5:1
def a():
    print

# Two spaces before comments, too.
if a():
    a()
#:

#: E306:3:5
def a():
    x = 1
    def b():
        pass
#: E306:3:5
async def a():
    x = 1
    def b():
        pass
#: E306:3:5 E306:5:9
def a():
    x = 2
    def b():
        x = 1
        def c():
            pass
#: E306:3:5 E306:6:5
def a():
    x = 1
    class C:
        pass
    x = 2
    def b():
        pass
#:

#: E305:8:1
# Example from https://github.com/PyCQA/pycodestyle/issues/400
import stuff


def main():
    blah, blah

if __name__ == '__main__':
    main()
# Previously just E272:1:6 E272:4:6
#: E302:4:1 E271:1:6 E271:4:6
async  def x():
    pass

async  def x(y: int = 1):
    pass
#: E704:3:1 E302:3:1
def bar():
    pass
def baz(): pass
#: E704:1:1 E302:2:1
def bar(): pass
def baz():
    pass
#: E704:4:5 E306:4:5
def foo():
    def bar():
        pass
    def baz(): pass
#: E704:2:5 E306:3:5
def foo():
    def bar(): pass
    def baz():
        pass
#: E302:5:1
def f():
    pass

# wat
@hi
def g():
    pass
