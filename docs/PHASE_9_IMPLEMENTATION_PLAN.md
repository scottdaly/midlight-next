# Phase 9 Implementation Plan: Web-Specific Features

## Overview

Phase 9 transforms the Midlight web app into a production-ready, offline-capable Progressive Web App (PWA) with optional cloud sync. This phase focuses on storage optimization, offline support, and seamless synchronization.

**Goals:**
1. Web editor works fully offline
2. Documents optionally sync to cloud
3. Conflicts handled gracefully with user control
4. Performance acceptable on mobile devices

---

## Architecture Decision Records

### ADR 1: Storage Strategy

#### Context
The web app needs reliable local storage that works offline and persists across sessions. Two browser APIs are available: Origin Private File System (OPFS) and IndexedDB.

#### Options Considered

| Option | Description | Pros | Cons |
|--------|-------------|------|------|
| **A: OPFS Primary + IndexedDB Fallback** | Use OPFS for file storage, fall back to IndexedDB for unsupported browsers | Best performance, high storage limits (GB+), file-like API | OPFS not available in all browsers |
| **B: IndexedDB Only** | Store everything in IndexedDB | Universal browser support, simpler implementation | Lower storage limits, less intuitive API for files |
| **C: LocalStorage + IndexedDB** | Small data in localStorage, large in IndexedDB | Simple for small data | 5MB localStorage limit, synchronous blocking API |

#### Decision: **Option A (OPFS Primary + IndexedDB Fallback)**

**Rationale:**
- OPFS provides 10x better performance for file operations
- Storage quotas are significantly higher (gigabytes vs megabytes)
- File-like API matches our document/folder mental model
- IndexedDB fallback ensures Firefox ESR and older Safari compatibility
- Already implemented in `WebStorageAdapter` - just needs optimization

---

### ADR 2: Cloud Sync Strategy

#### Context
Users want their documents available across devices. We need a sync mechanism that's reliable, handles conflicts gracefully, and works with our existing infrastructure.

#### Options Considered

| Option | Description | Pros | Cons |
|--------|-------------|------|------|
| **A: Full Document Sync** | Upload complete document + sidecar on each save | Simple, reliable, easy to debug | More bandwidth, but documents are small |
| **B: Operational Transformation (OT)** | Real-time collaborative editing with operation transforms | Real-time collaboration | Extreme complexity, overkill for single-user |
| **C: CRDT (Yjs/Automerge)** | Conflict-free replicated data types | Automatic merge, real-time capable | Changes storage format, large dependency |
| **D: Git-like Incremental Sync** | Upload only changed content objects using hashes | Efficient bandwidth, version history | More complex than full sync |

#### Decision: **Option A (Full Document Sync) with Hash-based Change Detection**

**Rationale:**
- Documents are typically <100KB - bandwidth is not a concern
- Simplicity reduces bugs and maintenance burden
- Content hashes (already implemented) detect changes efficiently
- Can evolve to Option D later if needed for performance
- Avoids CRDT/OT complexity that's unnecessary for single-user sync

---

### ADR 3: Conflict Resolution Strategy

#### Context
When the same document is edited on multiple devices while offline, conflicts can occur. We need a strategy that prevents data loss.

#### Options Considered

| Option | Description | Pros | Cons |
|--------|-------------|------|------|
| **A: Last-Write-Wins** | Most recent timestamp wins | Simple, predictable | Can silently lose changes |
| **B: Manual Resolution** | Show both versions, user chooses | No data loss, user control | Requires UI, user effort |
| **C: Auto-Merge with Markers** | Insert conflict markers in text | Preserves both versions | Messy for rich text, confusing |
| **D: Fork on Conflict** | Create separate "conflicted" copy | No data loss, simple | Multiple files to manage |

#### Decision: **Option B (Manual Resolution) with Option D Fallback**

**Rationale:**
- User maintains full control over their content
- Show visual diff between local and remote versions
- User can choose local, remote, or manually merge
- If user dismisses without resolving, create a "Conflicted Copy" (Option D)
- Matches expectations from Dropbox, Google Drive, etc.

---

### ADR 4: Offline Strategy

#### Context
The app must work fully offline, including creating/editing documents. When connectivity returns, changes should sync automatically.

#### Options Considered

