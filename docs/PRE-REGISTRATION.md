# RepoLens Experimental-v1 — measurement preregistration

This document fixes what will be measured, how, and what each outcome means,
**before any measurement is taken**. It is written and committed while the
result is unknown, which is the only time such a document can be written
honestly.

> **No criterion may be changed because an observed result is inconvenient,
> ambiguous, or inconsistent with the hypothesis.** Missing observability
> produces `NOT EVALUABLE`; insufficient valid windows produce `UNDERPOWERED`;
> neither authorises new instrumentation during Experimental-v1.

Feature development ended at `7f81660`, the merge of #48. The freeze target is
**the commit on `master` that introduces this file** — not the pull-request head
that proposed it. This repository rebases on merge, so those are different SHAs
and only the one on `master` was ever deployable. Its exact value is recorded in
[`DEPLOYMENT-ATTESTATION.md`](DEPLOYMENT-ATTESTATION.md) and carried by the
`experimental-v1` tag. A document cannot contain its own commit SHA, which is
why the attestation — written after the deployment it describes — is where that
identity is pinned. **The tag points at the deployed candidate, not at the
attestation commit that follows it.**

From the moment that tag exists, no application change is permitted for the
duration of measurement.

---

## 1. What is being tested

RepoLens exists to settle an argument about a stack choice: how much of a
request is Axum, and how much is everything else. Issue #9 asked for cold start,
warm latency, analysis duration, GitHub request counts, database connections,
CPU, memory and cost. Experimental-v1 instruments **some** of that and
deliberately not the rest.

This document states which questions the built instrument can answer, which it
cannot, and what answer would count as which.

The expectation on record in issue #37 is that Axum and the deterministic
analyzer are small fractions, and that GitHub, network, database and cold start
dominate. **That is the hypothesis under test, not the conclusion.** A result
contradicting it is the more valuable outcome and will be reported as found.

---

## 2. The instrument, and what it can and cannot report

Everything below is a property of code merged at `7f81660`, quoted rather than
summarised, because the criteria in §3 depend on it exactly.

### 2.1 What the operational snapshot publishes

`GET /api/v1/admin/overview` returns, per normalised route and method class:

```
RouteOverview:      route, method, requests, responses, latency
LatencySummary:     p50, p95, p99, total_micros
LatencyPercentile:  micros, lower_bound_micros, upper_bound_micros
```

and per process:

```
ProcessOverview:    build_sha, uptime_seconds, resident_bytes
HttpOverview:       in_flight, tracked_routes, max_tracked_routes, routes
```

### 2.2 Percentiles cannot be differenced between snapshots

**Histogram bucket counts are not published.** The wire carries percentiles and
their bucket bounds, a request count, and a latency sum — nothing from which a
distribution could be reconstructed. Percentiles do not subtract, so two
snapshots taken around a workload cannot be differenced into a distribution for
that workload alone.

Every internal figure is therefore **cumulative for the life of the process**.
This is the single constraint that shapes the whole protocol in §4: a window is
usable only if the process served the scripted workload and essentially nothing
else on the routes being measured.

### 2.3 Latency buckets, and why the thresholds are what they are

`crates/repolens-server/src/telemetry/metrics.rs`:

```
LATENCY_BUCKET_BOUNDS_MICROS =
  500, 1_000, 2_500, 5_000, 10_000, 25_000, 50_000,
  100_000, 250_000, 500_000, 1_000_000, 2_500_000, 5_000_000, 10_000_000
```

An observation is recorded in the first bucket whose bound satisfies
`micros <= bound`, so a value exactly equal to a bound belongs to the bucket
that bound closes.

`LatencyPercentile.micros` is a **linear interpolation inside one bucket**, and
the contract says so. It is an estimate with a stated resolution, not a
measurement.

**10 ms, 25 ms and 50 ms are exact bucket bounds.** Every H1 threshold is
therefore decidable from `upper_bound_micros` and `lower_bound_micros` alone,
without consulting the interpolation.

> **Locked rule.** Any decision threshold introduced later must be a member of
> `LATENCY_BUCKET_BOUNDS_MICROS`. A threshold chosen off the grid — 20 ms, say —
> silently makes a pass/fail decision depend on an interpolated number the
> contract explicitly declines to call a measurement.

### 2.4 Rank selection

`HistogramSnapshot::percentile` selects by nearest rank:
`rank = ceil(p × n / 100)`, floored at 1. Consequences relied on below:

| n | p95 selects | p99 selects |
| --- | --- | --- |
| 20 | rank 19 — the 2nd slowest | rank 20 — the slowest |
| 21 | rank 20 — warm | rank 21 — **the single cold observation** |
| 100 | rank 95 — the 6th slowest | rank 99 — the 2nd slowest |

