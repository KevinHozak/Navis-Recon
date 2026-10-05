import { useState, useEffect, useMemo } from 'react';
import { getRecurringObligations, saveSubscriptionMetadata } from '../services/apiClient';

export default function SubscriptionsWidget() {
  const [obligations, setObligations] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  // Filters & Search
  const [searchTerm, setSearchTerm] = useState('');
  const [filterCadence, setFilterCadence] = useState('ALL');
  const [filterAlertOnly, setFilterAlertOnly] = useState(false);
  const [filterAccount, setFilterAccount] = useState('ALL');

  // Metadata Edit Modal / Drawer
  const [editingItem, setEditingItem] = useState(null);
  const [isSaving, setIsSaving] = useState(false);
  const [saveSuccessMsg, setSaveSuccessMsg] = useState(null);
  const [saveErrorMsg, setSaveErrorMsg] = useState(null);

  // Edit Form Fields
  const [editBillingMethod, setEditBillingMethod] = useState('');
  const [editCancellationUrl, setEditCancellationUrl] = useState('');
  const [editNotes, setEditNotes] = useState('');
  const [editIsTrial, setEditIsTrial] = useState(false);
  const [editTrialEndDate, setEditTrialEndDate] = useState('');
  const [editAlertLeadDays, setEditAlertLeadDays] = useState('14');
  const [editCustomLabel, setEditCustomLabel] = useState('');
  const [editIsReviewed, setEditIsReviewed] = useState(false);

  async function loadData() {
    setLoading(true);
    setError(null);
    try {
      const data = await getRecurringObligations();
      setObligations(Array.isArray(data) ? data : []);
    } catch (err) {
      console.error('Failed to load obligations:', err);
      setError(err?.message || String(err) || 'Failed to load recurring obligations');
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    loadData();
  }, []);

  // Compute aggregate metrics
  const metrics = useMemo(() => {
    if (!obligations || obligations.length === 0) {
      return {
        totalSubscriptions: 0,
        monthlyRunRate: 0,
        currentMonthDue: 0,
        nextMonthDue: 0,
        alertsCount: 0,
        priceHikesCount: 0,
        activeTrialsCount: 0,
        accounts: [],
      };
    }

    const today = new Date();
    const curYear = today.getFullYear();
    const curMonth = today.getMonth() + 1; // 1-12
    const nextMonth = curMonth === 12 ? 1 : curMonth + 1;
    const nextYear = curMonth === 12 ? curYear + 1 : curYear;

    const curMonthPrefix = `${curYear}-${String(curMonth).padStart(2, '0')}`;
    const nextMonthPrefix = `${nextYear}-${String(nextMonth).padStart(2, '0')}`;

    // Target horizon: end of next month
    const nextMonthDays = new Date(nextYear, nextMonth, 0).getDate();
    const horizonEndDateStr = `${nextYear}-${String(nextMonth).padStart(2, '0')}-${String(nextMonthDays).padStart(2, '0')}`;

    let monthlyRunRate = 0;
    let currentMonthDue = 0;
    let nextMonthDue = 0;
    let alertsCount = 0;
    let priceHikesCount = 0;
    let activeTrialsCount = 0;
    const accountSet = new Set();

    // Helper to step a date forward by cadence
    const stepNextDate = (dStr, cad) => {
      const parts = dStr.split('-');
      if (parts.length < 3) return null;
      const y = parseInt(parts[0], 10);
      const m = parseInt(parts[1], 10);
      const d = parseInt(parts[2], 10);

      if (cad === 'weekly') {
        const dt = new Date(Date.UTC(y, m - 1, d));
        dt.setUTCDate(dt.getUTCDate() + 7);
        return dt.toISOString().slice(0, 10);
      }
      if (cad === 'biweekly') {
        const dt = new Date(Date.UTC(y, m - 1, d));
        dt.setUTCDate(dt.getUTCDate() + 14);
        return dt.toISOString().slice(0, 10);
      }
      if (cad === 'monthly') {
        const totalM = (y * 12 + (m - 1)) + 1;
        const ny = Math.floor(totalM / 12);
        const nm = (totalM % 12) + 1;
        const maxD = new Date(ny, nm, 0).getDate();
        const nd = Math.min(d, maxD);
        return `${ny}-${String(nm).padStart(2, '0')}-${String(nd).padStart(2, '0')}`;
      }
      if (cad === 'quarterly') {
        const totalM = (y * 12 + (m - 1)) + 3;
        const ny = Math.floor(totalM / 12);
        const nm = (totalM % 12) + 1;
        const maxD = new Date(ny, nm, 0).getDate();
        const nd = Math.min(d, maxD);
        return `${ny}-${String(nm).padStart(2, '0')}-${String(nd).padStart(2, '0')}`;
      }
      if (cad === 'semi_annual') {
        const totalM = (y * 12 + (m - 1)) + 6;
        const ny = Math.floor(totalM / 12);
        const nm = (totalM % 12) + 1;
        const maxD = new Date(ny, nm, 0).getDate();
        const nd = Math.min(d, maxD);
        return `${ny}-${String(nm).padStart(2, '0')}-${String(nd).padStart(2, '0')}`;
      }
      if (cad === 'annual') {
        const ny = y + 1;
        const maxD = new Date(ny, m, 0).getDate();
        const nd = Math.min(d, maxD);
        return `${ny}-${String(m).padStart(2, '0')}-${String(nd).padStart(2, '0')}`;
      }
      return null;
    };

    for (const ob of obligations) {
      if (ob.account) accountSet.add(ob.account);

      // Normalization factor to monthly run-rate
      const amt = ob.median_amount || ob.latest_amount || 0;
      switch (ob.cadence) {
        case 'weekly':
          monthlyRunRate += amt * (52 / 12);
          break;
        case 'biweekly':
          monthlyRunRate += amt * (26 / 12);
          break;
        case 'monthly':
          monthlyRunRate += amt;
          break;
        case 'quarterly':
          monthlyRunRate += amt / 3;
          break;
        case 'semi_annual':
          monthlyRunRate += amt / 6;
          break;
        case 'annual':
          monthlyRunRate += amt / 12;
          break;
        default:
          monthlyRunRate += amt;
          break;
      }

      // Project all occurrences falling within current month and next month
      let projDate = ob.next_projected_date;
      let iterations = 0;
      const chargeAmt = ob.latest_amount || amt;

      while (projDate && projDate <= horizonEndDateStr && iterations < 12) {
        iterations += 1;
        if (projDate.startsWith(curMonthPrefix)) {
          currentMonthDue += chargeAmt;
        } else if (projDate.startsWith(nextMonthPrefix)) {
          nextMonthDue += chargeAmt;
        }
        projDate = stepNextDate(projDate, ob.cadence);
      }

      // Count alerts
      if (ob.alerts && ob.alerts.length > 0) {
        alertsCount += ob.alerts.length;
        for (const al of ob.alerts) {
          if (al.kind === 'price_hike') priceHikesCount += 1;
        }
      }

      // Count trial once (from metadata directly)
      if (ob.metadata?.is_trial) {
        activeTrialsCount += 1;
      }
    }

    return {
      totalSubscriptions: obligations.length,
      monthlyRunRate: Math.round(monthlyRunRate * 100) / 100,
      currentMonthDue: Math.round(currentMonthDue * 100) / 100,
      nextMonthDue: Math.round(nextMonthDue * 100) / 100,
      alertsCount,
      priceHikesCount,
      activeTrialsCount,
      accounts: Array.from(accountSet).sort(),
    };
  }, [obligations]);

  // Filtered & sorted obligations list
  const filteredObligations = useMemo(() => {
    return obligations.filter((ob) => {
      // Cadence filter
      if (filterCadence !== 'ALL' && ob.cadence !== filterCadence) {
        return false;
      }
      // Account filter
      if (filterAccount !== 'ALL' && ob.account !== filterAccount) {
        return false;
      }
      // Alerts only filter
      if (filterAlertOnly && (!ob.alerts || ob.alerts.length === 0)) {
        return false;
      }
      // Search term
      if (searchTerm.trim()) {
        const query = searchTerm.toLowerCase();
        const payee = (ob.payee || '').toLowerCase();
        const account = (ob.account || '').toLowerCase();
        const notes = (ob.metadata?.notes || '').toLowerCase();
        const label = (ob.metadata?.custom_label || '').toLowerCase();
        const method = (ob.metadata?.billing_method || '').toLowerCase();
        if (
          !payee.includes(query) &&
          !account.includes(query) &&
          !notes.includes(query) &&
          !label.includes(query) &&
          !method.includes(query)
        ) {
          return false;
        }
      }
      return true;
    });
  }, [obligations, filterCadence, filterAccount, filterAlertOnly, searchTerm]);

  // Open Edit Drawer
  const handleEdit = (ob) => {
    setEditingItem(ob);
    setSaveSuccessMsg(null);
    setSaveErrorMsg(null);
    setEditBillingMethod(ob.metadata?.billing_method || '');
    setEditCancellationUrl(ob.metadata?.cancellation_url || '');
    setEditNotes(ob.metadata?.notes || '');
    setEditIsTrial(Boolean(ob.metadata?.is_trial));
    setEditTrialEndDate(ob.metadata?.trial_end_date || '');
    setEditAlertLeadDays(ob.metadata?.alert_lead_days ? String(ob.metadata.alert_lead_days) : '14');
    setEditCustomLabel(ob.metadata?.custom_label || '');
    setEditIsReviewed(Boolean(ob.metadata?.reviewed ?? ob.metadata?.extra?.reviewed));
  };

  // Close Edit Drawer
  const handleCloseEdit = () => {
    setEditingItem(null);
    setSaveSuccessMsg(null);
    setSaveErrorMsg(null);
  };

  // Save metadata
  const handleSaveMetadata = async (e) => {
    e?.preventDefault();
    if (!editingItem) return;

    setIsSaving(true);
    setSaveSuccessMsg(null);
    setSaveErrorMsg(null);

    const leadDaysNum = parseInt(editAlertLeadDays, 10);

    const newMetadata = {
      key: editingItem.key,
      payee: editingItem.payee,
      account: editingItem.account,
      billing_method: editBillingMethod.trim() || null,
      cancellation_url: editCancellationUrl.trim() || null,
      notes: editNotes.trim() || null,
      is_trial: editIsTrial,
      trial_end_date: editTrialEndDate.trim() || null,
      alert_lead_days: Number.isNaN(leadDaysNum) ? null : leadDaysNum,
      custom_label: editCustomLabel.trim() || null,
      reviewed: editIsReviewed,
    };

    try {
      await saveSubscriptionMetadata(newMetadata);
      setSaveSuccessMsg('Metadata saved successfully!');
      // Update local state directly to maintain snappy UI
      setObligations((prev) =>
        prev.map((item) => {
          if (item.key === editingItem.key) {
            return {
              ...item,
              metadata: {
                ...item.metadata,
                ...newMetadata,
                reviewed: editIsReviewed,
              },
            };
          }
          return item;
        })
      );
      setTimeout(() => {
        handleCloseEdit();
      }, 700);
    } catch (err) {
      console.error('Failed to save metadata:', err);
      setSaveErrorMsg(err?.message || String(err) || 'Error saving metadata');
    } finally {
      setIsSaving(false);
    }
  };

  // Quick toggle review status without opening full modal
  const handleToggleReviewed = async (ob) => {
    const currentReviewed = Boolean(ob.metadata?.reviewed ?? ob.metadata?.extra?.reviewed);
    const updatedMetadata = {
      ...(ob.metadata || {
        key: ob.key,
        payee: ob.payee,
        account: ob.account,
      }),
      reviewed: !currentReviewed,
    };

    try {
      await saveSubscriptionMetadata(updatedMetadata);
      setObligations((prev) =>
        prev.map((item) => {
          if (item.key === ob.key) {
            return {
              ...item,
              metadata: {
                ...item.metadata,
                ...updatedMetadata,
                reviewed: !currentReviewed,
              },
            };
          }
          return item;
        })
      );
    } catch (err) {
      console.error('Failed to toggle reviewed status:', err);
    }
  };

  const formatCurrency = (val) => {
    if (typeof val !== 'number' || Number.isNaN(val)) return '$0.00';
    return `$${val.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
  };

  const getCadenceBadge = (cadence) => {
    switch (cadence) {
      case 'weekly':
        return <span className="px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider bg-purple-500/10 text-purple-400 border border-purple-500/30">Weekly</span>;
      case 'biweekly':
        return <span className="px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider bg-blue-500/10 text-blue-400 border border-blue-500/30">Bi-Weekly</span>;
      case 'monthly':
        return <span className="px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider bg-cyan-500/10 text-cyan-400 border border-cyan-500/30">Monthly</span>;
      case 'quarterly':
        return <span className="px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider bg-amber-500/10 text-amber-400 border border-amber-500/30">Quarterly</span>;
      case 'semi_annual':
        return <span className="px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider bg-orange-500/10 text-orange-400 border border-orange-500/30">Semi-Annual</span>;
      case 'annual':
        return <span className="px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider bg-emerald-500/10 text-emerald-400 border border-emerald-500/30">Annual</span>;
      default:
        return <span className="px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider bg-zinc-800 text-zinc-400 border border-zinc-700">{cadence}</span>;
    }
  };

  return (
    <div className="space-y-8">
      {/* Header Banner */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 border-b border-white/10 pb-6">
        <div>
          <div className="flex items-center gap-3">
            <span className="text-3xl">💳</span>
            <h1 className="text-2xl font-black uppercase tracking-widest text-white">Upcoming Obligations & Subscriptions</h1>
          </div>
          <p className="text-sm text-white/50 mt-1">
            Predictive recurring charge triage, commitment forecasts, and renewal risk monitor.
          </p>
        </div>
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={loadData}
            disabled={loading}
            className="flex items-center gap-2 px-4 py-2 bg-zinc-900 border border-white/10 hover:border-accent/40 rounded text-xs font-bold uppercase tracking-widest text-white transition-all hover:bg-white/5 disabled:opacity-50"
          >
            <span>🔄</span> Refresh Telemetry
          </button>
        </div>
      </div>

      {/* Top Telemetry KPI Cards */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Monthly Overhead Card */}
        <div className="bg-zinc-950 p-5 rounded-lg border border-white/10 relative overflow-hidden shadow-lg">
          <div className="flex justify-between items-start">
            <div>
              <p className="text-[10px] font-bold uppercase tracking-widest text-white/40">Monthly Run-Rate</p>
              <h2 className="text-2xl font-black text-accent mt-1 tracking-tight">
                {formatCurrency(metrics.monthlyRunRate)}
              </h2>
            </div>
            <span className="text-xl opacity-60">📈</span>
          </div>
          <div className="mt-3 text-xs text-white/50 flex items-center justify-between border-t border-white/5 pt-2">
            <span>Active Subscriptions</span>
            <span className="font-mono font-bold text-white/80">{metrics.totalSubscriptions}</span>
          </div>
        </div>

        {/* Current Month Commitment */}
        <div className="bg-zinc-950 p-5 rounded-lg border border-white/10 relative overflow-hidden shadow-lg">
          <div className="flex justify-between items-start">
            <div>
              <p className="text-[10px] font-bold uppercase tracking-widest text-white/40">This Month Due</p>
              <h2 className="text-2xl font-black text-emerald-400 mt-1 tracking-tight">
                {formatCurrency(metrics.currentMonthDue)}
              </h2>
            </div>
            <span className="text-xl opacity-60">🗓️</span>
          </div>
          <div className="mt-3 text-xs text-white/50 flex items-center justify-between border-t border-white/5 pt-2">
            <span>Next Month Projection</span>
            <span className="font-mono font-bold text-white/80">{formatCurrency(metrics.nextMonthDue)}</span>
          </div>
        </div>

        {/* Renewal & Risk Radar */}
        <div className="bg-zinc-950 p-5 rounded-lg border border-white/10 relative overflow-hidden shadow-lg">
          <div className="flex justify-between items-start">
            <div>
              <p className="text-[10px] font-bold uppercase tracking-widest text-white/40">Active Alerts</p>
              <h2 className={`text-2xl font-black mt-1 tracking-tight ${metrics.alertsCount > 0 ? 'text-amber-400' : 'text-white/80'}`}>
                {metrics.alertsCount}
              </h2>
            </div>
            <span className="text-xl opacity-60">⚠️</span>
          </div>
          <div className="mt-3 text-xs text-white/50 flex items-center justify-between border-t border-white/5 pt-2">
            <span>Price Hikes Flagged</span>
            <span className={`font-mono font-bold ${metrics.priceHikesCount > 0 ? 'text-red-400' : 'text-white/80'}`}>
              {metrics.priceHikesCount}
            </span>
          </div>
        </div>

        {/* Trial & Renegotiation Watch */}
        <div className="bg-zinc-950 p-5 rounded-lg border border-white/10 relative overflow-hidden shadow-lg">
          <div className="flex justify-between items-start">
            <div>
              <p className="text-[10px] font-bold uppercase tracking-widest text-white/40">Active Trials</p>
              <h2 className="text-2xl font-black text-purple-400 mt-1 tracking-tight">
                {metrics.activeTrialsCount}
              </h2>
            </div>
            <span className="text-xl opacity-60">⏳</span>
          </div>
          <div className="mt-3 text-xs text-white/50 flex items-center justify-between border-t border-white/5 pt-2">
            <span>Review Coverage</span>
            <span className="font-mono font-bold text-white/80">
              {obligations.filter((o) => o.metadata?.reviewed ?? o.metadata?.extra?.reviewed).length} / {obligations.length}
            </span>
          </div>
        </div>
      </div>

      {/* Control Bar: Filters & Search */}
      <div className="bg-zinc-950 p-4 rounded-lg border border-white/10 flex flex-col md:flex-row gap-4 items-center justify-between">
        <div className="flex-1 w-full md:w-auto relative">
          <span className="absolute left-3 top-2.5 text-white/30 text-sm">🔍</span>
          <input
            type="text"
            placeholder="Search by payee, account, notes, label..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="w-full bg-zinc-900 border border-white/10 rounded pl-9 pr-4 py-2 text-sm text-white placeholder-white/30 focus:outline-none focus:border-accent/50"
          />
        </div>

        <div className="flex flex-wrap items-center gap-3 w-full md:w-auto">
          {/* Cadence Dropdown */}
          <select
            value={filterCadence}
            onChange={(e) => setFilterCadence(e.target.value)}
            aria-label="Filter obligations by cadence"
            className="bg-zinc-900 border border-white/10 rounded px-3 py-2 text-xs font-bold uppercase tracking-wider text-white focus:outline-none focus:border-accent/50"
          >
            <option value="ALL">All Cadences</option>
            <option value="weekly">Weekly</option>
            <option value="biweekly">Bi-Weekly</option>
            <option value="monthly">Monthly</option>
            <option value="quarterly">Quarterly</option>
            <option value="semi_annual">Semi-Annual</option>
            <option value="annual">Annual</option>
          </select>

          {/* Account Dropdown */}
          <select
            value={filterAccount}
            onChange={(e) => setFilterAccount(e.target.value)}
            aria-label="Filter obligations by account"
            className="bg-zinc-900 border border-white/10 rounded px-3 py-2 text-xs font-bold uppercase tracking-wider text-white focus:outline-none focus:border-accent/50 max-w-[180px] truncate"
          >
            <option value="ALL">All Accounts</option>
            {metrics.accounts.map((acc) => (
              <option key={acc} value={acc}>
                {acc}
              </option>
            ))}
          </select>

          {/* Alert Only Toggle */}
          <button
            type="button"
            onClick={() => setFilterAlertOnly(!filterAlertOnly)}
            className={`flex items-center gap-2 px-3 py-2 rounded text-xs font-bold uppercase tracking-wider transition-all border ${
              filterAlertOnly
                ? 'bg-amber-500/20 text-amber-300 border-amber-500/50'
                : 'bg-zinc-900 text-white/50 border-white/10 hover:text-white'
            }`}
          >
            <span>⚠️</span> Only Alerts ({metrics.alertsCount})
          </button>
        </div>
      </div>

      {/* Main Obligations Stream / List */}
      {loading ? (
        <div className="bg-zinc-950 p-12 rounded-lg border border-white/10 text-center space-y-4">
          <div className="inline-block animate-spin text-2xl">⏳</div>
          <p className="text-sm font-bold uppercase tracking-widest text-white/50">Analyzing ledger recurring obligations & registry telemetry...</p>
        </div>
      ) : error ? (
        <div className="bg-red-950/20 border border-red-500/40 p-6 rounded-lg text-center space-y-3">
          <p className="text-red-400 font-bold text-sm">Failed to load obligations</p>
          <p className="text-xs text-red-300/70 font-mono">{error}</p>
          <button
            type="button"
            onClick={loadData}
            className="px-4 py-2 bg-red-900/40 hover:bg-red-900/60 text-red-200 border border-red-500/30 rounded text-xs uppercase font-bold tracking-wider"
          >
            Retry
          </button>
        </div>
      ) : filteredObligations.length === 0 ? (
        <div className="bg-zinc-950 p-12 rounded-lg border border-white/10 text-center space-y-2">
          <p className="text-white/60 font-bold">No recurring obligations matched current filters.</p>
          <p className="text-xs text-white/30">Adjust the search parameters or filter toggles above.</p>
        </div>
      ) : (
        <div className="bg-zinc-950 rounded-lg border border-white/10 overflow-hidden shadow-2xl">
          <div className="overflow-x-auto">
            <table className="w-full text-left border-collapse">
              <thead>
                <tr className="border-b border-white/10 bg-zinc-900/50 text-[10px] font-black uppercase tracking-widest text-white/40">
                  <th className="py-3 px-4">Payee / Subscription</th>
                  <th className="py-3 px-4">Cadence</th>
                  <th className="py-3 px-4">Next Due Date</th>
                  <th className="py-3 px-4 text-right">Latest / Baseline</th>
                  <th className="py-3 px-4">Risk & Alerts</th>
                  <th className="py-3 px-4">Funding Source</th>
                  <th className="py-3 px-4 text-center">Status</th>
                  <th className="py-3 px-4 text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-white/5 text-sm">
                {filteredObligations.map((ob) => {
                  const hasAlerts = ob.alerts && ob.alerts.length > 0;
                  const isReviewed = Boolean(ob.metadata?.reviewed ?? ob.metadata?.extra?.reviewed);
                  const priceDelta = ob.price_delta;

                  return (
                    <tr
                      key={ob.key}
                      className={`hover:bg-white/[0.02] transition-colors ${
                        hasAlerts ? 'bg-amber-500/[0.02]' : ''
                      }`}
                    >
                      {/* Payee & Custom Label */}
                      <td className="py-4 px-4">
                        <div className="flex flex-col">
                          <div className="flex items-center gap-2">
                            <span className="font-bold text-white tracking-wide">
                              {ob.metadata?.custom_label || ob.payee}
                            </span>
                            {ob.metadata?.is_trial && (
                              <span className="px-1.5 py-0.5 rounded text-[9px] font-black uppercase tracking-wider bg-purple-500/20 text-purple-300 border border-purple-500/40">
                                Trial
                              </span>
                            )}
                          </div>
                          {ob.metadata?.custom_label && ob.metadata.custom_label !== ob.payee && (
                            <span className="text-xs text-white/40">{ob.payee}</span>
                          )}
                          {ob.metadata?.notes && (
                            <span className="text-[11px] text-white/50 italic mt-0.5 line-clamp-1">
                              &ldquo;{ob.metadata.notes}&rdquo;
                            </span>
                          )}
                        </div>
                      </td>

                      {/* Cadence */}
                      <td className="py-4 px-4 whitespace-nowrap">
                        {getCadenceBadge(ob.cadence)}
                      </td>

                      {/* Next Projected Date */}
                      <td className="py-4 px-4 whitespace-nowrap">
                        <div className="flex flex-col">
                          <span className="font-mono text-xs font-bold text-white/90">
                            {ob.next_projected_date}
                          </span>
                          <span className="text-[10px] text-white/30">
                            Last: {ob.last_date}
                          </span>
                        </div>
                      </td>

                      {/* Latest vs Baseline Amount */}
                      <td className="py-4 px-4 whitespace-nowrap text-right">
                        <div className="flex flex-col items-end">
                          <span className="font-mono text-sm font-bold text-white">
                            {formatCurrency(ob.latest_amount)}
                          </span>
                          <span className="text-[10px] font-mono text-white/40">
                            med: {formatCurrency(ob.median_amount)}
                          </span>
                        </div>
                      </td>

                      {/* Alerts & Risk Badges */}
                      <td className="py-4 px-4">
                        {hasAlerts ? (
                          <div className="flex flex-col gap-1 max-w-xs">
                            {ob.alerts.map((alert, idx) => (
                              <div
                                key={idx}
                                className={`text-[11px] px-2 py-0.5 rounded border flex items-center gap-1.5 ${
                                  alert.kind === 'price_hike'
                                    ? 'bg-red-500/10 text-red-300 border-red-500/30'
                                    : alert.kind === 'trial_expiring'
                                    ? 'bg-purple-500/10 text-purple-300 border-purple-500/30'
                                    : 'bg-amber-500/10 text-amber-300 border-amber-500/30'
                                }`}
                              >
                                <span>
                                  {alert.kind === 'price_hike'
                                    ? '📈'
                                    : alert.kind === 'trial_expiring'
                                    ? '⏳'
                                    : '⚠️'}
                                </span>
                                <span className="truncate">{alert.message}</span>
                              </div>
                            ))}
                          </div>
                        ) : priceDelta && priceDelta.is_increase ? (
                          <span className="text-xs text-red-400 font-mono">
                            +{priceDelta.percentage_delta}%
                          </span>
                        ) : (
                          <span className="text-xs text-white/20">Nominal</span>
                        )}
                      </td>

                      {/* Funding Account */}
                      <td className="py-4 px-4 whitespace-nowrap">
                        <div className="flex flex-col">
                          <span className="text-xs text-white/70 font-mono truncate max-w-[160px]" title={ob.account}>
                            {ob.account.replace(/^(Assets|Liabilities|Expenses):/, '')}
                          </span>
                          {ob.metadata?.billing_method && (
                            <span className="text-[10px] text-white/40 uppercase tracking-wider">
                              via {ob.metadata.billing_method}
                            </span>
                          )}
                        </div>
                      </td>

                      {/* Review Status */}
                      <td className="py-4 px-4 text-center whitespace-nowrap">
                        <button
                          type="button"
                          onClick={() => handleToggleReviewed(ob)}
                          title={isReviewed ? 'Mark as Unreviewed' : 'Mark as Reviewed'}
                          className={`px-2 py-1 rounded text-[10px] font-bold uppercase tracking-wider border transition-all ${
                            isReviewed
                              ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30 hover:bg-emerald-500/20'
                              : 'bg-zinc-800 text-white/40 border-white/5 hover:text-white/80'
                          }`}
                        >
                          {isReviewed ? '✓ Reviewed' : 'Pending'}
                        </button>
                      </td>

                      {/* Actions */}
                      <td className="py-4 px-4 text-right whitespace-nowrap">
                        <div className="flex items-center justify-end gap-2">
                          {ob.metadata?.cancellation_url && (
                            <a
                              href={ob.metadata.cancellation_url}
                              target="_blank"
                              rel="noreferrer"
                              className="px-2 py-1 bg-zinc-900 hover:bg-white/10 text-white/60 hover:text-white rounded text-xs border border-white/10 transition-colors"
                              title="Open Cancellation Portal"
                            >
                              🔗
                            </a>
                          )}
                          <button
                            type="button"
                            onClick={() => handleEdit(ob)}
                            className="px-3 py-1 bg-zinc-900 hover:bg-accent/10 border border-white/10 hover:border-accent/40 text-xs font-bold uppercase tracking-wider text-white hover:text-accent rounded transition-all"
                          >
                            Edit
                          </button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Metadata Edit Drawer / Modal */}
      {editingItem && (
        <div className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4">
          <div className="bg-zinc-950 border border-white/20 rounded-xl w-full max-w-xl max-h-[90vh] overflow-y-auto shadow-2xl p-6 space-y-6">
            <div className="flex items-start justify-between border-b border-white/10 pb-4">
              <div>
                <h3 className="text-lg font-black uppercase tracking-wider text-white">
                  Edit Subscription Metadata
                </h3>
                <p className="text-xs text-white/50 mt-1 font-mono">
                  {editingItem.payee} ({editingItem.account})
                </p>
              </div>
              <button
                type="button"
                onClick={handleCloseEdit}
                className="text-white/40 hover:text-white text-xl p-1"
              >
                ✕
              </button>
            </div>

            <form onSubmit={handleSaveMetadata} className="space-y-4">
              {/* Custom Display Label */}
              <div>
                <label className="block text-[10px] font-bold uppercase tracking-widest text-white/50 mb-1">
                  Custom Friendly Label
                </label>
                <input
                  type="text"
                  placeholder="e.g. Disney+ Bundle, Home Internet"
                  value={editCustomLabel}
                  onChange={(e) => setEditCustomLabel(e.target.value)}
                  className="w-full bg-zinc-900 border border-white/10 rounded px-3 py-2 text-sm text-white focus:outline-none focus:border-accent/50"
                />
              </div>

              {/* Billing Method & Payment Instrument */}
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                <div>
                  <label className="block text-[10px] font-bold uppercase tracking-widest text-white/50 mb-1">
                    Billing Method / Card
                  </label>
                  <input
                    type="text"
                    placeholder="e.g. Sapphire Preferred, ACH, Apple Pay"
                    value={editBillingMethod}
                    onChange={(e) => setEditBillingMethod(e.target.value)}
                    className="w-full bg-zinc-900 border border-white/10 rounded px-3 py-2 text-sm text-white focus:outline-none focus:border-accent/50"
                  />
                </div>

                <div>
                  <label className="block text-[10px] font-bold uppercase tracking-widest text-white/50 mb-1">
                    Alert Lead Days
                  </label>
                  <input
                    type="number"
                    min="1"
                    max="90"
                    value={editAlertLeadDays}
                    onChange={(e) => setEditAlertLeadDays(e.target.value)}
                    className="w-full bg-zinc-900 border border-white/10 rounded px-3 py-2 text-sm text-white focus:outline-none focus:border-accent/50"
                  />
                </div>
              </div>

              {/* Trial Management */}
              <div className="bg-zinc-900/60 p-4 rounded-lg border border-white/5 space-y-3">
                <div className="flex items-center gap-3">
                  <input
                    type="checkbox"
                    id="isTrialCheckbox"
                    checked={editIsTrial}
                    onChange={(e) => setEditIsTrial(e.target.checked)}
                    className="w-4 h-4 rounded bg-zinc-900 border-white/20 text-accent focus:ring-0"
                  />
                  <label htmlFor="isTrialCheckbox" className="text-xs font-bold uppercase tracking-wider text-white/80 cursor-pointer">
                    This is an introductory promo or free trial
                  </label>
                </div>

                {editIsTrial && (
                  <div>
                    <label className="block text-[10px] font-bold uppercase tracking-widest text-purple-300 mb-1">
                      Trial End Date (YYYY-MM-DD)
                    </label>
                    <input
                      type="date"
                      value={editTrialEndDate}
                      onChange={(e) => setEditTrialEndDate(e.target.value)}
                      className="w-full bg-zinc-900 border border-white/10 rounded px-3 py-2 text-sm text-white focus:outline-none focus:border-accent/50 font-mono"
                    />
                  </div>
                )}
              </div>

              {/* Cancellation URL */}
              <div>
                <label className="block text-[10px] font-bold uppercase tracking-widest text-white/50 mb-1">
                  Cancellation / Account Management URL
                </label>
                <input
                  type="url"
                  placeholder="https://..."
                  value={editCancellationUrl}
                  onChange={(e) => setEditCancellationUrl(e.target.value)}
                  className="w-full bg-zinc-900 border border-white/10 rounded px-3 py-2 text-sm text-white focus:outline-none focus:border-accent/50"
                />
              </div>

              {/* Notes */}
              <div>
                <label className="block text-[10px] font-bold uppercase tracking-widest text-white/50 mb-1">
                  Renegotiation / Family Notes
                </label>
                <textarea
                  rows="3"
                  placeholder="Notes on usage, price history, split with family..."
                  value={editNotes}
                  onChange={(e) => setEditNotes(e.target.value)}
                  className="w-full bg-zinc-900 border border-white/10 rounded px-3 py-2 text-sm text-white focus:outline-none focus:border-accent/50"
                />
              </div>

              {/* Reviewed Checkbox */}
              <div className="flex items-center gap-3 pt-2">
                <input
                  type="checkbox"
                  id="isReviewedCheckbox"
                  checked={editIsReviewed}
                  onChange={(e) => setEditIsReviewed(e.target.checked)}
                  className="w-4 h-4 rounded bg-zinc-900 border-white/20 text-emerald-400 focus:ring-0"
                />
                <label htmlFor="isReviewedCheckbox" className="text-xs font-bold uppercase tracking-wider text-white/80 cursor-pointer">
                  Mark as Reviewed & Verified
                </label>
              </div>

              {/* Status alerts */}
              {saveSuccessMsg && (
                <div className="p-3 bg-emerald-500/20 border border-emerald-500/40 text-emerald-300 text-xs rounded font-bold">
                  ✓ {saveSuccessMsg}
                </div>
              )}
              {saveErrorMsg && (
                <div className="p-3 bg-red-500/20 border border-red-500/40 text-red-300 text-xs rounded font-bold">
                  ✕ {saveErrorMsg}
                </div>
              )}

              {/* Buttons */}
              <div className="flex items-center justify-end gap-3 pt-4 border-t border-white/10">
                <button
                  type="button"
                  onClick={handleCloseEdit}
                  disabled={isSaving}
                  className="px-4 py-2 bg-zinc-900 hover:bg-white/10 border border-white/10 rounded text-xs font-bold uppercase tracking-wider text-white/60 hover:text-white"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={isSaving}
                  className="px-5 py-2 bg-accent/20 hover:bg-accent/30 border border-accent/40 text-accent font-black text-xs uppercase tracking-wider rounded transition-all shadow-[0_0_15px_rgba(0,212,255,0.2)] disabled:opacity-50"
                >
                  {isSaving ? 'Saving...' : 'Save Changes'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
