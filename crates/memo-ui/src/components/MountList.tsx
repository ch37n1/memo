import { useState } from 'react';
import type { CreateMountInput, MountDto } from '../hooks/useMemoClient';

interface Props {
  mounts: MountDto[];
  onCreate: (input: CreateMountInput) => Promise<void>;
  onRemove: (name: string) => Promise<void>;
}

export function MountList({ mounts, onCreate, onRemove }: Props) {
  const [form, setForm] = useState<CreateMountInput>({
    name: '',
    rootPath: '',
    mode: 'read_write',
    audience: 'shared',
    description: '',
  });

  return (
    <div className="card mounts">
      <h2>Mount Registry</h2>
      <div className="row">
        <input
          placeholder="MountName"
          value={form.name}
          onChange={(event) => setForm({ ...form, name: event.target.value })}
        />
        <input
          placeholder="/absolute/path"
          value={form.rootPath}
          onChange={(event) => setForm({ ...form, rootPath: event.target.value })}
        />
      </div>
      <div className="row">
        <select
          value={form.mode}
          onChange={(event) => setForm({ ...form, mode: event.target.value as CreateMountInput['mode'] })}
        >
          <option value="read_write">read_write</option>
          <option value="read_only">read_only</option>
        </select>
        <select
          value={form.audience}
          onChange={(event) => setForm({ ...form, audience: event.target.value as CreateMountInput['audience'] })}
        >
          <option value="shared">shared</option>
          <option value="private">private</option>
        </select>
        <input
          placeholder="description"
          value={form.description}
          onChange={(event) => setForm({ ...form, description: event.target.value })}
        />
        <button onClick={() => onCreate(form)}>Add</button>
      </div>
      <table className="table">
        <thead>
          <tr>
            <th>Name</th>
            <th>Root Path</th>
            <th>Mode</th>
            <th>Audience</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {mounts.map((mount) => (
            <tr key={mount.name}>
              <td className="mono">{mount.name}</td>
              <td className="mono">{mount.root_path}</td>
              <td>{mount.mode}</td>
              <td>{mount.audience}</td>
              <td>
                <button className="danger" onClick={() => onRemove(mount.name)}>
                  Remove
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
