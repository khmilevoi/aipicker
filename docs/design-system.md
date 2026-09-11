# AI Picker design system

The application retains its white and lilac visual identity. The shared implementation is `src/theme.rs`; window composition lives in `src/app.rs`, and drawn picker/chart components live in `src/charts.rs`.

## Penpot source

Design work must use Penpot MCP. Inspect the current design before editing, use tokens and components, and synchronize changes with the native implementation. If MCP is unavailable, explicitly record that synchronization is pending rather than claiming it is complete.

[Current design-system page in Penpot](http://pihost.local:8004/#/workspace?team-id=3573d248-7077-8184-8008-9e629aec5db3&project-id=3573d248-7077-8184-8008-9e629e8808fc&file-id=3573d248-7077-8184-8008-9e62a6560153&page-id=11f1cc3d-1d94-80cf-8008-9e97db78205c)

On 2026-09-10 the existing Penpot file was inspected through MCP. A separate design-system page was created, preserving the six original mockups as reference. The new page contains 31 shared tokens, nine reusable components, and six boards: design system, expanded picker, benchmarks, settings, compact widget, and filters. The palette below was synchronized with the code. Screen mockups describe composition; native dimensions adapt to available window space.

## Palette

| Token | Hex | Use |
|---|---|---|
| `CANVAS` | `#FFFFFF` | Window and clear backgrounds |
| `SURFACE` | `#F8F6FC` | Cards, fields, resting controls |
| `ACCENT` | `#9970DE` | Picker, selected outlines |
| `ACCENT_STRONG` | `#825EC0` | Primary actions, links, OpenAI points |
| `ACCENT_SOFT` | `#EFE7FC` | Selected and pressed fills |
| `HOVER` | `#F2EBFB` | Hover fills |
| `INK` | `#2A2634` | Main text |
| `MUTED` | `#777181` | Secondary text |
| `BORDER` | `#E7E2F0` | Cards, separators, controls |
| `WARNING` | `#9D5B27` | Demo, stale data, errors |
| `CORAL` | `#CE8662` | Anthropic points |
| `PICKER_FILL` | `#E5D7F9` | Shared lilac rail and two-dimensional field |
| `PICKER_DOT` | `#B6A1D7` | Resting model stops |

Provider colors distinguish data, while selection uses the same lilac interaction language throughout. Color is accompanied by a label, ring, value, or state change.

## Type, spacing, and shape

- Proportional type scale: 11 px captions, 12 px descriptions, 14 px body, 18 px section headings, 27 px the balance value. Button labels use 13 px for compact Russian labels.
- Spacing scale: 4, 8, 12, 16, 24 px. Cards have 16 px padding. The window uses 14 px outer padding to preserve the compact widget dimensions.
- Corner radii: 8 px controls, 12 px cards and popups, 18 px window.
- Picker field radius: 22 px; thumb radius: 16.5 px; field inset: 28 px. Release settles to the nearest point over 320 ms with cubic easing.
- Action buttons and segmented tabs: 32 px minimum height. Search and secret fields use 10 × 8 px inner padding. Tree rows remain compact and use a common checkbox style.
- The 420 × 148 compact widget uses a denser 5 px vertical gap and 18 px minimum text-row height. Its title remains 30 px high and the rail remains 42 px high.
- Hover, active, open, and inactive egui states all share the same borders, type colors, and radii. Hover does not enlarge a control or shift adjacent layout.

## Components and composition

`theme::button`, `primary_button`, `segment`, `section`, and `card` are the shared primitives. Standard egui fields, menus, checkboxes, sliders, tooltips, and scrollbars inherit the application theme rather than the default gray palette. The top-level window controls remain custom drawn icons.

Expanding the compact widget opens **Пикер**. The main view contains the two-dimensional picker on the left (approximately 60% of the body width) and a selected-model card on the right (40%, minimum 280 px), separated by 16 px. Both metric and price selectors control the axes. The point follows the pointer in both dimensions; release chooses the nearest plotted model in screen space and smoothly settles on its point. Selection changes remain available through keyboard navigation and the model selector.

Picker height follows the visible clipping bounds, keeping the thumb and axis labels within the minimum 780 × 580 window. The blended-price input share is grouped into the same toolbar row. Model details use compact 22 px rows, 12 px labels, and right-aligned values so all six metrics fit. The task-cost explanation is available on the methodology link tooltip.

**Бенчмарки** contains the scatter map and benchmark ranking together, in two equally sized columns, with model details below. Metric, price basis, and sort controls share the same toolbar language. All enabled reasoning variants remain available for comparison.

**Настройки** uses a shared card for data connection and balance preferences. The optional model filter uses the same card, search field, selection controls, and tree styling.

## Verification

Behavioral egui tests cover reopening to the picker, separating picker and benchmark content, nearest-model release and animation, narrow/wide picker behavior, filter tree selection, and compact content fitting within 420 × 148. The source of truth for interaction is the running egui implementation; Penpot documents the design and reusable visual language.
