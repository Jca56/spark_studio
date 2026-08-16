# Lantern Studio Title Bar — Spec

Handoff from the lantern-studio session (2026-08-15) so Spark Studio's title
bar can match. Values are exact, read from
`lantern-studio/src/app/menus.rs::view_title_bar` and `src/app/theme.rs`.
Lantern is iced 0.14 on wgpu; numbers are logical pixels.

## Bar

| Property   | Value |
|------------|-------|
| Height     | 44 px |
| Padding    | 0 vertical, 12 horizontal |
| Background | `#191200` (PANEL) |
| Border     | 1 px `#302820` (BORDER), radius 0 |
| Layout     | `menus … [spacer] … logo + window controls` in one row, spacing 8, vertically centered |

## Menu buttons (File / Edit / …)

- Text size **20**, padding `[4, 10]`
- Idle: transparent background, text `#e8dcc8` (TEXT)
- Hover/pressed: background `rgba(255,255,255,0.08)`, border radius 4
- Open menu: solid `#4a4038` (ACTIVE) background

## Logo block (right side, before window controls)

- App icon buttons: **30×30** icon inside a button with padding 4
  (plain non-button icon in earlier builds was 38×38)
- Wordmark: `L A N T E R N   S T U D I O` — literal spaces between
  letters, three spaces between words
- Text size **26**, **bold**, color `#ffc800` (ACCENT)
- Row spacing 10, vertically centered

## Window controls (minimize / maximize / close)

- Glyphs: `─` U+2500, `□` U+25A1, `✕` U+2715 — text, size **14**, centered
- Button padding `[4, 6]`, border radius **50** (fully round), spacing 2
- Idle: transparent background, glyph `#8a7d6a` (TEXT_DIM)
- Hover: background `rgba(255,255,255,0.15)`, glyph `#e8dcc8`
- Close hover: background `rgb(232,18,36)` (`Color::from_rgb(0.91, 0.07, 0.14)`), glyph white
- Hover state is tracked app-side (`WinBtnHover(id)` / `WinBtnUnhover`
  messages via `mouse_area`) so styling isn't limited to iced's
  built-in button status

## Palette used above

| Token     | Hex       |
|-----------|-----------|
| PANEL     | `#191200` |
| PANEL_DARK| `#100c00` |
| TEXT      | `#e8dcc8` |
| TEXT_DIM  | `#8a7d6a` |
| ACCENT    | `#ffc800` |
| ACCENT_HOVER | `#f0c040` |
| BORDER    | `#302820` |
| ACTIVE    | `#4a4038` |

Below the title bar Lantern runs a 4 px rainbow gradient rule
(`t::RAINBOW`, 6 stops) between the toolbar and viewport — mentioning it
because it reads as part of the chrome identity even though it isn't in
the title bar itself.
