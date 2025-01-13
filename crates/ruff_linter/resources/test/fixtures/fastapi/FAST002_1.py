"""Test that FAST002 doesn't suggest invalid Annotated fixes with default
values. See #15043 for more details."""

from fastapi import FastAPI, Query

app = FastAPI()


@app.get("/test")
def handler(echo: str = Query("")):
    return echo


@app.get("/test")
def handler2(echo: str = Query(default="")):
    return echo


@app.get("/test")
def handler3(echo: str = Query("123", min_length=3, max_length=50)):
    return echo
