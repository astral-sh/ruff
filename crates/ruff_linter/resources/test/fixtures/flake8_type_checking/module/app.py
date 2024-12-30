from __future__ import annotations

from typing import TYPE_CHECKING

import fastapi
from fastapi import FastAPI as Api

if TYPE_CHECKING:
    import datetime  # TC004
    from array import array  # TC004

app1 = fastapi.FastAPI("First application")
app2 = Api("Second application")

@app1.put("/datetime")
def set_datetime(value: datetime.datetime):
    pass

@app2.get("/array")
def get_array() -> array:
    pass
