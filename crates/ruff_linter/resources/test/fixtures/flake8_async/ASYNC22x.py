import os
import subprocess

# Violation cases:


async def func():
    subprocess.run("foo")  # ASYNC221


async def func():
    subprocess.call("foo")  # ASYNC221


async def func():
    subprocess.foo(0)  # OK


async def func():
    os.wait4(10)  # ASYNC222


async def func():
    os.wait(12)  # ASYNC222


async def foo():
    await async_fun(
        subprocess.getoutput()  # ASYNC221
    )
    subprocess.Popen()  # ASYNC220
    os.system()  # ASYNC221

    system()
    os.system.anything()
    os.anything()

    subprocess.run()  # ASYNC221
    subprocess.call()  # ASYNC221
    subprocess.check_call()  # ASYNC221
    subprocess.check_output()  # ASYNC221
    subprocess.getoutput()  # ASYNC221
    subprocess.getstatusoutput()  # ASYNC221

    await async_fun(
        subprocess.getoutput()  # ASYNC221
    )

    subprocess.anything()
    subprocess.foo()
    subprocess.bar.foo()
    subprocess()

    os.posix_spawn()  # ASYNC221
    os.posix_spawnp()  # ASYNC221

    os.spawn()
    os.spawn
    os.spawnllll()

    os.spawnl()  # ASYNC221
    os.spawnle()  # ASYNC221
    os.spawnlp()  # ASYNC221
    os.spawnlpe()  # ASYNC221
    os.spawnv()  # ASYNC221
    os.spawnve()  # ASYNC221
    os.spawnvp()  # ASYNC221
    os.spawnvpe()  # ASYNC221

    P_NOWAIT = os.P_NOWAIT

    # if mode is given, and is not os.P_WAIT: ASYNC220
    os.spawnl(os.P_NOWAIT)  # ASYNC220
    os.spawnl(P_NOWAIT)  # ASYNC220
    os.spawnl(mode=os.P_NOWAIT)  # ASYNC220
    os.spawnl(mode=P_NOWAIT)  # ASYNC220

    P_WAIT = os.P_WAIT

    # if it is P_WAIT, ASYNC221
    os.spawnl(P_WAIT)  # ASYNC221
    os.spawnl(mode=os.P_WAIT)  # ASYNC221
    os.spawnl(mode=P_WAIT)  # ASYNC221

    # other weird cases: ASYNC220
    os.spawnl(0)  # ASYNC220
    os.spawnl(1)  # ASYNC220
    os.spawnl(foo())  # ASYNC220

    # ASYNC222
    os.wait()  # ASYNC222
    os.wait3()  # ASYNC222
    os.wait4()  # ASYNC222
    os.waitid()  # ASYNC222
    os.waitpid()  # ASYNC222

    os.waitpi()
    os.waiti()
