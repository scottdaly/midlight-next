# Phase 9 Remediation Plan

## Overview

The Phase 9 audit revealed that while all code modules were implemented, many were **not wired up or integrated** into the running application. This plan addresses the critical integration gaps, memory leaks, security vulnerabilities, and other issues discovered during the audit.

**Priority Order:**
1. **Critical (P0):** Integration issues that prevent features from working at all
2. **Security (P1):** Vulnerabilities that could be exploited
3. **Important (P2):** Memory leaks, race conditions, performance issues
4. **Minor (P3):** Code quality and optimization improvements

---

## P0: Critical Integration Issues

### 1. Sync Integration Never Initialized

**Problem:** `apps/web/src/lib/sync/integration.ts` exports `initSyncIntegration()` but it's never called. The entire sync system is dormant.

**Files to modify:**
- `apps/web/src/routes/editor/+layout.svelte`

**Solution:**
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { initSyncIntegration, cleanupSyncIntegration } from '$lib/sync/integration';

  onMount(() => {
    initSyncIntegration();
    return () => cleanupSyncIntegration();
  });
</script>
```

**Status:** [ ] Not started

---

### 2. Network Store Never Initialized

**Problem:** `packages/stores/src/network.ts` exports `init()` and `cleanup()` but they're never called. Online/offline detection doesn't work.

**Files to modify:**
- `apps/web/src/routes/+layout.svelte` (root layout)

**Solution:**
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { network } from '@midlight/stores';

  onMount(() => {
    network.init();
    return () => network.cleanup();
  });
</script>
```

**Status:** [ ] Not started

---

### 3. PWA Store Never Initialized

**Problem:** `packages/stores/src/pwa.ts` exports `init()` and `cleanup()` but they're never called. Install prompts won't work.

**Files to modify:**
- `apps/web/src/routes/+layout.svelte` (root layout)

**Solution:**
```svelte
<script lang="ts">
  import { pwa } from '@midlight/stores';

  onMount(() => {
    pwa.init();
    return () => pwa.cleanup();
  });
</script>
```

**Status:** [ ] Not started

---

### 4. Storage Factory Bypassed

**Problem:** `apps/web/src/lib/storage/factory.ts` creates the correct adapter with fallback, but App.svelte directly instantiates `WebStorageAdapter` instead of using the factory.

**Files to modify:**
- `apps/web/src/routes/editor/+layout.svelte` or root layout

**Solution:**
```typescript
import { createStorageAdapter } from '$lib/storage/factory';

onMount(async () => {
  const adapter = await createStorageAdapter();
  fileSystem.setStorageAdapter(adapter);
});
```

**Status:** [ ] Not started

---

### 5. UI Components Not Mounted

**Problem:** These components exist but are never rendered:
- `OfflineIndicator.svelte`
- `InstallBanner.svelte`
- `ConflictDialog.svelte`
- `SyncStatus.svelte`

**Files to modify:**
- `apps/web/src/routes/+layout.svelte` - Add OfflineIndicator and InstallBanner
- `apps/web/src/routes/editor/+layout.svelte` - Add ConflictDialog (at editor level)
- `apps/web/src/routes/editor/+page.svelte` - Add SyncStatus to header/toolbar

**Solution for root layout:**
```svelte
<script lang="ts">
  import OfflineIndicator from '$lib/components/OfflineIndicator.svelte';
  import InstallBanner from '$lib/components/InstallBanner.svelte';
</script>

<OfflineIndicator />
<InstallBanner />
<slot />
```

**Solution for editor layout:**
```svelte
<script lang="ts">
  import ConflictDialog from '$lib/components/ConflictDialog.svelte';
  import { sync } from '@midlight/stores';
</script>

{#if $sync.hasConflicts}
  <ConflictDialog conflicts={$sync.conflicts} />
{/if}
<slot />
```

**Solution for editor page (add to toolbar/header):**
```svelte
<script lang="ts">
  import SyncStatus from '$lib/components/SyncStatus.svelte';
</script>

<!-- In header area -->
<SyncStatus />
```

**Status:** [ ] Not started

---

### 6. Missing PWA Icons

**Problem:** `manifest.json` references icons that don't exist:
- `/icons/icon-72x72.png`
- `/icons/icon-96x96.png`
- `/icons/icon-128x128.png`
- `/icons/icon-144x144.png`
- `/icons/icon-152x152.png`
- `/icons/icon-192x192.png`
- `/icons/icon-384x384.png`
- `/icons/icon-512x512.png`

**Files to create:**
- `apps/web/static/icons/` directory with all icon sizes

**Solution:**
1. Create a 512x512 source icon
2. Generate all sizes using ImageMagick or similar:
   ```bash
   for size in 72 96 128 144 152 192 384 512; do
     convert icon-512.png -resize ${size}x${size} icon-${size}x${size}.png
   done
   ```

