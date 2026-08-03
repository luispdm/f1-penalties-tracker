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
collapses it to a single column. The parsers take a grid over one band, and
`ingest::bands` derives the bands from the page: the legend lands at y 447.6 to
489.0, the table at y 364.4 to 421.9.

The spacing is what bounds the legend band above. Its four lines sit 13.8 points
apart and the prose sits 41.4 above them, so a blank line separates the two, as
it does on the documents at 27.6 against 13.8.

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
A cell asks for its padding through `CellSpec::padded` and `render_table`
places it, since `render_table` is what picks the font and the size the advance
depends on. The counts sit beside the column coordinates in
`pu_snapshot_spec()`.

Each run is the longest that still opens clear of the column to its left,
between 0.3 and 2.4 points past its ink. That margin is the price of the trap:
a space advances 2.502 pt against a 12 pt `column_gap`, so a shorter run would
break the chain. **Lengthening a team or driver name eats into it.** Go far
enough and a run opens left of that name's last glyph; cells are read in `x0`
order, so the padding would interleave mid-name.

Measured on the fixture: the widest gap between adjacent midpoints in the table
band is 6.79 pt, well under the 12 pt `column_gap`, so clustering midpoints with
the spaces left in yields **one** column against a true ten.
`crates/ingest/tests/pu_snapshot.rs` asserts the left edges, the collapse, and
the margin on all 36 runs, so the fixture cannot quietly stop carrying the trap.

The three header lines are not padded. Their fragments print at separately
measured positions, which is the wrap trap above; padding across them would be
inventing geometry that no document was measured for. The data rows carry the
trap on their own.

## `two_band_snapshot.pdf`

The legend and the table of `pu_snapshot.pdf`, at the same coordinates, with the
prose dropped and the legend's right hand block moved 20 points right, to x
317.7. Its spec is `two_band_snapshot_spec()`.

`pu_snapshot.pdf` proves that a caller must **select** the bands, since its prose
spans the text width and collapses the page to one column. This page proves the
other half: that a caller must **cluster each band on its own**.

One legend entry is why. `PU-CE`'s description runs from x 317.7 to x 497, past
the left edge of every component column beneath it, and the last column's ink
opens at 498, a point away. Single linkage therefore walks it across all seven
component boundaries, and the legend's left hand block does the same to the three
identity columns. Measured on the fixture:

| glyphs fed to `cluster` | columns out |
|---|---|
| both bands at once | 2 |
| the table band alone | 10 |

The real 2026 documents measure the same, where that entry runs from x 347 to x
533 over columns starting at 309.

`crates/ingest/tests/two_band_snapshot.rs` asserts both counts, so the fixture
cannot quietly stop carrying the trap, and reads the legend and the column
labels off the two bands.