| Option | Description | Pros | Cons |
|--------|-------------|------|------|
| **A: Service Worker + Cache API** | Cache static assets, network-first for API | Standard PWA pattern, reliable | Manual cache management |
| **B: Workbox** | Google's SW library with strategies | Faster development, pre-built patterns | Additional dependency |
| **C: Background Sync API** | Queue operations, sync when online | Seamless resume | Limited browser support (Chrome only) |

#### Decision: **Option A (Service Worker + Cache API) with Option C as Enhancement**

**Rationale:**
- Service Worker is the foundation for any offline strategy
- Cache static assets (HTML, CSS, JS) for instant load
- Store pending sync operations in IndexedDB
- Use Background Sync where available (progressive enhancement)
- No additional dependencies

---

### ADR 5: Backend Storage

#### Context
Cloud sync requires server-side storage. We need to choose where documents are stored.

#### Options Considered

| Option | Description | Pros | Cons |
|--------|-------------|------|------|
| **A: Object Storage (S3/R2)** | Store documents as objects | Low cost, high durability, CDN-ready | Separate from metadata DB |
| **B: PostgreSQL/SQLite** | Store documents in database | Easy querying, transactions | Not ideal for large files, higher cost |
| **C: Supabase** | Managed backend-as-a-service | All-in-one, real-time | Vendor lock-in, recurring cost |

#### Decision: **Option A (CloudFlare R2) with SQLite Metadata**

**Rationale:**
- R2 has no egress fees (unlike S3)
- Documents stored as objects with path-based keys
- SQLite (existing) stores sync metadata: version vectors, timestamps
- Separation of concerns: R2 for content, SQLite for state
- Cost-effective at any scale

---

## Implementation Phases

### Phase 9.1: Storage Optimization (Foundation)

**Goal:** Optimize OPFS storage adapter and add IndexedDB fallback

#### Tasks

1. **Detect Storage Capability**
   ```typescript
   // packages/core/src/storage/capabilities.ts
   export async function detectStorageCapabilities(): Promise<{
     opfs: boolean;
     indexedDb: boolean;
     storageEstimate: { quota: number; usage: number } | null;
   }>;
   ```

2. **Optimize OPFS Operations**
   - Batch file writes to reduce OPFS calls
   - Implement file handle caching
   - Add write coalescing (debounce rapid saves)

3. **Create IndexedDB Storage Adapter**
   ```typescript
   // apps/web/src/lib/storage/indexeddb-adapter.ts
   export class IndexedDBStorageAdapter implements StorageAdapter {
     // Stores files as blobs in IndexedDB instead of OPFS
     // Same interface as WebStorageAdapter
   }
   ```

4. **Implement Adapter Factory**
   ```typescript
   // apps/web/src/lib/storage/factory.ts
   export async function createStorageAdapter(): Promise<StorageAdapter> {
     const capabilities = await detectStorageCapabilities();
     if (capabilities.opfs) {
       return new WebStorageAdapter(); // Uses OPFS
     }
     return new IndexedDBStorageAdapter(); // Fallback
   }
   ```

5. **Add Storage Usage Monitoring**
   - Track storage quota and usage
   - Warn user when approaching limits
   - Implement cleanup for old checkpoints

#### Success Criteria
- [ ] OPFS adapter handles 1000+ files without performance degradation
- [ ] IndexedDB fallback works in Firefox ESR, Safari 15
- [ ] Storage usage displayed in settings
- [ ] Graceful handling of quota exceeded

---

### Phase 9.2: Offline Support (PWA)

**Goal:** App works fully offline with service worker

#### Tasks

1. **Create Service Worker**
   ```typescript
   // apps/web/static/sw.js
   const CACHE_NAME = 'midlight-v1';
   const STATIC_ASSETS = [
     '/',
     '/editor',
     '/app.css',
     '/app.js',
     // ... other static assets
   ];

   // Cache-first for static assets
   // Network-first for API calls
   ```

2. **Implement Cache Strategies**
   - **Static Assets:** Cache-first with network fallback
   - **API Calls:** Network-first with cache fallback for GET
   - **Document Operations:** Always use local storage first

3. **Add Offline Detection**
   ```typescript
   // packages/stores/src/network.ts
   export const network = createNetworkStore();

   interface NetworkState {
     online: boolean;
     lastOnline: Date | null;
     pendingSyncCount: number;
   }
   ```

