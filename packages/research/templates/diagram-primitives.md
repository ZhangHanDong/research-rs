# diagram-primitives — self-contained SVG toolkit for rich-html reports

This file ships inside `research-rs`. It is the **minimum subset** of
the [diagram-design](https://github.com/cathrynlavery/diagram-design)
skill (stone+rust variant) needed to author diagrams that match the
`rich-report.html` template exactly. If you have the full diagram-design
skill installed at `~/.claude/skills/diagram-design/`, prefer it — it has
13 type-specific references. Otherwise, this file alone is enough to
ship a well-formed diagram.

Scope: palette + fonts + 6 primitives + budgets + viewBox convention.
Everything an agent needs to hand-write an SVG under
`<session_dir>/diagrams/<name>.svg` that the CLI will inline without
warnings.

---

## 1. Palette — stone+rust tokens

These hex values are load-bearing — the template's CSS is tuned for
them. Do not substitute. Do not pick variants.

```
paper    #f5f4ed   page background
paper-2  #efeee5   container background (inside <div class="diagram">)
ink      #0b0d0b   primary text + stroke
muted    #52534e   secondary text, default arrow stroke
soft     #65655c   external / input node stroke
rule     rgba(11,13,11,0.10)   hairline border
accent   #f7591f   coral — 1–2 focal nodes only, never all
link     #1a70c7   HTTP / external / syscall arrows
```

## 2. Fonts

Include this `<link>` at the top of the SVG's parent HTML only if the
diagram is standalone. Diagrams inlined by the rich-report template
inherit fonts from the template itself — you don't need to re-declare.

```html
<link href="https://fonts.googleapis.com/css2?family=Instrument+Serif:ital@0;1&family=Geist:wght@400;500;600&family=Geist+Mono:wght@400;500;600&display=swap" rel="stylesheet">
```

Typography discipline:
- **Node name** — Geist sans, 12px, weight 600, `text-anchor="middle"`
- **Technical sublabel** — Geist Mono, 9px, muted
- **Type badge on node** — Geist Mono, 7px, uppercase, `letter-spacing="0.08em"`
- **Arrow label** — Geist Mono, 8px, uppercase, `letter-spacing="0.08em"`
- **Diagram title inside SVG** — Instrument Serif, 18px, weight 400
- **Editorial aside inside SVG** — Instrument Serif *italic*, 9px, muted

Mono is for **technical content only** (URLs, CDP methods, hex, syscall
names). Human-readable node names go in Geist sans. Never JetBrains Mono.

## 3. Complexity budget (per diagram)

| Limit | Value |
|-------|-------|
| Max nodes | 9 |
| Max arrows | 12 |
| Max coral / focal elements | 2 |
| Max annotation callouts | 2 |
| Max legend entries | 5 |
| Max items in a quadrant | 12 |

If you exceed: split into overview + detail diagrams.

## 4. viewBox & grid

- Default `viewBox="0 0 920 H"` where H is 320 / 380 / 480 / 520 depending on content density.
- 40 px left/right margin before node x-coordinates.
- Every font size, node dimension, gap, x/y coordinate **divisible by 4**. Non-negotiable. Exempt: stroke widths (`0.8`, `1`, `1.2`) and opacity values.
- Draw arrows **before** boxes so z-order puts lines behind nodes.
- Every arrow label sits on an opaque `fill="#f5f4ed"` (or `#efeee5` if inside `.diagram` wrapper) rect to prevent bleed-through.
- Legend is a horizontal strip at the bottom, never floating inside the diagram area.

## 5. Primitive snippets — paste-ready

### 5.1 Background + dot pattern + arrow markers

```svg
<defs>
  <pattern id="dots" width="22" height="22" patternUnits="userSpaceOnUse">
    <circle cx="1" cy="1" r="0.9" fill="rgba(11,13,11,0.10)"/>
  </pattern>
  <marker id="arrow" markerWidth="8" markerHeight="6" refX="7" refY="3" orient="auto">
    <polygon points="0 0, 8 3, 0 6" fill="#52534e"/>
  </marker>
  <marker id="arrow-accent" markerWidth="8" markerHeight="6" refX="7" refY="3" orient="auto">
    <polygon points="0 0, 8 3, 0 6" fill="#f7591f"/>
  </marker>
  <marker id="arrow-link" markerWidth="8" markerHeight="6" refX="7" refY="3" orient="auto">
    <polygon points="0 0, 8 3, 0 6" fill="#1a70c7"/>
  </marker>
</defs>
<rect width="100%" height="100%" fill="#efeee5"/>
<rect width="100%" height="100%" fill="url(#dots)" opacity="0.4"/>
```

### 5.2 Node box — backend / core

```svg
<!-- box -->
<rect x="X" y="Y" width="W" height="H" rx="6" fill="#ffffff" stroke="#0b0d0b" stroke-width="1"/>
<!-- type badge (rx=2, NOT a pill) -->
<rect x="X+8" y="Y+8" width="36" height="12" rx="2" fill="transparent" stroke="rgba(11,13,11,0.40)" stroke-width="0.8"/>
<text x="X+26" y="Y+17" fill="#0b0d0b" font-size="7" font-family="'Geist Mono', monospace" text-anchor="middle" letter-spacing="0.08em">CORE</text>
<!-- node name -->
<text x="CX" y="CY+2" fill="#0b0d0b" font-size="12" font-weight="600" font-family="'Geist', sans-serif" text-anchor="middle">Node Name</text>
<!-- technical sublabel -->
<text x="CX" y="CY+18" fill="#52534e" font-size="9" font-family="'Geist Mono', monospace" text-anchor="middle">tech:detail</text>
```

### 5.3 Node box — coral focal (1–2 per diagram max)

```svg
<rect x="X" y="Y" width="W" height="H" rx="6" fill="rgba(247,89,31,0.08)" stroke="#f7591f" stroke-width="1"/>
<rect x="X+8" y="Y+8" width="40" height="12" rx="2" fill="transparent" stroke="rgba(247,89,31,0.50)" stroke-width="0.8"/>
<text x="X+28" y="Y+17" fill="#f7591f" font-size="7" font-family="'Geist Mono', monospace" text-anchor="middle" letter-spacing="0.08em">FOCAL</text>
<!-- optional watermark number in lower right corner -->
<text x="X+W-8" y="Y+H-8" fill="rgba(247,89,31,0.10)" font-size="32" font-weight="600" font-family="'Geist Mono', monospace" text-anchor="end">01</text>
<!-- name + sublabel same as 5.2 -->
```

### 5.4 Node box — external / input / cloud

```svg
<!-- dashed border marks async / optional -->
<rect x="X" y="Y" width="W" height="H" rx="6" fill="rgba(82,83,78,0.10)" stroke="#65655c" stroke-width="1"/>
<rect x="X+8" y="Y+8" width="28" height="12" rx="2" fill="transparent" stroke="rgba(101,101,92,0.40)" stroke-width="0.8"/>
<text x="X+22" y="Y+17" fill="#65655c" font-size="7" font-family="'Geist Mono', monospace" text-anchor="middle" letter-spacing="0.08em">EXT</text>
```

### 5.5 Arrow with masked label

```svg
<!-- arrow: draw BEFORE the destination box -->
<line x1="X1" y1="Y1" x2="X2" y2="Y2" stroke="#52534e" stroke-width="1.2" marker-end="url(#arrow)"/>
<!-- masked label on arrow midpoint -->
<rect x="MID_X-22" y="ARROW_Y-7" width="44" height="12" rx="2" fill="#efeee5"/>
<text x="MID_X" y="ARROW_Y+2" fill="#52534e" font-size="8" font-family="'Geist Mono', monospace" text-anchor="middle" letter-spacing="0.08em">WRITE</text>
```

Dashed / async variants:
- add `stroke-dasharray="5,4"` to `<line>` / `<path>`
- use `stroke="#f7591f" marker-end="url(#arrow-accent)"` for primary hot path (1–2 only)
- use `stroke="#1a70c7" marker-end="url(#arrow-link)"` for HTTP / external API calls

### 5.6 Legend strip (bottom)

```svg
<!-- hairline separator above the strip -->
<line x1="40" y1="LEG_Y-8" x2="VIEWBOX_W-40" y2="LEG_Y-8" stroke="rgba(11,13,11,0.10)" stroke-width="0.8"/>
<text x="40" y="LEG_Y+8" fill="#52534e" font-size="8" font-family="'Geist Mono', monospace" letter-spacing="0.18em">LEGEND</text>
<!-- entries, 80–100 px apart -->
<rect x="120" y="LEG_Y" width="14" height="10" rx="2" fill="rgba(247,89,31,0.08)" stroke="#f7591f" stroke-width="1"/>
<text x="140" y="LEG_Y+8" fill="#52534e" font-size="8.5" font-family="'Geist', sans-serif">Focal</text>
<rect x="200" y="LEG_Y" width="14" height="10" rx="2" fill="#ffffff" stroke="#0b0d0b" stroke-width="1"/>
<text x="220" y="LEG_Y+8" fill="#52534e" font-size="8.5" font-family="'Geist', sans-serif">Core</text>
```

Expand `viewBox` height by ~60 px for the legend strip.

## 6. Type-specific anchors

### Architecture diagrams
- Nodes grouped by tier (frontend → backend → data) or by trust boundary.
- 1–2 coral focal nodes: the primary integration point or the key decision node.
- Primary flow runs left→right or top→down. Pick one, hold it.

### Quadrant diagrams (good for news / sentiment / comparison)
- Two axes crossing at center. Labels at each axis end (top / bottom / left / right), axis titles in Geist Mono uppercase.
- Four quadrant *corner* labels in very-muted ink (`rgba(11,13,11,0.35)`) so they guide without dominating.
- Plot points: `<circle r="5" fill="#0b0d0b"/>` for notable, `<circle r="7" fill="#f7591f"/>` + `<circle r="14" fill="none" stroke="#f7591f" opacity="0.35"/>` for 1–2 focal (dominant story).
- Each point has a title (Geist sans 10px weight 600) + sublabel (Geist Mono 9px muted).
- Never exceed 12 points — split if you have more.

### Timeline
- Single horizontal axis. Tick marks at meaningful dates only.
- Events as `<circle>` + angled label above/below alternating.

### Flow / sequence
- Lanes as translucent rectangles (`fill="rgba(11,13,11,0.03)"` with dashed border).
- Messages as arrows between lanes, labeled with action in mono.

## 7. Anti-patterns (auto-rejected by a careful reviewer)

- Every node coral ("this is important too") — hierarchy collapses.
- Bidirectional arrow where one direction is obvious from layout.
- Legend floating inside the diagram area rather than at the bottom.
- Arrow label without a masking rect — reads as crossed-out.
- Vertical `writing-mode` text on arrows — unreadable.
- 3 equal-width generic summary cards as default — vary widths.
- Shadows on anything.
- `rounded-2xl` (rx > 10) — keep rx between 4 and 8.
- Mixing Geist Mono and JetBrains Mono. Pick Geist Mono.

## 8. Pre-ship checklist

Before saving the SVG under `diagrams/`:
- [ ] viewBox declared, proportions divisible by 4
- [ ] Arrow markers declared in `<defs>` before first use
- [ ] Arrows drawn before their destination boxes (z-order)
- [ ] ≤ 2 coral / focal elements
- [ ] Every arrow label has an opaque `<rect>` behind it
- [ ] Legend at bottom, horizontal strip, not floating
- [ ] Human names in Geist sans; ports / URLs / commands in Geist Mono
- [ ] File size ≤ 512 KB (template degrades larger files to `<img>`)
- [ ] Filename kebab-case, short, descriptive (`axis.svg`, `architecture.svg`, `self-healing.svg`)

## 9. When to reach for the full diagram-design skill

The full skill at `~/.claude/skills/diagram-design/` adds:
- 13 type-specific references (ER, swimlane, venn, pyramid, tree, etc.)
- `primitive-annotation.md` (editorial callouts with curved leader lines)
- `primitive-sketchy.md` (hand-drawn wobble via turbulence filter)
- Onboarding flow for re-skinning (pull palette + fonts from a website)
- Type-specific anti-pattern catalogs

If you are doing a new diagram type this file doesn't cover, or want a
sketchy/editorial variant, install it:

```
git clone https://github.com/cathrynlavery/diagram-design ~/.claude/skills/diagram-design
```

Otherwise this file alone is enough to ship architecture, quadrant,
timeline, and flow diagrams that the rich-html template will render
correctly.
