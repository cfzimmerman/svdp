#!/usr/bin/env python3
"""Generate synthetic ServWare fixtures.

Field *names* and control types are reverse-engineered protocol facts and carry
no personal data. Every *value* here is invented. Real captures never enter the
repo: `.gitignore` globs `*.csv`/`*.har`, and CI fails the build if any is tracked.

The household-members fixtures plant sentinel values in the SSN and driver's
licence cells; `tests/exports.rs` asserts those never reach a parsed field or a
CSV. Do not remove them.
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

# The Household Members tab. Column order and header text are protocol facts;
# every value is invented. The SSN and driver's-licence cells carry sentinels so
# a test can assert they never reach a parsed field or a CSV.
MEMBER_COLUMNS = ["First Name", "Last Name", "Relationship", "Age",
                  "Phone", "SSN (Last 4)", "Drivers License/ID", "Disabled", "Notes"]

SENTINEL_SSN = "9999"
SENTINEL_DL = "SENTINELDLNEVERREAD"

# The neighbour themselves is NOT a row: ServWare lists only the other people in
# the house, verified across 139 real households (DECISIONS.md D19). A fixture
# with a "Self" row would pin a shape the live system does not produce, so the
# one fixture that has such a row is deliberately separate and exists only to
# prove the exporter excludes it.
HOUSEHOLD = [
    ("Peter", "Okonkwo", "Son", "17"),
    ("Daniel", "Okonkwo", "Son", "12"),
    ("Ruth", "Okonkwo", "Daughter", "8"),
]

def members_html(rows=None, extra_column=None):
    """`extra_column` inserts a header+cell before Age, to prove that columns are
    resolved by header text rather than by position."""
    rows = HOUSEHOLD if rows is None else rows
    heads = list(MEMBER_COLUMNS)
    if extra_column:
        heads.insert(heads.index("Age"), extra_column)
    head_html = "".join(f"<th>{h}</th>" for h in heads)
    body = ""
    for first, last, rel, age in rows:
        cells = [first, last, rel]
        if extra_column:
            cells.append("n/a")
        cells += [age, "555-0100", SENTINEL_SSN, SENTINEL_DL, "No", "SENTINELNOTE"]
        body += "<tr>" + "".join(f"<td>{c}</td>" for c in cells) + "</tr>"
    adults = sum(1 for r in rows if r[3] and int(r[3]) >= 18)
    return f'''<div id="tabs-familymembers" class="tab-pane">
  <div class="row"><div class="col-sm-12"><h4>Adults: {adults} &nbsp; Children: {len(rows) - adults}</h4></div></div>
  <table class="table table-striped table-condensed">
    <thead><tr>{head_html}</tr></thead>
    {body}
  </table>
</div>'''

def detail_page(*, status="Open", visit_completed=False, assigned="", items_html="",
                members=None):
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

{members_html() if members is None else members}

<table class="table table-striped table-condensed">
<tr><th>&nbsp;</th><th>Assistance</th><th>Value</th><th>Date Provided</th><th>Pending</th>
    <th>Promised Date</th><th>Chk Req</th><th>Chk/Conf Nbr</th><th>Notes</th><th>&nbsp;</th></tr>
{items_html}
</table>
</body></html>
"""

ITEM = ('<tr><td>&nbsp;</td><td>{name}</td><td>${value}</td><td>{date}</td><td>{pending}</td>'
        '<td></td><td>No</td><td></td><td>{notes}</td><td>&nbsp;</td></tr>')

