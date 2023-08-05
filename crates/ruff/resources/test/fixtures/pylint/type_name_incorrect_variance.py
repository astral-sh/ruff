from typing import ParamSpec, TypeVar

# Errors.

T = TypeVar("T", covariant=True)
T = TypeVar("T", covariant=True, contravariant=False)
T = TypeVar("T", contravariant=True)
T = TypeVar("T", covariant=False, contravariant=True)
P = ParamSpec("P", covariant=True)
P = ParamSpec("P", covariant=True, contravariant=False)
P = ParamSpec("P", contravariant=True)
P = ParamSpec("P", covariant=False, contravariant=True)

T_co = TypeVar("T_co")
T_co = TypeVar("T_co", covariant=False)
T_co = TypeVar("T_co", contravariant=False)
T_co = TypeVar("T_co", covariant=False, contravariant=False)
T_co = TypeVar("T_co", contravariant=True)
T_co = TypeVar("T_co", covariant=False, contravariant=True)
P_co = ParamSpec("P_co")
P_co = ParamSpec("P_co", covariant=False)
P_co = ParamSpec("P_co", contravariant=False)
P_co = ParamSpec("P_co", covariant=False, contravariant=False)
P_co = ParamSpec("P_co", contravariant=True)
P_co = ParamSpec("P_co", covariant=False, contravariant=True)

T_contra = TypeVar("T_contra")
T_contra = TypeVar("T_contra", covariant=False)
T_contra = TypeVar("T_contra", contravariant=False)
T_contra = TypeVar("T_contra", covariant=False, contravariant=False)
T_contra = TypeVar("T_contra", covariant=True)
T_contra = TypeVar("T_contra", covariant=True, contravariant=False)
P_contra = ParamSpec("P_contra")
P_contra = ParamSpec("P_contra", covariant=False)
P_contra = ParamSpec("P_contra", contravariant=False)
P_contra = ParamSpec("P_contra", covariant=False, contravariant=False)
P_contra = ParamSpec("P_contra", covariant=True)
P_contra = ParamSpec("P_contra", covariant=True, contravariant=False)

# Non-errors.

T = TypeVar("T")
T = TypeVar("T", covariant=False)
T = TypeVar("T", contravariant=False)
T = TypeVar("T", covariant=False, contravariant=False)
P = ParamSpec("P")
P = ParamSpec("P", covariant=False)
P = ParamSpec("P", contravariant=False)
P = ParamSpec("P", covariant=False, contravariant=False)

T_co = TypeVar("T_co", covariant=True)
T_co = TypeVar("T_co", covariant=True, contravariant=False)
P_co = ParamSpec("P_co", covariant=True)
P_co = ParamSpec("P_co", covariant=True, contravariant=False)

T_contra = TypeVar("T_contra", contravariant=True)
T_contra = TypeVar("T_contra", covariant=False, contravariant=True)
P_contra = ParamSpec("P_contra", contravariant=True)
P_contra = ParamSpec("P_contra", covariant=False, contravariant=True)

# Bivariate types are errors, but not covered by this check.

T = TypeVar("T", covariant=True, contravariant=True)
P = ParamSpec("P", covariant=True, contravariant=True)
T_co = TypeVar("T_co", covariant=True, contravariant=True)
P_co = ParamSpec("P_co", covariant=True, contravariant=True)
T_contra = TypeVar("T_contra", covariant=True, contravariant=True)
P_contra = ParamSpec("P_contra", covariant=True, contravariant=True)
