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
