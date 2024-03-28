def f(a, a): pass

def f2(a, *, a): pass

def f3(a, a=20): pass

def f4(a, *a): pass

def f5(a, *, **a): pass

# TODO(micha): This is inconsistent. All other examples only highlight the argument name.
def f6(a, a: str): pass
