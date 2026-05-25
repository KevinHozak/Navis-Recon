import React, { useState, useEffect } from 'react';

const API_BASE = 'http://localhost:8080';

export default function OverviewWidget({ onNavigateToReviews }) {
  const [summary, setSummary] = useState(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetchSummary();
  }, []);

  const fetchSummary = async () => {
    try {
      const res = await fetch(`${API_BASE}/api/ledger/summary`);
      if (res.ok) {
        const data = await res.json();
        setSummary(data);
      }
    } catch (e) {
      console.error("Failed to fetch summary", e);
    } finally {
      setLoading(false);
    }
  };

  if (loading) {
    return <div className="p-8 text-center text-accent uppercase font-mono animate-pulse">Calculating Net Worth...</div>;
  }

  if (!summary) {
    return <div className="text-red-400">Error loading summary.</div>;
  }

  // Helper to render a card section
  const renderCard = (title, icon, data, isLiability = false) => {
    if (!data || Object.keys(data).length === 0) return null;
    
    // Sum balances for the header
    const total = Object.values(data).reduce((acc, val) => acc + val, 0);
    
    const borderColor = isLiability ? "border-red-500/20" : "border-accent/20";
    const hoverBorder = isLiability ? "hover:border-red-500/50" : "hover:border-accent/50";
    const iconColor = isLiability ? "text-red-500" : "text-accent";

    return (
      <div className={`p-6 bg-zinc-950 border ${borderColor} rounded shadow-lg ${hoverBorder} transition-colors`}>
        <div className="flex items-center gap-2 mb-4">
          <span className={`${iconColor} text-xl`}>{icon}</span>
          <h3 className="text-xl font-bold text-white uppercase tracking-wider">{title}</h3>
        </div>
        <p className="text-3xl font-bold text-white/90 mb-6">${Math.abs(total).toLocaleString('en-US', { minimumFractionDigits: 2 })}</p>
        
        <div className="space-y-2">
          {Object.entries(data).map(([account, amount]) => (
            <div key={account} className="flex justify-between text-xs font-mono border-b border-white/5 pb-1">
              <span className="text-white/50">{account}</span>
              <span className="text-white/80">${Math.abs(amount).toLocaleString('en-US', { minimumFractionDigits: 2 })}</span>
            </div>
          ))}
        </div>
      </div>
    );
  };

  return (
    <div className="space-y-8 animate-in fade-in duration-500">
      {/* Top Level Metric */}
      <div className="p-6 bg-zinc-900 border border-white/10 rounded-lg shadow-2xl relative overflow-hidden group">
        <div className="absolute inset-0 bg-accent/5 opacity-0 group-hover:opacity-100 transition-opacity" />
        <p className="text-xs font-mono text-white/50 uppercase tracking-widest mb-1">Total Net Worth</p>
        <h2 className="text-5xl font-black text-white tracking-tighter">
          ${summary.net_worth.toLocaleString('en-US', { minimumFractionDigits: 2 })}
        </h2>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-2 xl:grid-cols-3 gap-6">
        {renderCard("Bank Accounts", "🏦", summary.bank_accounts)}
        {renderCard("Retirement", "📈", summary.retirement)}
        {renderCard("Investments", "💹", summary.investments)}
        {renderCard("Credit Cards", "💳", summary.credit_cards, true)}
        
        {/* Render Housing & Cash manually since they are flat values */}
        {renderCard("Property & Cash", "🏠", {
            "Housing": summary.housing,
            "Cash": summary.cash
        })}
      </div>
      
      <button 
        onClick={onNavigateToReviews}
        className="w-full mt-4 p-4 bg-accent/10 border border-accent/40 text-accent font-bold uppercase tracking-widest rounded hover:bg-accent hover:text-black transition-all flex items-center justify-center gap-2"
      >
        <span>Process Pending Reviews</span>
        <span>→</span>
      </button>
    </div>
  );
}
