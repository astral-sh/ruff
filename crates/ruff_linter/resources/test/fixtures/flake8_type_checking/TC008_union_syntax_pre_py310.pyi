from typing import Annotated, TypeAlias

a: TypeAlias = 'int | None'  # TC008
b: TypeAlias = 'Annotated[int, 1 | 2]'  # TC008
