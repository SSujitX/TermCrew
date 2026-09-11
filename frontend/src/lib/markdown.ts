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
  for (const line of m[1].split(/\r?\n/)) {
    const i = line.indexOf(':');
    if (i <= 0) continue;
    const key = line.slice(0, i).trim();
    const value = line.slice(i + 1).trim();
    if (key) fields.push([key, value]);
  }
  return { fields, body: m[2] };
}

export function renderMarkdown(src: string): string {
  const html = marked.parse(src, { async: false });
  return typeof html === 'string' ? html : '';
}

export function renderSkillMarkdown(src: string): string {
  const { fields, body } = splitFrontmatter(src);
  let head = '';
  if (fields.length > 0) {
    const rows = fields
      .map(
        ([k, v]) =>
          `<tr><th>${escapeHtml(k)}</th><td>${escapeHtml(v)}</td></tr>`,
      )
      .join('');
    head = `<table class="md-frontmatter"><tbody>${rows}</tbody></table>`;
  }
  return head + renderMarkdown(body);
}
