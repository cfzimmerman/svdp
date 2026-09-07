# Nativity Saint Vincent de Paul

The Saint Vincent de Paul society at Nativity Catholic church in Menlo Park
does wonderful work for the community. There's routine administrative work
behind volunteer operations, and this project attempts to automate
some of that.

If you are a volunteer, you can find the most recent bundle [here](https://drive.google.com/drive/folders/1kM6oCAba7GTORgtKHy1NHGGalyh2h1o7?usp=sharing).
Here's a video showing how to get set up: [https://youtu.be/Nijpbwm7PJc](https://youtu.be/Nijpbwm7PJc).
If you run into any issues, please send me an email at `coryzimmerman93@gmail.com`.


### Maintainer notes

This project exists to solve a problem. Since that problem couldn't
easily be solved in a weekend of hand coding, basically this entire
project is vibe coded with Claude. While that's not ideal, the limited
scope and fairly obvious correctness requirements provide guardrails.
And realistically doing this bookeeping work by hand in ServWare is
error prone too.

The ServWare API was distilled from browser network traces, which Claude
is good at parsing. And this approach enables fairly swift
iteration based on volunteer needs. Since I've already given up on code
quality, basically the only iteration constraint is correctness in
the simplest form possible.

The previous version of this project was a CLI tool, but that's not very
useful to the average SVdP volunteer. I didn't want to host this as a public
website, and one GUI to replace another GUI felt brittle. So instead this
is structured as a Claude Desktop plugin. Volunteers just ask Claude to
do the work they might ask another volunteer to do. While that implies
a Claude subscription, I think most volunteers find $20/month a fair trade
for hours of administrative work.

