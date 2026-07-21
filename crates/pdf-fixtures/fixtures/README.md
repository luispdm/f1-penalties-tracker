# Extraction fixtures

Synthetic, non-FIA PDFs for the `extract` tests. Committed, not generated at
test time. Regenerate after changing a spec:

```sh
cargo run -p pdf-fixtures --bin regen
```

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
