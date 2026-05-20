"""Minimal Flask fixture for the `flask_view_identity` golden test."""

from flask import Blueprint, request, render_template
from flask_login import login_required

bp = Blueprint("orders", __name__)


@bp.route("/orders")
@login_required
def order_list():
    q = request.args.get("q", "")
    status = request.args.get("status", "")
    dtype = request.args.get("type", "")
    query = Order.query.join(Customer)
    if q:
        like = f"%{q}%"
        query = query.filter(
            db.or_(
                Customer.name.ilike(like),
                Order.invoice_no.ilike(like),
                Order.quote_no.ilike(like),
                Order.order_no.ilike(like),
                Order.subject.ilike(like),
            )
        )
    if status:
        query = query.filter(Order.status == status)
    if dtype:
        query = query.filter(Order.doc_type == dtype)
    orders = query.order_by(Order.updated_at.desc()).limit(200).all()
    return render_template(
        "orders/list.html", orders=orders, q=q, status=status, dtype=dtype
    )
