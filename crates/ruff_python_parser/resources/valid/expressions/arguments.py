# This only tests the call arguments and not the expression before the opening parenthesis.

# Simple
call()
call(x, y)
call(x, y,)  # Trailing comma
call(x=1, y=2)
call(*x)
call(**x)

# Order
call(x, y=1)
call(x, *y)
call(x, **y)
call(x=1, *y)
call(x=1, **y)
call(*x, **y)
call(*x, y, z)
call(**x, y=1, z=2)
call(*x1, *x2, **y1, **y2)
call(x=1, **y, z=1)

# Keyword expression
call(x=1 if True else 2)
def _(): call(x=await y)
call(x=lambda y: y)
call(x=(y := 1))

# Yield expression
def _():
    call((yield x))
    call((yield from x))

# Named expression
call(x := 1)
call(x := 1 for i in iter)

# Starred expressions
call(*x and y)
call(*x | y)
def _(): call(*await x)
call(*lambda x: x)
call(*x if True else y)

# Double starred
call(**x)
call(**x and y)
def _(): call(**await x)
call(**x if True else y)
def _(): call(**(yield x))
call(**lambda x: x)
