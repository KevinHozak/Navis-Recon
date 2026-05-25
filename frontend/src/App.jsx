import React, { useState, useEffect } from 'react'
import LedgerReviewWidget from './components/LedgerReviewWidget'
import OverviewWidget from './components/OverviewWidget'
import './index.css'

const API_BASE = 'http://localhost:8080';

function App() {
  const [currentView, setCurrentView] = useState('overview');
  const [pendingCount, setPendingCount] = useState(0);

  useEffect(() => {
    fetchPendingCount();
    // Refresh count every 10 seconds to keep sidebar accurate
    const interval = setInterval(fetchPendingCount, 10000);
    return () => clearInterval(interval);
  }, []);

  const fetchPendingCount = async () => {
    try {
      const res = await fetch(`${API_BASE}/api/ledger/pending`);
      if (res.ok) {
        const data = await res.json();
        setPendingCount(data.length);
      }
    } catch (e) {
      console.error(e);
    }
  };

  return (
    <div className="min-h-screen bg-black flex overflow-hidden text-white">
      {/* Sidebar Navigation */}
      <div className="w-64 border-r border-white/10 bg-zinc-950 flex flex-col">
        <div className="p-6 border-b border-white/10 flex items-center gap-3">
          <div className="w-10 h-10 bg-accent/10 border border-accent/40 rounded flex justify-center items-center text-xl shadow-[0_0_15px_rgba(0,212,255,0.2)]">
            🏛️
          </div>
          <div>
            <h1 className="text-sm font-black uppercase tracking-widest italic leading-tight">Family<br/>Ledger</h1>
          </div>
        </div>
        
        <nav className="flex-1 p-4 space-y-2">
          <button 
            onClick={() => setCurrentView('overview')}
            className={`w-full flex items-center gap-3 p-3 text-sm font-bold uppercase tracking-widest rounded transition-all ${currentView === 'overview' ? 'bg-accent/10 text-accent border border-accent/30' : 'text-white/50 hover:bg-white/5 hover:text-white'}`}
          >
            📊 Overview
          </button>
          <button 
            onClick={() => setCurrentView('reviews')}
            className={`w-full flex items-center justify-between p-3 text-sm font-bold uppercase tracking-widest rounded transition-all ${currentView === 'reviews' ? 'bg-accent/10 text-accent border border-accent/30' : 'text-white/50 hover:bg-white/5 hover:text-white'}`}
          >
            <div className="flex items-center gap-3">
              📝 Reviews
            </div>
            {pendingCount > 0 && (
              <span className="bg-red-500/20 text-red-400 border border-red-500/50 px-2 py-0.5 rounded text-[10px] animate-pulse">
                {pendingCount}
              </span>
            )}
          </button>
        </nav>
      </div>

      {/* Main Content Area */}
      <div className="flex-1 flex flex-col h-screen overflow-y-auto custom-scrollbar">
        <div className="p-12 max-w-5xl mx-auto w-full">
          {currentView === 'overview' && <OverviewWidget onNavigateToReviews={() => setCurrentView('reviews')} />}
          {currentView === 'reviews' && (
            <div className="bg-zinc-900/50 p-8 rounded-lg border border-white/5 shadow-2xl">
              <LedgerReviewWidget />
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

export default App
