class MyException(Exception):
    pass


def bad():
    try:
        a = process()
        if not a:
            raise MyException(a)

        raise MyException(a)

        try:
            b = process()
            if not b:
                raise MyException(b)
        except Exception:
            logger.exception("something failed")
    except Exception:
        logger.exception("something failed")


def good():
    try:
        a = process()  # This throws the exception now
    except MyException:
        logger.exception("a failed")
    except Exception:
        logger.exception("something failed")
