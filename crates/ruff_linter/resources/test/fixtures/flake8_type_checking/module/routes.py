from __future__ import annotations

import pathlib  # OK
from datetime import date  # OK

from module.app import app, app_container

@app.get("/path")
def get_path() -> pathlib.Path:
    pass

@app_container.app.put("/date")
def set_date(d: date):
    pass
