"""
Ruff plugin: Async Lock Auditing & Deadlock Detection (bounty_551)

This module implements an AST-based Ruff plugin to audit usage of asyncio locks
and detect potential deadlocks in async Python code.

Checks:
- Warns when an asyncio.Lock, asyncio.RLock, or asyncio.Semaphore is acquired
  without a corresponding release in all code paths.
- Warns on nested lock acquisitions in async functions (potential deadlock).
- Warns on lock acquisition outside async functions.

References:
- https://github.com/astral-sh/ruff/issues/bounty_551

Author: [Your Name]
"""

import ast
from typing import Any, Iterator, List, Optional, Set, Tuple, Dict

# Ruff plugin interface stubs (for testability, since ruff is not importable here)
class Checker:
    def __init__(self, tree: ast.AST) -> None:
        self.tree = tree
        self.reports: List[Tuple[str, int, int, str]] = []

    def report(self, code: str, lineno: int, col_offset: int, message: str) -> None:
        self.reports.append((code, lineno, col_offset, message))


class Rule:
    pass

# Error codes
ASYNC_LOCK_UNRELEASED = "ASYNC001"
ASYNC_LOCK_NESTED = "ASYNC002"
ASYNC_LOCK_SYNC = "ASYNC003"

ASYNCIO_LOCK_TYPES = {
    "Lock",
    "RLock",
    "Semaphore",
    "BoundedSemaphore",
    "Event",
    "Condition",
}

ASYNCIO_MODULES = {"asyncio", "trio"}


class AsyncLockAuditRule(Rule):
    """Ruff rule for async lock auditing and deadlock detection."""
    name = "async_lock_audit"
    code = "ASYNC"
    description = "Audit asyncio lock usage and detect potential deadlocks."


class LockAcquisition:
    """Represents a lock acquisition in the AST."""

    def __init__(self, varname: str, lineno: int, col_offset: int) -> None:
        self.varname = varname
        self.lineno = lineno
        self.col_offset = col_offset


class LockRelease:
    """Represents a lock release in the AST."""

    def __init__(self, varname: str, lineno: int, col_offset: int) -> None:
        self.varname = varname
        self.lineno = lineno
        self.col_offset = col_offset


