// SPDX-License-Identifier: GPL-3.0-or-later
import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

async function unlock(page: Page) {
  await page.goto('/?platform=linux');
  await page.getByLabel('Master password').fill('password');
  await page.getByRole('button', { name: 'Unlock', exact: true }).click();
  await expect(page.getByRole('button', { name: /api-prod-eu-1/ }).first()).toBeVisible();
}

test('rejects a wrong master password', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Master password').fill('nope');
  await page.getByRole('button', { name: 'Unlock', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('wrong password');
});

test('unlocks the vault and lists hosts and groups', async ({ page }) => {
  await unlock(page);
  await expect(page.getByRole('button', { name: /Production/ }).first()).toBeVisible();
  await expect(page.getByRole('button', { name: /db-prod-eu-1/ }).first()).toBeVisible();
});

test('filters hosts with the omnibox and recognises ssh commands', async ({ page }) => {
  await unlock(page);
  const omnibox = page.getByLabel('Find a host or type ssh user@hostname…');
  await omnibox.fill('nginx');
  await expect(page.getByRole('button', { name: /web-staging/ }).first()).toBeVisible();
  await expect(page.getByRole('button', { name: /db-prod-eu-1/ })).toHaveCount(0);
  await omnibox.fill('ssh root@10.0.0.9 -p 2222');
  await expect(page.getByText('Press Enter to connect to root@10.0.0.9:2222')).toBeVisible();
});

test('opens a terminal session and runs a command', async ({ page, isMobile }) => {
  await unlock(page);
  const card = page.getByRole('button', { name: /router/ }).first();
  if (isMobile) await card.click();
  else await card.dblclick();
  const text = () =>
    page.evaluate(() => (window as unknown as { __terminalText: () => string }).__terminalText());
  await expect.poll(text, { timeout: 10_000 }).toContain('Welcome to router');
  await page.keyboard.type('whoami\r');
  await expect.poll(text).toContain('deploy\n');
});

test('creates a snippet', async ({ page, isMobile }) => {
  test.skip(isMobile, 'covered by the desktop layout');
  await unlock(page);
  await page.getByRole('button', { name: 'Snippets', exact: true }).click();
  await page.getByRole('button', { name: 'New snippet' }).first().click();
  await page.getByLabel('Description').fill('Show uptime');
  await page.getByRole('textbox', { name: 'Script', exact: true }).fill('uptime');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('Show uptime').first()).toBeVisible();
});

test('has no critical accessibility violations', async ({ page }) => {
  await unlock(page);
  const results = await new AxeBuilder({ page }).disableRules(['color-contrast']).analyze();
  const critical = results.violations.filter((v) => v.impact === 'critical');
  expect(critical, JSON.stringify(critical, null, 2)).toEqual([]);
});
