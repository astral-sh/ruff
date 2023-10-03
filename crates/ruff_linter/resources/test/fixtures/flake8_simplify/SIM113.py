
def f():
    # SIM113
    idx = 0
    for x in iterable:
        g(x, idx)
        idx +=1
        h(x)


def f():
    # SIM113
    sum = 0
    idx = 0
    for x in iterable:
        if g(x):
            break
        idx += 1
        sum += h(x, idx)


def f():
    # SIM113
    idx = 0
    for x, y in iterable_tuples():
        g(x)
        h(x, y)
        idx += 1
        
def f():
    # Current SIM113 doesn't catch this yet because for loop
    # doesn't immidiately follow index initialization
    idx = 0
    sum = 0
    for x in iterable:
        sum += h(x, idx)
        idx += 1


def f():
    # Current SIM113 doesn't catch this due to unpacking in
    # in intialization
    sum, idx = 0, 0
    for x in iterable:
        g(x, idx)
        idx +=1
        h(x)


def f():
    # loop doesn't start at zero
    idx = 10
    for x in iterable:
        g(x, idx)
        idx +=1
        h(x)

def f():
    # index doesn't increment by one
    idx = 0
    for x in iterable:
        g(x, idx)
        idx +=2
        h(x)

def f():
    # index increment inside condition
    idx = 0
    for x in iterable:
        if g(x):
            idx += 1
        h(x)
        
def f():
    # Continue in match-case
    idx = 0
    for x in iterable:
        match g(x):
            case 1: h(x)
            case 2: continue
            case _: h(idx)
        idx += 1

def f():
    # Continue inside with clause
    idx = 0
    for x in iterable:
        with context as c:
            if g(x):
                continue
            h(x, idx, c)
        idx += 1


def f():
    # Continue in try block
    idx = 0
    for x in iterable:
        try:
            g(x, idx)
        except:
            h(x)
            continue
        idx += 1