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
from airflow.decorators import (
    dag,
    setup,
    task,
    task_group,
)

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
from airflow.decorators import teardown
from airflow.io.path import ObjectStoragePath
from airflow.io.store import attach
from airflow.models import DAG as DAGFromModel
from airflow.models import (
    Connection,
    Variable,
)
from airflow.models.baseoperator import chain, chain_linear, cross_downstream
from airflow.models.baseoperatorlink import BaseOperatorLink
from airflow.models.dag import DAG as DAGFromDag

# airflow.decorators
teardown()

# # airflow.io
ObjectStoragePath()
attach()

# airflow.models
Connection()
DAGFromModel()
Variable()

# airflow.models.baseoperator
chain()
chain_linear()
cross_downstream()

# airflow.models.baseoperatolinker
BaseOperatorLink()

# airflow.models.dag
DAGFromDag()
from airflow.timetables.datasets import DatasetOrTimeSchedule
from airflow.utils.dag_parsing_context import get_parsing_context

# airflow.timetables.datasets
DatasetOrTimeSchedule(datasets=[])

# airflow.utils.dag_parsing_context
get_parsing_context()

from airflow.decorators.base import (
    DecoratedMappedOperator,
    DecoratedOperator,
    TaskDecorator,
    get_unique_task_id,
    task_decorator_factory,
)

# airflow.decorators.base
DecoratedMappedOperator()
DecoratedOperator()
TaskDecorator()
get_unique_task_id()
task_decorator_factory()


from airflow.models import DagParam, Param, ParamsDict

# airflow.models
Param()
DagParam()
ParamsDict()


from airflow.models.param import DagParam, Param, ParamsDict

# airflow.models.param
Param()
DagParam()
ParamsDict()


from airflow.sensors.base import (
    BaseSensorOperator,
    PokeReturnValue,
    poke_mode_only,
)

# airflow.sensors.base
BaseSensorOperator()
PokeReturnValue()
poke_mode_only()
