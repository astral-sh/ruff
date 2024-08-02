from fastapi import FastAPI

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


@app.get("/books/{author}/{title}")
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



# Ignored
@app.get("/things/{thing-id}")
async def read_thing(query: str):
    return {"query": query}