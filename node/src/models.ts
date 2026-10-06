export type MemoryType =
  | "fact"
  | "decision"
  | "instruction"
  | "preference"
  | "learning"
  | "goal"
  | "commitment"
  | "artifact"
  | "event"
  | "relationship"
  | "observation"
  | "error"
  | "context"
  | "password";

export type MemoryStatus = "active" | "expired";

export interface Memory {
  id: string;
  category: MemoryType;
  title: string;
  content: string;
  tags: string[];
  confidence: number;
  status: MemoryStatus;
  provenance: string;
  created_at: string;
  updated_at: string;
  expired_at?: string | null;
}

export interface LinkTarget {
  id: string;
  relation: string;
  title: string;
}

export interface MemoryInspection {
  memory: Memory;
  links: LinkTarget[];
}

export interface ScoredMemory {
  memory: Memory;
  score: number;
  vector_rank?: number | null;
  bm25_rank?: number | null;
}

export interface AnswerResult {
  answer: string;
  confidence: number;
  reranker_used: string;
  selected_memory?: Memory | null;
  evidence?: string | null;
}

export interface RelatedMemory {
  memory: Memory;
  distance: number;
  relation_path: string[];
  score: number;
}

export interface StorageStats {
  total_memories: number;
  active_memories: number;
  expired_memories: number;
  vector_count: number;
  categories: Record<string, number>;
  storage_size_bytes: number;
  db_path: string;
  backend?: string | null;
  embedding_model?: string | null;
  reranker?: string | null;
}

export interface PaginatedList<T> {
  items: T[];
  total: number;
  limit: number;
  offset: number;
  page: number;
  total_pages: number;
  has_more: boolean;
}

export interface MemoryConflict {
  id_a: string;
  id_b: string;
  title_a: string;
  title_b: string;
  content_a: string;
  content_b: string;
  conflict_type: string;
  similarity: number;
  reasoning: string;
}

export interface RememberOptions {
  category?: MemoryType;
  title?: string;
  tags?: string[];
  confidence?: number;
  ttl?: string;
  supersede?: boolean;
}

export interface RecallOptions {
  limit?: number;
  category?: MemoryType;
  minSimilarity?: number;
  multiHop?: boolean;
  asOf?: string;
  date?: string;
}

export interface AnswerOptions {
  limit?: number;
  category?: MemoryType;
  reranker?: string;
  precision?: boolean;
  asOf?: string;
  date?: string;
}

export interface ListOptions {
  limit?: number;
  offset?: number;
  page?: number;
  category?: MemoryType;
  asOf?: string;
  date?: string;
}

export interface ForgetOptions {
  hard?: boolean;
}

export interface ClearOptions {
  soft?: boolean;
}

export interface ConflictsOptions {
  threshold?: number;
  limit?: number;
  autoResolve?: boolean;
}

export interface LightMemConfig {
  dbPath?: string;
  globalDb?: boolean;
  binaryPath?: string;
}
