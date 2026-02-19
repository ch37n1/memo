import { useState } from 'react';
import type { CreateTokenInput, CreatedTokenDto, TokenViewDto } from '../hooks/useMemoClient';

interface Props {
  tokens: TokenViewDto[];
  latestToken: CreatedTokenDto | null;
  onCreate: (input: CreateTokenInput) => Promise<void>;
  onRevoke: (id: string) => Promise<void>;
}

export function TokenList({ tokens, latestToken, onCreate, onRevoke }: Props) {
  const [name, setName] = useState('agent-ui');
  const [scopes, setScopes] = useState('meta:*:read,admin:*:*');

  return (
    <div className="card tokens">
      <h2>Token Registry</h2>
      <div className="row">
        <input value={name} onChange={(event) => setName(event.target.value)} placeholder="token name" />
        <input
          value={scopes}
          onChange={(event) => setScopes(event.target.value)}
          placeholder="scope1,scope2"
        />
        <button
          onClick={() =>
            onCreate({
              name,
              scopes: scopes
                .split(',')
                .map((scope) => scope.trim())
                .filter(Boolean),
            })
          }
        >
          Create
        </button>
      </div>

      {latestToken ? (
        <p className="message mono">New token value: {latestToken.token}</p>
      ) : null}

      <table className="table">
        <thead>
          <tr>
            <th>Name</th>
            <th>Scopes</th>
            <th>Created</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {tokens.map((token) => (
            <tr key={token.id}>
              <td className="mono">{token.name}</td>
              <td>
                {token.scopes.map((scope) => (
                  <span key={scope} className="badge mono">
                    {scope}
                  </span>
                ))}
              </td>
              <td className="mono">{token.created_at}</td>
              <td>
                <button className="danger" onClick={() => onRevoke(token.id)}>
                  Revoke
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
