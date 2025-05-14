import sys
#
#
# _ThreadInfoName: TypeAlias = Literal["nt", "pthread", "pthread-stubs", "solaris"]
# _ThreadInfoLock: TypeAlias = Literal["semaphore", "mutex+cond"] | None
# _ReleaseLevel: TypeAlias = Literal["alpha", "beta", "candidate", "final"]

# This class is not exposed at runtime. It calls itself sys.version_info.
@final
@type_check_only
class _version_info(_UninstantiableStructseq, tuple[int, int, int, _ReleaseLevel, int]):
    if sys.version_info >= (3, 10):
        __match_args__: Final = ("major", "minor", "micro", "releaselevel", "serial")

    @property
    def major(self) -> int: ...
    @property
    def minor(self) -> int: ...
    @property
    def micro(self) -> int: ...
    @property
    def releaselevel(self) -> _ReleaseLevel: ...
    @property
    def serial(self) -> int: ...

version_info: _version_info
