import { expect, test } from '@playwright/test'
import type { Page, TestInfo } from '@playwright/test'

import {
  BUILD_IDS,
  INTEGRATION_IDS,
  NOTIFICATION_CHANNEL_IDS,
  PIPELINE_IDS,
  PROJECT_IDS,
} from '../src/demo/seed'

const DEMO_PASSWORD = 'owner'
const PERSONAS = {
  owner: 'demo+owner@oore.build',
  admin: 'demo+admin@oore.build',
  developer: 'demo+developer@oore.build',
  qa: 'demo+qa@oore.build',
} as const

const SCREENSHOT_VIEWPORTS = [
  { name: 'phone', width: 390, height: 844 },
  { name: 'tablet', width: 768, height: 1024 },
  { name: 'desktop', width: 1024, height: 768 },
  { name: 'wide', width: 1440, height: 900 },
] as const

const STABLE_OWNER_ROUTES = [
  { name: 'dashboard', path: '/' },
  { name: 'projects', path: '/projects' },
  {
    name: 'project-detail',
    path: `/projects/${PROJECT_IDS.flutterShop}`,
  },
  {
    name: 'pipeline-new',
    path: `/projects/${PROJECT_IDS.flutterShop}/pipelines/new`,
  },
  {
    name: 'pipeline-detail',
    path: `/projects/${PROJECT_IDS.flutterShop}/pipelines/${PIPELINE_IDS.shopAndroid}`,
  },
  {
    name: 'pipeline-edit',
    path: `/projects/${PROJECT_IDS.flutterShop}/pipelines/${PIPELINE_IDS.shopAndroid}/edit`,
  },
  { name: 'builds', path: '/builds' },
  { name: 'build-detail', path: `/builds/${BUILD_IDS.succeeded1}` },
  { name: 'users', path: '/settings/users' },
  { name: 'runners', path: '/settings/runners' },
  { name: 'audit-log', path: '/settings/audit-log' },
  { name: 'sources', path: '/settings/integrations' },
  {
    name: 'source-detail',
    path: `/settings/integrations/${INTEGRATION_IDS.github}`,
  },
  { name: 'source-github', path: '/settings/integrations/github' },
  { name: 'source-gitlab', path: '/settings/integrations/gitlab' },
  { name: 'source-local-git', path: '/settings/integrations/local-git' },
  { name: 'api-tokens', path: '/settings/api-tokens' },
  { name: 'preferences', path: '/settings/preferences' },
  { name: 'retention', path: '/settings/retention' },
  { name: 'notifications', path: '/settings/notifications' },
  { name: 'notification-new', path: '/settings/notifications/new' },
  {
    name: 'notification-detail',
    path: `/settings/notifications/${NOTIFICATION_CHANNEL_IDS.webhook}`,
  },
] as const

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(Navigator.prototype, 'doNotTrack', {
      configurable: true,
      get: () => '1',
    })
  })
  await page.route('**/v1/telemetry/web-performance', (route) =>
    route.fulfill({ status: 204 }),
  )
  await page.route('https://avatars.githubusercontent.com/**', (route) =>
    route.fulfill({
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="#737373"/><text x="32" y="39" text-anchor="middle" font-family="sans-serif" font-size="20" font-weight="700" fill="white">GH</text></svg>',
    }),
  )
})

async function signIn(page: Page, email: string): Promise<void> {
  await page.goto('/login')
  await expect(
    page.getByRole('heading', { name: 'Explore the Oore demo' }),
  ).toBeVisible()
  await page.getByLabel('Email').fill(email)
  await page.getByLabel('Password').fill(DEMO_PASSWORD)
  await page.getByRole('button', { name: /Sign in as/i }).click()
  await expect(page).toHaveURL(/\/$/)
  await expect(page.locator('#main-content')).toBeVisible()
}

async function waitForStableUi(page: Page): Promise<void> {
  await expect(page.locator('#main-content')).toBeVisible()
  await expect(
    page.getByRole('heading', { name: 'Something went wrong' }),
  ).toHaveCount(0)
  await expect(
    page.getByRole('heading', { name: 'Page not found' }),
  ).toHaveCount(0)
  await expect
    .poll(() => page.locator('[data-slot="skeleton"]').count())
    .toBe(0)
  await expect
    .poll(() => page.evaluate(() => document.fonts.status))
    .toBe('loaded')
  await expect
    .poll(() =>
      page.evaluate(() =>
        Array.from(document.images).every((image) => image.complete),
      ),
    )
    .toBe(true)
}

