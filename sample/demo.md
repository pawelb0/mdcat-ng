# mdcat-ng 0.2

**Fancy `cat` for CommonMark — with inline images, OSC 8 links,
GFM alerts, footnotes, and a markdown-aware interactive viewer.**

## Syntax highlighting across languages

Rust:

```rust
fn render(md: &str) -> String {
    Parser::new_ext(md, markdown_options())
        .map(stylise)
        .collect()
}
```

Python[^py]:

```python
from dataclasses import dataclass

@dataclass(frozen=True)
class Heading:
    level: int
    text: str

    def anchor(self) -> str:
        return self.text.lower().replace(" ", "-")
```

TypeScript:

```typescript
type Match = { line: number; styled: [number, number] };

const firstAfter = (matches: Match[], top: number): Match | null =>
    matches.find(m => m.line > top) ?? null;
```

Bash:

```bash
for f in target/release/mdcat target/release/mdless; do
    strip "$f" && codesign --force --deep --sign - "$f"
done
```

JSON:

```json
{
  "name": "mdcat",
  "version": "3.0.0",
  "bins": ["mdcat", "mdless"],
  "features": ["sixel", "svg", "image-processing"]
}
```

SQL[^sql]:

```sql
SELECT heading.text, COUNT(ref.id) AS references
FROM heading LEFT JOIN reference ref ON ref.heading_id = heading.id
GROUP BY heading.id
HAVING references > 0
ORDER BY references DESC;
```

Go:

```go
func stripAnsi(b []byte) []byte {
    out := b[:0]
    for i := 0; i < len(b); i++ {
        if b[i] == 0x1b {
            i = skipEscape(b, i) - 1
            continue
        }
        out = append(out, b[i])
    }
    return out
}
```

## A tight feature matrix

| Capability            | 2.x | **0.2** | Notes                         |
|:----------------------|:---:|:-------:|:------------------------------|
| Inline images         |  ✓  |  **✓**  | iTerm2, Kitty, Sixel, …       |
| Interactive viewer    |     |  **✓**  | vi keys, search, bookmarks    |
| GFM alerts            |     |  **✓**  | NOTE / TIP / WARNING / …      |
| Footnotes             |     |  **✓**  | Refs + bottom-of-doc bodies   |
| Definition lists      |     |  **✓**  |                               |
| Wiki links            |     |  **✓**  | `[[Page]]`                    |
| tmux passthrough      |     |  **✓**  | Images survive multiplexers   |

## Alert blockquotes speak colour

> [!NOTE]
> mdcat tags alert blockquotes with a coloured label you can see.

> [!TIP]
> Run `mdless FILE` for the built-in interactive viewer.

> [!WARNING]
> Image protocols are stripped inside `mdless` — scrolling is safe.

## Task lists track progress

- [x] Ship the interactive viewer (`mdless`)
- [x] Land markdown extensions
- [x] Record a demo worth watching
- [ ] Sleep

## Definition lists name things

pulldown-cmark
: upstream CommonMark parser mdcat consumes.

syntect
: syntax highlighter behind every fenced code block.

resvg
: SVG rasteriser feeding pixel protocols.

## Footnotes and wiki links

Click the names: [[HomePage]] or [[Installation|installation docs]]
render as OSC 8 hyperlinks. Inline references surface in the
body[^mdless] and collect their bodies[^repo] at the bottom of the
document.

[^py]: The `dataclasses` decorator landed in 3.7; `frozen=True`
      makes instances hashable and immutable.
[^sql]: The query tallies how many footnote references point at
      each heading — useful for finding popular sections.
[^mdless]: `mdless` turns the same document into an interactive view
      with search, jumps, bookmarks, and a TOC modal.
[^repo]: Source: https://github.com/pawelb0/mdcat-ng

---

## Try it yourself

```sh
mdcat README.md
mdless --search "## Installation" README.md
```
