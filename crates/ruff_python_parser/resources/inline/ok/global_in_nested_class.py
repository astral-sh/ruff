try: ...
except ImportError:
    x = 1
    class f():
        global x
        x = 2
