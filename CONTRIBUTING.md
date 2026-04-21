# Contributing to mdcat

Thanks for your interest in mdcat. This document covers what you need
to know to send a patch.

## Quick start

```sh
git clone https://github.com/pawelb0/mdcat
cd mdcat
cargo build --all-targets
cargo test --all-targets
```

That's it. mdcat needs `libcurl` (included on macOS, `libcurl4-dev` on
Debian/Ubuntu, `curl-devel` on Fedora).

The working branch for 3.x is `3.0-cleanup`; `main` still holds the
2.7.1 release and will not compile on current rustc.

## Project layout

| Path | Purpose |
|---|---|
| `src/main.rs` | Entry point for the `mdcat` binary |
| `src/bin/mdless.rs` | Entry point for the `mdless` binary |
| `src/cli.rs` | Shared command-line dispatch (clap multicall) |
| `src/lib.rs` | Public library API — `push_tty`, `process_file` |
| `src/render.rs` + `src/render/` | Render state machine and per-element handlers |
| `src/terminal/` | Terminal detection, DA1 probing, multiplexer passthrough, image protocols |
| `src/resources/` | File and HTTP resource handlers, SVG rasterisation, parallel prefetch |
| `src/mdless/` | Interactive pager — buffer, view, keys, search, TOC |
| `tests/` | Integration tests plus ~2000 insta snapshots |
| `sample/` | Smoke-test documents; `sample/stress.md` is the kitchen sink |

## What to run before you commit

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --no-default-features --features "svg image-processing" -- -D warnings
cargo test --all-targets
```

CI runs the same checks on every push. Everything must be clean.

## Snapshot tests

mdcat uses [`insta`](https://insta.rs) for rendering regression tests.
When you change rendering output:

1. Run `cargo test --all-targets`; any changed output writes to
   a `.snap.new` file next to the existing snapshot.
2. Audit a few of the diffs manually to confirm the change is
   intentional.
3. Accept the new baseline in bulk:

   ```sh
   find src/snapshots tests/snapshots -name '*.snap.new' \
       -exec sh -c 'mv "$1" "${1%.new}"' _ {} \;
   ```

4. Re-run `cargo test` until everything is green.

Commit the rebaseline alongside the code change, not separately.

## Commit style

- Summary ≤72 characters, blank line, paragraph.
- Explain the *why* and the approach, not just the *what*. A good
  commit message lets someone six months later understand the
  decision.
- No force-push to published branches without coordination.

## Coding conventions

Short version:

- Idiomatic Rust. Match standard-library conventions.
- Public items have a `///` doc comment; first sentence ≤15 words.
- Prefer concrete types and borrowed references. Reach for `Box` /
  `Arc` only when ownership genuinely needs to be shared.
- Errors: library code returns `RenderError`, CLI code uses
  `anyhow`. Panics only on logic bugs, never on bad user input.
- Lints: `clippy::pedantic = "warn"` with a curated allow-list in
  `Cargo.toml`. If you need a new `#[allow]`, add a comment
  explaining why the lint is misapplied.
- Tests cover observable behaviour, not internal field shapes.
- Trim more than you add. A patch that reduces line count at equal
  clarity is preferred; a patch that introduces abstraction without
  removing concrete code usually shouldn't land.

Longer version lives in the Microsoft Pragmatic Rust Guidelines
(<https://microsoft.github.io/rust-guidelines/>), which mdcat
follows where it makes sense.

## Writing conventions (docs + comments)

Applies to `///` and `//!` docs, README/manpage prose, and commit
messages. The goal is prose that reads like a careful engineer
wrote it.

Avoid:

- Em-dashes as catch-all punctuation. One per paragraph at most.
- Filler openers: "Notably", "Essentially", "Fundamentally",
  "Ultimately", "Furthermore", "Additionally", "Moreover".
- "It is worth noting that", "note that", "should be mentioned".
- Second-person address: prefer "The cursor advances" over
  "You'll see the cursor advance".
- Conclusion markers: "In conclusion", "Overall", "To summarise".
- Metaphor crutches: "north star", "silver bullet", "game-changer",
  "double-edged sword".

Prefer:

- Start sentences with the subject.
- State the claim directly. Reserve hedging for genuine uncertainty.
- Vary sentence length so a paragraph has rhythm instead of a
  staccato burst.

Code blocks, commit SHAs, error messages quoted verbatim are
exempt from these rules.

## Adding a new terminal

1. Add a variant to `TerminalProgram` in `src/terminal/detect.rs`.
2. Extend `detect_term` / `detect_term_program` /
   `detect_secondary_env` with the detection rule.
3. Map the variant to a `TerminalCapabilities` in `capabilities()`.
4. Add a unit test that sets the relevant environment variables
   via `temp_env::with_vars` and asserts the detection result.

## Getting help

File an issue for bugs or feature discussions. For security
disclosures see `SECURITY.md` (coming soon) or email the current
maintainer.

## References

- Upstream (unmaintained): <https://github.com/swsnr/mdcat>
- CommonMark spec: <https://spec.commonmark.org/>
- pulldown-cmark: <https://docs.rs/pulldown-cmark/>
- Rust API Guidelines: <https://rust-lang.github.io/api-guidelines/>
- Microsoft Rust Guidelines: <https://microsoft.github.io/rust-guidelines/>
