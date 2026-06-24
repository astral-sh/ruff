if len('TEST'):  # [PLC1802]
    pass

if not len('TEST'):  # [PLC1802]
    pass

z = []
if z and len(['T', 'E', 'S', 'T']):  # [PLC1802]
    pass

if True or len('TEST'):  # [PLC1802]
    pass

if len('TEST') == 0:  # Should be fine
    pass

if len('TEST') < 1:  # Should be fine
    pass

if len('TEST') <= 0:  # Should be fine
    pass

if 1 > len('TEST'):  # Should be fine
    pass

if 0 >= len('TEST'):  # Should be fine
    pass

if z and len('TEST') == 0:  # Should be fine
    pass

if 0 == len('TEST') < 10:  # Should be fine
    pass

# Should be fine
if 0 < 1 <= len('TEST') < 10:  # [comparison-of-constants]
    pass

if 10 > len('TEST') != 0:  # Should be fine
    pass

if 10 > len('TEST') > 1 > 0:  # Should be fine
    pass

if 0 <= len('TEST') < 100:  # Should be fine
    pass

if z or 10 > len('TEST') != 0:  # Should be fine
    pass

if z:
    pass
elif len('TEST'):  # [PLC1802]
    pass

if z:
    pass
elif not len('TEST'):  # [PLC1802]
    pass

while len('TEST'):  # [PLC1802]
    pass

while not len('TEST'):  # [PLC1802]
    pass

while z and len('TEST'):  # [PLC1802]
    pass

while not len('TEST') and z:  # [PLC1802]
    pass

assert len('TEST') > 0  # Should be fine

x = 1 if len('TEST') != 0 else 2  # Should be fine

f_o_o = len('TEST') or 42  # Should be fine

a = x and len(x)  # Should be fine

def some_func():
    return len('TEST') > 0  # Should be fine

def github_issue_1325():
    l = [1, 2, 3]
    length = len(l) if l else 0  # Should be fine
    return length

def github_issue_1331(*args):
    assert False, len(args)  # Should be fine

def github_issue_1331_v2(*args):
    assert len(args), args  # [PLC1802]

def github_issue_1331_v3(*args):
    assert len(args) or z, args  # [PLC1802]

def github_issue_1331_v4(*args):
    assert z and len(args), args  # [PLC1802]

def github_issue_1331_v5(**args):
    assert z and len(args), args  # [PLC1802]

b = bool(len(z)) # [PLC1802]
c = bool(len('TEST') or 42) # [PLC1802]

def github_issue_1879():

    class ClassWithBool(list):
        def __bool__(self):
            return True

    class ClassWithoutBool(list):
        pass

    class ChildClassWithBool(ClassWithBool):
        pass

    class ChildClassWithoutBool(ClassWithoutBool):
        pass

    assert len(ClassWithBool())
    assert len(ChildClassWithBool())
    assert len(ClassWithoutBool())  # unintuitive?, in pylint: [PLC1802]
    assert len(ChildClassWithoutBool())  # unintuitive?, in pylint: [PLC1802]
    assert len(range(0))  # [PLC1802]
    assert len([t + 1 for t in []])  # [PLC1802]
    # assert len(u + 1 for u in []) generator has no len
    assert len({"1":(v + 1) for v in {}})  # [PLC1802]
    assert len(set((w + 1) for w in set()))  # [PLC1802]


    import numpy
    numpy_array = numpy.array([0])
    if len(numpy_array) > 0:
        print('numpy_array')
    if len(numpy_array):
        print('numpy_array')
    if numpy_array:
        print('b')

    import pandas as pd
    pandas_df = pd.DataFrame()
    if len(pandas_df):
        print("this works, but pylint tells me not to use len() without comparison")
    if len(pandas_df) > 0:
        print("this works and pylint likes it, but it's not the solution intended by PEP-8")
    if pandas_df:
        print("this does not work (truth value of dataframe is ambiguous)")

    def function_returning_list(r):
        if r==1:
            return [1]
        return [2]

    def function_returning_int(r):
        if r==1:
            return 1
        return 2

    def function_returning_generator(r):
        for i in [r, 1, 2, 3]:
            yield i

    def function_returning_comprehension(r):
        return [x+1 for x in [r, 1, 2, 3]]

    def function_returning_function(r):
        return function_returning_generator(r)

    assert len(function_returning_list(z))  # [PLC1802] differs from pylint
    assert len(function_returning_int(z))
    # This should raise a PLC1802 once astroid can infer it
    # See https://github.com/pylint-dev/pylint/pull/3821#issuecomment-743771514
    assert len(function_returning_generator(z))
    assert len(function_returning_comprehension(z))
    assert len(function_returning_function(z))


def github_issue_4215():
    # Test undefined variables
    # https://github.com/pylint-dev/pylint/issues/4215
    if len(undefined_var):  # [undefined-variable]
        pass
    if len(undefined_var2[0]):  # [undefined-variable]
        pass


def f(cond:bool):
    x = [1,2,3]
    if cond:
        x = [4,5,6]
    if len(x): # this should be addressed
        print(x)

def g(cond:bool):
    x = [1,2,3]
    if cond:
        x = [4,5,6]
    if len(x): # this should be addressed
        print(x)
    del x

def h(cond:bool):
    x = [1,2,3]
    x = 123
    if len(x): # ok
        print(x)

def outer():
    x = [1,2,3]
    def inner(x:int):
        return x+1
    if len(x): # [PLC1802]
        print(x)

def redefined():
    x = 123
    x = [1, 2, 3]
    if len(x): # this should be addressed
        print(x)

global_seq = [1, 2, 3]

def i():
    global global_seq
    if len(global_seq): # ok
        print(global_seq)

def j():
    if False:
        x = [1, 2, 3]
    if len(x): # [PLC1802] should be fine
        print(x)

# regression tests for https://github.com/astral-sh/ruff/issues/14690
bool(len(ascii(1)))
bool(len(sorted("")))

# regression tests for https://github.com/astral-sh/ruff/issues/18811
fruits = []
if(len)(fruits):
    ...

# regression tests for https://github.com/astral-sh/ruff/issues/18812
fruits = []
if len(
    fruits  # comment
):
    ...
