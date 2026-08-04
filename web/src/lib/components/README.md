# Components

Nothing is built here yet. The report, progress, and submit surfaces are blocked on the
`analysis-v1` / `report-v1` executable fixtures (issue #14), and a component built against a
guessed DTO is a component that has to be rewritten.

Planned layout, from the frontend plan (§3.6):

```
components/
├── primitives/   Button · StatusChip · CopyableSha · Disclosure
├── analysis/     AnalysisIdentityHeader · ProgressTimeline · FailureNotice
└── report/       ReportHeader · ReportNav · FindingCard · EvidenceDisclosure · …
```

## The Bits UI rule

`bits-ui` is installed at the foundation, on purpose. It is cheap insurance: the expensive
failure mode is hand-rolling an inaccessible custom widget later, under time pressure, when a
listbox or a focus trap turns out to be needed.

**Bits UI does not displace native elements.** For the easy cases native is genuinely less
code, not more, and it is correct by default rather than correct by our implementation.

| Reach for Bits UI                                    | Keep native                                             |
| ---------------------------------------------------- | ------------------------------------------------------- |
| Select / Combobox (the findings filter)              | `<input>`, `<button>`, `<a>`                            |
| Dialog, Popover                                      | `<table>` for the LOC and largest-files views           |
| Collapsible **only where animation is wanted**       | `<details>` / `<summary>` for plain evidence disclosure |
| Tooltip — supplementary labels on icon-only controls | `<select>` for a simple single-choice filter            |

Bits UI is an implementation detail _inside_ a primitive. It is never the design system:
`tokens.css` stays authoritative, and Bits UI is chosen partly because it ships no CSS and no
visual opinion to fight.

## Constraints that bind before the first component exists

- **Tooltips never carry limitations.** Limitations are first-class visible information;
  tooltips are transient, absent on touch, and subject to WCAG 1.4.13. Limitations render
  inline or in a disclosure — never behind hover.
- **Retry needs no confirmation dialog.** Confirmations are for destructive actions. Retry is
  idempotent.
- **No toasts in Phase 0.**
- **Status is never colour alone.** A status chip carries text and shape as well. `MISSING`
  renders neutral grey, not red.
- **`<details>` breaks Ctrl+F.** Content inside a closed `<details>` is not found by browser
  find-in-page in most engines, and people _will_ search a report for a file path. Use
  `hidden="until-found"` where supported, and ship an explicit "Expand all evidence" control —
  which also serves printing and sharing.
- **Anchor navigation must move focus, not just scroll.** Section headings need
  `tabindex="-1"` and explicit focus on navigation, or a keyboard user Tabs from the top of the
  document again. This is the most commonly shipped accessibility bug of its kind.
- **Under `prefers-reduced-motion`, the progress timeline's active step needs a static,
  text-carried indicator.** The state is conveyed by the label, not by the motion.
- **LOC bars are backgrounds, never boxes that size text.** Draw them with
  `.metric-cell::before` and an `inline-size: calc(var(--proportion) * 100%)`, with the real
  number in the `<td>` above it. A language at 0.4% must not get a bar narrower than its own
  label. Add a `@media (forced-colors: active)` rule that drops the bar.
