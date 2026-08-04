/**
 * The staleness gate.
 *
 * `src/schema.ts` is generated from the API's OpenAPI document. A generated file that is
 * merely *committed* drifts silently — the backend adds a field, the frontend keeps
 * compiling against last week's shape, and nothing fails until a user sees it. This test
 * is what makes drift a build failure instead.
 *
 * Precedent: crates.io does exactly this, regenerating with
 * `vitest schema.test.ts --run --update`. Here that is wrapped as:
 *
 *     pnpm --filter @repolens/api-client schema:update
 *
 * The gate has two modes, because the OpenAPI document does not exist yet:
 *
 *   - **Document present** — regenerate and compare against the committed file. This is
 *     the real gate, and it turns on by itself the moment the backend emits
 *     `contracts/openapi.json`. No edit to this file is required to activate it.
 *   - **Document absent** — assert the generated header and the placeholder shape are
 *     still intact. Weaker, but not nothing: it catches the most likely mistake available
 *     today, which is somebody hand-editing the generated file to unblock themselves.
 */

import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import openapiTS, { astToString } from 'openapi-typescript';
import { describe, expect, test } from 'vitest';

/**
 * Where the backend publishes its OpenAPI document.
 *
 * `contracts/` is owned by the API side of the workspace (issues #11 and #14). This path
 * is the handshake between the two; if it moves, this constant is the only thing to change.
 */
const OPENAPI_DOCUMENT_PATH = fileURLToPath(new URL('../../contracts/openapi.json', import.meta.url));

const SCHEMA_PATH = fileURLToPath(new URL('./src/schema.ts', import.meta.url));

/**
 * The banner every generated schema must carry, byte for byte.
 *
 * It lives here rather than in `src/schema.ts` because this file is the authority: the
 * generator output is prefixed with it, and the placeholder is checked against it. Editing
 * the banner in the generated file therefore fails the gate, which is the intended
 * behaviour.
 */
const GENERATED_HEADER = `/**
 * GENERATED FILE — DO NOT EDIT BY HAND.
 *
 * Produced by \`openapi-typescript\` from the OpenAPI document that the Axum service emits
 * via utoipa. Regenerate with:
 *
 *     pnpm --filter @repolens/api-client schema:update
 *
 * \`schema.test.ts\` is the staleness gate: it regenerates this file from the committed
 * OpenAPI document and fails if the result differs. A hand edit here is therefore not a
 * shortcut — it is a build break waiting for the next CI run, and it silently decouples
 * the frontend's idea of the API from the backend's.
 */

`;

/** Git may check these files out with CRLF on Windows; the comparison must not care. */
function normalizeNewlines(source: string): string {
	return source.replace(/\r\n/g, '\n');
}

const documentExists = existsSync(OPENAPI_DOCUMENT_PATH);

describe('generated API schema', () => {
	test.runIf(documentExists)(
		'src/schema.ts is regenerated from the committed OpenAPI document',
		async () => {
			const ast = await openapiTS(new URL(`file://${OPENAPI_DOCUMENT_PATH}`));
			const generated = `${GENERATED_HEADER}${astToString(ast)}`;

			// The snapshot file IS the generated source, so `--update` regenerates in place.
			await expect(normalizeNewlines(generated)).toMatchFileSnapshot(SCHEMA_PATH);
		}
	);

	test.runIf(!documentExists)(
		'src/schema.ts is still the untouched generated placeholder',
		() => {
			const committed = normalizeNewlines(readFileSync(SCHEMA_PATH, 'utf8'));

			expect(
				committed.startsWith(normalizeNewlines(GENERATED_HEADER)),
				'src/schema.ts must begin with the generated-file banner verbatim'
			).toBe(true);

			// An empty OpenAPI document produces exactly these five aliases. If any of them
			// has been hand-edited into something real, the contract is being invented in the
			// frontend rather than generated from the backend.
			for (const name of ['paths', 'webhooks', 'components', '$defs', 'operations']) {
				expect(committed).toContain(`export type ${name} = Record<string, never>;`);
			}
		}
	);
});
