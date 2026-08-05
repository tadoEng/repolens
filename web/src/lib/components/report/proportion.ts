/**
 * Set `--proportion` on an element, for the LOC bar backgrounds.
 *
 * **Why an attachment rather than `style="--proportion: {p}"`.** The app ships under a
 * `style-src 'self'` Content Security Policy with no `'unsafe-inline'`. An inline `style`
 * attribute is governed by `style-src-attr`, which falls back to `style-src` — so a markup
 * style attribute is *blocked in the deployed build*, silently, leaving every bar at zero
 * width while the numbers still render. It would pass every local check and fail only
 * behind the CSP.
 *
 * CSSOM is not subject to `style-src`. `node.style.setProperty(...)` is therefore the one
 * way to hand a per-row value to CSS here, and doing it explicitly — rather than trusting
 * that a framework directive happens to compile to CSSOM — is what keeps that true after
 * the next Svelte upgrade. `e2e/report.spec.ts` fails on any `securitypolicyviolation`,
 * which is the standing guard.
 */

import type { Attachment } from 'svelte/attachments';

/**
 * @param proportion Share in `[0, 1]`. Out-of-range and non-finite values clamp rather than
 * throw: a bar is decoration, and the real number sits in the same cell either way.
 */
export function proportionBar(proportion: number): Attachment<HTMLElement> {
	const clamped = Number.isFinite(proportion) ? Math.min(1, Math.max(0, proportion)) : 0;

	return (node: HTMLElement) => {
		node.style.setProperty('--proportion', String(clamped));
	};
}
