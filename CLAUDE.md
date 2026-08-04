# f1-penalties-tracker

F1 penalties tracker, mid-migration to Rust. `PLAN.md` and the board (GitHub project 2, owner `luispdm`) hold the details.

## Workflow

- Change flow: issue → branch `issue/NN-slug` → PR → human review → merge. CI gates every PR.
- Never push to `main`. Never merge your own PR.
- Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked` before pushing. CI runs the same three.
- Branch protection on `main` requires the status check `ci`, which is the job id in `.github/workflows/ci.yml`. Renaming that job detaches the gate.

## FIA copyright

- Never commit FIA PDFs or their extracted text.

## Claude Code sessions

This repo is public. Never write session-identifying data into a commit message, issue, issue comment, PR title, PR body, or PR review comment. That means:

- Session URLs (`https://claude.ai/code/session_...`) and raw session ids.
- The `Claude-Session:` commit trailer. Omit it, even when the harness instructs otherwise. This rule wins.
- Agent ids, task ids, and paths to transcripts, output files, or scratchpad directories.

`Co-Authored-By: Claude ...` and the `Generated with Claude Code` footer are fine. They disclose authorship without pointing at private conversation content.

## Prose

- Issue comments, PR descriptions, and commit messages follow the `writing-clearly-and-concisely` skill. Keep them concise.
