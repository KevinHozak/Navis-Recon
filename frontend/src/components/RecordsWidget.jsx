import { useState, useEffect, useMemo } from 'react';
import {
  getRecords,
  getRecordsRadar,
  saveRecord,
  deleteRecord,
  revealRecordFile,
} from '../services/apiClient';

const CATEGORIES = [
  'Identity',
  'Vehicles',
  'Property',
  'Insurance',
  'Financial',
  'Legal',
  'Medical',
  'Warranties',
  'Other',
];

const RECURRENCE_OPTIONS = [
  { value: 'none', label: 'No Recurrence' },
  { value: 'annual', label: 'Annual (1 Year)' },
  { value: 'biennial', label: 'Biennial (2 Years)' },
  { value: 'five_year', label: '5 Years' },
  { value: 'ten_year', label: '10 Years (e.g. Passport)' },
  { value: 'monthly', label: 'Monthly' },
  { value: 'quarterly', label: 'Quarterly' },
  { value: 'semi_annual', label: 'Semi-Annual (6 Months)' },
];

export default function RecordsWidget() {
  const [report, setReport] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  // Filters & Tabs
  const [activeTab, setActiveTab] = useState('all'); // 'all' | 'action' | 'category' | 'owner'
  const [searchTerm, setSearchTerm] = useState('');
  const [selectedCategory, setSelectedCategory] = useState('ALL');
  const [selectedOwner, setSelectedOwner] = useState('ALL');

  // Modal / Drawer state
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [editingRecord, setEditingRecord] = useState(null);
  const [isSaving, setIsSaving] = useState(false);
  const [modalError, setModalError] = useState(null);

  // Form Fields
  const [formTitle, setFormTitle] = useState('');
  const [formCategory, setFormCategory] = useState('Identity');
  const [formOwner, setFormOwner] = useState('Kevin');
  const [formExpirationDate, setFormExpirationDate] = useState('');
  const [formFilePath, setFormFilePath] = useState('');
  const [formNotes, setFormNotes] = useState('');
  const [formRecurrenceInterval, setFormRecurrenceInterval] = useState('none');
  const [formLeadTimeDays, setFormLeadTimeDays] = useState('30');

  // Delete Confirmation
  const [recordToDelete, setRecordToDelete] = useState(null);

  async function loadRadarData() {
    setLoading(true);
    setError(null);
    try {
      const data = await getRecordsRadar();
      setReport(data);
    } catch (err) {
      console.error('Failed to load records radar:', err);
      // Fallback: try loading plain records if radar report fails
      try {
        const rawRecords = await getRecords();
        setReport({
          summary: {
            total: rawRecords.length,
            expired: 0,
            critical: 0,
            upcoming: 0,
            healthy: rawRecords.length,
            actionRequired: 0,
          },
          items: rawRecords.map((r) => ({
            record: r,
            bucket: 'healthy',
            daysUntilExpiration: null,
            nextRenewalDate: null,
            isActionRequired: false,
          })),
          asOfDate: new Date().toISOString().split('T')[0],
        });
      } catch {
        setError(err?.message || String(err) || 'Failed to load household records');
      }
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    loadRadarData();
  }, []);

  // Distinct Owners & Categories from data
  const { availableOwners, availableCategories } = useMemo(() => {
    if (!report?.items) return { availableOwners: [], availableCategories: [] };
    const owners = new Set();
    const categories = new Set();
    report.items.forEach(({ record }) => {
      if (record.owner?.trim()) owners.add(record.owner.trim());
      if (record.category?.trim()) categories.add(record.category.trim());
    });
    return {
      availableOwners: Array.from(owners).sort(),
      availableCategories: Array.from(categories).sort(),
    };
  }, [report]);

  // Filtered Items
  const filteredItems = useMemo(() => {
    if (!report?.items) return [];

    return report.items.filter(({ record, isActionRequired }) => {
      // Tab filtering
      if (activeTab === 'action' && !isActionRequired) return false;
      if (activeTab === 'category' && selectedCategory !== 'ALL' && record.category !== selectedCategory) {
        return false;
      }
      if (activeTab === 'owner' && selectedOwner !== 'ALL' && record.owner !== selectedOwner) {
        return false;
      }

      // Keyword search
      if (searchTerm.trim()) {
        const query = searchTerm.toLowerCase();
        const matchesTitle = record.title?.toLowerCase().includes(query);
        const matchesOwner = record.owner?.toLowerCase().includes(query);
        const matchesCat = record.category?.toLowerCase().includes(query);
        const matchesNotes = record.notes?.toLowerCase().includes(query);
        const matchesFile = record.filePath?.toLowerCase().includes(query);
        if (!matchesTitle && !matchesOwner && !matchesCat && !matchesNotes && !matchesFile) {
          return false;
        }
      }

      return true;
    });
  }, [report, activeTab, selectedCategory, selectedOwner, searchTerm]);

  // Modal Handlers
  function openCreateModal() {
    setEditingRecord(null);
    setFormTitle('');
    setFormCategory('Identity');
    setFormOwner('Kevin');
    setFormExpirationDate('');
    setFormFilePath('');
    setFormNotes('');
    setFormRecurrenceInterval('none');
    setFormLeadTimeDays('30');
    setModalError(null);
    setIsModalOpen(true);
  }

  function openEditModal(record) {
    setEditingRecord(record);
    setFormTitle(record.title || '');
    setFormCategory(record.category || 'Identity');
    setFormOwner(record.owner || 'Kevin');
    setFormExpirationDate(record.expirationDate || '');
    setFormFilePath(record.filePath || '');
    setFormNotes(record.notes || '');

    if (record.recurrenceRule?.interval) {
      const interval = typeof record.recurrenceRule.interval === 'string'
        ? record.recurrenceRule.interval
        : Object.keys(record.recurrenceRule.interval)[0];
      setFormRecurrenceInterval(interval || 'none');
      setFormLeadTimeDays(String(record.recurrenceRule.leadTimeDays ?? 30));
    } else {
      setFormRecurrenceInterval('none');
      setFormLeadTimeDays('30');
    }

    setModalError(null);
    setIsModalOpen(true);
  }

  async function handleSaveRecord(e) {
    e.preventDefault();
    if (!formTitle.trim()) {
      setModalError('Title is required');
      return;
    }
    if (!formCategory.trim()) {
      setModalError('Category is required');
      return;
    }
    if (!formOwner.trim()) {
      setModalError('Owner is required');
      return;
    }

    setIsSaving(true);
    setModalError(null);

    const recordPayload = {
      id: editingRecord?.id || `rec-${Date.now()}`,
      title: formTitle.trim(),
      category: formCategory.trim(),
      owner: formOwner.trim(),
      expirationDate: formExpirationDate.trim() || null,
      filePath: formFilePath.trim() || null,
      notes: formNotes.trim() || null,
      status: editingRecord?.status || 'Active',
      recurrenceRule:
        formRecurrenceInterval !== 'none'
          ? {
              interval: formRecurrenceInterval,
              leadTimeDays: parseInt(formLeadTimeDays, 10) || 30,
            }
          : null,
    };

    try {
      await saveRecord(recordPayload);
      setIsModalOpen(false);
      await loadRadarData();
    } catch (err) {
      console.error('Save record failed:', err);
      setModalError(err?.message || String(err) || 'Failed to save record');
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDeleteConfirm() {
    if (!recordToDelete) return;
    try {
      await deleteRecord(recordToDelete.id);
      setRecordToDelete(null);
      await loadRadarData();
    } catch (err) {
      console.error('Delete record failed:', err);
      alert(`Failed to delete record: ${err?.message || err}`);
    }
  }

  async function handleRevealFile(filePath) {
    if (!filePath?.trim()) return;
    try {
      await revealRecordFile(filePath.trim());
    } catch (err) {
      console.error('Reveal file failed:', err);
      alert(`Could not open file location: ${err?.message || err}`);
    }
  }

  // Helper badge styles
  function renderBucketBadge(bucket, days, isActionRequired) {
    switch (bucket) {
      case 'expired':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded bg-red-500/20 text-red-400 border border-red-500/40 text-xs font-black uppercase tracking-wider animate-pulse">
            🚨 Expired ({Math.abs(days)}d ago)
          </span>
        );
      case 'critical':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded bg-amber-500/20 text-amber-300 border border-amber-500/40 text-xs font-bold uppercase tracking-wider">
            ⚠️ Due in {days}d
          </span>
        );
      case 'upcoming':
        return (
          <span className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded ${isActionRequired ? 'bg-amber-500/20 text-amber-300 border border-amber-500/40' : 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'} text-xs font-semibold uppercase tracking-wider`}>
            {isActionRequired ? `⚡ Notice (${days}d)` : `📅 Due in ${days}d`}
          </span>
        );
      case 'healthy':
      default:
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 text-xs font-medium uppercase tracking-wider">
            {days ? `🛡️ ${days}d left` : '🛡️ Indefinite'}
          </span>
        );
    }
  }

  return (
    <div className="space-y-8 animate-fadeIn">
      {/* Top Header & Actions */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 pb-6 border-b border-white/10">
        <div>
          <div className="flex items-center gap-3">
            <h1 className="text-2xl font-black uppercase tracking-widest text-white">
              Household Records & Readiness Hub
            </h1>
            <span className="bg-accent/10 text-accent border border-accent/30 text-[11px] font-bold px-2 py-0.5 rounded uppercase">
              Local Vault Index
            </span>
          </div>
          <p className="text-sm text-white/50 mt-1">
            Privacy-first vital document registry, expiration radar, and renewal reminders.
          </p>
        </div>

        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={loadRadarData}
            disabled={loading}
            className="px-4 py-2 bg-white/5 hover:bg-white/10 border border-white/10 hover:border-white/20 rounded text-xs font-bold uppercase tracking-wider text-white transition-all flex items-center gap-2"
          >
            🔄 Refresh
          </button>
          <button
            type="button"
            onClick={openCreateModal}
            className="px-4 py-2 bg-accent hover:bg-accent/90 text-black rounded text-xs font-black uppercase tracking-widest transition-all shadow-[0_0_15px_rgba(0,212,255,0.3)] flex items-center gap-2"
          >
            ➕ Index Record
          </button>
        </div>
      </div>

      {/* Expiration Radar Metrics Cards */}
      {report?.summary && (
        <div className="grid grid-cols-2 md:grid-cols-5 gap-4">
          <div className="p-4 rounded-lg bg-zinc-950 border border-white/10">
            <span className="text-[10px] font-black uppercase tracking-widest text-white/40 block mb-1">
              Total Records
            </span>
            <div className="text-2xl font-black text-white">{report.summary.total}</div>
            <span className="text-[11px] text-white/40 mt-1 block">Indexed documents</span>
          </div>

          <div
            onClick={() => setActiveTab('action')}
            className={`p-4 rounded-lg cursor-pointer transition-all ${
              report.summary.actionRequired > 0
                ? 'bg-red-950/20 border-red-500/40 hover:border-red-500/80 shadow-[0_0_15px_rgba(239,68,68,0.15)]'
                : 'bg-zinc-950 border-white/10'
            }`}
          >
            <span className="text-[10px] font-black uppercase tracking-widest text-red-400 block mb-1 flex items-center justify-between">
              Action Required ⚡
            </span>
            <div className="text-2xl font-black text-red-400">
              {report.summary.actionRequired}
            </div>
            <span className="text-[11px] text-red-400/60 mt-1 block">Needs renewal triage</span>
          </div>

          <div className="p-4 rounded-lg bg-zinc-950 border-white/10 border">
            <span className="text-[10px] font-black uppercase tracking-widest text-amber-300 block mb-1">
              Critical (≤30d)
            </span>
            <div className="text-2xl font-black text-amber-300">{report.summary.critical}</div>
            <span className="text-[11px] text-amber-300/60 mt-1 block">Imminent expiry</span>
          </div>

          <div className="p-4 rounded-lg bg-zinc-950 border-white/10 border">
            <span className="text-[10px] font-black uppercase tracking-widest text-cyan-400 block mb-1">
              Upcoming (≤90d)
            </span>
            <div className="text-2xl font-black text-cyan-400">{report.summary.upcoming}</div>
            <span className="text-[11px] text-cyan-400/60 mt-1 block">Within 3 months</span>
          </div>

          <div className="p-4 rounded-lg bg-zinc-950 border-white/10 border">
            <span className="text-[10px] font-black uppercase tracking-widest text-emerald-400 block mb-1">
              Healthy / Valid
            </span>
            <div className="text-2xl font-black text-emerald-400">{report.summary.healthy}</div>
            <span className="text-[11px] text-emerald-400/60 mt-1 block">Good standing</span>
          </div>
        </div>
      )}

      {/* Filter Tabs & Search Controls */}
      <div className="bg-zinc-900/50 p-4 rounded-lg border border-white/10 flex flex-col md:flex-row items-center justify-between gap-4">
        {/* Navigation Tabs */}
        <div className="flex items-center gap-1 p-1 bg-black/40 rounded border border-white/5 w-full md:w-auto">
          <button
            type="button"
            onClick={() => setActiveTab('all')}
            className={`px-3 py-1.5 rounded text-xs font-bold uppercase tracking-wider transition-all ${
              activeTab === 'all'
                ? 'bg-accent/20 text-accent border border-accent/40'
                : 'text-white/50 hover:text-white'
            }`}
          >
            All Records ({report?.summary?.total || 0})
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('action')}
            className={`px-3 py-1.5 rounded text-xs font-bold uppercase tracking-wider transition-all flex items-center gap-1.5 ${
              activeTab === 'action'
                ? 'bg-red-500/20 text-red-400 border border-red-500/40'
                : 'text-white/50 hover:text-white'
            }`}
          >
            <span>🚨 Warnings</span>
            {report?.summary?.actionRequired > 0 && (
              <span className="px-1.5 py-0.2 bg-red-500 text-black text-[10px] font-black rounded-full">
                {report.summary.actionRequired}
              </span>
            )}
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('category')}
            className={`px-3 py-1.5 rounded text-xs font-bold uppercase tracking-wider transition-all ${
              activeTab === 'category'
                ? 'bg-accent/20 text-accent border border-accent/40'
                : 'text-white/50 hover:text-white'
            }`}
          >
            By Category
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('owner')}
            className={`px-3 py-1.5 rounded text-xs font-bold uppercase tracking-wider transition-all ${
              activeTab === 'owner'
                ? 'bg-accent/20 text-accent border border-accent/40'
                : 'text-white/50 hover:text-white'
            }`}
          >
            By Owner
          </button>
        </div>

        {/* Dynamic Selectors & Search Input */}
        <div className="flex flex-wrap items-center gap-3 w-full md:w-auto">
          {activeTab === 'category' && (
            <select
              value={selectedCategory}
              onChange={(e) => setSelectedCategory(e.target.value)}
              className="bg-zinc-950 border border-white/10 text-white rounded px-3 py-1.5 text-xs focus:border-accent outline-none"
            >
              <option value="ALL">All Categories</option>
              {availableCategories.map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </select>
          )}

          {activeTab === 'owner' && (
            <select
              value={selectedOwner}
              onChange={(e) => setSelectedOwner(e.target.value)}
              className="bg-zinc-950 border border-white/10 text-white rounded px-3 py-1.5 text-xs focus:border-accent outline-none"
            >
              <option value="ALL">All Owners</option>
              {availableOwners.map((o) => (
                <option key={o} value={o}>
                  {o}
                </option>
              ))}
            </select>
          )}

          <div className="relative flex-1 md:w-64">
            <input
              type="text"
              placeholder="Search title, category, owner..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="w-full bg-zinc-950 border border-white/10 rounded px-3 py-1.5 pl-8 text-xs text-white placeholder-white/30 focus:border-accent outline-none"
            />
            <span className="absolute left-2.5 top-2 text-xs opacity-40">🔍</span>
            {searchTerm && (
              <button
                type="button"
                onClick={() => setSearchTerm('')}
                className="absolute right-2 top-1.5 text-xs text-white/40 hover:text-white"
              >
                ✕
              </button>
            )}
          </div>
        </div>
      </div>

      {/* Error Banner */}
      {error && (
        <div className="p-4 rounded-lg bg-red-950/30 border border-red-500/50 text-red-300 text-xs">
          ⚠️ {error}
        </div>
      )}

      {/* Main Records Table / Grid */}
      {loading ? (
        <div className="p-12 text-center text-white/40 animate-pulse text-sm">
          Loading household records radar...
        </div>
      ) : filteredItems.length === 0 ? (
        <div className="p-12 text-center bg-zinc-900/30 border border-white/5 rounded-lg">
          <div className="text-3xl mb-3">📁</div>
          <p className="text-sm font-bold text-white/60">No household records found</p>
          <p className="text-xs text-white/30 mt-1">
            {searchTerm
              ? 'Try modifying your search or filter criteria.'
              : 'Add your first document index using the button above.'}
          </p>
        </div>
      ) : (
        <div className="bg-zinc-900/50 rounded-lg border border-white/10 overflow-hidden shadow-2xl">
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs border-collapse">
              <thead>
                <tr className="border-b border-white/10 bg-white/[0.02] text-white/40 uppercase tracking-widest font-black text-[10px]">
                  <th className="p-4">Document / Record</th>
                  <th className="p-4">Category</th>
                  <th className="p-4">Owner</th>
                  <th className="p-4">Expiration / Status</th>
                  <th className="p-4">Recurrence & Projection</th>
                  <th className="p-4">Storage Pointer</th>
                  <th className="p-4 text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-white/5">
                {filteredItems.map(({ record, bucket, daysUntilExpiration, nextRenewalDate, isActionRequired }) => (
                  <tr
                    key={record.id}
                    className={`hover:bg-white/[0.02] transition-colors ${
                      isActionRequired ? 'bg-red-500/[0.02]' : ''
                    }`}
                  >
                    <td className="p-4">
                      <div className="font-bold text-white text-sm flex items-center gap-2">
                        {record.title}
                        {isActionRequired && (
                          <span className="w-2 h-2 rounded-full bg-red-500 animate-ping" />
                        )}
                      </div>
                      {record.notes && (
                        <div className="text-[11px] text-white/40 mt-1 max-w-xs truncate" title={record.notes}>
                          {record.notes}
                        </div>
                      )}
                    </td>

                    <td className="p-4">
                      <span className="px-2 py-0.5 rounded bg-white/5 border border-white/10 text-white/70 font-medium">
                        {record.category}
                      </span>
                    </td>

                    <td className="p-4">
                      <span className="text-white/80 font-bold">{record.owner}</span>
                    </td>

                    <td className="p-4 space-y-1">
                      <div>{renderBucketBadge(bucket, daysUntilExpiration, isActionRequired)}</div>
                      {record.expirationDate && (
                        <div className="text-[11px] text-white/40 font-mono">
                          Expires: {record.expirationDate}
                        </div>
                      )}
                    </td>

                    <td className="p-4 text-[11px] text-white/60">
                      {record.recurrenceRule ? (
                        <div className="space-y-0.5">
                          <div className="font-semibold text-white/80 capitalize">
                            🔄 {typeof record.recurrenceRule.interval === 'string'
                              ? record.recurrenceRule.interval.replace('_', ' ')
                              : 'Custom'}
                          </div>
                          {nextRenewalDate && (
                            <div className="text-accent/80 font-mono text-[10px]">
                              Next: {nextRenewalDate}
                            </div>
                          )}
                        </div>
                      ) : (
                        <span className="text-white/30">—</span>
                      )}
                    </td>

                    <td className="p-4">
                      {record.filePath ? (
                        <div className="flex items-center gap-2">
                          <button
                            type="button"
                            onClick={() => handleRevealFile(record.filePath)}
                            className="text-accent hover:underline text-[11px] font-mono truncate max-w-xs block text-left"
                            title={`Click to reveal in Explorer: ${record.filePath}`}
                          >
                            📂 {record.filePath.split(/[\\/]/).pop()}
                          </button>
                        </div>
                      ) : (
                        <span className="text-white/20 italic text-[11px]">Unlinked</span>
                      )}
                    </td>

                    <td className="p-4 text-right">
                      <div className="flex items-center justify-end gap-2">
                        {record.filePath && (
                          <button
                            type="button"
                            onClick={() => handleRevealFile(record.filePath)}
                            title="Reveal in File Explorer"
                            className="p-1.5 bg-white/5 hover:bg-accent/20 hover:text-accent rounded transition-all text-white/60"
                          >
                            🔍
                          </button>
                        )}
                        <button
                          type="button"
                          onClick={() => openEditModal(record)}
                          title="Edit Record"
                          className="p-1.5 bg-white/5 hover:bg-white/10 rounded transition-all text-white/70 hover:text-white"
                        >
                          ✏️
                        </button>
                        <button
                          type="button"
                          onClick={() => setRecordToDelete(record)}
                          title="Delete Record"
                          className="p-1.5 bg-white/5 hover:bg-red-500/20 hover:text-red-400 rounded transition-all text-white/40"
                        >
                          🗑️
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Record Creation / Editing Modal */}
      {isModalOpen && (
        <div className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4">
          <div className="bg-zinc-950 border border-white/20 rounded-xl max-w-xl w-full p-6 shadow-2xl animate-scaleUp">
            <div className="flex items-center justify-between pb-4 border-b border-white/10 mb-4">
              <h2 className="text-lg font-black uppercase tracking-wider text-white">
                {editingRecord ? '✏️ Edit Household Record' : '➕ Index New Household Record'}
              </h2>
              <button
                type="button"
                onClick={() => setIsModalOpen(false)}
                className="text-white/40 hover:text-white text-lg font-bold"
              >
                ✕
              </button>
            </div>

            {modalError && (
              <div className="mb-4 p-3 rounded bg-red-950/40 border border-red-500/50 text-red-300 text-xs">
                ⚠️ {modalError}
              </div>
            )}

            <form onSubmit={handleSaveRecord} className="space-y-4">
              <div>
                <label className="block text-[11px] font-bold uppercase tracking-wider text-white/60 mb-1">
                  Record Title *
                </label>
                <input
                  type="text"
                  required
                  placeholder="e.g. US Passport, 2024 Subaru Registration"
                  value={formTitle}
                  onChange={(e) => setFormTitle(e.target.value)}
                  className="w-full bg-zinc-900 border border-white/10 rounded px-3 py-2 text-xs text-white focus:border-accent outline-none"
                />
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-[11px] font-bold uppercase tracking-wider text-white/60 mb-1">
                    Category *
                  </label>
                  <select
                    value={formCategory}
                    onChange={(e) => setFormCategory(e.target.value)}
                    className="w-full bg-zinc-900 border border-white/10 rounded px-3 py-2 text-xs text-white focus:border-accent outline-none"
                  >
                    {CATEGORIES.map((c) => (
                      <option key={c} value={c}>
                        {c}
                      </option>
                    ))}
                  </select>
                </div>

                <div>
                  <label className="block text-[11px] font-bold uppercase tracking-wider text-white/60 mb-1">
                    Owner / Beneficiary *
                  </label>
                  <input
                    type="text"
                    required
                    placeholder="e.g. Kevin, Family, Shared"
                    value={formOwner}
                    onChange={(e) => setFormOwner(e.target.value)}
                    className="w-full bg-zinc-900 border border-white/10 rounded px-3 py-2 text-xs text-white focus:border-accent outline-none"
                  />
                </div>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-[11px] font-bold uppercase tracking-wider text-white/60 mb-1">
                    Expiration Date (YYYY-MM-DD)
                  </label>
                  <input
                    type="date"
                    value={formExpirationDate}
                    onChange={(e) => setFormExpirationDate(e.target.value)}
                    className="w-full bg-zinc-900 border border-white/10 rounded px-3 py-2 text-xs text-white focus:border-accent outline-none"
                  />
                </div>

                <div>
                  <label className="block text-[11px] font-bold uppercase tracking-wider text-white/60 mb-1">
                    Recurrence Interval
                  </label>
                  <select
                    value={formRecurrenceInterval}
                    onChange={(e) => setFormRecurrenceInterval(e.target.value)}
                    className="w-full bg-zinc-900 border border-white/10 rounded px-3 py-2 text-xs text-white focus:border-accent outline-none"
                  >
                    {RECURRENCE_OPTIONS.map((opt) => (
                      <option key={opt.value} value={opt.value}>
                        {opt.label}
                      </option>
                    ))}
                  </select>
                </div>
              </div>

              {formRecurrenceInterval !== 'none' && (
                <div>
                  <label className="block text-[11px] font-bold uppercase tracking-wider text-white/60 mb-1">
                    Lead Notice Days (Action Warning Threshold)
                  </label>
                  <input
                    type="number"
                    min="1"
                    max="365"
                    value={formLeadTimeDays}
                    onChange={(e) => setFormLeadTimeDays(e.target.value)}
                    className="w-full bg-zinc-900 border border-white/10 rounded px-3 py-2 text-xs text-white focus:border-accent outline-none"
                  />
                  <span className="text-[10px] text-white/40 mt-1 block">
                    Triggers action warning when expiration is within this many days.
                  </span>
                </div>
              )}

              <div>
                <label className="block text-[11px] font-bold uppercase tracking-wider text-white/60 mb-1">
                  File Path (Absolute or Vault-Relative)
                </label>
                <input
                  type="text"
                  placeholder="e.g. G:\My Drive\Navis-Vault\Personal\Passports\kevin.pdf"
                  value={formFilePath}
                  onChange={(e) => setFormFilePath(e.target.value)}
                  className="w-full bg-zinc-900 border border-white/10 rounded px-3 py-2 text-xs text-white focus:border-accent outline-none font-mono"
                />
              </div>

              <div>
                <label className="block text-[11px] font-bold uppercase tracking-wider text-white/60 mb-1">
                  Notes & Physical Location
                </label>
                <textarea
                  rows="2"
                  placeholder="e.g. Original physical copy kept in Safe Box #2; renewal link on state portal"
                  value={formNotes}
                  onChange={(e) => setFormNotes(e.target.value)}
                  className="w-full bg-zinc-900 border border-white/10 rounded px-3 py-2 text-xs text-white focus:border-accent outline-none"
                />
              </div>

              <div className="flex items-center justify-end gap-3 pt-4 border-t border-white/10">
                <button
                  type="button"
                  onClick={() => setIsModalOpen(false)}
                  disabled={isSaving}
                  className="px-4 py-2 bg-white/5 hover:bg-white/10 rounded text-xs font-bold uppercase tracking-wider text-white/70"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={isSaving}
                  className="px-4 py-2 bg-accent hover:bg-accent/90 text-black font-black rounded text-xs uppercase tracking-widest shadow-[0_0_15px_rgba(0,212,255,0.3)] transition-all"
                >
                  {isSaving ? 'Saving...' : editingRecord ? 'Save Changes' : 'Index Record'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Delete Confirmation Dialog */}
      {recordToDelete && (
        <div className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4">
          <div className="bg-zinc-950 border border-red-500/40 rounded-xl max-w-md w-full p-6 shadow-2xl">
            <h3 className="text-base font-black uppercase tracking-wider text-red-400 mb-2">
              🗑️ Delete Record Index?
            </h3>
            <p className="text-xs text-white/70 mb-4">
              Are you sure you want to remove <span className="font-bold text-white">"{recordToDelete.title}"</span> from the household index?
              <br />
              <span className="text-[10px] text-white/40 mt-1 block">
                (This only removes the registry pointer; your underlying physical or digital file remains untouched.)
              </span>
            </p>
            <div className="flex items-center justify-end gap-3">
              <button
                type="button"
                onClick={() => setRecordToDelete(null)}
                className="px-4 py-2 bg-white/5 hover:bg-white/10 rounded text-xs font-bold uppercase text-white/70"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleDeleteConfirm}
                className="px-4 py-2 bg-red-600 hover:bg-red-500 text-white font-bold rounded text-xs uppercase tracking-wider transition-all"
              >
                Delete Record
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
