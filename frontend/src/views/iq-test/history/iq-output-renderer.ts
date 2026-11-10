import { katex } from '@mdit/plugin-katex';
import DOMPurify from 'dompurify';
import MarkdownIt from 'markdown-it';

const markdown = new MarkdownIt({ html: false, linkify: false, breaks: true }).use(katex, {
  delimiters: 'all',
  trust: false,
  throwOnError: false,
});

export type OutputBlock =
  { kind: 'markdown'; html: string } | { kind: 'code'; language: string; content: string };

export function outputBlocks(source: string): OutputBlock[] {
  if (/^\s*(?:<!doctype\s+html|<html\b|<svg\b)/i.test(source))
    return [
      { kind: 'code', language: /^\s*<svg\b/i.test(source) ? 'svg' : 'html', content: source },
    ];
  // Some upstreams double-escape math delimiters. Preserve code verbatim.
  let fence = '';
  const normalized = source
    .split('\n')
    .map((line) => {
      const marker = line.match(/^\s*(`{3,}|~{3,})/)?.[1];
      if (marker) {
        if (!fence) fence = marker;
        else if (marker[0] === fence[0] && marker.length >= fence.length) fence = '';
        return line;
      }
      return fence ? line : line.replace(/\\\\(?=[()[\]])/g, '\\');
    })
    .join('\n');
  const tokens = markdown.parse(normalized, {});
  const blocks: OutputBlock[] = [];
  let start = 0;
  const append = (end: number) => {
    if (end <= start) return;
    const html = markdown.renderer.render(tokens.slice(start, end), markdown.options, {});
    blocks.push({
      kind: 'markdown',
      html: DOMPurify.sanitize(html, { FORBID_TAGS: ['img', 'iframe', 'form', 'script'] }),
    });
  };
  tokens.forEach((token, index) => {
    if ((token.type !== 'fence' && token.type !== 'code_block') || token.level !== 0) return;
    append(index);
    blocks.push({
      kind: 'code',
      language: token.info.trim().split(/\s/)[0] ?? '',
      content: token.content,
    });
    start = index + 1;
  });
  append(tokens.length);
  return blocks;
}

export function previewDocument(source: string, channel: string): string {
  const policy =
    "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";
  const identity = JSON.stringify(channel).replace(/</g, '\\u003c');
  // The sandbox keeps its opaque origin. Only the measured height crosses to the host.
  const resizeScript = `<script>(() => {
    const channel = ${identity};
    let pending = false;
    let previous = 0;
    const measure = () => {
      pending = false;
      const body = document.body;
      if (!body) return;
      // Measure natural document flow, not a min-height:100vh feedback loop.
      document.documentElement.style.setProperty('height', 'auto', 'important');
      document.documentElement.style.setProperty('min-height', '0', 'important');
      document.documentElement.style.setProperty('overflow', 'hidden', 'important');
      body.style.setProperty('height', 'auto', 'important');
      body.style.setProperty('min-height', '0', 'important');
      const style = getComputedStyle(body);
      const margins = (parseFloat(style.marginTop) || 0) + (parseFloat(style.marginBottom) || 0);
      const height = Math.ceil(Math.max(body.scrollHeight, body.getBoundingClientRect().height) + margins);
      if (height > 0 && height !== previous) {
        previous = height;
        parent.postMessage({ type: 'iq-preview-size', channel, height }, '*');
      }
    };
    const schedule = () => {
      if (!pending) { pending = true; requestAnimationFrame(measure); }
    };
    addEventListener('DOMContentLoaded', () => {
      const observer = new ResizeObserver(schedule);
      observer.observe(document.documentElement);
      observer.observe(document.body);
      document.fonts.ready.then(schedule);
      schedule();
    }, { once: true });
    addEventListener('load', schedule, true);
    addEventListener('resize', schedule);
  })();</script>`;
  return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${policy}"><meta name="referrer" content="no-referrer">${resizeScript}</head><body>${source}</body></html>`;
}
