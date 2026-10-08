import type { TreeItem } from '../types'

export interface TreeNode extends TreeItem {
  children: TreeNode[]
}

export function buildTree(items: TreeItem[]): TreeNode[] {
  const nodes = new Map<string, TreeNode>(items.map((i) => [i.id, { ...i, children: [] }]))
  const roots: TreeNode[] = []
  for (const node of nodes.values()) {
    const parent = nodes.get(node.parentId)
    if (parent) parent.children.push(node)
    else roots.push(node)
  }
  const sort = (list: TreeNode[]): TreeNode[] => {
    list.sort((a, b) => a.seq - b.seq)
    for (const n of list) sort(n.children)
    return list
  }
  return sort(roots)
}