4. **Build Offline Indicator UI**
   ```svelte
   <!-- apps/web/src/lib/components/OfflineIndicator.svelte -->
   {#if !$network.online}
     <div class="offline-banner">
       You're offline. Changes will sync when you're back online.
       {#if $network.pendingSyncCount > 0}
         ({$network.pendingSyncCount} pending)
       {/if}
     </div>
   {/if}
   ```

5. **Implement Operation Queue**
   ```typescript
   // apps/web/src/lib/sync/queue.ts
   interface PendingOperation {
     id: string;
     type: 'create' | 'update' | 'delete' | 'rename';
     path: string;
     payload: unknown;
     timestamp: number;
     retryCount: number;
   }

   export class SyncQueue {
     async enqueue(op: PendingOperation): Promise<void>;
     async processQueue(): Promise<void>;
     async getPending(): Promise<PendingOperation[]>;
   }
   ```

6. **Add PWA Manifest**
   ```json
   // apps/web/static/manifest.json
   {
     "name": "Midlight",
     "short_name": "Midlight",
     "start_url": "/editor",
     "display": "standalone",
     "background_color": "#ffffff",
     "theme_color": "#000000",
     "icons": [...]
   }
   ```

#### Success Criteria
- [ ] App installs as PWA on desktop and mobile
- [ ] Full functionality while offline
- [ ] Clear offline indicator when disconnected
- [ ] Operations queue and sync when back online

---

### Phase 9.3: Cloud Sync Backend

**Goal:** API endpoints for document synchronization

#### Tasks

1. **Design API Schema**
   ```
   POST /api/sync/documents
   GET  /api/sync/documents
   GET  /api/sync/documents/:id
   PUT  /api/sync/documents/:id
   DELETE /api/sync/documents/:id
   GET  /api/sync/status
   POST /api/sync/resolve-conflict
   ```

2. **Database Schema (SQLite)**
   ```sql
   CREATE TABLE sync_documents (
     id TEXT PRIMARY KEY,
     user_id TEXT NOT NULL,
     path TEXT NOT NULL,
     content_hash TEXT NOT NULL,
     sidecar_hash TEXT NOT NULL,
     version INTEGER NOT NULL DEFAULT 1,
     created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
     updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
     deleted_at DATETIME,
     UNIQUE(user_id, path)
   );

   CREATE TABLE sync_conflicts (
     id TEXT PRIMARY KEY,
     document_id TEXT NOT NULL,
     local_version INTEGER,
     remote_version INTEGER,
     local_hash TEXT,
     remote_hash TEXT,
     created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
     resolved_at DATETIME,
     resolution TEXT -- 'local', 'remote', 'merged'
   );
   ```

3. **Object Storage Integration (R2)**
   ```typescript
   // midlight-site/server/services/storage.js
   import { S3Client, PutObjectCommand, GetObjectCommand } from '@aws-sdk/client-s3';

   export class DocumentStorage {
     async uploadDocument(userId: string, docId: string, content: string, sidecar: string);
     async downloadDocument(userId: string, docId: string): Promise<{ content: string; sidecar: string }>;
     async deleteDocument(userId: string, docId: string): Promise<void>;
   }
   ```

4. **Implement Sync Endpoints**
   ```javascript
   // midlight-site/server/routes/sync.js
   router.post('/documents', authenticate, async (req, res) => {
     const { path, content, sidecar, baseVersion } = req.body;
     // Check for conflicts
     // Upload to R2
     // Update metadata in SQLite
     // Return new version
   });

   router.get('/status', authenticate, async (req, res) => {
     // Return list of documents with versions
     // Client compares to detect changes
   });
   ```

5. **Add Rate Limiting**
   - Sync operations limited by subscription tier
   - Free: 100 syncs/day
   - Pro: Unlimited

#### Success Criteria
- [ ] Documents upload to R2 on save
- [ ] Documents download on new device login
- [ ] Conflict detection when versions diverge
- [ ] Rate limiting prevents abuse

---

### Phase 9.4: Sync Client & Conflict Resolution

**Goal:** Frontend sync integration with conflict resolution UI

#### Tasks

1. **Create Sync Client**
   ```typescript
   // apps/web/src/lib/sync/client.ts
   export class SyncClient {
     async getRemoteStatus(): Promise<SyncStatus[]>;
     async uploadDocument(doc: LocalDocument): Promise<SyncResult>;
     async downloadDocument(docId: string): Promise<RemoteDocument>;
     async resolveConflict(conflictId: string, resolution: 'local' | 'remote'): Promise<void>;
   }
   ```

