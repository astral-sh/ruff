import functools


class Fibonnaci:
    def __init__(self):
        self.result = []

    @functools.lru_cache(maxsize=None)  # [method-cache-max-size-none]
    def fibonacci(self, n):
        if n in {0, 1}:
            self.result.append(n)
            return n
        next = self.fibonacci(n - 1) + self.fibonacci(n - 2)
        self.result.append(next)
        return next

    @functools.cache  # [method-cache-max-size-none]
    def fibonacci2(self, n):
        if n in {0, 1}:
            self.result.append(n)
            return n
        next = self.fibonacci2(n - 1) + self.fibonacci2(n - 2)
        self.result.append(next)
        return next



@functools.cache
def cached_fibonacci(n):
    if n in {0, 1}:
        return n
    return cached_fibonacci(n - 1) + cached_fibonacci(n - 2)


class FibonnaciCorrect:
    def __init__(self):
        self.result = []

    @functools.lru_cache(maxsize=128)
    def fibonacci(self, n):
        self.result.append(cached_fibonacci(n))

    @functools.lru_cache()
    def fibonacci2(self, n):
        self.result.append(cached_fibonacci(n))
