# No Errors
def func(a):
    for b in range(1):
        ...


def func(a):
    try:
        ...
    except ValueError:
        ...
    except KeyError:
        ...


if True:
    def func(a):
        ...
else:
    for a in range(1):
        print(a)


def func(_):
    for _ in range(1):
        ...


# Errors
def func(a):
    for a in range(1):
        ...


def func(i):
    for i in range(10):
        print(i)


def func(e):
    try:
        ...
    except Exception as e:
        print(e)


def func(f):
    with open('', ) as f:
        print(f)


def func(a, b):
    with context() as (a, b, c):
        print(a, b, c)


def func(a, b):
    with context() as [a, b, c]:
        print(a, b, c)


def func(a):
    with open('foo.py', ) as f, open('bar.py') as a:
        ...


def func(a):
    def bar(b):
        for a in range(1):
            print(a)


def func(a):
    def bar(b):
        for b in range(1):
            print(b)


def func(a=1):
    def bar(b=2):
        for a in range(1):
            print(a)
        for b in range(1):
            print(b)
