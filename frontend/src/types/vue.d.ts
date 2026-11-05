import type { vOverflowTooltip } from '@/directives/overflow-tooltip';

declare module 'vue' {
  interface GlobalDirectives {
    vOverflowTooltip: typeof vOverflowTooltip;
  }
}
