import test from 'node:test';
import assert from 'node:assert/strict';
import { strokeDashIntervals } from '../src/view/stroke-dash.ts';

test('긴 파선은 실제 테두리 굵기의 24/8 패턴을 쓴다', () => {
  for (const width of [0.32, 0.80, 1.92, 18.88]) {
    assert.deepEqual(strokeDashIntervals('longDash', width), [24 * width, 8 * width]);
  }
});

test('아주 가는 선의 패턴을 paint 최소 굵기 0.3으로 올리지 않는다', () => {
  assert.deepEqual(strokeDashIntervals('longDash', 0.125), [3, 1]);
});

test('기존 dash 유형과 solid 계약은 굵기와 관계없이 유지한다', () => {
  for (const width of [0.32, 1, 18.88]) {
    assert.deepEqual(strokeDashIntervals('dash', width), [6, 3]);
    assert.deepEqual(strokeDashIntervals('dot', width), [2, 2]);
    assert.deepEqual(strokeDashIntervals('dashDot', width), [6, 3, 2, 3]);
    assert.deepEqual(strokeDashIntervals('dashDotDot', width), [6, 3, 2, 3, 2, 3]);
    assert.equal(strokeDashIntervals('solid', width), null);
    assert.equal(strokeDashIntervals(undefined, width), null);
  }
});

test('긴 파선은 0·음수·비유한 굵기를 CanvasKit에 전달하지 않는다', () => {
  for (const width of [0, -1, NaN, Infinity, -Infinity, Number.MAX_VALUE]) {
    assert.equal(strokeDashIntervals('longDash', width), undefined);
  }
});
