export interface GraphNode {
  id: string;
  memory_type: string;
  access_count: number;
  content: string;
  created_at: string;
  [key: string]: unknown;
}

export interface GraphEdge {
  source: string;
  target: string;
  relation_type: string;
  weight: number;
  [key: string]: unknown;
}

export interface GraphData {
  nodes: GraphNode[];
  links: GraphEdge[];
}
