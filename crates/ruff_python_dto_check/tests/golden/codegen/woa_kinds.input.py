# ruff: noqa: F821
"""Codegen fixture covering the remaining handler-kinds (Task B).

One route per kind so each emitter has a golden assertion:
- device_detail  → detail_for_tenant (GET, path param, scoped fetch, render)
- erp_index      → template_get (GET render, no model query)
- toggle_device  → toggle_bool_field (POST, scoped fetch, flip bool, redirect)
- system_index   → get_redirect_shortcut (GET, redirect, no render)
- device_create  → csrf_form_post_engine_call (POST, form fields, redirect)
- device_edit    → form_get_post (GET render + POST handle, form fields)
- dashboard_json → ajax_json (jsonify response)
- device_qr      → download_blob (send_file, non-PDF)
- workorder_pdf  → pdf_render (send_file + pdf-named)
- sa_health      → sa_admin_view (sa_-prefixed, GET render)
- portal_auto    → signed_link_action (auto_login function-name pattern)
"""

from __future__ import annotations

from flask import Blueprint, jsonify, redirect, render_template, request, send_file

bp = Blueprint("geraete", __name__)


@bp.route("/geraete/<int:did>")
@login_required
def device_detail(did):
    d = get_scoped_or_404(Device, did)
    return render_template("devices/detail.html", d=d)


@bp.route("/erp")
@login_required
def erp_index():
    return render_template("erp/index.html", title="ERP")


@bp.route("/geraete/<int:did>/toggle", methods=["POST"])
@login_required
def toggle_device(did):
    d = get_scoped_or_404(Device, did)
    d.aktiv = not d.aktiv
    db.session.commit()
    return redirect("/geraete")


@bp.route("/")
def system_index():
    return redirect("/dashboard")


@bp.route("/geraete/neu/save", methods=["POST"])
@login_required
def device_create():
    hostname = request.form.get("hostname")
    model = request.form.get("model")
    create_device(hostname, model)
    return redirect("/geraete")


@bp.route("/geraete/<int:did>/edit", methods=["GET", "POST"])
@login_required
def device_edit(did):
    d = get_scoped_or_404(Device, did)
    if request.method == "POST":
        d.hostname = request.form.get("hostname")
        d.standort = request.form.get("standort")
        db.session.commit()
        return redirect("/geraete")
    return render_template("devices/form.html", d=d)


@bp.route("/api/dashboard/stats")
@login_required
def dashboard_json():
    return jsonify(open_count=5, overdue=2)


@bp.route("/geraete/<int:did>/qr")
@login_required
def device_qr(did):
    return send_file(build_qr(did), mimetype="image/png")


@bp.route("/vorgaenge/<int:wid>/pdf")
@login_required
def workorder_pdf(wid):
    return send_file(build_pdf(wid), mimetype="application/pdf")


@bp.route("/sa/health")
@login_required
def sa_health():
    return render_template("superadmin/health.html")


@bp.route("/portal/auto-login")
def portal_auto_login():
    token = request.args.get("t")
    if not validate_auto_login_token(token):
        return redirect("/portal/login")
    return redirect("/portal/dashboard")
