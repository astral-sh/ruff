from typing import Union

from airflow.typing_compat import Literal, Optional


X = Union[Literal[False], Literal["db"]]
y = Optional["Class"]
