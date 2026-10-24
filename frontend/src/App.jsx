import { useState, useEffect } from 'react'
import LedgerReviewWidget from './components/LedgerReviewWidget'
import OverviewWidget from './components/OverviewWidget'
import CategoryDetailWidget from './components/CategoryDetailWidget'
import VaultChecklist from './components/VaultChecklist'
import StatementLibraryWidget from './components/StatementLibraryWidget'
import SubscriptionsWidget from './components/SubscriptionsWidget'
import SyncWidget from './components/SyncWidget'
import WealthDashboardWidget from './components/WealthDashboardWidget'
import RecordsWidget from './components/RecordsWidget'
import { ping, getPendingLedger, getRecurringObligations, getRecordsRadar, subscribeLedgerMutation, subscribeSyncResourceChanges } from './services/apiClient'
import { createLatestRequestLoader } from './services/latestRequestLoader'
import { createSyncResourceObserver } from './services/syncResourceObserver'
import './index.css'

function App() {
  const [currentView, setCurrentView] = useState('overview');
  const [pendingCount, setPendingCount] = useState(null);
  const [ledgerGeneration, setLedgerGeneration] = useState(0);
  const [statementGeneration, setStatementGeneration] = useState(0);
  const [alertObligationsCount, setAlertObligationsCount] = useState(null);
  const [recordsActionCount, setRecordsActionCount] = useState(0);
  const [desktopStatus, setDesktopStatus] = useState('checking');
  const [statementFilter, setStatementFilter] = useState(null);

  const navigateToStatements = (filters = {}) => {
    setStatementFilter(filters);
    setCurrentView('statements');
  };

  const navigateToChecklist = () => {
    setCurrentView('checklist');
  };

  useEffect(() => {
    let isMounted = true;
    const pendingLoader = createLatestRequestLoader({ load: getPendingLedger, update: (value) => { if (isMounted) setPendingCount(value) }, map: (rows) => Array.isArray(rows) ? rows.length : null });
    const obligationLoader = createLatestRequestLoader({ load: getRecurringObligations, update: (value) => { if (isMounted) setAlertObligationsCount(value) }, map: (rows) => Array.isArray(rows) ? rows.reduce((acc, curr) => acc + (curr.alerts ? curr.alerts.length : 0), 0) : null });
    const recordsLoader = createLatestRequestLoader({ load: getRecordsRadar, update: (value) => { if (isMounted) setRecordsActionCount(value) }, map: (report) => report?.summary ? (report.summary.actionRequired || 0) : null });

    ping()
      .then(() => {
        if (isMounted) setDesktopStatus('online');
      })
      .catch(() => {
        if (isMounted) setDesktopStatus('offline');
      });

    const updatePendingCount = () => pendingLoader.refresh();
    const updateObligationAlerts = () => obligationLoader.refresh();
    const updateRecordsRadarCount = () => recordsLoader.refresh();

    updatePendingCount();
    updateObligationAlerts();
    updateRecordsRadarCount();
    const unsubscribeLedgerMutation = subscribeLedgerMutation(() => { setLedgerGeneration((value) => value + 1); updatePendingCount(); updateObligationAlerts(); updateRecordsRadarCount(); });
    const observer = createSyncResourceObserver({ subscribe: subscribeSyncResourceChanges, snapshot: async () => {
      const { getSyncRun } = await import('./services/apiClient'); return getSyncRun()
    }, onEffects: () => {
      if (!isMounted) return;
      setLedgerGeneration(value => value + 1); setStatementGeneration(value => value + 1);
      updatePendingCount(); updateObligationAlerts(); updateRecordsRadarCount();
    }, onError: error => console.error('Unable to observe completed imports:', error) });
    observer.connect();
    // Refresh count every 10 seconds to keep sidebar accurate
    const interval = setInterval(() => {
      updatePendingCount();
      updateObligationAlerts();
      updateRecordsRadarCount();
    }, 10000);
    return () => {
      isMounted = false;
      observer.dispose();
      pendingLoader.dispose();
      obligationLoader.dispose();
      recordsLoader.dispose();
      unsubscribeLedgerMutation();
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
              onClick={() => setCurrentView('records')}
              className={`w-full flex items-center justify-between p-3 text-sm font-bold uppercase tracking-widest rounded transition-all ${currentView === 'records' ? 'bg-accent/10 text-accent border border-accent/30' : 'text-white/50 hover:bg-white/5 hover:text-white border border-transparent'}`}
            >
              <div className="flex items-center gap-3">
                <span className={currentView === 'records' ? 'text-accent' : 'opacity-70'}>📁</span> Records
              </div>
              {recordsActionCount === null ? <span className="text-[10px] text-white/40">Unavailable</span> : recordsActionCount > 0 && (
                <span className="bg-red-500/20 text-red-400 border border-red-500/50 px-2 py-0.5 rounded text-[10px] animate-pulse">
                  {recordsActionCount}
                </span>
              )}
            </button>

            <button 
              type="button"
              onClick={() => setCurrentView('subscriptions')}
              className={`w-full flex items-center justify-between p-3 text-sm font-bold uppercase tracking-widest rounded transition-all ${currentView === 'subscriptions' ? 'bg-accent/10 text-accent border border-accent/30' : 'text-white/50 hover:bg-white/5 hover:text-white border border-transparent'}`}
            >
              <div className="flex items-center gap-3">
                <span className={currentView === 'subscriptions' ? 'text-accent' : 'opacity-70'}>💳</span> Subscriptions
              </div>
              {alertObligationsCount === null ? <span className="text-[10px] text-white/40">Unavailable</span> : alertObligationsCount > 0 && (
                <span className="bg-amber-500/20 text-amber-300 border border-amber-500/50 px-2 py-0.5 rounded text-[10px]">
                  {alertObligationsCount}
                </span>
              )}
            </button>

            <button 
              type="button"
              onClick={() => setCurrentView('reviews')}
              className={`w-full flex items-center justify-between p-3 text-sm font-bold uppercase tracking-widest rounded transition-all ${currentView === 'reviews' ? 'bg-accent/10 text-accent border border-accent/30' : 'text-white/50 hover:bg-white/5 hover:text-white border border-transparent'}`}
            >
              <div className="flex items-center gap-3">
                <span className={currentView === 'reviews' ? 'text-accent' : 'opacity-70'}>📝</span> Reviews
              </div>
              {pendingCount === null ? <span className="text-[10px] text-white/40">Unavailable</span> : pendingCount > 0 && (
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
          {currentView === 'overview' && <OverviewWidget key={ledgerGeneration} onNavigateToReviews={() => setCurrentView('reviews')} />}
          
          {currentView === 'wealth' && (
            <div className="h-full">
              <WealthDashboardWidget key={ledgerGeneration} />
            </div>
          )}

          {currentView === 'reviews' && (
            <div className="bg-zinc-900/50 p-8 rounded-lg border border-white/5 shadow-2xl">
              <LedgerReviewWidget />
            </div>
          )}

          {currentView === 'records' && (
            <div className="h-full">
              <RecordsWidget />
            </div>
          )}

          {currentView === 'subscriptions' && (
            <div className="h-full">
              <SubscriptionsWidget key={ledgerGeneration} />
            </div>
          )}

          {currentView === 'statements' && (
            <div className="h-full">
              <StatementLibraryWidget key={statementGeneration}
                initialFilter={statementFilter} 
                onNavigateToChecklist={navigateToChecklist} 
              />
            </div>
          )}

          {currentView === 'checklist' && (
            <div className="bg-zinc-900/50 rounded-lg border border-white/5 shadow-2xl h-full">
              <VaultChecklist key={statementGeneration}
                onNavigateToStatements={navigateToStatements} 
              />
            </div>
          )}

          {currentView === 'sync' && (
            <div className="h-full">
              <SyncWidget />
            </div>
          )}

          {currentView === 'cat_bank_accounts' && <CategoryDetailWidget key={ledgerGeneration} categoryKey="bank_accounts" title="Bank Accounts" icon="🏦" />}
          {currentView === 'cat_retirement' && <CategoryDetailWidget key={ledgerGeneration} categoryKey="retirement" title="Retirement" icon="📈" />}
          {currentView === 'cat_investments' && <CategoryDetailWidget key={ledgerGeneration} categoryKey="investments" title="Investments" icon="💹" />}
          {currentView === 'cat_credit_cards' && <CategoryDetailWidget key={ledgerGeneration} categoryKey="credit_cards" title="Credit Cards" icon="💳" isLiability={true} />}

        </div>
      </div>
    </div>
  )
}

export default App
