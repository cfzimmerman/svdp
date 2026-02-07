# ServWare API Reference

> **Reverse-engineered from browser network captures (HAR files), February 2026.**
> There are no official public API docs for ServWare. This document was produced by
> analyzing four browser sessions: login, listing requests, marking a request complete,
> and adding assistance items.

---

## Table of Contents

1. [Overview](#1-overview)
2. [Authentication](#2-authentication)
3. [Home / Dashboard](#3-home--dashboard)
4. [Assistance Requests List](#4-assistance-requests-list)
5. [Assistance Request Detail](#5-assistance-request-detail)
6. [Update Assistance Request](#6-update-assistance-request)
7. [Add Assistance Item](#7-add-assistance-item)
8. [Helper / Lookup Endpoints](#8-helper--lookup-endpoints)
9. [Calendar Endpoints](#9-calendar-endpoints)
10. [Notes and Conventions](#10-notes-and-conventions)

---

## 1. Overview

| Property | Value |
|----------|-------|
| Base URL | `https://www.servware.org` |
| Server | nginx, HTTP/2 |
| Content types | HTML pages + JSON API via XHR |
| Auth | Session-based (HttpOnly cookies, not visible in HAR exports) |
| Frontend | jQuery 1.10.1, DataTables (server-side processing), Bootstrap 3 |
| App version | `org.servware-1.17.0-20260129-034339` |
| Session timeout | 3600 seconds (1 hour), warning 2 min before expiry |

All responses include these security headers:

```
cache-control: no-cache, no-store, max-age=0, must-revalidate
expires: 0
pragma: no-cache
strict-transport-security: max-age=31536000 ; includeSubDomains
x-content-type-options: nosniff
x-frame-options: SAMEORIGIN
x-xss-protection: 0
```

### Cookie jar setup

All `curl` examples below use a shared cookie jar variable. Set it once per session:

```bash
export COOKIE_JAR="/tmp/servware-cookies.txt"
```

---

## 2. Authentication

### POST /security/login

Authenticate and establish a session.

| Property | Value |
|----------|-------|
| Content-Type | `application/x-www-form-urlencoded` |
| Response | `302` redirect to `/app/home?continue` |
| CSRF token | None required |

**Form fields:**

| Field | Description |
|-------|-------------|
| `username` | ServWare username |
| `password` | ServWare password |

```bash
# Login (saves session cookies to jar)
curl -c "$COOKIE_JAR" -L \
  -d 'username=YOUR_USERNAME&password=YOUR_PASSWORD' \
  'https://www.servware.org/security/login'
```

The `-L` flag follows the 302 redirect to `/app/home?continue`. The server sets
HttpOnly session cookies that `curl -c` will capture.

### POST /security/logout

End the current session.

```bash
curl -b "$COOKIE_JAR" -X POST \
  'https://www.servware.org/security/logout'
```

### GET /security/extendSession

Extend the session timeout (resets the 1-hour timer).

```bash
curl -b "$COOKIE_JAR" \
  'https://www.servware.org/security/extendSession'
```

---

## 3. Home / Dashboard

### GET /app/home

Returns the HTML dashboard page. Useful for verifying login succeeded.

```bash
curl -b "$COOKIE_JAR" \
  'https://www.servware.org/app/home'
```

### GET /app/home/statistics

Returns fiscal-year statistics as a JSON array.

```bash
curl -b "$COOKIE_JAR" \
  -H 'X-Requested-With: XMLHttpRequest' \
  'https://www.servware.org/app/home/statistics'
```

**Response:**

```json
[
  { "count": 473, "countDescription": "Completed Requests (FY)" },
  { "count": 34,  "countDescription": "Open Requests (FY)" },
  { "count": 5,   "countDescription": "Completed Requests - Feb" },
  { "count": 464, "countDescription": "Home Visits Completed (FY)" },
  { "count": 9,   "countDescription": "Home Visits Completed - Feb" },
  { "count": 1,   "countDescription": "Other Visits Completed (FY)" },
  { "count": 0,   "countDescription": "Other Visits Completed - Feb" },
  { "count": 388, "countDescription": "Total Neighbors" }
]
```

**Schema:** `Array<{ count: number, countDescription: string }>`

---

## 4. Assistance Requests List

This is the primary endpoint for querying requests. It uses the
[DataTables server-side processing protocol](https://datatables.net/manual/server-side)
(legacy v1.x Hungarian notation format).

### GET /app/assistancerequests/list

```bash
# List all open requests, sorted by date ascending
curl -b "$COOKIE_JAR" \
  -H 'X-Requested-With: XMLHttpRequest' \
  -H 'Accept: application/json, text/javascript, */*; q=0.01' \
  'https://www.servware.org/app/assistancerequests/list?sEcho=1&iColumns=12&sColumns=id%2Cid%2Cstatus%2CdateRequested%2Cclient.lastName%2Cclient.firstName%2CrequestAssignedToMember%2CstreetAddressLine1%2Cclient.homePhone%2Cclient.mobilePhone%2CpendingItems%2Cid&iDisplayStart=0&iDisplayLength=100&mDataProp_0=id&mDataProp_1=id&mDataProp_2=status&mDataProp_3=dateRequested&mDataProp_4=client.lastName&mDataProp_5=client.firstName&mDataProp_6=requestAssignedToMember&mDataProp_7=streetAddressLine1&mDataProp_8=client.homePhone&mDataProp_9=client.mobilePhone&mDataProp_10=id&mDataProp_11=id&iSortCol_0=3&sSortDir_0=asc&iSortingCols=1&bSortable_0=false&bSortable_1=false&bSortable_2=true&bSortable_3=true&bSortable_4=true&bSortable_5=true&bSortable_6=false&bSortable_7=false&bSortable_8=false&bSortable_9=false&bSortable_10=false&bSortable_11=false&sSearch=&bRegex=false&filterByStatus=Open&filterByPartnerConf=&filterByReqAssigned=&filterByVisitAssigned=&_=1770500058340'
```

### Query Parameters

#### DataTables standard parameters

| Parameter | Example | Description |
|-----------|---------|-------------|
| `sEcho` | `1` | Request counter (echoed in response for matching) |
| `iColumns` | `12` | Number of columns |
| `sColumns` | `id,id,status,...` | Comma-separated column identifiers |
| `iDisplayStart` | `0` | Pagination offset (0-based) |
| `iDisplayLength` | `100` | Page size |
| `iSortCol_0` | `3` | Sort by column index (3 = dateRequested) |
| `sSortDir_0` | `asc` | Sort direction (`asc` or `desc`) |
| `iSortingCols` | `1` | Number of sort columns |
| `sSearch` | `""` | Global search term |
| `bRegex` | `false` | Whether search is regex |
| `mDataProp_N` | `id`, `status`, ... | Data property for column N |
| `bSortable_N` | `true`/`false` | Whether column N is sortable |
| `_` | `1770500058340` | jQuery cache-buster (Unix ms timestamp) |

#### ServWare custom filter parameters

| Parameter | Values | Description |
|-----------|--------|-------------|
| `filterByStatus` | `""` (all), `"Open"`, `"Completed"` | Filter by request status |
| `filterByPartnerConf` | `""` | Filter by partner conference |
| `filterByReqAssigned` | `""` | Filter by request assignment |
| `filterByVisitAssigned` | `""` | Filter by visit assignment |

#### Column definitions

| Index | sColumns value | mDataProp | Sortable | Description |
|-------|---------------|-----------|----------|-------------|
| 0 | `id` | `id` | no | Checkbox |
| 1 | `id` | `id` | no | Request ID link |
| 2 | `status` | `status` | yes | Status |
| 3 | `dateRequested` | `dateRequested` | yes | Date requested |
| 4 | `client.lastName` | `client.lastName` | yes | Last name |
| 5 | `client.firstName` | `client.firstName` | yes | First name |
| 6 | `requestAssignedToMember` | `requestAssignedToMember` | no | Assigned member |
| 7 | `streetAddressLine1` | `streetAddressLine1` | no | Street address |
| 8 | `client.homePhone` | `client.homePhone` | no | Home phone |
| 9 | `client.mobilePhone` | `client.mobilePhone` | no | Mobile phone |
| 10 | `pendingItems` | `id` | no | Pending items |
| 11 | `id` | `id` | no | Actions |

### Response Envelope

```json
{
  "sEcho": 1,
  "iTotalRecords": 4077,
  "iTotalDisplayRecords": 34,
  "additionalData": {},
  "aaData": [ ... ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `sEcho` | number | Echoes request `sEcho` |
| `iTotalRecords` | number | Total records in dataset |
| `iTotalDisplayRecords` | number | Records matching current filter |
| `additionalData` | object | Always empty `{}` |
| `aaData` | array | Array of AssistanceRequest objects |

### AssistanceRequest Object Schema

Each object in `aaData` has this structure. Fields marked with `*` are the most
useful for the CSV export use case.

```
AssistanceRequest {
  // --- Metadata ---
  id: number *                        // e.g. 3738446
  version: number
  markedForDeletion: boolean
  dateCreated: string                 // "MM/DD/YYYY HH:MM AM/PM"
  dateModified: string
  createdBy: string
  modifiedBy: string

  // --- Core Request Fields ---
  status: string *                    // "Open", "Completed"
  dateRequested: string *             // "MM/DD/YYYY"
  requestNote: string                 // HTML, e.g. "<p>Needs food assistance</p>"
  denialReason: string | null
  intakePerson: string | null
  caseNumber: string | null

  // --- Address ---
  streetAddressLine1: string *
  streetAddressLine2: string
  city: string *
  stateCode: string                   // e.g. "CA"
  postalCode: string

  // --- Assignment ---
  requestAssignedToMember: string | null *  // display name
  visitAssignedTo: string | null
  visitAssignedToMember: string | null
  visitAssignedToMemberSecondary: string | null

  // --- Household ---
  householdAdultCount: number | null
  householdChildCount: number | null
  calculatedAdultCount: number
  calculatedChildCount: number
  calculatedHouseholdCount: number *
  peopleHelpedOverride: number | null
  householdIncomeLevel: string | null
  householdIncomeLevelDesc: string | null

  // --- Client Characteristics ---
  parishioner: boolean
  homeless: boolean
  disabledClient: boolean

  // --- Visit Info ---
  homeVisitRequired: boolean
  homeVisitScheduled: string | null
  visitCompleted: boolean
  visitScheduledDate: string | null
  visitScheduledDurationMinutes: number | null
  visitNotes: string
  visitMileageHrsInSvc: number | null
  visitType: string | null
  homeVisitCnt: number | null

  // --- Visit Type Flags ---
  otherVisit: boolean
  prisonVisit: boolean
  hospitalVisit: boolean
  elderCareVisit: boolean
  telephoneVisit: boolean
  churchPantryVisit: boolean

  // --- Visit Counts ---
  otherVisitCnt: number | null
  prisonVisitCnt: number | null
  hospitalVisitCnt: number | null
  eldercareVisitCnt: number | null
  phoneVisitCnt: number | null
  churchPantryVisitCnt: number | null

  // --- Referrals ---
  referredToConference: boolean
  referralConference: object | null
  referredToAgency: boolean
  referredFromOrg: string | null
  referralNote: string
  referralOrganization: object | null

  // --- Conference/Partner ---
  partnerConference: object | null
  clientCounty: string | null
  conferenceViewRequired: boolean
  initiatedByDistrict: boolean
  initiatedByCouncil: boolean | null

  // --- Assistance ---
  includesOtherPayments: boolean
  requestedItems: array               // usually empty []
  assistanceItems: AssistanceItem[]    // see below
  pantryId: string | null

  // --- Nested ---
  client: Client                      // see below
}
```

### Client Object (nested in AssistanceRequest)

```
Client {
  id: number
  firstName: string *
  lastName: string *
  middleInitial: string
  maidenName: string
  birthDate: string
  gender: string
  ethnicity: string
  primaryLanguage: string             // e.g. "Spanish"
  maritalStatus: string

  // --- Contact ---
  homePhone: string *
  workPhone: string
  mobilePhone: string *
  emailAddress: string
  textCommunicationPreferred: boolean

  // --- Address ---
  streetAddressLine1: string
  streetAddressLine2: string
  city: string
  stateCode: string
  postalCode: string

  // --- Status ---
  parishioner: boolean
  homeless: boolean
  disabledClient: boolean
  veteran: boolean
  privateClient: boolean

  // --- Notes ---
  notes: string
  alertNote: string

  // --- Other ---
  lastRequestDate: string             // "MM/DD/YYYY"
  assignedMember: string | null
  openFollowUp: boolean
  followUps: array

  // --- Nested ---
  conference: Conference              // large config object, mostly ignorable
}
```

### AssistanceItem Object (nested in AssistanceRequest.assistanceItems[])

```
AssistanceItem {
  id: number
  monetaryValue: number *
  totalAssistanceItemValue: number
  quantity: number
  pending: boolean
  dateProvided: string                // "MM/DD/YYYY"
  promisedDate: string | null
  datePaid: string | null
  notes: string
  subType: string | null

  // --- Check/Payment ---
  checkRequested: boolean
  checkNumber: string
  payeeName: string

  // --- Nested ---
  assistanceType: AssistanceType      // see below
}
```

### AssistanceType Object (nested in AssistanceItem)

```
AssistanceType {
  id: number *                        // use this as assistanceTypeId
  name: string *                      // e.g. "Gift Cards", "Second Harvest Food"
  abbrName: string
  description: string
  active: boolean
  monetaryValue: number | null        // default value, if configured
  allowQuantityToBeSpecified: boolean
  trackQuantity: boolean
}
```

### Conference / District Objects

Each response includes deeply nested `Conference` and `District` configuration
objects with dozens of boolean flags. These are organizational settings and are
**mostly ignorable** for the CSV export use case. Notable fields:

- `conference.conferenceName` (e.g. `"Nativity"`)
- `conference.timeZone` (e.g. `"America/Los_Angeles"`)
- `conference.district.districtName` (e.g. `"California - San Mateo"`)

> **Data volume note:** Because the Conference/District config is duplicated in
> every record, a response with 34 open requests is still quite large. Plan for
> significant JSON payload sizes.

---

## 5. Assistance Request Detail

### GET /app/assistancerequests/{id}

Returns the HTML detail/edit page for a single request.

```bash
curl -b "$COOKIE_JAR" \
  'https://www.servware.org/app/assistancerequests/3724739'
```

This page loads and then fires three XHR requests automatically (see
[Helper / Lookup Endpoints](#8-helper--lookup-endpoints)).

---

## 6. Update Assistance Request

### POST /app/assistancerequests/{id}

Update a request (e.g., mark as completed). This is a standard HTML form POST.

| Property | Value |
|----------|-------|
| Content-Type | `application/x-www-form-urlencoded` |
| Response | `302` redirect to `/app/assistancerequests/{id}` |

```bash
# Mark request 3724739 as Completed with a home visit
curl -b "$COOKIE_JAR" -L \
  -d 'status=Completed'\
'&denialReasonId='\
'&denialReasonStr='\
'&clientFirstName=Jane'\
'&clientLastName=Doe'\
'&dateRequested=02%2F02%2F2026'\
'&requestAssignedToMemberId=44270'\
'&requestNote=%3Cp%3ENeeds+food+assistance%3C%2Fp%3E'\
'&files='\
'&homeVisitRequired=true'\
'&_homeVisitRequired=on'\
'&_otherVisit=on'\
'&_elderCareVisit=on'\
'&_hospitalVisit=on'\
'&_prisonVisit=on'\
'&_telephoneVisit=on'\
'&_churchPantryVisit=on'\
'&homeVisitCnt=1'\
'&visitCompleted=true'\
'&_visitCompleted=on'\
'&visitAssignedToMemberId=44270'\
'&visitAssignedToMemberIdSecondary='\
'&visitMileageInService=5'\
'&visitHoursInService='\
'&visitScheduledDate=02%2F07%2F2026'\
'&visitScheduledTime='\
'&peopleHelpedOverride='\
'&visitNotes=%3Cp%3EDelivered+food+and+gift+cards%3C%2Fp%3E'\
'&files='\
'&_referredToAgency=on'\
'&_referredToConference=on'\
'&referredFromOrganizationId='\
'&referralNote=' \
  'https://www.servware.org/app/assistancerequests/3724739'
```

### Form Fields

#### Status

| Field | Example | Description |
|-------|---------|-------------|
| `status` | `Completed` | Target status: `Open`, `Completed`, `Denied` |
| `denialReasonId` | `""` | Denial reason ID (when status = Denied) |
| `denialReasonStr` | `""` | Free-text denial reason |

#### Client Info

| Field | Example | Description |
|-------|---------|-------------|
| `clientFirstName` | `Jane` | Client first name |
| `clientLastName` | `Doe` | Client last name |
| `dateRequested` | `02/02/2026` | Request date (URL-encode the `/`) |

#### Assignment

| Field | Example | Description |
|-------|---------|-------------|
| `requestAssignedToMemberId` | `44270` | Numeric member ID |
| `requestNote` | `<p>...</p>` | HTML-formatted notes (URL-encoded) |
| `files` | `""` | File attachment field (appears twice) |

#### Visit Type Checkboxes

See [Checkbox Convention](#checkbox-convention) in Notes section.

| Field | When checked | When unchecked |
|-------|-------------|----------------|
| `homeVisitRequired` | `=true` + `_homeVisitRequired=on` | `_homeVisitRequired=on` only |
| (same pattern for:) | `otherVisit`, `elderCareVisit`, `hospitalVisit`, `prisonVisit`, `telephoneVisit`, `churchPantryVisit` | |

#### Visit Details

| Field | Example | Description |
|-------|---------|-------------|
| `homeVisitCnt` | `1` | Number of home visits |
| `visitCompleted` | `true` | Checkbox: visit completed |
| `_visitCompleted` | `on` | Hidden companion field |
| `visitAssignedToMemberId` | `44270` | Visit assignee member ID |
| `visitAssignedToMemberIdSecondary` | `""` | Optional second assignee |
| `visitMileageInService` | `5` | Mileage for visit |
| `visitHoursInService` | `""` | Hours for visit |
| `visitScheduledDate` | `02/07/2026` | Visit date (URL-encode `/`) |
| `visitScheduledTime` | `""` | Optional time |
| `peopleHelpedOverride` | `""` | Override count of people helped |
| `visitNotes` | `<p>...</p>` | HTML-formatted visit notes |

#### Referral

| Field | Example | Description |
|-------|---------|-------------|
| `_referredToAgency` | `on` | Checkbox companion (unchecked) |
| `_referredToConference` | `on` | Checkbox companion (unchecked) |
| `referredFromOrganizationId` | `""` | Organization ID |
| `referralNote` | `""` | Referral notes |

---

## 7. Add Assistance Item

### POST /app/assistancerequests/{id}/assistanceitems/new

Add an assistance item to a request.

| Property | Value |
|----------|-------|
| Content-Type | `application/x-www-form-urlencoded` |
| Response | `302` redirect (see `action` field) |

```bash
# Add a "Second Harvest Food" item ($70) and return to the form to add another
curl -b "$COOKIE_JAR" -L \
  -d 'assistanceTypeId=16542'\
'&clientId=580815'\
'&monetaryValue=70'\
'&quantity=1'\
'&dateProvided=02%2F07%2F2026'\
'&promisedDate='\
'&notes='\
'&clientAccountName='\
'&clientAccountHolder='\
'&clientAccountNumber='\
'&payeeName='\
'&_checkRequested=on'\
'&action=saveadd' \
  'https://www.servware.org/app/assistancerequests/3724739/assistanceitems/new'
```

```bash
# Add a "Gift Cards" item ($100) and return to the request detail page
curl -b "$COOKIE_JAR" -L \
  -d 'assistanceTypeId=16522'\
'&clientId=580815'\
'&monetaryValue=100'\
'&quantity=1'\
'&dateProvided=02%2F07%2F2026'\
'&promisedDate='\
'&notes='\
'&clientAccountName='\
'&clientAccountHolder='\
'&clientAccountNumber='\
'&payeeName='\
'&_checkRequested=on'\
'&action=save' \
  'https://www.servware.org/app/assistancerequests/3724739/assistanceitems/new'
```

### Form Fields

| Field | Example | Description |
|-------|---------|-------------|
| `assistanceTypeId` | `16542` | Assistance type ID (from dropdown) |
| `clientId` | `580815` | Client ID |
| `monetaryValue` | `70` | Dollar amount |
| `quantity` | `1` | Quantity |
| `dateProvided` | `02/07/2026` | Date provided (URL-encode `/`) |
| `promisedDate` | `""` | Optional promised date |
| `notes` | `""` | Notes |
| `clientAccountName` | `""` | Account name |
| `clientAccountHolder` | `""` | Account holder |
| `clientAccountNumber` | `""` | Account number |
| `payeeName` | `""` | Payee name |
| `_checkRequested` | `on` | Check requested checkbox (unchecked) |
| `action` | `saveadd` or `save` | See below |

### Action Values

| Value | Redirect Target | Description |
|-------|----------------|-------------|
| `saveadd` | `/app/assistancerequests/{id}/assistanceitems/new` | Save and add another item |
| `save` | `/app/assistancerequests/{id}` | Save and return to request detail |

### Known Assistance Type IDs

These are conference-specific. Observed values for the Nativity conference:

| ID | Name | Default Value |
|----|------|--------------|
| `16542` | Second Harvest Food | $70 |
| `16522` | Gift Cards | $100 |

To discover other type IDs, query the list endpoint and inspect
`assistanceItems[].assistanceType.id` and `.name` in the response.

---

## 8. Helper / Lookup Endpoints

### GET /app/assistancerequests/statuscheck

Check if a status value triggers special behavior (e.g., showing denial reason fields).

| Parameter | Example | Description |
|-----------|---------|-------------|
| `status` | `Open`, `Completed` | Status value to check |

```bash
curl -b "$COOKIE_JAR" \
  -H 'X-Requested-With: XMLHttpRequest' \
  'https://www.servware.org/app/assistancerequests/statuscheck?status=Completed'
```

**Response:** `application/json`, small object (e.g., `{"deniedStatus": false}`)

### GET /app/assistancerequests/itemvalue

Look up the default monetary value for an assistance type and client.

| Parameter | Example | Description |
|-----------|---------|-------------|
| `selectid` | `16542` | Assistance type ID |
| `clientid` | `580815` | Client ID |

```bash
curl -b "$COOKIE_JAR" \
  -H 'X-Requested-With: XMLHttpRequest' \
  'https://www.servware.org/app/assistancerequests/itemvalue?selectid=16542&clientid=580815'
```

**Response:** `application/json`, item value info.

### GET /app/assistancerequests/{id}/files/list

Returns the file attachments for a request as an HTML fragment.

```bash
curl -b "$COOKIE_JAR" \
  -H 'X-Requested-With: XMLHttpRequest' \
  'https://www.servware.org/app/assistancerequests/3724739/files/list'
```

**Response:** `text/html;charset=UTF-8`

### GET /app/assistancerequests/{id}/approval/list

Returns the approval chain for a request as an HTML fragment.

```bash
curl -b "$COOKIE_JAR" \
  -H 'X-Requested-With: XMLHttpRequest' \
  'https://www.servware.org/app/assistancerequests/3724739/approval/list'
```

**Response:** `text/html;charset=UTF-8`

---

## 9. Calendar Endpoints

All three calendar endpoints accept the same parameters and return the same
event object schema.

### Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /app/calendar/homevisits` | Scheduled home visits |
| `GET /app/calendar/followups` | Follow-up events |
| `GET /app/calendar/conferenceevents` | Conference events |

### Parameters

| Parameter | Example | Description |
|-----------|---------|-------------|
| `start` | `1769932800` | Range start (Unix timestamp, seconds) |
| `end` | `1773558000` | Range end (Unix timestamp, seconds) |
| `_` | `1770499854499` | Cache-buster (Unix timestamp, milliseconds) |

```bash
# Home visits for February 2026
curl -b "$COOKIE_JAR" \
  -H 'X-Requested-With: XMLHttpRequest' \
  -H 'Accept: application/json, text/javascript, */*; q=0.01' \
  'https://www.servware.org/app/calendar/homevisits?start=1769932800&end=1773558000&_=1770499854499'
```

### Event Object Schema

```json
[
  {
    "title": "Home Visit: Doe, Jane (Volunteer Name)",
    "start": "2026-02-03T00:00:00.000-0800",
    "end": null,
    "url": "/app/assistancerequests/3724974",
    "eventType": null,
    "colorCode": null,
    "textColorCode": null,
    "allDay": false,
    "editable": false
  }
]
```

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Event display text |
| `start` | string | ISO 8601 datetime with timezone offset |
| `end` | string \| null | End time (null for single-point events) |
| `url` | string | Relative URL to the assistance request |
| `eventType` | string \| null | `"calevent"` for conference events, null for visits |
| `colorCode` | string \| null | Custom color hex code |
| `textColorCode` | string \| null | Custom text color hex code |
| `allDay` | boolean | Whether the event spans the full day |
| `editable` | boolean | Whether the event is editable |

---

## 10. Notes and Conventions

### Checkbox Convention

ServWare uses the **Spring MVC checkbox convention**. For every checkbox field:

- **Checked:** Two fields sent: `fieldName=true` AND `_fieldName=on`
- **Unchecked:** Only the hidden companion: `_fieldName=on`

The `_fieldName=on` hidden field tells the server "this checkbox was present in
the form." Without the `fieldName=true` value, the server treats the checkbox as
unchecked.

| State | Fields Sent |
|-------|-------------|
| Checked | `homeVisitRequired=true&_homeVisitRequired=on` |
| Unchecked | `_homeVisitRequired=on` |

### Date Formats

| Context | Format | Example |
|---------|--------|---------|
| Date fields | `MM/DD/YYYY` | `02/07/2026` |
| Timestamp fields | `MM/DD/YYYY HH:MM AM/PM` | `02/07/2026 01:30 PM` |
| Calendar events | ISO 8601 with offset | `2026-02-03T00:00:00.000-0800` |
| Calendar params | Unix timestamp (seconds) | `1769932800` |

In form POSTs, date slashes must be URL-encoded: `02%2F07%2F2026`.

### HTML in Text Fields

Notes fields (`requestNote`, `visitNotes`) contain HTML wrapped in `<p>` tags:

```
requestNote=%3Cp%3ENeeds+food+assistance%3C%2Fp%3E
```

Which decodes to: `<p>Needs food assistance</p>`

### XHR Detection

The server uses the `X-Requested-With: XMLHttpRequest` header to distinguish
AJAX requests from page navigations. JSON API endpoints require this header.

### Navigation Map

Known application routes discovered from the home page HTML:

| Route | Description |
|-------|-------------|
| `/app/home` | Dashboard |
| `/app/clients` | Neighbors list |
| `/app/assistancerequests` | Requests list (HTML page) |
| `/app/assistancerequests/list` | Requests list (JSON API) |
| `/app/assistancerequests/{id}` | Request detail |
| `/app/assistancerequests/visits` | Request visits |
| `/app/assistancerequests/pendingassistance` | Pending assistance |
| `/app/clients/followups` | Neighbor follow-ups |
| `/app/calendarevents` | Calendar events page |
| `/app/mileagehoursinservicelist` | Mileage/hours tracking |
| `/app/reports` | Conference activity report |
| `/app/usersettings` | User profile settings |
| `/security/login` | Login page |
| `/security/logout` | Logout (POST) |
| `/security/extendSession` | Extend session timeout |
| `/security/redirectLogin` | Redirect to login on timeout |
