import type { Browser, BrowserContext, Page } from '@playwright/test';
import { expect, test } from '@playwright/test';
import { MockGameServer } from './helpers/mockGameServer';

const DEFAULT_TIMEOUT = 10_000;

type PlayerHandle = {
  context: BrowserContext;
  page: Page;
  name: string;
};

const lobbyHeading = (code: string) => `Lobby ${code}`;

const playerStatusLocator = (page: Page, playerName: string) =>
  page.locator('.players li').filter({ hasText: playerName }).locator('.player-status');

async function createHost(server: MockGameServer, browser: Browser, hostName: string): Promise<PlayerHandle> {
  const context = await browser.newContext();
  await server.attach(context);
  const page = await context.newPage();
  await page.goto('/');
  await page.getByLabel('Host name').fill(hostName);
  await page.getByRole('button', { name: 'Create lobby' }).click();
  return { context, page, name: hostName };
}

async function joinLobby(
  server: MockGameServer,
  browser: Browser,
  name: string,
  code: string,
): Promise<PlayerHandle> {
  const context = await browser.newContext();
  await server.attach(context);
  const page = await context.newPage();
  await page.goto('/');
  await page.getByLabel('Display name').fill(name);
  await page.getByLabel('Room code').fill(code);
  await page.getByRole('button', { name: 'Join lobby' }).click();
  return { context, page, name };
}

async function markReady(page: Page) {
  await page.getByRole('button', { name: /Ready up|Cancel ready|Updating…/ }).click();
}

test.describe('Multiplayer lobby behaviour', () => {
  test('ready players stay ready when a new player joins', async ({ browser }) => {
    const server = new MockGameServer({ presetCodes: ['ABCD'] });
    const handles: PlayerHandle[] = [];

    try {
      const host = await createHost(server, browser, 'Haley');
      handles.push(host);
      await expect(host.page).toHaveURL(/\/lobby\/ABCD$/);
      await expect(host.page.getByRole('heading', { name: lobbyHeading('ABCD') })).toBeVisible();

      const second = await joinLobby(server, browser, 'Riley', 'ABCD');
      handles.push(second);
      await expect(second.page.getByRole('heading', { name: lobbyHeading('ABCD') })).toBeVisible();
      await expect(host.page.getByText('Players: 2/8')).toBeVisible({ timeout: DEFAULT_TIMEOUT });

      await markReady(host.page);
      await expect(host.page.getByText('Ready: 1/2')).toBeVisible({ timeout: DEFAULT_TIMEOUT });
      await expect(playerStatusLocator(host.page, 'Haley')).toHaveText('Ready', { timeout: DEFAULT_TIMEOUT });

      await markReady(second.page);
      await expect(second.page.getByText('Ready: 2/2')).toBeVisible({ timeout: DEFAULT_TIMEOUT });
      await expect(playerStatusLocator(host.page, 'Riley')).toHaveText('Ready', { timeout: DEFAULT_TIMEOUT });

      const third = await joinLobby(server, browser, 'Quinn', 'ABCD');
      handles.push(third);
      await expect(third.page.getByRole('heading', { name: lobbyHeading('ABCD') })).toBeVisible();

      await expect(host.page.getByText('Ready: 2/3')).toBeVisible({ timeout: DEFAULT_TIMEOUT });
      await expect(playerStatusLocator(host.page, 'Haley')).toHaveText('Ready', { timeout: DEFAULT_TIMEOUT });
      await expect(playerStatusLocator(host.page, 'Riley')).toHaveText('Ready', { timeout: DEFAULT_TIMEOUT });
      await expect(playerStatusLocator(host.page, 'Quinn')).toHaveText('Waiting', { timeout: DEFAULT_TIMEOUT });
    } finally {
      await Promise.all(handles.map((handle) => handle.context.close()));
    }
  });

  test('non-host players follow the host into the round once it starts', async ({ browser }) => {
    const server = new MockGameServer({ presetCodes: ['WXYZ'] });
    const handles: PlayerHandle[] = [];

    try {
      const host = await createHost(server, browser, 'Morgan');
      handles.push(host);
      await expect(host.page.getByRole('heading', { name: lobbyHeading('WXYZ') })).toBeVisible();

      const second = await joinLobby(server, browser, 'Jamie', 'WXYZ');
      const third = await joinLobby(server, browser, 'Casey', 'WXYZ');
      handles.push(second, third);

      await expect(host.page.getByText('Players: 3/8')).toBeVisible({ timeout: DEFAULT_TIMEOUT });

      await host.page.getByRole('button', { name: 'Start round' }).click();
      await host.page.waitForURL(/\/round\/WXYZ$/, { timeout: DEFAULT_TIMEOUT });
      await expect(host.page.getByRole('heading', { name: 'Round 1' })).toBeVisible();

      await second.page.waitForURL(/\/round\/WXYZ$/, { timeout: DEFAULT_TIMEOUT });
      await expect(second.page.getByRole('heading', { name: 'Round 1' })).toBeVisible();

      await third.page.waitForURL(/\/round\/WXYZ$/, { timeout: DEFAULT_TIMEOUT });
      await expect(third.page.getByRole('heading', { name: 'Round 1' })).toBeVisible();
    } finally {
      await Promise.all(handles.map((handle) => handle.context.close()));
    }
  });

  test('non-host controls remain disabled in the lobby', async ({ browser }) => {
    const server = new MockGameServer({ presetCodes: ['PLAY'] });
    const handles: PlayerHandle[] = [];

    try {
      const host = await createHost(server, browser, 'Taylor');
      handles.push(host);
      await expect(host.page.getByRole('heading', { name: lobbyHeading('PLAY') })).toBeVisible();

      const guest = await joinLobby(server, browser, 'Avery', 'PLAY');
      handles.push(guest);
      await expect(guest.page.getByRole('heading', { name: lobbyHeading('PLAY') })).toBeVisible();

      await expect(guest.page.getByRole('button', { name: 'Start round' })).toBeDisabled();
      await expect(guest.page.getByRole('button', { name: 'Save rules' })).not.toBeVisible();
      await expect(guest.page.getByRole('button', { name: 'Ready up' })).toBeEnabled();
    } finally {
      await Promise.all(handles.map((handle) => handle.context.close()));
    }
  });
});
