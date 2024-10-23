from contextvars import ContextVar

from fastapi import Query
ContextVar("cv", default=Query(None))

from something_else import Depends
ContextVar("cv", default=Depends())
