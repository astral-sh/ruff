try:
    pass
except ValueError:
    pass
except Exception as e:
    raise e
finally:
    pass


try:
    pass
except BaseException as e:
    raise e
except TypeError:
    pass
else:
    pass


try:
    pass
except Exception as e:
    raise e
except BaseException:
    pass


try:
    pass
except Exception:
    pass
finally:
    try:
        pass
    except BaseException as e:
        raise e


try:
    pass
except Exception as e:
    try:
        raise e
    except BaseException:
        pass


try:
    try:
        pass
    except BaseException as e:
        raise e
except Exception:
    pass


try:
    pass
except Exception as e:
    raise bad
except BaseException:
    pass
