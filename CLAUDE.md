# f1-penalties-tracker

F1 penalties tracker, mid-migration to Rust. `PLAN.md` and the board (GitHub project 2, owner `luispdm`) hold the details.

## Workflow

- Change flow: issue → branch `issue/NN-slug` → PR → human review → merge. CI gates every PR.
- Never push to `main`. Never merge your own PR.
- Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` before pushing. CI runs the same three.
- Branch protection on `main` requires the status check `ci`, which is the job id in `.github/workflows/ci.yml`. Renaming that job detaches the gate.

## FIA copyright

- Never commit FIA PDFs or their extracted text.

## Prose

- Issue comments, PR descriptions, and commit messages follow the `writing-clearly-and-concisely` skill. Keep them concise.
