def f():
    try:
        pass
    except RuntimeError:
        pass
    except Exception:
        pass
    except KeyboardInterrupt:
        pass


def g():
    try:
        pass
    except RuntimeError:
        pass
    except Exception:
        pass
    except KeyboardInterrupt:
        pass
    except BaseException:
        pass
