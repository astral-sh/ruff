###
# Dummy variables with non-ASCII names should be silenced by the default
# dummy-variable-rgx, which is Unicode-aware.
###

def f(previous, _ci):
    return previous


def g(previous, _次):
    return previous


def h(previous, _Ω):
    return previous


def i(previous, _ä):
    return previous


def j(previous, _1a):
    return previous


lambda previous, _ci: previous

lambda previous, _次: previous

lambda previous, _Ω: previous

lambda previous, _ä: previous

lambda previous, _1a: previous


###
# A trailing underscore still disqualifies a name from being a dummy.
###

def k(previous, _a_):
    return previous


def l(previous, __a__):
    return previous


lambda previous, _a_: previous


###
# A name without a leading underscore is not a dummy.
###

def m(previous, a):
    return previous
