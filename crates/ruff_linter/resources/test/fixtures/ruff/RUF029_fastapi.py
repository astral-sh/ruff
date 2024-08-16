from fastapi import FastAPI

app = FastAPI()


@app.post("/count")
async def fastapi_route():
    return 1
