async def f():
    time.time()


async def f():
    time.sleep(0)  # ASYNC101


async def f():
    subprocess.foo(0)


async def f():
    subprocess.run(0)  # ASYNC101


async def f():
    subprocess.call(0)  # ASYNC101


async def f():
    open("foo")  # ASYNC101
    
    
async def f():
    os.fspath("foo")


async def f():
    os.wait(foo)  # ASYNC101
