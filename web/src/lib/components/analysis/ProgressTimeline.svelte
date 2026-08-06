<script lang="ts">
	/*
	 * Where the analysis has reached.
	 *
	 * ## The step order comes from the contract, not from here
	 *
	 * `AnalysisState` is documented as "ordered as the work actually proceeds, so a UI can
	 * render a checklist by position without a second table mapping states to steps". So
	 * the five steps are read off that enum, and the three terminal states have no step —
	 * `COMPLETED` means every step is behind us, and neither failure says how far it got.
	 *
	 * ## Reduced motion is not an afterthought here
	 *
	 * The active step's marker animates. Under `prefers-reduced-motion` that animation is
	 * gone, so if motion were the only signal for "this is where we are", the screen would
	 * silently stop saying anything. Every step therefore carries its status **as text**,
	 * always — the animation is redundant decoration on top of a label that already says
	 * "In progress".
	 *
	 * ## The live region does not spam
	 *
	 * Polling replaces the analysis object on every tick, but the announcement is derived
	 * from the *state*, so an unchanged poll produces an identical string and Svelte leaves
	 * the DOM alone. A screen reader is told when something happened, and only then.
	 */
	import {
		ANALYSIS_STEPS,
		analysisState,
		analysisStepNumber,
		isFailure
	} from '$lib/contract/enums';

	interface Props {
		/** Raw wire value. `string`, because that is what a response contains. */
		state: string;
	}

	let { state }: Props = $props();

	const display = $derived(analysisState(state));
	const currentStep = $derived(analysisStepNumber(state));
	const completed = $derived(state === 'COMPLETED');
	const failed = $derived(isFailure(state));

	type StepStatus = 'done' | 'active' | 'pending' | 'halted' | 'unknown';

	const STEP_STATUS_LABELS: Record<StepStatus, string> = {
		done: 'Done',
		active: 'In progress',
		pending: 'Not started',
		halted: 'Not reported',
		unknown: 'Not reported'
	};

	function statusOf(step: number): StepStatus {
		if (completed) return 'done';
		// A failure does not say which step it reached. Claiming one would be an invention,
		// and "Not reported" is the honest label for a fact the contract does not carry.
		if (failed) return 'halted';
		if (currentStep === null) return 'unknown';
		if (step < currentStep) return 'done';
		if (step === currentStep) return 'active';
		return 'pending';
	}

	/** One sentence, recomputed only when the state itself changes. */
	const announcement = $derived.by(() => {
		if (completed) return 'Analysis completed. The report is ready.';
		if (failed) return `Analysis stopped: ${display.label}.`;
		if (currentStep === null) {
			return `Analysis is in a state this page does not recognise: ${display.raw}.`;
		}
		return `Step ${currentStep} of ${ANALYSIS_STEPS.length}: ${display.label}.`;
	});
</script>

<!--
	`aria-live="polite"` rather than a `role="status"` element created on demand: the region
	exists from first paint, which is what makes later updates reliably announced.
-->
<p class="timeline__announcement" aria-live="polite">
	{announcement}
</p>

<ol class="timeline">
	{#each ANALYSIS_STEPS as step, index (step)}
		{@const number = index + 1}
		{@const status = statusOf(number)}
		<li class="timeline__step timeline__step--{status}">
			<span class="timeline__marker" aria-hidden="true">{number}</span>
			<span class="timeline__label">{analysisState(step).label}</span>
			<!-- The status in words. This is what survives reduced motion, greyscale and
			     forced colours, because it was never carried by any of them. -->
			<span class="timeline__status">{STEP_STATUS_LABELS[status]}</span>
		</li>
	{/each}
</ol>

{#if failed}
	<p class="timeline__note">
		The analysis stopped before completing. The contract does not report which step it reached, so
		no step is marked — see the failure below for what happened.
	</p>
{:else if currentStep === null && !completed}
	<p class="timeline__note">
		This build does not recognise the state <code>{display.raw}</code>, so it cannot place the
		analysis on the timeline. The analysis itself is unaffected.
	</p>
{/if}

<style>
	.timeline__announcement {
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}

	.timeline {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.timeline__step {
		display: grid;
		grid-template-columns: 2rem 1fr auto;
		align-items: baseline;
		gap: var(--space-3);
		padding: var(--space-2) var(--space-3);
		border: var(--border-width) solid var(--border-subtle);
		border-radius: var(--radius-sm);
	}

	.timeline__marker {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		inline-size: 1.75rem;
		block-size: 1.75rem;
		border: var(--border-width) solid var(--border-strong);
		border-radius: 999px;
		font-size: var(--font-size-sm);
		font-variant-numeric: tabular-nums;
		color: var(--text-secondary);
	}

	.timeline__label {
		font-weight: var(--font-weight-medium);
	}

	.timeline__status {
		font-size: var(--font-size-sm);
		color: var(--text-muted);
	}

	.timeline__step--done .timeline__marker {
		background-color: var(--status-detected-bg);
		border-color: var(--status-detected-border);
		color: var(--status-detected-fg);
	}

	.timeline__step--active {
		border-color: var(--border-strong);
		background-color: var(--surface-1);
	}

	.timeline__step--active .timeline__marker {
		background-color: var(--status-documented-bg);
		border-color: var(--status-documented-border);
		color: var(--status-documented-fg);
		/* Decoration only. The label already says "In progress", and the global
		 * reduced-motion sweep neutralises this without taking any meaning with it. */
		animation: timeline-pulse 1600ms var(--easing-standard) infinite;
	}

	.timeline__step--active .timeline__status {
		color: var(--text-primary);
		font-weight: var(--font-weight-medium);
	}

	.timeline__step--pending,
	.timeline__step--halted,
	.timeline__step--unknown {
		color: var(--text-muted);
	}

	.timeline__note {
		max-inline-size: var(--measure);
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}

	@keyframes timeline-pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.55;
		}
	}
</style>
