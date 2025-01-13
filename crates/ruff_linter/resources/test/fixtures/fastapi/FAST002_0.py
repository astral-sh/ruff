from fastapi import (
    FastAPI,
    APIRouter,
    Query,
    Path,
    Body,
    Cookie,
    Header,
    File,
    Form,
    Depends,
    Security,
)
from pydantic import BaseModel

app = FastAPI()
router = APIRouter()


# Fixable errors

@app.get("/items/")
def get_items(
    current_user: User = Depends(get_current_user),
    some_security_param: str = Security(get_oauth2_user),
):
    pass


@app.post("/stuff/")
def do_stuff(
    some_path_param: str = Path(),
    some_cookie_param: str = Cookie(),
    some_file_param: UploadFile = File(),
    some_form_param: str = Form(),
    some_query_param: str | None = Query(default=None),
    some_body_param: str = Body("foo"),
    some_header_param: int = Header(default=5),
):
    # do stuff
    pass

@app.get("/users/")
def get_users(
    skip: int,
    limit: int,
    current_user: User = Depends(get_current_user),
):
    pass

@app.get("/users/")
def get_users(
    current_user: User = Depends(get_current_user),
    skip: int = 0,
    limit: int = 10,
):
    pass


@app.get("/items/{item_id}")
async def read_items(*, item_id: int = Path(title="The ID of the item to get"), q: str):
    pass

# Non fixable errors

@app.get("/users/")
def get_users(
    skip: int = 0,
    limit: int = 10,
    current_user: User = Depends(get_current_user),
):
    pass


# Unchanged


@app.post("/stuff/")
def do_stuff(
    no_default: Body("foo"),
    no_type_annotation=str,
    no_fastapi_default: str = BaseModel(),
):
    pass


# OK

@app.post("/stuff/")
def do_stuff(
    some_path_param: Annotated[str, Path()],
    some_cookie_param: Annotated[str, Cookie()],
    some_file_param: Annotated[UploadFile, File()],
    some_form_param: Annotated[str, Form()],
    some_query_param: Annotated[str | None, Query()] = None,
    some_body_param: Annotated[str, Body()] = "foo",
    some_header_param: Annotated[int, Header()] = 5,
):
    pass
