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