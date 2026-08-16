import { describe, expect, it } from 'vitest';
import {
  formatIntelligenceEffort,
  formatIntelligenceModelName,
  formatIntelligenceTableEffort,
} from './intelligence-ladder-presentation';

describe('intelligence ladder presentation', () => {
  it('formats known GPT slugs without changing unknown or untrusted text', () => {
    expect(formatIntelligenceModelName('gpt-5.6-sol')).toBe('GPT-5.6 Sol');
    expect(formatIntelligenceModelName('gpt-5.6')).toBe('GPT-5.6');
    expect(formatIntelligenceModelName('Claude 4.1 Opus')).toBe('Claude 4.1 Opus');
    expect(formatIntelligenceModelName('<script>')).toBe('<script>');
    expect(formatIntelligenceEffort('xhigh')).toBe('XHIGH');
    expect(formatIntelligenceTableEffort('XHIGH')).toBe('xhigh');
  });
});
