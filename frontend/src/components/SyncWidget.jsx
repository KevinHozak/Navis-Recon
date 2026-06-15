import React, { useState, useEffect, useRef } from 'react';

const API_BASE = 'http://localhost:8080';

const SyncWidget = () => {
  const [phase, setPhase] = useState('idle'); // idle, analyzing, review, committing, complete
  const [logs, setLogs] = useState([]);
  const [matches, setMatches] = useState([]);
  const [unmatched, setUnmatched] = useState([]);
  const [errors, setErrors] = useState([]);
  const [selectedMatches, setSelectedMatches] = useState(new Set());
  
  const scrollRef = useRef(null);

  useEffect(() => {
    if (scrollRef.current && phase === 'analyzing') {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs, phase]);

  const handleStartAnalyze = () => {
    setPhase('analyzing');
    setLogs([]);
    setMatches([]);
    setUnmatched([]);
    setErrors([]);
    setSelectedMatches(new Set());

    const eventSource = new EventSource(`${API_BASE}/api/ledger/sync/stream`);

    eventSource.onmessage = (event) => {
      const dataStr = event.data;
      if (dataStr === '[DONE]') {
        eventSource.close();
        setPhase('review');
      } else {
        try {
          const data = JSON.parse(dataStr);
          if (data.type === 'log') {
            setLogs((prev) => [...prev, data.message]);
          } else if (data.type === 'match') {
            setMatches((prev) => {
              const newMatches = [...prev, data];
              setSelectedMatches(new Set(newMatches.map((_, i) => i))); // select all by default
              return newMatches;
            });
            setLogs((prev) => [...prev, `Matched: ${data.filename} -> ${data.new_name}`]);
          } else if (data.type === 'unmatched') {
            setUnmatched((prev) => [...prev, data]);
            setLogs((prev) => [...prev, `Unmatched: ${data.filename} (${data.message})`]);
          } else if (data.type === 'error') {
            setErrors((prev) => [...prev, data]);
            setLogs((prev) => [...prev, `Error: ${data.filename || ''} - ${data.message}`]);
          }
        } catch (e) {
          // Fallback if not JSON
          setLogs((prev) => [...prev, dataStr]);
        }
      }
    };

    eventSource.onerror = (err) => {
      console.error('EventSource failed:', err);
      eventSource.close();
      setPhase('review');
      setLogs((prev) => [...prev, 'Error: Connection lost or failed to start sync.']);
    };
  };

  const handleCommit = async () => {
    setPhase('committing');
    const toCommit = matches.filter((_, i) => selectedMatches.has(i));
    
    try {
      const response = await fetch(`${API_BASE}/api/ledger/sync/commit`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(toCommit),
      });
      const result = await response.json();
      if (result.status === 'success') {
        setPhase('complete');
      } else {
        alert('Commit failed: ' + result.stderr);
        setPhase('review');
      }
    } catch (e) {
      alert('Network error during commit.');
      setPhase('review');
    }
  };

  const toggleMatch = (index) => {
    const newSelected = new Set(selectedMatches);
    if (newSelected.has(index)) {
      newSelected.delete(index);
    } else {
      newSelected.add(index);
    }
    setSelectedMatches(newSelected);
  };

  return (
    <div className="w-full h-full flex flex-col gap-6">
      <div className="flex justify-between items-center bg-zinc-900/50 p-6 rounded-lg border border-white/5 shadow-2xl">
        <div>
          <h2 className="text-2xl font-black uppercase tracking-widest text-white/90">
            Vault Sync
          </h2>
          <p className="text-white/40 text-sm mt-1">
            Analyze, review, and commit financial statements.
          </p>
        </div>
        {phase === 'idle' && (
          <button
            onClick={handleStartAnalyze}
            className="px-8 py-3 rounded font-bold uppercase tracking-wider transition-all shadow-xl bg-cyan-500/20 text-cyan-400 border border-cyan-500/40 hover:bg-cyan-400 hover:text-zinc-950 hover:shadow-[0_0_20px_rgba(0,212,255,0.4)]"
          >
            Start Analysis
          </button>
        )}
        {phase === 'analyzing' && (
          <div className="px-8 py-3 rounded font-bold uppercase tracking-wider bg-zinc-800 text-white/30 border border-white/5 flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-accent animate-ping"></span>
            Analyzing...
          </div>
        )}
        {phase === 'review' && (
          <button
            onClick={handleCommit}
            disabled={selectedMatches.size === 0}
            className="px-8 py-3 rounded font-bold uppercase tracking-wider transition-all shadow-xl bg-green-500/20 text-green-400 border border-green-500/40 hover:bg-green-400 hover:text-zinc-950 hover:shadow-[0_0_20px_rgba(34,197,94,0.4)] disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Commit {selectedMatches.size} Items
          </button>
        )}
        {(phase === 'committing' || phase === 'complete') && (
          <button
            onClick={() => setPhase('idle')}
            className="px-8 py-3 rounded font-bold uppercase tracking-wider transition-all shadow-xl bg-zinc-800 text-white hover:bg-zinc-700"
          >
            {phase === 'committing' ? 'Committing...' : 'Start New Sync'}
          </button>
        )}
      </div>

      <div className="flex-1 bg-zinc-950 rounded-lg border border-white/10 shadow-2xl overflow-hidden flex flex-col relative">
        {(phase === 'idle' || phase === 'analyzing') && (
          <div className="flex-1 flex flex-col font-mono text-sm">
             <div className="bg-zinc-900 px-4 py-2 border-b border-white/10 flex items-center justify-between">
              <div className="text-white/30 text-xs tracking-widest uppercase">Analyzer Terminal</div>
            </div>
            <div ref={scrollRef} className="flex-1 p-6 overflow-y-auto custom-scrollbar">
              {logs.length === 0 ? (
                <div className="h-full flex items-center justify-center text-white/20 italic">Awaiting execution protocol...</div>
              ) : (
                <div className="space-y-1 pb-4">
                  {logs.map((log, index) => (
                    <div key={index} className="text-white/70 break-all">
                      <span className="text-white/20 mr-2">{`>`}</span>{log}
                    </div>
                  ))}
                  {phase === 'analyzing' && <div className="text-white/50 animate-pulse mt-2"><span className="text-accent/50 mr-2">{`>`}</span>_</div>}
                </div>
              )}
            </div>
          </div>
        )}

        {(phase === 'review' || phase === 'committing' || phase === 'complete') && (
          <div className="flex-1 overflow-y-auto custom-scrollbar p-6 space-y-8">
            {phase === 'complete' && (
              <div className="bg-green-500/10 border border-green-500/30 text-green-400 p-4 rounded text-center font-bold tracking-widest uppercase">
                Sync Completed Successfully
              </div>
            )}
            
            <div>
              <h3 className="text-lg font-black uppercase tracking-widest text-accent mb-4 border-b border-white/10 pb-2">Ready to Commit ({matches.length})</h3>
              {matches.length === 0 ? (
                <div className="text-white/40 italic">No matches found.</div>
              ) : (
                <div className="space-y-2">
                  {matches.map((m, i) => (
                    <div key={i} className={`flex items-center justify-between p-4 rounded border transition-colors ${selectedMatches.has(i) ? 'bg-zinc-900 border-accent/30' : 'bg-black border-white/5 opacity-50'}`}>
                      <div className="flex items-center gap-4">
                        <input type="checkbox" checked={selectedMatches.has(i)} onChange={() => toggleMatch(i)} className="w-5 h-5 accent-accent" disabled={phase !== 'review'} />
                        <div>
                          <div className="font-bold text-white/90">{m.rule} <span className="text-white/30 text-sm font-normal">({m.date})</span></div>
                          <div className="text-xs text-white/50 font-mono mt-1">{m.filename} -&gt; {m.new_name}</div>
                        </div>
                      </div>
                      <div className="text-right">
                        <div className="font-mono text-accent">{m.amount.toFixed(2)} USD</div>
                        <div className="text-xs text-white/50 mt-1">{m.category} {m.is_cleared ? '(Cleared)' : '(Pending)'}</div>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>

            {(unmatched.length > 0 || errors.length > 0) && (
              <div>
                <h3 className="text-lg font-black uppercase tracking-widest text-red-400 mb-4 border-b border-white/10 pb-2">Requires Manual Attention</h3>
                <div className="space-y-2">
                  {unmatched.map((u, i) => (
                    <div key={`u-${i}`} className="p-4 rounded border bg-red-950/20 border-red-900/30">
                      <div className="font-bold text-red-400">{u.filename}</div>
                      <div className="text-sm text-red-400/70 mt-1">{u.message}</div>
                    </div>
                  ))}
                  {errors.map((e, i) => (
                    <div key={`e-${i}`} className="p-4 rounded border bg-red-950/20 border-red-900/30">
                      <div className="font-bold text-red-400">{e.filename || 'System Error'}</div>
                      <div className="text-sm text-red-400/70 mt-1">{e.message}</div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};

export default SyncWidget;
