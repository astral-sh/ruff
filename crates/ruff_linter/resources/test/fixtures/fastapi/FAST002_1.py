"""Test that FAST002 doesn't suggest invalid Annotated fixes with default
values. See #15043 for more details."""

from fastapi import FastAPI, Query

app = FastAPI()


@app.get("/test")
def handler(echo: str = Query("")):
    return echo
