def bar(): ...  # OK
def foo():
    """foo"""  # OK, strings are handled by another rule

def buzz():
    print("buzz")  # ERROR PYI010

def foo2():
    123  # ERROR PYI010

def bizz():
    x = 123  # ERROR PYI010
