<script lang="ts">
	/*
	 * A `FindingState`, rendered as a chip.
	 *
	 * Three rules bind this component, and each one is a defect that ships routinely:
	 *
	 * 1. **`MISSING` is neutral grey, never red.** Absence is not failure. The analyzer
	 *    saying "no architecture document was found" is a fact about the repository, and
	 *    colouring it as an error smuggles a judgement into evidence. If absence matters,
	 *    the rule's own explanation says why — the chip does not get to decide.
	 *
	 * 2. **Status is never colour alone.** The word carries the meaning; the hue is
	 *    decoration, and the border style gives a third, non-colour channel so the states
	 *    remain distinguishable in greyscale, in forced-colors mode, and to a reader with
	 *    a colour vision deficiency.
	 *
	 * 3. **An unrecognised value is rendered, not dropped.** A statically hosted bundle
	 *    outlives the build it was compiled against. The fallback names the raw value in a
	 *    neutral chip rather than crashing or, worse, silently omitting a finding.
	 */
	import { findingState } from '$lib/contract/enums';

	interface Props {
		/** Raw wire value. Typed as `string`, because that is what a response contains. */
		state: string | null | undefined;
	}

	let { state }: Props = $props();

	const display = $derived(findingState(state));
</script>

<span class="chip chip--{display.token}" data-state={display.raw}>
	{display.label}
</span>

<style>
	.chip {
		display: inline-flex;
		align-items: baseline;
		gap: 0.25ch;
		padding: 0.1em var(--space-2);
		border: var(--border-width) solid;
		border-radius: var(--radius-sm);
		font-size: var(--font-size-sm);
		font-weight: var(--font-weight-medium);
		line-height: var(--line-height-body);
		white-space: nowrap;
	}

	/* The second, non-colour channel. Solid = the analyzer saw something; dashed = it
	 * looked and found nothing; dotted = it could not look. */
	.chip--detected,
	.chip--documented {
		border-style: solid;
	}

	.chip--missing,
	.chip--not-applicable {
		border-style: dashed;
	}

	.chip--unable-to-verify,
	.chip--unknown {
		border-style: dotted;
	}

	.chip--detected {
		background-color: var(--status-detected-bg);
		border-color: var(--status-detected-border);
		color: var(--status-detected-fg);
	}

	.chip--documented {
		background-color: var(--status-documented-bg);
		border-color: var(--status-documented-border);
		color: var(--status-documented-fg);
	}

	/* Grey. Deliberately, permanently. See rule 1 above. */
	.chip--missing {
		background-color: var(--status-missing-bg);
		border-color: var(--status-missing-border);
		color: var(--status-missing-fg);
	}

	.chip--not-applicable {
		background-color: var(--status-not-applicable-bg);
		border-color: var(--status-not-applicable-border);
		color: var(--status-not-applicable-fg);
	}

	/* The only warm hue in the set: it says something about *our* limits, not the repo's. */
	.chip--unable-to-verify {
		background-color: var(--status-unable-to-verify-bg);
		border-color: var(--status-unable-to-verify-border);
		color: var(--status-unable-to-verify-fg);
	}

	.chip--unknown {
		background-color: var(--surface-1);
		border-color: var(--border-strong);
		color: var(--text-secondary);
		font-family: var(--font-mono);
		white-space: normal;
	}
</style>
