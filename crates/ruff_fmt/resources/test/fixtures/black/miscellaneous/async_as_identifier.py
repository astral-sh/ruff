def async():
    pass


def await():
    pass


await = lambda: None
async = lambda: None
async()
await()


def sync_fn():
    await = lambda: None
    async = lambda: None
    async()
    await()


async def async_fn():
    await async_fn()


# output
def async():
    pass


def await():
    pass


await = lambda: None
async = lambda: None
async()
await()


def sync_fn():
    await = lambda: None
    async = lambda: None
    async()
    await()


async def async_fn():
    await async_fn()
