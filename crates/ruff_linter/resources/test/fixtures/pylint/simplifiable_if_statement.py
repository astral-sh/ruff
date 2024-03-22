
def is_flying_animal(an_object):
    # PLR1703
    if isinstance(an_object, Animal) and an_object in FLYING_THINGS:
        is_flying = True
    else:
        is_flying = False


def f():
    # PLR1703
    if a:
        x = True
    else:
        x = False


def f():
    # PLR1703
    if a == b:
        x = True
    else:
        x = False


def f():
    # Not detected right now
    if a:
        x = 1
    elif b:
        x = True
    else:
        x = False


def f():
    if a:
        x = 1
    else:
        # PLR1703
        if b:
            x = False
        else:
            x = True


def f():
    # OK
    if a:
        foo()
        x = True
    else:
        x = False


def f():
    # OK
    if a:
        x = "foo"
    else:
        x = False


def f():
    # PLR1703
    if a:
        x = False
    else:
        x = True


def f():
    # OK
    if a:
        x = False
    else:
        x = False


def f():
    # OK
    if a:
        x = True
    else:
        x = True


def f():
    def bool():
        ...
    # Won't fix
    # PLR1703
    if a:
        x = True
    else:
        x = False


def f():
    # OK
    if a:
        x = True
    else:
        y = False


def f():
    # OK
    if a == b:
        y = True
    else:
        x = y = False


def f():
    if a:
        x = 1
    else:
        # OK
        if b:
            x = False
        else:
            y = True