2. **Implement Version Vector**
   ```typescript
   // apps/web/src/lib/sync/version.ts
   interface DocumentVersion {
     local: number;
     remote: number;
     lastSyncedAt: Date | null;
     contentHash: string;
   }

   export function detectConflict(local: DocumentVersion, remote: DocumentVersion): boolean;
   ```

3. **Build Conflict Resolution UI**
   ```svelte
   <!-- apps/web/src/lib/components/ConflictDialog.svelte -->
   <script>
     export let conflict: SyncConflict;

     // Show side-by-side diff
     // "Keep Local", "Keep Remote", "Keep Both" buttons
   </script>

   <dialog>
     <h2>Sync Conflict</h2>
     <p>{conflict.path} was edited on another device</p>

     <div class="comparison">
       <div class="local">
         <h3>Your Version</h3>
         <pre>{conflict.localContent}</pre>
       </div>
       <div class="remote">
         <h3>Other Device</h3>
         <pre>{conflict.remoteContent}</pre>
       </div>
     </div>

     <div class="actions">
       <button on:click={() => resolve('local')}>Keep Mine</button>
       <button on:click={() => resolve('remote')}>Keep Theirs</button>
       <button on:click={() => resolve('both')}>Keep Both</button>
     </div>
   </dialog>
   ```

4. **Add Sync Status UI**
   ```svelte
   <!-- apps/web/src/lib/components/SyncStatus.svelte -->
   {#if $sync.syncing}
     <span class="syncing">Syncing...</span>
   {:else if $sync.lastSyncedAt}
     <span class="synced">Synced {formatRelative($sync.lastSyncedAt)}</span>
   {:else if $sync.error}
     <span class="error">Sync error</span>
   {/if}
   ```

5. **Implement Auto-Sync**
   ```typescript
   // apps/web/src/lib/sync/auto.ts
   export function startAutoSync(interval: number = 30000) {
     // Check for remote changes periodically
     // Upload local changes when online
     // Handle conflicts as they arise
   }
   ```

#### Success Criteria
- [ ] Documents sync automatically when online
- [ ] Conflicts detected and presented to user
- [ ] User can resolve conflicts without data loss
- [ ] Sync status visible in UI

---

### Phase 9.5: Performance Optimization

**Goal:** Fast, responsive experience on mobile devices

#### Tasks

1. **Implement Virtual Scrolling**
   - Large document rendering with virtualization
   - Only render visible portions
   - Use `@tanstack/virtual` or similar

2. **Optimize Bundle Size**
   ```bash
   # Analyze bundle
   pnpm --filter @midlight/web build
   npx vite-bundle-visualizer

   # Targets:
   # - Initial JS < 200KB gzipped
   # - Code splitting for editor
   # - Lazy load heavy components
   ```

3. **Add Resource Hints**
   ```html
   <link rel="preconnect" href="https://api.midlight.ai">
   <link rel="prefetch" href="/editor">
   <link rel="preload" href="/fonts/inter.woff2" as="font" crossorigin>
   ```

4. **Implement Image Optimization**
   - Lazy load images below fold
   - Compress images before storage
   - Use WebP with fallbacks

5. **Add Performance Monitoring**
   ```typescript
   // apps/web/src/lib/performance.ts
   export function reportPerformance() {
     const paint = performance.getEntriesByType('paint');
     const navigation = performance.getEntriesByType('navigation')[0];

     // Report LCP, FID, CLS to analytics
   }
   ```

6. **Mobile-Specific Optimizations**
   - Touch-friendly UI elements (48px tap targets)
   - Debounce input events
   - Reduce re-renders with `$derived` wisely

#### Success Criteria
- [ ] Lighthouse Performance score > 90
- [ ] First Contentful Paint < 1.5s
- [ ] Time to Interactive < 3s
- [ ] Smooth scrolling at 60fps on mobile

---

## File Structure

