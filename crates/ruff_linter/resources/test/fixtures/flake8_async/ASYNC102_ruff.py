import asyncio
import anyio as aio
import trio as t
from anyio import CancelScope as Shield
from anyio import get_cancelled_exc_class as cancelled
from trio import fail_at, move_on_at, fail_after


async def shields_without_timeouts():
    try:
        pass
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
            await cleanup()  # ASYNC102
        with Shield(shield=1):
            await cleanup()
        with Shield(shield=0):
            await cleanup()  # ASYNC102
        with Shield() as scope:
            scope.shield = 1
            await cleanup()
            scope.shield = None
            await cleanup()  # ASYNC102


async def exception_handlers():
    try:
        pass
    except Exception:
        await cleanup()
        raise  # Ordinary exceptions belong to ASYNC120.
    except cancelled():
        await cleanup()  # ASYNC102
    except BaseException:
        await cleanup()  # Cancellation was already caught.

    try:
        pass
    except* t.Cancelled:
        await cleanup()  # ASYNC102
    except* BaseException:
        await cleanup()  # ASYNC102: another subgroup can contain cancellation.

    try:
        pass
    except (ValueError, t.Cancelled):
        await cleanup()  # ASYNC102

    try:
        pass
    except cancelled(1):
        await cleanup()  # An invalid cancellation-class factory call.
    except cancelled:
        await cleanup()
    except:
        await cleanup()  # ASYNC102

    try:
        pass
    except asyncio.CancelledError:
        await cleanup()  # ASYNC102: AnyIO can use asyncio as its backend.

    try:
        pass
    except t.Cancelled:
        await cleanup()  # ASYNC102
    except asyncio.CancelledError:
        await cleanup()  # ASYNC102: a different cancellation family
    except BaseException:
        await cleanup()  # Both cancellation families were already caught.


async def nested_cleanup():
    with Shield(shield=True):
        try:
            pass
        finally:
            await cleanup()  # ASYNC102: the shield predates the cleanup.
    try:
        pass
    finally:
        try:
            await cleanup()  # ASYNC102, once
        except ValueError:
            await cleanup()  # ASYNC102, once
        finally:
            await cleanup()  # ASYNC102, once
        with Shield(shield=True):
            try:
                await cleanup()
            except BaseException:
                await cleanup()
            finally:
                await cleanup()  # ASYNC102: a new cleanup context


async def checkpoints(cm, items):
    try:
        pass
    finally:
        async with cm:  # ASYNC102
            pass
        async with cm, manager():  # ASYNC102 per item
            pass
        async for item in items:  # ASYNC102
            pass
        result = [item async for item in items]  # ASYNC102
        result = {item async for item in items}  # ASYNC102
        result = {item: item async for item in items}  # ASYNC102
        deferred = (item async for item in items)  # Evaluated on iteration.
        deferred = (item async for item in await source())  # ASYNC102
        await (await factory())()  # ASYNC102 twice
        with Shield(shield=await flag()):  # ASYNC102
            await cleanup()  # ASYNC102
        async with t.open_nursery(), cm:  # ASYNC102 on cm only
            pass
        async with aio.create_task_group() as group:
            await cleanup()  # ASYNC102
            group.cancel_scope.shield = True
            await cleanup()
            group.cancel_scope.shield = False
            await cleanup()  # ASYNC102


async def safe_calls(resource):
    from anyio import aclose_forcefully as force_close
    from trio.lowlevel import cancel_shielded_checkpoint as shielded_checkpoint

    try:
        pass
    finally:
        await resource.aclose()
        await force_close(resource)
        await aio.lowlevel.cancel_shielded_checkpoint()
        await shielded_checkpoint()
        await (await resource()).aclose()  # ASYNC102 on the receiver's await
        await force_close(await resource())  # ASYNC102 on the argument's await
        await resource.aclose(**kwargs)  # ASYNC102
        await shielded_checkpoint(1)  # ASYNC102
        await aclose_forcefully(resource)  # ASYNC102: unresolved name


async def local_import_and_shadowing():
    from anyio import CancelScope as LocalShield

    try:
        pass
    finally:
        with LocalShield(shield=True):
            await cleanup()
        LocalShield = arbitrary
        with LocalShield(shield=True):
            await cleanup()  # ASYNC102


