# mdcat stress test — every element, most gnarly combinations

This document is an intentional kitchen-sink to make sure the renderer
handles every CommonMark + GFM construct mdcat supports, plus the
typically gnarly combinations: deep nesting, wide content, Unicode,
emoji, ANSI-breaking text, and ASCII diagrams.

## Heading levels

# H1 — top level
## H2 — second level
### H3 — third level
#### H4 — fourth level
##### H5 — fifth level
###### H6 — sixth level

Headings with inline formatting: `code`, **bold**, _italic_, and
[a link](https://example.com/) should all survive.

## Inline markup

Plain paragraph with **bold**, *italic*, `inline code`, ~~strikethrough~~,
and ***bold italic***. Also **_bold with italic inside_** and
`code with **ignored** markup`.

Long words and URLs stress the wrapper:
https://example.com/a/very/long/path/that/does/not/break/nicely/until/way/past/eighty/columns.html

Soft wrapped line —  
with a hard break before this one, using two trailing spaces.

Unicode essentials: café, naïve, straße, μέλλον, 漢字, العربية, עברית, 🧪🚀✨.

Combining marks and ZWJ: é (U+0065 U+0301), 👨‍👩‍👧‍👦, 🏳️‍🌈.

## Links and references

Inline link: [mdcat on GitHub](https://github.com/swsnr/mdcat "Title here").
Reference link: [crates.io][cratesio] and again [crates.io][cratesio].
Autolink: <https://example.org/auto>. Email: <hello@example.org>.

Relative link: [README](./README.md) should become a `file://` URL.

[cratesio]: https://crates.io/crates/mdcat "See releases"

## Blockquotes

> Single-line quote.

> Multi-line quote that wraps nicely when the line is long enough
> to exceed the terminal width so we can see the bar appear on
> every wrapped line, including this third physical line.
>
> Second paragraph still inside the quote.

> Quote with **bold**, *italic*, `code`, and [a link](https://example.com/).

> Nested quotes:
>
>> inner quote with its own *emphasis*.
>>
>>> third level should still resolve, even if the bar doesn't nest.

## Lists

Unordered:

* level 1
  * level 2 with **bold**
    * level 3 with `code`
      * level 4 with a long sentence that has to wrap to demonstrate
        that list-item wrapping respects the current indent level
        and nests cleanly.
* another top-level item

Ordered:

1. first
2. second
   1. second-first
   2. second-second
      - mixed unordered inside ordered
      - [ ] task item unchecked
      - [x] task item checked
3. third with a blockquote:

   > the quote lives inside the list item

4. fourth with a code block:

   ```rust
   fn hello() {
       println!("from inside a list item");
   }
   ```

5. fifth with a *nested paragraph*

   Second paragraph of the list item. It should stay indented under
   the bullet.

Task list at top level:

- [ ] Unchecked todo
- [x] Done with `code` in it
- [ ] ~~Scratched-out~~ work
- [x] Bold task **matters**

## Code blocks

Fenced with language:

```rust
/// A doc comment — syntect should colour me.
pub fn fibonacci(n: u64) -> u64 {
    match n {
        0 | 1 => n,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

fn main() {
    for i in 0..10 {
        println!("fib({i}) = {}", fibonacci(i));
    }
}
```

```python
# Python with unicode, long line, and a trailing newline.
def greet(name: str) -> None:
    print(f"Hello, {name}! 🎉 — this line is intentionally long enough to exceed eighty columns so we see wrapping or truncation behaviour")

greet("world")
```

```bash
#!/usr/bin/env bash
set -euo pipefail
# A loop with a very long line embedded as a comment to test wrapping inside a highlighted block.
for i in 1 2 3; do echo "$i"; done
```

```json
{
  "name": "mdcat",
  "version": "3.0.0-alpha.0",
  "keywords": ["markdown", "terminal", "renderer"],
  "features": {
    "sixel": true,
    "kitty-graphics": true,
    "iterm2-images": true
  }
}
```

Fenced without a language (no syntect):

```
plain   code
   with   weird   whitespace
and a trailing
newline
```

Fenced with an unknown language (still a code block, but no highlighting):

```brainfuck
++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.
```

Indented code block (four-space):

    indented code does not have fence or language
    but should still get the code styling and box

Empty code block (should still render):

```
```

## Tables

Simple:

| Feature  | Status | Notes               |
|----------|--------|---------------------|
| Headings | ✅     | All six levels      |
| Lists    | ✅     | Task lists included |
| Images   | ⚠️     | Terminal-dependent  |
| Alerts   | ✅     | GFM blockquotes     |

Alignment:

| Left  | Center | Right |
|:------|:------:|------:|
| a     |   b    |     c |
| long  | center |    99 |
| thing |   x    |    42 |

Wide table that must truncate:

| Subcommand | Default behaviour                                 | `--json` output                                              | Notes                       |
|------------|---------------------------------------------------|--------------------------------------------------------------|-----------------------------|
| toc        | Indented bullets, one heading per line with `* `  | NDJSON with `{"level":N,"text":"...","anchor":"..."}` fields | `--depth N` caps at level   |
| slice      | Raw markdown of the requested section             | N/A — use `--render` to pipe through the normal renderer      | `--all` emits every match   |
| links      | `URL\tTITLE` per line                             | NDJSON `{url,title,text,kind}`                                | `--unique` dedupes          |

Single-column edge case:

| only |
|------|
| one  |
| two  |

## Horizontal rules

Before:

---

After, with asterisks:

***

And with underscores:

___

## HTML passthrough

Inline HTML: a plain <em>em</em> and <strong>strong</strong> tag, plus a
<code>code</code> span in angle brackets.

Block HTML:

<div class="note">
  <p>This is raw HTML and mdcat prints it verbatim with a distinct style.</p>
  <p>Multi-line should still align with the block's indent.</p>
</div>

## ASCII diagrams

A tree:

```
mdcat/
├── src/
│   ├── main.rs         # CLI entry
│   ├── lib.rs          # library + process_file
│   ├── render/
│   │   ├── blocks.rs   # paragraphs, headings, rules
│   │   ├── code.rs     # code blocks with the │ bar
│   │   ├── tables.rs   # unicode box tables
│   │   └── images.rs   # iTerm2 / Kitty / Sixel dispatch
│   └── terminal/
│       ├── detect.rs   # env-var based detection
│       ├── probe.rs    # DA1 sixel probe
│       └── multiplexer.rs
└── tests/
    ├── render.rs       # snapshot tests
    └── cli.rs          # CLI integration
```

A sequence diagram:

```
 User               mdcat                 Terminal
  │                   │                      │
  │  mdcat file.md    │                      │
  │ ────────────────▶ │                      │
  │                   │  detect capabilities │
  │                   │ ───────────────────▶ │
  │                   │ ◀─────────── Kitty,  │
  │                   │              sixel,  │
  │                   │              osc8    │
  │                   │  push_tty(events)    │
  │                   │ ─────────────────▶   │
  │                   │                      │
  │                   │     image escape     │
  │                   │ ◀──── wraps in tmux  │
  │                   │        DCS if $TMUX  │
  │ ◀──── rendered    │                      │
  │        output     │                      │
```

A box diagram:

```
╔══════════════════╗     ╔═══════════════════╗
║  pulldown-cmark  ║────▶║   render state    ║
║    Event stream  ║     ║    machine        ║
╚══════════════════╝     ╚═══════════════════╝
                                   │
                                   ▼
                          ╔═══════════════════╗
                          ║  terminal writer  ║
                          ╚═══════════════════╝
```

A flowchart:

```
      ┌──────────┐   no   ┌──────────────┐
      │ is TTY?  │──────▶│ Dumb (text)  │
      └────┬─────┘        └──────────────┘
           │ yes
           ▼
      ┌──────────┐
      │ --ansi?  │──yes─▶ Ansi
      └────┬─────┘
           │ no
           ▼
      ┌──────────┐
      │ detect() │──▶ iTerm2 / Kitty / Sixel / …
      └──────────┘
```

## Images

PNG:

![Rust logo](./rust-logo-128x128.png "The Rust logo")

SVG:

![Rust logo SVG](./rust-logo.svg)

Remote (requires network):

![example badge](https://img.shields.io/badge/mdcat-3.0-blue)

Image inside a list:

1. First step
2. Look at this:

   ![Rust logo inline](./rust-logo-128x128.png)

3. Third step

## Pathological edge cases

Consecutive hard breaks:

one  
two  
three

Backtick salad: ``` nested ``backticks`` inside ``` a span, plus a lone `.

A super long word that absolutely cannot be split anywhere:
supercalifragilisticexpialidocioussupercalifragilisticexpialidocioussupercalifragilisticexpialidocious

A paragraph containing the literal string `ESC[31m` should not colour the
rest of the page red — the library must quote or strip raw ANSI in user
text.

## Closing thoughts

If every heading, quote, list, code block, table, diagram, image, and
inline run above looked right, we're in good shape. Any visual oddities
here are a bug.
