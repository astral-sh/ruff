# ruff: noqa: F821
"""Codegen fixture mirroring WoA route shapes for two handler-kinds.

- `device_list`  → list_for_tenant (GET render; model Customer; Customer is a
  FLAT model so the emitted path MUST be `crate::models::customer::Model`, not
  the doubled `crate::models::customer::customer::Model` Sonnet bug).
- `cash_journals_list` → list_for_tenant over a nested ERP model
  (`crate::models::erp::k6_cash::cash_journal::Model`).
- `device_delete` → soft_delete (POST; hard delete; tenant-scoped).
- `customer_delete` → soft_delete with `aktiv = False` (true soft-delete).
"""
from __future__ import annotations

from flask import Blueprint, redirect, render_template

bp = Blueprint("geraete", __name__)


@bp.route("/geraete")
@login_required
def device_list():
    devices = Customer.query.filter_by(tenant_id=g.tenant_id).order_by(
        Customer.id.asc()
    ).all()
    return render_template("devices/list.html", devices=devices, title="Geräte")


@bp.route("/erp/cash-journals")
@login_required
def cash_journals_list():
    cash_journals = ErpCashJournal.query.filter_by(tenant_id=g.tenant_id).order_by(
        ErpCashJournal.id.asc()
    ).all()
    return render_template(
        "erp/k6_cash/cash_journals.html",
        cash_journals=cash_journals,
        title="Kassenbücher",
    )


@bp.route("/geraete/<int:did>/delete", methods=["POST"])
@login_required
def device_delete(did):
    d = get_scoped_or_404(Device, did)
    db.session.delete(d)
    db.session.commit()
    flash("Gerät gelöscht.", "warning")
    return redirect("/geraete")


@bp.route("/kunden/<int:cid>/delete", methods=["POST"])
@login_required
def customer_delete(cid):
    c = get_scoped_or_404(Customer, cid)
    c.aktiv = False
    db.session.commit()
    flash("Kunde deaktiviert.", "warning")
    return redirect("/kunden")