async function expectNoDocumentOverflow(
  page: Page,
  options: { soft?: boolean } = {},
): Promise<void> {
  const dimensions = await page.evaluate(() => ({
    viewport: window.innerWidth,
    document: document.documentElement.scrollWidth,
  }))
  const message = `${new URL(page.url()).pathname} must not overflow the document`
  if (options.soft) {
    expect
      .soft(dimensions.document, message)
      .toBeLessThanOrEqual(dimensions.viewport + 1)
    return
  }
  expect(dimensions.document, message).toBeLessThanOrEqual(
    dimensions.viewport + 1,
  )
}

async function screenshot(
  page: Page,
  testInfo: TestInfo,
  name: string,
): Promise<void> {
  await page.screenshot({
    path: testInfo.outputPath(`${name}.png`),
    fullPage: true,
    animations: 'disabled',
    caret: 'hide',
  })
}

test.describe('Chromium route screenshot matrix', () => {
  test.skip(({ browserName }) => browserName !== 'chromium')

  for (const viewport of SCREENSHOT_VIEWPORTS) {
    for (const theme of ['light', 'dark'] as const) {
      test(`${viewport.name} ${theme}`, async ({ page }, testInfo) => {
        const runtimeErrors: Array<string> = []
        page.on('pageerror', (error) => runtimeErrors.push(error.message))
        await page.setViewportSize(viewport)
        await page.addInitScript((value) => {
          localStorage.setItem('theme', value)
          localStorage.setItem('oore_welcomed', '1')
        }, theme)

        await page.goto('/login')
        await expect(
          page.getByRole('heading', { name: 'Explore the Oore demo' }),
        ).toBeVisible()
        await expect(page.locator('html')).toHaveClass(new RegExp(theme))
        await screenshot(page, testInfo, 'login')
        await signIn(page, PERSONAS.owner)

        for (const route of STABLE_OWNER_ROUTES) {
          await page.goto(route.path)
          await waitForStableUi(page)
          await screenshot(page, testInfo, route.name)
          await expectNoDocumentOverflow(page, { soft: true })
        }

        expect(runtimeErrors).toEqual([])
      })
    }
  }
})

test.describe('Chromium layout boundaries', () => {
  test.skip(({ browserName }) => browserName !== 'chromium')

  for (const viewport of [
    { width: 320, height: 568 },
    { width: 1920, height: 1080 },
  ]) {
    test(`${viewport.width}x${viewport.height}`, async ({ page }) => {
      await page.setViewportSize(viewport)
      await signIn(page, PERSONAS.owner)
      for (const path of ['/', '/projects', '/builds']) {
        await page.goto(path)
        await waitForStableUi(page)
        await expectNoDocumentOverflow(page)
      }
    })
  }
})

