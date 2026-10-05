import { useState, useEffect } from 'react'
import LedgerReviewWidget from './components/LedgerReviewWidget'
import OverviewWidget from './components/OverviewWidget'
import CategoryDetailWidget from './components/CategoryDetailWidget'
import VaultChecklist from './components/VaultChecklist'
import StatementLibraryWidget from './components/StatementLibraryWidget'
import SyncWidget from './components/SyncWidget'
import WealthDashboardWidget from './components/WealthDashboardWidget'
import { ping, request } from './services/apiClient'
import './index.css'

function App() {
  const [currentView, setCurrentView] = useState('overview');
  const [pendingCount, setPendingCount] = useState(0);
  const [desktopStatus, setDesktopStatus] = useState('checking');

  useEffect(() => {
    let isMounted = true;

    ping()
      .then(() => {
        if (isMounted) setDesktopStatus('online');
      })
      .catch(() => {
        if (isMounted) setDesktopStatus('offline');
      });

    const updatePendingCount = async () => {
      try {
        const data = await request('/api/ledger/pending');
        if (isMounted) {
          setPendingCount(data.length);
        }
      } catch (e) {
        console.error(e);
      }
    };

    updatePendingCount();
    // Refresh count every 10 seconds to keep sidebar accurate
    const interval = setInterval(updatePendingCount, 10000);
    return () => {
      isMounted = false;
      clearInterval(interval);
    };
  }, []);

  const navButton = (viewId, icon, label) => {
    const isActive = currentView === viewId;
    return (
      <button 
        type="button"
        onClick={() => setCurrentView(viewId)}
        className={`w-full flex items-center gap-3 p-3 text-sm font-bold uppercase tracking-widest rounded transition-all ${isActive ? 'bg-accent/10 text-accent border border-accent/30' : 'text-white/50 hover:bg-white/5 hover:text-white border border-transparent'}`}
      >
        <span className={isActive ? 'text-accent' : 'opacity-70'}>{icon}</span> {label}
      </button>
    );
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
            <h1 className="text-sm font-black uppercase tracking-widest italic leading-tight">Navis<br/>Recon</h1>
            <span className="text-[9px] uppercase tracking-widest text-white/30">IPC {desktopStatus}</span>
          </div>
        </div>
        
        <nav className="flex-1 p-4 space-y-6 overflow-y-auto custom-scrollbar">
          <div className="space-y-2">
            <h3 className="text-[10px] font-black text-white/30 uppercase tracking-widest px-3 mb-2">Main</h3>
            {navButton('overview', '📊', 'Overview')}
            {navButton('wealth', '🏦', 'The Vault')}
            {navButton('sync', '🔄', 'Sync & Import')}
            {navButton('statements', '📂', 'Statements')}
            {navButton('checklist', '⚖️', 'Checklist')}
            
            <button 
              type="button"
              onClick={() => setCurrentView('reviews')}
              className={`w-full flex items-center justify-between p-3 text-sm font-bold uppercase tracking-widest rounded transition-all ${currentView === 'reviews' ? 'bg-accent/10 text-accent border border-accent/30' : 'text-white/50 hover:bg-white/5 hover:text-white border border-transparent'}`}
            >
              <div className="flex items-center gap-3">
                <span className={currentView === 'reviews' ? 'text-accent' : 'opacity-70'}>📝</span> Reviews
              </div>
              {pendingCount > 0 && (
                <span className="bg-red-500/20 text-red-400 border border-red-500/50 px-2 py-0.5 rounded text-[10px] animate-pulse">
                  {pendingCount}
                </span>
              )}
            </button>
          </div>

          <div className="space-y-2">
            <h3 className="text-[10px] font-black text-white/30 uppercase tracking-widest px-3 mb-2">Accounts</h3>
            {navButton('cat_bank_accounts', '🏦', 'Bank Accounts')}
            {navButton('cat_retirement', '📈', 'Retirement')}
            {navButton('cat_investments', '💹', 'Investments')}
            {navButton('cat_credit_cards', '💳', 'Credit Cards')}
          </div>
        </nav>
      </div>

      {/* Main Content Area */}
      <div className="flex-1 flex flex-col h-screen overflow-y-auto custom-scrollbar">
        <div className="p-12 mx-auto w-full xl:max-w-7xl 2xl:max-w-full">
          {currentView === 'overview' && <OverviewWidget onNavigateToReviews={() => setCurrentView('reviews')} />}
          
          {currentView === 'wealth' && (
            <div className="h-full">
              <WealthDashboardWidget />
            </div>
          )}

          {currentView === 'reviews' && (
            <div className="bg-zinc-900/50 p-8 rounded-lg border border-white/5 shadow-2xl">
              <LedgerReviewWidget />
            </div>
          )}

          {currentView === 'statements' && (
            <div className="h-full">
              <StatementLibraryWidget />
            </div>
          )}

          {currentView === 'checklist' && (
            <div className="bg-zinc-900/50 rounded-lg border border-white/5 shadow-2xl h-full">
              <VaultChecklist />
            </div>
          )}

          {currentView === 'sync' && (
            <div className="h-full">
              <SyncWidget />
            </div>
          )}

          {currentView === 'cat_bank_accounts' && <CategoryDetailWidget categoryKey="bank_accounts" title="Bank Accounts" icon="🏦" />}
          {currentView === 'cat_retirement' && <CategoryDetailWidget categoryKey="retirement" title="Retirement" icon="📈" />}
          {currentView === 'cat_investments' && <CategoryDetailWidget categoryKey="investments" title="Investments" icon="💹" />}
          {currentView === 'cat_credit_cards' && <CategoryDetailWidget categoryKey="credit_cards" title="Credit Cards" icon="💳" isLiability={true} />}

        </div>
      </div>
    </div>
  )
}

export default App
