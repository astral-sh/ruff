from __future__ import annotations

from airflow import Dataset as DatasetFromRoot
from airflow.datasets import (
    Dataset,
    DatasetAlias,
    DatasetAll,
    DatasetAny,
    expand_alias_to_datasets,
)
from airflow.datasets.metadata import Metadata
from airflow.models.baseoperator import chain, chain_linear, cross_downstream
from airflow.models.baseoperatorlink import BaseOperatorLink
from airflow.timetables.datasets import DatasetOrTimeSchedule
from airflow.utils.dag_parsing_context import get_parsing_context

DatasetFromRoot()

# airflow.datasets
Dataset()
DatasetAlias()
DatasetAll()
DatasetAny()
Metadata()
expand_alias_to_datasets()

# airflow.models.baseoperator
chain()
chain_linear()
cross_downstream()

# airflow.models.baseoperatolinker
BaseOperatorLink()

# airflow.timetables.datasets
DatasetOrTimeSchedule()

# airflow.utils.dag_parsing_context
get_parsing_context()


from airflow.decorators import dag, task, task_group, setup, teardown

dag()
task()
task_group()
setup()
teardown()

from airflow.models.dag import DAG as DAGFromDag
from airflow.models import DAG as DAGFromModel

DAGFromDag()
DAGFromModel()

from airflow.io.path import ObjectStoragePath
from airflow.io.storage import attach

ObjectStoragePath()
attach()
