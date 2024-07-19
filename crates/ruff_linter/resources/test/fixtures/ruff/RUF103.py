from fastapi import FastAPI, APIRouter, Query, Path, Body, Cookie, Header, File, Form, Depends, Security
from pydantic import BaseModel

app = FastAPI()
router = APIRouter()

# Errors.
@app.get("/items/")
def get_items(
    current_user: User = Depends(get_current_user),
    some_security_param: str = Security(get_oauth2_user),
):
    pass

@app.post("/stuff/")
def do_stuff(
    some_query_param: str | None = Query(default=None),
    some_path_param: str = Path(),
    some_body_param: str = Body("foo"),
    some_cookie_param: str = Cookie(),
    some_header_param: int = Header(default=5),
    some_file_param: UploadFile = File(),
    some_form_param: str = Form(),
):
    # do stuff
    pass

# Shouldn't change

@app.post("/stuff/")
def do_stuff(
    no_default: Body("foo"),
    no_type_annotation = str,
    no_fastapi_default: str = BaseModel(),
):
    pass

# Ok.

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
