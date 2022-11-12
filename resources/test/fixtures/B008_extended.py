from typing import List

import fastapi
from fastapi import Query


def this_is_okay_extended(db=fastapi.Depends(get_db)):
    ...


def this_is_okay_extended_second(data: List[str] = fastapi.Query(None)):
    ...


def this_is_not_okay_relative_import_not_listed(data: List[str] = Query(None)):
    ...