async def scope_mutations(flag):
    try:
        pass
    finally:
        with Shield(shield=True) as scope:
            scope.shield = flag
            await cleanup()  # ASYNC102: unknown assignment invalidates True
            scope.shield = True
            await cleanup()
            scope.shield: bool = False
            await cleanup()  # ASYNC102
        with Shield() as scope:
            if flag:
                scope.shield = True
            await cleanup()  # ASYNC102: not shielded on every path
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
            await cleanup()  # ASYNC102
        with Shield() as scope:
            scope = other
            scope.shield = True
            await cleanup()  # ASYNC102: the original handle was rebound
        with Shield() as outer:
            with Shield():
                outer.shield = True
            await cleanup()
        with Shield() as outer:
            with Shield() as inner:
                inner.shield = True
                await cleanup()
            await cleanup()  # ASYNC102: the inner shield has exited


async def definition_boundaries():
    try:
        pass
    finally:
        async def nested(value=await factory()):  # ASYNC102 on the default
            await cleanup()

        class Nested:
            async def method(self):
                await cleanup()

            async def __aexit__(self, *args):
                await cleanup()  # ASYNC102: an independent cleanup context

        deferred = lambda: cleanup()


class Manager:
    async def __aexit__(self, *args):
        await cleanup()  # ASYNC102
        with Shield(shield=True):
            await cleanup()


async def control_flow_and_targets(flag, cm):
    try:
        pass
    finally:
        with Shield(shield=True) as scope:
            async with cm:  # ASYNC102: cancellation on exit
                scope.shield = False
        with Shield(shield=True) as scope:
            async for item in source:  # ASYNC102: later iterations
                scope.shield = False
        with Shield(shield=True) as scope:
            while flag:
                await cleanup()  # ASYNC102: later iterations
                scope.shield = False
        with Shield() as scope:
            for item in source:
                scope.shield = True
            await cleanup()  # ASYNC102: the loop may be empty
        with Shield() as scope:
            match flag:
                case 1:
                    scope.shield = True
                case _:
                    await cleanup()  # ASYNC102
            await cleanup()  # ASYNC102
        with Shield(shield=True) as scope:
            scope.shield &= False
            await cleanup()  # ASYNC102
        with Shield() as scope:
            (scope := other)
            scope.shield = True
            await cleanup()  # ASYNC102
        with Shield() as scope:
            scope, = [other]
            scope.shield = True
            await cleanup()  # ASYNC102
        (await factory()).field: int = 1  # ASYNC102
        (await factory()).field: int  # ASYNC102
        for (await factory()).field in items:  # ASYNC102
            pass
        with Shield(shield=True) as (await factory()).field:
            await cleanup()
        async with cm as (await factory()).field:  # ASYNC102 twice
            pass
        with Shield(shield=True) as scope:
            try:
                scope.shield = False
            except ValueError:
                pass
            await cleanup()  # ASYNC102
        with Shield(shield=True) as scope:
            try:
                pass
            finally:
                scope.shield = False
            await cleanup()  # ASYNC102


async def handler_type_checkpoints():
    try:
        pass
    except (BaseException, await exception_type()):  # ASYNC102
        await cleanup()  # ASYNC102


async def class_body_effects():
    try:
        pass
    finally:
        with Shield(shield=True) as scope:
            class DisablesOuterShield:
                scope.shield = False

            await cleanup()  # ASYNC102
        with Shield(shield=True) as scope:
            class ShadowsOuterScope:
                scope = other
                scope.shield = False

            await cleanup()


async def loop_else_shielding(items, flag):
    try:
        pass
    finally:
        with Shield() as scope:
            for item in items:
                break
            else:
                scope.shield = True
            await cleanup()  # ASYNC102
        with Shield() as scope:
            while flag:
                break
            else:
                scope.shield = True
            await cleanup()  # ASYNC102
        with Shield() as scope:
            for item in items:
                while flag:
                    break
            else:
                scope.shield = True
            await cleanup()


async def assignment_target_order():
    try:
        pass
    finally:
        with Shield(shield=True) as scope:
            scope.shield = (await factory()).field = False  # ASYNC102
        with Shield(shield=True) as scope:
            (scope.shield, (await factory()).field) = (False, value)  # ASYNC102


async def async_for_target_state(source):
    try:
        pass
    finally:
        with Shield(shield=True) as scope:
            async for scope.shield in source:  # ASYNC102: later iterations
                pass
        with Shield(shield=True) as scope:
            async for scope in source:
                pass
