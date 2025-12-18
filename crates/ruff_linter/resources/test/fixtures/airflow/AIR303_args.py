from __future__ import annotations

from airflow.lineage.hook import HookLineageCollector

# airflow.lineage.hook
hlc = HookLineageCollector()
hlc.create_asset("there")
hlc.create_asset("should", "be", "no", "posarg")
hlc.create_asset(name="but", uri="kwargs are ok")
hlc.create_asset()