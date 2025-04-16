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
from airflow.decorators import dag, setup, task, task_group, teardown
from airflow.io.path import ObjectStoragePath
from airflow.io.storage import attach
from airflow.models import DAG as DAGFromModel
from airflow.models.baseoperator import chain, chain_linear, cross_downstream
from airflow.models.baseoperatorlink import BaseOperatorLink
from airflow.models.dag import DAG as DAGFromDag
from airflow.timetables.datasets import DatasetOrTimeSchedule
from airflow.utils.dag_parsing_context import get_parsing_context

# airflow
DatasetFromRoot()

# airflow.datasets
Dataset()
DatasetAlias()
DatasetAll()
DatasetAny()
Metadata()
expand_alias_to_datasets()

# airflow.decorators
dag()
task()
task_group()
setup()
teardown()

# airflow.io
ObjectStoragePath()
attach()

# airflow.models
DAGFromModel()

# airflow.models.baseoperator
chain()
chain_linear()
cross_downstream()

# airflow.models.baseoperatolinker
BaseOperatorLink()

# airflow.models.dag
DAGFromDag()
# airflow.timetables.datasets
DatasetOrTimeSchedule()

# airflow.utils.dag_parsing_context
get_parsing_context()
