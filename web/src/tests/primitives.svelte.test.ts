import { createRawSnippet } from 'svelte';
import { expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';

import ConfidenceBadge from '$lib/components/primitives/ConfidenceBadge.svelte';
import CopyableSha from '$lib/components/primitives/CopyableSha.svelte';
import Disclosure from '$lib/components/primitives/Disclosure.svelte';
import SeverityBadge from '$lib/components/primitives/SeverityBadge.svelte';
import StatusChip from '$lib/components/primitives/StatusChip.svelte';
import '$lib/styles/global.css';

/**
 * The shared primitives, in a real browser.
 *
 * Browser mode rather than jsdom is not a preference here: three of these tests read
 * *computed* styles and one reads `document.activeElement`. jsdom computes neither
 * faithfully, so a green jsdom run would assert nothing about the two rules these
 * components exist to enforce — that `MISSING` is neutral, and that severity and confidence
 * are never one badge.
 */

/** Parse a computed `rgb(...)` / `rgba(...)` colour into channels. */
function channels(colour: string): [number, number, number] {
	const parts = colour.match(/\d+(\.\d+)?/g) ?? [];
	return [Number(parts[0] ?? 0), Number(parts[1] ?? 0), Number(parts[2] ?? 0)];
}

/** Whether a colour is neutral: no channel meaningfully dominates another. */
function isNeutral(colour: string): boolean {
	const [red, green, blue] = channels(colour);
	const spread = Math.max(red, green, blue) - Math.min(red, green, blue);
	return spread <= 12;
}

test('MISSING renders neutral grey, never as an error', async () => {
	const screen = await render(StatusChip, { props: { state: 'MISSING' } });

	const chip = screen.container.querySelector('[data-state="MISSING"]');
	expect(chip).not.toBeNull();
	// The word carries the meaning; colour is decoration. Assert both.
	expect(chip?.textContent?.trim()).toBe('Missing');

	const style = getComputedStyle(chip as Element);

	/*
	 * The rule this guards: absence is not failure. A red chip would smuggle a judgement
	 * into evidence, and it is the single easiest way for an evidence-first report to start
	 * reading like a score. Asserting neutrality rather than an exact hex means the test
	 * survives a palette change but not a change of *meaning*.
	 */
	expect(isNeutral(style.backgroundColor), `background ${style.backgroundColor}`).toBe(true);
	expect(isNeutral(style.color), `text ${style.color}`).toBe(true);

	// And specifically not the warm hue reserved for "we could not check".
	const [red, , blue] = channels(style.backgroundColor);
	expect(red - blue).toBeLessThanOrEqual(12);
});

test('DETECTED and MISSING differ by more than colour', async () => {
	const detected = await render(StatusChip, { props: { state: 'DETECTED' } });
	const missing = await render(StatusChip, { props: { state: 'MISSING' } });

	const detectedChip = detected.container.querySelector('[data-state]') as Element;
	const missingChip = missing.container.querySelector('[data-state]') as Element;

	// Text differs...
	expect(detectedChip.textContent?.trim()).toBe('Detected');
	expect(missingChip.textContent?.trim()).toBe('Missing');

	// ...and so does the border style, which is the channel that survives greyscale,
	// forced colours, and a reader who cannot distinguish the two hues.
	expect(getComputedStyle(detectedChip).borderStyle).toBe('solid');
	expect(getComputedStyle(missingChip).borderStyle).toBe('dashed');
});

test('an unrecognised finding state is named, not dropped and not crashed on', async () => {
	// The case a statically hosted bundle actually meets: the API gained a variant months
	// after this JavaScript was cached.
	const screen = await render(StatusChip, { props: { state: 'PARTIALLY_DETECTED' } });

	const chip = screen.container.querySelector('[data-state="PARTIALLY_DETECTED"]');
	expect(chip).not.toBeNull();
	// Named. Rule 2 of the unknown-variant policy: never silently drop it.
	expect(chip?.textContent).toContain('PARTIALLY_DETECTED');
	// And neutral, so it cannot be mistaken for a state this build knows how to rank.
	expect(isNeutral(getComputedStyle(chip as Element).backgroundColor)).toBe(true);
});

test('severity and confidence render as two separate, labelled badges', async () => {
	const severity = await render(SeverityBadge, { props: { value: 'HIGH' } });
	const confidence = await render(ConfidenceBadge, { props: { value: 'LOW' } });

	const severityBadge = severity.container.querySelector('[data-severity="HIGH"]');
	const confidenceBadge = confidence.container.querySelector('[data-confidence="LOW"]');

	expect(severityBadge).not.toBeNull();
	expect(confidenceBadge).not.toBeNull();
	// Two elements, two attributes: neither component can render the other's axis.
	expect(severity.container.querySelector('[data-confidence]')).toBeNull();
	expect(confidence.container.querySelector('[data-severity]')).toBeNull();

	/*
	 * The axis is named on screen. Two bare badges reading "High" and "Low" tell a reader
	 * nothing about which is impact and which is evidence — and that ambiguity is exactly
	 * what lets a low-confidence guess read as a certainty.
	 */
	expect(severityBadge?.textContent).toContain('Severity');
	expect(severityBadge?.textContent).toContain('High');
	expect(confidenceBadge?.textContent).toContain('Confidence');
	expect(confidenceBadge?.textContent).toContain('Low');
});

test('the two badges differ in shape, not only in wording', async () => {
	const severity = await render(SeverityBadge, { props: { value: 'MEDIUM' } });
	const confidence = await render(ConfidenceBadge, { props: { value: 'MEDIUM' } });

	const severityRadius = getComputedStyle(
		severity.container.querySelector('[data-severity]') as Element
	).borderRadius;
	const confidenceRadius = getComputedStyle(
		confidence.container.querySelector('[data-confidence]') as Element
	).borderRadius;

	expect(severityRadius).not.toBe(confidenceRadius);
});

test('CopyableSha shows seven characters and keeps the full value reachable', async () => {
	const full = '0584a2df65968a4e9e6859ef46bbed430408a3f1';
	const screen = await render(CopyableSha, { props: { value: full, label: 'commit SHA' } });

	const code = screen.container.querySelector('code') as HTMLElement;
	expect(code.textContent).toBe('0584a2d');
	expect(code.textContent).toHaveLength(7);
	// A report is evidence: the full value must remain copyable, never merely elided.
	expect(code.getAttribute('title')).toBe(full);
	// ...but the *visible* string stays short. A footer-length hash is not a design.
	expect(code.textContent).not.toContain(full.slice(8));

	// `title` is not an accessible substitute — it is absent on touch and unreliably
	// announced — so the full value is in the button's accessible name too.
	const button = screen.container.querySelector('button') as HTMLElement;
	expect(button.textContent).toContain(full);
});

test('CopyableSha keeps a digest algorithm prefix when truncating', async () => {
	const digest = 'sha256:6b8f9e2c1a4d7f30b5c8e1a2d4f6089b3c5e7a9d1f2408b6c8e0a2d4f6b8c0e2';
	const screen = await render(CopyableSha, { props: { value: digest, label: 'content digest' } });

	// Dropping the prefix would leave a string that looks like a hash and cannot be
	// verified as one.
	expect(screen.container.querySelector('code')?.textContent).toBe('sha256:6b8f9e2');
});

test('Disclosure is a native details/summary and toggles with the keyboard', async () => {
	const body = createRawSnippet(() => ({ render: () => '<p>Cargo.toml</p>' }));
	const screen = await render(Disclosure, {
		props: { summary: 'Evidence', hint: '1 item', children: body }
	});

	const details = screen.container.querySelector('details');
	const summary = screen.container.querySelector('summary');

	// Native, so `aria-expanded`, keyboard operation and the open state come from the
	// platform rather than from our re-implementation of it.
	expect(details).not.toBeNull();
	expect(summary).not.toBeNull();
	expect(details?.open).toBe(false);

	summary?.focus();
	expect(document.activeElement).toBe(summary);

	await screen.getByText('Evidence').click();
	expect(details?.open).toBe(true);
});
