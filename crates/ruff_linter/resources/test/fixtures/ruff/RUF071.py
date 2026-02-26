"""Tests for RUF071 (incorrect-decorator-order)."""

from abc import abstractmethod, abstractproperty, abstractclassmethod, abstractstaticmethod
from contextlib import contextmanager, asynccontextmanager
from functools import cache, cached_property, lru_cache, wraps
import abc
import functools

# ===== Errors =====

# --- Core: abstractmethod above descriptors ---

class AbstractAboveProperty:
    @abstractmethod  # RUF071
    @property
    def foo(self): ...

class AbstractAboveClassmethod:
    @abstractmethod  # RUF071
    @classmethod
    def foo(cls): ...

class AbstractAboveStaticmethod:
    @abstractmethod  # RUF071
    @staticmethod
    def foo(): ...

# --- Core: contextmanager above descriptors ---

class ContextManagerAboveStaticmethod:
    @contextmanager  # RUF071
    @staticmethod
    def foo(): ...

class ContextManagerAboveClassmethod:
    @contextmanager  # RUF071
    @classmethod
    def foo(cls): ...

class AsyncContextManagerAboveStaticmethod:
    @asynccontextmanager  # RUF071
    @staticmethod
    def foo(): ...

class AsyncContextManagerAboveClassmethod:
    @asynccontextmanager  # RUF071
    @classmethod
    def foo(cls): ...

# --- Functools: caching decorators above descriptors ---

class CacheAboveProperty:
    @cache  # RUF071
    @property
    def foo(self): ...

class LruCacheAboveProperty:
    @lru_cache  # RUF071
    @property
    def foo(self): ...

class CacheAboveClassmethod:
    @cache  # RUF071
    @classmethod
    def foo(cls): ...

class LruCacheAboveClassmethod:
    @lru_cache  # RUF071
    @classmethod
    def foo(cls): ...

class CacheAboveCachedProperty:
    @cache  # RUF071
    @cached_property
    def foo(self): ...

class LruCacheAboveCachedProperty:
    @lru_cache  # RUF071
    @cached_property
    def foo(self): ...

class ClassmethodAboveCachedProperty:
    @classmethod  # RUF071
    @cached_property
    def foo(cls): ...

# --- Qualified imports ---

class QualifiedAbstractAboveProperty:
    @abc.abstractmethod  # RUF071
    @property
    def foo(self): ...

class QualifiedLruCacheAboveProperty:
    @functools.lru_cache  # RUF071
    @property
    def foo(self): ...

# --- Deprecated abc forms ---

class DeprecatedAbstractPropertyBelowAbstractmethod:
    @abstractmethod  # RUF071
    @abstractproperty
    def foo(self): ...

class DeprecatedAbstractClassmethodBelowAbstractmethod:
    @abstractmethod  # RUF071
    @abstractclassmethod
    def foo(cls): ...

class DeprecatedAbstractStaticmethodBelowAbstractmethod:
    @abstractmethod  # RUF071
    @abstractstaticmethod
    def foo(): ...

# --- Decorator call form ---

class LruCacheCallAboveProperty:
    @lru_cache()  # RUF071
    @property
    def foo(self): ...

class LruCacheCallWithArgsAboveProperty:
    @lru_cache(maxsize=128)  # RUF071
    @property
    def foo(self): ...

# --- Multiple violations on same function (adjacent pairs only) ---

class MultipleViolations:
    @abstractmethod  # RUF071
    @property
    @contextmanager  # RUF071
    @classmethod
    def foo(cls): ...

# ===== No errors =====

# --- Correct orderings ---

class CorrectPropertyAboveAbstractmethod:
    @property
    @abstractmethod
    def foo(self): ...

class CorrectClassmethodAboveAbstractmethod:
    @classmethod
    @abstractmethod
    def foo(cls): ...

class CorrectStaticmethodAboveAbstractmethod:
    @staticmethod
    @abstractmethod
    def foo(): ...

class CorrectStaticmethodAboveContextmanager:
    @staticmethod
    @contextmanager
    def foo(): ...

class CorrectClassmethodAboveContextmanager:
    @classmethod
    @contextmanager
    def foo(cls): ...

class CorrectPropertyAboveCache:
    @property
    @cache
    def foo(self): ...

class CorrectPropertyAboveLruCache:
    @property
    @lru_cache
    def foo(self): ...

class CorrectAbstractmethodAboveCachedProperty:
    @abstractmethod
    @cached_property
    def foo(self): ...

class CachedPropertyAboveAbstractmethod:
    @cached_property
    @abstractmethod
    def foo(self): ...

# --- Non-adjacent known-bad pair (only adjacent pairs are checked) ---

class InterleavedDecorator:
    @abstractmethod
    @some_other_decorator
    @property
    def foo(self): ...

# --- Single decorators ---

class SingleAbstractmethod:
    @abstractmethod
    def foo(self): ...

class SingleProperty:
    @property
    def foo(self): ...

# --- Unrelated decorators only ---

class UnrelatedDecorators:
    @some_decorator
    @another_decorator
    def foo(self): ...

# --- Same decorator twice ---

class DuplicateDecorator:
    @abstractmethod
    @abstractmethod
    def foo(self): ...

# --- functools.wraps is not checked ---

class WrapsWithAnything:
    @wraps(some_func)
    @property
    def foo(self): ...

    @abstractmethod
    @wraps(some_func)
    def bar(self): ...
