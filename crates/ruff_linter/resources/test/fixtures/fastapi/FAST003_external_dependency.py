from fastapi import APIRouter, Depends

# Assume get_user is defined elsewhere (e.g., another module)
# For testing, we might need to mock or represent this external dependency.
# However, the core issue is that its "unresolvability" stops other checks.
from .external_module import get_user # Or just comment out for ruff to treat as unresolved

router = APIRouter()

@router.get("/items/{item_id}/{user_id}")
def get_item_details(
    item_id: int,
    # user_id is in path but missing here
    current_user: str = Depends(get_user),
):
    return {"item_id": item_id, "user": current_user}

@router.get("/users/{user_param}")
def get_user_info(
    # user_param is in path but missing here
    current_user: str = Depends(get_user),
    admin_user: str = Depends(get_user) # Another one
):
    return {"user_param": "...", "user": current_user}

# Case from the issue
@router.get("/{address_id}/{user_id}")
def get_address(
    address_id: int,
    # user_id is missing
    current_user: str = Depends(get_user),
):
    return {"address_id": address_id }

# Control case: No missing path params, should pass
@router.get("/orders/{order_id}")
def get_order(
    order_id: int,
    current_user: str = Depends(get_user),
):
    return {"order_id": order_id}

# Control case: Missing path param, no Depends, should fail
@router.get("/products/{product_id}")
def get_product(
    # product_id is missing
):
    return {"product_id": "..." } 