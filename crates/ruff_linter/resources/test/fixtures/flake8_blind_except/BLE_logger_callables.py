import sentry_sdk

import my_module
from my_module import report_error


# Configured logger-callables: ["sentry_sdk.capture_exception", "my_module.report_error"]


try:
    pass
except Exception as e:
    sentry_sdk.capture_exception(e)  # ok


try:
    pass
except BaseException as e:
    report_error(e)  # ok


try:
    pass
except BaseException as e:
    my_module.report_error(e)  # ok


try:
    pass
except Exception as e:
    print(e)  # BLE001


try:
    pass
except Exception as e:
    if True:
        sentry_sdk.capture_exception(e)  # ok


try:
    pass
except Exception as e:
    sentry_sdk.set_tag("error", str(e))  # BLE001


try:
    pass
except Exception as e:
    sentry_sdk.capture_exception(e, hint={"exc_info": True})  # ok
