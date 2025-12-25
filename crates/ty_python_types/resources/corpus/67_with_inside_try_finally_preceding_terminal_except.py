def foo():
    try:
        try:
            pass
        except:
            return None
        with x:
            pass
    finally:
        pass
