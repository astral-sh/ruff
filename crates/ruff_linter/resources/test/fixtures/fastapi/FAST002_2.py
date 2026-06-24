"""Test FAST002 ellipsis handling."""

from fastapi import Body, Cookie, FastAPI, Header, Query

app = FastAPI()


# Cases that should be fixed - ellipsis should be removed


@app.get("/test1")
async def test_ellipsis_query(
    # This should become: param: Annotated[str, Query(description="Test param")]
    param: str = Query(..., description="Test param"),
) -> str:
    return param


@app.get("/test2")
async def test_ellipsis_header(
    # This should become: auth: Annotated[str, Header(description="Auth header")]
    auth: str = Header(..., description="Auth header"),
) -> str:
    return auth


@app.post("/test3")
async def test_ellipsis_body(
    # This should become: data: Annotated[dict, Body(description="Request body")]
    data: dict = Body(..., description="Request body"),
) -> dict:
    return data


@app.get("/test4")
async def test_ellipsis_cookie(
    # This should become: session: Annotated[str, Cookie(description="Session ID")]
    session: str = Cookie(..., description="Session ID"),
) -> str:
    return session


@app.get("/test5")
async def test_simple_ellipsis(
    # This should become: id: Annotated[str, Query()]
    id: str = Query(...),
) -> str:
    return id


@app.get("/test6")
async def test_multiple_kwargs_with_ellipsis(
    # This should become: param: Annotated[str, Query(description="Test", min_length=1, max_length=10)]
    param: str = Query(..., description="Test", min_length=1, max_length=10),
) -> str:
    return param


# Cases with actual default values - these should preserve the default


@app.get("/test7")
async def test_with_default_value(
    # This should become: param: Annotated[str, Query(description="Test")] = "default"
    param: str = Query("default", description="Test"),
) -> str:
    return param


@app.get("/test8")
async def test_with_default_none(
    # This should become: param: Annotated[str | None, Query(description="Test")] = None
    param: str | None = Query(None, description="Test"),
) -> str:
    return param or "empty"


@app.get("/test9")
async def test_mixed_parameters(
    # First param should be fixed with default preserved  
    optional_param: str = Query("default", description="Optional"),
    # Second param should not be fixed because of the preceding default
    required_param: str = Query(..., description="Required"),
    # Third param should be fixed with default preserved
    another_optional_param: int = Query(42, description="Another optional"),
) -> str:
    return f"{required_param}-{optional_param}-{another_optional_param}"
