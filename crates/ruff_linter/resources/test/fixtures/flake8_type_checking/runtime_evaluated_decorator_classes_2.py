from __future__ import annotations

import pandas
import pyproj

import fastapi
from fastapi import FastAPI as Api
from example import DecoratingClass

import numpy  # TC002

app1 = fastapi.FastAPI("First application")
app2 = Api("Second application")

decorating_instance = DecoratingClass()


@app1.get("/transformer")
def get_transformer() -> pyproj.Transformer:
    pass

@app2.put("/dataframe")
def set_dataframe(df: pandas.DataFrame):
    pass

@decorating_instance
def foo(x: pandas.DataFrame):
    pass

@DecoratingClass
def bar(x: numpy.ndarray):
    pass
