# flags: --minimum-version=3.10
with (CtxManager() as example):
    ...

with (CtxManager1(), CtxManager2()):
    ...

with (CtxManager1() as example, CtxManager2()):
    ...

with (CtxManager1(), CtxManager2() as example):
    ...

with (CtxManager1() as example1, CtxManager2() as example2):
    ...

with (
    CtxManager1() as example1,
    CtxManager2() as example2,
    CtxManager3() as example3,
):
    ...
