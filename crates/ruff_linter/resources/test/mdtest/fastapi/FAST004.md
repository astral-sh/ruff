# FAST004

```toml
rules = ["FAST004"]
```

```py
from http import HTTPStatus

from fastapi import APIRouter, FastAPI, HTTPException, status
from fastapi.responses import JSONResponse, RedirectResponse
from starlette import status as starlette_status
from starlette.exceptions import HTTPException as StarletteHTTPException

app = FastAPI()
router = APIRouter()
documented_router = APIRouter(responses={401: {"description": "Unauthorized"}})
wildcard_router = APIRouter(responses={"4XX": {"description": "Client error"}})
five_xx_router = APIRouter(responses={"5XX": {"description": "Server error"}})
default_router = APIRouter(responses={"default": {"description": "Anything"}})
hidden_router = APIRouter(include_in_schema=False)
annotated_router: APIRouter = APIRouter(responses={404: {"description": "Missing"}})


# Violation: literal int raised but not documented.
@app.get("/missing-literal")  # error: [fast-api-undocumented-error-response] "raises HTTP 404"
async def missing_literal():
    raise HTTPException(status_code=404, detail="missing")


# Violation: positional literal int.
@app.get("/missing-positional")  # error: [fast-api-undocumented-error-response] "raises HTTP 404"
async def missing_positional():
    raise HTTPException(404, detail="missing")


# OK: route-level responses documents the raised code.
@app.get("/documented-literal", responses={404: {"description": "Missing"}})
async def documented_literal():
    raise HTTPException(404, detail="missing")


# Violation: HTTPStatus resolves to an error code.
@app.get("/missing-http-status")  # error: [fast-api-undocumented-error-response] "raises HTTP 409"
async def missing_http_status():
    raise HTTPException(status_code=HTTPStatus.CONFLICT, detail="conflict")


# Violation: fastapi.status constant resolves to an error code.
@app.get("/missing-fastapi-status")  # error: [fast-api-undocumented-error-response] "raises HTTP 403"
async def missing_fastapi_status():
    raise HTTPException(status.HTTP_403_FORBIDDEN, detail="forbidden")


# OK: router-level responses documents the code.
@documented_router.get("/router-documented")
async def router_documented():
    raise HTTPException(status_code=401, detail="unauthorized")


# OK: openapi_extra documents the code.
@app.get(
    "/openapi-extra-documented",
    openapi_extra={"responses": {"409": {"description": "Conflict"}}},
)
async def openapi_extra_documented():
    raise HTTPException(status_code=409, detail="conflict")


# OK: wildcard "4XX" covers the code.
@wildcard_router.get("/wildcard-documented")
async def wildcard_documented():
    raise HTTPException(status_code=404, detail="missing")


# OK: include_in_schema=False suppresses the rule.
@app.get("/hidden", include_in_schema=False)
async def hidden():
    raise HTTPException(status_code=404, detail="missing")


# Violation: returned JSONResponse error code is not documented.
@app.get("/missing-json-response")  # error: [fast-api-undocumented-error-response] "raises HTTP 404"
async def missing_json_response():
    return JSONResponse({"detail": "missing"}, status_code=404)


# Violation: 422 always flagged (auto-422 schema is for Pydantic, not user body).
@app.get("/explicit-422")  # error: [fast-api-undocumented-error-response] "raises HTTP 422"
async def explicit_422(item_id: int):
    raise HTTPException(status_code=422, detail="domain validation failed")


# OK: 3xx redirect is out of scope (RedirectResponse default 307, success-ish).
@app.get("/redirect")
async def redirect():
    return RedirectResponse(url="/elsewhere", status_code=307)


# OK: raised code is inside a nested function (we don't descend).
@app.get("/nested-scope")
async def nested_scope():
    def helper():
        raise HTTPException(status_code=500, detail="boom")

    return {"ok": True}


# Violation: raise inside try/except still counts.
@app.get("/raise-in-try")  # error: [fast-api-undocumented-error-response] "raises HTTP 503"
async def raise_in_try():
    try:
        raise HTTPException(status_code=503, detail="degraded")
    except Exception:
        raise


# Violation: starlette's HTTPException is treated the same as fastapi's.
@app.get("/starlette-exception")  # error: [fast-api-undocumented-error-response] "raises HTTP 418"
async def starlette_exception():
    raise StarletteHTTPException(status_code=418, detail="teapot")


# Violation: starlette.status constants resolve too.
@app.get("/starlette-status-const")  # error: [fast-api-undocumented-error-response] "raises HTTP 500"
async def starlette_status_const():
    raise HTTPException(starlette_status.HTTP_500_INTERNAL_SERVER_ERROR)


# OK: "default" wildcard covers any unlisted code.
@default_router.get("/default-wildcard")
async def default_wildcard():
    raise HTTPException(status_code=503, detail="degraded")


# OK: "5XX" wildcard covers 5xx codes.
@five_xx_router.get("/five-xx-wildcard")
async def five_xx_wildcard():
    raise HTTPException(status_code=502, detail="upstream broken")


# OK: integer-as-string key in responses also documents the code.
@app.get("/string-key", responses={"404": {"description": "Missing"}})
async def string_key():
    raise HTTPException(status_code=404, detail="missing")


# Violation: two different codes raised, both missing from responses.
@app.get("/multiple-violations")  # error: [fast-api-undocumented-error-response] "raises HTTP 400, 503"
async def multiple_violations(user_id: int):
    if user_id < 0:
        raise HTTPException(status_code=400, detail="bad id")
    raise HTTPException(status_code=503, detail="unavailable")


# Violation: rule fires on sync routes too.
@app.get("/sync-route")  # error: [fast-api-undocumented-error-response] "raises HTTP 404"
def sync_route():
    raise HTTPException(status_code=404, detail="missing")


# Violation: non-GET methods are also routes (POST in this case).
@app.post("/create")  # error: [fast-api-undocumented-error-response] "raises HTTP 409"
async def create():
    raise HTTPException(status_code=409, detail="already exists")


# OK: non-HTTPException raises are ignored.
@app.get("/raise-value-error")
async def raise_value_error():
    raise ValueError("not an HTTP exception")


# OK: status code passed through a variable cannot be resolved statically.
@app.get("/indirect-status")
async def indirect_status():
    code = 404
    raise HTTPException(status_code=code, detail="missing")


# Violation: include_in_schema=True does not suppress the rule (only False does).
@app.get("/explicit-include-true", include_in_schema=True)  # error: [fast-api-undocumented-error-response] "raises HTTP 404"
async def explicit_include_true():
    raise HTTPException(status_code=404, detail="missing")


# OK: a 2xx code is not an error and should not be flagged.
@app.get("/two-hundred-status")
async def two_hundred_status():
    raise HTTPException(status_code=200, detail="weird but legal")


# Violation: status_code as a positional argument to a Response constructor.
@app.get("/positional-response-status")  # error: [fast-api-undocumented-error-response] "raises HTTP 404"
async def positional_response_status():
    return JSONResponse({"detail": "missing"}, 404)


# OK: an annotated router binding (`router: APIRouter = APIRouter(...)`) still
# documents the code; we should resolve the call site through the annotation.
@annotated_router.get("/annotated-router")
async def annotated_router_route():
    raise HTTPException(status_code=404, detail="missing")


# OK: routers built with include_in_schema=False suppress every route attached
# to them, the same way the decorator-level flag does.
@hidden_router.get("/router-level-hidden")
async def router_level_hidden():
    raise HTTPException(status_code=404, detail="missing")


# Violation: HTTPStatus members also expose `.value` for the int form.
@app.get("/missing-http-status-value")  # error: [fast-api-undocumented-error-response] "raises HTTP 404"
async def missing_http_status_value():
    raise HTTPException(status_code=HTTPStatus.NOT_FOUND.value, detail="missing")


# OK: HTTPStatus.<X>.value used as a documented key is also resolved.
@app.get(
    "/documented-http-status-value",
    responses={HTTPStatus.NOT_FOUND.value: {"description": "Missing"}},
)
async def documented_http_status_value():
    raise HTTPException(status_code=HTTPStatus.NOT_FOUND, detail="missing")


COMMON_RESPONSES = {404: {"description": "Missing"}}


def get_responses():
    return COMMON_RESPONSES


def get_status_code():
    return 404


# OK: responses passed through a variable cannot be resolved statically, so
# assume it may document the raised code rather than reporting a false positive.
@app.get("/indirect-responses", responses=COMMON_RESPONSES)
async def indirect_responses():
    raise HTTPException(status_code=404, detail="missing")


# OK: non-dict responses may be dynamic enough to document the raised code.
@app.get("/non-dict-responses", responses=[])
async def non_dict_responses():
    raise HTTPException(status_code=404, detail="missing")


# OK: dynamic responses may document the raised code.
@app.get("/dynamic-responses", responses=get_responses())
async def dynamic_responses():
    raise HTTPException(status_code=404, detail="missing")


# OK: dict unions may document the raised code.
@app.get("/union-responses", responses={} | COMMON_RESPONSES)
async def union_responses():
    raise HTTPException(status_code=404, detail="missing")


# OK: computed response keys may document the raised code.
@app.get("/computed-response-key", responses={get_status_code(): {"description": "Missing"}})
async def computed_response_key():
    raise HTTPException(status_code=404, detail="missing")


common_responses_router = APIRouter(responses=COMMON_RESPONSES)


# OK: router-level responses passed through a variable are treated the same way.
@common_responses_router.get("/indirect-router-responses")
async def indirect_router_responses():
    raise HTTPException(status_code=404, detail="missing")


# OK: openapi_extra may contain responses through a variable.
@app.get("/indirect-openapi-extra", openapi_extra={"responses": COMMON_RESPONSES})
async def indirect_openapi_extra():
    raise HTTPException(status_code=404, detail="missing")


def get_openapi_extra():
    return {"responses": COMMON_RESPONSES}


# OK: non-dict openapi_extra may document the raised code.
@app.get("/non-dict-openapi-extra", openapi_extra=[])
async def non_dict_openapi_extra():
    raise HTTPException(status_code=404, detail="missing")


# OK: dynamic openapi_extra may document the raised code.
@app.get("/dynamic-openapi-extra", openapi_extra=get_openapi_extra())
async def dynamic_openapi_extra():
    raise HTTPException(status_code=404, detail="missing")


# OK: a dict unpack may document any response code.
@app.get("/unpacked-responses", responses={**COMMON_RESPONSES})
async def unpacked_responses():
    raise HTTPException(status_code=404, detail="missing")


# OK: a dict unpack inside openapi_extra responses may document any response code.
@app.get("/unpacked-openapi-extra-responses", openapi_extra={"responses": {**COMMON_RESPONSES}})
async def unpacked_openapi_extra_responses():
    raise HTTPException(status_code=404, detail="missing")


ROUTE_KWARGS = {"responses": COMMON_RESPONSES}
ROUTER_KWARGS = {"responses": COMMON_RESPONSES}
HIDDEN_KWARGS = {"include_in_schema": False}


# OK: decorator-level keyword unpacking may document the raised code.
@app.get("/indirect-kwargs", **ROUTE_KWARGS)
async def indirect_kwargs():
    raise HTTPException(status_code=404, detail="missing")


indirect_kwargs_router = APIRouter(**ROUTER_KWARGS)


# OK: router-level keyword unpacking may document the raised code.
@indirect_kwargs_router.get("/indirect-router-kwargs")
async def indirect_router_kwargs():
    raise HTTPException(status_code=404, detail="missing")


# OK: keyword unpacking may also suppress schema inclusion.
@app.get("/indirect-hidden-kwargs", **HIDDEN_KWARGS)
async def indirect_hidden_kwargs():
    raise HTTPException(status_code=404, detail="missing")


# Violation: None is known not to document any responses.
@app.get("/none-responses", responses=None)  # error: [fast-api-undocumented-error-response] "raises HTTP 404"
async def none_responses():
    raise HTTPException(status_code=404, detail="missing")


# Violation: None is known not to provide openapi_extra responses.
@app.get("/none-openapi-extra", openapi_extra=None)  # error: [fast-api-undocumented-error-response] "raises HTTP 404"
async def none_openapi_extra():
    raise HTTPException(status_code=404, detail="missing")


# Violation: documenting a non-error HTTPStatus does not cover a raised 404.
@app.get("/http-status-ok-key", responses={HTTPStatus.OK: {"description": "OK"}})  # error: [fast-api-undocumented-error-response] "raises HTTP 404"
async def http_status_ok_key():
    raise HTTPException(status_code=404, detail="missing")


def make_router(**kwargs):
    return APIRouter(**kwargs)


factory_router: APIRouter = make_router()
factory_responses_router: APIRouter = make_router(responses=COMMON_RESPONSES)


# OK: an annotated router initialized by a factory may include default responses.
@factory_router.get("/factory-router")
async def factory_router_route():
    raise HTTPException(status_code=404, detail="missing")


# OK: factory kwargs are not treated as APIRouter constructor kwargs.
@factory_responses_router.get("/factory-responses-router")
async def factory_responses_router_route():
    raise HTTPException(status_code=404, detail="missing")


RESPONSES_EXTRA_KEY = "responses"
NOT_RESPONSES_EXTRA_KEY = "not-responses"


def get_extra_key():
    return "responses"


# OK: a resolvable dynamic openapi_extra key can document responses.
@app.get(
    "/resolved-openapi-extra-key",
    openapi_extra={RESPONSES_EXTRA_KEY: {404: {"description": "Missing"}}},
)
async def resolved_openapi_extra_key():
    raise HTTPException(status_code=404, detail="missing")


# Violation: a dynamic openapi_extra key that resolves away from "responses" cannot document responses.
@app.get(  # error: [fast-api-undocumented-error-response] "raises HTTP 404"
    "/resolved-non-responses-openapi-extra-key",
    openapi_extra={NOT_RESPONSES_EXTRA_KEY: None},
)
async def resolved_non_responses_openapi_extra_key():
    raise HTTPException(status_code=404, detail="missing")


# OK: an unresolvable dynamic openapi_extra key may be "responses".
@app.get(
    "/unresolved-openapi-extra-key",
    openapi_extra={get_extra_key(): None},
)
async def unresolved_openapi_extra_key():
    raise HTTPException(status_code=404, detail="missing")
```
