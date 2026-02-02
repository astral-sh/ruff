from typing import Annotated

from fastapi import Depends, FastAPI, Path

app = FastAPI()


# Errors
@app.get("/things/{thing_id}")
async def read_thing(query: str):
    return {"query": query}


@app.get("/books/isbn-{isbn}")
async def read_thing():
    ...


@app.get("/things/{thing_id:path}")
async def read_thing(query: str):
    return {"query": query}


@app.get("/things/{thing_id : path}")
async def read_thing(query: str):
    return {"query": query}


@app.get("/books/{author}/{title}")
async def read_thing(author: str):
    return {"author": author}


@app.get("/books/{author_name}/{title}")
async def read_thing():
    ...


@app.get("/books/{author}/{title}")
async def read_thing(author: str, title: str, /):
    return {"author": author, "title": title}


@app.get("/books/{author}/{title}/{page}")
async def read_thing(
    author: str,
    query: str,
): ...


@app.get("/books/{author}/{title}")
async def read_thing():
    ...


@app.get("/books/{author}/{title}")
async def read_thing(*, author: str):
    ...


@app.get("/books/{author}/{title}")
async def read_thing(hello, /, *, author: str):
    ...


@app.get("/things/{thing_id}")
async def read_thing(
        query: str,
):
    return {"query": query}


@app.get("/things/{thing_id}")
async def read_thing(
        query: str = "default",
):
    return {"query": query}


@app.get("/things/{thing_id}")
async def read_thing(
        *, query: str = "default",
):
    return {"query": query}


@app.get("/books/{name}/{title}")
async def read_thing(*, author: Annotated[str, Path(alias="author_name")], title: str):
    return {"author": author, "title": title}


# OK
@app.get("/things/{thing_id}")
async def read_thing(thing_id: int, query: str):
    return {"thing_id": thing_id, "query": query}


@app.get("/books/isbn-{isbn}")
async def read_thing(isbn: str):
    return {"isbn": isbn}


@app.get("/things/{thing_id:path}")
async def read_thing(thing_id: str, query: str):
    return {"thing_id": thing_id, "query": query}


@app.get("/things/{thing_id : path}")
async def read_thing(thing_id: str, query: str):
    return {"thing_id": thing_id, "query": query}


@app.get("/books/{author}/{title}")
async def read_thing(author: str, title: str):
    return {"author": author, "title": title}


@app.get("/books/{author}/{title}")
async def read_thing(*, author: str, title: str):
    return {"author": author, "title": title}


@app.get("/books/{author}/{title:path}")
async def read_thing(*, author: str, title: str):
    return {"author": author, "title": title}


@app.get("/books/{name}/{title}")
async def read_thing(*, author: Annotated[str, Path(alias="name")], title: str):
    return {"author": author, "title": title}


# Ignored
@app.get("/things/{thing-id}")
async def read_thing(query: str):
    return {"query": query}


@app.get("/things/{thing_id!r}")
async def read_thing(query: str):
    return {"query": query}


@app.get("/things/{thing_id=}")
async def read_thing(query: str):
    return {"query": query}


# https://github.com/astral-sh/ruff/issues/13657
def takes_thing_id(thing_id): ...
def something_else(lorem): ...

from foo import unknown_imported
unknown_not_function = unknown_imported()


### Errors
@app.get("/things/{thing_id}")
async def single(other: Annotated[str, Depends(something_else)]): ...
@app.get("/things/{thing_id}")
async def default(other: str = Depends(something_else)): ...


### No errors
# A parameter with multiple `Depends()` has undefined behaviour.
# https://github.com/astral-sh/ruff/pull/15364#discussion_r1912551710
@app.get("/things/{thing_id}")
async def single(other: Annotated[str, Depends(takes_thing_id)]): ...
@app.get("/things/{thing_id}")
async def double(other: Annotated[str, Depends(something_else), Depends(takes_thing_id)]): ...
@app.get("/things/{thing_id}")
async def double(other: Annotated[str, Depends(takes_thing_id), Depends(something_else)]): ...
@app.get("/things/{thing_id}")
async def default(other: str = Depends(takes_thing_id)): ...
@app.get("/things/{thing_id}")
async def unknown_1(other: str = Depends(unknown_unresolved)): ...
@app.get("/things/{thing_id}")
async def unknown_2(other: str = Depends(unknown_not_function)): ...
@app.get("/things/{thing_id}")
async def unknown_3(other: str = Depends(unknown_imported)): ...


# Class dependencies
from pydantic import BaseModel
from dataclasses import dataclass

class PydanticParams(BaseModel):
    my_id: int


class InitParams:
    def __init__(self, my_id: int):
        self.my_id = my_id


# Errors
@app.get("/{id}")
async def get_id_pydantic_full(
    params: Annotated[PydanticParams, Depends(PydanticParams)],
): ...
@app.get("/{id}")
async def get_id_pydantic_short(params: Annotated[PydanticParams, Depends()]): ...
@app.get("/{id}")
async def get_id_init_not_annotated(params = Depends(InitParams)): ...


# No errors
@app.get("/{my_id}")
async def get_id_pydantic_full(
    params: Annotated[PydanticParams, Depends(PydanticParams)],
): ...
@app.get("/{my_id}")
async def get_id_pydantic_short(params: Annotated[PydanticParams, Depends()]): ...
@app.get("/{my_id}")
async def get_id_init_not_annotated(params = Depends(InitParams)): ...

@app.get("/things/{ thing_id }")
async def read_thing(query: str):
    return {"query": query}


@app.get("/things/{ thing_id : path }")
async def read_thing(query: str):
    return {"query": query}


@app.get("/things/{ thing_id : str }")
async def read_thing(query: str):
    return {"query": query}


# https://github.com/astral-sh/ruff/issues/20680
# These should NOT trigger FAST003 because FastAPI doesn't recognize them as path parameters

# Non-ASCII characters in parameter name
@app.get("/f1/{用户身份}")
async def f1():
    return locals()

# Space in parameter name  
@app.get("/f2/{x: str}")
async def f2():
    return locals()

# Non-ASCII converter
@app.get("/f3/{complex_number:ℂ}")
async def f3():
    return locals()

# Mixed non-ASCII characters
@app.get("/f4/{用户_id}")
async def f4():
    return locals()

# Space in parameter name with converter
@app.get("/f5/{param: int}")
async def f5():
    return locals()

# https://github.com/astral-sh/ruff/issues/20941
@app.get("/imports/{import}")
async def get_import():
    ...

@app.get("/debug/{__debug__}")
async def get_debug():
    ...
