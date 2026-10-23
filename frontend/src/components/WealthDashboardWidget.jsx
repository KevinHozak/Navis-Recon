import { useState, useEffect } from 'react';
import { AreaChart, Area, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from 'recharts';
import { getLedgerHistory } from '../services/apiClient';

const formatYAxis = (tickItem) => {
  if (tickItem >= 1000000) return `$${(tickItem / 1000000).toFixed(1)}M`;
  if (tickItem >= 1000) return `$${(tickItem / 1000).toFixed(0)}k`;
  return `$${tickItem}`;
};

const CustomTooltip = ({ active, payload, label }) => {
  if (active && payload && payload.length) {
    const data = payload[0];
    return (
      <div className="bg-zinc-950/90 border border-white/10 p-4 rounded-lg shadow-2xl backdrop-blur-sm">
        <p className="text-white/50 text-xs font-mono uppercase mb-2">{label}</p>
        <p className="text-white font-bold text-lg">
          {data.name}: <span style={{ color: data.color }}>${data.value.toLocaleString('en-US', { minimumFractionDigits: 2 })}</span>
        </p>
      </div>
    );
  }
  return null;
};

// Formats "2026-04" -> "Apr 26"
const formatXAxis = (tickItem) => {
  if (!tickItem) return '';
  const [year, month] = tickItem.split('-');
  const date = new Date(year, month - 1);
  return date.toLocaleDateString('en-US', { month: 'short', year: '2-digit' });
};

export default function WealthDashboardWidget() {
  const [history, setHistory] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  async function fetchHistory() {
    try {
      setHistory(await getLedgerHistory());
    } catch (e) {
      console.error(e);
      setError(e?.message || String(e) || "Failed to fetch dashboard history");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    // Load external ledger history when the dashboard mounts.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    fetchHistory();
  }, []);

  if (loading) {
    return <div className="p-8 text-center text-accent uppercase font-mono animate-pulse">Calculating Lifetime Wealth Metrics...</div>;
  }

  if (error) {
    return <div className="text-red-400 font-mono text-center">{error}</div>;
  }

  const renderChart = (title, dataKey, color) => (
    <div className="mb-12">
      <h3 className="text-xl font-bold mb-6 text-white uppercase tracking-widest">{title}</h3>
      <div className="h-64 w-full bg-zinc-950 border border-white/5 rounded-lg p-4 shadow-inner relative group">
        <div className="absolute inset-0 bg-gradient-to-b from-white/[0.02] to-transparent opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none rounded-lg" />
        <ResponsiveContainer width="100%" height="100%">
          <AreaChart data={history} margin={{ top: 10, right: 10, left: 0, bottom: 0 }}>
            <defs>
              <linearGradient id={`color_${dataKey}`} x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor={color} stopOpacity={0.3}/>
                <stop offset="95%" stopColor={color} stopOpacity={0}/>
              </linearGradient>
            </defs>
            <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.05)" vertical={false} />
            <XAxis 
              dataKey="name" 
              stroke="rgba(255,255,255,0.2)" 
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 10, fontFamily: 'monospace' }} 
              tickFormatter={formatXAxis}
              tickMargin={10}
              minTickGap={20}
            />
            <YAxis 
              stroke="rgba(255,255,255,0.2)" 
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 10, fontFamily: 'monospace' }} 
              tickFormatter={formatYAxis}
              width={60}
              domain={['auto', 'auto']}
            />
            <Tooltip content={<CustomTooltip />} />
            <Area 
              type="monotone" 
              dataKey={dataKey} 
              stroke={color} 
              strokeWidth={3}
              fillOpacity={1} 
              fill={`url(#color_${dataKey})`} 
              animationDuration={1500}
              animationEasing="ease-out"
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  );

  return (
    <div className="space-y-8 animate-in fade-in duration-500 text-white">
      <div className="flex items-center gap-3 mb-8 border-b border-white/10 pb-6">
        <span className="text-4xl text-accent">🏦</span>
        <div>
          <h2 className="text-3xl font-black uppercase tracking-widest text-white">The Vault Performance</h2>
          <p className="text-white/40 text-sm font-mono mt-1">Live Beancount Telemetry</p>
        </div>
      </div>
      
      <div className="max-w-5xl mx-auto pb-12">
        {renderChart("📈 Overall Portfolio Growth", "Net Worth", "#059669")}
        
        <h2 className="text-2xl font-black mt-16 mb-8 text-accent border-b border-accent/20 pb-4">Retirement Accounts</h2>
        <div className="grid grid-cols-1 xl:grid-cols-2 gap-8">
          {renderChart("401k", "401k", "#4f46e5")}
          {renderChart("IRA Primary", "IRA Primary", "#4f46e5")}
          {renderChart("IRA Secondary", "IRA Secondary", "#4f46e5")}
          {renderChart("HSA", "HSA", "#4f46e5")}
        </div>

        <h2 className="text-2xl font-black mt-16 mb-8 text-accent border-b border-accent/20 pb-4">Brokerage & Investments</h2>
        <div className="grid grid-cols-1 xl:grid-cols-2 gap-8">
          {renderChart("Stocks", "Stocks", "#0ea5e9")}
          {renderChart("Stocks Secondary", "Stocks Secondary", "#0ea5e9")}
        </div>
      </div>
    </div>
  );
}
