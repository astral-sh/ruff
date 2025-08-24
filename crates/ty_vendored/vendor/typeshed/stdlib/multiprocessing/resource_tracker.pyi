import sys
from _typeshed import FileDescriptorOrPath
from collections.abc import Sized

__all__ = ["ensure_running", "register", "unregister"]

class ResourceTracker:
    def getfd(self) -> int | None: ...
    def ensure_running(self) -> None:
        """Make sure that resource tracker process is running.

        This can be run from any process.  Usually a child process will use
        the resource created by its parent.
        """

    def register(self, name: Sized, rtype: str) -> None:
        """Register name of resource with resource tracker."""

    def unregister(self, name: Sized, rtype: str) -> None:
        """Unregister name of resource with resource tracker."""
    if sys.version_info >= (3, 12):
        def __del__(self) -> None: ...

_resource_tracker: ResourceTracker
ensure_running = _resource_tracker.ensure_running
register = _resource_tracker.register
unregister = _resource_tracker.unregister
getfd = _resource_tracker.getfd

def main(fd: FileDescriptorOrPath) -> None:
    """Run resource tracker."""