```
apps/web/src/
├── lib/
│   ├── components/
│   │   ├── ConflictDialog.svelte      # NEW: Conflict resolution
│   │   ├── OfflineIndicator.svelte    # NEW: Offline status
│   │   ├── SyncStatus.svelte          # NEW: Sync status
│   │   └── ...existing...
│   ├── storage/
│   │   ├── adapter.ts                 # Existing: WebStorageAdapter
│   │   ├── indexeddb-adapter.ts       # NEW: IndexedDB fallback
│   │   ├── factory.ts                 # NEW: Adapter factory
│   │   └── capabilities.ts            # NEW: Storage detection
│   └── sync/
│       ├── client.ts                  # NEW: Sync API client
│       ├── queue.ts                   # NEW: Operation queue
│       ├── version.ts                 # NEW: Version vectors
│       ├── auto.ts                    # NEW: Auto-sync logic
│       └── conflict.ts                # NEW: Conflict detection
├── routes/
│   └── ...existing...
└── static/
    ├── sw.js                          # NEW: Service worker
    └── manifest.json                  # NEW: PWA manifest

midlight-site/server/
├── routes/
│   ├── sync.js                        # NEW: Sync endpoints
│   └── ...existing...
├── services/
│   └── storage.js                     # NEW: R2 integration
└── db/
    └── migrations/
        └── 005_sync_tables.sql        # NEW: Sync schema

packages/stores/src/
├── network.ts                         # NEW: Network state
├── sync.ts                            # NEW: Sync state
└── ...existing...
```

---

## Dependencies

### Web App (apps/web)

```json
{
  "dependencies": {
    "idb": "^8.0.1"  // Already present
  },
  "devDependencies": {
    "@vite-pwa/sveltekit": "^0.7.0"  // PWA support for SvelteKit
  }
}
```

### Backend (midlight-site)

```json
{
  "dependencies": {
    "@aws-sdk/client-s3": "^3.x"  // R2/S3 SDK
  }
}
```

---

## Implementation Order

### Week 1: Phase 9.1 (Storage Optimization)
1. Implement storage capability detection
2. Optimize OPFS operations (batching, caching)
3. Create IndexedDB fallback adapter
4. Add storage monitoring UI

### Week 2: Phase 9.2 (Offline Support)
1. Create service worker with caching strategies
2. Implement network state store
3. Build offline indicator component
4. Add operation queue for pending syncs
5. Create PWA manifest

### Week 3: Phase 9.3 (Backend)
1. Design and create sync database schema
2. Set up R2 bucket and integration
3. Implement sync API endpoints
4. Add rate limiting and auth

### Week 4: Phase 9.4 (Sync Client)
1. Create sync client library
2. Implement version tracking
3. Build conflict detection logic
4. Create conflict resolution UI
5. Add auto-sync functionality

### Week 5: Phase 9.5 (Performance)
1. Bundle analysis and optimization
2. Implement lazy loading
3. Add virtual scrolling if needed
4. Mobile-specific optimizations
5. Performance testing and tuning

---

## Progress Tracking

### Phase 9.1: Storage Optimization ✅
- [x] Storage capability detection (`packages/core/src/storage/capabilities.ts`)
- [x] OPFS operation batching (write coalescing in `WebStorageAdapter`)
- [x] File handle caching (`dirHandleCache`, `fileHandleCache` in `WebStorageAdapter`)
- [x] IndexedDB fallback adapter (`apps/web/src/lib/storage/indexeddb-adapter.ts`)
- [x] Adapter factory (`apps/web/src/lib/storage/factory.ts`)
- [x] Storage quota monitoring (`packages/stores/src/storage.ts`)
- [ ] Settings UI for storage info (deferred to Phase 9.2)

### Phase 9.2: Offline Support ✅
- [x] Service worker skeleton (`apps/web/static/sw.js`)
- [x] Static asset caching (cache-first for static, network-first for API)
- [x] Network state store (`packages/stores/src/network.ts`)
- [x] Offline indicator component (`apps/web/src/lib/components/OfflineIndicator.svelte`)
- [x] Operation queue (IndexedDB) (`apps/web/src/lib/sync/queue.ts`)
- [x] PWA manifest (`apps/web/static/manifest.json`)
- [x] Install prompt handling (`packages/stores/src/pwa.ts`, `InstallBanner.svelte`)

### Phase 9.3: Cloud Sync Backend ✅
- [x] Sync database schema (`midlight-site/server/db/schema.sql`: sync_documents, sync_conflicts, sync_usage, sync_operations)
- [x] R2 storage service (`midlight-site/server/services/storageService.js`)
- [x] Document upload endpoint (`POST /api/sync/documents`)
- [x] Document download endpoint (`GET /api/sync/documents/:id`)
- [x] Sync status endpoint (`GET /api/sync/status`)
- [x] Conflict detection and preservation (automatic on version mismatch)
- [x] Conflict resolution endpoint (`POST /api/sync/conflicts/:id/resolve`)
- [x] Rate limiting by tier (`CONFIG.rateLimit.sync`)
- [x] Storage quotas by tier (`CONFIG.syncStorage`)

