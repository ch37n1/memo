import type { TreeNodeDto } from '../hooks/useMemoClient';

interface Props {
  tree: TreeNodeDto | null;
}

function Node({ node, level }: { node: TreeNodeDto; level: number }) {
  return (
    <div style={{ marginLeft: `${level * 12}px` }}>
      <span className="mono">
        {node.kind === 'dir' ? 'd' : 'f'} {node.name}
      </span>
      {node.children.map((child) => (
        <Node key={`${level}-${child.name}`} node={child} level={level + 1} />
      ))}
    </div>
  );
}

export function FileBrowser({ tree }: Props) {
  return (
    <div className="card browser">
      <h2>Read-only Browser</h2>
      {tree ? <Node node={tree} level={0} /> : <p className="muted">Select mount to load tree.</p>}
    </div>
  );
}
