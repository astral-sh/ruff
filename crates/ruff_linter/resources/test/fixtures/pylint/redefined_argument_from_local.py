
# No Errors

def foo(a):
    for b in range(1):
        ...

def foo(a):
    try:
        ...
    except Exception as e:
        ...

def foo(a):
    with open('', ) as f:
        ...

if True:
    def foo(a):
        ...
else:
    for a in range(1):
        ...

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

def foo(a, b):
    with context() as (a, b, c):
        ...

def foo(a, b):
    with context() as [a, b, c]:
        ...

def foo(a):
    def bar(b):
        for a in range(1):
            ...

def foo(a):
    def bar(b):
        for b in range(1):
            ...

def foo(a=1):
    def bar(b=2):
        for a in range(1):
            ...
        for b in range(1):
            ...
