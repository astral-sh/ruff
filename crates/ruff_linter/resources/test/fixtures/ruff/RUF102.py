from fastapi import FastAPI, APIRouter
from pydantic import BaseModel

app = FastAPI()
router = APIRouter()
class Item(BaseModel):
    name: str

# Errors.
@app.post("/items/", response_model=Item)
async def create_item(item: Item) -> Item:
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

# Ok.
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

app = None
@app.post("/items/", response_model=Item)
async def create_item(item: Item) -> Item:
    return item