import { executeLmem, executeLmemAsync, getBinaryPath } from "./binary";
import type {
  Memory,
  ScoredMemory,
  AnswerResult,
  RelatedMemory,
  StorageStats,
  PaginatedList,
  MemoryInspection,
  MemoryConflict,
  RememberOptions,
  RecallOptions,
  AnswerOptions,
  ListOptions,
  ForgetOptions,
  ClearOptions,
  ConflictsOptions,
  LightMemConfig,
} from "./models";

export class LightMem {
  readonly dbPath?: string;
  readonly globalDb: boolean;
  private readonly _customBinaryPath?: string;

  constructor(config: LightMemConfig = {}) {
    this.dbPath = config.dbPath;
    this.globalDb = Boolean(config.globalDb);
    this._customBinaryPath = config.binaryPath;
  }

  get binaryPath(): string {
    return this._customBinaryPath || getBinaryPath();
  }

  private _buildBaseArgs(): string[] {
    const args: string[] = [];
    if (this.globalDb) {
      args.push("--global");
    } else if (this.dbPath) {
      args.push("--db", this.dbPath);
    }
    return args;
  }

  private _callSync<T>(subcommand: string, extraArgs: string[], inputText?: string): T {
    const fullArgs = [...this._buildBaseArgs(), subcommand, ...extraArgs, "--json"];
    const { code, stdout, stderr } = executeLmem(fullArgs, inputText, this._customBinaryPath);

    if (code !== 0) {
      const err = stderr.trim() || stdout.trim();
      throw new Error(`lmem ${subcommand} failed (exit code ${code}): ${err}`);
    }

    try {
      return JSON.parse(stdout) as T;
    } catch (err) {
      throw new Error(`Failed to parse JSON output from lmem ${subcommand}: ${stdout}`);
    }
  }

  private async _call<T>(subcommand: string, extraArgs: string[], inputText?: string): Promise<T> {
    const fullArgs = [...this._buildBaseArgs(), subcommand, ...extraArgs, "--json"];
    const { code, stdout, stderr } = await executeLmemAsync(fullArgs, inputText, this._customBinaryPath);

    if (code !== 0) {
      const err = stderr.trim() || stdout.trim();
      throw new Error(`lmem ${subcommand} failed (exit code ${code}): ${err}`);
    }

    try {
      return JSON.parse(stdout) as T;
    } catch (err) {
      throw new Error(`Failed to parse JSON output from lmem ${subcommand}: ${stdout}`);
    }
  }

  // 1. Remember (Store)
  async remember(content: string, options: RememberOptions = {}): Promise<Memory> {
    const args = [content];
    if (options.category) args.push("-t", options.category);
    if (options.title) args.push("--title", options.title);
    if (options.tags && options.tags.length > 0) args.push("--tags", options.tags.join(","));
    if (options.confidence !== undefined) args.push("--confidence", String(options.confidence));
    if (options.ttl) args.push("--ttl", options.ttl);
    if (options.supersede) args.push("--supersede");

    const data = await this._call<any>("remember", args);
    return data.memory || data;
  }

  rememberSync(content: string, options: RememberOptions = {}): Memory {
    const args = [content];
    if (options.category) args.push("-t", options.category);
    if (options.title) args.push("--title", options.title);
    if (options.tags && options.tags.length > 0) args.push("--tags", options.tags.join(","));
    if (options.confidence !== undefined) args.push("--confidence", String(options.confidence));
    if (options.ttl) args.push("--ttl", options.ttl);
    if (options.supersede) args.push("--supersede");

    const data = this._callSync<any>("remember", args);
    return data.memory || data;
  }

  // 2. Recall (Hybrid Search)
  async recall(query: string, options: RecallOptions = {}): Promise<ScoredMemory[]> {
    const limit = options.limit ?? 5;
    const args = [query, "-l", String(limit)];
    if (options.category) args.push("-t", options.category);
    if (options.minSimilarity !== undefined) args.push("--min-similarity", String(options.minSimilarity));
    if (options.multiHop) args.push("--multi-hop");
    if (options.asOf) args.push("--as-of", options.asOf);
    if (options.date) args.push("--date", options.date);

    const data = await this._call<any[]>("recall", args);
    return data.map((item) => ({
      memory: item.memory || item,
      score: Number(item.score ?? 0),
      vector_rank: item.vector_rank,
      bm25_rank: item.bm25_rank,
    }));
  }

