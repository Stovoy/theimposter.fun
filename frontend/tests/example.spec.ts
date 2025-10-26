import { test, expect } from '@playwright/test';

test('smoke: landing page renders', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveTitle(/Imposter/i);
});