The `n = 21` row is why the wake-up request never touches a benchmark route
(§4.3), and the `n = 20` row is why the sample is 100 rather than 20.

### 2.5 What the handler figure includes

Internal latency is measured by middleware around the whole application stack.
For the two benchmark routes it therefore includes the PostgreSQL round trip:

- `GET /api/v1/system/probe` executes `SELECT 1` and then a `max(version)` read
  of `_sqlx_migrations`;
- `GET /api/v1/analyses/{analysis_id}/report` reads through `store::load_report`.

`GET /healthz` performs **no** database work — it returns a fixed body — which is
what makes it usable as a transport baseline and as the wake-up request.

### 2.6 What is not instrumented, by decision

No per-phase analyzer timings. No query latency, pool size, pool wait or error
counts. No CPU accounting. No deploy age. No history or persistence of any
figure. These absences are recorded in the product itself, on `/admin`, as
*Not instrumented in Experimental-v1* — distinct from *Not measured*, which
means a figure exists and this platform cannot produce it.

---

## 3. Hypotheses

### H1 — Warm service-side handling for simple read routes is small

**Renamed from "Axum is not the bottleneck", deliberately.** §2.5 shows the
internal figure covers Axum, the middleware stack, the application handler and
the database round trip together. The name has to match what the number can
support.

**Benchmark routes:** `GET /api/v1/system/probe` and
`GET /api/v1/analyses/{analysis_id}/report`, for one already-completed analysis.

Per benchmark route, per valid window, using bucket bounds only:

| Verdict | Condition |
| --- | --- |
| **PASS** | `p50.upper_bound_micros ≤ 10_000` **and** `p95.upper_bound_micros ≤ 25_000` |
| **FAIL** | `p95.lower_bound_micros ≥ 50_000` |
| **INCONCLUSIVE** | anything else |

`FAIL` uses `lower_bound_micros ≥ 50_000` because exactly 50 ms belongs to the
bucket that bound closes; a selected observation in a bucket whose lower bound
is 50 ms is therefore strictly slower than 50 ms.

**Overall verdict:**

- **CONFIRMED** if both benchmark routes PASS in at least **4 of 5** H1-valid windows.
- **REJECTED** if either benchmark route FAILS in at least **3 of 5** H1-valid windows.
- **INCONCLUSIVE** otherwise.

**No ratio participates in this verdict.** See §3.4.

**Asymmetry of the outcomes, stated in advance.** A pass is the stronger claim:
Axum, its middleware, the handler and the Neon round trip together are below the
threshold, so the framework cannot plausibly be the material cost on these
paths. A failure locates the cost *inside the service-side path* and says
nothing about which layer owns it — Axum, PostgreSQL, pool wait, or something
else. Experimental-v1 has no instrument that could separate them, which is the
same attribution gap that makes H3 not evaluable.

### H2 — Cold start is materially more expensive than warm serving

Measured on `/healthz` only, so no database work and neither benchmark route is
touched.

Per genuine cold window: record the first `/healthz` external latency, then 100
sequential warm `/healthz` external timings. Compute for that window:

```
ratio   = cold / warm_p95
penalty = cold − warm_p95
```

Pairing cold with warm **inside the same window** is deliberate: it compares a
cold request against the network conditions it actually occurred in, rather than
against a pooled baseline from another day.

| Verdict | Condition on the median across the 5 H2-valid windows |
| --- | --- |
| **CONFIRMED** | median ratio ≥ 2.0× **and** median penalty ≥ +500 ms |
| **REJECTED** | median ratio < 1.25× **and** median penalty < +250 ms |
| **INCONCLUSIVE** | anything else |

A first request that fails or never answers is **recorded as a failure**. It is
not replaced by the successful retry, and the retry does not become the cold
observation.

**A failed cold request makes its window not H2-valid.** That window therefore:

- does not count toward the five windows H2 requires;
- contributes to neither median;
- is reported as a failure, with its count stated beside the medians.

Collection continues until **five H2-valid windows with a successful cold
observation** exist, or day 14 arrives. Fewer than five at the cutoff is
`UNDERPOWERED`.

This is stated because the alternative reading is available and wrong: four
failures and one success must never produce a one-observation median wearing the
authority of a five-window verdict.

### H3 — GitHub, network and database dominate deterministic analysis work

**`NOT EVALUABLE` in Experimental-v1.**

There are no per-phase timings: nothing separates repository resolution, tree
retrieval, blob collection, ruleset evaluation, report construction and database
writes. Only a total analysis duration exists.

