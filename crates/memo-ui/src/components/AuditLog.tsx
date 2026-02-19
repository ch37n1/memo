import { useState } from 'react';
import type { AuditEntryDto, AuditFilterInput } from '../hooks/useMemoClient';

interface Props {
  entries: AuditEntryDto[];
  onFilter: (filter: AuditFilterInput) => Promise<void>;
}

export function AuditLog({ entries, onFilter }: Props) {
  const [mount, setMount] = useState('');
  const [operation, setOperation] = useState('');
  const [result, setResult] = useState<'' | 'ok' | 'error'>('');

  return (
    <div className="card audit">
      <h2>Audit Stream</h2>
      <div className="row">
        <input value={mount} onChange={(event) => setMount(event.target.value)} placeholder="mount" />
        <input
          value={operation}
          onChange={(event) => setOperation(event.target.value)}
          placeholder="operation"
        />
        <select value={result} onChange={(event) => setResult(event.target.value as '' | 'ok' | 'error')}>
          <option value="">any result</option>
          <option value="ok">ok</option>
          <option value="error">error</option>
        </select>
        <button
          className="secondary"
          onClick={() =>
            onFilter({
              mount: mount || undefined,
              operation: operation || undefined,
              result: result || undefined,
              limit: 200,
            })
          }
        >
          Apply
        </button>
      </div>

      <table className="table">
        <thead>
          <tr>
            <th>Id</th>
            <th>Timestamp</th>
            <th>Operation</th>
            <th>Mount</th>
            <th>Result</th>
            <th>Error</th>
          </tr>
        </thead>
        <tbody>
          {entries.map((entry) => (
            <tr key={entry.id}>
              <td className="mono">{entry.id}</td>
              <td className="mono">{entry.timestamp}</td>
              <td className="mono">{entry.operation}</td>
              <td className="mono">{entry.mount ?? '-'}</td>
              <td>{entry.result}</td>
              <td className="mono">{entry.error_code ?? '-'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
