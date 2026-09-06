#!/usr/bin/env python3
"""Generate synthetic ServWare fixtures.

Field *names* and control types are reverse-engineered protocol facts and carry
no personal data. Every *value* here is invented. Real captures never enter the
repo; see tests/no_pii.rs for the enforcing assertion.
"""
import pathlib

OUT = pathlib.Path(__file__).resolve().parent.parent / "tests" / "fixtures"

# (name, kind, value/opts, checked) mirroring the live request edit form.
SELECT_STATUS = ["Open", "Completed", "Denied"]
MEMBERS = [("", "-- Select --"), ("44270", "Ada Lovelace"), ("44271", "Grace Hopper")]
ORGS = [("", "-- Select --"), ("900", "County Referral"), ("901", "Parish Referral")]

def opts(pairs, selected):
    return "".join(
        f'<option value="{v}"{" selected" if v == selected else ""}>{t}</option>'
        for v, t in pairs
    )

def checkbox(name, checked):
    """Spring MVC pair: the control, plus its always-present `_name` companion."""
    c = ' checked' if checked else ''
    return (f'<input type="checkbox" name="{name}" value="true"{c}/>'
            f'<input type="hidden" name="_{name}" value="on"/>')

def detail_page(*, status="Open", visit_completed=False, assigned="", items_html=""):
    return f"""<!DOCTYPE html>
<html><head><title>Assistance Request</title></head><body>
<form id="editForm" method="post" action="/app/assistancerequests/3724739">
  <select name="status">{opts([(s, s) for s in SELECT_STATUS], status)}</select>
  <input type="hidden" name="denialReasonId" value=""/>
  <input type="hidden" name="denialReasonStr" value=""/>
  <input type="hidden" name="clientFirstName" value="Jane"/>
  <input type="hidden" name="clientLastName" value="Doe"/>
  <input type="text" name="dateRequested" value="02/02/2026"/>
  <select id="requestAssignedToMemberId" name="requestAssignedToMemberId">{opts(MEMBERS, assigned)}</select>
  <textarea name="requestNote">&lt;p&gt;Needs food assistance&lt;/p&gt;</textarea>
  <input type="file"/>
  {checkbox("homeVisitRequired", True)}
  <input type="number" name="homeVisitCnt" value="1"/>
  {checkbox("otherVisit", False)}<input type="number" name="otherVisitCnt" value="3"/>
  {checkbox("elderCareVisit", False)}<input type="number" name="eldercareVisitCnt" value="4"/>
  {checkbox("hospitalVisit", False)}<input type="number" name="hospitalVisitCnt" value="5"/>
  {checkbox("prisonVisit", False)}<input type="number" name="prisonVisitCnt" value="6"/>
  {checkbox("telephoneVisit", False)}<input type="number" name="phoneVisitCnt" value="7"/>
  {checkbox("churchPantryVisit", False)}<input type="number" name="churchPantryVisitCnt" value="8"/>
  {checkbox("visitCompleted", visit_completed)}
  <select id="homeVisitAssignedFirst" name="visitAssignedToMemberId">{opts(MEMBERS, assigned)}</select>
  <select id="homeVisitAssignedSecond" name="visitAssignedToMemberIdSecondary">{opts(MEMBERS, "")}</select>
  <input type="number" name="visitMileageInService" value="12"/>
  <input type="number" name="visitHoursInService" value="2"/>
  <input type="text" name="visitScheduledDate" value="02/07/2026"/>
  <input type="text" name="visitScheduledTime" value="09:30"/>
  <input type="number" name="peopleHelpedOverride" value=""/>
  <textarea name="visitNotes">&lt;p&gt;Prior visit&lt;/p&gt;</textarea>
  {checkbox("referredToAgency", False)}
  <select id="referralOrgId" name="referralOrganizationId">{opts(ORGS, "900")}</select>
  <select id="referralOrgId2" name="referralOrganizationId2">{opts(ORGS, "")}</select>
  {checkbox("referredToConference", False)}
  <input type="hidden" name="referralConference" value="17"/>
  <select name="referredFromOrganizationId">{opts(ORGS, "")}</select>
  <textarea name="referralNote"></textarea>
  <input type="text" name="ignoredBecauseDisabled" value="nope" disabled/>
  <button type="submit">Save</button>
</form>

<!-- A modal form on the same page. Extraction is form-scoped and must ignore it. -->
<form id="sendEmailForm" method="post" action="/app/sendemail">
  <select name="sendToRoleId"><option value="3">Conference President</option></select>
  <input type="hidden" name="clientMapBoundaryScope" value="conference"/>
</form>

<table class="table table-striped table-condensed">
<tr><th>&nbsp;</th><th>Assistance</th><th>Value</th><th>Date Provided</th><th>Pending</th>
    <th>Promised Date</th><th>Chk Req</th><th>Chk/Conf Nbr</th><th>Notes</th><th>&nbsp;</th></tr>
{items_html}
</table>
</body></html>
"""

ITEM = ('<tr><td>&nbsp;</td><td>{name}</td><td>${value}</td><td>{date}</td><td>{pending}</td>'
        '<td></td><td>No</td><td></td><td>{notes}</td><td>&nbsp;</td></tr>')

def main():
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "detail_open.html").write_text(detail_page())
    (OUT / "detail_completed.html").write_text(detail_page(
        status="Completed", visit_completed=True, assigned="44270",
        items_html=(
            ITEM.format(name="Second Harvest Food", value="70.00", date="09/05/2026",
                        pending="No", notes="svdp:s=01JBQTESTSESSION;i=food — SVdP delivery")
            + ITEM.format(name="Gift Cards", value="90.00", date="09/05/2026",
                          pending="No", notes="svdp:s=01JBQTESTSESSION;i=giftcard — SVdP delivery")
            + ITEM.format(name="Gift Cards", value="50.00", date="01/03/2026",
                          pending="No", notes=""))))
    for p in sorted(OUT.glob("*.html")):
        print(f"  {p.relative_to(OUT.parent.parent)}  {p.stat().st_size}B")

if __name__ == "__main__":
    main()
