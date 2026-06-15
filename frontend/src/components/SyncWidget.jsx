import React, { useState, useEffect, useRef } from 'react';

const API_BASE = 'http://localhost:8080';

const SyncWidget = () => {
  const [logs, setLogs] = useState([]);
  const [isSyncing, setIsSyncing] = useState(false);
  const [isComplete, setIsComplete] = useState(false);
  const scrollRef = useRef(null);

  // Auto-scroll to the bottom as new logs arrive
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs]);

  const handleStartSync = () => {
    setLogs([]);
    setIsSyncing(true);
    setIsComplete(false);

    const eventSource = new EventSource(`${API_BASE}/api/ledger/sync/stream`);

    eventSource.onmessage = (event) => {
      const data = event.data;
      if (data === '[DONE]') {
        eventSource.close();
        setIsSyncing(false);
        setIsComplete(true);
      } else {
        setLogs((prev) => [...prev, data]);
      }
    };

    eventSource.onerror = (err) => {
      console.error('EventSource failed:', err);
      eventSource.close();
      setIsSyncing(false);
      setLogs((prev) => [...prev, 'Error: Connection lost or failed to start sync.']);
    };
  };

  return (
    <div className="w-full h-full flex flex-col gap-6">
      <div className="flex justify-between items-center bg-zinc-900/50 p-6 rounded-lg border border-white/5 shadow-2xl">
        <div>
          <h2 className="text-2xl font-black uppercase tracking-widest text-white/90">
            Vault Sync
          </h2>
          <p className="text-white/40 text-sm mt-1">
            Import, rename, and parse recent financial statements.
          </p>
        </div>
        <button
          onClick={handleStartSync}
          disabled={isSyncing}
          className={`px-8 py-3 rounded font-bold uppercase tracking-wider transition-all shadow-xl ${
            isSyncing
              ? 'bg-zinc-800 text-white/30 cursor-not-allowed border border-white/5'
              : 'bg-cyan-500/20 text-cyan-400 border border-cyan-500/40 hover:bg-cyan-400 hover:text-zinc-950 hover:shadow-[0_0_20px_rgba(0,212,255,0.4)]'
          }`}
        >
          {isSyncing ? 'Syncing...' : 'Start Sync'}
        </button>
      </div>

      <div className="flex-1 bg-black rounded-lg border border-white/10 shadow-2xl overflow-hidden flex flex-col font-mono text-sm relative">
        <div className="bg-zinc-900 px-4 py-2 border-b border-white/10 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <div className="w-3 h-3 rounded-full bg-red-500/50 border border-red-500"></div>
            <div className="w-3 h-3 rounded-full bg-yellow-500/50 border border-yellow-500"></div>
            <div className="w-3 h-3 rounded-full bg-green-500/50 border border-green-500"></div>
          </div>
          <div className="text-white/30 text-xs tracking-widest uppercase">Terminal</div>
          <div className="w-16"></div> {/* Spacer to center the title */}
        </div>
        
        <div 
          ref={scrollRef}
          className="flex-1 p-6 overflow-y-auto custom-scrollbar min-h-[300px]"
        >
          {logs.length === 0 && !isSyncing && !isComplete && (
            <div className="h-full flex items-center justify-center text-white/20 italic">
              Awaiting execution protocol...
            </div>
          )}
          
          <div className="space-y-1 pb-4">
            {logs.map((log, index) => {
              let colorClass = 'text-white/70';
              if (log.includes('Processing:')) colorClass = 'text-accent font-bold';
              else if (log.includes('Moved to:')) colorClass = 'text-green-400';
              else if (log.includes('Error')) colorClass = 'text-red-400';
              else if (log.includes('Appended Beancount')) colorClass = 'text-blue-300';
              else if (log.includes('Successfully processed')) colorClass = 'text-green-400 font-bold';

              return (
                <div key={index} className={`${colorClass} break-all`}>
                  <span className="text-white/20 mr-2">{`>`}</span>
                  {log}
                </div>
              );
            })}
            
            {isSyncing && (
              <div className="text-white/50 animate-pulse mt-2">
                <span className="text-accent/50 mr-2">{`>`}</span>
                _
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default SyncWidget;