  recallSync(query: string, options: RecallOptions = {}): ScoredMemory[] {
    const limit = options.limit ?? 5;
    const args = [query, "-l", String(limit)];
    if (options.category) args.push("-t", options.category);
    if (options.minSimilarity !== undefined) args.push("--min-similarity", String(options.minSimilarity));
    if (options.multiHop) args.push("--multi-hop");
    if (options.asOf) args.push("--as-of", options.asOf);
    if (options.date) args.push("--date", options.date);

    const data = this._callSync<any[]>("recall", args);
    return data.map((item) => ({
      memory: item.memory || item,
      score: Number(item.score ?? 0),
      vector_rank: item.vector_rank,
      bm25_rank: item.bm25_rank,
    }));
  }

  // 3. Answer (Extractive QA)
  async answer(question: string, options: AnswerOptions = {}): Promise<AnswerResult> {
    const limit = options.limit ?? 10;
    const args = [question, "-l", String(limit)];
    if (options.category) args.push("-t", options.category);
    if (options.reranker) args.push("-r", options.reranker);
    if (options.precision) args.push("--precision");
    if (options.asOf) args.push("--as-of", options.asOf);
    if (options.date) args.push("--date", options.date);

    const data = await this._call<any>("answer", args);
    return {
      answer: data.answer || "",
      confidence: Number(data.confidence ?? 0),
      reranker_used: data.reranker_used || "",
      selected_memory: data.selected_memory ? data.selected_memory.memory || data.selected_memory : null,
      evidence: data.selected_memory ? (data.selected_memory.content || null) : null,
    };
  }

  answerSync(question: string, options: AnswerOptions = {}): AnswerResult {
    const limit = options.limit ?? 10;
    const args = [question, "-l", String(limit)];
    if (options.category) args.push("-t", options.category);
    if (options.reranker) args.push("-r", options.reranker);
    if (options.precision) args.push("--precision");
    if (options.asOf) args.push("--as-of", options.asOf);
    if (options.date) args.push("--date", options.date);

    const data = this._callSync<any>("answer", args);
    return {
      answer: data.answer || "",
      confidence: Number(data.confidence ?? 0),
      reranker_used: data.reranker_used || "",
      selected_memory: data.selected_memory ? data.selected_memory.memory || data.selected_memory : null,
      evidence: data.selected_memory ? (data.selected_memory.content || null) : null,
    };
  }

  // 4. Get & Inspect
  async get(id: string): Promise<Memory | null> {
    try {
      const data = await this._call<any>("inspect", [id]);
      return data.memory || data;
    } catch {
      return null;
    }
  }

  getSync(id: string): Memory | null {
    try {
      const data = this._callSync<any>("inspect", [id]);
      return data.memory || data;
    } catch {
      return null;
    }
  }

  async inspect(id: string): Promise<MemoryInspection> {
    const data = await this._call<any>("inspect", [id]);
    return {
      memory: data.memory || data,
      links: data.links || [],
    };
  }

  inspectSync(id: string): MemoryInspection {
    const data = this._callSync<any>("inspect", [id]);
    return {
      memory: data.memory || data,
      links: data.links || [],
    };
  }

  // 5. List
  async list(options: ListOptions = {}): Promise<PaginatedList<Memory>> {
    const args: string[] = [];
    if (options.limit !== undefined) args.push("-l", String(options.limit));
    if (options.offset !== undefined) args.push("--offset", String(options.offset));
    if (options.page !== undefined) args.push("--page", String(options.page));
    if (options.category) args.push("-t", options.category);
    if (options.asOf) args.push("--as-of", options.asOf);
    if (options.date) args.push("--date", options.date);

    const data = await this._call<any>("list", args);
    const rawList: any[] = Array.isArray(data) ? data : data.memories || data.items || [];

    return {
      items: rawList.map((item) => item.memory || item),
      total: Number(data.total ?? rawList.length),
      limit: Number(data.limit ?? options.limit ?? 20),
      offset: Number(data.offset ?? options.offset ?? 0),
      page: Number(data.page ?? options.page ?? 1),
      total_pages: Number(data.total_pages ?? 1),
      has_more: Boolean(data.has_more ?? false),
    };
  }

