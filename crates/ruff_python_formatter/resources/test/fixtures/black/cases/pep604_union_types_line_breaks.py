# This has always worked
z= Loooooooooooooooooooooooong | Loooooooooooooooooooooooong | Loooooooooooooooooooooooong | Loooooooooooooooooooooooong

# "AnnAssign"s now also work
z: Loooooooooooooooooooooooong | Loooooooooooooooooooooooong | Loooooooooooooooooooooooong | Loooooooooooooooooooooooong
z: (Short
    | Short2
    | Short3
    | Short4)
z: (int)
z: ((int))


z: Loooooooooooooooooooooooong | Loooooooooooooooooooooooong | Loooooooooooooooooooooooong | Loooooooooooooooooooooooong = 7
z: (Short
    | Short2
    | Short3
    | Short4) = 8
z: (int) = 2.3
z: ((int)) = foo()

# In case I go for not enforcing parantheses, this might get improved at the same time
x = (
    z
    == 9999999999999999999999999999999999999999
    | 9999999999999999999999999999999999999999
    | 9999999999999999999999999999999999999999
    | 9999999999999999999999999999999999999999,
    y
    == 9999999999999999999999999999999999999999
    + 9999999999999999999999999999999999999999
    + 9999999999999999999999999999999999999999
    + 9999999999999999999999999999999999999999,
)

x = (
    z == (9999999999999999999999999999999999999999
    | 9999999999999999999999999999999999999999
    | 9999999999999999999999999999999999999999
    | 9999999999999999999999999999999999999999),
    y == (9999999999999999999999999999999999999999
    + 9999999999999999999999999999999999999999
    + 9999999999999999999999999999999999999999
    + 9999999999999999999999999999999999999999),
)

# handle formatting of "tname"s in parameter list

# remove unnecessary paren
def foo(i: (int)) -> None: ...


# this is a syntax error in the type annotation according to mypy, but it's not invalid *python* code, so make sure we don't mess with it and make it so.
def foo(i: (int,)) -> None: ...

def foo(
    i: int,
    x: Loooooooooooooooooooooooong
    | Looooooooooooooooong
    | Looooooooooooooooooooong
    | Looooooong,
    *,
    s: str,
) -> None:
    pass


@app.get("/path/")
async def foo(
    q: str
    | None = Query(None, title="Some long title", description="Some long description")
):
    pass


def f(
    max_jobs: int
    | None = Option(
        None, help="Maximum number of jobs to launch. And some additional text."
        ),
    another_option: bool = False
    ):
    ...
