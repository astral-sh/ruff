import contextlib

with contextlib.ExitStack() as exit_stack:
    f = exit_stack.enter_context(open("filename"))

with contextlib.ExitStack() as stack:
    files = [stack.enter_context(open(fname)) for fname in filenames]
    close_files = stack.pop_all().close

with contextlib.AsyncExitStack() as exit_stack:
    f = await exit_stack.enter_async_context(open("filename"))

with contextlib.ExitStack():
    f = exit_stack.enter_context(open("filename"))  # SIM115

with contextlib.ExitStack():
    f = open("filename")  # SIM115
