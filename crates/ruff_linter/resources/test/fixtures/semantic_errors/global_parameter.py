def f(a):
    global a

def g(a):
    if True:
        global a 

def h(a):
    def inner():
        global a 

def i(a):
    try:
        global a 
    except Exception:
        pass

def f(a):
    a = 1
    global a

def f(a):
    a = 1
    a = 2
    global a

def f(a):
    class Inner:
        global a   # ok