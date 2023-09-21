# Bad: This example returns 3!
def foo():
    try:
        1 / 0
        return "1"
    except:
        return "2"
    finally:
        return "3"


# Good
def foo():
    return_value = None
    try:
        1 / 0
        return_value = "1"
    except:
        return_value = "2"
    finally:
        return_value = "3"  # you might also want to check if "except" happened
    return return_value
