import { Fragment, useState, useEffect, useMemo, useRef } from 'react';
import { getLedgerTransactions } from '../services/apiClient';

const MONTH_NAMES = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

export default function TransactionDrillDown({ prefix, isLiability }) {
  const [transactions, setTransactions] = useState([]);
  const [loading, setLoading] = useState(true);
  const [path, setPath] = useState([]); // e.g. [] -> ['2026'] -> ['2026', 'Q2'] -> ['2026', 'Q2', 'May']
  const [error, setError] = useState(null);
  const requestId = useRef(0);

  async function fetchTransactions(requestedPrefix) {
    const id = ++requestId.current;
    try {
      setLoading(true);
      setError(null);
      const result = await getLedgerTransactions(requestedPrefix);
      if (id === requestId.current) setTransactions(result);
    } catch (e) {
      console.error(e);
      if (id === requestId.current) setError(e?.message || String(e));
    } finally {
      if (id === requestId.current) setLoading(false);
    }
  }

  useEffect(() => {
    // Refresh the external transaction stream when the account prefix changes.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    fetchTransactions(prefix);
    setPath([]);
    // The loader is intentionally scoped to the current prefix.
  }, [prefix]);

  // Grouping Logic
  const getQuarter = (monthIndex) => `Q${Math.floor(monthIndex / 3) + 1}`;

  const currentData = useMemo(() => {
    if (!transactions.length) return [];

    let filtered = transactions;
    
    // Apply year filter
    if (path.length > 0) {
      filtered = filtered.filter(t => new Date(t.date).getFullYear().toString() === path[0]);
    }
    // Apply quarter filter
    if (path.length > 1) {
      filtered = filtered.filter(t => getQuarter(new Date(t.date).getMonth()) === path[1]);
    }
    // Apply month filter
    if (path.length > 2) {
      filtered = filtered.filter(t => MONTH_NAMES[new Date(t.date).getMonth()] === path[2]);
    }

    if (path.length === 3) {
      // Leaf level: return actual transactions
      return filtered;
    }

    // Grouping level
    const groups = {};
    filtered.forEach(t => {
      const dateObj = new Date(t.date);
      let groupKey = '';
      if (path.length === 0) {
        groupKey = dateObj.getFullYear().toString();
      } else if (path.length === 1) {
        groupKey = getQuarter(dateObj.getMonth());
      } else if (path.length === 2) {
        groupKey = MONTH_NAMES[dateObj.getMonth()];
      }

      if (!groups[groupKey]) groups[groupKey] = { name: groupKey, sum: 0, count: 0 };
      groups[groupKey].sum += t.amount;
      groups[groupKey].count += 1;
    });

    return Object.values(groups).sort((a, b) => b.name.localeCompare(a.name));
  }, [transactions, path]);

  if (loading) {
    return <div className="text-white/50 text-center font-mono animate-pulse">Loading transaction graph...</div>;
  }

  if (error) return <div className="text-red-400 text-center font-mono">Transactions unavailable: {error}</div>;
  if (transactions.length === 0) {
    return <div className="text-white/30 text-center font-mono">No transactions found for this category.</div>;
  }

  const handleLevelClick = (name) => {
    setPath([...path, name]);
  };

  const handleBreadcrumbClick = (index) => {
    setPath(path.slice(0, index));
  };

  const accentColor = isLiability ? "text-red-400" : "text-accent";
  const hoverBg = isLiability ? "hover:bg-red-500/10" : "hover:bg-accent/10";
  return (
    <div className={`mt-8 border border-white/5 bg-zinc-900/50 rounded-lg p-6 shadow-inner`}>
      {/* Breadcrumb Header */}
      <div className="flex items-center gap-2 mb-6 font-mono text-sm uppercase tracking-widest text-white/50">
        <button 
          onClick={() => handleBreadcrumbClick(0)}
          className={`hover:text-white transition-colors ${path.length === 0 ? 'text-white font-bold' : ''}`}
        >
          All Time
        </button>
        {path.map((segment, idx) => (
          <Fragment key={idx}>
            <span>/</span>
            <button 
              onClick={() => handleBreadcrumbClick(idx + 1)}
              className={`hover:text-white transition-colors ${idx === path.length - 1 ? 'text-white font-bold' : ''}`}
            >
              {segment}
            </button>
          </Fragment>
        ))}
      </div>

      {/* Content */}
      {path.length < 3 ? (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          {currentData.map(group => (
            <button
              key={group.name}
              onClick={() => handleLevelClick(group.name)}
              className={`p-6 border border-white/5 rounded bg-zinc-950 ${hoverBg} transition-all flex flex-col items-center gap-2`}
            >
              <h3 className="text-2xl font-black text-white">{group.name}</h3>
              <p className={`font-mono font-bold ${accentColor}`}>
                ${Math.abs(group.sum).toLocaleString('en-US', { minimumFractionDigits: 2 })}
              </p>
              <p className="text-[10px] text-white/30 uppercase tracking-widest mt-2">{group.count} Txns</p>
            </button>
          ))}
        </div>
      ) : (
        <div className="space-y-2 max-h-[500px] overflow-y-auto custom-scrollbar pr-2">
          {currentData.map((txn, idx) => (
            <div key={idx} className="flex flex-col md:flex-row justify-between items-start md:items-center p-4 bg-zinc-950 border border-white/5 rounded hover:border-white/20 transition-colors">
              <div className="flex flex-col">
                <span className="text-xs text-white/40 font-mono mb-1">{txn.date} &middot; {txn.account.split(':').pop()}</span>
                <span className="text-sm font-bold text-white/90">{txn.payee || 'Unknown'} {txn.narration && `- ${txn.narration}`}</span>
              </div>
              <div className={`text-lg font-bold ${txn.amount > 0 ? 'text-green-400' : 'text-white/80'} mt-2 md:mt-0`}>
                ${Math.abs(txn.amount).toLocaleString('en-US', { minimumFractionDigits: 2 })}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
