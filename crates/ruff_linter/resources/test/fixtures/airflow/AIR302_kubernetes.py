from __future__ import annotations

from airflow.executors.kubernetes_executor_types import (
    ALL_NAMESPACES,
    POD_EXECUTOR_DONE_KEY,
)
from airflow.kubernetes.k8s_model import (
    K8SModel,
    append_to_pod,
)
from airflow.kubernetes.kube_client import (
    _disable_verify_ssl,
    _enable_tcp_keepalive,
    get_kube_client,
)
from airflow.kubernetes.kubernetes_helper_functions import (
    add_pod_suffix,
    annotations_for_logging_task_metadata,
    annotations_to_key,
    create_pod_id,
    get_logs_task_metadata,
    rand_str,
)
from airflow.kubernetes.pod import (
    Port,
    Resources,
)

ALL_NAMESPACES
POD_EXECUTOR_DONE_KEY

K8SModel()
append_to_pod()

_disable_verify_ssl()
_enable_tcp_keepalive()
get_kube_client()

add_pod_suffix()
create_pod_id()

annotations_for_logging_task_metadata()
annotations_to_key()
get_logs_task_metadata()
rand_str()

Port()
Resources()


from airflow.kubernetes.pod_generator import (
    PodDefaults,
    PodGenerator,
    PodGeneratorDeprecated,
    add_pod_suffix,
    datetime_to_label_safe_datestring,
    extend_object_field,
    label_safe_datestring_to_datetime,
    make_safe_label_value,
    merge_objects,
    rand_str,
)

datetime_to_label_safe_datestring()
extend_object_field()
label_safe_datestring_to_datetime()
make_safe_label_value()
merge_objects()
PodGenerator()
PodDefaults()
PodGeneratorDeprecated()
add_pod_suffix()
rand_str()


from airflow.kubernetes.pod_generator_deprecated import (
    PodDefaults,
    PodGenerator,
    make_safe_label_value,
)
from airflow.kubernetes.pod_launcher import (
    PodLauncher,
    PodStatus,
)

PodDefaults()
PodGenerator()
make_safe_label_value()

PodLauncher()
PodStatus()


from airflow.kubernetes.pod_launcher_deprecated import (
    PodDefaults,
    PodLauncher,
    PodStatus,
    get_kube_client,
)
from airflow.kubernetes.pod_runtime_info_env import PodRuntimeInfoEnv
from airflow.kubernetes.secret import (
    K8SModel,
    Secret,
)
from airflow.kubernetes.volume import Volume
from airflow.kubernetes.volume_mount import VolumeMount

PodDefaults()
PodLauncher()
PodStatus()
get_kube_client()

PodRuntimeInfoEnv()
K8SModel()
Secret()
Volume()
VolumeMount()
