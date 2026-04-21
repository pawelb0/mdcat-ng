<div align="center">

# mdcat

**Render Markdown in the terminal. Inline images, clickable links,
syntax-highlighted code, an interactive pager.**

[![Crates.io](https://img.shields.io/crates/v/mdcat.svg)](https://crates.io/crates/mdcat)
[![License: MPL-2.0](https://img.shields.io/badge/license-MPL--2.0-blue.svg)](./LICENSE)
[![Rust 1.83+](https://img.shields.io/badge/rust-1.83+-orange.svg)](https://www.rust-lang.org)

</div>

> [!NOTE]
> 3.x is a continuation fork of [swsnr/mdcat], which was marked
> unmaintained at v2.7.1 in December 2024. 3.0 adds the interactive
> `mdless` pager, broader terminal coverage, Sixel image output, and
> reorganises the library API.
>
> [swsnr/mdcat]: https://github.com/swsnr/mdcat

<p align="center">
  <img src="./tapes/demo.gif" alt="mdcat + mdless demo">
</p>

---

## Features

- **Images inline**, natively, on iTerm2, Kitty, WezTerm, Ghostty, Rio,
  VS Code, Terminology, and any terminal that speaks the [Sixel]
  protocol (Foot, Contour, mlterm, Windows Terminal, xterm).
- **tmux and GNU screen passthrough.** Image escapes wrap in DCS so
  multiplexed sessions forward them to the underlying terminal.
- **Sixel detection via DA1 probe** when environment variables leave
  mdcat on a plain ANSI profile.
- **OSC 8 hyperlinks** for every link and reference. Clickable in
  the terminal.
- **Syntect syntax highlighting** for fenced code blocks in hundreds
  of languages. Solarized Dark default.
- **GFM alert blockquotes** (`> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`,
  `> [!WARNING]`, `> [!CAUTION]`) with colour-coded labels.
- **Smart punctuation** at parse time (`"quoted"`, en/em dash, ellipsis).
- **Interactive `mdless` pager** with search, heading jumps, TOC modal,
  and vi-style bookmarks.
- **Pipe-safe output.** Styling and image escapes drop when stdout is
  not a TTY, matching `grep`/`ls`/`cat` behaviour.
- **No silent network calls.** Remote images render as hyperlinks
  unless `--remote-images` is set.

[Sixel]: https://en.wikipedia.org/wiki/Sixel

## Performance

Four choices keep the render loop tight:

- Local-only by default. No HTTP fetches unless `--remote-images`
  is set. Most `mdcat doc.md` runs never hit the network.
- Parallel remote prefetch. When `--remote-images` is enabled,
  image URLs fetch concurrently; a doc with many badges pays one
  round-trip instead of one per image.
- Short DA1 capability probe, overridable via `--probe-timeout-ms`.
- Lazy libcurl init. Only runs when remote fetching is configured.

## Quick start

```sh
# Render a file
mdcat README.md

# Render from stdin
curl -sL https://example.com/doc.md | mdcat -

# Open the interactive pager
mdless README.md

# Open at first match of a pattern
mdless --search "## Installation" README.md
```

## Install

| Platform                  | Command / source                                                     |
|---------------------------|----------------------------------------------------------------------|
| Cargo                     | `cargo install mdcat`                                                |
| Release binaries          | [GitHub Releases] (provenance attestations attached)                 |
| Distribution packages     | [Repology tracker]                                                   |
| From source (this fork)   | `git clone … && cargo install --path .`                              |

A `cargo install` produces two binaries: `mdcat` (renderer) and
`mdless` (interactive pager). `libcurl` must be available at build
time.

[GitHub Releases]: https://github.com/swsnr/mdcat/releases
[Repology tracker]: https://repology.org/project/mdcat/versions

## `mdcat` — render to the terminal

```sh
mdcat FILE.md                 # render to stdout
mdcat file1.md file2.md       # concatenate renders
mdcat -                       # read from stdin
mdcat --paginate FILE         # pipe through $PAGER / less -r
mdcat --columns 80 FILE       # pin the wrap width
mdcat --ansi FILE             # force styling when stdout is not a TTY
mdcat --detect-terminal       # print detected terminal + multiplexer + probed caps
mdcat --no-probe-terminal …   # skip the DA1 Sixel probe
```

Full flag reference: `mdcat --help` or [`mdcat(1)`](./mdcat.1.adoc).

### Non-TTY output

When stdout is piped, redirected, or captured by tooling, `mdcat`
drops all ANSI styling and image protocols and emits plain text. Keep
colours explicitly when you want them:

- `mdcat --paginate` (built-in `less -r` shellout)
- `mdless` (built-in interactive pager)
- `LESS=-R` in the environment when piping to `less` manually
- `--ansi` to force styled output unconditionally

### Inside tmux or GNU screen

`$TMUX` or `$STY` triggers DCS passthrough so Kitty graphics, iTerm2
inline images, and Sixel reach the underlying terminal. For tmux,
enable passthrough in the server config:

```tmux
set -g allow-passthrough on
```

GNU screen needs no additional configuration.

## `mdless` — interactive markdown pager

Running the binary as `mdless` launches a built-in markdown-aware
pager. The document renders once to an in-memory buffer; scrolling,
searching, and highlighting operate on that buffer without
re-rendering.

### Keybindings

| Key              | Action                                                    |
|------------------|-----------------------------------------------------------|
| `j` / `k`        | Scroll one rendered line down / up                        |
| `Space` / `b`    | Page forward / back                                       |
| `Ctrl+D` / `U`   | Half page forward / back                                  |
| `g` / `G`        | Jump to top / bottom                                      |
| `NG`             | Jump to rendered line `N` (numeric prefix)                |
| `/PATTERN`       | Forward search (smart-case, literal by default)           |
| `?PATTERN`       | Backward search                                           |
| `n` / `N`        | Cycle to next / previous match                            |
| `Esc`            | Clear search highlights                                   |
| `]]` / `[[`      | Jump to next / previous heading                           |
| `T`              | Open the TOC modal (`Enter` jumps, `Esc` closes)          |
| `m{a-z}`         | Save the current viewport top as bookmark `a`..`z`        |
| `'{a-z}`         | Jump back to a saved bookmark                             |
| `Ctrl+L`         | Force redraw                                              |
| `q` / `Ctrl+C`   | Quit                                                      |

### Flags

| Flag                  | Purpose                                                     |
|-----------------------|-------------------------------------------------------------|
| `--search PATTERN`    | Commit a query before the event loop starts                 |
| `--regex`             | Interpret the pattern as a regex (literal by default)       |
| `--case-sensitive`    | Disable smart-case; force case-sensitive matching           |
| `--external-pager`    | Fall back to `$PAGER` / `less -r` (pre-3.0 behaviour)       |
| `--no-pager` / `-P`   | Skip the pager entirely and print to stdout                 |

Matches highlight in-place against the SGR-styled buffer. Bold,
italic, colour, and OSC 8 link styles survive the highlight reset —
the pager re-emits whatever CSI-m state was active at the match
start.

## Terminal support matrix

| Terminal              | Styling | Hyperlinks | Images  | Notes                               |
|-----------------------|:-------:|:----------:|:-------:|-------------------------------------|
| [iTerm2]              | ✓       | ✓          | native  | Jump marks for headings (⇧⌘↑ / ⇧⌘↓) |
| [Kitty]               | ✓       | ✓          | native  | Kitty graphics protocol              |
| [WezTerm]             | ✓       | ✓          | native  | Kitty + iTerm2 protocols             |
| [Ghostty]             | ✓       | ✓          | native  | Kitty protocol                       |
| [Rio]                 | ✓       | ✓          | native  | Kitty protocol                       |
| [VS Code]             | ✓       | ✓          | native  | iTerm2 protocol                      |
| [Terminology]         | ✓       | ✓          | native  | tycat protocol                       |
| [Foot]                | ✓       | ✓          | Sixel   | Pure Sixel                           |
| [Windows Terminal]    | ✓       | ✓          | Sixel   | Sixel since 1.22                     |
| [Contour]             | ✓       | ✓          | Sixel   |                                      |
| [mlterm]              | ✓       |            | Sixel   |                                      |
| [xterm]               | ✓       |            | Sixel¹  | Requires `xterm -ti vt340`           |
| [Alacritty]           | ✓       | ✓          |         |                                      |
| [Konsole]             | ✓       | ✓          |         |                                      |
| [Warp]                | ✓       | ✓          |         |                                      |
| [Hyper]               | ✓       |            |         |                                      |
| [Apple Terminal]      | ✓       | macOS 15+  |         | OSC 8 support gated on macOS version |
| Basic ANSI            | ✓       |            |         | Strikethrough + OSC 8 required       |

1) Detected at runtime via a Primary Device Attributes probe unless
   `--no-probe-terminal` is set.

SVG images are rasterised with [resvg]; see its [support matrix][svg]
for which SVG features render faithfully.

[iTerm2]: https://www.iterm2.com
[Kitty]: https://sw.kovidgoyal.net/kitty/
[WezTerm]: https://wezfurlong.org/wezterm/
[Ghostty]: https://mitchellh.com/ghostty
[Rio]: https://raphamorim.io/rio/
[VS Code]: https://code.visualstudio.com
[Terminology]: http://terminolo.gy
[Foot]: https://codeberg.org/dnkl/foot
[Windows Terminal]: https://aka.ms/terminal
[Contour]: https://contour-terminal.org
[mlterm]: http://mlterm.sourceforge.net
[xterm]: https://invisible-island.net/xterm/
[Alacritty]: https://alacritty.org
[Konsole]: https://konsole.kde.org
[Warp]: https://www.warp.dev
[Hyper]: https://hyper.is
[Apple Terminal]: https://support.apple.com/guide/terminal/welcome/mac
[resvg]: https://github.com/RazrFalcon/resvg
[svg]: https://github.com/RazrFalcon/resvg#svg-support

## Markdown support

`mdcat` parses CommonMark plus these extensions:

- Pipe tables
- Task lists (`- [x]`)
- Strikethrough (`~~text~~`)
- GFM alert blockquotes (`> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`,
  `> [!WARNING]`, `> [!CAUTION]`)
- Smart punctuation (curly quotes, en/em dash, ellipsis)
- Footnotes (`[^1]` refs + definitions)
- Definition lists (`term\n: definition`)
- Wiki links (`[[Page]]`, `[[Page|label]]`)

Not yet implemented:

- Math (`$inline$`, `$$block$$`).
- Styled inline markup inside table cells (plain text only).
- Cell reflow and text wrap inside tables — cells truncate with `…`.

## Library usage

`mdcat` exposes its renderer as a Rust crate. The 3.0 line collapsed
the former `pulldown-cmark-mdcat` workspace member back into the main
crate; downstream consumers should migrate to `mdcat::*` paths.

```rust
use mdcat::{push_tty, Environment, Settings, TerminalProgram, Theme};
use pulldown_cmark::Parser;
use syntect::parsing::SyntaxSet;

let markdown = "# Hello\n\nRendered with **mdcat**.\n";
let settings = Settings {
    terminal_capabilities: TerminalProgram::Ansi.capabilities(),
    terminal_size: Default::default(),
    multiplexer: mdcat::Multiplexer::None,
    syntax_set: &SyntaxSet::load_defaults_newlines(),
    theme: Theme::default(),
    wrap_code: false,
};
let env = Environment::for_local_directory(&std::env::current_dir()?)?;
let handler = mdcat::create_resource_handler(mdcat::args::ResourceAccess::LocalOnly)?;

let mut out = Vec::new();
push_tty(&settings, &env, &handler, &mut out, Parser::new(markdown))?;
```

`push_tty_with_observer` accepts a `RenderObserver` invoked on every
`pulldown-cmark` event with the output writer's current byte offset.
The interactive pager uses it to collect heading positions.

## Packaging

When packaging `mdcat`, include:

- Both `mdcat` and `mdless` binaries (produced by a single
  `cargo install`).
- Shell completions via `--completions`:
  ```sh
  mdcat  --completions fish > /usr/share/fish/vendor_completions.d/mdcat.fish
  mdcat  --completions bash > /usr/share/bash-completion/completions/mdcat
  mdcat  --completions zsh  > /usr/share/zsh/site-functions/_mdcat
  mdless --completions fish > /usr/share/fish/vendor_completions.d/mdless.fish
  mdless --completions bash > /usr/share/bash-completion/completions/mdless
  mdless --completions zsh  > /usr/share/zsh/site-functions/_mdless
  ```
- A built manpage from `mdcat.1.adoc` via [AsciiDoctor]:
  ```sh
  asciidoctor -b manpage -a reproducible -o /usr/share/man/man1/mdcat.1 mdcat.1.adoc
  gzip /usr/share/man/man1/mdcat.1
  ln -s mdcat.1.gz /usr/share/man/man1/mdless.1.gz
  ```

[AsciiDoctor]: https://asciidoctor.org

## Troubleshooting

Set `MDCAT_LOG=trace` for full tracing output, or a module-scoped
filter such as `MDCAT_LOG=mdcat::render=trace` to narrow it down.
Logs go to stderr so they don't contaminate rendered output.

`mdcat --detect-terminal` reports the detected terminal, multiplexer,
and any capabilities discovered via the DA1 probe — useful when an
expected protocol isn't firing.

## Contributing

Bug reports and patches are welcome. Please run the full check
locally before opening a PR:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

Rendering changes require a snapshot rebase; see
[`CONTRIBUTING.md`](./CONTRIBUTING.md) for the full workflow, coding
conventions, and writing-style rules applied to comments and
documentation.

## Authors

- Sebastian Wiesner — original author of `mdcat` (2.x and earlier).
- Pawel Boguszewski — 3.x continuation fork, interactive `mdless`,
  expanded terminal matrix.

See [CHANGELOG.md](./CHANGELOG.md) for the full list of changes per
release.

## License

Binaries and most source files are distributed under the Mozilla
Public License, v. 2.0 — see [LICENSE](./LICENSE). A small number of
files are dual-licensed under Apache 2.0; the file header indicates
the applicable licence.
