/**
 * Move focus to an in-page anchor target.
 *
 * Shared by the section nav and the overview's supporting-rule links so there is exactly
 * one answer to "what happens when a reader jumps somewhere in this document". Two copies
 * of this would be two chances for one of them to scroll without moving focus, which is the
 * defect being designed out.
 *
 * The caller must **not** call `preventDefault`: the browser's own handling of the link is
 * what updates the URL hash (making the position shareable) and what scrolls. This adds
 * only the part the platform omits.
 */
export function focusAnchor(id: string): void {
	// `preventScroll` keeps this from racing the browser's own scroll for the same anchor.
	// The target needs `tabindex="-1"`; a heading is not focusable without it.
	document.getElementById(id)?.focus({ preventScroll: true });
}
