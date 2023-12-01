try:
    raise OSError("blah")
except               * ExceptionGroup as e:
    pass


try:
    async with trio.open_nursery() as nursery:
        # Make two concurrent calls to child()
        nursery.start_soon(child)
        nursery.start_soon(child)
except *ValueError:
    pass

try:
    try:
        raise ValueError(42)
    except:
        try:
            raise TypeError(int)
        except *(Exception):
            pass
        1 / 0
except Exception as e:
    exc = e

try:
    try:
        raise FalsyEG("eg", [TypeError(1), ValueError(2)])
    except \
        *TypeError as e:
        tes = e
        raise
    except  *  ValueError as e:
        ves = e
        pass
except Exception as e:
    exc = e

try:
    try:
        raise orig
    except *(TypeError, ValueError, *OTHER_EXCEPTIONS) as e:
        raise SyntaxError(3) from e
except BaseException as e:
    exc = e

try:
    try:
        raise orig
    except\
        * OSError as e:
        raise TypeError(3) from e
except ExceptionGroup as e:
    exc = e
