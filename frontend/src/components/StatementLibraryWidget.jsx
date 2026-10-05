import { useState, useEffect, useMemo } from 'react';
import { getStatementCatalog, openStatementFile, getVaultChecklist, saveVaultChecklist } from '../services/apiClient';
import { findBestInstitutionMatch } from '../utils/institutionMatching';

export default function StatementLibraryWidget({ initialFilter = null, onNavigateToChecklist = null }) {
  const [catalog, setCatalog] = useState(null);
  const [checklist, setChecklist] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [openingPath, setOpeningPath] = useState(null);
  const [openError, setOpenError] = useState(null);
  const [reconcilingPath, setReconcilingPath] = useState(null);

  // Filters
  const [searchTerm, setSearchTerm] = useState(initialFilter?.search || '');
  const [selectedInstitution, setSelectedInstitution] = useState(initialFilter?.institution || 'ALL');
  const [selectedYear, setSelectedYear] = useState(initialFilter?.year || 'ALL');
  const [selectedStatus, setSelectedStatus] = useState(initialFilter?.status || 'ALL'); // ALL, CANONICAL, NON_CANONICAL, UNREVIEWED, ANOMALY

  async function loadData() {
    setLoading(true);
    setError(null);
    setOpenError(null);
    try {
      const [catalogData, checklistData] = await Promise.all([
        getStatementCatalog().catch(err => {
          console.error("Failed to load statement catalog:", err);
          throw err;
        }),
        getVaultChecklist().catch(err => {
          console.warn("Failed to load checklist (optional for review status):", err);
          return null;
        }),
      ]);
      setCatalog(catalogData);
      setChecklist(checklistData);
    } catch (err) {
      setError(err?.message || String(err) || "Failed to load statement catalog");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    // Load external statement catalog and checklist on mount
    // eslint-disable-next-line react-hooks/set-state-in-effect
    loadData();
  }, []);

  useEffect(() => {
    if (initialFilter) {
      // Synchronize parent navigation filter props into interactive widget filter state
      /* eslint-disable react-hooks/set-state-in-effect */
      if (initialFilter.search !== undefined) setSearchTerm(initialFilter.search);
      if (initialFilter.institution !== undefined) setSelectedInstitution(initialFilter.institution);
      if (initialFilter.year !== undefined) setSelectedYear(initialFilter.year);
      if (initialFilter.status !== undefined) setSelectedStatus(initialFilter.status);
      /* eslint-enable react-hooks/set-state-in-effect */
    }
  }, [initialFilter]);

  const handleOpenStatement = async (statement) => {
    setOpeningPath(statement.path);
    setOpenError(null);
    try {
      await openStatementFile(statement.path);
    } catch (err) {
      console.error("Failed to open statement file:", err);
      setOpenError(`Failed to open "${statement.filename}": ${err?.message || String(err)}`);
    } finally {
      setOpeningPath(null);
    }
  };

  // Derive unique institutions and years for filter dropdowns
  const { institutions, years } = useMemo(() => {
    if (!catalog?.statements) {
      return { institutions: [], years: [] };
    }
    const instSet = new Set();
    const yearSet = new Set();

    catalog.statements.forEach(stmt => {
      if (stmt.institution) instSet.add(stmt.institution);
      if (stmt.date && stmt.date.length >= 4) {
        yearSet.add(stmt.date.slice(0, 4));
      }
    });

    return {
      institutions: Array.from(instSet).sort((a, b) => a.localeCompare(b)),
      years: Array.from(yearSet).sort((a, b) => b.localeCompare(a)),
    };
  }, [catalog]);

  // Set of statement files that have anomalies associated with them
  const anomalyFilesSet = useMemo(() => {
    const set = new Set();
    if (catalog?.anomalies) {
      catalog.anomalies.forEach(a => {
        if (a.files) {
          a.files.forEach(f => set.add(f));
        }
      });
    }
    return set;
  }, [catalog]);

  // Helper to determine checklist item & review status for a statement
  const getStatementChecklistStatus = (stmt) => {
    if (!checklist?.statements || !stmt.date || stmt.date.length < 7) {
      return null;
    }
    const stmtYear = stmt.date.slice(0, 4);
    if (checklist.year && String(checklist.year) !== stmtYear) {
      return null;
    }
    const monthNum = parseInt(stmt.date.slice(5, 7), 10);
    if (isNaN(monthNum) || monthNum < 1 || monthNum > 12) {
      return null;
    }
    const monthIndex = monthNum - 1;

    const candidateNames = checklist.statements.map(s => s.institution);
    const matchedInst = findBestInstitutionMatch(stmt.institution, candidateNames);
    if (!matchedInst) return null;

    const instIndex = checklist.statements.findIndex(s => s.institution === matchedInst);
    if (instIndex === -1) return null;

    const instEntry = checklist.statements[instIndex];
    const isReviewed = Boolean(instEntry.months?.[monthIndex]);

    return {
      instIndex,
      monthIndex,
      institution: matchedInst,
      isReviewed,
    };
  };

  const isStatementUnreviewed = (stmt) => {
    const status = getStatementChecklistStatus(stmt);
    return status ? !status.isReviewed : false;
  };

  const handleToggleReconcile = async (stmt) => {
    const status = getStatementChecklistStatus(stmt);
    if (!status || !checklist) return;

    setReconcilingPath(stmt.path);
    try {
      const newChecklist = structuredClone(checklist);
      const current = Boolean(newChecklist.statements[status.instIndex].months[status.monthIndex]);
      newChecklist.statements[status.instIndex].months[status.monthIndex] = !current;
      setChecklist(newChecklist);
      await saveVaultChecklist(newChecklist);
    } catch (err) {
      console.error("Failed to toggle reconciliation status:", err);
      setOpenError(`Failed to update checklist: ${err?.message || String(err)}`);
    } finally {
      setReconcilingPath(null);
    }
  };

  // Filtered statements list
  const filteredStatements = useMemo(() => {
    if (!catalog?.statements) return [];

    const searchLower = searchTerm.trim().toLowerCase();

    return catalog.statements.filter(stmt => {
      // 1. Search term match (filename, institution, folder, relative_path)
      if (searchLower) {
        const matchesName = stmt.filename?.toLowerCase().includes(searchLower);
        const matchesInst = stmt.institution?.toLowerCase().includes(searchLower);
        const matchesRel = stmt.relative_path?.toLowerCase().includes(searchLower);
        const matchesFolder = stmt.folder?.toLowerCase().includes(searchLower);
        if (!matchesName && !matchesInst && !matchesRel && !matchesFolder) {
          return false;
        }
      }

      // 2. Institution filter
      if (selectedInstitution !== 'ALL' && stmt.institution !== selectedInstitution) {
        return false;
      }

      // 3. Year filter
      if (selectedYear !== 'ALL') {
        const year = stmt.date?.slice(0, 4);
        if (year !== selectedYear) {
          return false;
        }
      }

      // 4. Status filter
      if (selectedStatus === 'CANONICAL' && !stmt.is_canonical) return false;
      if (selectedStatus === 'NON_CANONICAL' && stmt.is_canonical) return false;
      if (selectedStatus === 'UNREVIEWED' && !isStatementUnreviewed(stmt)) return false;
      if (selectedStatus === 'ANOMALY' && !anomalyFilesSet.has(stmt.filename)) return false;

      return true;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [catalog, searchTerm, selectedInstitution, selectedYear, selectedStatus, anomalyFilesSet, checklist]);

  // Format byte size human readable
  const formatBytes = (bytes) => {
    if (!bytes || bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
  };

  if (loading) {
    return (
      <div className="h-96 flex flex-col items-center justify-center space-y-4">
        <div className="w-12 h-12 border-4 border-cyan-400 border-t-transparent rounded-full animate-spin"></div>
        <p className="text-sm font-mono text-cyan-400 uppercase tracking-widest animate-pulse">
          Indexing Statement Vault Catalog...
        </p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-red-950/30 border border-red-500/50 rounded-xl p-6 text-center space-y-4">
        <div className="text-3xl">⚠️</div>
        <h3 className="text-lg font-bold text-red-400 tracking-wider uppercase">Statement Catalog Error</h3>
        <p className="text-sm text-white/70 font-mono">{error}</p>
        <button
          type="button"
          onClick={loadData}
          className="px-4 py-2 bg-red-500/20 hover:bg-red-500/40 text-red-300 border border-red-500/40 rounded text-xs uppercase font-bold tracking-widest transition-all"
        >
          Retry Scan
        </button>
      </div>
    );
  }

  const anomalies = catalog?.anomalies || [];
  const totalStatements = catalog?.total_count || 0;
  const canonicalCount = catalog?.canonical_count || 0;
  const nonCanonicalCount = totalStatements - canonicalCount;

  return (
    <div className="space-y-8 pb-16">
      {/* Header Banner */}
      <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4 border-b border-white/10 pb-6">
        <div>
          <div className="flex items-center gap-3">
            <span className="text-3xl drop-shadow-[0_0_12px_rgba(0,212,255,0.4)]">📂</span>
            <h2 className="text-2xl font-black text-white tracking-widest uppercase italic">Statement Library</h2>
          </div>
          <p className="text-xs font-mono text-cyan-400/80 uppercase tracking-widest mt-1">
            Vault Catalog & Sequence Inspector &bull; <span className="text-white/40">{catalog?.base_path}</span>
          </p>
        </div>

        {/* Stats summary badges */}
        <div className="flex items-center gap-3 flex-wrap">
          <div className="bg-zinc-900/80 border border-white/10 px-3 py-1.5 rounded-lg flex flex-col items-center min-w-[70px]">
            <span className="text-xs font-mono text-white/50 uppercase tracking-wider">Total</span>
            <span className="text-sm font-black text-white">{totalStatements}</span>
          </div>
          <div className="bg-zinc-900/80 border border-white/10 px-3 py-1.5 rounded-lg flex flex-col items-center min-w-[70px]">
            <span className="text-xs font-mono text-emerald-400/70 uppercase tracking-wider">Canonical</span>
            <span className="text-sm font-black text-emerald-400">{canonicalCount}</span>
          </div>
          {nonCanonicalCount > 0 && (
            <div className="bg-zinc-900/80 border border-amber-500/30 px-3 py-1.5 rounded-lg flex flex-col items-center min-w-[70px]">
              <span className="text-xs font-mono text-amber-400/70 uppercase tracking-wider">Non-Canonical</span>
              <span className="text-sm font-black text-amber-400">{nonCanonicalCount}</span>
            </div>
          )}
          {anomalies.length > 0 && (
            <div className="bg-zinc-900/80 border border-red-500/40 px-3 py-1.5 rounded-lg flex flex-col items-center min-w-[70px]">
              <span className="text-xs font-mono text-red-400/70 uppercase tracking-wider">Anomalies</span>
              <span className="text-sm font-black text-red-400">{anomalies.length}</span>
            </div>
          )}
          <button
            type="button"
            onClick={loadData}
            title="Refresh statement index"
            className="p-2.5 bg-white/5 hover:bg-white/10 border border-white/10 rounded-lg text-white/70 hover:text-cyan-400 transition-all"
          >
            🔄
          </button>
        </div>
      </div>

      {openError && (
        <div className="p-3 bg-red-950/40 border border-red-500/50 rounded-lg text-xs font-mono text-red-300 flex items-center justify-between">
          <span>⚠️ {openError}</span>
          <button
            type="button"
            onClick={() => setOpenError(null)}
            className="text-white/40 hover:text-white ml-2 uppercase text-[10px]"
          >
            ✕ Dismiss
          </button>
        </div>
      )}

      {/* Anomalies Banner (if any sequence gaps or collisions exist) */}
      {anomalies.length > 0 && (
        <div className="bg-gradient-to-r from-red-950/40 via-zinc-900/60 to-black/80 border border-red-500/40 rounded-xl p-5 space-y-4 shadow-xl">
          <div className="flex items-center justify-between border-b border-red-500/20 pb-2">
            <div className="flex items-center gap-2">
              <span className="text-red-400 text-lg">⚠️</span>
              <h3 className="text-sm font-black uppercase tracking-wider text-red-400">
                Detected Sequence Gaps & Anomaly Warnings ({anomalies.length})
              </h3>
            </div>
            <span className="text-[10px] font-mono text-white/40 uppercase">Action Recommended</span>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
            {anomalies.map((anomaly, idx) => {
              const isGap = anomaly.kind === 'missing_interval';
              const isDup = anomaly.kind === 'exact_duplicate';
              const isCollision = anomaly.kind === 'disambiguation_collision';

              return (
                <div
                  key={idx}
                  className="bg-black/60 border border-red-500/30 rounded-lg p-3 flex flex-col justify-between space-y-2 hover:border-red-400/60 transition-all"
                >
                  <div>
                    <div className="flex items-center justify-between gap-2 mb-1">
                      <span className="text-xs font-bold text-white truncate">{anomaly.institution}</span>
                      <span className="text-[9px] uppercase font-mono px-1.5 py-0.5 rounded bg-red-500/20 text-red-300 border border-red-500/30">
                        {isGap ? 'GAP' : isDup ? 'DUPLICATE' : isCollision ? 'COLLISION' : 'ANOMALY'}
                      </span>
                    </div>
                    <p className="text-xs text-white/70 leading-relaxed font-sans">{anomaly.message}</p>
                  </div>

                  {anomaly.missing_periods && anomaly.missing_periods.length > 0 && (
                    <div className="flex flex-wrap gap-1 pt-1 border-t border-white/5">
                      {anomaly.missing_periods.map(period => (
                        <span
                          key={period}
                          className="text-[10px] font-mono bg-red-950/60 text-red-300 px-1.5 py-0.5 rounded border border-red-500/40"
                        >
                          {period}
                        </span>
                      ))}
                    </div>
                  )}

                  {anomaly.files && anomaly.files.length > 0 && (
                    <div className="pt-1 border-t border-white/5 space-y-1">
                      <span className="text-[9px] font-mono uppercase text-white/30">Affected files:</span>
                      <div className="space-y-0.5 max-h-16 overflow-y-auto custom-scrollbar">
                        {anomaly.files.map((f, fIdx) => (
                          <div key={fIdx} className="text-[10px] font-mono text-white/50 truncate" title={f}>
                            &bull; {f}
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Filter and Search Controls */}
      <div className="bg-zinc-950/90 border border-white/10 rounded-xl p-4 space-y-4 backdrop-blur-md shadow-lg">
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          {/* Instant Search Bar */}
          <div className="relative">
            <label className="block text-[10px] font-mono uppercase tracking-widest text-white/40 mb-1">
              Search Filename / Path
            </label>
            <div className="relative">
              <input
                type="text"
                value={searchTerm}
                onChange={e => setSearchTerm(e.target.value)}
                placeholder="e.g. Alerus, 2026-01, Chase..."
                className="w-full bg-black/60 border border-white/15 focus:border-cyan-400 focus:outline-none rounded-lg px-3 py-2 text-xs text-white placeholder-white/30 font-mono transition-all"
              />
              {searchTerm && (
                <button
                  type="button"
                  onClick={() => setSearchTerm('')}
                  className="absolute right-2.5 top-2 text-white/40 hover:text-white text-xs"
                >
                  ✕
                </button>
              )}
            </div>
          </div>

          {/* Institution Selector */}
          <div>
            <label className="block text-[10px] font-mono uppercase tracking-widest text-white/40 mb-1">
              Institution
            </label>
            <select
              value={selectedInstitution}
              onChange={e => setSelectedInstitution(e.target.value)}
              className="w-full bg-black/60 border border-white/15 focus:border-cyan-400 focus:outline-none rounded-lg px-3 py-2 text-xs text-white font-mono transition-all cursor-pointer"
            >
              <option value="ALL">All Institutions ({institutions.length})</option>
              {institutions.map(inst => (
                <option key={inst} value={inst}>
                  {inst}
                </option>
              ))}
            </select>
          </div>

          {/* Year Selector */}
          <div>
            <label className="block text-[10px] font-mono uppercase tracking-widest text-white/40 mb-1">
              Year
            </label>
            <select
              value={selectedYear}
              onChange={e => setSelectedYear(e.target.value)}
              className="w-full bg-black/60 border border-white/15 focus:border-cyan-400 focus:outline-none rounded-lg px-3 py-2 text-xs text-white font-mono transition-all cursor-pointer"
            >
              <option value="ALL">All Years ({years.length})</option>
              {years.map(yr => (
                <option key={yr} value={yr}>
                  {yr}
                </option>
              ))}
            </select>
          </div>

          {/* Status Filter */}
          <div>
            <label className="block text-[10px] font-mono uppercase tracking-widest text-white/40 mb-1">
              Status Filter
            </label>
            <select
              value={selectedStatus}
              onChange={e => setSelectedStatus(e.target.value)}
              className="w-full bg-black/60 border border-white/15 focus:border-cyan-400 focus:outline-none rounded-lg px-3 py-2 text-xs text-white font-mono transition-all cursor-pointer"
            >
              <option value="ALL">All Statements</option>
              <option value="CANONICAL">Canonical Only</option>
              <option value="NON_CANONICAL">Non-Canonical / Disambiguated</option>
              <option value="UNREVIEWED">Unreviewed in Checklist</option>
              <option value="ANOMALY">Has Anomaly / Warning</option>
            </select>
          </div>
        </div>

        {/* Filter status row */}
        <div className="flex items-center justify-between text-[11px] font-mono text-white/40 pt-1 border-t border-white/5">
          <span>
            Showing <strong className="text-cyan-400">{filteredStatements.length}</strong> of {totalStatements} statements
          </span>
          {(searchTerm || selectedInstitution !== 'ALL' || selectedYear !== 'ALL' || selectedStatus !== 'ALL') && (
            <button
              type="button"
              onClick={() => {
                setSearchTerm('');
                setSelectedInstitution('ALL');
                setSelectedYear('ALL');
                setSelectedStatus('ALL');
              }}
              className="text-cyan-400/80 hover:text-cyan-300 uppercase tracking-wider text-[10px] underline"
            >
              Reset Filters
            </button>
          )}
        </div>
      </div>

      {/* Statement Table */}
      <div className="bg-zinc-950/80 border border-white/10 rounded-xl overflow-hidden backdrop-blur-md shadow-2xl">
        <div className="overflow-x-auto custom-scrollbar">
          <table className="w-full text-left border-collapse">
            <thead>
              <tr className="bg-black/70 border-b border-white/10 text-[10px] font-bold uppercase tracking-widest text-white/50">
                <th className="py-3 px-4">Statement File</th>
                <th className="py-3 px-4">Institution</th>
                <th className="py-3 px-4">Period Date</th>
                <th className="py-3 px-4">Status & Indicators</th>
                <th className="py-3 px-4 text-right">Size</th>
                <th className="py-3 px-4 text-center">Action</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-white/5 text-xs font-mono">
              {filteredStatements.length === 0 ? (
                <tr>
                  <td colSpan={6} className="py-12 text-center text-white/40 uppercase tracking-widest">
                    No statements matching current filters
                  </td>
                </tr>
              ) : (
                filteredStatements.map(stmt => {
                  const hasAnomaly = anomalyFilesSet.has(stmt.filename);
                  const isUnreviewed = isStatementUnreviewed(stmt);
                  const isOpening = openingPath === stmt.path;

                  return (
                    <tr
                      key={stmt.path}
                      className="hover:bg-white/5 transition-colors group"
                    >
                      {/* Statement File */}
                      <td className="py-3 px-4 max-w-md">
                        <div className="flex items-center gap-2">
                          <span className="text-base opacity-70 group-hover:opacity-100 transition-opacity">📄</span>
                          <div className="flex flex-col truncate">
                            <span
                              className="font-bold text-white group-hover:text-cyan-400 transition-colors truncate cursor-pointer"
                              title={stmt.filename}
                              onClick={() => handleOpenStatement(stmt)}
                            >
                              {stmt.filename}
                            </span>
                            <span className="text-[10px] text-white/40 truncate" title={stmt.relative_path}>
                              {stmt.relative_path}
                            </span>
                          </div>
                        </div>
                      </td>

                      {/* Institution */}
                      <td className="py-3 px-4 whitespace-nowrap">
                        <span className="text-white/80 font-sans font-bold">{stmt.institution}</span>
                      </td>

                      {/* Period Date */}
                      <td className="py-3 px-4 whitespace-nowrap">
                        {stmt.date ? (
                          <span className="bg-black/50 border border-white/10 text-cyan-300 px-2 py-0.5 rounded text-[11px]">
                            {stmt.date}
                          </span>
                        ) : (
                          <span className="text-white/30 text-[10px] italic">Undated</span>
                        )}
                      </td>

                      {/* Status & Indicators */}
                      <td className="py-3 px-4">
                        <div className="flex items-center gap-1.5 flex-wrap">
                          {stmt.is_canonical ? (
                            <span className="text-[9px] uppercase tracking-wider px-1.5 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/30">
                              Canonical
                            </span>
                          ) : (
                            <span className="text-[9px] uppercase tracking-wider px-1.5 py-0.5 rounded bg-amber-500/10 text-amber-400 border border-amber-500/30">
                              {stmt.disambiguation ? `Suffix: ${stmt.disambiguation}` : 'Non-Standard'}
                            </span>
                          )}

                          {hasAnomaly && (
                            <span className="text-[9px] uppercase tracking-wider px-1.5 py-0.5 rounded bg-red-500/20 text-red-300 border border-red-500/50 animate-pulse">
                              Anomaly
                            </span>
                          )}

                          {isUnreviewed && (
                            <span className="text-[9px] uppercase tracking-wider px-1.5 py-0.5 rounded bg-cyan-500/10 text-cyan-300 border border-cyan-500/30">
                              Unreviewed
                            </span>
                          )}
                        </div>
                      </td>

                      {/* Size */}
                      <td className="py-3 px-4 text-right whitespace-nowrap text-white/50 text-[11px]">
                        {formatBytes(stmt.size_bytes)}
                      </td>

                      {/* Action */}
                      <td className="py-3 px-4 text-center whitespace-nowrap">
                        <div className="flex items-center justify-center gap-1.5">
                          <button
                            type="button"
                            onClick={() => handleOpenStatement(stmt)}
                            disabled={isOpening}
                            className="px-2.5 py-1 bg-cyan-500/10 hover:bg-cyan-500/20 active:bg-cyan-500/30 text-cyan-300 border border-cyan-500/30 hover:border-cyan-400 rounded text-[11px] font-bold uppercase tracking-wider transition-all disabled:opacity-50"
                            title="Open Statement PDF in default system viewer"
                          >
                            {isOpening ? 'Opening...' : 'View PDF'}
                          </button>
                          {(() => {
                            const checkStatus = getStatementChecklistStatus(stmt);
                            if (!checkStatus) return null;
                            const isReconciling = reconcilingPath === stmt.path;
                            return (
                              <button
                                type="button"
                                onClick={() => handleToggleReconcile(stmt)}
                                disabled={isReconciling}
                                title={checkStatus.isReviewed ? "Mark Unreconciled in Checklist" : "Mark Reconciled in Checklist"}
                                className={`px-2 py-1 rounded text-[11px] font-bold uppercase tracking-wider border transition-all disabled:opacity-50 ${
                                  checkStatus.isReviewed
                                    ? 'bg-emerald-500/20 text-emerald-300 border-emerald-500/40 hover:bg-emerald-500/30'
                                    : 'bg-white/5 text-white/50 border-white/10 hover:border-emerald-400 hover:text-emerald-400'
                                }`}
                              >
                                {isReconciling ? '...' : (checkStatus.isReviewed ? '✓ Reconciled' : 'Reconcile')}
                              </button>
                            );
                          })()}
                          {onNavigateToChecklist && (
                            <button
                              type="button"
                              onClick={onNavigateToChecklist}
                              title="Jump to Master Checklist"
                              className="px-1.5 py-1 text-white/40 hover:text-cyan-400 hover:bg-white/5 rounded border border-transparent hover:border-white/10 text-[11px] transition-all"
                            >
                              ⚖️
                            </button>
                          )}
                        </div>
                      </td>
                    </tr>
                  );
                })
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
