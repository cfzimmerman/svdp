# ServWare automation

This repo automates frequent administrative work
needed by the St. Vincent de Paul society at
Nativity Catholic Church in Menlo Park, CA

# First-time setup

### Have a Servware username and password

These are given by the SVdP chapter.

### Clone this repo and cd into it

I did not publish this to crates.io because it’s not useful to a general audience.

### Install rust

https://rust-lang.org/tools/install/

The commands below use `cargo run`. You can also build the binary
and execute it directly, but I typically don't bother with
that or with release builds because there's just no need.

### Optional: make an env file

Servware uses username:password auth via https. This was not my decision :)
You can either provide your username and password for every command or make
it an env variable. `.env` is gitignored at the repo root.

The `-e` flag in commands tells this tool to load and use the env file.

```.env
SERVWARE_USER="..."
SERVWARE_PASS="..."
```

### Generate volunteers.csv

Request completions are assigned to a volunteer. You supply the
volunteer id as a cli arg when marking requests complete. This
command generates a list of volunteer ids to choose from.

Redo this step if a new volunteer is added and you need to update
your local list. But this is not part of the weekly workflow. It
could probably be version controlled, but that's kind of DOXXING
our SVdP members.

```sh
cargo run -- -e list-members
```

### CLI docs

When in doubt, the help flag will show your options:

```sh
cargo run -- --help
```

# Weekly workflow

### Generate requests.csv

Remember, the `-e` flag is only if you keep your credentials
in a .env file. Otherwise leave it out.

```sh
cargo run -- -e get-requests
```

### Hand edit the CSV

`get-requests` writes open requests into whatever CSV path you
provide or `requests.csv` by default.  

Delete any entries that should not be marked complete. Align the
dollar values with the gift cards you gave out. You might need to
break requests.csv into multiple CSVs if you had multiple delivery groups.

### Mark complete

This resolves the open request. Use a volunteer id from the
volunteers.csv you generated during first-time setup.

The csv parameter reads requests.csv by default. But you'll need to
specify something else if you're assigning different requests to
different volunteer ids.

```sh
cargo run -- -e mark-complete --volunteer-id "FROM VOLUNTEERS.CSV"
```

### Add assistance

This adds Second Harvest and Gift Card items to the requests in the CSV.

Again this defaults to requests.csv.

```sh
cargo run -- -e add-assistance
```

# Contributing

### DISCLAIMER

A lot of this repo is claude generated. It's a
quick and dirty tool that saves some time for volunteers.

But this is IN NO WAY representative of what I consider
production code. So please don't assume this is a reflection
of my standards of software craftsmanship.

### Background

Community members (neighbors) may submit requests for
food or other assistance to a county office, which
writes those requests into a web app called ServWare.

Twice a week, pairs of volunteers do the following:

1. Fetch a list of open requests from ServWare. Get a list
   of open requests older than ~3 weeks.
2. Call each person on that list. If they're available, we'll
   deliver to their family that day.
3. Pack bags of food.
4. Allocate grocery store gift cards proportional to family size.
5. Plan a shortest-path route through the deliveries.
6. Make the deliveries.
7. Set all the delivered requests to 'Complete' in ServWare.
8. Log the assistance values for food and gift cards in ServWare.

Everything else exists to serve (6), so it's in our interest to
optimize around that. (7) and (8) take a while, so they're great
targets for automation. (5) is worth automating, but Traveling
Salesman lol. And (1) just generates inputs for (7) and (8). So
this project aims to accomplish (1), (7), and (8) with
optional scope for (5).

### ServWare

My contact: `(first name)(last name)93@gmail.com`

ServWare is a great tool, and I'm grateful to the maintainers.
Afaik there's no official ServWare API, which is why I had
Claude make this one.
- If you work on ServWare and want to talk, reach out! I hope
  this doesn't violate TOS, and there's no commercial interest
  at stake here.
- If you're from Nativity in a distant future where I've moved
  away, hmu and take this over.
- If you're from another SVdP chapter and want similar tools, maybe
  reach out and let's see how similar our needs are. Very cool
  if others can use this too.
