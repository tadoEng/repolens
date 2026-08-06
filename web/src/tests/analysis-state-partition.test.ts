import { HANDLED_VARIANTS } from '@repolens/api-client';
import { describe, expect, test } from 'vitest';

import {
	ANALYSIS_STATE_PARTITION,
	ANALYSIS_STEPS,
	analysisStepNumber,
	isFailure,
	isTerminal,
	type AnalysisStatePhase
} from '$lib/contract/enums';

/**
 * The closed partition gate over `AnalysisState`.
 *
 * ## What this catches that the compiler does not
 *
 * `ANALYSIS_STATE_PARTITION` is annotated `Readonly<Record<AnalysisState, …>>`, so a state
 * that is added, removed, duplicated or miscategorised fails `pnpm -r check`. That is the
 * fast gate. It is also the gate that disappears the moment somebody widens the annotation
 * to `Record<string, …>` to get unblocked — which compiles, ships, and leaves the timeline
 * quietly stalled at the state before the new one.
 *
 * So the same properties are re-checked here at run time, against a list this file does not
 * own. The chain is: `contracts/openapi.json` → `HANDLED_VARIANTS` (asserted against the
 * document by `packages/repolens-api-client/unknown-variant.test.ts`) → this test. No link
 * in it is a literal typed out in the frontend.
 *
 * ## Why the checker takes its inputs as arguments
 *
 * A gate that only ever runs against the real, correct data proves nothing: it passes
 * whether or not it checks anything. `partitionDefects` is therefore a pure function of
 * (states, partition), asserted to be silent on the real pair and then **fed four tampered
 * pairs that it must reject**. The non-vacuity proof is part of the committed suite rather
 * than something someone claims to have done once.
 */

/** Loosened on purpose: the checker must be able to inspect a *wrong* partition. */
type LoosePartition = Readonly<Record<string, AnalysisStatePhase>>;

/**
 * Every way the partition can disagree with the contract, as sentences.
 *
 * Returns an empty array when the partition is a correct, total, contiguous cover of
 * `states`.
 */
function partitionDefects(states: readonly string[], partition: LoosePartition): string[] {
	const defects: string[] = [];
	const keys = Object.keys(partition);

	// Exactly once, checked in both directions. The first catches an added state; the
	// second catches a partition entry left behind after the contract dropped it.
	for (const state of states) {
		if (!Object.hasOwn(partition, state)) defects.push(`state is not partitioned: ${state}`);
	}
	for (const key of keys) {
		if (!states.includes(key)) defects.push(`partition names a state the contract lacks: ${key}`);
	}
	if (new Set(states).size !== states.length) {
		defects.push('the contract lists a state more than once');
	}

	const steps = keys.flatMap((key) => {
		const phase = partition[key];
		return phase !== undefined && phase.phase === 'step' ? [{ key, order: phase.order }] : [];
	});

	// Step order: unique, and contiguous from 1. A gap means a step number a reader can
	// never see; a repeat means two steps claiming the same position in "Step 3 of 5".
	const orders = steps.map((step) => step.order);
	if (new Set(orders).size !== orders.length) defects.push('two steps claim the same order');
	[...orders]
		.sort((left, right) => left - right)
		.forEach((order, index) => {
			if (order !== index + 1) defects.push(`step order is not contiguous from 1: ${order}`);
		});

	for (const key of keys) {
		const phase = partition[key];
		if (phase === undefined) continue;

		// A terminal state carrying a step number would place a finished analysis on the
		// timeline. Unrepresentable in the union, so this only fires on a tampered object.
		if (phase.phase === 'terminal' && 'order' in phase) {
			defects.push(`terminal state carries a step number: ${key}`);
		}

		// Failure is a property of the outcome, not of the name — but the contract's own
		// naming is the only cross-check available here, and a `FAILED_` state that is not
		// a terminal failure would make `isFailure` disagree with the wire.
		if (key.startsWith('FAILED_') && !(phase.phase === 'terminal' && phase.failure)) {
			defects.push(`failure state is not terminal: ${key}`);
		}
	}

	return defects;
}

const STATES = HANDLED_VARIANTS.AnalysisState ?? [];

/** The real partition, seen through the loose type so tampered copies compare like for like. */
const REAL: LoosePartition = ANALYSIS_STATE_PARTITION;

test('the contract still publishes analysis states to partition', () => {
	// Guards every assertion below: an empty list would make the whole file vacuous.
	expect(STATES.length).toBeGreaterThan(0);
});

