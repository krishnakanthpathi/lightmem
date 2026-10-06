import { LightMem } from "./client";
export { LightMem };
export {
  getBinaryPath,
  isNativeBinary,
  ensureBinaryDownloaded,
  executeLmem,
  executeLmemAsync,
} from "./binary";
export type {
  Memory,
  MemoryType,
  MemoryStatus,
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
  LinkTarget,
} from "./models";

export const VERSION = "0.2.4";
export default LightMem;