  listSync(options: ListOptions = {}): PaginatedList<Memory> {
    const args: string[] = [];
    if (options.limit !== undefined) args.push("-l", String(options.limit));
    if (options.offset !== undefined) args.push("--offset", String(options.offset));
    if (options.page !== undefined) args.push("--page", String(options.page));
    if (options.category) args.push("-t", options.category);
    if (options.asOf) args.push("--as-of", options.asOf);
    if (options.date) args.push("--date", options.date);

    const data = this._callSync<any>("list", args);
    const rawList: any[] = Array.isArray(data) ? data : data.memories || data.items || [];

    return {
      items: rawList.map((item) => item.memory || item),
      total: Number(data.total ?? rawList.length),
      limit: Number(data.limit ?? options.limit ?? 20),
      offset: Number(data.offset ?? options.offset ?? 0),
      page: Number(data.page ?? options.page ?? 1),
      total_pages: Number(data.total_pages ?? 1),
      has_more: Boolean(data.has_more ?? false),
    };
  }

  // 6. Stats
  async stats(): Promise<StorageStats> {
    const data = await this._call<any>("stats", []);
    return {
      total_memories: Number(data.total_memories ?? 0),
      active_memories: Number(data.active_memories ?? 0),
      expired_memories: Number(data.expired_memories ?? 0),
      vector_count: Number(data.vector_count ?? 0),
      categories: data.categories || {},
      storage_size_bytes: Number(data.storage_size_bytes ?? 0),
      db_path: data.db_path || "",
      backend: data.backend,
      embedding_model: data.embedding_model,
      reranker: data.reranker,
    };
  }

  statsSync(): StorageStats {
    const data = this._callSync<any>("stats", []);
    return {
      total_memories: Number(data.total_memories ?? 0),
      active_memories: Number(data.active_memories ?? 0),
      expired_memories: Number(data.expired_memories ?? 0),
      vector_count: Number(data.vector_count ?? 0),
      categories: data.categories || {},
      storage_size_bytes: Number(data.storage_size_bytes ?? 0),
      db_path: data.db_path || "",
      backend: data.backend,
      embedding_model: data.embedding_model,
      reranker: data.reranker,
    };
  }

  // 7. Forget
  async forget(id: string, options: ForgetOptions = {}): Promise<boolean> {
    const args = [id];
    if (options.hard) args.push("--hard");
    const data = await this._call<any>("forget", args);
    return Boolean(data.success ?? true);
  }

  forgetSync(id: string, options: ForgetOptions = {}): boolean {
    const args = [id];
    if (options.hard) args.push("--hard");
    const data = this._callSync<any>("forget", args);
    return Boolean(data.success ?? true);
  }

  // 8. Clear
  async clear(options: ClearOptions = {}): Promise<boolean> {
    const args: string[] = [];
    if (options.soft) args.push("--soft");
    const data = await this._call<any>("clear", args);
    return Boolean(data.success ?? true);
  }

  clearSync(options: ClearOptions = {}): boolean {
    const args: string[] = [];
    if (options.soft) args.push("--soft");
    const data = this._callSync<any>("clear", args);
    return Boolean(data.success ?? true);
  }

  // 9. Conflicts
  async conflicts(options: ConflictsOptions = {}): Promise<MemoryConflict[]> {
    const args: string[] = [];
    if (options.threshold !== undefined) args.push("--threshold", String(options.threshold));
    if (options.limit !== undefined) args.push("-l", String(options.limit));
    if (options.autoResolve) args.push("--yes");

    const data = await this._call<any[]>("conflicts", args);
    return (data || []).map((item) => ({
      id_a: item.id_a || "",
      id_b: item.id_b || "",
      title_a: item.title_a || "",
      title_b: item.title_b || "",
      content_a: item.content_a || "",
      content_b: item.content_b || "",
      conflict_type: item.conflict_type || "",
      similarity: Number(item.similarity ?? 0),
      reasoning: item.reasoning || "",
    }));
  }

