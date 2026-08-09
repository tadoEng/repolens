/**
 * Formatting for values that came off the wire.
 *
 * All of these are display-only. None of them round a number the reader is meant to check
 * against the repository — where a count appears, it appears in full.
 */

const INTEGER = new Intl.NumberFormat('en', { useGrouping: true, maximumFractionDigits: 0 });

/** A count, grouped. `78310` → `78,310`. */
export function integer(value: number): string {
	return INTEGER.format(value);
}

/**
 * A proportion in `[0, 1]` as a percentage string.
 *
 * Never rounds a non-zero share down to `0%`. A language at 0.04% of a codebase is present,
 * and printing `0.0%` next to a visible row states the opposite of the row's own existence.
 * Below the resolution of one decimal place the honest rendering is `<0.1%`.
 */
export function percent(proportion: number): string {
	if (!Number.isFinite(proportion) || proportion <= 0) return '0%';
	const value = proportion * 100;
	if (value < 0.1) return '<0.1%';
	if (value < 10) return `${value.toFixed(1)}%`;
	return `${Math.round(value)}%`;
}

const BYTE_UNITS = ['bytes', 'kB', 'MB', 'GB', 'TB'] as const;

/** Byte counts, decimal units, one decimal place above `kB`. */
export function bytes(value: number): string {
	if (!Number.isFinite(value) || value < 1000) return `${integer(Math.max(0, value))} bytes`;

	let scaled = value;
	let unit = 0;
	while (scaled >= 1000 && unit < BYTE_UNITS.length - 1) {
		scaled /= 1000;
		unit += 1;
	}
	return `${scaled.toFixed(1)} ${BYTE_UNITS[unit]}`;
}

/**
 * An RFC 3339 timestamp, rendered in the reader's own zone.
 *
 * Returns the raw string unchanged when it cannot be parsed. A report is evidence; an
 * unparseable timestamp is a fact about the response, and replacing it with "Invalid Date"
 * would destroy the only copy of it the reader has.
 */
export function timestamp(value: string): string {
	const parsed = new Date(value);
	if (Number.isNaN(parsed.getTime())) return value;
	return parsed.toLocaleString(undefined, {
		dateStyle: 'medium',
		timeStyle: 'short'
	});
}

/**
 * A duration in seconds, as words.
 *
 * Used for `retry_after_seconds`, which the contract makes absent rather than zero when it
 * is unknown — so this is only ever called with a number the server actually supplied.
 */
export function duration(seconds: number): string {
	if (!Number.isFinite(seconds) || seconds < 0) return 'an unknown time';
	if (seconds < 60) return `${Math.round(seconds)} seconds`;
	const minutes = Math.round(seconds / 60);
	if (minutes < 60) return `${minutes} minute${minutes === 1 ? '' : 's'}`;
	const hours = Math.round(minutes / 60);
	return `${hours} hour${hours === 1 ? '' : 's'}`;
}

/**
 * A latency in microseconds, in the largest unit that keeps it readable.
 *
 * Microseconds are what the contract publishes, because the figure under test is expected
 * to be small: a histogram whose finest bucket was one millisecond could not tell "fast"
 * from "immeasurable". Rendering them raw would make a route table unreadable, and
 * rendering them in milliseconds alone would print `0 ms` for the handler this whole
 * experiment exists to measure — which states the opposite of what was observed.
 *
 * So the unit follows the value, and sub-millisecond figures keep their own unit rather
 * than being rounded into a zero.
 */
export function micros(value: number): string {
	if (!Number.isFinite(value) || value < 0) return 'unknown';
	if (value < 1000) return `${integer(value)} µs`;
	if (value < 10_000) return `${(value / 1000).toFixed(1)} ms`;
	if (value < 1_000_000) return `${integer(Math.round(value / 1000))} ms`;
	return `${(value / 1_000_000).toFixed(2)} s`;
}

/**
 * A process uptime in seconds, as the coarsest two units that describe it.
 *
 * Two units rather than one, because one is ambiguous exactly where it matters: "2 days" and
 * "2 days 23 hours" are nearly a day apart, and a reader comparing a cold start against a
 * warm process needs to know which. Two rather than all four, because seconds stop being
 * interesting the moment hours are involved.
 */
export function uptime(seconds: number): string {
	if (!Number.isFinite(seconds) || seconds < 0) return 'unknown';

	const whole = Math.floor(seconds);
	const parts: string[] = [];
	const units: readonly (readonly [number, string])[] = [
		[86_400, 'd'],
		[3600, 'h'],
		[60, 'm'],
		[1, 's']
	];

	let remaining = whole;
	for (const [size, suffix] of units) {
		const count = Math.floor(remaining / size);
		remaining %= size;
		// A leading zero is noise; a middle one is information. `1 d 0 h` says the process
		// started just over a day ago, which `1 d 3 m` would not.
		if (count === 0 && parts.length === 0) continue;
		parts.push(`${count} ${suffix}`);
		if (parts.length === 2) break;
	}

	return parts.length > 0 ? parts.join(' ') : '0 s';
}

/**
 * The first seven characters of a digest, keeping any algorithm prefix.
 *
 * `sha256:6b8f9e2c…` shortens to `sha256:6b8f9e2`, not to `sha256:`. The prefix is part of
 * what makes the digest checkable, so truncating it away would leave a string that looks
 * like a hash and cannot be verified as one.
 */
export function shortDigest(value: string): string {
	const separator = value.indexOf(':');
	if (separator === -1) return value.slice(0, 7);
	const algorithm = value.slice(0, separator + 1);
	return `${algorithm}${value.slice(separator + 1, separator + 8)}`;
}