# Deliberately hand-written rather than produced by `detail_page`: the point of
# this fixture is the *structure*, and spelling it out makes the trap visible.
NESTED_MODAL_PAGE = """<!DOCTYPE html>
<html><head><title>Assistance Request</title></head><body>
<form id="editForm" method="post" action="/app/assistancerequests/3724739">
  <select name="status"><option value="Open" selected>Open</option></select>
  <textarea name="visitNotes">&lt;p&gt;Prior visit&lt;/p&gt;</textarea>
  <input type="checkbox" name="visitCompleted" value="true"/>
  <div class="modal">
    <form id="sendEmailForm" method="post" action="/app/sendemail">
      <select name="sendToRoleId"><option value="3">Conference President</option></select>
    </form>
  </div>
  <input type="hidden" name="controlAfterTheModal" value="dropped-by-the-parser"/>
</form>
</body></html>
"""


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "detail_open.html").write_text(detail_page())
    # An open request the county has already assigned to someone at intake.
    (OUT / "detail_open_assigned.html").write_text(detail_page(assigned="44271"))
    (OUT / "detail_completed.html").write_text(detail_page(
        status="Completed", visit_completed=True, assigned="44270",
        items_html=(
            ITEM.format(name="Second Harvest Food", value="70.00", date="09/05/2026",
                        pending="No", notes="svdp:s=01JBQTESTSESSION;i=food — SVdP delivery")
            + ITEM.format(name="Gift Cards", value="90.00", date="09/05/2026",
                          pending="No", notes="svdp:s=01JBQTESTSESSION;i=giftcard — SVdP delivery")
            + ITEM.format(name="Gift Cards", value="50.00", date="01/03/2026",
                          pending="No", notes=""))))
    # Columns are resolved by header text, so an inserted column must not shift
    # the meaning of Age.
    (OUT / "detail_members_shifted.html").write_text(
        detail_page(members=members_html(extra_column="Nickname")))
    # Deliberately larger than the four columns a hand-built spreadsheet had room
    # for. Extraction must never saturate just because a human's layout did.
    (OUT / "detail_members_large.html").write_text(detail_page(members=members_html(rows=[
        ("Ana", "Bergstrom", "Spouse", "38"),
        ("Bo", "Bergstrom", "Son", "17"),
        ("Cleo", "Bergstrom", "Daughter", "15"),
        ("Dov", "Bergstrom", "Son", "13"),
        ("Esme", "Bergstrom", "Daughter", "11"),
        ("Fen", "Bergstrom", "Son", "9"),
        ("Gia", "Bergstrom", "Daughter", "6"),
        ("Hal", "Bergstrom", "Son", "4"),
        ("Ivy", "Bergstrom", "Daughter", "2"),
        ("Jun", "Bergstrom", "Grandparent", "71"),
    ])))
    # A household with an unrecorded age, and one with nobody but the neighbour.
    (OUT / "detail_members_sparse.html").write_text(detail_page(members=members_html(
        rows=[("Ada", "Nakamura", "Grandparent", "63"),
              ("Wren", "Nakamura", "Grandchild", "")])))
    # The tab and its table are present but list nobody: ServWare accepts a head
    # count OR individual people, and 19 of 158 households in one window had only
    # the head count. This is a legitimate answer and must parse as an empty
    # roster, NOT as a failure.
    (OUT / "detail_members_empty.html").write_text(detail_page(members=members_html(rows=[])))
    # No tab at all. That is ServWare's page having changed shape, which is a
    # different thing entirely, and must be an error rather than silently
    # reading as "this family has nobody" -- see DECISIONS.md D32.
    (OUT / "detail_members_absent.html").write_text(detail_page(members=""))
    # Another conference might record the neighbour as a row. Household size is
    # "rows plus one", so such a row has to be excluded or that family is counted
    # twice and can move up a gift-card rung.
    (OUT / "detail_members_with_self.html").write_text(detail_page(members=members_html(
        rows=[("Maria", "Okonkwo", "Self", "41"), ("Ruth", "Okonkwo", "Daughter", "8")])))
    # A modal nested INSIDE the edit form. html5ever drops the inner <form> start
    # tag and the first </form> closes the outer one, so the parsed tree both
    # gains the modal's controls and loses every real control after it. Nothing
    # downstream can detect that, so extraction refuses outright.
    (OUT / "detail_nested_modal.html").write_text(NESTED_MODAL_PAGE)

    for p in sorted(OUT.glob("*.html")):
        print(f"  {p.relative_to(OUT.parent.parent)}  {p.stat().st_size}B")

if __name__ == "__main__":
    main()
