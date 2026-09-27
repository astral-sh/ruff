def f(a):
    nonlocal a

def g(a):
    if True:
        nonlocal a

def h(a):
    def inner():
        nonlocal a

def i(a):
    try:
        nonlocal a
    except Exception:
        pass

def f(a):
    a = 1
    a = 2
    nonlocal a

def f(a):
    class Inner:
        nonlocal a   # ok

def f(a):
    def inner(a):
        nonlocal a

def f(a=1):
    def inner():
        nonlocal a   # ok