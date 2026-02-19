import { useEffect, useMemo, useState } from 'react';
import { Store } from '@tauri-apps/plugin-store';
import { MountList } from './components/MountList';
import { TokenList } from './components/TokenList';
import { AuditLog } from './components/AuditLog';
import { FileBrowser } from './components/FileBrowser';
import {
  useMemoClient,
  type AuditEntryDto,
  type CreatedTokenDto,
  type MountDto,
  type TokenViewDto,
  type TreeNodeDto,
} from './hooks/useMemoClient';

const TOKEN_STORE = 'memo-ui.store.json';
const TOKEN_KEY = 'admin_token';
const BASE_URL = 'http://127.0.0.1:18301';

export function App() {
  const client = useMemoClient();
  const [token, setToken] = useState('');
  const [mounts, setMounts] = useState<MountDto[]>([]);
  const [tokens, setTokens] = useState<TokenViewDto[]>([]);
  const [auditEntries, setAuditEntries] = useState<AuditEntryDto[]>([]);
  const [tree, setTree] = useState<TreeNodeDto | null>(null);
  const [latestToken, setLatestToken] = useState<CreatedTokenDto | null>(null);
  const [status, setStatus] = useState('');
  const [error, setError] = useState('');

  const store = useMemo(() => new Store(TOKEN_STORE), []);

  async function reloadAll(activeToken: string) {
    const [mountList, tokenList, audit] = await Promise.all([
      client.listMounts(BASE_URL, activeToken),
      client.listTokens(BASE_URL, activeToken),
      client.queryAudit(BASE_URL, activeToken, { limit: 100 }),
    ]);

    setMounts(mountList);
    setTokens(tokenList);
    setAuditEntries(audit);

    if (mountList.length > 0) {
      const firstMount = mountList[0]?.name;
      if (firstMount) {
        const treeResponse = await client.browseTree(BASE_URL, activeToken, firstMount, 3);
        setTree(treeResponse.tree);
      }
    }
  }

  useEffect(() => {
    void (async () => {
      const saved = await store.get<string>(TOKEN_KEY);
      if (saved) {
        setToken(saved);
        try {
          await reloadAll(saved);
          setStatus('Connected to memod.');
        } catch (loadError) {
          setError(String(loadError));
        }
      }
    })();
  }, [store]);

  if (!token) {
    return (
      <div className="app token-setup">
        <div className="card">
          <h2>Admin Token Setup</h2>
          <p className="muted">Paste bootstrap/admin token. It will be stored via tauri-plugin-store.</p>
          <textarea rows={4} value={token} onChange={(event) => setToken(event.target.value)} />
          <div className="row">
            <button
              onClick={() =>
                void (async () => {
                  setError('');
                  try {
                    await store.set(TOKEN_KEY, token);
                    await store.save();
                    await reloadAll(token);
                    setStatus('Connected to memod.');
                  } catch (saveError) {
                    setError(String(saveError));
                  }
                })()
              }
            >
              Save and Connect
            </button>
          </div>
          {error ? <p className="error">{error}</p> : null}
        </div>
      </div>
    );
  }

  return (
    <div className="app">
      <div className="header">
        <h1>memo-ui control plane</h1>
        <div className="mono">{BASE_URL}</div>
      </div>
      {status ? <p className="message">{status}</p> : null}
      {error ? <p className="error">{error}</p> : null}

      <div className="grid">
        <MountList
          mounts={mounts}
          onCreate={async (input) => {
            setError('');
            await client.createMount(BASE_URL, token, input);
            await reloadAll(token);
          }}
          onRemove={async (name) => {
            setError('');
            await client.removeMount(BASE_URL, token, name);
            await reloadAll(token);
          }}
        />

        <TokenList
          tokens={tokens}
          latestToken={latestToken}
          onCreate={async (input) => {
            setError('');
            const created = await client.createToken(BASE_URL, token, input);
            setLatestToken(created);
            await reloadAll(token);
          }}
          onRevoke={async (id) => {
            setError('');
            await client.revokeToken(BASE_URL, token, id);
            await reloadAll(token);
          }}
        />

        <AuditLog
          entries={auditEntries}
          onFilter={async (filter) => {
            setError('');
            const entries = await client.queryAudit(BASE_URL, token, filter);
            setAuditEntries(entries);
          }}
        />

        <FileBrowser tree={tree} />
      </div>
    </div>
  );
}