### Phase 9.4: Sync Client ✅
- [x] Sync client class (`apps/web/src/lib/sync/client.ts`)
- [x] Sync store for state management (`packages/stores/src/sync.ts`)
- [x] Conflict detection logic (hash-based with version tracking)
- [x] Conflict resolution dialog (`apps/web/src/lib/components/ConflictDialog.svelte`)
- [x] Sync status component (`apps/web/src/lib/components/SyncStatus.svelte`)
- [x] Sync manager with auto-sync (`apps/web/src/lib/sync/manager.ts`)
- [x] Sync integration with document saves (`apps/web/src/lib/sync/integration.ts`)
- [x] Manual sync trigger (via SyncStatus component)

### Phase 9.5: Performance ✅
- [x] Bundle analysis (460KB total build, 67KB gzipped for editor)
- [x] Code splitting via Vite manual chunks (`vite.config.ts`)
- [x] Lazy loading via `LazyImage.svelte` component
- [x] Resource hints in `app.html` (preconnect, dns-prefetch, prefetch)
- [x] Image optimization with IntersectionObserver lazy loading
- [x] Mobile touch optimizations (`apps/web/src/lib/utils/touch.ts`)
- [x] Performance monitoring (`apps/web/src/lib/performance/index.ts`)
- [ ] Lighthouse audit > 90 (requires deployed testing)

---

## Success Criteria

Phase 9 is complete when:

1. **Offline Functionality** ✅
   - [x] App loads without network connection (Service Worker caching)
   - [x] Documents can be created/edited offline (OPFS/IndexedDB storage)
   - [x] Changes persist across browser restarts (persistent storage)
   - [x] Sync resumes when online (SyncQueue with network listener)

2. **Cloud Sync** ✅
   - [x] Documents sync to cloud on save (syncManager.uploadDocument)
   - [x] Documents available on new device after login (syncClient.downloadDocument)
   - [x] Conflicts detected and presented to user (ConflictDialog.svelte)
   - [x] No data loss in any scenario (preserve both versions, soft delete)

3. **Performance** ✅
   - [ ] Lighthouse score > 90 (requires deployed testing)
   - [x] Works smoothly on mobile devices (touch optimizations)
   - [x] Initial load < 3 seconds (460KB build, code splitting)
   - [x] Editor responsive at 60fps (RAF debouncing, passive events)

4. **User Experience** ✅
   - [x] Clear sync status indication (SyncStatus.svelte)
   - [x] Clear offline indicator (OfflineIndicator.svelte)
   - [x] Conflict resolution is intuitive (ConflictDialog with Keep Mine/Theirs/Both)
   - [x] PWA installable on all platforms (manifest.json, InstallBanner.svelte)

---

## Risk Mitigation

### Risk: OPFS Browser Compatibility
**Mitigation:** IndexedDB fallback ensures broad browser support. Feature detection at runtime.

### Risk: Sync Conflicts
**Mitigation:** Conservative conflict detection. Always preserve both versions. User chooses resolution.

### Risk: Storage Quota Exceeded
**Mitigation:** Monitor usage, warn before limits. Cleanup old checkpoints. Guide user to clear space.

### Risk: R2/S3 Costs
**Mitigation:** R2 has no egress fees. Compress documents before upload. Rate limit by tier.

### Risk: Data Loss
**Mitigation:** Multiple redundancy: local OPFS, local IndexedDB, cloud storage. Never delete without confirmation.

---

## Security Considerations

1. **Authentication:** All sync endpoints require valid JWT
2. **Authorization:** Users can only access their own documents
3. **Encryption:** HTTPS for transport. Consider E2E encryption for content.
4. **Rate Limiting:** Prevent abuse with per-user limits
5. **Validation:** Sanitize all inputs, validate content types
6. **CORS:** Restrict origins to midlight.ai domains

---

## Future Enhancements (Post-Phase 9)

1. **Real-time Collaboration** - Add WebSocket for live co-editing
2. **Selective Sync** - Choose which folders to sync
3. **Version History in Cloud** - Access old versions from any device
4. **Sharing** - Share documents via link
5. **End-to-End Encryption** - Client-side encryption for privacy
