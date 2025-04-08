try: ...
except ImportError:
    x = 1
    def f():
        global x
        x = 2
