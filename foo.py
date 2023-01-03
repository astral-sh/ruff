def f():
    for x in y:
        yield x

def g():
    def f():
        for x in y:
            yield x
