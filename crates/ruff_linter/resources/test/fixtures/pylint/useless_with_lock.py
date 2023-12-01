import threading
from threading import Lock, RLock, Condition, Semaphore, BoundedSemaphore


with threading.Lock():  # [useless-with-lock]
    ...

with Lock():  # [useless-with-lock]
    ...

with threading.Lock() as this_shouldnt_matter:  # [useless-with-lock]
    ...

with threading.RLock():  # [useless-with-lock]
    ...

with RLock():  # [useless-with-lock]
    ...

with threading.Condition():  # [useless-with-lock]
    ...

with Condition():  # [useless-with-lock]
    ...

with threading.Semaphore():  # [useless-with-lock]
    ...

with Semaphore():  # [useless-with-lock]
    ...

with threading.BoundedSemaphore():  # [useless-with-lock]
    ...

with BoundedSemaphore():  # [useless-with-lock]
    ...

lock = threading.Lock()
with lock:  # this is ok
    ...

rlock = threading.RLock()
with rlock:  # this is ok
    ...

cond = threading.Condition()
with cond:  # this is ok
    ...

sem = threading.Semaphore()
with sem:  # this is ok
    ...

b_sem = threading.BoundedSemaphore()
with b_sem:  # this is ok
    ...
