# Color theme presets

## Goal

Let users pick a color preset for mdcat and mdless. Ship four
named presets and switch the default from the mdcat 1.x palette
to a pastel-leaning `catppuccin` preset. Preserve the current
ANSI-only output pipeline.

## Non-goals

- 24-bit RGB output. The pipeline keeps emitting AnsiColor
  values; the user's terminal palette decides actual hues.
- User-supplied TOML themes. All presets are compiled in.
- Light/dark variants of a single preset. Each preset is a single
  set of AnsiColor slot bindings.

## Background

Two layers carry color today:

1. `Theme` (`src/theme.rs`) — chrome (headings, links, inline
   code, rules, code-block borders, blockquote bars, HTML).
   Nine fields, all `AnsiColor` named values.
2. Syntax highlighting (`src/render/highlighting.rs`) — syntect
   loads a dumped `Solarized (dark).tmTheme`. The
   `write_as_ansi` function maps the eight Solarized accent RGB
   values to `AnsiColor` and discards Solarized base colors.

Both layers emit only ANSI named colors. A "preset" is therefore
a pair: a `Theme` instance plus an eight-slot table mapping
Solarized accents to `AnsiColor`.

## Public surface

CLI flag on both binaries:

```
--theme <NAME>          [default: catppuccin] [env: MDCAT_THEME]
                        possible values: catppuccin, classic,
                                         dracula, nord
--list-themes           print preset names + descriptions, exit 0
```

Resolution order: CLI flag, then `MDCAT_THEME`, then default.
Invalid name fails clap parsing with a clear message.

`--list-themes` short-circuits before opening input. Output:

```
catppuccin   Pastel default. Cool slots, magenta headings.
classic      mdcat 1.x defaults.
dracula      Warm magenta-led palette.
nord         Cool blue palette.
```

## Library surface

`src/theme.rs` adds:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Preset {
    #[default]
    Catppuccin,
    Classic,
    Dracula,
    Nord,
}

pub type SyntaxMap = [AnsiColor; 8];

