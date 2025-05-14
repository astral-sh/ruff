import sys

# Keep asyncio.__all__ updated with any changes to __all__ here
__all__ = ("create_subprocess_exec", "create_subprocess_shell")


if sys.version_info >= (3, 11):
    async def create_subprocess_shell(
    ) -> Process: ...
    async def create_subprocess_exec(
    ) -> Process: ...

else:  # >= 3.9
    async def create_subprocess_shell(

    ) -> Process: ...
    async def create_subprocess_exec(

    ) -> Process: ...