**Status:** [ ] Not started

---

## P1: Security Vulnerabilities

### 7. Path Traversal in Backend Sync

**Problem:** `midlight-site/server/routes/sync.js` doesn't validate `path` parameter, allowing potential path traversal attacks.

**File to modify:**
- `midlight-site/server/routes/sync.js`

**Solution:**
```javascript
// Add at top of file
const path = require('path');

function validatePath(inputPath) {
  // Reject absolute paths
  if (path.isAbsolute(inputPath)) {
    return { valid: false, error: 'Absolute paths not allowed' };
  }

  // Normalize and check for traversal
  const normalized = path.normalize(inputPath);
  if (normalized.startsWith('..') || normalized.includes('/../') || normalized.includes('\\..\\')) {
    return { valid: false, error: 'Path traversal not allowed' };
  }

  // Check for null bytes
  if (inputPath.includes('\0')) {
    return { valid: false, error: 'Invalid path characters' };
  }

  // Limit path length
  if (inputPath.length > 1000) {
    return { valid: false, error: 'Path too long' };
  }

  return { valid: true, normalized };
}

// Use in routes:
router.post('/documents', async (req, res) => {
  const pathValidation = validatePath(req.body.path);
  if (!pathValidation.valid) {
    return res.status(400).json({ error: pathValidation.error });
  }
  // ... rest of handler
});
```

**Status:** [ ] Not started

---

### 8. Missing CSRF Protection on Sync Routes

**Problem:** Sync routes don't have CSRF protection despite handling mutations.

**File to modify:**
- `midlight-site/server/routes/sync.js`

**Solution:**
```javascript
// sync.js should use the CSRF middleware already in server/index.js
// Verify sync routes are mounted AFTER csrf middleware
```

**Status:** [ ] Not started

---

### 9. Sidecar Content Not Sanitized

**Problem:** Sidecar JSON from clients is stored without validation, could contain malicious content.

**File to modify:**
- `midlight-site/server/routes/sync.js`

**Solution:**
```javascript
function validateSidecar(sidecar) {
  // Parse JSON if string
  const data = typeof sidecar === 'string' ? JSON.parse(sidecar) : sidecar;

  // Validate required structure
  if (typeof data !== 'object' || data === null) {
    throw new Error('Sidecar must be an object');
  }

  // Limit total size
  if (JSON.stringify(data).length > 1024 * 1024) { // 1MB
    throw new Error('Sidecar too large');
  }

  // Strip any potentially dangerous fields
  delete data.__proto__;
  delete data.constructor;
  delete data.prototype;

  return data;
}
```

**Status:** [ ] Not started

---

### 10. Document Enumeration Prevention

**Problem:** `/api/sync/status` returns all document paths for a user, potentially exposing document structure.

**File to modify:**
- `midlight-site/server/routes/sync.js`

**Solution:**
```javascript
// Return only hashes and IDs, not full paths
router.get('/status', authenticate, async (req, res) => {
  const documents = await db.all(`
    SELECT id, content_hash, sidecar_hash, version, updated_at
    FROM sync_documents
    WHERE user_id = ? AND deleted_at IS NULL
  `, [req.user.id]);

  // Client already knows their paths, can match by hash
  res.json({ documents });
});
```

**Status:** [ ] Not started

---

### 11. Rate Limit Per-Route

**Problem:** Rate limiting is global, not per-endpoint. Should have stricter limits on sync operations.

**File to modify:**
- `midlight-site/server/routes/sync.js`

**Solution:**
```javascript
const rateLimit = require('express-rate-limit');

const syncLimiter = rateLimit({
  windowMs: 15 * 60 * 1000, // 15 minutes
  max: 100, // limit each user to 100 sync requests per window
  keyGenerator: (req) => req.user?.id || req.ip,
  message: { error: 'Too many sync requests, please try again later' }
});

router.use(syncLimiter);
```

**Status:** [ ] Not started

---

## P2: Memory Leaks & Race Conditions

### 12. Memory Leak in SyncQueue

**Problem:** `apps/web/src/lib/sync/queue.ts` adds `online` listener in constructor but never removes it.

**File to modify:**
- `apps/web/src/lib/sync/queue.ts`

**Solution:**
```typescript
export class SyncQueue {
  private onlineHandler: () => void;

  constructor() {
    this.onlineHandler = () => this.processQueue();
    window.addEventListener('online', this.onlineHandler);
  }

  // Add cleanup method
  cleanup(): void {
    window.removeEventListener('online', this.onlineHandler);
  }
}
```

Also update sync manager to call cleanup:
```typescript
// apps/web/src/lib/sync/manager.ts
export function cleanupSyncManager(): void {
  queue.cleanup();
  // ... other cleanup
}
```

**Status:** [ ] Not started

