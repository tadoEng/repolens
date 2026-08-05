import { defineConfig, devices } from '@playwright/test';

/**
 * Integration configuration: the browser against a **real Axum server**.
 *
 * Separate from `playwright.config.ts` on purpose. The main suite mocks every probe
 * request, needs no Rust toolchain, and stays fast — making it depend on `cargo build`
 * would slow the ordinary frontend loop for the sake of two tests.
 *
 * Two servers, deliberately on different ports:
 *
 *     web  → http://localhost:4174   (vite preview, production build)
 *     API  → http://localhost:8090   (cargo run --bin server)
 *
 * Different origins is the point. Same-origin would prove nothing about CORS, and CORS is
 * the failure this configuration exists to catch — a statically hosted frontend calling
 * Cloud Run is always cross-origin, and the browser, not the server, is what rejects it.
 *
 * Ports differ from the main config so both suites can run concurrently without one
 * silently attaching to the other's server via `reuseExistingServer`.
 *
 * **No database.** `DATABASE_URL` is not passed, so the probe reports `UNAVAILABLE` and
 * the frontend renders the null `schema_version`. No Neon secret is required, which keeps
 * this runnable in CI and on a fork.
 */

const API_ORIGIN = 'http://localhost:8090';
const WEB_ORIGIN = 'http://localhost:4174';

export default defineConfig({
	testDir: 'e2e',
	testMatch: 'real-api.spec.ts',
	fullyParallel: false,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 1 : 0,
	reporter: process.env.CI ? 'github' : 'list',

	webServer: [
		{
			// `cwd` is the workspace root: cargo must run where Cargo.toml lives, and
			// load_dotenv() resolves .env.local relative to the working directory.
			command: 'cargo run --quiet --bin server',
			cwd: '..',
			url: `${API_ORIGIN}/healthz`,
			reuseExistingServer: !process.env.CI,
			// A cold `cargo build` on a clean CI runner is slow; this is a build budget,
			// not a startup budget.
			timeout: 300_000,
			env: {
				PORT: '8090',
				// The exact origin the browser will send. Without this the probe request
				// is blocked by the browser before the response is ever read.
				CORS_ALLOWED_ORIGIN: WEB_ORIGIN,
				// Explicitly empty, not merely absent. `load_dotenv()` would otherwise
				// pick up a developer's real `.env.local` and connect to Neon, so this
				// spec would assert UNAVAILABLE in CI and OK on a laptop — a test whose
				// meaning depends on who runs it. An empty value is treated as missing
				// by `config::required()`, and dotenv never overrides a variable that is
				// already set, so the no-database path is deterministic everywhere.
				DATABASE_URL: '',
				DATABASE_DIRECT_URL: '',
				RUST_LOG: 'info'
			}
		},
		{
			command: 'pnpm run build && pnpm run preview --port 4174 --strictPort',
			url: WEB_ORIGIN,
			reuseExistingServer: !process.env.CI,
			timeout: 180_000,
			env: {
				// Baked into both the generated client's base URL and the CSP
				// connect-src allowlist at build time. If these disagree, the browser
				// blocks the request and the spec fails — which is the whole point.
				PUBLIC_API_ORIGIN: API_ORIGIN
			}
		}
	],

	use: {
		baseURL: WEB_ORIGIN,
		trace: 'on-first-retry'
	},

	projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }]
});
