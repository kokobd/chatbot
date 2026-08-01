import { randomUUID } from "node:crypto";
import { defineConfig, devices } from "@playwright/test";

/**
 * Read environment variables from file.
 * https://github.com/motdotla/dotenv
 */
import { config } from "dotenv";

config({
  path: ".env.local",
});

config({
  path: ".env",
});

/* Use an isolated port so an unrelated dev server cannot be reused. */
const PORT = process.env.E2E_PORT || process.env.PORT || 3100;
const webServerCommand =
  process.env.E2E_USE_DEV_SERVER === "1"
    ? `pnpm exec next dev --webpack -H 127.0.0.1 -p ${PORT}`
    : `pnpm exec next build && pnpm exec next start -H 127.0.0.1 -p ${PORT}`;

/**
 * Set webServer.url and use.baseURL with the location
 * of the WebServer respecting the correct set port
 */
const baseURL = `http://localhost:${PORT}`;
const runId = (process.env.E2E_RUN_ID ?? randomUUID()).replace(
  /[^a-zA-Z0-9-]/g,
  "-"
);
const baseIapTestSubject =
  process.env.IAP_TEST_SUBJECT ?? "playwright-test-subject";
const baseIapTestEmail = process.env.IAP_TEST_EMAIL ?? "playwright@example.com";
const useUniqueIdentity = process.env.E2E_FIXED_IDENTITY !== "1";
const iapTestSubject = useUniqueIdentity
  ? `${baseIapTestSubject}-e2e-${runId}`
  : baseIapTestSubject;
const iapTestEmail = useUniqueIdentity
  ? (() => {
      const at = baseIapTestEmail.lastIndexOf("@");
      if (at < 1) {
        return `${baseIapTestEmail}-e2e-${runId}`;
      }
      return `${baseIapTestEmail.slice(0, at)}+e2e-${runId}${baseIapTestEmail.slice(at)}`;
    })()
  : baseIapTestEmail;

process.env.E2E_IAP_TEST_EMAIL = iapTestEmail;
process.env.E2E_IAP_TEST_SUBJECT = iapTestSubject;

/**
 * See https://playwright.dev/docs/test-configuration.
 */
export default defineConfig({
  expect: { timeout: 30 * 1000 },
  /* Fail the build on CI if you accidentally left test.only in the source code. */
  forbidOnly: !!process.env.CI,
  /* Run tests in files in parallel */
  fullyParallel: false,

  /* Configure projects */
  projects: [
    {
      name: "e2e",
      testIgnore: /e2e\/mobile-layout\.test\.ts/,
      testMatch: /e2e\/.*.test.ts/,
      use: {
        ...devices["Desktop Chrome"],
      },
    },

    {
      name: "mobile-layout",
      testMatch: /e2e\/mobile-layout\.test\.ts/,
      use: {
        ...devices["Pixel 5"],
      },
    },

    // {
    //   name: 'firefox',
    //   use: { ...devices['Desktop Firefox'] },
    // },

    // {
    //   name: 'webkit',
    //   use: { ...devices['Desktop Safari'] },
    // },

    /* Test against mobile viewports. */
    // {
    //   name: 'Mobile Chrome',
    //   use: { ...devices['Pixel 5'] },
    // },
    // {
    //   name: 'Mobile Safari',
    //   use: { ...devices['iPhone 12'] },
    // },

    /* Test against branded browsers. */
    // {
    //   name: 'Microsoft Edge',
    //   use: { ...devices['Desktop Edge'], channel: 'msedge' },
    // },
    // {
    //   name: 'Google Chrome',
    //   use: { ...devices['Desktop Chrome'], channel: 'chrome' },
    // },
  ],
  /* Reporter to use. See https://playwright.dev/docs/test-reporters */
  reporter: "html",
  /* Retry on CI only */
  retries: 0,
  testDir: "./tests",

  /* Configure global timeout for each test */
  timeout: 90 * 1000,
  /* Shared settings for all the projects below. See https://playwright.dev/docs/api/class-testoptions. */
  use: {
    /* Base URL to use in actions like `await page.goto('/')`. */
    baseURL,

    extraHTTPHeaders: {
      "x-goog-authenticated-user-email": `accounts.google.com:${iapTestEmail}`,
      "x-goog-authenticated-user-id": `accounts.google.com:${iapTestSubject}`,
    },

    /* Collect trace when retrying the failed test. See https://playwright.dev/docs/trace-viewer */
    trace: "retain-on-failure",
  },

  /* Run your local dev server before starting the tests */
  webServer: {
    command: webServerCommand,
    env: {
      E2E_REAL_TESTS: "1",
      IAP_AUTH_PROVIDER: "test",
      IAP_TEST_EMAIL: iapTestEmail,
      IAP_TEST_SUBJECT: iapTestSubject,
    },
    reuseExistingServer: false,
    timeout: 120 * 1000,
    url: `${baseURL}/api/e2e/ready`,
  },
  /* Limit workers to prevent browser crashes */
  workers: 1,
});