Total duration will be reported, and its scale may be compared with HTTP serving
latency. **No claim will be made that GitHub, PostgreSQL or the deterministic
analyzer owned any particular share of it.** The missing attribution is itself a
result, and it is reported as one.

### 3.4 The p95 scale ratio — contextual evidence, not a verdict

The internal-to-external comparison is reported but decides nothing, for two
reasons.

**It partly measures distance from the server.** With a large transport
baseline, a slow handler still looks like a small fraction. The measurement
client and location are therefore fixed for the entire decision dataset (§4.1)
and a transport baseline is recorded per window (§4.4).

**The two percentiles need not describe the same request.** The internal p95 is
selected by rank from the server's histogram; the external p95 from the client's
timings. They are two summaries of one workload, not two measurements of one
request — hence *p95 scale ratio*, never "fraction of request time".

Reported as an **interval** from the internal bucket bounds
(`[lower/external, upper/external]`), never as a point derived from the
interpolated `micros`.

---

## 4. Protocol

### 4.1 Fixed for the entire decision dataset

- One measurement client, one machine, one network location. If any of these
  changes, collection restarts; observations are not pooled across locations.
- The deployed artifact is the `experimental-v1` tag and does not change.

### 4.2 The decision dataset is scripted; real usage is supplementary

Ordinary use of the deployed application is recorded as observations and
**never** alters a threshold, the workload, an exclusion, or the stopping rule.
A spectacular 12-second cold request or a 2-millisecond warm one seen while
browsing is an anecdote in the report, not evidence in a verdict.

### 4.3 HTTP windows (H1 and H2)

A **window** begins when a process starts and ends before the next restart. Five
H1-valid **and** five H2-valid windows are required; §4.5 defines the two
conditions, and a single window commonly satisfies both.

1. **Idle** for at least 20 minutes with no inbound traffic to the service.
   Render documents that Free web services spin down after 15 minutes without
   inbound HTTP or WebSocket traffic, and warns that Free services may also
   restart independently. The extra five minutes is margin, not a measurement.
2. **Wake with exactly one `GET /healthz`.** Record its external latency; this
   is the H2 cold observation. `/healthz` is in neither benchmark set, so the
   cold observation cannot contaminate an H1 distribution (§2.4).
3. **Capture the opening snapshot** by calling
   `GET /api/v1/admin/overview` directly with the admin credential.
4. **Verify the window is genuinely cold** — §4.5.
5. **Run the sequential workload**, in this order, no concurrency:
   - 100 × `GET /healthz` — H2 warm baseline and the transport baseline;
   - 100 × `GET /api/v1/system/probe`;
   - 100 × `GET /api/v1/analyses/{id}/report` for one already-completed analysis.
6. **Capture the closing snapshot**, again by calling the endpoint directly.
7. Record every external wall-clock latency separately from the internal figures.

### 4.4 Transport baseline

The external p95 of the 100 warm `/healthz` requests in the same window. It is
called a transport baseline rather than an RTT because it is not a network
round-trip measurement — it is the end-to-end cost of the cheapest request this
service can answer, which is the closest experiment-native equivalent.

### 4.5 Validity conditions, all decided before latency is examined

A window is **valid for H1** only if, for each benchmark route and method:

- the opening snapshot shows **exactly 0** requests, and
- the closing snapshot shows **exactly 100** requests.

The closing condition is not redundant. A clean start followed by `101` means
another caller reached the same normalised route mid-window, and the cumulative
percentile is then drawn from a population that includes traffic we did not
send.

A window is **valid for H2** (genuinely cold) only if the opening snapshot shows:

- `uptime_seconds` less than the preceding idle gap, proving the process started
  during that idle period — uptime is anchored at the top of `main` and is
  monotonic from process start; and
- **exactly one completed `GET /healthz`** and no other completed requests,
  proving the measured `/healthz` was the request that woke this process rather
  than the seventh sent to one somebody else had already warmed.

If that stricter condition leaves fewer than five genuine cold windows, **H2 is
reported `UNDERPOWERED`.** It is not weakened afterwards.

### 4.6 Contamination rules

- **Never use the `/admin` page to capture a snapshot.** Call
  `GET /api/v1/admin/overview` directly. The page fetches the snapshot and
  `GET /api/v1/system/probe` in parallel, so loading it would increment a
  benchmark route and destroy the zero-history condition. Both halves are
  individually correct; the composition changes the experiment.
- **No browser may load any page of the deployed site during a window.**
  `SystemProbe` is mounted in `web/src/routes/+layout.svelte`, which wraps every
  route, so *any* page view calls `/api/v1/system/probe` — the home page, a
  report, anything.
- A window that fails any validity condition in §4.5 is excluded **with the
  reason recorded**, and the reason is always a condition evaluated before its
  latency was looked at.

