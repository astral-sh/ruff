a: int = 1
def f1():
    global a
    a: str = "foo"  # error

b: int = 1
def outer():
    def inner():
        global b
        b: str = "nested"  # error

c: int = 1
def f2():
    global c
    c: list[str] = []  # error

d: int = 1
def f3():
    global d
    d: str  # error

e: int = 1
def f4():
    e: str = "happy"  # okay

global f
f: int = 1  # okay

g: int = 1
global g  # error

class C:
    x: str
    global x  # error

class D:
    global x  # error
    x: str