test('every generated analysis state appears in the partition exactly once', () => {
	expect(partitionDefects(STATES, REAL)).toEqual([]);
});

test('the step list is derived from the partition and stays in declared order', () => {
	const declared = STATES.filter((state) => REAL[state]?.phase === 'step');

	expect([...ANALYSIS_STEPS].sort()).toEqual([...declared].sort());
	// Derived, so it cannot contain a duplicate — asserted anyway, because the whole point
	// is that no second list is allowed to disagree with the partition.
	expect(new Set(ANALYSIS_STEPS).size).toBe(ANALYSIS_STEPS.length);
	ANALYSIS_STEPS.forEach((state, index) => {
		expect(analysisStepNumber(state)).toBe(index + 1);
	});
});

test('each state is either a numbered step or terminal, never both and never neither', () => {
	for (const state of STATES) {
		const step = analysisStepNumber(state);
		const terminal = isTerminal(state);

		expect(step === null, state).toBe(terminal);
		// No terminal state has a step number, which is the half of the partition that a
		// `satisfies`-checked array could never express.
		if (terminal) expect(step, state).toBeNull();
		// And every failure is terminal — polling must stop.
		if (isFailure(state)) expect(terminal, state).toBe(true);
	}
});

test('a state this build has never seen is neither a step nor terminal', () => {
	// The safe error for an unrecognised state is to keep polling: declaring an analysis
	// finished that is still running is the failure that cannot be recovered from.
	expect(isTerminal('VALIDATING_SIGNATURES')).toBe(false);
	expect(isFailure('VALIDATING_SIGNATURES')).toBe(false);
	expect(analysisStepNumber('VALIDATING_SIGNATURES')).toBeNull();

	// Inherited `Object.prototype` keys are not states either.
	expect(isTerminal('constructor')).toBe(false);
	expect(analysisStepNumber('toString')).toBeNull();
});

describe('the gate is non-vacuous', () => {
	/*
	 * Each case is a partition that a real change could produce, fed to the same checker
	 * that reports the real one clean. If any of these came back empty, the test above
	 * would be decoration.
	 */

	test('a state added to the contract and not to the partition is rejected', () => {
		const defects = partitionDefects([...STATES, 'VALIDATING_SIGNATURES'], REAL);
		expect(defects).toContain('state is not partitioned: VALIDATING_SIGNATURES');
	});

	test('a partition entry the contract no longer has is rejected', () => {
		const stale: LoosePartition = { ...REAL, EXTRACTING: { phase: 'step', order: 6 } };
		expect(partitionDefects(STATES, stale)).toContain(
			'partition names a state the contract lacks: EXTRACTING'
		);
	});

	test('a state omitted from the partition is rejected', () => {
		const missing = Object.fromEntries(
			Object.entries(REAL).filter(([state]) => state !== 'ANALYZING')
		);
		const defects = partitionDefects(STATES, missing);

		expect(defects).toContain('state is not partitioned: ANALYZING');
		// And the hole it leaves in the numbering is reported too, rather than renumbering
		// the remaining steps and quietly showing "Step 4 of 4".
		expect(defects).toContain('step order is not contiguous from 1: 5');
	});

	test('a state listed twice by the contract is rejected', () => {
		expect(partitionDefects([...STATES, 'QUEUED'], REAL)).toContain(
			'the contract lists a state more than once'
		);
	});

	test('two steps sharing an order are rejected', () => {
		const duplicated: LoosePartition = { ...REAL, ANALYZING: { phase: 'step', order: 3 } };
		expect(partitionDefects(STATES, duplicated)).toContain('two steps claim the same order');
	});

	test('a failure state demoted out of terminal is rejected', () => {
		const demoted: LoosePartition = { ...REAL, FAILED_RETRIABLE: { phase: 'step', order: 6 } };
		expect(partitionDefects(STATES, demoted)).toContain(
			'failure state is not terminal: FAILED_RETRIABLE'
		);
	});

	test('a terminal state carrying a step number is rejected', () => {
		// Unrepresentable in `AnalysisStatePhase`, so this is reachable only through a cast —
		// which is exactly what a hurried "just make it compile" edit looks like.
		const smuggled = {
			...REAL,
			COMPLETED: { phase: 'terminal', failure: false, order: 6 }
		} as unknown as LoosePartition;

		expect(partitionDefects(STATES, smuggled)).toContain(
			'terminal state carries a step number: COMPLETED'
		);
	});
});
