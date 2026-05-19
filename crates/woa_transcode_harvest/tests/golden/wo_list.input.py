"""Minimal fixture mirroring woa/blueprints/vorgaenge_ops.py::wo_list."""

from flask import Blueprint, request, render_template
from woa.scoping import tenant_filter
from woa.decorators import login_required

bp = Blueprint("vorgaenge_ops", __name__)


@bp.route("/vorgaenge")
@login_required
def wo_list():
    q = request.args.get("q", "")
    status = request.args.get("status", "")
    dtype = request.args.get("type", "")
    query = WorkOrder.query.join(Customer)
    query = tenant_filter(query, WorkOrder)
    if q:
        like = f"%{q}%"
        query = query.filter(
            db.or_(
                Customer.firma.ilike(like),
                WorkOrder.rechnung_nr.ilike(like),
                WorkOrder.angebot_nr.ilike(like),
                WorkOrder.workorder_nr.ilike(like),
                WorkOrder.betreff.ilike(like),
            )
        )
    if status:
        query = query.filter(WorkOrder.status == status)
    if dtype:
        query = query.filter(WorkOrder.doc_type == dtype)
    orders = query.order_by(WorkOrder.updated_at.desc()).limit(200).all()
    return render_template(
        "workorders/list.html", orders=orders, q=q, status=status, dtype=dtype
    )
