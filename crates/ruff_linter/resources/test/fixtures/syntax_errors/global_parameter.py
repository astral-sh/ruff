def f(a):
    global a # error

def g(a):
    if True:
        global a # error

def h(a):
    def inner():
        global a # error

def i(a):
    try:
        global a # error
    except Exception:
        pass
