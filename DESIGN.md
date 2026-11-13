# MacType Control Center Design Contract

## 1. Atmosphere and signature

This interface is a precise Windows control surface for people who care about text rendering. It should feel engineered, calm, and inspectable: dense rows, hairline separators, stable alignment, and one cyan-blue accent used only for selection and action. It must not resemble a marketing dashboard. Cards, gradients, decorative statistics, oversized headings, and ornamental status dots are prohibited.

Design dials: `DESIGN_VARIANCE=4`, `MOTION_INTENSITY=3`, `VISUAL_DENSITY=8`.

## 2. Color

All UI colors are declared here and mirrored as CSS custom properties.

| Token | Light | Dark | Role |
| --- | --- | --- | --- |
| `--color-canvas` | `#F3F5F7` | `#11161B` | window background |
| `--color-surface` | `#FFFFFF` | `#192027` | primary work surface |
| `--color-surface-subtle` | `#E9EDF1` | `#222B33` | selected/secondary surface |
| `--color-foreground` | `#17212B` | `#E8EDF2` | primary text |
| `--color-muted` | `#5A6773` | `#9AA8B5` | secondary text |
| `--color-border` | `#C9D1D8` | `#34414C` | separators and controls |
| `--color-border-strong` | `#8C99A5` | `#566673` | emphasized control edge |
| `--color-primary` | `#0067C0` | `#4CA6E8` | selection and primary action |
| `--color-on-primary` | `#FFFFFF` | `#07131C` | text on primary |
| `--color-focus` | `#005FB8` | `#75BDF0` | keyboard focus ring |
| `--color-success` | `#18794E` | `#51B88B` | verified state |
| `--color-warning` | `#9A6700` | `#E4B84D` | attention state |
| `--color-destructive` | `#C42B1C` | `#FF7B6E` | failure and destructive action |
| `--color-preview` | `#EEF1F4` | `#0D1115` | text preview canvas |

Primary text against canvas and surface meets WCAG AA. The accent is not used as body text on the canvas.

## 3. Typography

Use the native Windows UI stack: `"Segoe UI Variable Text", "Segoe UI", sans-serif`. It is deliberate because this is a Windows-only system utility and must match platform metrics without downloading fonts.

| Token | Recipe | Role |
| --- | --- | --- |
| `--type-title` | 24px / 600 / 1.25 / -0.2px | view title |
| `--type-section` | 16px / 600 / 1.4 / 0 | section title |
| `--type-body` | 14px / 400 / 1.5 / 0 | body and controls |
| `--type-label` | 13px / 600 / 1.35 / 0 | labels |
| `--type-caption` | 12px / 400 / 1.4 / 0.1px | metadata |
| `--type-mono` | 12px / 400 / 1.5 / 0 | versions and paths, Consolas stack |

## 4. Spacing

Base unit is 4px. Allowed tokens are `--space-1: 4px`, `--space-2: 8px`, `--space-3: 12px`, `--space-4: 16px`, `--space-5: 20px`, `--space-6: 24px`, `--space-8: 32px`, and `--space-10: 40px`. Zero and 1px hairlines are allowed. Layout dimensions use named component tokens, never unexplained component-local values.

## 5. Components

- Window shell: 220px navigation rail, 1px divider, flexible work area, 360px optional inspector at desktop width.
- Navigation item: 36px high, 8px horizontal padding, 4px radius. Hover uses subtle surface, selected uses subtle surface plus a 3px primary inset marker, focus uses a 2px focus outline.
- Button: 32px minimum height, 12px horizontal padding, 4px radius, 1px border. Primary uses primary/on-primary. Disabled opacity is `--opacity-disabled: 0.46`.
- Field: 32px minimum height, 8px horizontal padding, 4px radius, 1px border. Focus uses a 2px focus outline and retains the border.
- Setting row: no surrounding card. Rows use 12px vertical padding and a bottom hairline. Label and help occupy the flexible column; control uses a stable 220px column.
- Status band: flat bordered row with state icon, concise message, and one action. It is never a decorative metric card.
- Preview: 1px border, 4px radius, fixed backing-pixel-aware canvas. Never scale a preview image with CSS transforms.

## 6. Motion

Use `--motion-fast: 120ms` and `--motion-normal: 180ms` with `--ease-standard: cubic-bezier(0.2, 0, 0, 1)`. Animate only opacity, transform, and filter. View changes use a 4px vertical translate plus opacity. `prefers-reduced-motion: reduce` removes transforms and sets durations to 1ms.

## 7. Depth

Depth is borders plus tonal shifts. `--shadow-window: 0 8px 24px rgba(15, 23, 31, 0.14)` is reserved for detached native windows and must not appear on in-window sections. No other shadows are permitted.

## Do and do not

- Do preserve information density and keyboard reachability.
- Do show defaults, current values, dirty state, and apply requirements as text.
- Do use one icon family and the existing MacType product icon.
- Do not use gradients, glass, bento layouts, marketing copy, pills over images, or fake statistics.
- Do not use em dashes or decorative uppercase eyebrow labels in visible copy.
