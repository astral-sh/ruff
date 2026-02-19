###
# Errors
###

def foo():
    x = 1
    yield x  # RUF070

def foo():
    x = [1, 2, 3]
    yield from x  # RUF070

def foo():
    for i in range(10):
        x = i * 2
        yield x  # RUF070

def foo():
    if True:
        x = 1
        yield x  # RUF070

def foo():
    with open("foo.txt") as f:
        x = f.read()
        yield x  # RUF070

def foo():
    try:
        x = something()
        yield x  # RUF070
    except Exception:
        pass

def foo():
    x = some_function()
    yield x  # RUF070

def foo():
    x = some_generator()
    yield from x  # RUF070

def foo():
    x = lambda: 1
    yield x  # RUF070

def foo():
    x = (y := 1)
    yield x  # RUF070

def foo():
    x =1
    yield x  # RUF070 (no space after `=`)


###
# Non-errors
###

# Variable used after yield
def foo():
    x = 1
    yield x
    print(x)

# Variable used before yield (two references)
def foo():
    x = 1
    print(x)
    yield x

# Multiple yields of same var
def foo():
    x = 1
    yield x
    yield x

# Annotated variable
def foo():
    x: int
    x = 1
    yield x

# Nonlocal variable
def foo():
    x = 0
    def bar():
        nonlocal x
        x = 1
        yield x

# Global variable
def foo():
    global x
    x = 1
    yield x

# Intervening statement between assign and yield
def foo():
    x = 1
    print("hello")
    yield x

# Augmented assignment
def foo():
    x = 1
    x += 1
    yield x

# Unpacking assignment
def foo():
    x, y = 1, 2
    yield x

# Non-name target (attribute)
def foo():
    self.x = 1
    yield self.x

# Yield with no value
def foo():
    x = 1
    yield

# Multiple assignment targets (e.g. `x = y = 1`)
def foo():
    x = y = 1
    yield x

# Different variable names
def foo():
    x = 1
    yield y

# Cross-scope reference (closure)
def foo():
    x = 1
    def inner():
        print(x)
    yield x

# Cross-scope reference (comprehension)
def foo():
    x = 1
    _ = [i for i in x]
    yield x

# Yield from with non-name value
def foo():
    yield from [1, 2, 3]

# Yield non-name value
def foo():
    yield 1

# Variable used in nested function after yield
def foo():
    x = compute()
    yield x
    def bar():
        return x

# Yield non-name value preceded by assignment
def foo():
    x = 1
    yield x + 1

# Yield from non-name value preceded by assignment
def foo():
    x = [1, 2, 3]
    yield from [1, 2, 3]

# Annotated assignment with value (not a plain assignment)
def foo():
    x: int = 1
    yield x

# Yield expression as assigned value (inlining would produce `yield yield 1`)
def foo():
    x = yield 1
    yield x
