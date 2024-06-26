import trio


async def func() -> None:
    trio.run(foo)  # OK, not async

    # OK
    await trio.aclose_forcefully(foo)
    await trio.open_file(foo)
    await trio.open_ssl_over_tcp_listeners(foo, foo)
    await trio.open_ssl_over_tcp_stream(foo, foo)
    await trio.open_tcp_listeners(foo)
    await trio.open_tcp_stream(foo, foo)
    await trio.open_unix_socket(foo)
    await trio.run_process(foo)
    await trio.sleep(5)
    await trio.sleep_until(5)
    await trio.lowlevel.cancel_shielded_checkpoint()
    await trio.lowlevel.checkpoint()
    await trio.lowlevel.checkpoint_if_cancelled()
    await trio.lowlevel.open_process(foo)
    await trio.lowlevel.permanently_detach_coroutine_object(foo)
    await trio.lowlevel.reattach_detached_coroutine_object(foo, foo)
    await trio.lowlevel.temporarily_detach_coroutine_object(foo)
    await trio.lowlevel.wait_readable(foo)
    await trio.lowlevel.wait_task_rescheduled(foo)
    await trio.lowlevel.wait_writable(foo)

    # ASYNC105
    trio.aclose_forcefully(foo)
    trio.open_file(foo)
    trio.open_ssl_over_tcp_listeners(foo, foo)
    trio.open_ssl_over_tcp_stream(foo, foo)
    trio.open_tcp_listeners(foo)
    trio.open_tcp_stream(foo, foo)
    trio.open_unix_socket(foo)
    trio.run_process(foo)
    trio.serve_listeners(foo, foo)
    trio.serve_ssl_over_tcp(foo, foo, foo)
    trio.serve_tcp(foo, foo)
    trio.sleep(foo)
    trio.sleep_forever()
    trio.sleep_until(foo)
    trio.lowlevel.cancel_shielded_checkpoint()
    trio.lowlevel.checkpoint()
    trio.lowlevel.checkpoint_if_cancelled()
    trio.lowlevel.open_process()
    trio.lowlevel.permanently_detach_coroutine_object(foo)
    trio.lowlevel.reattach_detached_coroutine_object(foo, foo)
    trio.lowlevel.temporarily_detach_coroutine_object(foo)
    trio.lowlevel.wait_readable(foo)
    trio.lowlevel.wait_task_rescheduled(foo)
    trio.lowlevel.wait_writable(foo)

    async with await trio.open_file(foo):  # Ok
        pass

    async with trio.open_file(foo):  # ASYNC105
        pass


def func() -> None:
    # ASYNC105 (without fix)
    trio.open_file(foo)
