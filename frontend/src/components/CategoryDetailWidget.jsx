import React, { useState, useEffect } from 'react';
import { PieChart, Pie, Cell, Tooltip, Legend, ResponsiveContainer } from 'recharts';
import TransactionDrillDown from './TransactionDrillDown';

const API_BASE = 'http://localhost:8080';

export default function CategoryDetailWidget({ categoryKey, title, icon, isLiability = false }) {
  const [summary, setSummary] = useState(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetchSummary();
  }, [categoryKey]);

  const fetchSummary = async () => {
    try {
      setLoading(true);
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
    return <div className="p-8 text-center text-accent uppercase font-mono animate-pulse">Loading {title}...</div>;
  }

  if (!summary || !summary[categoryKey]) {
    return <div className="text-red-400">Error loading data for {title}.</div>;
  }

  const rawData = summary[categoryKey];
  const chartData = Object.entries(rawData)
    .filter(([_, value]) => value !== 0)
    .map(([name, value]) => ({
      name,
      value: Math.abs(value),
      originalValue: value
    }))
    .sort((a, b) => b.value - a.value);

  const total = chartData.reduce((acc, item) => acc + item.value, 0);

  // Colors for the donut chart (using Tailwind-ish hex codes)
  const COLORS = isLiability 
    ? ['#ef4444', '#f87171', '#fca5a5', '#fecaca', '#fee2e2'] 
    : ['#00d4ff', '#00b4d8', '#0096c7', '#0077b6', '#023e8a', '#03045e'];

  const borderColor = isLiability ? "border-red-500/20" : "border-accent/20";
  const textColor = isLiability ? "text-red-500" : "text-accent";

  // Map categoryKey to the Beancount prefix for transactions
  const prefixMap = {
    'bank_accounts': 'Assets:Bank',
    'retirement': 'Assets:Retirement',
    'investments': 'Assets:Investments',
    'credit_cards': 'Liabilities:CreditCard'
  };
  const beancountPrefix = prefixMap[categoryKey];

  return (
    <div className="space-y-8 animate-in fade-in duration-500">
      <div className={`p-8 bg-zinc-950 border ${borderColor} rounded-lg shadow-2xl`}>
        <div className="flex items-center gap-4 mb-8">
          <span className={`text-4xl ${textColor}`}>{icon}</span>
          <h2 className="text-4xl font-black text-white uppercase tracking-tighter">{title}</h2>
          <div className="flex-1" />
          <span className={`text-3xl font-bold ${textColor}`}>${total.toLocaleString('en-US', { minimumFractionDigits: 2 })}</span>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-12 items-center">
          {/* Chart Section */}
          <div className="h-80 w-full">
            <ResponsiveContainer width="100%" height="100%">
              <PieChart>
                <Pie
                  data={chartData}
                  cx="50%"
                  cy="50%"
                  innerRadius={90}
                  outerRadius={130}
                  paddingAngle={3}
                  dataKey="value"
                  stroke="rgba(255,255,255,0.02)"
                  strokeWidth={2}
                >
                  {chartData.map((entry, index) => (
                    <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} />
                  ))}
                </Pie>
                <Tooltip 
                  formatter={(value) => `$${value.toLocaleString('en-US', { minimumFractionDigits: 2 })}`}
                  contentStyle={{ backgroundColor: '#09090b', borderColor: 'rgba(255,255,255,0.1)', color: '#fff', borderRadius: '8px' }}
                  itemStyle={{ color: '#fff', fontWeight: 'bold' }}
                />
              </PieChart>
            </ResponsiveContainer>
          </div>

          {/* List Section */}
          <div className="space-y-4 flex flex-col justify-center">
            {chartData.map((item, index) => (
              <div key={item.name} className="flex justify-between items-center p-4 bg-white/5 rounded border border-white/5 hover:bg-white/10 transition-colors">
                <div className="flex items-center gap-3">
                  <div className="w-3 h-3 rounded-full shadow-[0_0_8px_currentColor]" style={{ backgroundColor: COLORS[index % COLORS.length], color: COLORS[index % COLORS.length] }}></div>
                  <span className="font-mono text-sm text-white/90 font-bold">{item.name}</span>
                </div>
                <div className="text-right">
                  <span className="font-bold text-white/90">${item.value.toLocaleString('en-US', { minimumFractionDigits: 2 })}</span>
                  <div className="text-[10px] text-white/40 font-mono mt-1">
                    {((item.value / total) * 100).toFixed(1)}%
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Transaction Drill Down */}
        {beancountPrefix && (
          <TransactionDrillDown prefix={beancountPrefix} isLiability={isLiability} />
        )}
      </div>
    </div>
  );
}