test.describe('role and direct-route policy', () => {
  test.skip(({ browserName }) => browserName !== 'chromium')

  for (const viewport of [
    { name: 'phone', width: 390, height: 844 },
    { name: 'desktop', width: 1440, height: 900 },
  ]) {
    for (const role of ['owner', 'admin'] as const) {
      test(`${role} full operator access on ${viewport.name}`, async ({
        page,
      }) => {
        await page.setViewportSize(viewport)
        await signIn(page, PERSONAS[role])
        for (const path of [
          '/settings/users',
          '/settings/notifications',
          `/projects/${PROJECT_IDS.flutterShop}/pipelines/new`,
        ]) {
          await page.goto(path)
          await waitForStableUi(page)
          await expect(page).toHaveURL(new RegExp(`${path}/?$`))
        }
      })
    }

    test(`developer project roles on ${viewport.name}`, async ({ page }) => {
      await page.setViewportSize(viewport)
      await signIn(page, PERSONAS.developer)

      await page.goto(
        `/projects/${PROJECT_IDS.flutterShop}/pipelines/${PIPELINE_IDS.shopAndroid}/edit`,
      )
      await waitForStableUi(page)
      await expect(page).toHaveURL(/\/edit$/)

      await page.goto(`/projects/${PROJECT_IDS.flutterShop}?tab=settings`)
      await waitForStableUi(page)
      await expect(
        page.getByRole('button', { name: 'Add project member' }),
      ).toBeVisible()

      await page.goto(
        `/projects/${PROJECT_IDS.nativePayments}/pipelines/${PIPELINE_IDS.paymentsAll}/edit`,
      )
      await expect(page).toHaveURL(
        new RegExp(`/projects/${PROJECT_IDS.nativePayments}/?$`),
      )

      await page.goto(`/projects/${PROJECT_IDS.nativePayments}?tab=settings`)
      await waitForStableUi(page)
      await expect(page.getByRole('button', { name: 'Add member' })).toHaveCount(0)

      await page.goto(`/builds/${BUILD_IDS.succeeded1}`)
      await waitForStableUi(page)
      await expect(page.getByRole('button', { name: 'Re-run' })).toBeVisible()

      await page.goto(`/builds/${BUILD_IDS.running2}`)
      await waitForStableUi(page)
      await expect(
        page.getByRole('button', { name: 'Cancel Build' }),
      ).toHaveCount(0)

      await page.goto(`/builds/${BUILD_IDS.succeeded6}`)
      await waitForStableUi(page)
      await expect(page.getByRole('button', { name: 'Re-run' })).toHaveCount(0)
      await expect(
        page.getByRole('button', { name: /Share options for/ }),
      ).toHaveCount(0)

      for (const path of ['/settings/integrations', '/settings/runners']) {
        await page.goto(path)
        await waitForStableUi(page)
        await expect(page).toHaveURL(new RegExp(`${path}/?$`))
      }

      for (const path of ['/settings/users', '/settings/notifications']) {
        await page.goto(path)
        await expect(page).toHaveURL(/\/$/)
      }
    })

    test(`QA tester-only routes on ${viewport.name}`, async ({ page }) => {
      await page.setViewportSize(viewport)
      await signIn(page, PERSONAS.qa)

      for (const path of ['/projects', '/builds', '/settings/users']) {
        await page.goto(path)
        await expect(page).toHaveURL(/\/$/)
      }

      await page.goto(`/builds/${BUILD_IDS.succeeded1}`)
      await waitForStableUi(page)
      await expect(page.getByRole('tab', { name: 'Release' })).toBeVisible()
      await expect(page.getByRole('tab', { name: 'Logs' })).toBeVisible()
    })
  }
})

test.describe('WebKit critical Safari surfaces', () => {
  test.skip(({ browserName }) => browserName !== 'webkit')

  for (const viewport of [
    { width: 390, height: 844 },
    { width: 768, height: 1024 },
    { width: 1440, height: 900 },
  ]) {
    test(`${viewport.width}px`, async ({ page }, testInfo) => {
      await page.setViewportSize(viewport)
      await signIn(page, PERSONAS.qa)
      await page.goto(`/builds/${BUILD_IDS.succeeded1}`)
      await waitForStableUi(page)

      const surfaces = await page.evaluate(() => {
        const background = (element: Element | null) =>
          element ? getComputedStyle(element).backgroundColor : ''
        return {
          html: background(document.documentElement),
          body: background(document.body),
          app: background(document.querySelector('#app')),
          appHeight:
            document.querySelector('#app')?.getBoundingClientRect().height ?? 0,
          viewportHeight: window.innerHeight,
        }
      })
      for (const color of [surfaces.html, surfaces.body, surfaces.app]) {
        expect(color).not.toBe('rgba(0, 0, 0, 0)')
        expect(color).not.toBe('transparent')
      }
      expect(surfaces.appHeight).toBeGreaterThanOrEqual(
        surfaces.viewportHeight - 1,
      )
      await expect(
        page.getByRole('button', { name: /Install|Download APK/ }),
      ).toBeVisible()
      await expectNoDocumentOverflow(page)
      await screenshot(page, testInfo, 'qa-release')
    })
  }
})

test.describe('Firefox core smoke', () => {
  test.skip(({ browserName }) => browserName !== 'firefox')

  test('operator routes', async ({ page }) => {
    await signIn(page, PERSONAS.owner)
    for (const path of ['/', '/projects', '/builds', '/settings/preferences']) {
      await page.goto(path)
      await waitForStableUi(page)
    }
  })

  test('QA routes', async ({ page }) => {
    await signIn(page, PERSONAS.qa)
    await waitForStableUi(page)
    await page.goto(`/builds/${BUILD_IDS.succeeded1}`)
    await waitForStableUi(page)
    await expect(page.getByRole('tab', { name: 'Release' })).toBeVisible()
  })
})
