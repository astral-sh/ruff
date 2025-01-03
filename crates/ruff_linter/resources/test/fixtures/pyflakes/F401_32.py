from datetime import datetime
from typing import no_type_check


# No errors

@no_type_check
def f(a: datetime, b: "datetime"): ...
