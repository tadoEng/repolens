import {
	COMPLETED_REPORT_FIXTURE,
	FAILED_PERMANENT_FIXTURE,
	FAILED_RETRIABLE_FIXTURE,
	QUEUED_FIXTURE,
	RESOLVING_FIXTURE
} from '@repolens/api-client';
import { tick } from 'svelte';
import { expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import AnalysisIdentityHeader from '$lib/components/analysis/AnalysisIdentityHeader.svelte';
import FailureNotice from '$lib/components/analysis/FailureNotice.svelte';
import ProgressTimeline from '$lib/components/analysis/ProgressTimeline.svelte';
import RetryControl from '$lib/components/analysis/RetryControl.svelte';
import '$lib/styles/global.css';

/**
 * The progress surface, driven by the executable fixtures.
 *
 * Every scenario here is one the contract deliberately made expressible and a hand-clicked
 * check never sees: a null commit SHA, a failure that must not offer a retry, and an enum
 * value from a newer backend.
 */

/** Let Svelte flush, then let the MutationObserver callbacks run. */
async function settle(): Promise<void> {
	await tick();
	await new Promise((resolve) => setTimeout(resolve, 0));
}

test('a null commit SHA renders "resolving…", never a blank', async () => {
	const analysis = QUEUED_FIXTURE.analysis;
	// Guards the fixture: a resolved SHA here would make the assertion below vacuous.
	expect(analysis.commit_sha).toBeNull();

	const screen = await render(AnalysisIdentityHeader, { props: { analysis } });

	const commit = screen.container.querySelector('dd') as HTMLElement;
	expect(commit.textContent?.trim()).toBe('resolving…');
	// Repository identity is known from creation, so the header paints immediately.
	expect(screen.container.textContent).toContain('rust-lang/crates.io');
	// Nothing pretends to be a commit yet.
	expect(screen.container.querySelector('a[href*="/commit/"]')).toBeNull();
});

test('a resolved commit SHA is shown short and linked to GitHub', async () => {
	const analysis = COMPLETED_REPORT_FIXTURE.analysis;
	const screen = await render(AnalysisIdentityHeader, { props: { analysis } });

	expect(screen.container.textContent).toContain(analysis.commit_sha?.slice(0, 7) ?? '');
	expect(screen.container.querySelector('a[href*="/commit/"]')).not.toBeNull();
});

test('QUEUED puts the analysis on step 1 of 5 with the rest not started', async () => {
	const screen = await render(ProgressTimeline, { props: { state: 'QUEUED' } });

	const steps = [...screen.container.querySelectorAll('li')];
	expect(steps).toHaveLength(5);

	// The status is in words on every step. That is what survives `prefers-reduced-motion`,
	// greyscale and forced colours — the animation is redundant decoration on top of it.
	expect(steps[0]?.textContent).toContain('In progress');
	expect(steps[1]?.textContent).toContain('Not started');
	expect(steps[4]?.textContent).toContain('Not started');

	expect(screen.container.querySelector('[aria-live="polite"]')?.textContent).toContain(
		'Step 1 of 5'
	);
});

test('RESOLVING marks the earlier step done and follows the contract order', async () => {
	const screen = await render(ProgressTimeline, {
		props: { state: RESOLVING_FIXTURE.analysis.state }
	});

	const steps = [...screen.container.querySelectorAll('li')];
	expect(steps[0]?.textContent).toContain('Done');
	expect(steps[1]?.textContent).toContain('In progress');
	expect(steps[1]?.textContent).toContain('Resolving commit');
	expect(steps[2]?.textContent).toContain('Not started');
});

test('COMPLETED marks every step done and announces the report', async () => {
	const screen = await render(ProgressTimeline, { props: { state: 'COMPLETED' } });

	const steps = [...screen.container.querySelectorAll('li')];
	expect(steps.every((step) => step.textContent?.includes('Done'))).toBe(true);
	expect(screen.container.querySelector('[aria-live="polite"]')?.textContent).toContain(
		'Analysis completed'
	);
});

test('a failure claims no step, because the contract does not report one', async () => {
	const screen = await render(ProgressTimeline, {
		props: { state: FAILED_PERMANENT_FIXTURE.analysis.state }
	});

	const steps = [...screen.container.querySelectorAll('li')];
	// Marking a step would be an invention: neither failure state says how far it got.
	expect(steps.every((step) => step.textContent?.includes('Not reported'))).toBe(true);
	// Normalised, because the sentence a reader sees is not the whitespace the markup has.
	const prose = (screen.container.textContent ?? '').replace(/\s+/g, ' ');
	expect(prose).toContain('does not report which step it reached');
});

test('an unrecognised analysis state neither crashes nor is silently dropped', async () => {
	const screen = await render(ProgressTimeline, { props: { state: 'VALIDATING_SIGNATURES' } });

	// Rendered, named, and explicitly not placed on the timeline.
	expect(screen.container.textContent).toContain('VALIDATING_SIGNATURES');
	expect(screen.container.querySelectorAll('li')).toHaveLength(5);
	expect(screen.container.querySelector('[aria-live="polite"]')?.textContent).toContain(
		'does not recognise'
	);
});

test('the live region is polite and stays silent when a poll changes nothing', async () => {
	const screen = await render(ProgressTimeline, { props: { state: 'COLLECTING' } });

	const live = screen.container.querySelector('[aria-live="polite"]') as HTMLElement;
	// `polite`, not `assertive`: progress is not worth interrupting someone mid-sentence.
	expect(live.getAttribute('aria-live')).toBe('polite');

	const mutations: MutationRecord[] = [];
	const observer = new MutationObserver((records) => mutations.push(...records));
	observer.observe(live, { childList: true, characterData: true, subtree: true });

	// A poll that returns the same state. Polling replaces the analysis object every tick,
	// so without a value-level guard this is where a screen reader gets spammed.
	await screen.rerender({ state: 'COLLECTING' });
	await settle();
	expect(mutations, 'an unchanged poll must not re-announce').toHaveLength(0);

	// A real transition, however, must be announced.
	await screen.rerender({ state: 'ANALYZING' });
	await settle();
	expect(mutations.length).toBeGreaterThan(0);
	expect(live.textContent).toContain('Step 4 of 5');

	observer.disconnect();
});

test('retry is offered when the server allows it, and calls back on click', async () => {
	const analysis = FAILED_RETRIABLE_FIXTURE.analysis;
	expect(analysis.retry.allowed).toBe(true);

	const onRetry = vi.fn();
	const screen = await render(RetryControl, { props: { retry: analysis.retry, onRetry } });

	const button = screen.getByRole('button', { name: 'Retry this analysis' });
	await expect.element(button).toBeInTheDocument();

	// No confirmation dialog: confirmations are for destructive actions, and retry is
	// idempotent.
	await button.click();
	expect(onRetry).toHaveBeenCalledTimes(1);
});

test('FAILED_PERMANENT offers no retry and shows the server reason verbatim', async () => {
	const analysis = FAILED_PERMANENT_FIXTURE.analysis;
	expect(analysis.retry.allowed).toBe(false);

	const screen = await render(RetryControl, {
		props: { retry: analysis.retry, onRetry: vi.fn() }
	});

	expect(screen.container.querySelector('button')).toBeNull();
	// Displayed verbatim, because it explains rather than merely denies.
	expect(screen.container.textContent).toContain(analysis.retry.reason ?? '');
});

test('retry follows retry.allowed, never the state name', async () => {
	/*
	 * The defect this rules out: a frontend that renders a retry button because the state
	 * is called `FAILED_RETRIABLE`. Whether a retry is accepted also depends on attempts
	 * already spent and whether the work is still claimable — facts only the server holds.
	 *
	 * So: the retriable failure's own error, paired with a policy that refuses. A UI keyed
	 * off the state name shows a button here and it does nothing.
	 */
	const screen = await render(FailureNotice, {
		props: {
			error: FAILED_RETRIABLE_FIXTURE.analysis.error,
			retry: FAILED_PERMANENT_FIXTURE.analysis.retry,
			onRetry: vi.fn()
		}
	});

	expect(screen.container.querySelector('button')).toBeNull();
	expect(screen.container.textContent).toContain('Retry is not available');
	// The failure itself is still fully described.
	expect(screen.container.textContent).toContain('RATE_LIMITED');
});

test('a supplied retry-after is rendered as words, and an absent one is not invented', async () => {
	const withWait = await render(FailureNotice, {
		props: {
			error: FAILED_RETRIABLE_FIXTURE.analysis.error,
			retry: FAILED_RETRIABLE_FIXTURE.analysis.retry,
			onRetry: vi.fn()
		}
	});
	expect(FAILED_RETRIABLE_FIXTURE.analysis.error?.retry_after_seconds).toBe(900);
	expect(withWait.container.textContent).toContain('15 minutes');

	const withoutWait = await render(FailureNotice, {
		props: {
			error: FAILED_PERMANENT_FIXTURE.analysis.error,
			retry: FAILED_PERMANENT_FIXTURE.analysis.retry,
			onRetry: vi.fn()
		}
	});
	// Absent rather than zero when unknown: "retry in 0s" from a missing value is worse
	// than no countdown at all.
	expect(withoutWait.container.textContent).not.toContain('wait of about');
});