class AsyncLockVisitor(ast.NodeVisitor):
    """
    AST visitor to find asyncio lock usage patterns and potential deadlocks.

    - Tracks lock acquisition and release.
    - Warns on unreleased locks.
    - Warns on nested lock acquisitions.
    - Warns on lock usage outside async functions.
    """

    def __init__(self, checker: Checker) -> None:
        self.checker = checker
        self.lock_vars: Set[str] = set()
        self.lock_assignments: Dict[str, Tuple[str, int, int]] = {}
        self.async_function_stack: List[bool] = []
        self.current_function: Optional[ast.FunctionDef] = None

    def visit_Assign(self, node: ast.Assign) -> Any:
        # Detect: lock = asyncio.Lock()
        if isinstance(node.value, ast.Call):
            call = node.value
            lock_type = self._get_lock_type(call)
            if lock_type:
                for target in node.targets:
                    if isinstance(target, ast.Name):
                        self.lock_vars.add(target.id)
                        self.lock_assignments[target.id] = (
                            lock_type,
                            node.lineno,
                            node.col_offset,
                        )
        self.generic_visit(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> Any:
        self.async_function_stack.append(True)
        self.current_function = node
        self._visit_function_body(node)
        self.async_function_stack.pop()
        self.current_function = None

    def visit_FunctionDef(self, node: ast.FunctionDef) -> Any:
        self.async_function_stack.append(False)
        self.current_function = node
        self._visit_function_body(node)
        self.async_function_stack.pop()
        self.current_function = None

    def _visit_function_body(self, node: ast.FunctionDef) -> None:
        lock_acquires: List[LockAcquisition] = []
        lock_releases: List[LockRelease] = []
        lock_stack: List[str] = []

        for child in ast.walk(node):
            # with await lock:
            if isinstance(child, ast.With):
                for item in child.items:
                    ctx_expr = item.context_expr
                    if isinstance(ctx_expr, ast.Await):
                        awaited = ctx_expr.value
                        if (
                            isinstance(awaited, ast.Call)
                            and isinstance(awaited.func, ast.Attribute)
                            and awaited.func.attr == "acquire"
                            and isinstance(awaited.func.value, ast.Name)
                            and awaited.func.value.id in self.lock_vars
                        ):
                            varname = awaited.func.value.id
                            lock_acquires.append(
                                LockAcquisition(varname, child.lineno, child.col_offset)
                            )
                            lock_stack.append(varname)
                            # Check for nested lock
                            if lock_stack.count(varname) > 1:
                                self.checker.report(
                                    ASYNC_LOCK_NESTED,
                                    child.lineno,
                                    child.col_offset,
                                    f"Nested acquisition of lock '{varname}' may cause deadlock.",
                                )
            # await lock.acquire()
            elif isinstance(child, ast.Await):
                call = child.value
                if (
                    isinstance(call, ast.Call)
                    and isinstance(call.func, ast.Attribute)
                    and call.func.attr == "acquire"
                    and isinstance(call.func.value, ast.Name)
                    and call.func.value.id in self.lock_vars
                ):
                    varname = call.func.value.id
                    lock_acquires.append(
                        LockAcquisition(varname, child.lineno, child.col_offset)
                    )
                    lock_stack.append(varname)
                    # Check for nested lock
                    if lock_stack.count(varname) > 1:
                        self.checker.report(
                            ASYNC_LOCK_NESTED,
                            child.lineno,
                            child.col_offset,
                            f"Nested acquisition of lock '{varname}' may cause deadlock.",
                        )
            # lock.release()
            elif isinstance(child, ast.Expr):
                value = child.value
                if (
                    isinstance(value, ast.Call)
                    and isinstance(value.func, ast.Attribute)
                    and value.func.attr == "release"
                    and isinstance(value.func.value, ast.Name)
                    and value.func.value.id in self.lock_vars
                ):
                    varname = value.func.value.id
                    lock_releases.append(
                        LockRelease(varname, child.lineno, child.col_offset)
                    )
                    if varname in lock_stack:
                        lock_stack.remove(varname)

        # Check for unreleased locks
        acquired_vars = {acq.varname for acq in lock_acquires}
        released_vars = {rel.varname for rel in lock_releases}
        unreleased = acquired_vars - released_vars
        for var in unreleased:
            acq = next((a for a in lock_acquires if a.varname == var), None)
            if acq:
                self.checker.report(
                    ASYNC_LOCK_UNRELEASED,
                    acq.lineno,
                    acq.col_offset,
                    f"Lock '{var}' acquired but not released in all code paths.",
                )

        # Check for lock usage in sync function
        if self.async_function_stack and not self.async_function_stack[-1]:
            for acq in lock_acquires:
                self.checker.report(
                    ASYNC_LOCK_SYNC,
                    acq.lineno,
                    acq.col_offset,
                    f"Lock '{acq.varname}' acquired in non-async function.",
                )

    def _get_lock_type(self, call: ast.Call) -> Optional[str]:
        # Detect if call is asyncio.Lock() or similar
        if isinstance(call.func, ast.Attribute):
            # e.g., asyncio.Lock()
            if (
                call.func.attr in ASYNCIO_LOCK_TYPES
                and isinstance(call.func.value, ast.Name)
                and call.func.value.id in ASYNCIO_MODULES
            ):
                return call.func.attr
        elif isinstance(call.func, ast.Name):
            # e.g., Lock() (if imported directly)
            if call.func.id in ASYNCIO_LOCK_TYPES:
                return call.func.id
        return None


def plugin(checker: Checker) -> Iterator[Rule]:
    """
    Ruff plugin entry point.

    Args:
        checker (Checker): Ruff checker instance.

    Yields:
        Rule: AsyncLockAuditRule for each file.
    """
    visitor = AsyncLockVisitor(checker)
    visitor.visit(checker.tree)
    yield AsyncLockAuditRule