import { expect, test } from 'vitest';
import { render, type Component, type ComponentImport } from 'vitest-browser-svelte';

import AnalysisRoute from '../routes/analyses/[analysisId]/+page.svelte';
import HomeRoute from '../routes/+page.svelte';
import ReportRoute from '../routes/reports/[analysisId]/+page.svelte';

/**
 * Foundation smoke tests, in a real browser.
 *
 * These deliberately assert structure rather than copy: the copy is placeholder and will
 * change, but "exactly one h1 per route, no skipped heading levels" is a standing
 * accessibility requirement that must not regress while the screens are being built.
 *
 * Browser mode, not jsdom — jsdom misreports focus order and computed ARIA, which are
 * precisely what the rest of this suite will come to assert. A green jsdom run would be
 * actively misleading here.
 */

const routes: Array<{ name: string; component: ComponentImport<Component> }> = [
	{ name: 'landing', component: HomeRoute },
	{ name: 'analysis progress', component: AnalysisRoute },
	{ name: 'report', component: ReportRoute }
];

for (const route of routes) {
	test(`${route.name} route renders exactly one level-1 heading`, async () => {
		const screen = await render(route.component);

		await expect.element(screen.getByRole('heading', { level: 1 })).toBeInTheDocument();
		expect(screen.container.querySelectorAll('h1')).toHaveLength(1);
	});

	test(`${route.name} route does not skip heading levels`, async () => {
		const screen = await render(route.component);

		const levels = [...screen.container.querySelectorAll('h1, h2, h3, h4, h5, h6')].map((heading) =>
			Number(heading.tagName.slice(1))
		);

		expect(levels[0]).toBe(1);
		for (let index = 1; index < levels.length; index += 1) {
			const previous = levels[index - 1] ?? 1;
			const current = levels[index] ?? 1;
			expect(current - previous).toBeLessThanOrEqual(1);
		}
	});
}
