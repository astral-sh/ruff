# This file doesn't use the standard decomposition.
# Decorator syntax test cases are separated by double # comments.
# Those before the 'output' comment are valid under the old syntax.
# Those after the 'ouput' comment require PEP614 relaxed syntax.
# Do not remove the double # separator before the first test case, it allows
# the comment before the test case to be ignored.

##

@decorator
def f():
    ...

##

@decorator()
def f():
    ...

##

@decorator(arg)
def f():
    ...
    
##

@decorator(kwarg=0)
def f():
    ...

##

@decorator(*args)
def f():
    ...

##

@decorator(**kwargs)
def f():
    ...

##

@decorator(*args, **kwargs)
def f():
    ...

##

@decorator(*args, **kwargs,)
def f():
    ...

##

@dotted.decorator
def f():
    ...

##

@dotted.decorator(arg)
def f():
    ...
    
##

@dotted.decorator(kwarg=0)
def f():
    ...

##

@dotted.decorator(*args)
def f():
    ...

##

@dotted.decorator(**kwargs)
def f():
    ...

##

@dotted.decorator(*args, **kwargs)
def f():
    ...

##

@dotted.decorator(*args, **kwargs,)
def f():
    ...

##

@double.dotted.decorator
def f():
    ...

##

@double.dotted.decorator(arg)
def f():
    ...
    
##

@double.dotted.decorator(kwarg=0)
def f():
    ...

##

@double.dotted.decorator(*args)
def f():
    ...

##

@double.dotted.decorator(**kwargs)
def f():
    ...

##

@double.dotted.decorator(*args, **kwargs)
def f():
    ...

##

@double.dotted.decorator(*args, **kwargs,)
def f():
    ...

##

@_(sequence["decorator"])
def f():
    ...

##

@eval("sequence['decorator']")
def f():
    ...

# output

##

@decorator()()
def f():
    ...

##

@(decorator)
def f():
    ...

##

@sequence["decorator"]
def f():
    ...

##

@decorator[List[str]]
def f():
    ...

##

@var := decorator
def f():
    ...