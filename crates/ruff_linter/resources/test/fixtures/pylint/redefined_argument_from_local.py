
# No Errors

def foo(a):
    for b in range(1):
        ...

def foo(a):
    for a in range(1):  # unused
        ...

def foo(a):
    try:
        ...
    except ValueError as e:
        ...
    except KeyError as a:
        ...

def foo(a):
    with open('foo.py', ) as f, open('bar.py') as a:
        ...

if True:
    def foo(a):
        ...
else:
    for a in range(1):  # diff branch
        print(a)

# Errors

def foo(i):
    for i in range(10):
        print(i)

def foo(e):
    try:
        ...
    except Exception as e:
        print(e)

def foo(f):
    with open('', ) as f:
        print(f)

def foo(a, b):
    with context() as (a, b, c):
        print(a, b, c)

def foo(a, b):
    with context() as [a, b, c]:
        print(a, b, c)

def foo(a):
    def bar(b):
        for a in range(1):
            print(a)

def foo(a):
    def bar(b):
        for b in range(1):
            print(b)

def foo(a=1):
    def bar(b=2):
        for a in range(1):
            print(a)
        for b in range(1):
            print(b)
