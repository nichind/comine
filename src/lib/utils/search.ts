
/**
 * Calculates the Levenshtein distance between two strings using the iterative matrix approach.
 * Memory optimized to use only two rows.
 */
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
      row[j + 1] = Math.min(
        row[j] + 1,        // insertion
        prevRow[j + 1] + 1,// deletion
        prevRow[j] + cost  // substitution
      );
    }
    const temp = prevRow;
    prevRow = row;
    row = temp;
  }

  return prevRow[a.length];
}

/**
 * Checks if a query token fuzzy matches a target word.
 * Allows:
 * 1. Substring matches (highest priority)
 * 2. Levenshtein distance based on length (typo tolerance)
 */
function fuzzyTokenMatch(token: string, targetWord: string): number {
  // 1. Exact substring match (Bonus if starts with)
  if (targetWord.includes(token)) {
    if (targetWord.startsWith(token)) return 1.0;
    return 0.9;
  }

  // 2. Typo tolerance (Levenshtein)
  // Only check if lengths are somewhat similar to avoid comparing "a" with "supercalifragilistic"
  if (Math.abs(token.length - targetWord.length) > 3) return 0;

  const dist = levenshtein(token, targetWord);
  
  // Rule of thumb: Allow 1 error per 3 characters, max 3 errors
  const maxErrors = Math.min(Math.floor(token.length / 3) + 1, 3);
  
  if (dist <= maxErrors) {
    // Score decreases with distance
    return 0.7 - (dist * 0.15); 
  }

  return 0;
}

/**
 * Smart search scoring function.
 * Returns a score > 0 if the query matches the text.
 * Handles exact matches, word starts, and typos.
 */
export function calculateMatchScore(text: string, query: string): number {
  const normText = text.toLowerCase().trim();
  const normQuery = query.toLowerCase().trim();

  // Empty query matches everything
  if (!normQuery) return 1;

  // Exact match shortcut
  if (normText === normQuery) return 100;
  
  // Direct substring shortcut (very common, very fast)
  if (normText.includes(normQuery)) {
    // Boost for starting with query
    if (normText.startsWith(normQuery)) return 50;
    // Boost for word boundary start
    const wordBoundaryIndex = normText.indexOf(' ' + normQuery);
    if (wordBoundaryIndex !== -1) return 40;
    return 30;
  }

  // Tokenize
  const textWords = normText.split(/\s+/);
  const queryTokens = normQuery.split(/\s+/);

  let totalScore = 0;

  // AND-like logic: Every query token must match *something* in the text
  for (const token of queryTokens) {
    let bestTokenScore = 0;

    for (const word of textWords) {
      const score = fuzzyTokenMatch(token, word);
      if (score > bestTokenScore) {
        bestTokenScore = score;
      }
    }

    // If a token has no match in the text, the whole query fails
    if (bestTokenScore === 0) return 0;
    
    totalScore += bestTokenScore;
  }

  return totalScore / queryTokens.length;
}
