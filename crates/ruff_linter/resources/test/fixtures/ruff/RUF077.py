"""Tests for RUF077 (method-receiver-default)."""

class InstanceReceiverDefault:
    def method(self=None): ...  # RUF077


class ClassReceiverDefault:
    @classmethod
    def build(cls=None): ...  # RUF077


class NewMethodClassReceiver:
    def __new__(cls=None): ...  # RUF077


# No errors
class StaticMethodWithDefault:
    @staticmethod
    def helper(arg=None): ...


class InstanceReceiverNoDefault:
    def method(self): ...


class ClassReceiverNoDefault:
    @classmethod
    def build(cls): ...
