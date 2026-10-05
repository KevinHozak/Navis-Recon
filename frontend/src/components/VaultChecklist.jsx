import { useState, useEffect, useMemo } from 'react';
import { motion } from 'framer-motion';
import { getVaultChecklist, saveVaultChecklist, getStatementCatalog, openStatementFile } from '../services/apiClient';
import { findBestInstitutionMatch, findMatchingStatements } from '../utils/institutionMatching';

export default function VaultChecklist({ onNavigateToStatements = null }) {
  const [data, setData] = useState(null);
  const [catalog, setCatalog] = useState(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [openingPath, setOpeningPath] = useState(null);
  const [actionError, setActionError] = useState(null);

  async function loadData() {
    setLoading(true);
    setActionError(null);
    try {
      const [checklistJson, catalogJson] = await Promise.all([
        getVaultChecklist().catch(err => {
          console.error("DEBUG: Failed to fetch checklist:", err);
          return null;
        }),
        getStatementCatalog().catch(err => {
          console.warn("DEBUG: Statement catalog not available:", err);
          return null;
        }),
      ]);

      if (checklistJson && !checklistJson.error) {
        setData(checklistJson);
      }
      if (catalogJson && !catalogJson.error) {
        setCatalog(catalogJson);
      }
    } catch (err) {
      console.error("DEBUG: Error loading checklist view data:", err);
      setActionError(err?.message || String(err));
    } finally {
      setLoading(false);
    }
  }

  async function saveChecklist(updatedData) {
    setSaving(true);
    try {
      await saveVaultChecklist(updatedData);
    } catch (err) {
      console.error("DEBUG: Failed to save checklist:", err);
      setActionError(`Failed to save checklist: ${err?.message || String(err)}`);
    } finally {
      setSaving(false);
    }
  }

  useEffect(() => {
    // Loading external checklist and catalog state on mount
    // eslint-disable-next-line react-hooks/set-state-in-effect
    loadData();
  }, []);

  const handleOpenPdf = async (path, filename) => {
    setOpeningPath(path);
    setActionError(null);
    try {
      await openStatementFile(path);
    } catch (err) {
      console.error("DEBUG: Failed to open statement file:", err);
      setActionError(`Failed to open statement "${filename}": ${err?.message || String(err)}`);
    } finally {
      setOpeningPath(null);
    }
  };

  const handleStatementToggle = (instIndex, monthIndex) => {
    const newData = structuredClone(data);
    newData.statements[instIndex].months[monthIndex] = !newData.statements[instIndex].months[monthIndex];
    setData(newData);
    saveChecklist(newData);
  };

  const handleTaskToggle = (weekIndex, taskIndex) => {
    const newData = structuredClone(data);
    newData.triage[weekIndex].tasks[taskIndex].completed = !newData.triage[weekIndex].tasks[taskIndex].completed;
    setData(newData);
    saveChecklist(newData);
  };

  // Derive catalog statement map for the current checklist year
  const catalogStatements = useMemo(() => {
    return catalog?.statements || [];
  }, [catalog]);

  const checklistYear = useMemo(() => {
    return data?.year ? String(data.year) : new Date().getFullYear().toString();
  }, [data]);

  // Pre-index statements by institution match for fast grid and task lookups
  const gridStatementsMap = useMemo(() => {
    if (!data?.statements || catalogStatements.length === 0) return new Map();

    const map = new Map();
    data.statements.forEach((instItem) => {
      // Find all statements matching this institution and year
      const matched = findMatchingStatements(catalogStatements, instItem.institution, checklistYear);
      // Bucket by month (1-12)
      const monthMap = new Map();
      matched.forEach(stmt => {
        if (stmt.date && stmt.date.length >= 7) {
          const m = parseInt(stmt.date.slice(5, 7), 10);
          if (!isNaN(m) && m >= 1 && m <= 12) {
            if (!monthMap.has(m)) monthMap.set(m, []);
            monthMap.get(m).push(stmt);
          }
        }
      });
      map.set(instItem.institution, { matched, monthMap });
    });
    return map;
  }, [data, catalogStatements, checklistYear]);

  // Helper to find matching statement for a triage task
  const getTaskMatchingStatements = (taskTitle) => {
    if (!catalogStatements.length || !taskTitle) return [];
    // Ignore non-institution administrative tasks
    const lower = taskTitle.toLowerCase();
    if (lower.includes('receipt') || lower.includes('paperwork') || lower.includes('shred')) {
      return [];
    }
    return findMatchingStatements(catalogStatements, taskTitle, checklistYear);
  };

  if (loading) {
    return (
      <div className="h-full flex items-center justify-center">
        <div className="w-12 h-12 border-4 border-cyan-400 border-t-transparent rounded-full animate-spin"></div>
      </div>
    );
  }

  if (!data) return <div className="text-white p-6">Error loading checklist data.</div>;

  const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

  return (
    <div className="h-full flex flex-col overflow-y-auto custom-scrollbar p-6 space-y-12 pb-24">
      
      {/* Header */}
      <div className="flex flex-col items-center justify-center space-y-2 mb-8 border-b border-white/10 pb-6">
        <span className="text-5xl drop-shadow-[0_0_15px_rgba(0,212,255,0.5)]">⚖️</span>
        <h2 className="text-3xl font-black text-white tracking-widest uppercase italic">Master Checklist</h2>
        <p className="text-sm font-mono text-cyan-400/80 uppercase tracking-widest">Sovereign Ledger Operations</p>
        <div className="flex items-center gap-4 text-xs font-mono">
          {saving && <span className="text-amber-400 animate-pulse">Saving changes...</span>}
          {catalog && (
            <span className="text-white/40">
              Statement Catalog Linked: <strong className="text-cyan-400">{catalogStatements.length}</strong> archived
            </span>
          )}
        </div>
        {actionError && (
          <div className="mt-2 px-4 py-2 bg-red-500/10 border border-red-500/30 rounded text-red-300 text-xs font-mono">
            {actionError}
          </div>
        )}
      </div>

      {/* Monthly Triage */}
      <div className="space-y-6">
        <div className="flex items-center justify-between border-b border-white/10 pb-2">
          <div className="flex items-center gap-3">
            <span className="text-2xl">🏦</span>
            <h3 className="text-xl font-bold text-white uppercase tracking-widest">Monthly Statement Triage</h3>
          </div>
          {onNavigateToStatements && (
            <button
              type="button"
              onClick={() => onNavigateToStatements({})}
              className="text-xs font-mono uppercase tracking-widest text-cyan-400 hover:text-cyan-300 transition-colors flex items-center gap-1.5"
            >
              <span>📂 Open Statement Library</span>
              <span className="text-sm">→</span>
            </button>
          )}
        </div>
        
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          {data.triage.map((weekData, wIdx) => {
            const isWeek1 = weekData.color === 'green';
            const isWeek2 = weekData.color === 'yellow';
            const isWeek3 = weekData.color === 'orange';
            const isWeek4 = weekData.color === 'red';
            
            let borderColor = 'border-cyan-500/30';
            let bgGradient = 'from-cyan-950/40 to-black/60';
            let titleColor = 'text-cyan-400';
            
            if (isWeek1) { borderColor = 'border-emerald-500/30'; bgGradient = 'from-emerald-950/40 to-black/60'; titleColor = 'text-emerald-400'; }
            if (isWeek2) { borderColor = 'border-amber-500/30'; bgGradient = 'from-amber-950/40 to-black/60'; titleColor = 'text-amber-400'; }
            if (isWeek3) { borderColor = 'border-orange-500/30'; bgGradient = 'from-orange-950/40 to-black/60'; titleColor = 'text-orange-400'; }
            if (isWeek4) { borderColor = 'border-red-500/30'; bgGradient = 'from-red-950/40 to-black/60'; titleColor = 'text-red-400'; }

            return (
              <motion.div 
                key={wIdx}
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: wIdx * 0.1 }}
                className={`bg-gradient-to-br ${bgGradient} border ${borderColor} rounded-xl p-5 backdrop-blur-md shadow-xl flex flex-col`}
              >
                <h4 className={`text-sm font-black uppercase tracking-widest mb-4 ${titleColor} border-b border-white/5 pb-2`}>
                  {weekData.week}
                </h4>
                <div className="space-y-2 flex-1">
                  {weekData.tasks.map((task, tIdx) => {
                    const taskStmts = getTaskMatchingStatements(task.title);
                    // Select newest matching statement (sort descending by date, with modified_ms fallback)
                    const sortedStmts = [...taskStmts].sort((a, b) => {
                      const dateA = a.date || '';
                      const dateB = b.date || '';
                      if (dateA !== dateB) return dateB.localeCompare(dateA);
                      return (b.modified_ms || 0) - (a.modified_ms || 0);
                    });
                    const latestStmt = sortedStmts.length > 0 ? sortedStmts[0] : null;

                    return (
                      <div 
                        key={task.id} 
                        className={`flex items-center justify-between p-2 rounded transition-all ${task.completed ? 'bg-white/5 opacity-60' : 'hover:bg-white/5'}`}
                      >
                        <div className="flex items-center gap-3 min-w-0">
                          <button 
                            type="button"
                            onClick={() => handleTaskToggle(wIdx, tIdx)}
                            title={task.completed ? "Mark pending" : "Mark completed / reconciled"}
                            className={`w-5 h-5 rounded border flex items-center justify-center shrink-0 transition-all ${
                              task.completed 
                              ? 'bg-emerald-500 border-emerald-500 text-black shadow-[0_0_8px_rgba(16,185,129,0.5)]' 
                              : 'bg-black/50 border-white/20 hover:border-cyan-400'
                            }`}
                          >
                            {task.completed && <span className="text-xs font-bold">✓</span>}
                          </button>
                          <div className="flex flex-col min-w-0">
                            {task.url ? (
                              <a href={task.url} target="_blank" rel="noreferrer" className={`text-sm font-bold truncate transition-colors ${task.completed ? 'line-through text-white/50' : 'text-white hover:text-cyan-400'}`}>
                                {task.title}
                              </a>
                            ) : (
                              <span className={`text-sm font-bold truncate ${task.completed ? 'line-through text-white/50' : 'text-white'}`}>
                                {task.title}
                              </span>
                            )}
                          </div>
                        </div>

                        <div className="flex items-center gap-2 shrink-0 ml-3">
                          {/* Reconcile Link: PDF View action if matching statement exists in catalog */}
                          {latestStmt ? (
                            <div className="flex items-center gap-1">
                              <button
                                type="button"
                                onClick={() => handleOpenPdf(latestStmt.path, latestStmt.filename)}
                                disabled={openingPath === latestStmt.path}
                                title={`Open Statement PDF: ${latestStmt.filename}`}
                                className="px-2 py-0.5 rounded text-[10px] font-mono font-bold tracking-wider bg-cyan-500/10 hover:bg-cyan-500/20 text-cyan-300 border border-cyan-500/40 hover:border-cyan-400 transition-all flex items-center gap-1 disabled:opacity-50"
                              >
                                <span>📄</span>
                                <span>{openingPath === latestStmt.path ? 'Opening...' : 'View PDF'}</span>
                              </button>
                              {onNavigateToStatements && (
                                <button
                                  type="button"
                                  onClick={() => onNavigateToStatements({ institution: latestStmt.institution || task.title })}
                                  title="Filter Statement Library by this institution"
                                  className="px-1.5 py-0.5 rounded text-[10px] text-white/40 hover:text-cyan-400 hover:bg-white/10 transition-colors"
                                >
                                  📂
                                </button>
                              )}
                            </div>
                          ) : null}

                          <div className="text-[10px] font-mono text-white/30 uppercase tracking-widest bg-black/40 px-2 py-0.5 rounded">
                            Day {task.day}
                          </div>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </motion.div>
            );
          })}
        </div>
      </div>

      {/* Statement Status Grid */}
      <div className="space-y-6 pt-8">
        <div className="flex items-center justify-between border-b border-white/10 pb-2">
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-3">
              <span className="text-2xl">📊</span>
              <h3 className="text-xl font-bold text-white uppercase tracking-widest">{data.year} Statement Status Grid</h3>
            </div>
            <p className="text-xs text-white/40 font-mono italic pl-9">
              Verified blocks represent reconciled statements. Glow badge indicates PDF present in library.
            </p>
          </div>
          <div className="flex items-center gap-4 text-xs font-mono text-white/40">
            <div className="flex items-center gap-1.5">
              <span className="w-3 h-3 rounded bg-emerald-500 border border-emerald-400 inline-block"></span>
              <span>Reconciled</span>
            </div>
            <div className="flex items-center gap-1.5">
              <span className="w-3 h-3 rounded bg-white/5 border border-cyan-500 inline-block"></span>
              <span>PDF Available</span>
            </div>
            <div className="flex items-center gap-1.5">
              <span className="w-3 h-3 rounded bg-white/5 border border-white/10 inline-block"></span>
              <span>Pending</span>
            </div>
          </div>
        </div>

        <div className="overflow-x-auto custom-scrollbar pb-4">
          <div className="min-w-[1100px] bg-zinc-950/80 border border-white/10 rounded-xl overflow-hidden backdrop-blur-md shadow-2xl">
            {/* Grid Header */}
            <div className="grid grid-cols-[2.5fr_repeat(12,0.6fr)] bg-black/60 border-b border-white/10 text-[10px] uppercase font-bold tracking-widest text-white/50 py-3">
              <div className="pl-4">Institution</div>
              {months.map(m => (
                <div key={m} className="text-center">{m}</div>
              ))}
            </div>
            
            {/* Grid Body */}
            <div className="divide-y divide-white/5">
              {data.statements.map((inst, iIdx) => {
                const instMapEntry = gridStatementsMap.get(inst.institution);
                const monthMap = instMapEntry?.monthMap;

                return (
                  <div key={inst.institution} className="grid grid-cols-[2.5fr_repeat(12,0.6fr)] hover:bg-white/5 transition-colors items-center py-2">
                    <div className="pl-4 flex items-center justify-between pr-2 truncate">
                      <span className="text-xs font-bold text-white/80 truncate">
                        {inst.institution}
                      </span>
                      {onNavigateToStatements && (
                        <button
                          type="button"
                          onClick={() => {
                            // Find matching institution name in catalog if possible
                            const candidateNames = catalog?.statements?.map(s => s.institution) || [];
                            const matchedName = findBestInstitutionMatch(inst.institution, candidateNames) || inst.institution;
                            onNavigateToStatements({ institution: matchedName, year: checklistYear });
                          }}
                          title={`View all ${inst.institution} statements in library`}
                          className="text-[10px] text-white/30 hover:text-cyan-400 px-1 py-0.5 rounded transition-colors"
                        >
                          📂
                        </button>
                      )}
                    </div>

                    {inst.months.map((isComplete, mIdx) => {
                      const monthNum = mIdx + 1;
                      const monthStmts = monthMap?.get(monthNum) || [];
                      const hasPdf = monthStmts.length > 0;
                      const targetPdf = hasPdf ? monthStmts[0] : null;

                      let cellTitle = `${inst.institution} - ${months[mIdx]}: `;
                      if (isComplete && hasPdf) {
                        cellTitle += `Reconciled & PDF Archived (${targetPdf.filename}). Click to toggle reconcile, Shift+Click or double-click to view PDF.`;
                      } else if (isComplete && !hasPdf) {
                        cellTitle += 'Reconciled (No PDF detected in catalog). Click to toggle.';
                      } else if (!isComplete && hasPdf) {
                        cellTitle += `PDF Available (${targetPdf.filename}), awaiting reconciliation. Click to mark reconciled.`;
                      } else {
                        cellTitle += 'Pending statement. Click to toggle status.';
                      }

                      return (
                        <div key={mIdx} className="flex items-center justify-center relative group/cell">
                          <button 
                            type="button"
                            onClick={(e) => {
                              // If user held Alt or Shift or clicked and hasPdf, check modifier
                              if (e.shiftKey && targetPdf) {
                                handleOpenPdf(targetPdf.path, targetPdf.filename);
                              } else {
                                handleStatementToggle(iIdx, mIdx);
                              }
                            }}
                            title={cellTitle}
                            className={`w-6 h-6 rounded shadow-sm transition-all duration-300 relative flex items-center justify-center ${
                              isComplete 
                              ? 'bg-emerald-500/80 shadow-[0_0_10px_rgba(16,185,129,0.4)] border border-emerald-400' 
                              : hasPdf
                              ? 'bg-cyan-500/20 border border-cyan-400 hover:bg-cyan-500/30 shadow-[0_0_8px_rgba(0,212,255,0.25)]'
                              : 'bg-white/5 border border-white/10 hover:border-cyan-500 hover:bg-cyan-500/20'
                            }`}
                          >
                            {/* Inner mark */}
                            {isComplete ? (
                              <span className="text-[10px] text-black font-black leading-none">✓</span>
                            ) : hasPdf ? (
                              <span className="w-1.5 h-1.5 rounded-full bg-cyan-400"></span>
                            ) : null}
                          </button>

                          {/* Quick PDF button popover on hover if PDF exists */}
                          {hasPdf && targetPdf && (
                            <button
                              type="button"
                              onClick={(e) => {
                                e.stopPropagation();
                                handleOpenPdf(targetPdf.path, targetPdf.filename);
                              }}
                              disabled={openingPath === targetPdf.path}
                              title={`Open ${targetPdf.filename}`}
                              className="absolute -top-6 hidden group-hover/cell:flex items-center gap-1 z-20 bg-zinc-900 border border-cyan-400/80 text-cyan-300 px-1.5 py-0.5 rounded text-[9px] font-mono shadow-xl hover:bg-cyan-950 whitespace-nowrap"
                            >
                              <span>📄</span>
                              <span>PDF</span>
                            </button>
                          )}
                        </div>
                      );
                    })}
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      </div>
      
    </div>
  );
}
