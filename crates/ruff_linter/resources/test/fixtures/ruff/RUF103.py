# Test cases for RUF103: global-nonlocal-skippable

g = 1

# Basic case - should trigger
def f1(b: bool) -> None:
    if b:
        global g
        g = 2  # RUF103: assignment reachable without global declaration
    else:
        g = 3  # RUF103: assignment reachable without global declaration

# No global declaration - should not trigger
def f2(b: bool) -> None:
    if b:
        g = 2  # OK: no global declaration
    else:
        g = 3  # OK: no global declaration

# Global at function start - should not trigger
def f3(b: bool) -> None:
    global g
    if b:
        g = 2  # OK: global declaration comes first
    else:
        g = 3  # OK: global declaration comes first

# Complex control flow - should trigger
def f4(condition: bool, other: bool) -> None:
    if condition:
        if other:
            global g
            g = 10  # RUF103: can be reached without global
        g = 20  # RUF103: can be reached without global
    else:
        g = 30  # RUF103: can be reached without global

# Nested function - should not trigger parent
def f5() -> None:
    global g
    def inner():
        g = 100  # OK: uses parent's global
    inner()

# Multiple variables
x = 1
y = 2

def f6(condition: bool) -> None:
    if condition:
        global x, y
        x = 10  # RUF103
        y = 20  # RUF103
    else:
        x = 30  # RUF103
        # y not used here

# While loop case
def f7(items):
    for item in items:
        if item > 5:
            global g
            g = item  # RUF103: can skip global declaration
        break
    g = 0  # RUF103: can skip global declaration

# Try/except case
def f8():
    try:
        if True:
            global g
            g = 100  # RUF103
    except:
        g = 200  # RUF103: can skip global declaration

# Simple nonlocal case - should trigger
def f9():
    n = 1

    def inner(condition: bool) -> None:
        if condition:
            nonlocal n
            n = 10  # RUF103: Should trigger for nonlocal
        else:
            n = 20  # RUF103: Should trigger for nonlocal

    return inner