### 4.7 Analysis dataset (descriptive only)

Six public GitHub repositories at exact commit SHAs, five sequential runs each,
**30 attempts** total. No concurrency: this experiment is about cost
attribution, not throughput saturation.

**Attempts, not completions.** `COMPLETED` is one specific terminal state and
the failures have their own, so a target of "30 completed analyses" would keep
running until thirty *succeeded* — selecting the dataset on the outcome. Exactly
30 are attempted. A failure is data, is never re-run to replace it, and is
reported with its error code. Each attempt is followed to a terminal state or to
the day-14 cutoff; one still running at the cutoff is recorded as still running,
never substituted.

The six repositories are fixed **now**, before any result is seen, spanning
small, medium and large:

| Size | Repository |
| --- | --- |
| small | `BurntSushi/memchr` |
| small | `rust-lang/log` |
| medium | `tokio-rs/axum` |
| medium | `serde-rs/serde` |
| large | `sveltejs/kit` |
| large | `rust-lang/crates.io` |

Their exact commit SHAs are resolved **once**, before the first measured
analysis, and recorded in `DEPLOYMENT-ATTESTATION.md`. A repository is never
substituted and a SHA is never re-resolved, whatever its numbers turn out to be.

Recorded per analysis: repository, commit SHA, total duration, terminal state,
error code where applicable, and the limitations the report carries. **Not**
recorded, because it does not exist: any breakdown of where that duration went.

---

## 5. Stopping rule

Collection stops at whichever comes first:

- **all 30 fixed analysis attempts have reached a terminal state** (five per
  repository), **and** 5 windows valid for H1 exist, **and** 5 windows valid for
  H2 with a successful cold observation exist; or
- 14 calendar days from the first measurement.

**Terminal state, not attempt count.** "30 attempts" is satisfied the moment the
thirtieth *starts*, which would let collection stop with an analysis still
running — contradicting §4.7, which requires every attempt to be followed to a
terminal state or to the cutoff. Terminal includes every failure state, not only
`COMPLETED`, so this does not reintroduce the success selection §4.7 removed. An
attempt that hangs simply carries the experiment to day 14, where it is recorded
as still running.

The two window counts are stated separately because §4.5 defines the two
validity conditions separately. One window usually satisfies both, and a window
that satisfies only one counts only toward that one.

**Collection is never extended because a result is ambiguous.** If the minimum
sample is not reached by day 14, the affected hypothesis is reported
`UNDERPOWERED`, with the counts actually obtained. Each hypothesis is judged on
its own sample: H1 may be decided while H2 is underpowered, or the reverse.

---

## 6. What would falsify the engineering thesis

Two distinct failures, both worth reporting.

**Performance.** Falsification occurs **exactly when H1 is `REJECTED` under the
rule in §3** — either benchmark route showing `p95.lower_bound_micros ≥ 50_000`
in at least three of five H1-valid windows.

There is no second, softer route to this conclusion. **The p95 scale ratio can
neither confirm nor reject H1**, and no impression that handling "consumes a
large share of external latency" may stand in for the preregistered rule. One
authority, fixed in advance, or the criterion is whatever the result makes
convenient.

Should it happen, the application request path is materially in the critical
path and "the framework layer is negligible" is wrong — though, per H1's
asymmetry, still without saying which layer owns it.

**Observability.** If measurement finishes and the question that most matters
turns out to be *where analysis time went*, and Experimental-v1 cannot answer it
because instrumentation stopped at total duration, the report says so plainly:

> The architecture may be sound, but the observability chosen was insufficient
> to test the analyzer-dominance hypothesis.

That is a finding, not a reason to retroactively instrument. It is a lesson for
the next product.

---

## 7. Outputs

| File | Contents |
| --- | --- |
| `requests.csv` | one row per HTTP observation: window, route, external latency, and the window's internal bucket bounds |
| `analyses.csv` | one row per analysis: repository, commit SHA, duration, terminal state, limitations |
| `EXPERIMENT-REPORT.md` | the verdicts, with every exclusion and its pre-decided reason |
| `LESSONS-LEARNED.md` | what was reusable engineering and what was experiment-specific tax |
| `STACK-DECISION.md` | what this does and does not settle about the stack |

`requests.csv` is a series of snapshots rather than a continuous record, because
the counters are per process and reset on restart. That is a property of the
instrument, stated here rather than discovered during analysis.

---

## 8. Signed before measurement

The criteria above were fixed while every result was unknown. Any deviation
discovered during measurement is recorded in `EXPERIMENT-REPORT.md` as a
deviation, with its reason, rather than applied silently to the criteria.
