from __future__ import annotations

from airflow.lineage.hook import HookLineageCollector

# airflow.lineage.hook
hlc = HookLineageCollector()
hlc.create_asset("there")
hlc.create_asset("should", "be", "no", "posarg")
hlc.create_asset(name="but", uri="kwargs are ok")
hlc.create_asset()

HookLineageCollector().create_asset(name="but", uri="kwargs are ok")
HookLineageCollector().create_asset("there")
HookLineageCollector().create_asset("should", "be", "no", "posarg")

args = ["uri_value"]
hlc.create_asset(*args)
HookLineageCollector().create_asset(*args)

# Literal unpacking
hlc.create_asset(*["literal_uri"])
HookLineageCollector().create_asset(*["literal_uri"])

# starred args with keyword args
hlc.create_asset(*args, extra="value")
HookLineageCollector().create_asset(*args, extra="value")

# Double-starred keyword arguments
kwargs = {"uri": "value", "name": "test"}
hlc.create_asset(**kwargs)
HookLineageCollector().create_asset(**kwargs)

# airflow.Dataset
from airflow import Dataset

extra_params = {"extra": "metadata"}

# second argument is of type dict, should raise diagnostic
Dataset("ds1", {"extra": "metadata"})
Dataset("ds1", {})
Dataset("ds1", dict())
Dataset("ds1", extra_params)
Dataset("ds1", {k: v for k, v in zip(["extra"], ["metadata"])})

# second argument is not of type dict, should not raise diagnostic
Dataset("ds1")
Dataset("ds1", extra={"key": "value"})
Dataset(uri="ds1", extra={"key": "value"})

# airflow.datasets.Dataset
from airflow.datasets import Dataset

Dataset("ds2", {"extra": "metadata"})
Dataset("ds2", {})
Dataset("ds2", dict())
Dataset("ds2", extra_params)
Dataset("ds2", {k: v for k, v in zip(["extra"], ["metadata"])})

Dataset("ds2")
Dataset("ds2", extra={"key": "value"})
Dataset(uri="ds2", extra={"key": "value"})

# airflow.sdk.Asset
from airflow.sdk import Asset

Asset("asset1", {"extra": "metadata"})
Asset("asset1", {})
Asset("asset1", dict())
Asset("asset1", extra_params)
Asset("asset1", {k: v for k, v in zip(["extra"], ["metadata"])})

Asset("asset1")
Asset("asset1", extra={"key": "value"})
Asset(uri="asset1", extra={"key": "value"})


from airflow.models.baseoperatorlink import BaseOperatorLink

# 3 positional args, 3rd NOT named 'ti_key'
class DeprecatedOperatorLink1(BaseOperatorLink):
    def get_link(self, operator, dttm):
        pass

# keyword-only ti_key
class ValidOperatorLink1(BaseOperatorLink):
    def get_link(self, operator, *, ti_key):
        pass

# positional ti_key with correct name
class ValidOperatorLink2(BaseOperatorLink):
    def get_link(self, operator, ti_key):
        pass

# Multiple keyword-only args with ti_key
class ValidOperatorLink3(BaseOperatorLink):
    def get_link(self, operator, *, ti_key, extra=None):
        pass

# *args with ti_key keyword-only
class ValidOperatorLink4(BaseOperatorLink):
    def get_link(self, operator, *args, ti_key):
        pass

# **kwargs alongside ti_key
class ValidOperatorLink5(BaseOperatorLink):
    def get_link(self, operator, *, ti_key, **kwargs):
        pass

# not exactly 3 positional args
class InvalidOperatorLink1(BaseOperatorLink):
    def get_link(self, operator):
        pass

class InvalidOperatorLink2(BaseOperatorLink):
    def get_link(self, operator, ti_key, extra):
        pass

class InvalidOperatorLink3(BaseOperatorLink):
    def get_link(self, operator, *, something):
        pass

# Not a subclass of BaseOperatorLink
class NotOperatorLinkSubclass:
    def get_link(self, operator, dttm):
        pass

# Not a class method
def get_link(operator, dttm):
    pass


from airflow.sdk import BaseOperatorLink

# 3 positional args, 3rd NOT named 'ti_key'
class DeprecatedOperatorLink2(BaseOperatorLink):
    def get_link(self, operator, dttm):
        pass

# keyword-only ti_key
class ValidOperatorLink6(BaseOperatorLink):
    def get_link(self, operator, *, ti_key):
        pass

# positional ti_key with correct name
class ValidOperatorLink7(BaseOperatorLink):
    def get_link(self, operator, ti_key):
        pass

# Multiple keyword-only args with ti_key
class ValidOperatorLink8(BaseOperatorLink):
    def get_link(self, operator, *, ti_key, extra=None):
        pass

# *args with ti_key keyword-only
class ValidOperatorLink9(BaseOperatorLink):
    def get_link(self, operator, *args, ti_key):
        pass

# **kwargs alongside ti_key
class ValidOperatorLink10(BaseOperatorLink):
    def get_link(self, operator, *, ti_key, **kwargs):
        pass

# not exactly 3 positional args
class InvalidOperatorLink4(BaseOperatorLink):
    def get_link(self, operator):
        pass

class InvalidOperatorLink5(BaseOperatorLink):
    def get_link(self, operator, ti_key, extra):
        pass

class InvalidOperatorLink6(BaseOperatorLink):
    def get_link(self, operator, *, something):
        pass

# Not a subclass of BaseOperatorLink
class NotOperatorLinkSubclass2:
    def get_link(self, operator, dttm):
        pass

# Not a class method
def get_link(operator, dttm):
    pass
