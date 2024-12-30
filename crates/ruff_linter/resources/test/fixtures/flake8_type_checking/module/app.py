from __future__ import annotations

from typing import TYPE_CHECKING

import fastapi
from fastapi import FastAPI as Api

if TYPE_CHECKING:
    import datetime  # TC004
    from array import array  # TC004

app = fastapi.FastAPI("First application")

class AppContainer:
    app = Api("Second application")

app_container = AppContainer()

@app.put("/datetime")
def set_datetime(value: datetime.datetime):
    pass

@app_container.app.get("/array")
def get_array() -> array:
    pass
