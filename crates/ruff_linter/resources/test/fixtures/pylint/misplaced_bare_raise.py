# bare raise is an except block
try:
    pass
except Exception:
    raise

try:
    pass
except Exception:
    if True:
        raise

# bare raise is in a finally block which is in an except block
try:
    raise Exception
except Exception:
    try:
        raise Exception
    finally:
        raise

# bare raise is in an __exit__ method
class ContextManager:
    def __enter__(self):
        return self
    def __exit__(self, *args):
        raise

try:
    raise # [misplaced-bare-raise]
except Exception:
    pass

def f():
    try:
        raise # [misplaced-bare-raise]
    except Exception:
        pass

def g():
    raise # [misplaced-bare-raise]

def h():
    try:
        if True:
            def i():
                raise # [misplaced-bare-raise]
    except Exception:
        pass
    raise # [misplaced-bare-raise]

raise # [misplaced-bare-raise]

try:
    pass
except:
    def i():
        raise # [misplaced-bare-raise]

try:
    pass
except:
    class C:
        raise # [misplaced-bare-raise]

try:
    pass
except:
    pass
finally:
    raise # [misplaced-bare-raise]
