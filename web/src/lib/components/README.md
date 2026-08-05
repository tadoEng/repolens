# Components

```
components/
├── primitives/   Button · StatusChip · SeverityBadge · ConfidenceBadge · CopyableSha
│                 Disclosure · ExternalCommitLink · ScrollRegion
├── analysis/     AnalysisIdentityHeader · ProgressTimeline · FailureNotice · RetryControl
└── report/       ReportHeader · ReportNav · ReportSection · OverviewSection
                  FindingsSection · FindingCard · EvidenceItem · LimitationsList
                  CompositionSection · MetricTable · EvidenceAppendix
```

Everything here is props-driven and contract-typed. No component fetches; the two routes own
the transport (`$lib/api/analysis`) and hand down DTOs imported from `@repolens/api-client`.

## Rules that are load-bearing, not stylistic

Each of these exists because the obvious alternative ships a specific defect.

- **`SeverityBadge` and `ConfidenceBadge` are two components and must stay two.** Severity is
  impact if the finding is valid; confidence is the strength of the evidence. Merging them —
  even behind an `axis` prop — is one refactor away from a merged badge, and a merged badge is
  how a low-confidence guess about something important becomes indistinguishable from a
  measurement. Both render the axis name on screen for the same reason.
- **`MISSING` is neutral grey.** Absence is not failure. If absence matters, the rule's own
  explanation says why; the chip does not get to decide. `StatusChip` also varies border
  _style_, so the states stay distinguishable in greyscale and forced colours.
- **Unknown enum values render, never crash and never disappear.** Labels come from
  `@repolens/api-client`, which fails `pnpm -r check` when a variant is added without
  handling. `$lib/contract/enums` adds only a styling token, collapsing everything
  unrecognised to one neutral slug.
- **Limitations are inline text, never a tooltip.** Tooltips do not exist on touch, are
  transient, and are subject to WCAG 1.4.13. A limitation is the sentence that stops "no
  architecture document was found" being read as "this project has no architecture".
- **Retry is driven by `retry.allowed` alone.** Never by the state name — `FAILED_RETRIABLE`
  describes the kind of failure, not whether the server would accept another attempt. No
  confirmation dialog: confirmations are for destructive actions and retry is idempotent.
- **LOC bars are `::before` backgrounds set through CSSOM.** Two constraints at once: a bar
  that _contains_ the number clips a 0.4% language's own label, and an inline `style`
  attribute is blocked by our `style-src 'self'` policy (via `style-src-attr`), which would
  leave every bar at zero width in production only. `e2e/report.spec.ts` asserts both.
- **Anchor targets carry `tabindex="-1"` and links move focus.** Clicking an in-page link
  scrolls but leaves focus at the document root in every major browser, so a keyboard user
  Tabs from the top again. `focusAnchor` is the single implementation.
- **`ScrollRegion` is deliberately focusable.** axe's `scrollable-region-focusable` and
  Svelte's `a11y_no_noninteractive_tabindex` genuinely conflict here; scrolling is the
  interaction, and content a keyboard user cannot reach is the more serious failure.
- **The progress timeline states every step in words.** The active step animates, but the
  animation is redundant decoration — under `prefers-reduced-motion` the label still says
  "In progress".

## The Bits UI rule

`bits-ui` is installed at the foundation, on purpose. It is cheap insurance: the expensive
failure mode is hand-rolling an inaccessible custom widget later, under time pressure, when a
listbox or a focus trap turns out to be needed.

**Bits UI does not displace native elements.** For the easy cases native is genuinely less
code, not more, and it is correct by default rather than correct by our implementation.
Nothing in the list above needed it yet: the disclosures are `<details>`, the charts are
`<table>`, the controls are `<button>` and `<a>`.

| Reach for Bits UI                                    | Keep native                                             |
| ---------------------------------------------------- | ------------------------------------------------------- |
| Select / Combobox (the findings filter)              | `<input>`, `<button>`, `<a>`                            |
| Dialog, Popover                                      | `<table>` for the LOC and largest-files views           |
| Collapsible **only where animation is wanted**       | `<details>` / `<summary>` for plain evidence disclosure |
| Tooltip — supplementary labels on icon-only controls | `<select>` for a simple single-choice filter            |

Bits UI is an implementation detail _inside_ a primitive. It is never the design system:
`tokens.css` stays authoritative, and Bits UI is chosen partly because it ships no CSS and no
visual opinion to fight.

## Still to build

The submit form (`RepositorySubmitForm → UrlField · ScopeNote · AuthGate ·
SubmitErrorSummary`) is blocked on the Firebase auth gate (#13) and on the
analysis-creation request contract, neither of which exists yet.
