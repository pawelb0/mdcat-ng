# Contributing

## Quick start

```sh
git clone https://github.com/pawelb0/mdcat-ng
cd mdcat-ng
cargo build --all-targets
cargo test --all-targets
```

`libcurl` is required: macOS bundles it, Debian/Ubuntu need
`libcurl4-dev`, Fedora `curl-devel`.

## Layout

| Path | Purpose |
| --- | --- |
| `src/main.rs` | `mdcat` binary entry |
| `src/bin/mdless.rs` | `mdless` binary entry |
| `src/cli.rs` | Shared CLI dispatch |
| `src/lib.rs` | Library API |
| `src/render/` | Render state machine |
| `src/terminal/` | Detection, DA1, multiplexer, image protocols |
| `src/resources/` | File / HTTP handlers, SVG, prefetch |
| `src/mdless/` | Interactive viewer |
| `tests/` | Integration + insta snapshots |
| `sample/` | Smoke-test documents |

## Before you commit

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --no-default-features --features "svg image-processing" -- -D warnings
cargo test --all-targets
```

## Snapshot tests

Changed output writes to `.snap.new` next to the baseline. After
reviewing the diffs:

```sh
find src/snapshots tests/snapshots -name '*.snap.new' \
    -exec sh -c 'mv "$1" "${1%.new}"' _ {} \;
```

Commit the rebaseline alongside the code change.

## Coding conventions

- Idiomatic Rust.
- `///` on public items; first sentence ≤15 words.
- Prefer concrete types and borrowed references.
- Library errors: `RenderError`. CLI errors: `anyhow`.
- No panics on bad user input.
- New `#[allow]` needs a one-line reason.
- Trim more than you add.

## Adding a terminal

1. New variant in `TerminalProgram` (`src/terminal/detect.rs`).
2. Detection rule in `detect_term*`.
3. Capabilities in `capabilities()`.
4. Unit test with `temp_env::with_vars`.

## References

- Upstream: <https://github.com/swsnr/mdcat>
- CommonMark: <https://spec.commonmark.org/>
- pulldown-cmark: <https://docs.rs/pulldown-cmark/>
