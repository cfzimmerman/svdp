# SVdP ServWare — for volunteers

This connects Claude to ServWare so you do not have to type deliveries in one at a time, and so
you can get lists of families without building a spreadsheet by hand.

You need a ServWare username and password. Your conference president issues those; this cannot
create one for you.

There is one file for Mac and it works on every Mac, so there is nothing to check about your
computer first.

## Setting it up, once

**1. Install Claude Desktop.** Download it from [claude.ai/download](https://claude.ai/download)
and sign in.

**2. Unzip the file you were sent.** It is called something like
`svdp-servware-macos.zip`. Double-click it. You get a folder called **svdp-servware**
containing everything below, plus a `START-HERE.txt` that repeats these steps.

**3. Add the SVdP extension.** In Claude, open **Settings → Extensions**, and drag
**`svdp-servware.mcpb`** from that folder into the window.

**4. Type in your ServWare username and password.** Click **SVdP ServWare** in that same
Extensions list and fill in the two boxes. They are stored by your own computer. They never
appear in the conversation, and nobody else can see them.

**5. Turn it on.** A newly added extension starts switched **off**. Make sure the switch next to
it is on, or nothing will happen.

That is all. You will not need to do any of this again.

> **If it seems to do nothing,** open a new chat and type *what can you do?* — it will tell you
> what is missing and how to fix it.

## Using it

Open Claude and say what you want, in your own words. You do not need to learn any commands.

**After a delivery day:**

> I did deliveries today and need to record them.

It will show the families who are waiting, ask which ones you reached, check the gift card
amounts with you, ask who drove, show you the whole list, and only save it to ServWare after you
say yes. **Nothing is written until you say yes.** If something looks wrong, say so.

**When you need a list of families:**

> I need a list of families with children under 13 for Adopt-a-Family.

It saves spreadsheets to your Desktop and helps you work out the answer. You can open them in
Excel, or just keep talking and let it do the arithmetic.

There are four things it can save:

| File | One row per |
|---|---|
| `svdp-neighbors-…csv` | family — name, address, phone numbers |
| `svdp-requests-…csv` | request for help — who asked, and when |
| `svdp-assistance-…csv` | help given — what it was, and the date it was given |
| `svdp-household-members-…csv` | **person in the house, with their age** |

That last one is the useful new thing. Ages are not in any report ServWare can export, which is
why they used to be typed in by hand.

**If you are not sure what to ask for,** say *what can you do?* and it will explain.

## Things worth knowing

- **Ask it to double-check anything that matters.** It will show you its work.
- **Some families have no ages recorded.** ServWare lets a conference enter either a head count
  *or* the individual people, not both. Families entered as a head count have no ages in the
  system at all. You will be told which ones those are, so you can check them yourself — they
  will not be quietly left out.
- **The spreadsheets hold real family information.** Keep them on your own computer.
- **Nothing is sent anywhere except ServWare.**

## Optional: the guided skills

These make the conversation more guided. The extension works fine without them.

Inside the folder you unzipped there is a **skills** folder containing a zip file per skill.
**Leave those zipped** — Claude wants the zip file itself.

1. In Claude: Settings → **Capabilities** → **Skills**.
2. Add a skill and choose one of the zip files in the `skills` folder.
3. Do it again for the other one. Add them one at a time.
4. Make sure each one is switched on.

## Desktop shortcuts, if you want them

Someone technical can put icons on your desktop that open Claude with the request already
written — you double-click and press Enter. Ask whoever sent you the extension file.

## If something goes wrong

Say what happened in the chat and ask it to explain. It is built to say plainly what worked, what
did not, and what is safe to try again. Recording deliveries can always be picked up where it
left off — it checks what is already in ServWare first, so nothing gets recorded twice.

If it is still stuck, contact whoever sent you the extension.
