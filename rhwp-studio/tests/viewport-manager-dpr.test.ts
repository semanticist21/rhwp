import test, { type TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { getEventListeners } from 'node:events';
import { EventBus } from '../src/core/event-bus.ts';
import { ViewportManager } from '../src/view/viewport-manager.ts';

function fixture(t: TestContext) {
  const previous = Object.getOwnPropertyDescriptors(globalThis);
  const timers = new Map<number, () => void>();
  const queries: Array<EventTarget & { media: string; matches: boolean; ratio: number }> = [];
  const observers: Array<{ callback: ResizeObserverCallback; connected: boolean }> = [];
  let nextId = 0;
  const browser = Object.assign(new EventTarget(), {
    devicePixelRatio: 1,
    setTimeout(callback: () => void) { timers.set(++nextId, callback); return nextId; },
    matchMedia(media: string) {
      const match = /^\(resolution: ([0-9.]+)dppx\)$/.exec(media);
      assert.ok(match);
      const ratio = Number(match[1]);
      const query = Object.assign(new EventTarget(), { media, ratio, matches: ratio === browser.devicePixelRatio });
      queries.push(query);
      return query as unknown as MediaQueryList;
    },
  });
  const bus = new EventBus();
  const viewport = new ViewportManager(bus);
  const events: number[][] = [];
  bus.on('viewport-resize', (width, height) => events.push([Number(width), Number(height)]));
  Object.defineProperty(globalThis, 'window', { configurable: true, value: browser });
  Object.defineProperty(globalThis, 'ResizeObserver', { configurable: true, value: class {
    private entry: { callback: ResizeObserverCallback; connected: boolean };
    constructor(callback: ResizeObserverCallback) { this.entry = { callback, connected: false }; observers.push(this.entry); }
    observe() { this.entry.connected = true; }
    disconnect() { this.entry.connected = false; }
  } });
  Object.defineProperty(globalThis, 'clearTimeout', { configurable: true, value: (id: number) => timers.delete(id) });
  t.after(() => {
    viewport.detach();
    for (const name of ['window', 'ResizeObserver', 'clearTimeout']) {
      const descriptor = previous[name];
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else Reflect.deleteProperty(globalThis, name);
    }
  });
  const container = Object.assign(new EventTarget(), { clientWidth: 800, clientHeight: 600 });
  const flush = () => { const pending = [...timers.values()]; timers.clear(); for (const callback of pending) callback(); };
  const changeDpr = (ratio: number) => {
    browser.devicePixelRatio = ratio;
    for (const query of [...queries]) {
      const matches = query.ratio === ratio;
      if (matches !== query.matches) { query.matches = matches; query.dispatchEvent(new Event('change')); }
    }
  };
  const resize = () => { for (const entry of observers) if (entry.connected) entry.callback([], {} as ResizeObserver); };
  const listeners = () => queries.reduce((count, query) => count + getEventListeners(query, 'change').length, 0);
  const attach = (target = container) => viewport.attachTo(target as unknown as HTMLElement);
  return { viewport, container, events, timers, browser, attach, changeDpr, resize, flush, listeners };
}

test('CSS 크기가 같은 DPR 단독 변경도 공통 viewport 갱신을 예약하고 반복 전환을 따라간다', t => {
  const f = fixture(t);
  f.attach();
  assert.deepEqual(f.events, []);
  for (const ratio of [2, 3, 1.25, 1]) {
    f.changeDpr(ratio);
    assert.equal(f.events.length, [2, 3, 1.25, 1].indexOf(ratio));
    f.flush();
    assert.deepEqual(f.events.at(-1), [800, 600]);
    assert.equal(f.listeners(), 1, '이전 해상도의 구독은 남기지 않는다');
  }
  assert.equal(f.events.length, 4);
  f.changeDpr(1);
  f.flush();
  assert.equal(f.events.length, 4, '변화 없는 해상도는 갱신하지 않는다');
});

test('DPR와 ResizeObserver 알림은 같은 예약을 공유하고 최신 container 크기를 읽는다', t => {
  const f = fixture(t);
  f.attach();
  f.changeDpr(2);
  f.container.clientWidth = 920;
  f.resize();
  f.changeDpr(3);
  f.resize();
  assert.equal(f.timers.size, 1);
  f.flush();
  assert.deepEqual(f.events, [[920, 600]]);
  f.container.clientHeight = 720;
  f.resize();
  f.flush();
  assert.deepEqual(f.events, [[920, 600], [920, 720]], '일반 resize도 그대로 동작한다');
});

test('detach는 해상도 구독과 대기 중인 viewport 갱신을 모두 해제한다', t => {
  const f = fixture(t);
  f.attach();
  f.changeDpr(2);
  assert.equal(f.timers.size, 1);
  f.viewport.detach();
  assert.equal(f.listeners(), 0);
  assert.equal(f.timers.size, 0);
  f.changeDpr(3);
  f.resize();
  f.flush();
  assert.deepEqual(f.events, []);
});

test('다른 container로 다시 연결하면 현재 DPR부터 관찰하고 예전 예약을 전달하지 않는다', t => {
  const f = fixture(t);
  f.attach();
  f.changeDpr(2);
  const second = Object.assign(new EventTarget(), { clientWidth: 1100, clientHeight: 850 });
  f.attach(second);
  assert.equal(f.listeners(), 1);
  f.flush();
  assert.deepEqual(f.events, []);
  f.changeDpr(1);
  f.flush();
  assert.deepEqual(f.events, [[1100, 850]]);
  assert.deepEqual(f.viewport.getViewportSize(), { width: 1100, height: 850 });
});
