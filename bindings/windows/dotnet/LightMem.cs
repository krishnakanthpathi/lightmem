using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace LightMem
{
    public record MemoryRecord(
        [property: JsonPropertyName("id")] string Id,
        [property: JsonPropertyName("category")] string Category,
        [property: JsonPropertyName("title")] string Title,
        [property: JsonPropertyName("content")] string Content,
        [property: JsonPropertyName("tags")] List<string> Tags,
        [property: JsonPropertyName("confidence")] float Confidence,
        [property: JsonPropertyName("status")] string Status,
        [property: JsonPropertyName("provenance")] string Provenance,
        [property: JsonPropertyName("created_at")] string CreatedAt,
        [property: JsonPropertyName("updated_at")] string UpdatedAt,
        [property: JsonPropertyName("expired_at")] string? ExpiredAt
    );

    public record ScoredMemory(
        [property: JsonPropertyName("memory")] MemoryRecord Memory,
        [property: JsonPropertyName("score")] float Score
    );

    public record AnswerResult(
        [property: JsonPropertyName("answer")] string Answer,
        [property: JsonPropertyName("selected_memory")] MemoryRecord? SelectedMemory,
        [property: JsonPropertyName("confidence")] float Confidence,
        [property: JsonPropertyName("reranker_used")] string RerankerUsed
    );

    public record CategoryCount(
        [property: JsonPropertyName("category")] string Category,
        [property: JsonPropertyName("count")] ulong Count
    );

    public record StorageStats(
        [property: JsonPropertyName("total_memories")] ulong TotalMemories,
        [property: JsonPropertyName("active_memories")] ulong ActiveMemories,
        [property: JsonPropertyName("expired_memories")] ulong ExpiredMemories,
        [property: JsonPropertyName("total_vectors")] ulong TotalVectors,
        [property: JsonPropertyName("by_category")] List<CategoryCount> ByCategory
    );

    public class LightMemClient : IDisposable
    {
        private const string LibraryName = "lightmem";

        private IntPtr _handle;
        private bool _disposed;

        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "lmem_open")]
        private static extern IntPtr NativeOpen([MarshalAs(UnmanagedType.LPUTF8Str)] string? dbPath, int globalDb);

        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "lmem_close")]
        private static extern void NativeClose(IntPtr handle);

        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "lmem_free_string")]
        private static extern void NativeFreeString(IntPtr ptr);

        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "lmem_get_last_error")]
        private static extern IntPtr NativeGetLastError();

        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "lmem_remember")]
        private static extern IntPtr NativeRemember(
            IntPtr handle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string content,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? category,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? title,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? tagsCsv,
            float confidence
        );

        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "lmem_recall")]
        private static extern IntPtr NativeRecall(
            IntPtr handle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string query,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? category,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? asOf,
            uint limit,
            float minSimilarity
        );

        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "lmem_answer")]
        private static extern IntPtr NativeAnswer(
            IntPtr handle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string question,
            int needle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? category,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? asOf,
            uint limit
        );

        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "lmem_list")]
        private static extern IntPtr NativeList(
            IntPtr handle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? category,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? status,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? asOf,
            uint limit
        );

        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "lmem_forget")]
        private static extern int NativeForget(
            IntPtr handle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string id,
            int hard
        );

        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "lmem_stats")]
        private static extern IntPtr NativeStats(IntPtr handle);

        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "lmem_export_okf")]
        private static extern IntPtr NativeExportOkf(IntPtr handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string? outputPath);

        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "lmem_import_file")]
        private static extern long NativeImportFile(IntPtr handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string filePath);

        [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "lmem_import_file_needle")]
        private static extern long NativeImportFileNeedle(IntPtr handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string filePath, int needle);

        public LightMemClient(string? dbPath = null, bool globalDb = false)
        {
            _handle = NativeOpen(dbPath, globalDb ? 1 : 0);
            if (_handle == IntPtr.Zero)
            {
                throw new InvalidOperationException($"Failed to open LightMem database: {GetLastError()}");
            }
        }

        private static string GetLastError()
        {
            var ptr = NativeGetLastError();
            if (ptr == IntPtr.Zero) return "Unknown native error";
            var err = Marshal.PtrToStringUTF8(ptr) ?? "Unknown native error";
            NativeFreeString(ptr);
            return err;
        }

        private static string PtrToStringAndFree(IntPtr ptr)
        {
            if (ptr == IntPtr.Zero) return string.Empty;
            try
            {
                return Marshal.PtrToStringUTF8(ptr) ?? string.Empty;
            }
            finally
            {
                NativeFreeString(ptr);
            }
        }

        public MemoryRecord Remember(
            string content,
            string? category = null,
            string? title = null,
            IEnumerable<string>? tags = null,
            float? confidence = null
        )
        {
            EnsureNotDisposed();
            var tagsCsv = tags != null ? string.Join(",", tags) : null;
            var confVal = confidence ?? -1.0f;

            var ptr = NativeRemember(_handle, content, category, title, tagsCsv, confVal);
            if (ptr == IntPtr.Zero)
            {
                throw new InvalidOperationException($"Remember failed: {GetLastError()}");
            }

            var json = PtrToStringAndFree(ptr);
            return JsonSerializer.Deserialize<MemoryRecord>(json)!;
        }

        public List<ScoredMemory> Recall(
            string query,
            string? category = null,
            string? asOf = null,
            uint limit = 10,
            float? minSimilarity = null
        )
        {
            EnsureNotDisposed();
            var minSim = minSimilarity ?? -1.0f;

            var ptr = NativeRecall(_handle, query, category, asOf, limit, minSim);
            if (ptr == IntPtr.Zero)
            {
                throw new InvalidOperationException($"Recall failed: {GetLastError()}");
            }

            var json = PtrToStringAndFree(ptr);
            return JsonSerializer.Deserialize<List<ScoredMemory>>(json) ?? new List<ScoredMemory>();
        }

        public AnswerResult Answer(
            string question,
            bool needle = false,
            string? category = null,
            string? asOf = null,
            uint limit = 5
        )
        {
            EnsureNotDisposed();
            var ptr = NativeAnswer(_handle, question, needle ? 1 : 0, category, asOf, limit);
            if (ptr == IntPtr.Zero)
            {
                throw new InvalidOperationException($"Answer failed: {GetLastError()}");
            }

            var json = PtrToStringAndFree(ptr);
            return JsonSerializer.Deserialize<AnswerResult>(json)!;
        }

        public List<MemoryRecord> List(
            string? category = null,
            string? status = null,
            string? asOf = null,
            uint limit = 50
        )
        {
            EnsureNotDisposed();
            var ptr = NativeList(_handle, category, status, asOf, limit);
            if (ptr == IntPtr.Zero)
            {
                throw new InvalidOperationException($"List failed: {GetLastError()}");
            }

            var json = PtrToStringAndFree(ptr);
            return JsonSerializer.Deserialize<List<MemoryRecord>>(json) ?? new List<MemoryRecord>();
        }

        public bool Forget(string id, bool hard = false)
        {
            EnsureNotDisposed();
            return NativeForget(_handle, id, hard ? 1 : 0) == 1;
        }

        public StorageStats Stats()
        {
            EnsureNotDisposed();
            var ptr = NativeStats(_handle);
            if (ptr == IntPtr.Zero)
            {
                throw new InvalidOperationException($"Stats failed: {GetLastError()}");
            }

            var json = PtrToStringAndFree(ptr);
            return JsonSerializer.Deserialize<StorageStats>(json)!;
        }

        public string ExportOkf(string? outputPath = null)
        {
            EnsureNotDisposed();
            var ptr = NativeExportOkf(_handle, outputPath);
            if (ptr == IntPtr.Zero)
            {
                throw new InvalidOperationException($"ExportOkf failed: {GetLastError()}");
            }

            return PtrToStringAndFree(ptr);
        }

        public long ImportFile(string filePath, bool needle = false)
        {
            EnsureNotDisposed();
            var count = needle
                ? NativeImportFileNeedle(_handle, filePath, 1)
                : NativeImportFile(_handle, filePath);
            if (count < 0)
            {
                throw new InvalidOperationException($"ImportFile failed: {GetLastError()}");
            }

            return count;
        }

        private void EnsureNotDisposed()
        {
            if (_disposed || _handle == IntPtr.Zero)
            {
                throw new ObjectDisposedException(nameof(LightMemClient));
            }
        }

        public void Dispose()
        {
            if (!_disposed)
            {
                if (_handle != IntPtr.Zero)
                {
                    NativeClose(_handle);
                    _handle = IntPtr.Zero;
                }
                _disposed = true;
            }
            GC.SuppressFinalize(this);
        }

        ~LightMemClient()
        {
            Dispose();
        }
    }
}
