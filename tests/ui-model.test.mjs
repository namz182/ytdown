import test from 'node:test';
import assert from 'node:assert/strict';
import { recentCompleted, fitSidebars } from '../src/ui-model.ts';

test('Home shows two completed downloads in completion order', () => {
  const tasks = [
    { id: 'active', status: 'Downloading', created: 500 },
    { id: 'failed', status: 'Failed', created: 450 },
    { id: 'recent', status: 'Completed', created: 100, completedAt: 600 },
    { id: 'older', status: 'Completed', created: 200, completedAt: 400 },
    { id: 'oldest', status: 'Completed', created: 50, completedAt: 300 },
    { id: 'queued', status: 'Queued', created: 700 },
  ];
  assert.deepEqual(recentCompleted(tasks).map(t => t.id), ['recent', 'older']);
  assert.equal(tasks[0].id, 'active');
});

test('Old completed entries use their saved creation time', () => {
  assert.deepEqual(recentCompleted([
    {status: 'Completed', created: 1}, {status: 'Completed', created: 3}, {status: 'Completed', created: 2},
  ]).map(task => task.created), [3, 2]);
  assert.deepEqual(recentCompleted([{status: 'Cancelled', created: 10}]), []);
});

test('Resizing preserves room for the main view and enforces pane limits', () => {
  for (const width of [900, 1096, 1400]) {
    for (const left of [-200, 208, 900]) {
      for (const right of [-100, 280, 900]) {
        const fit = fitSidebars(width, left, right);
        assert.ok(fit.left >= 160 && fit.left <= 320);
        assert.ok(fit.right >= 220 && fit.right <= 420);
        assert.ok(width - fit.left - fit.right - 24 >= 360);
      }
    }
  }
});

test('Hidden right pane and invalid stored sizes stay usable', () => {
  assert.deepEqual(fitSidebars(800, 208, 280, false), {left: 208, right: 0});
  assert.deepEqual(fitSidebars(1096, NaN, Infinity), {left: 208, right: 280});
});

test('The sidebar is a single newest-first queue across all statuses', async () => {
  const { unifiedQueue } = await import('../src/ui-model.ts');
  const tasks = [
    {id: 'old-active', created: 1, status: 'Downloading'},
    {id: 'new-complete', created: 4, status: 'Completed'},
    {id: 'newest-failed', created: 5, status: 'Failed'},
    {id: 'dismissed', created: 8, dismissed: true},
  ];
  assert.deepEqual(unifiedQueue(tasks).map(t => t.id), ['newest-failed', 'new-complete', 'old-active']);
  assert.equal(tasks[0].id, 'old-active');
  assert.deepEqual(unifiedQueue([{created: 1, dismissed: true}]), []);
});
