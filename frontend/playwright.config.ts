import { defineConfig, devices } from '@playwright/test';

const PORT = parseInt(process.env.PLAYWRIGHT_PORT ?? '4173', 10);
const HOST = process.env.PLAYWRIGHT_HOST ?? '127.0.0.1';
const BASE_URL =
  process.env.PLAYWRIGHT_BASE_URL ?? `http://${HOST}:${PORT}`;

/**
 * Default Playwright configuration.
 * Spins up the Vite dev server before running tests so we can exercise the app end-to-end.
 * Extend or override this in individual tests via `test.use`.
 */
export default defineConfig({
  testDir: './tests',
  fullyParallel: true,
  retries: process.env.CI ? 2 : 0,
  reporter: [['list'], ['html', { outputFolder: 'playwright-report' }]],
  use: {
    baseURL: BASE_URL,
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },
  webServer: {
    command: process.env.CI
      ? `npm run preview -- --host ${HOST} --port ${PORT}`
      : `npm run dev -- --host ${HOST} --port ${PORT}`,
    url: BASE_URL,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'firefox',
      use: { ...devices['Desktop Firefox'] },
    },
    {
      name: 'webkit',
      use: { ...devices['Desktop Safari'] },
    },
  ],
});
