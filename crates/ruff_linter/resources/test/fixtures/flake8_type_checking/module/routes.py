from __future__ import annotations

import pathlib  # OK

from module.app import app1, app2

@app1.get("/path")
def get_path() -> pathlib.Path:
    pass

@app2.put("/pure_path")
def set_pure_path(df: pathlib.PurePath):
    pass
