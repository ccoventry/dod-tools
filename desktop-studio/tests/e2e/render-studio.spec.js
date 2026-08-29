// Render Studio frontend behavior, driven against tests/e2e/render-studio.html
// (a minimal harness — see the comment at the top of that file and
// tests/e2e/README.md for what this suite covers and what it doesn't).
//
// Scoped to what's actually on this branch's render_pane.js — general
// harness/mock smoke coverage (does a snapshot render correctly, does a
// button click reach the right IPC call with the right arguments). Feature
// branches that change render_pane.js's own behavior (e.g. the keyed-DOM
// reconciliation and Skip toggle added on feature/obs-capture) should add
// their own tests here alongside that code, once this suite is available to
// them — a test for behavior that does not exist yet on this branch would
// just be a test that fails here for the wrong reason.
import { test, expect } from '@playwright/test';

/** A RenderJobView-shaped fixture with sane defaults, overridable per test. */
function job(overrides = {}) {
  return {
    id: '0',
    name: 'demo1-chain_01_b0',
    stream: 'all',
    frames: 100,
    date: '2026-08-29 10:00 AM',
    status: 'Queued',
    speed: '',
    progress: 0,
    error_log: null,
    settings_summary: 'ProRes @ 300fps',
    output_path: '',
    take_folder: 'C:\\captures\\demo1\\chain_01_b0\\take0000',
    ...overrides,
  };
}

async function gotoHarness(page) {
  await page.goto('/tests/e2e/render-studio.html');
  await page.waitForFunction(() => window.__renderPaneReady === true);
}

async function emitSnapshot(page, jobs) {
  await page.evaluate((j) => window.__mockEmit('render_jobs_snapshot', j), jobs);
}

function invocationsFor(page, cmd) {
  return page.evaluate(
    (c) => window.__mockInvocations.filter((i) => i.cmd === c).map((i) => i.args),
    cmd,
  );
}

/** Cell locators by position — this branch's render_pane.js doesn't tag
 *  cells with their own classes, so this pins the column order down in one
 *  place rather than six `nth-child` calls scattered through the tests. */
function cells(page, jobId) {
  const row = page.locator(`tr[data-job-id="${jobId}"]`);
  return {
    row,
    name: row.locator('td:nth-child(1)'),
    status: row.locator('td:nth-child(6)'),
    progressFill: row.locator('td:nth-child(8) .progress-bar-fill'),
    cancelBtn: row.locator('.render-job-cancel-btn'),
    resetBtn: row.locator('.render-job-reset-btn'),
  };
}

test.describe('empty and basic rendering', () => {
  test('shows the empty-state row before any snapshot arrives', async ({ page }) => {
    await gotoHarness(page);
    await expect(page.locator('#render-jobs-tbody td.table-empty')).toBeVisible();
  });

  test('a snapshot renders one row per job with the right content', async ({ page }) => {
    await gotoHarness(page);
    await emitSnapshot(page, [
      job({ id: '0', name: 'clip-a', status: 'Rendering', progress: 42 }),
      job({ id: '1', name: 'clip-b', status: 'Finished', progress: 100 }),
    ]);

    await expect(page.locator('#render-jobs-tbody tr[data-job-id]')).toHaveCount(2);
    const a = cells(page, '0');
    await expect(a.name).toHaveText('clip-a');
    await expect(a.status).toHaveText('Rendering');
    await expect(a.progressFill).toHaveAttribute('style', /width:\s*42%/);
    await expect(cells(page, '1').status).toHaveText('Finished');
  });

  test('the empty-state row is replaced once jobs exist, and comes back once they are cleared', async ({ page }) => {
    await gotoHarness(page);
    await emitSnapshot(page, [job()]);
    await expect(page.locator('#render-jobs-tbody td.table-empty')).toHaveCount(0);
    await emitSnapshot(page, []);
    await expect(page.locator('#render-jobs-tbody td.table-empty')).toBeVisible();
  });
});

test.describe('job row actions reach the backend correctly', () => {
  test('Cancel is shown for a Queued or Rendering job, and calls cancel_render_job with that job\'s id', async ({ page }) => {
    await gotoHarness(page);
    await emitSnapshot(page, [job({ id: '0', status: 'Rendering' })]);
    await cells(page, '0').cancelBtn.click();
    expect(await invocationsFor(page, 'cancel_render_job')).toEqual([{ jobId: '0' }]);
  });

  test('Reset (not Cancel) is shown for a Finished job, and calls reset_render_job', async ({ page }) => {
    await gotoHarness(page);
    await emitSnapshot(page, [job({ id: '0', status: 'Finished', progress: 100 })]);
    await expect(cells(page, '0').cancelBtn).toHaveCount(0);
    await cells(page, '0').resetBtn.click();
    expect(await invocationsFor(page, 'reset_render_job')).toEqual([{ jobId: '0' }]);
  });

  test('clicking across several rows targets the correct id for each, not a neighbor\'s', async ({ page }) => {
    await gotoHarness(page);
    const ids = ['0', '1', '2', '3'];
    await emitSnapshot(page, ids.map((id) => job({ id, status: 'Rendering' })));

    for (const id of ids) {
      await cells(page, id).cancelBtn.click();
    }

    const calledIds = (await invocationsFor(page, 'cancel_render_job')).map((a) => a.jobId).sort();
    expect(calledIds).toEqual(ids);
  });
});
