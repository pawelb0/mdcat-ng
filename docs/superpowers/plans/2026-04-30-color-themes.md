# Color theme presets — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add four ANSI color presets selectable via `--theme` and `MDCAT_THEME`, switch the default to a pastel `catppuccin` preset.

**Architecture:** Define a `Preset` enum that maps to a `Theme` (chrome) and an 8-slot `SyntaxMap` (Solarized accents → AnsiColor). Thread the syntax map through `Settings` to `write_as_ansi`, replacing today's hardcoded match. `clap::ValueEnum` on `Preset` drives `--theme`; `--list-themes` short-circuits before file I/O.

**Tech Stack:** Rust, clap 4 (derive + ValueEnum), anstyle, syntect, insta (snapshot tests).

---

## Files

- Modify `src/theme.rs` — add `Preset`, `SyntaxMap`, `Preset::theme`, `Preset::syntax_map`.
- Modify `src/render/highlighting.rs` — replace hardcoded match in `write_as_ansi` with `&SyntaxMap` lookup.
- Modify `src/lib.rs` — add `syntax_color_map: SyntaxMap` to `Settings`; thread to `write_as_ansi` call sites; update doctests/tests.
- Modify `src/render/code.rs`, `src/render/write.rs` — pass `&Settings.syntax_color_map` into `write_as_ansi` calls (or pass the whole `Settings` if cleaner).
- Modify `src/args.rs` — add `--theme`, `--list-themes` to `CommonArgs`.
- Modify `src/cli.rs` — handle `--list-themes`; build `Settings` from `args.theme`.
- Modify `src/mdless/mod.rs` — same `Settings` wiring (line 417 site).
- Modify `tests/render.rs`, `tests/wrapping.rs` — update `Settings` literals.
- Add `tests/cli.rs` cases — `--theme nord`, `--list-themes`, invalid name.
- Add snapshot tests under `tests/render.rs` — one per preset over `sample/demo.md`.
- Modify `README.md`, `mdcat.1.adoc`, `CHANGELOG.md`.

---

## Task 1: `Preset` enum + chrome themes

**Files:**

- Modify: `src/theme.rs`

- [ ] **Step 1: Write the failing tests**

