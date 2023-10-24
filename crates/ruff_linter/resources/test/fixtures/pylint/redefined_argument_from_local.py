
# Errors

def foo(i):
    for i in range(10):
        ...

def foo(e):
    try:
        ...
    except Exception as e:
        ...

def foo(f):
    with open('', ) as f:
        ...

def foo(a):
    def bar(b):
        for a in range(10):
            ...

def foo(a):
    def bar(d):
        for d in range(10):
            ...

def foo(a):
    def bar(a):
        for a in range(10):
            ...

def foo(a):
    def bar(a):
        """
        There are two ways to show diagnostics in nested function.
        1) show only one diagnostic
        2) show all diagnotics with function info.
        Current way is 1)
        """
        for a in range(1):
            ...


def foo(a=1):
    def bar(b=2):
        for a in range(1):
            ...
        for b in range(1):
            ...
        print(a)  # expected = 1 but actual = 0
        print(b)  # expected = 2 but actual = 0
    bar()

foo()
