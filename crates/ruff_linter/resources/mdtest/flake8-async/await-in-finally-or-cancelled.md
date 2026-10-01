# `await-in-finally-or-cancelled` (`ASYNC102`)

```toml
preview = true
lint.select = ["ASYNC102"]
```

Cancellation points in `finally`, cancellation-catching exception handlers, and
`__aexit__` methods must be protected by a shielded Trio or AnyIO cancel scope.
The rule only applies when the module contains evidence that it uses Trio or AnyIO.

The reference cases are adapted from `async102.py`, `async102_anyio.py`, `async102_trio.py`, and
`async102_120_py311.py` in the
[flake8-async test suite](https://github.com/python-trio/flake8-async/tree/c695f61dd9e237375c197f20f3a3eb41885b4eca/tests/eval_files).
The last file also covers ASYNC120, but this suite selects only ASYNC102.

## Shielded cancel scopes

### Shield state in `finally`

Ruff recognizes a shield passed to a Trio timeout scope or enabled through the bound scope object.
Changing that object later updates the protection for subsequent cancellation points.

```py
import trio


async def shield_state():
    try:
        ...
    finally:
        with trio.move_on_after(30, shield=True) as s:
            await cleanup()

    try:
        ...
    finally:
        await cleanup()  # snapshot: await-in-finally-or-cancelled

    try:
        ...
    finally:
        with trio.move_on_after(30):
            await cleanup()  # error: [await-in-finally-or-cancelled]

    try:
        ...
    finally:
        with trio.move_on_after(30) as s:
            s.shield = True
            await cleanup()
            s.shield = False
            await cleanup()  # error: [await-in-finally-or-cancelled]
            s.shield = True
            await cleanup()
```

```snapshot
error[ASYNC102]: Cancellation point in `finally` must be protected by a shielded cancel scope
  --> src/mdtest_snippet.py:14:9
   |
14 |         await cleanup()  # snapshot: await-in-finally-or-cancelled
   |         ^^^^^^^^^^^^^^^
```

### Cancel-scope boundaries

Only a truthy literal or a tracked assignment enables shielding. An ordinary context manager,
an unshielded cancel scope, and a shield value that Ruff cannot determine leave cleanup exposed.
An outer shield also does not implicitly protect a nested cleanup context.

```py
import trio


async def cancel_scope_boundaries():
    try:
        ...
    finally:
        with open("bar"):
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with trio.CancelScope(shield=True):
            await cleanup()
        with trio.CancelScope():
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with trio.CancelScope(shield=(1 == 1)):
            await cleanup()  # error: [await-in-finally-or-cancelled]
        shield = True
        with trio.CancelScope() as scope:
            scope.shield = shield
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with trio.CancelScope(shield=True):
            with trio.move_on_after(30):
                await cleanup()

    with trio.CancelScope(shield=True):
        try:
            ...
        finally:
            await cleanup()  # error: [await-in-finally-or-cancelled]
```

Literal shield values use their truthiness, both in constructors and in assignments.

```py
async def literal_shield_values():
    try:
        ...
    finally:
        with trio.CancelScope(shield=False):
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with trio.CancelScope(shield=1):
            await cleanup()
        with trio.CancelScope(shield=0):
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with trio.CancelScope() as scope:
            scope.shield = 1
            await cleanup()
            scope.shield = None
            await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Cancel-scope resolution

Module aliases and directly imported Trio and AnyIO cancel-scope helpers are recognized.

```py
import anyio as aio
import trio as t
from anyio import CancelScope as Shield
from trio import fail_at, move_on_at, fail_after
```

```py
async def cancel_scope_aliases():
    try:
        ...
    finally:
        with Shield(shield=True):
            await cleanup()
        with t.CancelScope(shield=True):
            await cleanup()
        with aio.move_on_after(10, shield=True):
            await cleanup()
        with aio.fail_after(10, shield=True):
            await cleanup()
        with fail_at(10, shield=True):
            await cleanup()
        with move_on_at(10, shield=True):
            await cleanup()
        with fail_after(10, shield=True):
            await cleanup()
```

### Multiple cancel scopes

A shield enabled on the first cancel scope protects cleanup even when another context manager
shares the `with` statement.

```py
import trio


async def multiple_context_managers():
    try:
        ...
    finally:
        with trio.move_on_after(30) as s, trio.fail_after(5):
            s.shield = True
            await cleanup()
        with trio.move_on_after(30) as s, trio.fail_after(5):
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with open(""), trio.CancelScope(deadline=30, shield=True):
            await cleanup()
```

### Nested cancel scopes

Nested cancel scopes update shielding only for their own lexical lifetime.

```py
import trio


async def nested_cancel_scopes():
    try:
        ...
    except:
        with trio.CancelScope(deadline=10) as cs1:
            with trio.CancelScope(deadline=10) as cs2:
                await cleanup()  # error: [await-in-finally-or-cancelled]
                cs1.shield = True
                await cleanup()
                cs1.shield = False
                await cleanup()  # error: [await-in-finally-or-cancelled]
                cs2.shield = True
                await cleanup()
            await cleanup()  # error: [await-in-finally-or-cancelled]
            cs2.shield = True
            await cleanup()  # error: [await-in-finally-or-cancelled]
            cs1.shield = True
            await cleanup()
```

### Assignments to enclosing cancel scopes

Assignments to an outer scope inside a nested `with` remain effective after the inner scope exits.

```py
from anyio import CancelScope as Shield


async def outer_scope_assignment():
    try:
        ...
    finally:
        with Shield() as outer:
            with Shield():
                outer.shield = True
            await cleanup()
```

### Local imports and shadowing

Local aliases are recognized until rebinding shadows the imported symbol.

```py
import anyio


async def local_import_and_shadowing():
    from anyio import CancelScope as LocalShield

    try:
        ...
    finally:
        with LocalShield(shield=True):
            await cleanup()
        LocalShield = arbitrary
        with LocalShield(shield=True):
            await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Malformed cancel-scope expressions

These expressions are syntactically valid but fail at runtime: the timeout call has invalid
arguments, and `trio.CancelScope` is not an async context manager. They ensure Ruff still handles
malformed code consistently. In the `async with` case, entering the context is a cancellation point
before its apparent shield could apply.

```py
import trio


async def malformed_cancel_scopes():
    try:
        ...
    finally:
        with trio.move_on_after():
            await cleanup()  # error: [await-in-finally-or-cancelled]
    try:
        ...
    finally:
        async with trio.CancelScope(  # error: [await-in-finally-or-cancelled]
            deadline=30, shield=True
        ):
            await cleanup()
```

## Cancellation-catching handlers

### Trio and catch-all handlers

`trio.Cancelled`, `BaseException`, and bare handlers can catch cancellation. Once a specific
cancellation handler matches, later handlers remain ordinary code for this rule.

```py
import trio


async def handler_selection():
    try:
        ...
    except ValueError:
        await cleanup()
    except trio.Cancelled:
        await cleanup()  # error: [await-in-finally-or-cancelled]
    except:
        await cleanup()
```

A shielded cancel scope protects cancellation points in cancellation-catching handlers.

```py
async def shielded_handlers():
    try:
        ...
    except trio.Cancelled:
        with trio.CancelScope(shield=True):
            await cleanup()
    except BaseException:
        await cleanup()

    try:
        ...
    except:
        with trio.CancelScope(deadline=30, shield=True):
            await cleanup()
```

`finally` suites are checked independently of adjacent exception handlers.

```py
async def handlers_with_finally():
    try:
        ...
    except BaseException:
        await cleanup()  # error: [await-in-finally-or-cancelled]
    finally:
        await cleanup()  # error: [await-in-finally-or-cancelled]
```

Each cancellation point receives its own diagnostic, including multiple awaits on one line.

```py
async def multiple_awaits():
    try:
        ...
    except BaseException:
        # error: [await-in-finally-or-cancelled]
        # error: [await-in-finally-or-cancelled]
        _ = await cleanup(), await cleanup()
```

### AnyIO cancellation handlers

AnyIO cancellation handlers can be recognized through the module or an imported helper.

```py
import anyio
from anyio import get_cancelled_exc_class as cancelled
```

Calls to `get_cancelled_exc_class()` identify handlers that may catch cancellation for the active
AnyIO backend. Once such a handler matches, later handlers remain ordinary code for this rule.

```py
async def recognized_anyio_handlers():
    try:
        ...
    except anyio.get_cancelled_exc_class():
        await cleanup()  # snapshot: await-in-finally-or-cancelled
    except:
        await cleanup()

    try:
        ...
    except cancelled():
        await cleanup()  # error: [await-in-finally-or-cancelled]
    except:
        await cleanup()
```

```snapshot
error[ASYNC102]: Cancellation point in a cancellation-catching exception handler must be protected by a shielded cancel scope
 --> src/mdtest_snippet.py:7:9
  |
7 |         await cleanup()  # snapshot: await-in-finally-or-cancelled
  |         ^^^^^^^^^^^^^^^
```

A shielded AnyIO cancel scope protects its handler's cancellation points.

```py
async def shielded_anyio_handler():
    try:
        ...
    except anyio.get_cancelled_exc_class():
        with anyio.CancelScope(shield=True):
            await cleanup()
    except BaseException:
        await cleanup()
```

### Handler tuples and cancellation families

A tuple can catch cancellation through any of its members. Trio and asyncio cancellation remain
separate families: catching one does not consume the other. After both are caught, a later
`BaseException` handler cannot catch cancellation again.

```py
import asyncio
import trio as t


async def exception_handlers():
    try:
        ...
    except (ValueError, t.Cancelled):
        await cleanup()  # error: [await-in-finally-or-cancelled]

    try:
        ...
    except t.Cancelled:
        await cleanup()  # error: [await-in-finally-or-cancelled]
    except asyncio.CancelledError:
        await cleanup()  # error: [await-in-finally-or-cancelled]
    except BaseException:
        await cleanup()
```

### Exception groups

```toml
preview = true
target-version = "py311"
lint.select = ["ASYNC102"]
```

Every `except*` subgroup is independent, so a cancellation-catching subgroup does not make
later subgroups safe. An ordinary-exception subgroup is outside this rule's scope even though
ASYNC120 may report it.

```py
import trio


async def exception_groups():
    try:
        ...
    except* ValueError:
        await cleanup()
        raise
    except* BaseException:
        await cleanup()  # error: [await-in-finally-or-cancelled]
    finally:
        await cleanup()  # error: [await-in-finally-or-cancelled]

    try:
        ...
    except* BaseException:
        with trio.move_on_after(30, shield=True):
            await cleanup()
```

Successive cancellation-catching subgroups are both checked.

```py
async def cancellation_subgroups():
    try:
        ...
    except* trio.Cancelled:
        await cleanup()  # error: [await-in-finally-or-cancelled]
    except* BaseException:
        await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Malformed handler expressions

These handler expressions are syntactically valid but fail when Python evaluates them at runtime.
They ensure Ruff only recognizes valid calls to `get_cancelled_exc_class()` and ignores unrelated
lookalikes. Because the malformed expressions are not recognized as cancellation handlers, the
later bare handlers remain cancellation-catching for this static analysis.

```py
import anyio
from anyio import get_cancelled_exc_class as cancelled


async def malformed_anyio_handlers():
    try:
        ...
    except anyio.get_cancelled_exc_class:
        await cleanup()
    except cancelled(1):
        await cleanup()
    except cancelled:
        await cleanup()
    except:
        await cleanup()  # error: [await-in-finally-or-cancelled]

    try:
        ...
    except anyio.foo():
        await cleanup()
    except anyio.Cancelled:
        await cleanup()
    except:
        await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Exception-handler type expressions

Await expressions in exception-handler types are checked before the handler body.

```py
import trio


async def handler_type_checkpoints():
    try:
        ...
    except (BaseException, await exception_type()):  # error: [await-in-finally-or-cancelled]
        await cleanup()  # error: [await-in-finally-or-cancelled]
```

## Cancellation points

### Implicit and explicit cancellation points

Implicit and explicit cancellation points include async context managers, async iteration,
comprehensions, and nested awaits.

```py
import trio as t
from anyio import CancelScope as Shield


async def checkpoints(cm, items):
    try:
        ...
    finally:
        async with cm:  # error: [await-in-finally-or-cancelled]
            pass
        # error: [await-in-finally-or-cancelled]
        # error: [await-in-finally-or-cancelled]
        async with cm, manager():
            pass
        async for item in items:  # error: [await-in-finally-or-cancelled]
            pass
        result = [item async for item in items]  # error: [await-in-finally-or-cancelled]
        deferred = (item async for item in items)
        deferred = (item async for item in await source())  # error: [await-in-finally-or-cancelled]
        # error: [await-in-finally-or-cancelled]
        # error: [await-in-finally-or-cancelled]
        await (await factory())()
        with Shield(shield=await flag()):  # error: [await-in-finally-or-cancelled]
            await cleanup()  # error: [await-in-finally-or-cancelled]
        async with t.open_nursery(), cm:  # error: [await-in-finally-or-cancelled]
            pass
```

### AnyIO task groups

Creating an AnyIO task group is not itself a cancellation point on entry or exit.
Cancellation points in its body still require shielding through its cancel scope.

```py
import anyio


async def task_group_cleanup():
    try:
        ...
    finally:
        async with anyio.create_task_group() as tg:
            await cleanup()  # error: [await-in-finally-or-cancelled]
            tg.cancel_scope.shield = True
            await cleanup()
            tg.cancel_scope.shield = False
            await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Trio nurseries

Opening a Trio nursery is not itself a cancellation point on entry or exit.

```py
import trio


async def nursery_cleanup():
    try:
        ...
    finally:
        async with trio.open_nursery() as nursery:
            nursery.cancel_scope.shield = True
            await cleanup()
```

### Context-manager exit checkpoints

An async context manager can become unshielded between entry and exit. A literal assignment that
disables its surrounding shield causes Ruff to report the exit checkpoint.

```py
import trio


async def disable_shield_before_exit(cm):
    try:
        ...
    finally:
        with trio.CancelScope(shield=True) as scope:
            async with cm:  # error: [await-in-finally-or-cancelled]
                scope.shield = False
```

### Cancellation-safe cleanup operations

Argument-free `aclose()` calls are allowed without resolving the receiver's type. Positional,
keyword, and unpacked arguments prevent this exemption.

```py
import trio


async def aclose_calls(resource):
    try:
        ...
    finally:
        await resource.aclose()
        await resource.child.aclose()
        await resource.aclose(value)  # error: [await-in-finally-or-cancelled]
        await resource.aclose(value=1)  # error: [await-in-finally-or-cancelled]
        await resource.aclose(*args)  # error: [await-in-finally-or-cancelled]
        await resource.aclose(**kwargs)  # error: [await-in-finally-or-cancelled]
```

Qualified AnyIO and Trio `aclose_forcefully()` calls are cancellation-safe, as described in
[flake8-async#446](https://github.com/python-trio/flake8-async/issues/446). Imported aliases are
recognized, but unresolved lookalikes remain cancellation points.

```py
import anyio as aio


async def forceful_close_calls(resource):
    from anyio import aclose_forcefully as force_close

    try:
        ...
    except BaseException:
        await trio.aclose_forcefully(resource)
        await force_close(resource)
        await aclose_forcefully(resource)  # error: [await-in-finally-or-cancelled]
```

The low-level checkpoint exemption requires a recognized name and no arguments.

```py
async def shielded_checkpoint_calls():
    from trio.lowlevel import cancel_shielded_checkpoint as shielded_checkpoint

    try:
        ...
    finally:
        await trio.lowlevel.cancel_shielded_checkpoint()
        await aio.lowlevel.cancel_shielded_checkpoint()
        await shielded_checkpoint()
        await shielded_checkpoint(1)  # error: [await-in-finally-or-cancelled]
        await trio.lowlevel.checkpoint()  # error: [await-in-finally-or-cancelled]
```

An exempt outer call does not protect awaits in its receiver or arguments.

```py
async def nested_safe_calls(resource):
    from anyio import aclose_forcefully as force_close

    try:
        ...
    finally:
        await (await resource()).aclose()  # error: [await-in-finally-or-cancelled]
        await force_close(await resource())  # error: [await-in-finally-or-cancelled]
```

### Assignment-target cancellation points

Await expressions in assignment targets are checked even when the assignment does not update a
tracked shield.

```py
import trio


async def assignment_target_checkpoints(items, cm):
    try:
        ...
    finally:
        (await factory()).field: int = 1  # error: [await-in-finally-or-cancelled]
        (await factory()).field: int  # error: [await-in-finally-or-cancelled]
        for (await factory()).field in items:  # error: [await-in-finally-or-cancelled]
            pass
        # error: [await-in-finally-or-cancelled]
        # error: [await-in-finally-or-cancelled]
        async with cm as (await factory()).field:
            pass
```

## Cleanup context boundaries

### Nested exception handlers

Nested exception handlers preserve and restore the surrounding cleanup context. A handler nested
inside ordinary exception handling can establish a new cancellation-cleanup context of its own.

```py
import trio


async def nested_exception_handlers():
    await cleanup()
    try:
        await cleanup()
    except trio.Cancelled:
        await cleanup()  # error: [await-in-finally-or-cancelled]
        try:
            await cleanup()  # error: [await-in-finally-or-cancelled]
        except trio.Cancelled:
            await cleanup()  # error: [await-in-finally-or-cancelled]
        except:
            await cleanup()  # error: [await-in-finally-or-cancelled]
        await cleanup()  # error: [await-in-finally-or-cancelled]
    except:
        await cleanup()
        try:
            await cleanup()
        except trio.Cancelled:
            await cleanup()  # error: [await-in-finally-or-cancelled]
        except:
            await cleanup()
        await cleanup()
    await cleanup()
```

### Nested finally suites

Nested cleanup contexts diagnose each cancellation point once and do not reuse an outer shield.

```py
from anyio import CancelScope as Shield


async def nested_cleanup():
    try:
        ...
    finally:
        try:
            await cleanup()  # error: [await-in-finally-or-cancelled]
        except ValueError:
            await cleanup()  # error: [await-in-finally-or-cancelled]
        finally:
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield(shield=True):
            try:
                await cleanup()
            except BaseException:
                await cleanup()
            finally:
                await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Shield assignments in nested handlers

A handler nested inside an existing cleanup context retains that context's shields. Disabling
the shield before raising an exception leaves the handler's cancellation point exposed.

```py
import trio


async def nested_handler_assignment():
    try:
        ...
    finally:
        with trio.CancelScope(shield=True) as scope:
            try:
                scope.shield = False
                raise ValueError
            except ValueError:
                await trio.sleep(1)  # error: [await-in-finally-or-cancelled]
```

### Context-manager generators

An async context manager does not make its generator's cleanup implicitly safe, matching the
behavior established by [flake8-async#54](https://github.com/python-trio/flake8-async/issues/54).

```py
from contextlib import asynccontextmanager

import trio


@asynccontextmanager
async def context_manager_cleanup():
    try:
        yield 1
    finally:
        await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Definition boundaries

Defaults are evaluated in the enclosing cleanup context, while nested bodies are independent except
for their own cleanup methods.

```py
import trio


async def definition_boundaries():
    try:
        ...
    finally:
        async def nested(value=await factory()):  # error: [await-in-finally-or-cancelled]
            await cleanup()

        class Nested:
            async def method(self):
                await cleanup()

            async def __aexit__(self, *args):
                await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Asynchronous exit methods

An asynchronous exit method is itself a cleanup context.

```py
import trio


async def __aexit__():
    await cleanup()  # snapshot: await-in-finally-or-cancelled
```

```snapshot
error[ASYNC102]: Cancellation point in `__aexit__` must be protected by a shielded cancel scope
 --> src/mdtest_snippet.py:5:5
  |
5 |     await cleanup()  # snapshot: await-in-finally-or-cancelled
  |     ^^^^^^^^^^^^^^^
```

An `__aexit__` method is checked as an independent cleanup context.

```py
from anyio import CancelScope as Shield


class Manager:
    async def __aexit__(self, *args):
        await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield(shield=True):
            await cleanup()
```

## Rule activation

### Asyncio does not enable the rule

Asyncio uses edge cancellation rather than Trio and AnyIO's level cancellation semantics, so
importing only `asyncio` does not enable this rule.

```py
import asyncio


async def cleanup():
    try:
        await work()
    except asyncio.CancelledError:
        await finish()
    except BaseException:
        await finish()
    finally:
        async with manager:
            async for item in source:
                await finish()


class Manager:
    async def __aexit__(self, *args):
        await finish()
```

### No cancellation library import

Without evidence of Trio or AnyIO, Ruff does not assume level cancellation.

```py
async def cleanup():
    try:
        await work()
    except BaseException:
        await finish()
    finally:
        await finish()


async def __aexit__(*args):
    await finish()
```

## Annotation evaluation

Annotation expressions are evaluated before Python 3.14, while default expressions are evaluated
in every supported version.

### Python 3.13

Annotation and default expressions are evaluated in the enclosing cleanup context on Python 3.13.

```toml
preview = true
target-version = "py313"
lint.select = ["ASYNC102"]
```

```py
import trio


async def cleanup():
    try:
        ...
    finally:
        # error: [await-in-finally-or-cancelled]
        # error: [await-in-finally-or-cancelled]
        async def annotated(value: await factory()) -> await factory():
            await work()

        async def default(value=await factory()):  # error: [await-in-finally-or-cancelled]
            await work()
```

### Python 3.14

Python 3.14 defers annotation evaluation, but still evaluates default expressions immediately.

```toml
preview = true
target-version = "py314"
lint.select = ["ASYNC102"]
```

```py
import trio


async def cleanup():
    try:
        ...
    finally:
        async def annotated(value: await factory()) -> await factory():
            await work()

        async def default(value=await factory()):  # error: [await-in-finally-or-cancelled]
            await work()
```

### Future annotations

With postponed annotation evaluation on Python 3.13, only the default expression is evaluated in
the cleanup context.

```toml
preview = true
target-version = "py313"
lint.select = ["ASYNC102"]
```

```py
from __future__ import annotations

import trio


async def cleanup():
    try:
        ...
    finally:
        async def annotated(value: await factory()) -> await factory():
            await work()

        async def default(value=await factory()):  # error: [await-in-finally-or-cancelled]
            await work()
```
