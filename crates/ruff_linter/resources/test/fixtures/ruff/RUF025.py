# Violation cases: RUF025

def foo():
    numbers = [1,2,3]
    {n: None for n in numbers} # RUF025

def foo():
    for key, value in {n: 1 for n in [1,2,3]}.items(): # RUF025
        pass

def foo():
    {n: 1.1 for n in [1,2,3]} # RUF025

def foo():
    {n: complex(3, 5) for n in [1,2,3]} # RUF025

def foo():
    def f(data):
        return data
    f({c: "a" for c in "12345"}) # RUF025

def foo():
    {n: True for n in [1,2,2]} # RUF025

def foo():
    {n: b'hello' for n in (1,2,2)} # RUF025

def foo():
    {n: ... for n in [1,2,3]} # RUF025

def foo():
    values = ["a", "b", "c"]
    [{n: values for n in [1,2,3]}] # RUF025

def foo():
    def f():
        return 1
    
    a = f()
    {n: a for n in [1,2,3]} # RUF025

def foo():
    def f():
        return {n: 1 for c in [1,2,3,4,5] for n in [1,2,3]} # RUF025
    
    f()


# Non-violation cases: RUF025

def foo():
    {n: 1 for n in [1,2,3,4,5] if n < 3} # OK

def foo():
    {n: 1 for c in [1,2,3,4,5] for n in [1,2,3] if c < 3} # OK

def foo():
    def f():
        pass
    {n: f() for n in [1,2,3]} # OK
