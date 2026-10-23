"use client";

import { useState, useEffect } from 'react';
import CategorySelector from './CategorySelector';
import { approveLedgerTransaction, getLedgerCategories, getPendingLedger } from '../services/apiClient';
import { formatMoney, negateMoney, compareMoney } from '../utils/money';

export default function LedgerReviewWidget() {
  const [pending, setPending] = useState([]);
  const [categories, setCategories] = useState([]);
  const [loading, setLoading] = useState(true);

  async function fetchPending() {
    setLoading(true);
    try {
      setPending(await getPendingLedger());
    } catch (e) {
      console.error("Failed to fetch pending ledger", e);
    } finally {
      setLoading(false);
    }
  }

  async function fetchCategories() {
    try {
      setCategories(await getLedgerCategories());
    } catch (e) {
      console.error("Failed to fetch ledger categories", e);
    }
  }

  useEffect(() => {
    // Load the external ledger state when the review panel mounts.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    fetchPending();
    fetchCategories();
  }, []);

  const handleApprove = async (index, lineNum, category) => {
    try {
      await approveLedgerTransaction(lineNum, category);
      setPending(prev => prev.filter((_, i) => i !== index));
    } catch (e) {
      alert("Error: " + e.message);
    }
  };

  const handleCategoryChange = (index, value) => {
    setPending(prev => {
      const copy = [...prev];
      copy[index].category = value;
      return copy;
    });
  };

  if (loading) {
    return <div className="p-8 text-center text-accent uppercase font-mono animate-pulse">Scanning Ledger for Pending Transactions...</div>;
  }

  return (
    <div className="space-y-6">
      <h3 className="text-xl font-black text-white tracking-widest uppercase italic border-b border-white/10 pb-4">
        Pending Ledger Reviews
      </h3>
      {pending.length === 0 ? (
        <div className="p-8 border border-white/5 bg-zinc-900/60 rounded text-center text-white/50 font-mono text-sm">
          No pending transactions in Sovereign.beancount
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4">
          {pending.map((tx, idx) => (
            <div key={idx} className="p-4 bg-zinc-950 border border-white/10 rounded group hover:border-accent/30 transition-all flex justify-between items-center">
              <div>
                <div className="text-[10px] font-mono text-white/40 mb-1">{tx.date}</div>
                <div className="text-sm font-bold text-white tracking-wider">{tx.payee}</div>
                <div className={`text-lg font-mono font-bold ${compareMoney(tx.amount, '0') < 0 ? 'text-green-400' : 'text-red-400'}`}>
                  {formatMoney(negateMoney(tx.amount), tx.currency)}
                </div>
              </div>
              <div className="flex items-center gap-4">
                <div className="flex flex-col gap-1">
                  <label className="text-[9px] uppercase font-bold text-white/40 tracking-widest">AI Categorization</label>
                  <CategorySelector 
                    value={tx.category} 
                    onChange={(val) => handleCategoryChange(idx, val)} 
                    categories={categories}
                  />
                </div>
                <button 
                  onClick={() => handleApprove(idx, tx.line_num, tx.category)}
                  className="px-6 py-2 bg-accent/10 border border-accent/50 text-accent font-bold uppercase text-xs tracking-widest rounded hover:bg-accent hover:text-black transition-all"
                >
                  Approve
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
