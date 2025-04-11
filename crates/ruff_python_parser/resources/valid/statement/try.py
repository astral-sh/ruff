try:
    ...
except:
    ...

try:
    ...
except Exception1 as e:
    ...
except Exception2 as e:
    ...

try:
    ...
except Exception as e:
    ...
except:
    ...
finally:
    ...

try:
    ...
except:
    ...
else:
    ...

try:
    ...
except:
    ...
else:
    ...
finally:
    ...

try:
    ...
finally:
    ...

try:
    ...
else:
    ...
finally:
    ...

try:
    ...
except* GroupA as eg:
    ...
except* ExceptionGroup:
    ...

try:
    raise ValueError(1)
except TypeError as e:
    print(f"caught {type(e)}")
except OSError as e:
    print(f"caught {type(e)}")

try:
    raise ExceptionGroup("eg", [ValueError(1), TypeError(2), OSError(3), OSError(4)])
except* TypeError as e:
    print(f"caught {type(e)} with nested {e.exceptions}")
except* OSError as e:
    print(f"caught {type(e)} with nested {e.exceptions}")

try:
    pass
except "exception":
    pass
except 1:
    pass
except True:
    pass
except 1 + 1:
    pass
except a | b:
    pass
except x and y:
    pass
except await x:
    pass
except lambda x: x:
    pass
except x if True else y:
    pass

if True:
    try:
        pass
    finally:
        pass
# This `else` is not part of the `try` statement, so don't raise an error
else:
    pass