  conflictsSync(options: ConflictsOptions = {}): MemoryConflict[] {
    const args: string[] = [];
    if (options.threshold !== undefined) args.push("--threshold", String(options.threshold));
    if (options.limit !== undefined) args.push("-l", String(options.limit));
    if (options.autoResolve) args.push("--yes");

    const data = this._callSync<any[]>("conflicts", args);
    return (data || []).map((item) => ({
      id_a: item.id_a || "",
      id_b: item.id_b || "",
      title_a: item.title_a || "",
      title_b: item.title_b || "",
      content_a: item.content_a || "",
      content_b: item.content_b || "",
      conflict_type: item.conflict_type || "",
      similarity: Number(item.similarity ?? 0),
      reasoning: item.reasoning || "",
    }));
  }

  // 10. Links & Graph
  async link(sourceId: string, targetId: string, relation = "relates_to"): Promise<boolean> {
    const data = await this._call<any>("link", [sourceId, targetId, "-r", relation]);
    return Boolean(data.success ?? true);
  }

  linkSync(sourceId: string, targetId: string, relation = "relates_to"): boolean {
    const data = this._callSync<any>("link", [sourceId, targetId, "-r", relation]);
    return Boolean(data.success ?? true);
  }

  async unlink(sourceId: string, targetId: string, relation?: string): Promise<boolean> {
    const args = [sourceId, targetId];
    if (relation) args.push("-r", relation);
    const data = await this._call<any>("unlink", args);
    return Boolean(data.success ?? true);
  }

  unlinkSync(sourceId: string, targetId: string, relation?: string): boolean {
    const args = [sourceId, targetId];
    if (relation) args.push("-r", relation);
    const data = this._callSync<any>("unlink", args);
    return Boolean(data.success ?? true);
  }

  async related(id: string, options: { hops?: number } = {}): Promise<RelatedMemory[]> {
    const hops = options.hops ?? 2;
    const data = await this._call<any[]>("related", [id, "--hops", String(hops)]);
    return (data || []).map((item) => ({
      memory: item.memory || item,
      distance: Number(item.distance ?? 1),
      relation_path: item.relation_path || [],
      score: Number(item.score ?? 1.0),
    }));
  }

  relatedSync(id: string, options: { hops?: number } = {}): RelatedMemory[] {
    const hops = options.hops ?? 2;
    const data = this._callSync<any[]>("related", [id, "--hops", String(hops)]);
    return (data || []).map((item) => ({
      memory: item.memory || item,
      distance: Number(item.distance ?? 1),
      relation_path: item.relation_path || [],
      score: Number(item.score ?? 1.0),
    }));
  }

  async graph(options: { hops?: number; rootId?: string } = {}): Promise<any> {
    const args: string[] = [];
    if (options.hops !== undefined) args.push("--hops", String(options.hops));
    if (options.rootId) args.push("--root", options.rootId);
    return this._call<any>("graph", args);
  }

  graphSync(options: { hops?: number; rootId?: string } = {}): any {
    const args: string[] = [];
    if (options.hops !== undefined) args.push("--hops", String(options.hops));
    if (options.rootId) args.push("--root", options.rootId);
    return this._callSync<any>("graph", args);
  }

  async autolink(): Promise<any> {
    return this._call<any>("autolink", []);
  }

  autolinkSync(): any {
    return this._callSync<any>("autolink", []);
  }

  // 11. Maintenance
  async reindex(options: { yes?: boolean } = {}): Promise<boolean> {
    const args: string[] = [];
    if (options.yes) args.push("--yes");
    const data = await this._call<any>("reindex", args);
    return Boolean(data.success ?? true);
  }

  reindexSync(options: { yes?: boolean } = {}): boolean {
    const args: string[] = [];
    if (options.yes) args.push("--yes");
    const data = this._callSync<any>("reindex", args);
    return Boolean(data.success ?? true);
  }
}
