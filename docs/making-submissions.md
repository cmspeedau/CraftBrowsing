# Making Submissions Easy to Read

The person reading our work is usually busy and often doing it between other
tasks. Optimize for their time. A submission that takes effort to parse gets
skimmed, delayed, or dropped, no matter how correct it is. This applies to commit
messages, pull requests, issue reports (including upstream to projects like
Servo), and docs.

The test for every submission: can a tired reviewer get the point and act on it
in about 20 seconds?

## Commit messages

- First line: the what and the why, under ~72 chars, scoped. Example:
  `fix(engine): composite the page into the content region, not behind the chrome`.
- Body: the reason and anything a reader needs to judge it. Not a restatement of
  the diff. If the change is obvious, a one-line message is enough.
- Say why it was needed, what breaks without it, and how it was verified. A claim
  like "tested on YouTube" beats a paragraph describing the fix.

## Issue and PR reports

- Lead with the single point. First line or first sentence states the result, not
  the buildup.
- Put reproduction next, not last: smallest input that shows it, exact command,
  expected versus observed.
- Show evidence, not adjectives. A captured error line, a version from the lock
  file, or a diff beats "it seems broken sometimes".
- Keep each item to one concern. Split unrelated problems into separate reports.
- Link, do not restate. Point at the file and line or the existing issue rather
  than re-describing them.

## Documentation

- Headers are the map. Someone scanning should find their topic by heading alone.
- Every factual claim traces to a source we can name (a log line, a file:line, a
  release, a captured run). No invented numbers, versions, dates, or names.
- Separate what we verified from what we inferred. Label guesses.
- Prefer the plain sentence over the impressive one. Cut words that only signal
  importance ("critical", "seamless", "cutting-edge") and say the specific thing.

## Smallest reproducible test case

When reporting an engine or protocol problem, attach the smallest thing that still
shows it, ideally one file or one command. For a web-API gap, a page that touches
only that API. For a transport bug, one URL and the captured error line. A
reviewer should be able to reproduce it without reverse-engineering the report.

## Before sending

- Remove anything that does not help the reader act.
- Read the first line alone; it should stand on its own.
- Check that claims are sourced and guesses are marked.
