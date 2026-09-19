import { useState, useEffect } from 'react';
import { motion } from 'framer-motion';
import { getVaultChecklist, saveVaultChecklist } from '../services/apiClient';

export default function VaultChecklist() {
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  async function fetchChecklist() {
    try {
      const json = await getVaultChecklist();
      if (!json.error) {
        setData(json);
      }
    } catch (err) {
      console.error("DEBUG: Failed to fetch checklist:", err);
    } finally {
      setLoading(false);
    }
  }

  async function saveChecklist(updatedData) {
    setSaving(true);
    try {
      await saveVaultChecklist(updatedData);
    } catch (err) {
      console.error("DEBUG: Failed to save checklist:", err);
    } finally {
      setSaving(false);
    }
  }

  useEffect(() => {
    // Loading external checklist state is the purpose of this effect.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    fetchChecklist();
  }, []);

  const handleStatementToggle = (instIndex, monthIndex) => {
    const newData = structuredClone(data);
    newData.statements[instIndex].months[monthIndex] = !newData.statements[instIndex].months[monthIndex];
    setData(newData);
    saveChecklist(newData);
  };

  const handleTaskToggle = (weekIndex, taskIndex) => {
    const newData = structuredClone(data);
    newData.triage[weekIndex].tasks[taskIndex].completed = !newData.triage[weekIndex].tasks[taskIndex].completed;
    setData(newData);
    saveChecklist(newData);
  };

  if (loading) {
    return (
      <div className="h-full flex items-center justify-center">
        <div className="w-12 h-12 border-4 border-cyan-400 border-t-transparent rounded-full animate-spin"></div>
      </div>
    );
  }

  if (!data) return <div className="text-white">Error loading checklist data.</div>;

  const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

  return (
    <div className="h-full flex flex-col overflow-y-auto custom-scrollbar p-6 space-y-12 pb-24">
      
      {/* Header */}
      <div className="flex flex-col items-center justify-center space-y-2 mb-8 border-b border-white/10 pb-6">
        <span className="text-5xl drop-shadow-[0_0_15px_rgba(0,212,255,0.5)]">⚖️</span>
        <h2 className="text-3xl font-black text-white tracking-widest uppercase italic">Master Checklist</h2>
        <p className="text-sm font-mono text-cyan-400/80 uppercase tracking-widest">Sovereign Ledger Operations</p>
        {saving && <span className="text-[10px] font-mono text-white/40 uppercase tracking-widest">Saving...</span>}
      </div>

      {/* Monthly Triage */}
      <div className="space-y-6">
        <div className="flex items-center gap-3 border-b border-white/10 pb-2">
          <span className="text-2xl">🏦</span>
          <h3 className="text-xl font-bold text-white uppercase tracking-widest">Monthly Statement Triage</h3>
        </div>
        
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          {data.triage.map((weekData, wIdx) => {
            const isWeek1 = weekData.color === 'green';
            const isWeek2 = weekData.color === 'yellow';
            const isWeek3 = weekData.color === 'orange';
            const isWeek4 = weekData.color === 'red';
            
            let borderColor = 'border-cyan-500/30';
            let bgGradient = 'from-cyan-950/40 to-black/60';
            let titleColor = 'text-cyan-400';
            
            if (isWeek1) { borderColor = 'border-emerald-500/30'; bgGradient = 'from-emerald-950/40 to-black/60'; titleColor = 'text-emerald-400'; }
            if (isWeek2) { borderColor = 'border-amber-500/30'; bgGradient = 'from-amber-950/40 to-black/60'; titleColor = 'text-amber-400'; }
            if (isWeek3) { borderColor = 'border-orange-500/30'; bgGradient = 'from-orange-950/40 to-black/60'; titleColor = 'text-orange-400'; }
            if (isWeek4) { borderColor = 'border-red-500/30'; bgGradient = 'from-red-950/40 to-black/60'; titleColor = 'text-red-400'; }

            return (
              <motion.div 
                key={wIdx}
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: wIdx * 0.1 }}
                className={`bg-gradient-to-br ${bgGradient} border ${borderColor} rounded-xl p-5 backdrop-blur-md shadow-xl flex flex-col`}
              >
                <h4 className={`text-sm font-black uppercase tracking-widest mb-4 ${titleColor} border-b border-white/5 pb-2`}>
                  {weekData.week}
                </h4>
                <div className="space-y-2 flex-1">
                  {weekData.tasks.map((task, tIdx) => (
                    <div 
                      key={task.id} 
                      className={`flex items-center justify-between p-2 rounded transition-all ${task.completed ? 'bg-white/5 opacity-50' : 'hover:bg-white/5'}`}
                    >
                      <div className="flex items-center gap-3">
                        <button 
                          onClick={() => handleTaskToggle(wIdx, tIdx)}
                          className={`w-5 h-5 rounded border flex items-center justify-center transition-all ${
                            task.completed 
                            ? 'bg-emerald-500 border-emerald-500 text-black' 
                            : 'bg-black/50 border-white/20 hover:border-cyan-400'
                          }`}
                        >
                          {task.completed && <span className="text-xs font-bold">✓</span>}
                        </button>
                        <div className="flex flex-col">
                          {task.url ? (
                            <a href={task.url} target="_blank" rel="noreferrer" className={`text-sm font-bold transition-colors ${task.completed ? 'line-through text-white/50' : 'text-white hover:text-cyan-400'}`}>
                              {task.title}
                            </a>
                          ) : (
                            <span className={`text-sm font-bold ${task.completed ? 'line-through text-white/50' : 'text-white'}`}>
                              {task.title}
                            </span>
                          )}
                        </div>
                      </div>
                      <div className="text-[10px] font-mono text-white/30 uppercase tracking-widest bg-black/40 px-2 py-0.5 rounded">
                        Day {task.day}
                      </div>
                    </div>
                  ))}
                </div>
              </motion.div>
            );
          })}
        </div>
      </div>

      {/* Statement Status Grid */}
      <div className="space-y-6 pt-8">
        <div className="flex flex-col gap-1 border-b border-white/10 pb-2">
          <div className="flex items-center gap-3">
            <span className="text-2xl">📊</span>
            <h3 className="text-xl font-bold text-white uppercase tracking-widest">{data.year} Statement Status Grid</h3>
          </div>
          <p className="text-xs text-white/40 font-mono italic pl-9">Tap blocks once PDF is archived in Vault/Statements/{data.year}/</p>
        </div>

        <div className="overflow-x-auto custom-scrollbar pb-4">
          <div className="min-w-[1100px] bg-zinc-950/80 border border-white/10 rounded-xl overflow-hidden backdrop-blur-md shadow-2xl">
            {/* Grid Header */}
            <div className="grid grid-cols-[2.5fr_repeat(12,0.6fr)] bg-black/60 border-b border-white/10 text-[10px] uppercase font-bold tracking-widest text-white/50 py-3">
              <div className="pl-4">Institution</div>
              {months.map(m => (
                <div key={m} className="text-center">{m}</div>
              ))}
            </div>
            
            {/* Grid Body */}
            <div className="divide-y divide-white/5">
              {data.statements.map((inst, iIdx) => (
                <div key={inst.institution} className="grid grid-cols-[2.5fr_repeat(12,0.6fr)] hover:bg-white/5 transition-colors items-center py-2">
                  <div className="pl-4 text-xs font-bold text-white/80 truncate pr-2">
                    {inst.institution}
                  </div>
                  {inst.months.map((isComplete, mIdx) => (
                    <div key={mIdx} className="flex justify-center">
                      <button 
                        onClick={() => handleStatementToggle(iIdx, mIdx)}
                        className={`w-6 h-6 rounded shadow-sm transition-all duration-300 ${
                          isComplete 
                          ? 'bg-emerald-500/80 shadow-[0_0_10px_rgba(16,185,129,0.4)] border border-emerald-400' 
                          : 'bg-white/5 border border-white/10 hover:border-cyan-500 hover:bg-cyan-500/20'
                        }`}
                      />
                    </div>
                  ))}
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
      
    </div>
  );
}
