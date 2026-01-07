def function():
    return dependency()

def decorator(fn):
    return fn

@decorator
def dependency():
    return True
