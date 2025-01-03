from typing import List, Dict

from fastapi import FastAPI, APIRouter
from pydantic import BaseModel

app = FastAPI()
router = APIRouter()


class Item(BaseModel):
    name: str


# Errors


@app.post("/items/", response_model=Item)
async def create_item(item: Item) -> Item:
    return item


@app.post("/items/", response_model=list[Item])
async def create_item(item: Item) -> list[Item]:
    return item


@app.post("/items/", response_model=List[Item])
async def create_item(item: Item) -> List[Item]:
    return item


@app.post("/items/", response_model=Dict[str, Item])
async def create_item(item: Item) -> Dict[str, Item]:
    return item


@app.post("/items/", response_model=str)
async def create_item(item: Item) -> str:
    return item


@app.get("/items/", response_model=Item)
async def create_item(item: Item) -> Item:
    return item


@app.get("/items/", response_model=Item)
@app.post("/items/", response_model=Item)
async def create_item(item: Item) -> Item:
    return item


@router.get("/items/", response_model=Item)
async def create_item(item: Item) -> Item:
    return item


# OK


async def create_item(item: Item) -> Item:
    return item


@app("/items/", response_model=Item)
async def create_item(item: Item) -> Item:
    return item


@cache
async def create_item(item: Item) -> Item:
    return item


@app.post("/items/", response_model=str)
async def create_item(item: Item) -> Item:
    return item


@app.post("/items/")
async def create_item(item: Item) -> Item:
    return item


@app.post("/items/", response_model=str)
async def create_item(item: Item):
    return item


@app.post("/items/", response_model=list[str])
async def create_item(item: Item) -> Dict[str, Item]:
    return item


@app.post("/items/", response_model=list[str])
async def create_item(item: Item) -> list[str, str]:
    return item


@app.post("/items/", response_model=Dict[str, int])
async def create_item(item: Item) -> Dict[str, str]:
    return item


app = None


@app.post("/items/", response_model=Item)
async def create_item(item: Item) -> Item:
    return item


# Routes might be defined inside functions


def setup_app(app_arg: FastAPI, non_app: str) -> None:
    # Error
    @app_arg.get("/", response_model=str)
    async def get_root() -> str:
        return "Hello World!"

    # Ok
    @non_app.get("/", response_model=str)
    async def get_root() -> str:
        return "Hello World!"
