def foo():
    ...

# Flagged
def f():
    a, b = foo()
    print(a)

def f():
    (a, b) = foo() # Testing with parentheses
    print(a)

def f():
    a, b = c, d = foo()
    print(a)


# Not flagged
def f():
    a, b = foo() # All variables are not used hence it is left to rule F841

def f():
    a, b = foo()
    print(a)
    print(b)

def f():
    for e, f in foo(): # Dealt with by B007
        print(e)

def f():

    match foo(): # Not flagged because doesn't count as tuple unpacking?
        case (e, f):
            print(e)

def f():
    locals() # Locals used so we skip because of potential dynamic variable usage
    a, b = foo()
    print(a)


def f():
    d, _e = foo()
    print(d)


def f():
    f, _ = foo()
    print(f)

def f():
    _i, _j = foo()

def f():
    a, b = (1, 2)
    print(a)

def f():
    a, b = c, d = foo()
