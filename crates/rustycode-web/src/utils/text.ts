const FILE_PATH_RE = /([\w./-]+\.(?:json|jsx|tsx|ts|js|toml|yaml|yml|cpp|rs|py|md|css|html|sql|sh|go|java|c|h))(?::?\d*(?::?\d*)?)/g;

export function splitByFilePaths(text: string): Array<{ text: string; highlight: boolean }> {
  const segments: Array<{ text: string; highlight: boolean }> = [];
  let last = 0;
  for (const m of text.matchAll(FILE_PATH_RE)) {
    const idx = m.index!;
    if (idx > last) {
      segments.push({ text: text.slice(last, idx), highlight: false });
    }
    segments.push({ text: m[0], highlight: true });
    last = idx + m[0].length;
  }
  if (last < text.length) {
    segments.push({ text: text.slice(last), highlight: false });
  }
  return segments.length ? segments : [{ text, highlight: false }];
}

export function fuzzyMatch(query: string, text: string): boolean {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (t.includes(q)) return true;
  let qi = 0;
  for (let ti = 0; ti < t.length && qi < q.length; ti++) {
    if (t[ti] === q[qi]) qi++;
  }
  return qi === q.length;
}
