# SVdP ServWare — a guide for volunteers

This connects Claude to ServWare so you do not have to type deliveries in one at a time, and so
you can get lists of families without building a spreadsheet by hand.

You need a ServWare username and password. Your conference president issues those — this cannot
create one for you.

There is one download for Mac and it works on every Mac, so there is nothing to check about your
computer first.

---

## Setting it up, once

**1. Install Claude Desktop.** Get it from [claude.ai/download](https://claude.ai/download) and
sign in.

**2. Unzip the file you were sent.** It is called `svdp-servware-macos.zip` or similar.
Double-click it. You get a folder called **svdp-servware** with everything in it, including a
`START-HERE.txt` that repeats these steps.

**3. Add the extension.** In Claude, open **Settings → Extensions** and drag
**`svdp-servware.mcpb`** from that folder into the window.

**4. Type in your ServWare username and password.** Still in Settings → Extensions, click **SVdP
ServWare** and fill in the two boxes. They are stored by your own computer. They never appear in
the conversation and nobody else can see them.

**5. Turn it on.** A newly added extension starts switched **off**. Make sure the switch next to
it is on, or nothing will happen.

**6. Add the skills** (optional, but recommended). These make the conversation more guided. In
Claude, open **Settings → Capabilities → Skills**, add a skill, and choose one of the zip files in
the `skills` folder. Then do it again for the other one — one at a time. **Leave those files
zipped**; Claude wants the zip, not a folder.

That is everything, and you never do it again.

> **If it seems to do nothing,** open a new chat and type *what can you do?* It will tell you what
> it can do and what is still missing.

---

## Recording a delivery day

Open Claude and say it however you like:

> I did deliveries today and need to record them.

You will be shown **two lists**: everyone with a request in, and then the shorter list of families
who are actually due. Families get one delivery a month, so anyone who already had one in the last
four weeks is in the first list but not the second. Both are shown so you can see if something
looks wrong.

From there it will ask which families you reached, check the gift card amounts with you, ask who
drove, and show you the whole plan. **Nothing is written to ServWare until you say yes.**

Two things worth knowing:

- **Your numbers win.** The gift card amount comes from household size, but if you handed over
  something different, say so and it will use yours. You were there.
- **If you delivered to someone who was not on the due list, say so.** It will be recorded. The
  monthly rule is a reminder, not a gate.

**If something looks wrong, say so.** ServWare has no way to delete an assistance entry once it
is added, so the confirmation step is the only safety net — read it properly, and never say yes to
a list you have not checked.

If it stops halfway, nothing is lost. Ask it to carry on; it checks what is already in ServWare
first, so nothing gets recorded twice.

---

## Getting a list of families for a project

For occasional projects — Christmas Adopt-a-Family, a mailing, a summary for the conference — this
can pull information **out** of ServWare into spreadsheets on your Desktop. Open them in Excel, or
just keep talking and let Claude work through them with you.

Say what you need:

> I need a list of families with children under 13 for Adopt-a-Family.

It will ask what counts as recent, save the spreadsheets, and help you get to the answer. Four
files are available:

| File | One row per | What it holds |
|---|---|---|
| `svdp-neighbors-…csv` | family | everyone the conference serves: name, address, phones |
| `svdp-requests-…csv` | request for help | who asked, and when |
| `svdp-assistance-…csv` | item of help given | what it was, and the date it was given |
| `svdp-household-members-…csv` | person in the house | **names and ages of everyone in the family** |

That last one is the useful new thing. **Ages are not in any report ServWare can export** — until
now the only way to get them was to open each family's record and type them in by hand.

Four things worth knowing:

- **Looking up ages takes a minute or two.** It opens one page per family, so it will ask you to
  approve it, and it will ask for a date range first. A date range keeps it quick.
- **Some families have no ages recorded at all.** ServWare lets a conference enter either a head
  count *or* the individual people, not both — so families entered as a head count have no ages in
  the system. You will be told which ones those are, as a separate list, rather than having them
  quietly left out. Those are the ones you would still need to check by hand.
- **"When they asked" and "when they got help" are different dates.** A family can ask in June and
  get their gift card in July. If you want to know how much a family has *received* in a period,
  that is the assistance file.
- **Ask it to double-check anything that matters.** It will show you its work. For anything
  involving counting or adding, ask it to work it out with code rather than by eye.

**These spreadsheets hold real family information. Keep them on your own computer.**

---

## Shortcuts, if you want them

Someone technical can put icons on your desktop that open Claude with the request already written,
so you double-click and press Enter. Ask whoever sent you the extension.

---

## If something goes wrong

Say what happened in the chat and ask it to explain. It is built to say plainly what worked, what
did not, and what is safe to try again — and it will never show you a raw error message.

If it is still stuck, contact whoever sent you the extension file.
