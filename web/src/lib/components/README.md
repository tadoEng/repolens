# Components

```
components/
├── primitives/   Button · StatusChip · SeverityBadge · ConfidenceBadge · CopyableSha
│                 Disclosure · ExternalCommitLink · ScrollRegion
├── submit/       AuthGate · SubmitErrorSummary
├── analysis/     AnalysisIdentityHeader · ProgressTimeline · FailureNotice · RetryNotice
└── report/       ReportHeader · ReportNav · ReportSection · OverviewSection
                  CategoryFindings · FindingsIndex · FindingCard · EvidenceExpander
                  EvidenceItem · LimitationsList · CompositionSection · MetricTable
                  EvidenceAppendix
```

Everything here is props-driven and contract-typed. No component fetches; the two routes own
the transport (`$lib/api/analysis`) and hand down DTOs imported from `@repolens/api-client`.

## The report's shape

Eight sections, defined once in `report/sections.ts` and consumed by both the nav and the
route. Four of them — Technology, Architecture, Engineering system, Maintenance — are built
by grouping the server-ordered findings on `FindingCategory`. That mapping is annotated
`Readonly<Record<FindingCategory, …>>`, so a category added to the contract fails
`pnpm -r check` instead of quietly disappearing from the hierarchy.

**Every finding is a full card exactly once**, in the section it reads under, so no
`finding-…` anchor is duplicated. `FindingsIndex` is an index over those cards, not a second
copy of them — except for a finding whose category this build has never seen, which has no
section to read under and is rendered there in full rather than dropped.

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
- **Nothing here starts a retry, and `RetryNotice` says why.** Retry is an authenticated
  mutation that starts paid work, and the contract defines no operation for it — no generated
  method, no error schema, no Firebase credential (#13), no idempotency semantics. A
  hand-written `POST` would supply none of those and would fail by _succeeding twice_. The
  affordance is therefore withheld with the reason on screen, never silently dropped, and
  `e2e/analysis.spec.ts` asserts on the wire that no non-`GET` leaves either route.
- **What is said about retry still follows `retry.allowed` alone.** Never the state name —
  `FAILED_RETRIABLE` describes the kind of failure, not whether the server would accept
  another attempt. "The server refused" and "this build cannot ask yet" stay separate
  sentences, because they are separate facts.
- **Exactly two composition views draw bars**, and they are the two comparative ones: code by
  language and code by area. Production/test/generated and largest-files are tables, because
  a single composition of a known whole is stated exactly by a share column and only
  approximated by a bar. Asserted in both the browser and end-to-end suites.
- **LOC bars are `::before` backgrounds set through CSSOM.** Two constraints at once: a bar
  that _contains_ the number clips a 0.4% language's own label, and an inline `style`
  attribute is blocked by our `style-src 'self'` policy (via `style-src-attr`), which would
  leave every bar at zero width in production only. `e2e/report.spec.ts` asserts both.
- **`CodeRole` is rendered wherever a file is listed.** A 1,980-line generated client at the
  top of the largest-files list is not the same fact as a 1,980-line hand-written module, and
  omitting the column is the most common way that list misleads. Every role is styled
  neutrally: a role is structural evidence, not a verdict.
- **Anchor targets carry `tabindex="-1"` and links move focus.** Clicking an in-page link
  scrolls but leaves focus at the document root in every major browser, so a keyboard user
  Tabs from the top again. `focusAnchor` is the single implementation.
- **`ScrollRegion` is deliberately focusable.** axe's `scrollable-region-focusable` and
  Svelte's `a11y_no_noninteractive_tabindex` genuinely conflict here; scrolling is the
  interaction, and content a keyboard user cannot reach is the more serious failure.
- **The progress timeline states every step in words.** The active step animates, but the
  animation is redundant decoration — under `prefers-reduced-motion` the label still says
  "In progress".
- **`AnalysisState` is partitioned, not listed.** `$lib/contract/enums` holds a total
  `Record<AnalysisState, …>` splitting every state into a numbered step or a terminal
  outcome, and `ANALYSIS_STEPS` is derived from it. A `satisfies`-checked array proved only
  that the values it listed were valid — never that every state appeared exactly once, which
  is how a new pipeline stage stalls a timeline that still compiles.
- **A failure moves focus to its own heading.** The route resolves asynchronously, so without
  it a keyboard user is left on `<body>` while the failure notice renders below the fold.
  Once per analysis, and only when nothing else already holds focus.
- **`AuthGate` renders four states, and `unknown` is one of them.** Firebase restores a
  session asynchronously, so "not yet known" is not "signed out": collapsing the two flashes
  a sign-in button at somebody who is already signed in, on every page load. `unknown` gets a
  placeholder and no control; the gate reserves a minimum height so the resolved state does
  not shove the form down the page. `unavailable` is likewise **not an error** — a deployment
  with no Firebase project is a read-only demo, and only creation is closed.
- **`SubmitErrorSummary` keeps `rejected` and `unreachable` apart.** A refusal carries a
  status, a code and the server's own sentence; a transport failure carries none of those and
  must never be worded as a missing or invalid repository. It renders the server's `message`
  verbatim rather than paraphrasing — the API is the only party that knows why it refused —
  and takes its code label from `@repolens/api-client`, never from a table of its own.
- **`Button` defaults to `type="button"`, and `submit` is opt-in.** The HTML default is
  `submit`, which makes every unlabelled button inside a `<form>` a submit button. Only the
  form's one real submit control passes `type="submit"`; nothing acquires it by placement.

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

## Visual baselines

`e2e/visual.spec.ts` holds four `toHaveScreenshot()` captures — the completed report at 1280
and 360, a retriable failure, and a report with no line counts. Deliberately four and not
forty: this design is carried by typography, spacing and border weight, which no assertion
catches, and a baseline per permutation produces noise that gets approved without being read.

Baselines are per platform, because Chromium rasterises text differently on each. A platform
with none **skips and names the command**; `playwright.config.ts` pins `updateSnapshots` to
`none` so an ordinary run can never write one — a first run on a new platform would otherwise
approve whatever it happened to render, regression included.

```
pnpm --filter @repolens/web exec playwright test visual.spec.ts --update-snapshots
```

## The submit form

Built now that both blockers have landed: the Firebase gate (#13) and
`POST /api/v1/analyses` in the generated contract (#6). It is deliberately **not** the
four-component shape the plan sketched (`RepositorySubmitForm → UrlField · ScopeNote ·
AuthGate · SubmitErrorSummary`). The route owns the form, the field and the scope note,
because the field is one `<label>` and one `<input>` and the note is one sentence — wrapping
either in a component buys indirection and no seam. The two pieces with real state to render
are components, for the usual reason: `AuthGate` and `SubmitErrorSummary` cover states that
are awkward to reach through a network and trivial to reach through a prop.

The route, not a component, performs the transport — `components/` never does.

## Still to build

Retry returns with a generated, authenticated operation carrying request, response and error
schemas, declared idempotency semantics, an MSW handler, and a browser test that clicks it
and proves focus lands somewhere deterministic afterwards. Creation now has the credential;
retry still has no operation in the contract.
