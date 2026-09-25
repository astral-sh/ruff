# `await-in-finally-or-cancelled` (`ASYNC102`)

```toml
preview = true
lint.select = ["ASYNC102"]
```

Cancellation points in `finally`, cancellation-catching exception handlers, and
`__aexit__` methods must be protected by a shielded Trio or AnyIO cancel scope.
The rule only applies when the module contains evidence that it uses Trio or AnyIO.

<details>
<summary>License for adapted flake8-async cases</summary>

The reference cases are adapted from `async102.py`, `async102_anyio.py`, `async102_trio.py`, and
`async102_120_py311.py` in the
[flake8-async test suite](https://github.com/python-trio/flake8-async/tree/c695f61dd9e237375c197f20f3a3eb41885b4eca/tests/eval_files).
The last file also covers ASYNC120, but this suite selects only ASYNC102.

MIT License

Copyright (c) 2022 Zac Hatfield-Dodds

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE.

</details>

## Trio-compatible reference behavior

These cases are adapted from the upstream flake8-async fixtures. They cover the shared
behavior available when Trio is imported.

### Shield state in `finally`

Ruff recognizes a shield passed to a Trio timeout scope or enabled through the bound scope object.
Changing that object later updates the protection for subsequent cancellation points.

```py
import trio


async def shield_state():
    try:
        ...
    finally:
        with trio.move_on_after(deadline=30) as s:
            s.shield = True
            await cleanup()

    try:
        ...
    finally:
        with trio.move_on_after(30) as s:
            s.shield = True
            await cleanup()

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
        with trio.move_on_after(30) as s:
            await cleanup()  # error: [await-in-finally-or-cancelled]

    try:
        ...
    finally:
        with trio.move_on_after(30):
            await cleanup()  # error: [await-in-finally-or-cancelled]

    bar = 10

    try:
        ...
    finally:
        with trio.move_on_after(bar) as s:
            s.shield = True
            await cleanup()

    try:
        ...
    finally:
        with trio.move_on_after(bar) as s:
            s.shield = False
            s.shield = True
            await cleanup()

    try:
        ...
    finally:
        with trio.move_on_after(bar) as s:
            s.shield = True
            await cleanup()
            s.shield = False
            await cleanup()  # error: [await-in-finally-or-cancelled]
            s.shield = True
            await cleanup()
```

```snapshot
error[ASYNC102]: Cancellation point in `finally` must be protected by a shielded cancel scope
  --> src/mdtest_snippet.py:28:9
   |
28 |         await cleanup()  # snapshot: await-in-finally-or-cancelled
   |         ^^^^^^^^^^^^^^^
```

### Cancel-scope boundaries

Only a literal true value or a tracked assignment enables shielding. An ordinary context manager,
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
    try:
        ...
    finally:
        with open("bar"):
            pass
    try:
        ...
    finally:
        with trio.CancelScope(deadline=30, shield=True):
            await cleanup()
    try:
        ...
    finally:
        with trio.CancelScope(shield=True):
            await cleanup()
    try:
        ...
    finally:
        with trio.CancelScope(deadline=30):
            await cleanup()  # error: [await-in-finally-or-cancelled]
    try:
        ...
    finally:
        with trio.CancelScope(deadline=30, shield=(1 == 1)):
            await cleanup()  # error: [await-in-finally-or-cancelled]
    try:
        ...
    finally:
        shield = True
        with trio.CancelScope(deadline=10) as scope:
            scope.shield = shield
            await cleanup()  # error: [await-in-finally-or-cancelled]
    try:
        ...
    finally:
        with trio.CancelScope(deadline=30, shield=True):
            with trio.move_on_after(30):
                await cleanup()
    try:
        ...
    finally:
        async for item in source:  # error: [await-in-finally-or-cancelled]
            pass

    with trio.CancelScope(deadline=30, shield=True):
        try:
            ...
        finally:
            await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Malformed cancel-scope expressions

These expressions are syntactically valid but fail at runtime: the timeout calls have invalid
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
        with trio.move_on_after(foo=10):
            await cleanup()  # error: [await-in-finally-or-cancelled]
    try:
        ...
    finally:
        async with trio.CancelScope(  # error: [await-in-finally-or-cancelled]
            deadline=30, shield=True
        ):
            await cleanup()
```

### Context managers

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

Every context manager item contributes to the effective shield state.

```py
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
        with trio.fail_after(5), trio.move_on_after(30) as s:
            s.shield = True
            await cleanup()
```

### Cancellation-catching handlers

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
        with trio.CancelScope(deadline=30, shield=True):
            await cleanup()
    except:
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

    try:
        ...
    except:
        await cleanup()  # error: [await-in-finally-or-cancelled]
    finally:
        await cleanup()  # error: [await-in-finally-or-cancelled]

    try:
        ...
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

### Nested cleanup contexts

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

The cleanup context propagates into nested asynchronous loops.

```py
async def nested_async_iteration():
    async for i in trio.bypasslinters:
        try:
            ...
        except BaseException:
            async for (  # error: [await-in-finally-or-cancelled]
                j
            ) in trio.bypasslinters:
                ...
```

Nested function bodies do not inherit the enclosing function's cleanup context.

```py
async def nested_function_definition():
    try:
        ...
    except:

        async def nested():
            await cleanup()
```

Nested cancel scopes update shielding only for their own lexical lifetime.

```py
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

### Cancellation-safe cleanup operations

Argument-free `aclose()` calls are allowed without resolving the receiver's type.

```py
import trio


async def argument_free_aclose():
    x = None

    try:
        ...
    except BaseException:
        await x.aclose()
        await x.y.aclose()
    finally:
        await x.aclose()
        await x.y.aclose()
```

Argument-free `aclose()` calls are treated as cancellation-safe cleanup, but calls with arguments
are still cancellation points.

```py
async def aclose_with_arguments():
    x = None

    try:
        ...
    except BaseException:
        await x.aclose(foo)  # error: [await-in-finally-or-cancelled]
        await x.aclose(bar=foo)  # error: [await-in-finally-or-cancelled]
        await x.aclose(*foo)  # error: [await-in-finally-or-cancelled]
        await x.aclose(None)  # error: [await-in-finally-or-cancelled]
    finally:
        await x.aclose(foo)  # error: [await-in-finally-or-cancelled]
        await x.aclose(bar=foo)  # error: [await-in-finally-or-cancelled]
        await x.aclose(*foo)  # error: [await-in-finally-or-cancelled]
        await x.aclose(None)  # error: [await-in-finally-or-cancelled]
```

Qualified AnyIO and Trio `aclose_forcefully()` calls are cancellation-safe, as described in
[flake8-async#446](https://github.com/python-trio/flake8-async/issues/446). Unresolved lookalikes
are not exempt.

```py
async def qualified_aclose_forcefully():
    x = None

    try:
        ...
    except BaseException:
        await trio.aclose_forcefully(x)
    finally:
        await trio.aclose_forcefully(x)

    try:
        ...
    finally:
        await aclose_forcefully(x)  # error: [await-in-finally-or-cancelled]
```

Qualified low-level cancel-shielded checkpoints are allowed.

```py
async def qualified_cancel_shielded_checkpoint():
    try:
        ...
    except BaseException:
        await trio.lowlevel.cancel_shielded_checkpoint()
    finally:
        await trio.lowlevel.cancel_shielded_checkpoint()
```

The checkpoint exemption requires the recognized qualified name and no arguments.

```py
async def unrecognized_checkpoint_calls():
    try:
        ...
    finally:
        await trio.lowlevel.cancel_shielded_checkpoint(cleanup)  # error: [await-in-finally-or-cancelled]
        await trio.lowlevel.checkpoint()  # error: [await-in-finally-or-cancelled]
```

## AnyIO cancellation handlers

These cases cover AnyIO-specific exception detection and task-group behavior.

### Recognized cancellation handlers

AnyIO cancellation handlers can be recognized through the module or an imported helper.

```py
import anyio
from anyio import get_cancelled_exc_class
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
    except anyio.get_cancelled_exc_class():
        await cleanup()  # error: [await-in-finally-or-cancelled]
    except BaseException:
        await cleanup()

    try:
        ...
    except get_cancelled_exc_class():
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

A repeated cancellation handler is unreachable at runtime because the first identical handler
already matches every exception the second could receive. This case is retained to verify that
Ruff's handler-state tracking does not report cleanup in the repeated handler.

```py
async def repeated_anyio_handler():
    try:
        ...
    except anyio.get_cancelled_exc_class():
        await cleanup()  # error: [await-in-finally-or-cancelled]
    except anyio.get_cancelled_exc_class():
        await cleanup()
```

A shielded AnyIO cancel scope protects its handler's cancellation points.

```py
async def shielded_anyio_handler():
    try:
        ...
    except anyio.get_cancelled_exc_class():
        with anyio.CancelScope(deadline=30, shield=True):
            await cleanup()
    except BaseException:
        await cleanup()
```

### Malformed handler expressions

These handler expressions are syntactically valid but fail when Python evaluates them at runtime.
They ensure Ruff only recognizes valid calls to `get_cancelled_exc_class()` and ignores unrelated
lookalikes. Because the malformed expressions are not recognized as cancellation handlers, the
later bare handlers remain cancellation-catching for this static analysis.

```py
import anyio
from anyio import get_cancelled_exc_class


async def malformed_anyio_handlers():
    try:
        ...
    except anyio.get_cancelled_exc_class:
        await cleanup()
    except anyio.get_cancelled_exc_class(...):
        await cleanup()
    except get_cancelled_exc_class:
        await cleanup()
    except:
        await cleanup()  # error: [await-in-finally-or-cancelled]

    try:
        ...
    except anyio.foo():
        await cleanup()
    except ValueError:
        await cleanup()
    except anyio.Cancelled:
        await cleanup()
    except:
        await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Task groups

Creating an AnyIO task group is not itself a cancellation point on entry or exit.

```py
import anyio


async def task_group_cleanup():
    try:
        ...
    finally:
        async with anyio.create_task_group() as tg:
            tg.cancel_scope.deadline = anyio.current_time() + 10
            tg.cancel_scope.shield = True
            await cleanup()
```

## Trio cancellation handlers

These cases cover Trio's concrete cancellation exception and nursery behavior.

### Cancellation exception handlers

Trio-specific cases use the concrete `Cancelled` exception and nursery API.

```py
import trio
```

A `trio.Cancelled` handler is a cancellation-catching cleanup context.

```py
async def trio_handlers():
    try:
        ...
    except trio.Cancelled:
        await cleanup()  # error: [await-in-finally-or-cancelled]
    except:
        await cleanup()

    try:
        ...
    except trio.Cancelled:
        await cleanup()  # error: [await-in-finally-or-cancelled]
    except BaseException:
        await cleanup()
```

Earlier handlers that consume the cancellation family affect which later handlers can catch
cancellation.

```py
async def shielded_trio_handler():
    try:
        ...
    except trio.Cancelled:
        with trio.CancelScope(deadline=30, shield=True):
            await cleanup()
    except BaseException:
        await cleanup()
```

### Nurseries

Opening a Trio nursery is not itself a cancellation point on entry or exit.

```py
import trio


async def nursery_cleanup():
    try:
        ...
    finally:
        async with trio.open_nursery() as nursery:
            nursery.cancel_scope.deadline = trio.current_time() + 10
            nursery.cancel_scope.shield = True
            await cleanup()
```

## Exception groups

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


async def foo():
    try:
        ...
    except* ValueError:
        await foo()
        raise
    except* BaseException:
        await foo()  # error: [await-in-finally-or-cancelled]
    finally:
        await foo()  # error: [await-in-finally-or-cancelled]

    try:
        ...
    except* BaseException:
        with trio.move_on_after(30, shield=True):
            await foo()
```

## Ruff-specific regressions

These cases exercise semantic resolution, data-flow tracking, traversal order, and nested scopes.

### Cancel-scope resolution

Aliases for Trio and AnyIO cancel scopes are recognized. Literal true values enable shielding,
while false or unknown values leave cleanup exposed.

```py
import anyio as aio
import trio as t
from anyio import CancelScope as Shield
from trio import fail_at, move_on_at, fail_after
```

```py
async def shields_without_timeouts():
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
        with t.move_on_after(10, shield=True):
            await cleanup()
        with Shield(), Shield(shield=True):
            await cleanup()
        with Shield() as a, Shield() as b:
            b.shield = True
            await cleanup()
        with Shield(shield=False):
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield(shield=1):
            await cleanup()
        with Shield(shield=0):
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield() as scope:
            scope.shield = 1
            await cleanup()
            scope.shield = None
            await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Cancellation-handler resolution

Cancellation-catching handlers include bare handlers, `BaseException`, Trio and AnyIO cancellation
types, tuples, and exception groups. Once an earlier handler catches a cancellation family, a later
handler does not catch it again. Trio and asyncio cancellation remain separate families because
AnyIO can use either backend; catching one family does not consume the other. Ordinary exceptions
are outside the rule's scope.

```py
import asyncio
import trio as t
from anyio import get_cancelled_exc_class as cancelled


async def exception_handlers():
    try:
        ...
    except Exception:
        await cleanup()
        raise
    except cancelled():
        await cleanup()  # error: [await-in-finally-or-cancelled]
    except BaseException:
        await cleanup()

    try:
        ...
    except* t.Cancelled:
        await cleanup()  # error: [await-in-finally-or-cancelled]
    except* BaseException:
        await cleanup()  # error: [await-in-finally-or-cancelled]

    try:
        ...
    except (ValueError, t.Cancelled):
        await cleanup()  # error: [await-in-finally-or-cancelled]

    try:
        ...
    except asyncio.CancelledError:
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

### Malformed cancellation-factory aliases

Calling an imported `get_cancelled_exc_class` alias with arguments, or using the function itself as
an exception type, fails at runtime. This regression ensures Ruff does not recognize either form as
a cancellation handler and continues its static analysis with the later bare handler.

```py
from anyio import get_cancelled_exc_class as cancelled


async def malformed_cancellation_factory_aliases():
    try:
        ...
    except cancelled(1):
        await cleanup()
    except cancelled:
        await cleanup()
    except:
        await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Nested cleanup and cancellation points

Nested cleanup contexts diagnose each cancellation point once and do not reuse an outer shield.

```py
import anyio as aio
import trio as t
from anyio import CancelScope as Shield


async def nested_cleanup():
    with Shield(shield=True):
        try:
            ...
        finally:
            await cleanup()  # error: [await-in-finally-or-cancelled]
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

Implicit and explicit cancellation points include async context managers, async iteration,
comprehensions, and nested awaits.

```py
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
        result = {item async for item in items}  # error: [await-in-finally-or-cancelled]
        result = {item: item async for item in items}  # error: [await-in-finally-or-cancelled]
        deferred = (item async for item in items)
        deferred = (item async for item in await source())  # error: [await-in-finally-or-cancelled]
        # error: [await-in-finally-or-cancelled]
        # error: [await-in-finally-or-cancelled]
        await (await factory())()
        with Shield(shield=await flag()):  # error: [await-in-finally-or-cancelled]
            await cleanup()  # error: [await-in-finally-or-cancelled]
        async with t.open_nursery(), cm:  # error: [await-in-finally-or-cancelled]
            pass
        async with aio.create_task_group() as group:
            await cleanup()  # error: [await-in-finally-or-cancelled]
            group.cancel_scope.shield = True
            await cleanup()
            group.cancel_scope.shield = False
            await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Safe calls and local imports

Only the recognized cancellation-safe cleanup call shapes are exempt.

```py
import anyio as aio


async def safe_calls(resource):
    from anyio import aclose_forcefully as force_close
    from trio.lowlevel import cancel_shielded_checkpoint as shielded_checkpoint

    try:
        ...
    finally:
        await resource.aclose()
        await force_close(resource)
        await aio.lowlevel.cancel_shielded_checkpoint()
        await shielded_checkpoint()
        await (await resource()).aclose()  # error: [await-in-finally-or-cancelled]
        await force_close(await resource())  # error: [await-in-finally-or-cancelled]
        await resource.aclose(**kwargs)  # error: [await-in-finally-or-cancelled]
        await shielded_checkpoint(1)  # error: [await-in-finally-or-cancelled]
        await aclose_forcefully(resource)  # error: [await-in-finally-or-cancelled]
```

Local aliases are recognized until rebinding shadows the imported symbol.

```py
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

### Shield-state data flow

Shield mutations are tracked conservatively across branches, rebinding, and nested scopes.

```py
from anyio import CancelScope as Shield


async def scope_mutations(flag):
    try:
        ...
    finally:
        with Shield(shield=True) as scope:
            scope.shield = flag
            await cleanup()  # error: [await-in-finally-or-cancelled]
            scope.shield = True
            await cleanup()
            scope.shield: bool = False
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield() as scope:
            if flag:
                scope.shield = True
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield() as scope:
            if flag:
                scope.shield = True
            else:
                scope.shield = True
            await cleanup()
        with Shield(shield=True) as scope:
            if flag:
                scope.shield = False
            else:
                await cleanup()  # The other branch remains shielded.
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield() as scope:
            scope = other
            scope.shield = True
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield() as outer:
            with Shield():
                outer.shield = True
            await cleanup()
        with Shield() as outer:
            with Shield() as inner:
                inner.shield = True
                await cleanup()
            await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Definition boundaries

Defaults are evaluated in the enclosing cleanup context, while nested bodies are independent except
for their own cleanup methods.

```py
from anyio import CancelScope as Shield


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

        deferred = lambda: cleanup()
```

An `__aexit__` method is checked as an independent cleanup context.

```py
class Manager:
    async def __aexit__(self, *args):
        await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield(shield=True):
            await cleanup()
```

### Control flow and rebinding

Control flow and assignment targets can introduce cancellation points or invalidate a shield.

```py
from anyio import CancelScope as Shield


async def control_flow_and_targets(flag, cm):
    try:
        ...
    finally:
        with Shield(shield=True) as scope:
            async with cm:  # error: [await-in-finally-or-cancelled]
                scope.shield = False
        with Shield(shield=True) as scope:
            async for item in source:  # error: [await-in-finally-or-cancelled]
                scope.shield = False
        with Shield(shield=True) as scope:
            while flag:
                await cleanup()  # error: [await-in-finally-or-cancelled]
                scope.shield = False
        with Shield() as scope:
            for item in source:
                scope.shield = True
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield() as scope:
            match flag:
                case 1:
                    scope.shield = True
                case _:
                    await cleanup()  # error: [await-in-finally-or-cancelled]
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield(shield=True) as scope:
            scope.shield &= False
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield() as scope:
            (scope := other)
            scope.shield = True
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield() as scope:
            scope, = [other]
            scope.shield = True
            await cleanup()  # error: [await-in-finally-or-cancelled]
        (await factory()).field: int = 1  # error: [await-in-finally-or-cancelled]
        (await factory()).field: int  # error: [await-in-finally-or-cancelled]
        for (await factory()).field in items:  # error: [await-in-finally-or-cancelled]
            pass
        with Shield(shield=True) as (await factory()).field:
            await cleanup()
        # error: [await-in-finally-or-cancelled]
        # error: [await-in-finally-or-cancelled]
        async with cm as (await factory()).field:
            pass
        with Shield(shield=True) as scope:
            try:
                scope.shield = False
            except ValueError:
                pass
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield(shield=True) as scope:
            try:
                ...
            finally:
                scope.shield = False
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

### Class-body bindings

Class bodies can mutate an enclosing shield unless a class-local binding shadows it.

```py
from anyio import CancelScope as Shield


async def class_body_effects():
    try:
        ...
    finally:
        with Shield(shield=True) as scope:
            class DisablesOuterShield:
                scope.shield = False

            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield(shield=True) as scope:
            class ShadowsOuterScope:
                scope = other
                scope.shield = False

            await cleanup()
```

### Loop `else` paths

Loop `else` assignments only establish a shield when every path reaches them.

```py
from anyio import CancelScope as Shield


async def loop_else_shielding(items, flag):
    try:
        ...
    finally:
        with Shield() as scope:
            for item in items:
                break
            else:
                scope.shield = True
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield() as scope:
            while flag:
                break
            else:
                scope.shield = True
            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield() as scope:
            for item in items:
                while flag:
                    break
            else:
                scope.shield = True
            await cleanup()
```

### Assignment-target evaluation

Assignment targets are visited in evaluation order, including awaits inside targets.

```py
from anyio import CancelScope as Shield


async def assignment_target_order():
    try:
        ...
    finally:
        with Shield(shield=True) as scope:
            scope.shield = (await factory()).field = False  # error: [await-in-finally-or-cancelled]
        with Shield(shield=True) as scope:
            (scope.shield, (await factory()).field) = (False, value)  # error: [await-in-finally-or-cancelled]
```

Async-loop targets update shield state after the iteration checkpoint.

```py
async def async_for_target_state(source):
    try:
        ...
    finally:
        with Shield(shield=True) as scope:
            async for scope.shield in source:  # error: [await-in-finally-or-cancelled]
                pass
        with Shield(shield=True) as scope:
            async for scope in source:
                pass
```

### Class-body control flow

Class-body control flow is analyzed conservatively when it may change an enclosing shield.

```py
from anyio import CancelScope as Shield


async def class_body_control_flow(flag):
    try:
        ...
    finally:
        with Shield(shield=True) as scope:
            while flag:
                await cleanup()  # error: [await-in-finally-or-cancelled]

                class DisablesShield:
                    scope.shield = False

        with Shield(shield=True) as scope:
            try:
                class DisablesShieldAndRaises:
                    scope.shield = False
                    raise ValueError
            except ValueError:
                await cleanup()  # error: [await-in-finally-or-cancelled]
```

A class body's `nonlocal` declaration makes writes affect the enclosing scope handle.

```py
async def class_body_nonlocal():
    try:
        ...
    finally:
        with Shield(shield=True) as scope:
            class DisablesShield:
                nonlocal scope
                scope.shield = False

            await cleanup()  # error: [await-in-finally-or-cancelled]
        with Shield() as scope:
            class EnablesShield:
                nonlocal scope
                scope.shield = True

            await cleanup()
        with Shield() as scope:
            class RebindsHandle:
                nonlocal scope
                scope = other
                scope.shield = True

            await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Nested loop `else` paths

Breaks in nested loops do not incorrectly suppress the outer loop's `else` path.

```py
from anyio import CancelScope as Shield


async def nested_loop_else_break(items, flag):
    try:
        ...
    finally:
        with Shield() as scope:
            for item in items:
                while flag:
                    pass
                else:
                    break
            else:
                scope.shield = True
            await cleanup()  # error: [await-in-finally-or-cancelled]
```

### Class-local rebinding

Class-local rebinding prevents later writes from changing the enclosing cancel scope.

```py
from anyio import CancelScope as Shield


async def class_body_local_rebinding():
    try:
        ...
    finally:
        with Shield(shield=True) as scope:
            class ShadowsOuterScope:
                scope = other

            scope.shield = False
            await cleanup()  # error: [await-in-finally-or-cancelled]
```

## Asyncio does not enable the rule

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

## No cancellation library import

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