impl Preset {
    pub fn theme(self) -> Theme { ... }
    pub fn syntax_map(self) -> SyntaxMap { ... }
}
```

Slot order in `SyntaxMap`: yellow, orange, red, magenta, violet,
blue, cyan, green (matches the existing match arms in
`write_as_ansi`).

`Theme::default()` keeps returning the Classic palette so library
users that depend on the type's `Default` impl don't shift.
`Preset::default()` is `Catppuccin`; the binary uses that path.

`Settings` (in `src/lib.rs`) gains `pub syntax_color_map: SyntaxMap`.
`push_tty` and `push_tty_with_observer` thread it through to
`write_as_ansi`.

## Preset palettes

### Catppuccin (default)

Chrome:

| Slot               | AnsiColor          |
| ------------------ | ------------------ |
| heading            | Magenta + bold     |
| link               | Cyan               |
| inline code        | BrightYellow       |
| image link         | BrightMagenta      |
| rule               | BrightBlack        |
| code block border  | BrightBlack        |
| quote bar          | BrightBlack        |
| html block         | BrightBlack        |
| inline html        | BrightBlack        |

Syntax map: yellow→BrightYellow; orange→BrightRed; red→Red;
magenta→Magenta; violet→BrightMagenta; blue→Blue; cyan→Cyan;
green→Green.

### Classic

Chrome: matches today's `Theme::default()` byte-for-byte.

Syntax map: matches today's `write_as_ansi` table byte-for-byte
(yellow→Yellow; orange→BrightRed; red→Red; magenta→Magenta;
violet→BrightMagenta; blue→Blue; cyan→Cyan; green→Green).

### Dracula

Chrome:

| Slot               | AnsiColor          |
| ------------------ | ------------------ |
| heading            | BrightMagenta + bold |
| link               | BrightCyan         |
| inline code        | BrightYellow       |
| image link         | BrightMagenta      |
| rule               | BrightMagenta      |
| code block border  | BrightBlack        |
| quote bar          | BrightBlack        |
| html block         | BrightMagenta      |
| inline html        | BrightMagenta      |

Syntax map: yellow→BrightYellow; orange→BrightRed; red→BrightRed;
magenta→BrightMagenta; violet→BrightMagenta; blue→BrightCyan;
cyan→BrightCyan; green→BrightGreen.

### Nord

Chrome:

| Slot               | AnsiColor          |
| ------------------ | ------------------ |
| heading            | BrightCyan + bold  |
| link               | Cyan               |
| inline code        | BrightBlue         |
| image link         | Blue               |
| rule               | BrightBlack        |
| code block border  | BrightBlack        |
| quote bar          | BrightBlack        |
| html block         | Cyan               |
| inline html        | Cyan               |

Syntax map: yellow→Yellow; orange→BrightRed; red→Red;
magenta→Magenta; violet→BrightCyan; blue→Blue; cyan→Cyan;
green→Green.

## Implementation outline

1. Add `Preset` enum and per-preset `theme()` / `syntax_map()`
   in `src/theme.rs`. Move the contents of the current
   `Theme::default` impl into `Preset::Classic.theme()`.
2. Generalize `write_as_ansi` to take `&SyntaxMap` and replace
   the hardcoded match arms with table lookup keyed by the same
   eight Solarized accent RGB triples. Keep the panic on
   unexpected RGB.
3. Add `syntax_color_map: SyntaxMap` to `Settings`. Update every
   construction site (production code, tests, doc examples).
4. Wire `--theme` and `--list-themes` in `src/args.rs`. Use
   clap's `value_enum` and `env = "MDCAT_THEME"`. Resolve to
   `Preset`, derive `Theme` and `SyntaxMap`, populate `Settings`.
5. Repeat the wiring in `src/bin/mdless.rs` so the wrapper
   honors the same flag and env var.
6. Update `README.md` and `mdcat.1.adoc` with the new flag and
   one paragraph noting that ANSI-only output means appearance
   depends on the terminal palette.
7. Add a `CHANGELOG.md` entry under "Breaking" announcing the
   default switch and the `classic` opt-out.

## Tests

- Unit in `src/theme.rs`: each preset returns a `Theme` whose
  fields match the table above. `Preset::Classic.theme()` equals
  `Theme::default()` (regression guard for library consumers).
- Unit in `src/render/highlighting.rs`: `write_as_ansi` with the
  Catppuccin syntax map produces output that differs from the
  Classic map for at least one accent (smoke check that the
  table is actually consulted).
- Snapshot in `tests/render.rs`: render `sample/demo.md` once
  per preset, four `insta` snapshots. Each snapshot pins the
  full ANSI byte stream so palette drift is visible in review.
- CLI in `tests/cli.rs`: three new cases:
  - `mdcat --theme nord sample/demo.md` exits 0 and produces
    output containing the Nord heading sequence.
  - `mdcat --list-themes` prints exactly four lines starting
    with the preset names in declaration order, exits 0.
  - `mdcat --theme nope` exits 2 with a clap error.

## Migration notes

Users who relied on the prior default palette set
`MDCAT_THEME=classic` in their shell rc, or pass
`--theme classic` per invocation. The `mdless` wrapper accepts
the same flag and env var. No config file is read; previously
there was no per-user theme config to migrate.

## Risks

- Catppuccin's slot picks assume a cool ANSI palette in the
  user's terminal. On Terminal.app's default 16-color palette
  the result reads as "saturated", not "pastel". The README
  paragraph calls this out.
- The default switch is a visible change. The CHANGELOG entry
  and the `--theme classic` opt-out are the mitigation.
- `write_as_ansi` still panics on unexpected RGB. Because we
  keep the same dumped Solarized theme as the syntect input,
  the set of input RGB values does not grow and the panic
  remains unreachable in practice.
