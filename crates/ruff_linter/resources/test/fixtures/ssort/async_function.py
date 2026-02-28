def function():
    return dependency()

def dependency():
    return True

async def async_function():
    return function()
