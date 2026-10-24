export const ALLOWED_SCHEMES = ["http:", "https:", "ftp:"] as const;
const MAX_LINK_LENGTH = 2048;

export interface LinkLine {
  /** 1-based line number in the textarea. */
  line: number;
  text: string;
  /** Null when the line is valid. */
  error: string | null;
}

export function validateLink(raw: string): string | null {
  const text = raw.trim();
  if (text.length > MAX_LINK_LENGTH) return "Link is too long.";
  let url: URL;
  try {
    url = new URL(text);
  } catch {
    return "Not a valid link.";
  }
  if (!(ALLOWED_SCHEMES as readonly string[]).includes(url.protocol)) {
    return "Only http, https and ftp links are supported.";
  }
  if (!url.hostname) return "The link has no host name.";
  return null;
}

/** Parse a multi-line box: one link per line, blank lines ignored, duplicates flagged. */
export function parseLinks(input: string): LinkLine[] {
  const seen = new Set<string>();
  const out: LinkLine[] = [];
  input.split(/\r?\n/).forEach((raw, index) => {
    const text = raw.trim();
    if (!text) return;
    let error = validateLink(text);
    if (!error) {
      if (seen.has(text)) error = "Duplicate of an earlier line.";
      seen.add(text);
    }
    out.push({ line: index + 1, text, error });
  });
  return out;
}
