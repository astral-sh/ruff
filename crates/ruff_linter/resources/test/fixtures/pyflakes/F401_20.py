import re
from typing import Annotated

type X = Annotated[int, lambda: re.compile("x")]
