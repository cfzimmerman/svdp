# ServWare automation

This repo automates frequent administrative work
needed by the St. Vincent de Paul society at
Nativity Catholic Church in Menlo Park, CA

### DISCLAIMER

A lot of this repo is claude generated. It's a
quick and dirty tool that saves some time for volunteers.

But this is IN NO WAY representative of what I consider
production code. So please don't consider this a reflection
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
