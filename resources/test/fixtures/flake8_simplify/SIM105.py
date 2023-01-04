def foo():
    pass

try:
    foo()
except ValueError:  # SIM105
    pass

try:
    foo()
except (ValueError, OSError):  # SIM105
    pass

try:
    foo()
except:  # SIM105
    pass

try:
    foo()
except ValueError:
    print('foo')
except OSError:
    pass

try:
    foo()
except ValueError:
    pass
else:
    print('bar')
