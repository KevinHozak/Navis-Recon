/**
 * Normalizes an institution name or task title into a standardized key
 * for matching across checklist tasks, checklist statement grid, and catalog statements.
 */
export function cleanInstitution(str) {
  if (!str) return '';
  let s = str.toLowerCase();
  s = s.replace(/\bcard\b/g, '');
  s = s.replace(/\bmc\b/g, 'mastercard');
  s = s.replace(/\bfinancial\b/g, '');
  s = s.replace(/\bhsa\b/g, '');
  s = s.replace(/\b401k\b/g, '');
  s = s.replace(/\bj\s+paypal\b/g, 'paypal');
  // Handle tasks named "United Gateway" or "United Explorer" without "Chase"
  if (s.includes('united') && !s.includes('chase')) {
    s = 'chase' + s;
  }
  // Strip account numbers in parentheses (e.g. "(1749)", "(2519)")
  s = s.replace(/\(\d+\)/g, '');
  // Normalize person suffix tags (H), (K), etc.
  s = s.replace(/\(h\)/g, 'h')
       .replace(/\(k\)/g, 'k')
       .replace(/\(z\)/g, 'z')
       .replace(/\(m\)/g, 'm');
  // Strip non-alphanumeric
  s = s.replace(/[^a-z0-9]/g, '').trim();
  return s;
}

/**
 * Finds the best matching candidate name from a list of candidate strings for a given target name.
 * Priority:
 * 1. Exact cleaned match
 * 2. Exact cleaned match ignoring trailing person letter (e.g. 'h', 'k')
 * 3. Most specific prefix/contains match (sorted by length descending)
 */
export function findBestInstitutionMatch(targetName, candidateNames) {
  const target = cleanInstitution(targetName);
  if (!target || !Array.isArray(candidateNames) || candidateNames.length === 0) return null;

  // 1. Exact cleaned match
  const exact = candidateNames.find(c => cleanInstitution(c) === target);
  if (exact) return exact;

  // 2. Exact match ignoring trailing person letter
  const targetNoPerson = target.replace(/[hkzm]$/, '');
  if (targetNoPerson) {
    const exactNoPerson = candidateNames.find(c => {
      const cClean = cleanInstitution(c);
      return cClean === target || cClean.replace(/[hkzm]$/, '') === targetNoPerson;
    });
    if (exactNoPerson) return exactNoPerson;
  }

  // 3. Most specific prefix/contains match
  const matches = candidateNames.filter(c => {
    const k = cleanInstitution(c);
    return k && (k.startsWith(target) || target.startsWith(k));
  });

  if (matches.length > 0) {
    matches.sort((a, b) => cleanInstitution(b).length - cleanInstitution(a).length);
    return matches[0];
  }

  return null;
}

/**
 * Checks whether two institution names represent the same institution.
 */
export function institutionsMatch(nameA, nameB) {
  if (!nameA || !nameB) return false;
  return Boolean(findBestInstitutionMatch(nameA, [nameB]));
}

/**
 * Finds matching statements from a catalog for a given institution name and optional month number (1-12) or year.
 */
export function findMatchingStatements(catalogStatements, institutionName, year = null, month = null) {
  if (!Array.isArray(catalogStatements) || !institutionName) return [];
  const targetKey = cleanInstitution(institutionName);
  if (!targetKey) return [];

  return catalogStatements.filter(stmt => {
    if (!stmt.institution) return false;
    const matched = institutionsMatch(institutionName, stmt.institution);
    if (!matched) return false;

    if (year != null) {
      if (!stmt.date || stmt.date.length < 4 || String(stmt.date.slice(0, 4)) !== String(year)) {
        return false;
      }
    }

    if (month != null) {
      if (!stmt.date || stmt.date.length < 7) {
        return false;
      }
      const stmtMonth = parseInt(stmt.date.slice(5, 7), 10);
      if (isNaN(stmtMonth) || stmtMonth !== month) {
        return false;
      }
    }

    return true;
  });
}