---

### 13. Memory Leak in Network Store

**Problem:** Network store's `init()` adds event listeners but `cleanup()` doesn't properly track them.

**File to modify:**
- `packages/stores/src/network.ts`

**Solution:**
```typescript
let onlineHandler: (() => void) | null = null;
let offlineHandler: (() => void) | null = null;

export function init(): void {
  onlineHandler = () => {
    state.online = true;
    state.lastOnline = new Date();
  };
  offlineHandler = () => {
    state.online = false;
  };

  window.addEventListener('online', onlineHandler);
  window.addEventListener('offline', offlineHandler);

  // Set initial state
  state.online = navigator.onLine;
}

export function cleanup(): void {
  if (onlineHandler) {
    window.removeEventListener('online', onlineHandler);
    onlineHandler = null;
  }
  if (offlineHandler) {
    window.removeEventListener('offline', offlineHandler);
    offlineHandler = null;
  }
}
```

**Status:** [ ] Not started

---

### 14. Race Condition in Storage Adapter Init

**Problem:** Multiple components might call `createStorageAdapter()` before the first completes, creating multiple adapters.

**File to modify:**
- `apps/web/src/lib/storage/factory.ts`

**Solution:**
```typescript
let adapterPromise: Promise<StorageAdapter> | null = null;
let cachedAdapter: StorageAdapter | null = null;

export async function createStorageAdapter(): Promise<StorageAdapter> {
  // Return cached if available
  if (cachedAdapter) {
    return cachedAdapter;
  }

  // Return in-flight promise if initializing
  if (adapterPromise) {
    return adapterPromise;
  }

  // Start initialization
  adapterPromise = initializeAdapter();
  try {
    cachedAdapter = await adapterPromise;
    return cachedAdapter;
  } finally {
    adapterPromise = null;
  }
}

async function initializeAdapter(): Promise<StorageAdapter> {
  const capabilities = await detectStorageCapabilities();
  if (capabilities.opfs) {
    return new WebStorageAdapter();
  }
  return new IndexedDBStorageAdapter();
}
```

**Status:** [ ] Not started

---

### 15. No Request Timeout in Sync Client

**Problem:** `apps/web/src/lib/sync/client.ts` fetch calls have no timeout, could hang indefinitely.

**File to modify:**
- `apps/web/src/lib/sync/client.ts`

**Solution:**
```typescript
async function fetchWithTimeout(
  url: string,
  options: RequestInit,
  timeout = 30000
): Promise<Response> {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), timeout);

  try {
    const response = await fetch(url, {
      ...options,
      signal: controller.signal
    });
    return response;
  } finally {
    clearTimeout(timeoutId);
  }
}

// Use in all fetch calls
async getRemoteStatus(): Promise<SyncStatus[]> {
  const response = await fetchWithTimeout(
    `${this.baseUrl}/sync/status`,
    { headers: this.getHeaders() },
    15000 // 15 second timeout
  );
  // ...
}
```

**Status:** [ ] Not started

---

### 16. Empty triggerSync in Sync Store

**Problem:** `packages/stores/src/sync.ts` has `triggerSync()` method that doesn't do anything.

**File to modify:**
- `packages/stores/src/sync.ts`

**Solution:**
```typescript
export function triggerSync(): void {
  // Import sync manager
  import('$lib/sync/manager').then(({ syncManager }) => {
    syncManager.syncAll();
  });
}
```

Or wire it up through dependency injection when initializing the store.

**Status:** [ ] Not started

---

## P3: Minor Issues & Optimizations

### 17. Inefficient IndexedDB Adapter

**Problem:** `IndexedDBStorageAdapter` opens a new database connection for each operation.

**File to modify:**
- `apps/web/src/lib/storage/indexeddb-adapter.ts`

**Solution:**
```typescript
let dbPromise: Promise<IDBPDatabase> | null = null;

function getDB(): Promise<IDBPDatabase> {
  if (!dbPromise) {
    dbPromise = openDB('midlight-storage', 1, {
      upgrade(db) {
        if (!db.objectStoreNames.contains('files')) {
          db.createObjectStore('files', { keyPath: 'path' });
        }
      }
    });
  }
  return dbPromise;
}

// Use in all methods
async readFile(path: string): Promise<string | null> {
  const db = await getDB();
  const file = await db.get('files', path);
  return file?.content ?? null;
}
```

**Status:** [ ] Not started

---

### 18. Static Service Worker Cache Version

**Problem:** `apps/web/static/sw.js` uses hardcoded `CACHE_NAME = 'midlight-v1'`. Cache won't invalidate on deploys.

**File to modify:**
- `apps/web/static/sw.js`
- Build configuration to inject version

**Solution:**
```javascript
// Option 1: Use build timestamp
const CACHE_NAME = `midlight-${BUILD_VERSION}`; // Injected at build time

// Option 2: Use content hash in vite.config.ts
// Configure Vite to generate versioned sw.js
```

