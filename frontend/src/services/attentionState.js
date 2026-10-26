export const attentionSources = ['sync', 'statements', 'reviews', 'renewals', 'records'];
const array = value => { if (!Array.isArray(value)) throw new Error('Malformed source response'); return value; };
const text = value => { if (typeof value !== 'string' || !value) throw new Error('Missing source identity'); return value; };
const item = (id, title, view, filters, priority) => ({ id, title, view, filters, priority });

export function attentionItems(source, data) {
  switch (source) {
    case 'sync': {
      const outcomes = array(Array.isArray(data) ? data : data?.outcomes);
      if (!Array.isArray(data) && !['idle', 'analyzing', 'review', 'committing', 'complete', 'failed', 'cancelled'].includes(data?.run?.status)) throw new Error('Sync run status unavailable');
      const items = outcomes.flatMap(row => {
      if (!['succeeded', 'failed', 'uncertain'].includes(row.state)) throw new Error('Malformed sync state');
      if (row.item_id === 'store' || row.item_id === 'store_error' || row.item_id === 'recovery_store_error') throw new Error(row.message || 'Sync outcome store unavailable');
      return row.state === 'succeeded' ? [] : [item(text(row.operation_id || row.item_id), `${row.state === 'failed' ? 'Failed import' : 'Uncertain import'}: ${text(row.filename || 'Statement')}`, 'sync', { recoveryOnly: true, operationId: row.operation_id || row.item_id }, 0)];
      });
      if (data?.run?.status === 'failed') items.push(item(`run:${text(data.run.run_id)}`, 'Failed sync run · inspect diagnostics', 'sync', {}, 0));
      return items;
    }
    case 'statements': return array(data?.anomalies).flatMap((row, index) => row.kind === 'missing_interval' ? [item(`${text(row.institution)}-${index}`, `Missing statements · ${row.institution}: ${text(row.message)}`, 'statements', { institution: row.institution, status: 'ALL' }, 1)] : []);
    case 'reviews': return array(data).length ? [item('pending', `${data.length} pending ledger review${data.length === 1 ? '' : 's'}`, 'reviews', {}, 2)] : [];
    case 'renewals': return array(data).flatMap(row => {
      if (row.metadata?.forecast_cancelled ?? row.metadata?.extra?.forecast_cancelled) return [];
      return array(row.alerts).length ? [item(text(row.key), `${text(row.payee)} · ${row.alerts.map(alert => text(alert.message)).join('; ')}`, 'subscriptions', { search: row.payee, currency: row.currency, alertsOnly: true, account: row.account }, 3)] : [];
    });
    case 'records': return array(data?.items).flatMap(row => row.isActionRequired || ['expired', 'critical', 'upcoming'].includes(row.bucket) ? [item(text(row.record?.id), `${text(row.record?.title)} · ${text(row.bucket)}${row.record.expirationDate ? ` (${row.record.expirationDate})` : ''}`, 'records', { recordId: row.record.id }, row.bucket === 'expired' ? 1 : 4)] : []);
    default: throw new Error('Unknown attention source');
  }
}

export function createAttentionController({ loaders, onChange, now = Date.now }) {
  let disposed = false;
  const generations = {};
  const sources = Object.fromEntries(attentionSources.map(id => [id, { status: 'unavailable', items: [], checkedAt: null, error: null }]));
  const snapshot = () => Object.fromEntries(Object.entries(sources).map(([id, state]) => [id, { ...state, items: [...state.items] }]));
  const publish = () => { if (!disposed) onChange(snapshot()); };
  async function refresh(ids = attentionSources) {
    await Promise.all([...ids].filter(id => loaders[id]).map(async id => {
      const generation = generations[id] = (generations[id] || 0) + 1;
      sources[id] = { ...sources[id], status: sources[id].checkedAt === null ? 'loading' : 'stale', loading: true };
      publish();
      try {
        const data = await loaders[id]();
        const items = attentionItems(id, data);
        if (disposed || generation !== generations[id]) return;
        sources[id] = { status: 'current', loading: false, items, checkedAt: now(), error: null, count: id === 'reviews' ? data.length : items.length, asOfDate: data?.asOfDate || null, ledgerRevision: id === 'reviews' ? data[0]?.ledger_revision || null : null };
      } catch (error) {
        if (disposed || generation !== generations[id]) return;
        sources[id] = { ...sources[id], loading: false, status: sources[id].checkedAt === null ? 'unavailable' : 'stale', error: error?.message || String(error) };
      }
      publish();
    }));
  }
  return { refresh, snapshot, dispose: () => { disposed = true; } };
}
