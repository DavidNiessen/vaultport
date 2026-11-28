// SPDX-License-Identifier: GPL-3.0-or-later
import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

// The Tauri webview runs with `freezePrototype` (tauri.conf.json); a plain browser does not.
// Freeze it here as well, so a dependency that assigns over an inherited `Object.prototype`
// property fails these tests instead of leaving the packaged app with a blank window.
test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    Object.freeze(Object.prototype);
  });
});

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

test('suggests completions while typing and inserts one with Tab', async ({ page, isMobile }) => {
  test.skip(isMobile, 'needs a hardware keyboard');
  await unlock(page);
  await page
    .getByRole('button', { name: /router/ })
    .first()
    .dblclick();
  const text = () =>
    page.evaluate(() => (window as unknown as { __terminalText: () => string }).__terminalText());
  await expect.poll(text, { timeout: 10_000 }).toContain('Welcome to router');
  await page.keyboard.type('git ch');
  const popup = page.getByTestId('autocomplete');
  await expect(popup.getByRole('option', { name: /checkout/ })).toBeVisible();
  await expect(popup.getByRole('option', { name: /cherry-pick/ })).toBeVisible();
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Tab');
  await expect(popup).toBeHidden();
  await expect.poll(text).toContain('$ git cherry-pick ');
  await page.keyboard.press('Escape');
  await page.keyboard.type('\r');
  await expect.poll(text).toContain('git: command not found');
});

test('shows commands as blocks, searches history and suggests inline', async ({
  page,
  isMobile,
}) => {
  test.skip(isMobile, 'needs a hardware keyboard');
  await unlock(page);
  await page
    .getByRole('button', { name: /router/ })
    .first()
    .dblclick();
  const text = () =>
    page.evaluate(() => (window as unknown as { __terminalText: () => string }).__terminalText());
  await expect.poll(text, { timeout: 10_000 }).toContain('Welcome to router');

  // A failing command gets a red badge with its exit status.
  await page.keyboard.type('false\r');
  const badge = page.locator('.vp-block-badge.vp-fail').first();
  await expect(badge).toContainText('✗ 1');
  await badge.click();
  const menu = page.getByTestId('block-menu');
  await expect(menu.getByRole('menuitem', { name: 'Run again' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(menu).toBeHidden();

  // Ctrl+R searches the command history.
  await page.locator('.xterm-helper-textarea').first().focus();
  await page.keyboard.press('Control+r');
  const search = page.getByTestId('history-search');
  await search.getByRole('textbox').fill('nginx');
  await expect(search.getByRole('option', { name: /systemctl status nginx/ })).toBeVisible();
  await page.keyboard.press('Enter');
  await expect(search).toBeHidden();
  await expect.poll(text).toContain('$ systemctl status nginx');
  await page.keyboard.press('Control+u');

  // The rest of a matching history command shows in grey; → accepts it.
  await page.keyboard.type('docker co');
  await expect(page.getByTestId('ghost-text')).toHaveText('mpose up -d');
  await page.keyboard.press('ArrowRight');
  await expect.poll(text).toContain('$ docker compose up -d');
});

test('opens, closes and reopens terminal tabs from the keyboard', async ({ page, isMobile }) => {
  test.skip(isMobile, 'needs a hardware keyboard');
  await unlock(page);
  const tabs = page.getByRole('tablist', { name: 'Tabs' });
  await page.keyboard.press('Control+t');
  await expect(tabs.getByRole('tab', { name: /Local/ })).toHaveCount(1);
  await page.keyboard.press('Control+t');
  await expect(tabs.getByRole('tab', { name: /Local/ })).toHaveCount(2);
  await page.keyboard.press('Control+w');
  await expect(tabs.getByRole('tab', { name: /Local/ })).toHaveCount(1);
  await page.keyboard.press('Control+Shift+t');
  await expect(tabs.getByRole('tab', { name: /Local/ })).toHaveCount(2);
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

test('configures AI agent access and approves an agent action', async ({ page, isMobile }) => {
  test.skip(isMobile, 'agents connect to the desktop app');
  await unlock(page);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'AI agents', exact: true }).click();
  const toggle = page.getByRole('switch', { name: 'Allow AI agents' });
  await toggle.click();
  await expect(toggle).toHaveAttribute('data-state', 'checked');
  await expect(page.getByTestId('agent-config')).toContainText('claude mcp add');
  await page.getByRole('radio', { name: 'Claude Desktop' }).click();
  await expect(page.getByTestId('agent-config')).toContainText('"mcpServers"');

  await page.evaluate(() =>
    (window as unknown as { __mockAgentRequest: (d: string) => string }).__mockAgentRequest(
      'systemctl restart nginx',
    ),
  );
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('Claude Code 2.1 wants to act');
  await expect(dialog.getByTestId('agent-approval')).toContainText('systemctl restart nginx');
  await dialog.getByRole('button', { name: 'Allow once' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText('run_command').first()).toBeVisible();
  await expect(page.getByText('Done', { exact: true }).first()).toBeVisible();
});
