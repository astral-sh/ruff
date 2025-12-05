a: int = 1
def f1():
    global a
    a: str = "foo"

b: int = 1
def outer():
    def inner():
        global b
        b: str = "nested"

c: int = 1
def f2():
    global c
    c: list[str] = []

d: int = 1
def f3():
    global d
    d: str

e: int = 1
def f4():
    e: str = "happy"

global f
f: int = 1