For Vite, add to `vite.config.ts`:
```typescript
import { VitePWA } from 'vite-plugin-pwa';

export default defineConfig({
  plugins: [
    VitePWA({
      registerType: 'autoUpdate',
      // This generates a versioned service worker
    })
  ]
});
```

**Status:** [ ] Not started

---

### 19. No Content Size Limits on Sync

**Problem:** Client can upload arbitrarily large documents to sync endpoints.

**Files to modify:**
- `midlight-site/server/routes/sync.js`

**Solution:**
```javascript
// Add before routes
const MAX_CONTENT_SIZE = 10 * 1024 * 1024; // 10MB
const MAX_SIDECAR_SIZE = 1 * 1024 * 1024; // 1MB

router.post('/documents', async (req, res) => {
  const { content, sidecar } = req.body;

  if (content && content.length > MAX_CONTENT_SIZE) {
    return res.status(413).json({ error: 'Document too large' });
  }

  if (sidecar && sidecar.length > MAX_SIDECAR_SIZE) {
    return res.status(413).json({ error: 'Sidecar too large' });
  }

  // ... rest of handler
});
```

**Status:** [ ] Not started

---

### 20. Add Retry Logic to Sync Queue

**Problem:** `SyncQueue.processQueue()` doesn't retry failed operations.

**File to modify:**
- `apps/web/src/lib/sync/queue.ts`

**Solution:**
```typescript
interface PendingOperation {
  id: string;
  type: 'create' | 'update' | 'delete' | 'rename';
  path: string;
  payload: unknown;
  timestamp: number;
  retryCount: number;
  lastError?: string;
}

const MAX_RETRIES = 3;
const RETRY_DELAY_MS = 5000;

async processQueue(): Promise<void> {
  const operations = await this.getPending();

  for (const op of operations) {
    if (op.retryCount >= MAX_RETRIES) {
      console.error(`Giving up on operation ${op.id} after ${MAX_RETRIES} retries`);
      await this.markFailed(op.id);
      continue;
    }

    try {
      await this.executeOperation(op);
      await this.remove(op.id);
    } catch (error) {
      op.retryCount++;
      op.lastError = error.message;
      await this.update(op);

      // Exponential backoff
      await new Promise(r => setTimeout(r, RETRY_DELAY_MS * Math.pow(2, op.retryCount)));
    }
  }
}
```

**Status:** [ ] Not started

---

## Implementation Order

### Phase 1: Make It Work (Critical Integration)
1. [ ] Initialize network store in root layout
2. [ ] Initialize PWA store in root layout
3. [ ] Use storage factory instead of direct instantiation
4. [ ] Initialize sync integration in editor layout
5. [ ] Mount OfflineIndicator and InstallBanner in root layout
6. [ ] Mount ConflictDialog in editor layout
7. [ ] Add SyncStatus to editor toolbar
8. [ ] Generate and add PWA icons

### Phase 2: Make It Secure
9. [ ] Add path validation to backend sync routes
10. [ ] Verify CSRF protection on sync routes
11. [ ] Add sidecar sanitization
12. [ ] Implement per-route rate limiting
13. [ ] Add content size limits

### Phase 3: Make It Stable
14. [ ] Fix SyncQueue memory leak
15. [ ] Fix network store memory leak
16. [ ] Fix storage adapter race condition
17. [ ] Add fetch timeout to sync client
18. [ ] Implement triggerSync properly

### Phase 4: Make It Better
19. [ ] Optimize IndexedDB connection handling
20. [ ] Add versioned service worker caching
21. [ ] Implement sync queue retry logic

---

## Testing Checklist

After remediation, verify:

### Integration Tests
- [ ] App loads offline after visiting once
- [ ] Offline indicator appears when disconnected
- [ ] Install banner appears on supported browsers
- [ ] Sync status shows correct state
- [ ] Conflicts are detected and dialog appears
- [ ] PWA installs successfully on desktop
- [ ] PWA installs successfully on mobile

### Security Tests
- [ ] Path traversal attempts return 400 error
- [ ] Oversized documents are rejected
- [ ] Malformed sidecar JSON is rejected
- [ ] Rate limiting kicks in after threshold
- [ ] CSRF token required for sync mutations

### Memory/Performance Tests
- [ ] No memory growth after repeated sync operations
- [ ] No memory growth during online/offline cycling
- [ ] Storage adapter initializes only once
- [ ] IndexedDB fallback works on Firefox

---

## Notes

- The root cause of most issues is that Phase 9 focused on **building modules** but not on **wiring them together**
- All the code is correct individually; it just needs to be connected
- The security issues in the backend should be fixed before deploying sync to production
- Consider adding integration tests to prevent regression
