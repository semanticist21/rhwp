import type { LayerStrokeDash } from '../core/types.ts';

// 긴 파선만 실제 획 굵기에 비례하고 기존 파선의 고정 픽셀 계약은 유지한다.
export function strokeDashIntervals(
  dash: LayerStrokeDash | undefined,
  width: number,
): number[] | null | undefined {
  switch (dash) {
    case undefined:
    case 'solid': return null;
    case 'dash': return [6, 3];
    case 'dot': return [2, 2];
    case 'dashDot': return [6, 3, 2, 3];
    case 'dashDotDot': return [6, 3, 2, 3, 2, 3];
    case 'longDash': {
      const values = [width * 24, width * 8];
      return width > 0 && values.every(value => Number.isFinite(value) && value > 0)
        ? values
        : undefined;
    }
    default: return undefined;
  }
}
