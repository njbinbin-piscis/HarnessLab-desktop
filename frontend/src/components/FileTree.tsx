import { useMemo, useState } from "react";

interface Props {
  files: string[];
  selected: string;
  onSelect: (path: string) => void;
}

interface Node {
  name: string;
  path: string;
  isFile: boolean;
  children: Map<string, Node>;
}

function buildTree(paths: string[]): Node {
  const root: Node = { name: "", path: "", isFile: false, children: new Map() };
  for (const p of paths) {
    const parts = p.split("/").filter(Boolean);
    let cur = root;
    parts.forEach((part, i) => {
      const isLeaf = i === parts.length - 1;
      let next = cur.children.get(part);
      if (!next) {
        next = {
          name: part,
          path: parts.slice(0, i + 1).join("/"),
          isFile: isLeaf,
          children: new Map(),
        };
        cur.children.set(part, next);
      }
      cur = next;
    });
  }
  return root;
}

function sortedChildren(node: Node): Node[] {
  return [...node.children.values()].sort((a, b) => {
    if (a.isFile !== b.isFile) return a.isFile ? 1 : -1; // folders first
    return a.name.localeCompare(b.name);
  });
}

function FolderIcon({ open }: { open: boolean }) {
  return (
    <svg className="ft-twisty" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
      {open ? <path d="M1 3 L9 3 L5 8 Z" fill="currentColor" /> : <path d="M3 1 L8 5 L3 9 Z" fill="currentColor" />}
    </svg>
  );
}

function FileGlyph() {
  return (
    <svg className="ft-glyph" width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
      <path d="M2.5 1 H7 L9.5 3.5 V11 H2.5 Z" fill="none" stroke="currentColor" strokeWidth="1" />
      <path d="M7 1 V3.5 H9.5" fill="none" stroke="currentColor" strokeWidth="1" />
    </svg>
  );
}

function Row({
  node,
  depth,
  selected,
  onSelect,
  collapsed,
  toggle,
}: {
  node: Node;
  depth: number;
  selected: string;
  onSelect: (p: string) => void;
  collapsed: Set<string>;
  toggle: (p: string) => void;
}) {
  const pad = { paddingLeft: 8 + depth * 12 };
  if (node.isFile) {
    return (
      <div
        className={"ft-row ft-file" + (selected === node.path ? " active" : "")}
        style={pad}
        title={node.path}
        onClick={() => onSelect(node.path)}
      >
        <FileGlyph />
        <span className="ft-name">{node.name}</span>
      </div>
    );
  }
  const open = !collapsed.has(node.path);
  return (
    <>
      <div className="ft-row ft-folder" style={pad} title={node.path} onClick={() => toggle(node.path)}>
        <FolderIcon open={open} />
        <span className="ft-name">{node.name}</span>
      </div>
      {open &&
        sortedChildren(node).map((c) => (
          <Row
            key={c.path}
            node={c}
            depth={depth + 1}
            selected={selected}
            onSelect={onSelect}
            collapsed={collapsed}
            toggle={toggle}
          />
        ))}
    </>
  );
}

export function FileTree({ files, selected, onSelect }: Props) {
  const tree = useMemo(() => buildTree(files), [files]);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const toggle = (p: string) =>
    setCollapsed((s) => {
      const next = new Set(s);
      if (next.has(p)) next.delete(p);
      else next.add(p);
      return next;
    });

  return (
    <div className="file-tree">
      {sortedChildren(tree).map((c) => (
        <Row
          key={c.path}
          node={c}
          depth={0}
          selected={selected}
          onSelect={onSelect}
          collapsed={collapsed}
          toggle={toggle}
        />
      ))}
    </div>
  );
}
