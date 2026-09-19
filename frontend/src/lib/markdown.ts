import { Marked } from 'marked';

const marked = new Marked({
  gfm: true,
  breaks: false,
  renderer: {
    html({ text }: { text: string }) {
      return escapeHtml(text);
    },
  },
});

function escapeHtml(s: string): string {
  return s
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

export function splitFrontmatter(src: string): { fields: [string, string][]; body: string } {
  const m = src.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/);
  if (!m) return { fields: [], body: src };
  const fields: [string, string][] = [];
  const lines = m[1].split(/\r?\n/);
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    const colon = line.indexOf(':');
    if (colon <= 0 || /^\s/.test(line)) {
      i += 1;
      continue;
    }
    const key = line.slice(0, colon).trim();
    let value = line.slice(colon + 1).trim();
    if (!key) {
      i += 1;
      continue;
    }
    if (value === '>' || value === '|' || value === '>-' || value === '|-') {
      const chunk: string[] = [];
      i += 1;
      while (i < lines.length) {
        const next = lines[i];
        if (/^\s/.test(next)) {
          chunk.push(next.trim());
          i += 1;
          continue;
        }
        if (next.trim() === '') {
          i += 1;
          continue;
        }
        break;
      }
      value = chunk.join(' ').replace(/\s+/g, ' ').trim();
      fields.push([key, value]);
      continue;
    }
    fields.push([key, value]);
    i += 1;
  }
  return { fields, body: m[2] };
}

export function renderMarkdown(src: string): string {
  const html = marked.parse(src, { async: false });
  return typeof html === 'string' ? html : '';
}

export function renderSkillMarkdown(src: string): string {
  const { fields, body } = splitFrontmatter(src);
  const name = fields.find(([k]) => k.toLowerCase() === 'name')?.[1];
  const license = fields.find(([k]) => k.toLowerCase() === 'license')?.[1];
  const bits = [name, license].filter((v) => v && v !== '>' && v !== '|');
  const head =
    bits.length > 0
      ? `<p class="md-meta">${bits.map(escapeHtml).join(' · ')}</p>`
      : '';
  return head + renderMarkdown(body);
}
