from fastapi import FastAPI
from pydantic import BaseModel

app = FastAPI()
class Item(BaseModel):
    name: str


@app.post("/items/", response_model=Item)
async def create_item0(item: Item) -> Item:
    return item


async def create_item1(item: Item) -> Item:
    return item


@app("/items/", response_model=Item)
async def create_item2(item: Item) -> Item:
    return item

@cache
async def create_item3(item: Item) -> Item:
    return item


@app.get("/items/", response_model=Item)
async def create_item4(item: Item) -> Item:
    return item


@app.get("/items/", response_model=Item)
@app.post("/items/", response_model=Item)
async def create_item5(item: Item) -> Item:
    return item