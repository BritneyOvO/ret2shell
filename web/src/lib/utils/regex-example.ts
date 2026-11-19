const ESCAPED_LITERAL = /[\\^$.*+?()[\]{}|]/;

function findClosingGroup(source: string, start: number): number {
  let depth = 0;
  let inClass = false;

  for (let index = start; index < source.length; index += 1) {
    const char = source[index];
    if (char === "\\") {
      index += 1;
      continue;
    }
    if (char === "[") {
      inClass = true;
      continue;
    }
    if (char === "]") {
      inClass = false;
      continue;
    }
    if (inClass) continue;
    if (char === "(") depth += 1;
    if (char === ")") {
      depth -= 1;
      if (depth === 0) return index;
    }
  }

  return -1;
}

function findTopLevelAlternative(source: string): number {
  let depth = 0;
  let inClass = false;

  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    if (char === "\\") {
      index += 1;
      continue;
    }
    if (char === "[") {
      inClass = true;
      continue;
    }
    if (char === "]") {
      inClass = false;
      continue;
    }
    if (inClass) continue;
    if (char === "(") depth += 1;
    if (char === ")") depth -= 1;
    if (char === "|" && depth === 0) return index;
  }

  return -1;
}

function classExample(source: string): string {
  const content = source.startsWith("^") ? source.slice(1) : source;
  if (content.includes("\\p{Han}") || content.includes("\\p{sc=Han}")) return "张";
  if (content.includes("\\d") || content.includes("0-9")) return "0";
  if (content.includes("A-Z")) return "A";
  if (content.includes("a-z")) return "a";

  for (let index = 0; index < content.length; index += 1) {
    if (content[index] === "\\" && index + 1 < content.length) return content[index + 1];
    if (content[index] !== "-") return content[index];
  }

  return "a";
}

function atomExample(source: string, start: number): { value: string; next: number } {
  const char = source[start];
  if (char === "[") {
    let end = start + 1;
    while (end < source.length) {
      if (source[end] === "\\") {
        end += 2;
        continue;
      }
      if (source[end] === "]") break;
      end += 1;
    }
    return { value: classExample(source.slice(start + 1, end)), next: Math.min(end + 1, source.length) };
  }

  if (char === "(") {
    const end = findClosingGroup(source, start);
    if (end < 0) return { value: "", next: source.length };
    let content = source.slice(start + 1, end);
    if (content.startsWith("?:") || content.startsWith("?=")) content = content.slice(2);
    if (content.startsWith("?!") || content.startsWith("?<=") || content.startsWith("?<!"))
      return { value: "", next: end + 1 };
    const alternative = findTopLevelAlternative(content);
    if (alternative >= 0) content = content.slice(0, alternative);
    return { value: generateRegexExample(content) || "", next: end + 1 };
  }

  if (char === "\\") {
    const escapeEnd =
      source[start + 1] === "p" && source[start + 2] === "{" ? source.indexOf("}", start + 3) + 1 : start + 2;
    const escapeSequence = source.slice(start, escapeEnd);
    if (escapeSequence === "\\d") return { value: "0", next: escapeEnd };
    if (escapeSequence === "\\w") return { value: "a", next: escapeEnd };
    if (escapeSequence === "\\s") return { value: " ", next: escapeEnd };
    if (escapeSequence === "\\p{Han}" || escapeSequence === "\\p{sc=Han}") {
      return { value: "张", next: escapeEnd };
    }
    if (/^\\[AbBzZ]$/.test(escapeSequence)) return { value: "", next: escapeEnd };
    return { value: escapeSequence.slice(1) || "", next: escapeEnd };
  }

  if (char === ".") return { value: "a", next: start + 1 };
  if (ESCAPED_LITERAL.test(char)) return { value: "", next: start + 1 };
  return { value: char, next: start + 1 };
}

function applyQuantifier(source: string, start: number, value: string): { value: string; next: number } {
  const char = source[start];
  if (char === "*") return { value: value || "a", next: start + 1 };
  if (char === "+") return { value: value || "a", next: start + 1 };
  if (char === "?") return { value: value || "a", next: start + 1 };
  if (char !== "{") return { value, next: start };

  const end = source.indexOf("}", start + 1);
  if (end < 0) return { value, next: start };
  const match = source.slice(start + 1, end).match(/^(\d+)(?:,\d*)?$/);
  if (!match) return { value, next: start };
  const count = Math.max(Number.parseInt(match[1], 10), 1);
  return { value: value.repeat(count), next: end + 1 };
}

export function generateRegexExample(pattern: string): string | null {
  let source = pattern.trim();
  if (!source) return null;
  if (source.startsWith("^") || source.startsWith("\\A")) source = source.replace(/^(?:\^|\\A)/, "");
  if (source.endsWith("$") || source.endsWith("\\z")) source = source.replace(/(?:\$|\\z)$/, "");

  let result = "";
  let index = 0;
  while (index < source.length) {
    if (source[index] === "|") break;
    if (source[index] === "$") break;
    const atom = atomExample(source, index);
    index = atom.next;
    const quantified = applyQuantifier(source, index, atom.value);
    result += quantified.value;
    index = quantified.next;
  }

  return result || null;
}
