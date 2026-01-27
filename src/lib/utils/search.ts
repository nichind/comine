export function levenshtein(a: string, b: string): number {
  if (a === b) return 0;
  if (a.length === 0) return b.length;
  if (b.length === 0) return a.length;

  let row = new Array(a.length + 1);
  let prevRow = new Array(a.length + 1);

  for (let i = 0; i <= a.length; i++) {
    prevRow[i] = i;
  }

  for (let i = 0; i < b.length; i++) {
    row[0] = i + 1;
    for (let j = 0; j < a.length; j++) {
      const cost = a[j] === b[i] ? 0 : 1;
      row[j + 1] = Math.min(row[j] + 1, prevRow[j + 1] + 1, prevRow[j] + cost);
    }
    const temp = prevRow;
    prevRow = row;
    row = temp;
  }

  return prevRow[a.length];
}
function fuzzyTokenMatch(token: string, targetWord: string): number {
  if (targetWord.includes(token)) {
    if (targetWord.startsWith(token)) return 1.0;
    return 0.9;
  }

  if (Math.abs(token.length - targetWord.length) > 3) return 0;

  const dist = levenshtein(token, targetWord);

  const maxErrors = Math.min(Math.floor(token.length / 3) + 1, 3);

  if (dist <= maxErrors) {
    return 0.7 - dist * 0.15;
  }

  return 0;
}
export function calculateMatchScore(text: string, query: string): number {
  const normText = text.toLowerCase().trim();
  const normQuery = query.toLowerCase().trim();

  if (!normQuery) return 1;

  if (normText === normQuery) return 100;

  if (normText.includes(normQuery)) {
    if (normText.startsWith(normQuery)) return 50;
    const wordBoundaryIndex = normText.indexOf(' ' + normQuery);
    if (wordBoundaryIndex !== -1) return 40;
    return 30;
  }

  const textWords = normText.split(/\s+/);
  const queryTokens = normQuery.split(/\s+/);

  let totalScore = 0;

  for (const token of queryTokens) {
    let bestTokenScore = 0;

    for (const word of textWords) {
      const score = fuzzyTokenMatch(token, word);
      if (score > bestTokenScore) {
        bestTokenScore = score;
      }
    }

    if (bestTokenScore === 0) return 0;

    totalScore += bestTokenScore;
  }

  return totalScore / queryTokens.length;
}
