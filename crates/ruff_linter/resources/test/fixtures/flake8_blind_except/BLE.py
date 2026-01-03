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

import logging

try:
    pass
except Exception:
    logging.error("...")  # BLE001


try:
    pass
except Exception:
    logging.error("...", exc_info=False)  # BLE001


try:
    pass
except Exception:
    logging.error("...", exc_info=None)  # BLE001


try:
    pass
except Exception:
    logging.exception("...")  # ok


try:
    pass
except Exception:
    logging.error("...", exc_info=True)  # ok


from logging import critical, error, exception

try:
    pass
except Exception:
    error("...")  # BLE001


try:
    pass
except Exception:
    error("...", exc_info=False)  # BLE001


try:
    pass
except Exception:
    error("...", exc_info=None)  # BLE001


try:
    pass
except Exception:
    critical("...")  # BLE001


try:
    pass
except Exception:
    critical("...", exc_info=False)  # BLE001


try:
    pass
except Exception:
    critical("...", exc_info=None)  # BLE001

try:
    pass
except Exception:
    exception("...")  # ok


try:
    pass
except Exception:
    error("...", exc_info=True)  # ok


try:
    pass
except Exception:
    critical("...", exc_info=True)  # ok


try:
    ...
except Exception as e:
    raise ValueError from e

try:
    ...
except Exception as e:
    raise e from ValueError("hello")


try:
    pass
except Exception:
    if True:
        exception("An error occurred")
    else:
        exception("An error occurred")

# Test tuple exceptions
try:
    pass
except (Exception,):
    pass

try:
    pass
except (Exception, ValueError):
    pass

try:
    pass
except (ValueError, Exception):
    pass

try:
    pass
except (ValueError, Exception) as e:
    print(e)

try:
    pass
except (BaseException, TypeError):
    pass

try:
    pass
except (TypeError, BaseException):
    pass

try:
    pass
except (Exception, BaseException):
    pass

try:
    pass
except (BaseException, Exception):
    pass

# Test nested tuples
try:
    pass
except ((Exception, ValueError), TypeError):
    pass

try:
    pass
except (ValueError, (BaseException, TypeError)):
    pass

# Test valid tuple exceptions (should not trigger)
try:
    pass
except (ValueError, TypeError):
    pass

try:
    pass
except (OSError, FileNotFoundError):
    pass

try:
    pass
except (OSError, FileNotFoundError) as e:
    print(e)

try:
    pass
except (Exception, ValueError):
    critical("...", exc_info=True)  # ok

try:
    pass
except (Exception, ValueError):
    raise

try:
    pass
except (Exception, ValueError) as e:
    raise e

# `from None` cause
try:
    pass
except BaseException as e:
    raise e from None


try:
    pass
except Exception as e:
    logging.error("...", exc_info=e)  # ok


try:
    pass
except Exception as e:
    logging.debug("...", exc_info=e)  # ok


try:
    pass
except Exception:
    logging.info("...", exc_info=True)  # ok 

try:
    pass
except Exception as e:
    logging.warn("...", exc_info=e)  # ok


try:
    pass
except Exception:
    logging.warning("...", exc_info=True)  # ok


try:
    pass
except Exception as e:
    logging.log(logging.INFO, "...", exc_info=e)  # ok