Append to `src/theme.rs` inside a new `#[cfg(test)] mod tests`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use anstyle::AnsiColor;

    fn fg(s: anstyle::Style) -> Option<anstyle::Color> {
        s.get_fg_color()
    }

    #[test]
    fn classic_matches_legacy_default() {
        let p = Preset::Classic.theme();
        let d = Theme::default();
        assert_eq!(fg(p.heading_style), fg(d.heading_style));
        assert_eq!(fg(p.link_style), fg(d.link_style));
        assert_eq!(fg(p.code_style), fg(d.code_style));
        assert_eq!(p.rule_color, d.rule_color);
        assert_eq!(p.quote_bar_color, d.quote_bar_color);
    }

    #[test]
    fn catppuccin_heading_is_magenta_bold() {
        let t = Preset::Catppuccin.theme();
        assert_eq!(fg(t.heading_style), Some(AnsiColor::Magenta.into()));
        assert!(t.heading_style.get_effects().contains(anstyle::Effects::BOLD));
    }

    #[test]
    fn dracula_link_is_brightcyan() {
        let t = Preset::Dracula.theme();
        assert_eq!(fg(t.link_style), Some(AnsiColor::BrightCyan.into()));
    }

    #[test]
    fn nord_heading_is_brightcyan_bold() {
        let t = Preset::Nord.theme();
        assert_eq!(fg(t.heading_style), Some(AnsiColor::BrightCyan.into()));
        assert!(t.heading_style.get_effects().contains(anstyle::Effects::BOLD));
    }

    #[test]
    fn default_preset_is_catppuccin() {
        assert_eq!(Preset::default(), Preset::Catppuccin);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib theme::tests`
Expected: compile errors — `Preset` not found.

- [ ] **Step 3: Add `Preset` enum and `theme()`**

Append to `src/theme.rs` (above the `#[cfg(test)]` block):

```rust
/// Built-in color preset selectable via `--theme`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Preset {
    /// Pastel default: cool slots, magenta headings.
    #[default]
    Catppuccin,
    /// mdcat 1.x defaults.
    Classic,
    /// Warm magenta-led palette.
    Dracula,
    /// Cool blue palette.
    Nord,
}

impl Preset {
    /// Short one-line description for `--list-themes`.
    pub fn description(self) -> &'static str {
        match self {
            Preset::Catppuccin => "Pastel default. Cool slots, magenta headings.",
            Preset::Classic => "mdcat 1.x defaults.",
            Preset::Dracula => "Warm magenta-led palette.",
            Preset::Nord => "Cool blue palette.",
        }
    }

    /// Chrome colors (headings, links, rules, etc.) for this preset.
    pub fn theme(self) -> Theme {
        use anstyle::AnsiColor::{
            Blue, BrightBlack, BrightBlue, BrightCyan, BrightMagenta, BrightYellow, Cyan, Magenta,
        };
        use anstyle::Style;
        match self {
            Preset::Catppuccin => Theme {
                heading_style: Style::new().fg_color(Some(Magenta.into())).bold(),
                link_style: Style::new().fg_color(Some(Cyan.into())),
                code_style: Style::new().fg_color(Some(BrightYellow.into())),
                image_link_style: Style::new().fg_color(Some(BrightMagenta.into())),
                rule_color: BrightBlack.into(),
                code_block_border_color: BrightBlack.into(),
                quote_bar_color: BrightBlack.into(),
                html_block_style: Style::new().fg_color(Some(BrightBlack.into())),
                inline_html_style: Style::new().fg_color(Some(BrightBlack.into())),
            },
            Preset::Classic => Theme::default(),
            Preset::Dracula => Theme {
                heading_style: Style::new().fg_color(Some(BrightMagenta.into())).bold(),
                link_style: Style::new().fg_color(Some(BrightCyan.into())),
                code_style: Style::new().fg_color(Some(BrightYellow.into())),
                image_link_style: Style::new().fg_color(Some(BrightMagenta.into())),
                rule_color: BrightMagenta.into(),
                code_block_border_color: BrightBlack.into(),
                quote_bar_color: BrightBlack.into(),
                html_block_style: Style::new().fg_color(Some(BrightMagenta.into())),
                inline_html_style: Style::new().fg_color(Some(BrightMagenta.into())),
            },
            Preset::Nord => Theme {
                heading_style: Style::new().fg_color(Some(BrightCyan.into())).bold(),
                link_style: Style::new().fg_color(Some(Cyan.into())),
                code_style: Style::new().fg_color(Some(BrightBlue.into())),
                image_link_style: Style::new().fg_color(Some(Blue.into())),
                rule_color: BrightBlack.into(),
                code_block_border_color: BrightBlack.into(),
                quote_bar_color: BrightBlack.into(),
                html_block_style: Style::new().fg_color(Some(Cyan.into())),
                inline_html_style: Style::new().fg_color(Some(Cyan.into())),
            },
        }
    }
}
```

Re-export from `src/lib.rs`:

```rust
pub use crate::theme::{Preset, Theme};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib theme::tests`
Expected: 5 PASS.

- [ ] **Step 5: Commit**

```bash
git add src/theme.rs src/lib.rs
git commit -m "theme: add Preset enum and chrome palettes"
```

---

## Task 2: `SyntaxMap` type + `Preset::syntax_map`

**Files:**

- Modify: `src/theme.rs`

- [ ] **Step 1: Write the failing tests**

Add to the test module:

```rust
#[test]
fn classic_syntax_map_matches_legacy_table() {
    let m = Preset::Classic.syntax_map();
    // Slot order: yellow, orange, red, magenta, violet, blue, cyan, green
    assert_eq!(m, [
        AnsiColor::Yellow,
        AnsiColor::BrightRed,
        AnsiColor::Red,
        AnsiColor::Magenta,
        AnsiColor::BrightMagenta,
        AnsiColor::Blue,
        AnsiColor::Cyan,
        AnsiColor::Green,
    ]);
}

#[test]
fn catppuccin_syntax_map_bumps_yellow_to_bright() {
    let m = Preset::Catppuccin.syntax_map();
    assert_eq!(m[0], AnsiColor::BrightYellow);
}

#[test]
fn dracula_syntax_map_uses_bright_variants() {
    let m = Preset::Dracula.syntax_map();
    assert_eq!(m[2], AnsiColor::BrightRed);  // red slot
    assert_eq!(m[5], AnsiColor::BrightCyan); // blue slot
    assert_eq!(m[7], AnsiColor::BrightGreen); // green slot
}

#[test]
fn nord_remaps_violet_to_brightcyan() {
    let m = Preset::Nord.syntax_map();
    assert_eq!(m[4], AnsiColor::BrightCyan); // violet slot
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib theme::tests::classic_syntax_map_matches_legacy_table`
Expected: compile error — `syntax_map` not found.

- [ ] **Step 3: Add `SyntaxMap` and `Preset::syntax_map`**

In `src/theme.rs`, before the `Preset` impl:

```rust
/// AnsiColor slots for the eight Solarized accent colors that
/// the syntect theme can produce. Order: yellow, orange, red,
/// magenta, violet, blue, cyan, green.
pub type SyntaxMap = [anstyle::AnsiColor; 8];
```

Add a method to `impl Preset`:

```rust
/// Syntax-token AnsiColor mapping for this preset.
pub fn syntax_map(self) -> SyntaxMap {
    use anstyle::AnsiColor::{
        Blue, BrightCyan, BrightGreen, BrightMagenta, BrightRed, BrightYellow, Cyan, Green,
        Magenta, Red, Yellow,
    };
    match self {
        Preset::Catppuccin => [
            BrightYellow, BrightRed, Red, Magenta, BrightMagenta, Blue, Cyan, Green,
        ],
        Preset::Classic => [
            Yellow, BrightRed, Red, Magenta, BrightMagenta, Blue, Cyan, Green,
        ],
        Preset::Dracula => [
            BrightYellow, BrightRed, BrightRed, BrightMagenta, BrightMagenta, BrightCyan,
            BrightCyan, BrightGreen,
        ],
        Preset::Nord => [
            Yellow, BrightRed, Red, Magenta, BrightCyan, Blue, Cyan, Green,
        ],
    }
}
```

Re-export from `src/lib.rs`:

```rust
pub use crate::theme::{Preset, SyntaxMap, Theme};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib theme::tests`
Expected: 9 PASS.

- [ ] **Step 5: Commit**

```bash
git add src/theme.rs src/lib.rs
git commit -m "theme: add per-preset syntax accent map"
```

---

## Task 3: Generalize `write_as_ansi` to take `&SyntaxMap`

**Files:**

- Modify: `src/render/highlighting.rs`
- Modify: `src/lib.rs` (add `syntax_color_map` to `Settings`)
- Modify: `src/render/code.rs`, `src/render/write.rs` (call sites)
- Modify: `src/cli.rs`, `src/mdless/mod.rs`, `tests/render.rs`, `tests/wrapping.rs` (Settings construction)

- [ ] **Step 1: Write the failing test**

Append to `src/render/highlighting.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use anstyle::AnsiColor;
    use syntect::highlighting::{Color, FontStyle, Style as SynStyle};

    fn region_yellow_accent() -> Vec<(SynStyle, &'static str)> {
        vec![(
            SynStyle {
                foreground: Color { r: 0xb5, g: 0x89, b: 0x00, a: 0xff },
                background: Color::default(),
                font_style: FontStyle::empty(),
            },
            "code",
        )]
    }

    #[test]
    fn write_as_ansi_uses_provided_map() {
        let mut classic_buf = Vec::new();
        let classic = [
            AnsiColor::Yellow, AnsiColor::BrightRed, AnsiColor::Red, AnsiColor::Magenta,
            AnsiColor::BrightMagenta, AnsiColor::Blue, AnsiColor::Cyan, AnsiColor::Green,
        ];
        write_as_ansi(&mut classic_buf, region_yellow_accent().into_iter(), &classic).unwrap();

        let mut bright_buf = Vec::new();
        let mut bright = classic;
        bright[0] = AnsiColor::BrightYellow;
        write_as_ansi(&mut bright_buf, region_yellow_accent().into_iter(), &bright).unwrap();

        assert_ne!(classic_buf, bright_buf);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib render::highlighting::tests`
Expected: compile error — wrong number of arguments to `write_as_ansi`.

- [ ] **Step 3: Refactor `write_as_ansi`**

Replace the function in `src/render/highlighting.rs`:

```rust
pub fn write_as_ansi<'a, W: Write, I: Iterator<Item = (Style, &'a str)>>(
    writer: &mut W,
    regions: I,
    syntax_map: &crate::SyntaxMap,
) -> Result<()> {
    for (style, text) in regions {
        let rgb = {
            let fg = style.foreground;
            (fg.r, fg.g, fg.b)
        };
        let color = match rgb {
            (0x00, 0x2b, 0x36)
            | (0x07, 0x36, 0x42)
            | (0x58, 0x6e, 0x75)
            | (0x65, 0x7b, 0x83)
            | (0x83, 0x94, 0x96)
            | (0x93, 0xa1, 0xa1)
            | (0xee, 0xe8, 0xd5)
            | (0xfd, 0xf6, 0xe3) => None,
            (0xb5, 0x89, 0x00) => Some(syntax_map[0].into()),
            (0xcb, 0x4b, 0x16) => Some(syntax_map[1].into()),
            (0xdc, 0x32, 0x2f) => Some(syntax_map[2].into()),
            (0xd3, 0x36, 0x82) => Some(syntax_map[3].into()),
            (0x6c, 0x71, 0xc4) => Some(syntax_map[4].into()),
            (0x26, 0x8b, 0xd2) => Some(syntax_map[5].into()),
            (0x2a, 0xa1, 0x98) => Some(syntax_map[6].into()),
            (0x85, 0x99, 0x00) => Some(syntax_map[7].into()),
            (r, g, b) => panic!("Unexpected RGB colour: #{r:2>0x}{g:2>0x}{b:2>0x}"),
        };
        let font = style.font_style;
        let effects = Effects::new()
            .set(Effects::BOLD, font.contains(FontStyle::BOLD))
            .set(Effects::ITALIC, font.contains(FontStyle::ITALIC))
            .set(Effects::UNDERLINE, font.contains(FontStyle::UNDERLINE));
        let style = anstyle::Style::new().fg_color(color).effects(effects);
        write!(writer, "{}{}{}", style.render(), text, style.render_reset())?;
    }
    Ok(())
}
```

- [ ] **Step 4: Add `syntax_color_map` field to `Settings`**

Edit `src/lib.rs` `Settings` struct (around line 92):

```rust
/// Colour theme for mdcat
pub theme: Theme,
/// AnsiColor mapping for syntax-highlight accents.
pub syntax_color_map: SyntaxMap,
```

- [ ] **Step 5: Update every `Settings { ... }` literal**

The compile errors will list the sites. Update each to add
`syntax_color_map: Preset::Classic.syntax_map(),` (preserves
current visual behavior). Sites:

- `src/lib.rs:366`, `src/lib.rs:417`, `src/lib.rs:465`
- `src/cli.rs:112`
- `src/mdless/mod.rs:417`
- `tests/render.rs:68`, `tests/render.rs:76`, `tests/render.rs:84`
- `tests/wrapping.rs:38`

- [ ] **Step 6: Update `write_as_ansi` callers**

In `src/render/code.rs` and `src/render/write.rs`, every call to
`write_as_ansi(writer, regions)` becomes
`write_as_ansi(writer, regions, &settings.syntax_color_map)`.
Confirm the call sites with:

```bash
grep -n "write_as_ansi" src/render/
```

Each function that calls `write_as_ansi` already takes either
`settings: &Settings` or `theme: &Theme`. For the latter, pass
`syntax_map: &SyntaxMap` alongside.

- [ ] **Step 7: Run the full test suite**

Run: `cargo test`
Expected: all PASS, including the new `write_as_ansi_uses_provided_map`.

- [ ] **Step 8: Commit**

```bash
git add src/render/highlighting.rs src/render/code.rs src/render/write.rs \
        src/lib.rs src/cli.rs src/mdless/mod.rs tests/render.rs tests/wrapping.rs
git commit -m "render: thread SyntaxMap through write_as_ansi"
```

---

## Task 4: Wire `--theme` and `MDCAT_THEME`

**Files:**

- Modify: `src/args.rs`
- Modify: `src/cli.rs`

- [ ] **Step 1: Write the failing tests**

Add to `src/args.rs` test module:

```rust
#[test]
fn default_theme_is_catppuccin() {
    let args = Args::parse_from(["mdcat", "-"]).command;
    assert_eq!(args.theme, crate::Preset::Catppuccin);
}

#[test]
fn theme_flag_picks_nord() {
    let args = Args::parse_from(["mdcat", "--theme", "nord", "-"]).command;
    assert_eq!(args.theme, crate::Preset::Nord);
}

#[test]
fn theme_env_var_used_when_flag_absent() {
    temp_env::with_var("MDCAT_THEME", Some("dracula"), || {
        let args = Args::parse_from(["mdcat", "-"]).command;
        assert_eq!(args.theme, crate::Preset::Dracula);
    });
}

#[test]
fn invalid_theme_name_fails_parsing() {
    let r = Args::try_parse_from(["mdcat", "--theme", "nope", "-"]);
    assert!(r.is_err());
}
```

(`temp_env` is already a dev-dependency.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib args::tests`
Expected: compile error — `theme` field not found.

- [ ] **Step 3: Add `theme` and `list_themes` to `CommonArgs`**

In `src/args.rs` `CommonArgs`, append:

```rust
/// Color preset.
#[arg(long = "theme", value_enum, env = "MDCAT_THEME", default_value_t = crate::Preset::Catppuccin)]
pub theme: crate::Preset,
/// Print the available color presets and exit.
#[arg(long = "list-themes")]
pub list_themes: bool,
```

- [ ] **Step 4: Wire to `Settings` in `src/cli.rs`**

In `src/cli.rs` after the `--completions` short-circuit (around
line 48), add:

```rust
if args.list_themes {
    use clap::ValueEnum;
    for v in crate::Preset::value_variants() {
        let name = v.to_possible_value().unwrap();
        println!("{:<12} {}", name.get_name(), v.description());
    }
    std::process::exit(0);
}
```

Replace the `Settings` literal at line 112 with:

```rust
let settings = Settings {
    terminal_capabilities: capabilities,
    terminal_size,
    multiplexer,
    syntax_set: &SyntaxSet::load_defaults_newlines(),
    theme: args.theme.theme(),
    syntax_color_map: args.theme.syntax_map(),
    wrap_code: args.wrap_code,
};
```

Apply the same `theme` + `syntax_color_map` substitution at the
`Settings` construction in `src/mdless/mod.rs:417`.

Add `Preset` to the existing `use` import in `src/cli.rs`:

```rust
use crate::{
    create_resource_handler, process_file, MarkdownParser, Multiplexer, Preset, Settings,
    TerminalProgram, TerminalSize, Theme,
};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib args::tests`
Expected: 4 new tests PASS.

- [ ] **Step 6: Smoke-test the binary**

```bash
cargo run --bin mdcat -- --list-themes
```

Expected output (exit 0):

```
catppuccin   Pastel default. Cool slots, magenta headings.
classic      mdcat 1.x defaults.
dracula      Warm magenta-led palette.
nord         Cool blue palette.
```

```bash
cargo run --bin mdcat -- --theme nope sample/demo.md
```

Expected: clap error mentioning `nope`, exit 2.

- [ ] **Step 7: Commit**

```bash
git add src/args.rs src/cli.rs src/mdless/mod.rs
git commit -m "args: --theme flag, MDCAT_THEME, --list-themes"
```

---

## Task 5: Snapshot per preset over `sample/demo.md`

**Files:**

- Modify: `tests/render.rs`

- [ ] **Step 1: Extend the import line**

In `tests/render.rs:24`, add `Preset`:

```rust
use mdcat::{Environment, Event, Multiplexer, Preset, Settings, Theme};
```

- [ ] **Step 2: Add the snapshot test**

Append to `tests/render.rs`, after `test_render`:

```rust
#[test]
fn render_themes() {
    let mut isettings = insta::Settings::clone_current();
    isettings.set_snapshot_path("snapshots/render");
    isettings.set_prepend_module_to_snapshot(false);
    let _guard = isettings.bind_to_scope();

    for preset in [Preset::Catppuccin, Preset::Classic, Preset::Dracula, Preset::Nord] {
        let settings = Settings {
            terminal_capabilities: TerminalProgram::Ansi.capabilities(),
            terminal_size: TerminalSize::default(),
            multiplexer: Multiplexer::default(),
            theme: preset.theme(),
            syntax_color_map: preset.syntax_map(),
            syntax_set: syntax_set(),
            wrap_code: false,
        };
        let name = format!("themes-{preset:?}").to_lowercase();
        assert_snapshot!(name, render_to_string("sample/demo.md", &settings));
    }
    drop(_guard);
}
```

- [ ] **Step 3: Run and accept snapshots**

Run: `cargo insta test --review`
Expected: four new snapshots written under
`tests/snapshots/render/`, accept all.

- [ ] **Step 4: Verify deterministic re-run**

Run: `cargo test --test render render_themes`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/render.rs tests/snapshots/
git commit -m "tests: snapshot demo.md per color preset"
```

---

## Task 6: User-facing docs

**Files:**

- Modify: `README.md`
- Modify: `mdcat.1.adoc`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: README**

Find the section that lists CLI flags (or the closest equivalent
under "Usage" / "Features"). Add one line:

```
- `--theme NAME` — pick a color preset. Names: catppuccin
  (default), classic, dracula, nord. Reads `MDCAT_THEME`.
```

Add a short paragraph below the install/usage sections:

> mdcat emits ANSI named colors only. Each preset picks
> different slots; the actual hue comes from your terminal's
> 16-color palette. Configure your terminal with a Catppuccin
> palette to get the pastel look implied by the default. To
> restore the previous mdcat 1.x look, run with
> `--theme classic` or set `MDCAT_THEME=classic`.

- [ ] **Step 2: Man page**

In `mdcat.1.adoc`, add inside the OPTIONS list:

```
*--theme*=_NAME_::
    Color preset. One of: *catppuccin* (default), *classic*,
    *dracula*, *nord*. Also read from *MDCAT_THEME*.

*--list-themes*::
    Print available color presets and exit.
```

Add a short note in the ENVIRONMENT section:

```
*MDCAT_THEME*::
    Default value for *--theme*. Overridden by the flag.
```

- [ ] **Step 3: CHANGELOG**

Prepend an entry for the next unreleased version:

```markdown
## Unreleased

### Added

- `--theme` flag (and `MDCAT_THEME` env var) selecting one of
  four built-in color presets: `catppuccin`, `classic`,
  `dracula`, `nord`.
- `--list-themes` to print the available presets.

### Changed

- Default color preset is now `catppuccin`. Pass `--theme
  classic` (or set `MDCAT_THEME=classic`) to restore the prior
  appearance.
```

- [ ] **Step 4: Verify docs**

Run: `cargo run --bin mdcat -- --help | grep -i theme`
Expected: `--theme` and `--list-themes` lines visible.

Run: `asciidoctor -b manpage mdcat.1.adoc -o /tmp/mdcat.1 && man /tmp/mdcat.1 | grep -A1 theme`
Expected: the two new entries render.

- [ ] **Step 5: Commit**

```bash
git add README.md mdcat.1.adoc CHANGELOG.md
git commit -m "docs: --theme flag and default switch"
```

---

## Task 7: Final verification

- [ ] **Step 1: Full test run**

Run: `cargo test --all-targets`
Expected: all PASS.

- [ ] **Step 2: Lint**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 3: Visual sanity check**

```bash
cargo run --bin mdcat -- --theme catppuccin sample/demo.md | head -40
cargo run --bin mdcat -- --theme classic sample/demo.md | head -40
cargo run --bin mdcat -- --theme dracula sample/demo.md | head -40
cargo run --bin mdcat -- --theme nord sample/demo.md | head -40
```

Expected: visibly different chrome and code-block colors per preset.

- [ ] **Step 4: PR open** (optional, only if instructed)

```bash
git log --oneline origin/main..HEAD
gh pr create --title "Color theme presets" --body "$(cat <<'EOF'
## What

- Add `--theme` and `MDCAT_THEME` selecting one of four ANSI
  color presets.
- Switch default to `catppuccin`; `--theme classic` restores
  prior appearance.
- Snapshot tests pin each preset's output over `sample/demo.md`.

## Checks

- `cargo test --all-targets`
- `cargo clippy --all-targets -- -D warnings`
EOF
)"
```
