// Small, safe Markdown renderer for task notes (no dependencies, works offline).
// Safety by construction: all text is HTML-escaped first, only a fixed set of
// tags is ever produced, links are limited to http(s)/mailto, and images only
// to attached photos ("photo:<id>"). `sanitize` then re-checks the output with
// an allowlist (tags + attributes) as a second line of defence.
//
// Supported: # ## ### headings, paragraphs, line breaks, **bold**, *italic* /
// _italic_, ~~strike~~, `code`, ``` fenced code ```, [text](url), bare URLs,
// - / * / 1. lists, - [ ] / - [x] checklists, > quotes, --- rules, ![alt](photo:id).

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");

function safeUrl(raw: string): string | null {
  const u = raw.trim().replace(/&amp;/g, "&");
  if (/^(https?:\/\/|mailto:)/i.test(u) && !/[\s<>"']/.test(u)) return esc(u);
  return null;
}

/** Inline markdown on one already-escaped line. */
function inline(s: string): string {
  const codes: string[] = [];
  s = s.replace(/`([^`]+)`/g, (_, c) => `\u0000${codes.push(c) - 1}\u0000`);
  // photos: ![alt](photo:id)
  s = s.replace(/!\[([^\]]*)\]\(photo:([A-Za-z0-9-]+)\)/g, (_, alt, id) => `<img data-photo="${id}" alt="${alt}">`);
  // links [text](url)
  s = s.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (m, text, url) => {
    const u = safeUrl(url);
    return u ? `<a href="${u}">${text}</a>` : m;
  });
  // bare URLs (not already inside an attribute or link text)
  s = s.replace(/(^|[\s(])(https?:\/\/[^\s<]+[^\s<.,;:!?)])/g, (m, pre, url) => {
    const u = safeUrl(url);
    return u ? `${pre}<a href="${u}">${url}</a>` : m;
  });
  s = s.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>").replace(/__([^_]+)__/g, "<strong>$1</strong>");
  s = s.replace(/(^|[^*\w])\*([^*\s][^*]*)\*/g, "$1<em>$2</em>").replace(/(^|[^_\w])_([^_\s][^_]*)_(?!\w)/g, "$1<em>$2</em>");
  s = s.replace(/~~([^~]+)~~/g, "<del>$1</del>");
  return s.replace(/\u0000(\d+)\u0000/g, (_, i) => `<code>${codes[+i]}</code>`);
}

export function renderMarkdown(src: string): string {
  const lines = esc(src.replace(/\r\n?/g, "\n")).split("\n");
  const out: string[] = [];
  let para: string[] = [];
  let list: { type: "ul" | "ol"; items: string[] } | null = null;
  const flushPara = () => {
    if (para.length) out.push(`<p>${para.map(inline).join("<br>")}</p>`);
    para = [];
  };
  const flushList = () => {
    if (list) out.push(`<${list.type}>${list.items.join("")}</${list.type}>`);
    list = null;
  };
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i];
    if (/^```/.test(l)) {
      flushPara(); flushList();
      const code: string[] = [];
      while (++i < lines.length && !/^```/.test(lines[i])) code.push(lines[i]);
      out.push(`<pre><code>${code.join("\n")}</code></pre>`);
      continue;
    }
    let m: RegExpExecArray | null;
    if (!l.trim()) { flushPara(); flushList(); continue; }
    if ((m = /^(#{1,3})\s+(.*)$/.exec(l))) {
      flushPara(); flushList();
      const n = m[1].length + 2; // h3..h5: notes headings stay small
      out.push(`<h${n}>${inline(m[2])}</h${n}>`);
      continue;
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(l)) { flushPara(); flushList(); out.push("<hr>"); continue; }
    if ((m = /^&gt;\s?(.*)$/.exec(l))) { flushPara(); flushList(); out.push(`<blockquote>${inline(m[1])}</blockquote>`); continue; }
    if ((m = /^\s*[-*+]\s+\[( |x|X)\]\s+(.*)$/.exec(l))) {
      flushPara();
      if (!list || list.type !== "ul") { flushList(); list = { type: "ul", items: [] }; }
      const on = m[1] !== " ";
      list.items.push(`<li class="task"><input type="checkbox" disabled${on ? " checked" : ""}> ${inline(m[2])}</li>`);
      continue;
    }
    if ((m = /^\s*[-*+]\s+(.*)$/.exec(l))) {
      flushPara();
      if (!list || list.type !== "ul") { flushList(); list = { type: "ul", items: [] }; }
      list.items.push(`<li>${inline(m[1])}</li>`);
      continue;
    }
    if ((m = /^\s*\d+[.)]\s+(.*)$/.exec(l))) {
      flushPara();
      if (!list || list.type !== "ol") { flushList(); list = { type: "ol", items: [] }; }
      list.items.push(`<li>${inline(m[1])}</li>`);
      continue;
    }
    flushList();
    para.push(l);
  }
  flushPara(); flushList();
  return sanitize(out.join(""));
}

const TAGS = new Set(["P", "BR", "STRONG", "EM", "DEL", "CODE", "PRE", "A", "UL", "OL", "LI", "H3", "H4", "H5", "HR", "BLOCKQUOTE", "IMG", "INPUT"]);
const ATTRS: Record<string, string[]> = { A: ["href"], IMG: ["data-photo", "alt"], INPUT: ["type", "disabled", "checked"], LI: ["class"] };

/** Allowlist sanitizer: drops unknown elements (keeping their text) and attributes. */
export function sanitize(html: string): string {
  const doc = new DOMParser().parseFromString(`<div>${html}</div>`, "text/html");
  const root = doc.body.firstElementChild as HTMLElement;
  const walk = (el: Element) => {
    for (const c of [...el.children]) {
      if (!TAGS.has(c.tagName)) {
        c.replaceWith(doc.createTextNode(c.textContent ?? ""));
        continue;
      }
      for (const a of [...c.attributes]) {
        if (!(ATTRS[c.tagName] ?? []).includes(a.name)) c.removeAttribute(a.name);
      }
      if (c.tagName === "A" && !/^(https?:\/\/|mailto:)/i.test(c.getAttribute("href") ?? "")) c.removeAttribute("href");
      if (c.tagName === "IMG" && !/^[A-Za-z0-9-]+$/.test(c.getAttribute("data-photo") ?? "")) c.remove();
      if (c.tagName === "INPUT" && c.getAttribute("type") !== "checkbox") c.remove();
      walk(c);
    }
  };
  walk(root);
  return root.innerHTML;
}

/** Photo ids referenced inline in the notes. */
export const inlinePhotos = (src?: string) => [...(src ?? "").matchAll(/!\[[^\]]*\]\(photo:([A-Za-z0-9-]+)\)/g)].map((m) => m[1]);
