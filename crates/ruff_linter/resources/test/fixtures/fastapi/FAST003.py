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
async def double(other: Annotated[str, Depends(something_else), Depends(takes_thing_id)]): ...
@app.get("/things/{thing_id}")
async def default(other: str = Depends(something_else)): ...


### No errors
@app.get("/things/{thing_id}")
async def single(other: Annotated[str, Depends(takes_thing_id)]): ...
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
