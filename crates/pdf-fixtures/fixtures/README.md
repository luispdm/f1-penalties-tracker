# Fixtures

Synthetic, non-FIA PDFs for the `extract` and `ingest` tests. Committed, not
generated at test time. Regenerate after changing a spec:

```sh
cargo run -p pdf-fixtures --bin regen
```

Every driver, team, and lap time here is invented. The component codes are
regulation terminology.

## `table_grid.pdf`

A multi-column, multi-baseline table. Its spec is `table_grid_spec()` in
`../src/lib.rs`; the numbers below mirror it, so the 0.24 split is verifiable by
inspection. Coordinates are page-space points (origin bottom-left, `y` up).

Helvetica 11 pt. Column left edges: No at x 70, Team at x 130, Time at x 330.

| Row | Baseline y | No | Team | Time |
|-----|-----------|----|------|------|
| header | 720.00 | `No` | `Team` | `Time` |
| 1 | 690.00 / 689.76 | `1` | `Falcon Racing` | `1:31.201` |
| 2 | 665.00 | `2` | `Comet GP` | `1:31.888` |
| 3 | 640.00 | `3` | `Vertex Motors` | `1:32.044` |

Row 1 is the trap. Its number sits on baseline 690.00 while its team and time
sit on 689.76, a 0.24-point split. A reader that assumes one baseline per row
would split this into two rows, merging the number away from its team or
dropping a row. The clustering's `row_gap` spans the split, so the row stays
whole.

## Expected grid

The `extract` clustering must produce, top to bottom and left to right:

```
["No", "Team", "Time"]
["1", "Falcon Racing", "1:31.201"]
["2", "Comet GP", "1:31.888"]
["3", "Vertex Motors", "1:32.044"]
```

Asserted by `crates/extract/tests/fixture.rs`.

## `pu_snapshot.pdf`

A page shaped like the 2026 `PU elements used per driver up to now` documents,
at their measured coordinates. Its spec is `pu_snapshot_spec()` in
`../src/lib.rs`. Helvetica 9 pt.

It carries four traps for the column mapping.

### The whole page has no columns

Prose sits above the legend and spans the text width, so clustering the page
collapses it to a single column. The parsers take a grid over one band of the
page, and the caller slices it: the legend band at y 440–500, the table band at
y 360–425.

### The header wraps over three baselines

Long codes split down the page. Only the short ones fit the middle line, so it
reads `ICE TC EXH ES` and its fourth label sits over the `MGU-K` column.

| Baseline | Cells |
|---|---|
| 421.9 | `MGU`, `PU-`, `PU-` |
| 416.1 | `N`, `Car`, `Driver`, `ICE`, `TC`, `EXH`, `ES` |
| 410.4 | `-K`, `CE`, `ANC` |

Reading the labels along the middle line gives `ES` the `MGU-K` column and
shifts every later code by one. Reading down each column spells `MGU` then `-K`
and gets it right. `crates/ingest/tests/pu_snapshot.rs` asserts both: that the
mapping is correct, and that the naive reading disagrees with it.

### The two-part codes avoid U+002D

The real documents separate `PU-CE` and `PU-ANC` with **U+0002**, not a hyphen.
Readers disagree on it: pdfium reports it raw, poppler and pdf_oxide rewrite it.
Either way a parser matching a hardcoded `"PU-CE"` reads the wrong column, which
is why the codes come from the legend instead.

**This fixture substitutes U+00AD SOFT HYPHEN**, because no round trip can carry
U+0002 back out. Two blockers sit in series.

The writer's blocker can be lifted. `render_table` draws with the built-in
Helvetica, whose WinAnsi encoding turns every unmappable character into `?`, so
writing `PU\u{2}CE` reads back as `PU?CE`. U+2011 and U+2212 come back as `?`
for the same reason; U+00AD and U+2013 survive because WinAnsi maps them. An
embedded font lifts that limit, and printpdf 0.12.5 writes U+0002 into the
ToUnicode CMap; the pinned 0.11.3 drops that entry while building it.

The reader's cannot. pdf_oxide 0.3.74 returns U+FFFD for U+0002 however the file
spells it, so no test reading this fixture could assert the character even from
a perfect writer. Embedding a font to carry U+0002 buys nothing.

U+00AD stands in faithfully: invisible when rendered, not U+002D, and fatal to a
hardcoded literal. The real U+0002 is asserted in the `ingest` unit tests, which
build a grid directly and need no font.

### The data rows are padded with spaces

A real snapshot draws a data row as one run of text whose cells are spaced
apart, so a space glyph sits in every gap between columns. Those spaces have
real boxes, and clustering their midpoints steps from one column to the next
until the ten columns come out as one. Column left edges:

| Column | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |
|---|---|---|---|---|---|---|---|---|---|---|
| x | 47.9 | 74.0 | 211.0 | 303.5 | 333.5 | 360.7 | 393.7 | 430.6 | 459.2 | 489.6 |

The padding must not move them, or the fixture would stop matching the
documents it is measured from. So it is drawn on each cell's **left**, with the
cell's origin moved back by exactly the padding's advance: the built-in
Helvetica's space is 278/1000 em, an exact 2.502 pt at 9 pt, so a whole number of
spaces lands the text back on the point it started from.
`CellSpec::padded` does this, and the counts sit beside the column
coordinates in `pu_snapshot_spec()`. Each run is the longest that still opens
clear of the column to its left, between 0.3 and 2.4 points past its ink.

Measured on the fixture: the widest gap between adjacent midpoints in the table
band is 6.79 pt, well under the 12 pt `column_gap`, so clustering midpoints with
the spaces left in yields **one** column against a true ten.
`crates/ingest/tests/pu_snapshot.rs` asserts both, so the fixture cannot quietly
stop carrying the trap.

The three header lines are not padded. Their fragments print at separately
measured positions, which is the wrap trap above; padding across them would be
inventing geometry that no document was measured for. The data rows carry the
trap on their own.

