"""
Should emit:
B904 - on lines 10, 11, 16, 62, and 64
"""

try:
    raise ValueError
except ValueError:
    if "abc":
        raise TypeError
    raise UserWarning
except AssertionError:
    raise  # Bare `raise` should not be an error
except Exception as err:
    assert err
    raise Exception("No cause here...")
except BaseException as err:
    raise err
except BaseException as err:
    raise some_other_err
finally:
    raise Exception("Nothing to chain from, so no warning here")

try:
    raise ValueError
except ValueError:
    # should not emit, since we are not raising something
    def proxy():
        raise NameError


try:
    from preferred_library import Thing
except ImportError:
    try:
        from fallback_library import Thing
    except ImportError:

        class Thing:
            def __getattr__(self, name):
                # same as the case above, should not emit.
                raise AttributeError


try:
    from preferred_library import Thing
except ImportError:
    try:
        from fallback_library import Thing
    except ImportError:

        def context_switch():
            try:
                raise ValueError
            except ValueError:
                raise


try:
    ...
except Exception as e:
    if ...:
        raise RuntimeError("boom!")
    else:
        raise RuntimeError("bang!")


try:
    ...
except Exception as e:
    match 0:
        case 0:
            raise RuntimeError("boom!")
