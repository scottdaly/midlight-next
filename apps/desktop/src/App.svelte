<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { get } from 'svelte/store';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { open } from '@tauri-apps/plugin-dialog';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { fileSystem, activeFile, settings, ui, isRightPanelOpen, ai, auth, recoveryStore, clearAllWalWrites, toastStore, fileWatcherStore, shortcuts } from '@midlight/stores';
  import type { Shortcut } from '@midlight/stores';
  import { TauriStorageAdapter } from '$lib/tauri';
  import { createTauriLLMClient } from '$lib/llm';
  import { authClient, startAuthEventListeners, stopAuthEventListeners } from '$lib/auth';
  import { subscriptionClient } from '$lib/subscription';
  import { recoveryClient } from '$lib/recovery';
  import { fileWatcherClient } from '$lib/fileWatcher';
  import { errorReporter } from '$lib/errorReporter';
  import { updatesClient } from '$lib/updates';
  import { windowStateClient } from '$lib/windowState';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import TabBar from '$lib/components/TabBar.svelte';
  import Toolbar from '$lib/components/Toolbar.svelte';
  import Editor from '$lib/components/Editor.svelte';
  import RightSidebar from '$lib/components/RightSidebar.svelte';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import SettingsModal from '$lib/components/SettingsModal.svelte';
  import AuthModal from '$lib/components/AuthModal.svelte';
  import UpgradeModal from '$lib/components/UpgradeModal.svelte';
  import RecoveryDialog from '$lib/components/RecoveryDialog.svelte';
  import ExternalChangeDialog from '$lib/components/ExternalChangeDialog.svelte';
  import ToastContainer from '$lib/components/ToastContainer.svelte';
  import UpdateDialog from '$lib/components/UpdateDialog.svelte';
  import DocxImportDialog from '$lib/components/DocxImportDialog.svelte';
  import type { DocxImportResult } from '$lib/import';

  let initialized = $state(false);
  let sidebarWidth = $state(240);
  let rightSidebarWidth = $state(320);
  let showAuthModal = $state(false);
  let showUpgradeModal = $state(false);
  let showDocxImportDialog = $state(false);
  let fileWatcherUnlisten: (() => void) | null = null;
  let currentWatchedWorkspace: string | null = null;
  let menuUnlisteners: UnlistenFn[] = [];

  const ALL_THEMES = ['light', 'dark', 'midnight', 'sepia', 'forest', 'cyberpunk', 'coffee'];
  const DARK_THEMES = ['dark', 'midnight', 'forest', 'cyberpunk'];

  // Windows-specific setup
  $effect(() => {
    if (navigator.userAgent.includes('Windows')) {
      const win = getCurrentWindow();
      win.setDecorations(false);
    }
  });

  onMount(() => {
    // System theme media query
    const systemThemeQuery = window.matchMedia('(prefers-color-scheme: dark)');

    // Function to apply theme based on settings and system preference
    function applyTheme(settingsTheme: string) {
      const root = document.documentElement;

      // Remove all theme classes first
      root.classList.remove(...ALL_THEMES);

      if (settingsTheme === 'system') {
        // System theme - apply dark class based on system preference
        const prefersDark = systemThemeQuery.matches;
        root.classList.toggle('dark', prefersDark);
      } else {
        // Apply the specific theme class
        root.classList.add(settingsTheme);
      }
    }

    // Listen for system theme changes
    let currentTheme = get(settings).theme;
    function handleSystemThemeChange() {
      // Only re-apply if using system theme
      if (currentTheme === 'system') {
        applyTheme('system');
      }
    }
    systemThemeQuery.addEventListener('change', handleSystemThemeChange);

    // Apply theme when settings change
    const unsubscribe = settings.subscribe(($settings) => {
      currentTheme = $settings.theme;
      applyTheme($settings.theme);
    });

    // Initialize Tauri storage adapter
    const adapter = new TauriStorageAdapter();
    fileSystem.setStorageAdapter(adapter);

    // Register app-level keyboard shortcuts
    registerShortcuts();

    // Initialize LLM client with auth token
    const llmClient = createTauriLLMClient(async () => {
      // Get access token from auth service
      return await authClient.getAccessToken();
    });
    ai.setLLMClient(llmClient);

    // Initialize tool executor for agent mode
    ai.setToolExecutor(async (workspaceRoot, toolName, args) => {
      return await invoke('agent_execute_tool', {
        request: {
          workspaceRoot,
          toolName,
          arguments: args,
        },
      });
    });

    // Refresh file tree and reload document when agent modifies files
    ai.setOnFileChange((path: string) => {
      fileSystem.refresh();
      fileSystem.reloadDocument(path);
    });

    // Initialize auth and workspace
    (async () => {
      try {
        // Initialize window state persistence (restore window position/size)
        await windowStateClient.init();

        // Initialize error reporting from settings
        const settingsState = get(settings);
        await errorReporter.setEnabled(settingsState.errorReportingEnabled);

        // Initialize auth first (attempt silent refresh)
        await authClient.init();
        await startAuthEventListeners();

        // Load default workspace
        const defaultWorkspace = await invoke<string>('get_default_workspace');
        await fileSystem.loadDir(defaultWorkspace);
        // Set workspace root for agent mode
        ai.setWorkspaceRoot(defaultWorkspace);

        // Check for recovery files after workspace loads
        await checkForRecovery(defaultWorkspace);

        // Start file watcher for external changes
        await startFileWatcher(defaultWorkspace);

        // Initialize auto-updates (checks for updates after 10s delay)
        await updatesClient.init();
      } catch (error) {
        console.error('Failed to initialize:', error);
        // Report initialization error (if reporting is enabled)
        await errorReporter.reportError('unknown', error, { phase: 'initialization' });
      }
      initialized = true;
    })();

    // Listen for native macOS menu events (emitted from Rust)
    setupMenuListeners();

    // Refresh subscription data when window regains focus
    // This catches post-checkout updates when user returns from Stripe
    const handleWindowFocus = async () => {
      const authState = get(auth);
      if (authState.isAuthenticated) {
        try {
          await subscriptionClient.refresh();
        } catch (error) {
          console.error('Failed to refresh subscription on focus:', error);
        }
      }
    };
    window.addEventListener('focus', handleWindowFocus);

    return () => {
      unsubscribe();
      stopAuthEventListeners();
      window.removeEventListener('focus', handleWindowFocus);
      systemThemeQuery.removeEventListener('change', handleSystemThemeChange);
    };
  });

  onDestroy(() => {
    stopAuthEventListeners();
    // Clear any pending WAL writes on app close
    clearAllWalWrites();
    // Stop file watcher
    stopFileWatcher();
    // Clean up updates client
    updatesClient.destroy();
    // Clean up window state client
    windowStateClient.destroy();
    // Clean up menu listeners
    menuUnlisteners.forEach((unlisten) => unlisten());
    menuUnlisteners = [];
  });

  // Check for recovery files and prompt user
  async function checkForRecovery(workspaceRoot: string) {
    try {
      recoveryStore.startCheck();
      const recoveries = await recoveryClient.checkForRecovery(workspaceRoot);

      if (recoveries.length > 0) {
        // Transform to store format
        const storeRecoveries = recoveries.map((r) => ({
          fileKey: r.fileKey,
          walContent: r.walContent,
          walTime: r.walTime.toISOString(),
          workspaceRoot: r.workspaceRoot,
        }));
        recoveryStore.setPendingRecoveries(storeRecoveries);
      } else {
        recoveryStore.setPendingRecoveries([]);
      }
    } catch (error) {
      console.error('Failed to check for recovery files:', error);
      recoveryStore.checkFailed(error instanceof Error ? error.message : String(error));
    }
  }

  // Start file watcher for a workspace
  async function startFileWatcher(workspaceRoot: string) {
    try {
      // Stop existing watcher if any
      await stopFileWatcher();

      // Start new watcher
      await fileWatcherClient.start(workspaceRoot);
      fileWatcherStore.startWatching(workspaceRoot);
      currentWatchedWorkspace = workspaceRoot;

      // Listen for file change events
      fileWatcherUnlisten = await fileWatcherClient.onFileChange((change) => {
        // Don't show dialog for files not in open tabs (only care about currently open files)
        const fs = get(fileSystem);
        const openFiles = fs.openFiles || [];
        const isOpenFile = openFiles.some((f) => f.path === change.fileKey);

        if (isOpenFile || change.changeType === 'delete') {
          fileWatcherStore.addChange(change);
        }
      });

      console.log('File watcher started for:', workspaceRoot);
    } catch (error) {
      console.error('Failed to start file watcher:', error);
      fileWatcherStore.setError(error instanceof Error ? error.message : String(error));
    }
  }

  // Stop file watcher
  async function stopFileWatcher() {
    if (fileWatcherUnlisten) {
      fileWatcherUnlisten();
      fileWatcherUnlisten = null;
    }

    if (currentWatchedWorkspace) {
      try {
        await fileWatcherClient.stop(currentWatchedWorkspace);
      } catch (error) {
        console.error('Failed to stop file watcher:', error);
      }
      fileWatcherStore.stopWatching();
      currentWatchedWorkspace = null;
    }
  }

  // Handle reloading a file from disk
  async function handleReloadFile(fileKey: string) {
    await fileSystem.reloadDocument(fileKey);
    toastStore.info(`Reloaded "${fileKey.split('/').pop()}"`);
  }

  // Handle closing a deleted file
  function handleCloseFile(fileKey: string) {
    fileSystem.closeFile(fileKey);
    toastStore.info(`Closed deleted file "${fileKey.split('/').pop()}"`);
  }

  // Handle refreshing file tree for created files
  function handleRefreshFileTree() {
    fileSystem.refresh();
  }

  // Handle DOCX import completion - create new document with imported content
  async function handleDocxImportComplete(result: DocxImportResult, fileName: string) {
    const fs = get(fileSystem);
    if (!fs.rootDir) return;

    try {
      // Create a new file with the base name from the DOCX
      const baseName = fileName.replace(/\.docx$/i, '');
      await fileSystem.createFile(fs.rootDir, baseName);

      // Set the imported content after the file is created and active
      // Small delay to ensure the file is fully loaded
      setTimeout(() => {
        fileSystem.setEditorContent(result.tiptapJson);
        fileSystem.setIsDirty(true); // Mark as dirty so user can save
        toastStore.success(`Imported "${baseName}" successfully`);
      }, 100);
    } catch (error) {
      console.error('Failed to create document from DOCX import:', error);
      toastStore.error('Failed to create document from import');
    }
  }

  // Handle recovering a file's content
  async function handleRecoverFile(fileKey: string, content: string) {
    const fs = get(fileSystem);
    if (!fs.rootDir) return;

    // Try to parse the content as JSON (Tiptap document)
    try {
      const parsedContent = JSON.parse(content);

      // Set the file as active (this will load its current content)
      await fileSystem.setActiveFile(fileKey);

      // Immediately override with recovered content
      fileSystem.setEditorContent(parsedContent);

      // Mark as dirty since we've restored unsaved changes
      fileSystem.setIsDirty(true);

      // Get the file name for the toast
      const fileName = fileKey.split('/').pop() || fileKey;
      toastStore.success(`Recovered unsaved changes to "${fileName}"`);
    } catch (error) {
      console.error('Failed to apply recovered content:', error);
      toastStore.error('Failed to recover document');
    }
  }

  // Set up listeners for native macOS menu events
  async function setupMenuListeners() {
    // Only set up on macOS - Windows uses custom menu component
    if (!navigator.userAgent.includes('Mac')) return;

    const listeners = await Promise.all([
      // App menu
      listen('menu:settings', () => settings.open()),
      listen('menu:check-for-updates', () => updatesClient.checkForUpdates(true)),

      // File menu
      listen('menu:new-document', async () => {
        const fs = get(fileSystem);
        if (fs.rootDir) {
          try {
            await fileSystem.createFile(fs.rootDir, 'Untitled');
          } catch (error) {
            console.error('Failed to create document:', error);
          }
        }
      }),
      listen('menu:open-workspace', () => openFolder()),
      listen('menu:import-docx', () => {
        showDocxImportDialog = true;
      }),
      listen('menu:save', async () => {
        const file = get(activeFile);
        if (file) {
          await fileSystem.save('manual');
          toastStore.success('Document saved');
        }
      }),
      listen('menu:export-docx', async () => {
        const file = get(activeFile);
        if (file) {
          // Trigger export - implementation depends on existing export flow
          window.dispatchEvent(new CustomEvent('midlight:export-docx'));
        }
      }),
      listen('menu:export-pdf', async () => {
        const file = get(activeFile);
        if (file) {
          window.dispatchEvent(new CustomEvent('midlight:export-pdf'));
        }
      }),
      listen('menu:close-tab', () => {
        const file = get(activeFile);
        if (file) {
          fileSystem.closeFile(file.path);
        }
      }),

      // Edit menu (find)
      listen('menu:find', () => {
        // Trigger find in editor - emit event for editor component
        window.dispatchEvent(new CustomEvent('midlight:find'));
      }),

      // View menu
      listen('menu:toggle-ai-panel', () => ui.togglePanelMode('chat')),
      listen('menu:toggle-versions-panel', () => ui.togglePanelMode('versions')),

      // Help menu
      listen('menu:documentation', async () => {
        const { openExternal } = await import('$lib/system');
        await openExternal('https://midlight.ai/docs');
      }),
      listen('menu:report-issue', async () => {
        const { openExternal } = await import('$lib/system');
        await openExternal('https://midlight.ai/support');
      }),
    ]);

    menuUnlisteners = listeners;
  }

  async function openFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: 'Open Workspace',
    });

    if (selected && typeof selected === 'string') {
      // Clear any pending WAL writes for old workspace
      clearAllWalWrites();
      // Clear pending external changes for old workspace
      fileWatcherStore.clearAllChanges();
      // Stop old file watcher
      await stopFileWatcher();
      // Load new workspace
      await fileSystem.loadDir(selected);
      // Update agent workspace root
      ai.setWorkspaceRoot(selected);
      // Check for recovery in new workspace
      await checkForRecovery(selected);
      // Start file watcher for new workspace
      await startFileWatcher(selected);
    }
  }

  function handleGlobalKeydown(e: KeyboardEvent) {
    // Let the shortcut store handle it first
    if (shortcuts.handleKeyDown(e)) {
      return;
    }
  }

  // Register app-level shortcuts
  function registerShortcuts() {
    const appShortcuts: Shortcut[] = [
      {
        id: 'settings',
        keys: 'mod+,',
        description: 'Open settings',
        category: 'view',
        action: () => settings.open(),
      },
      {
        id: 'save',
        keys: 'mod+s',
        description: 'Save document',
        category: 'file',
        action: async () => {
          const file = get(activeFile);
          if (file) {
            await fileSystem.save('manual');
            toastStore.success('Document saved');
          }
        },
        when: () => get(activeFile) !== null,
      },
      {
        id: 'open-workspace',
        keys: 'mod+o',
        description: 'Open workspace',
        category: 'file',
        action: () => openFolder(),
      },
      {
        id: 'close-tab',
        keys: 'mod+w',
        description: 'Close current tab',
        category: 'file',
        action: () => {
          const file = get(activeFile);
          if (file) {
            fileSystem.closeFile(file.path);
          }
        },
        when: () => get(activeFile) !== null,
      },
      {
        id: 'toggle-ai-panel',
        keys: 'mod+shift+a',
        description: 'Toggle AI panel',
        category: 'ai',
        action: () => ui.togglePanelMode('chat'),
      },
      {
        id: 'toggle-versions-panel',
        keys: 'mod+shift+v',
        description: 'Toggle versions panel',
        category: 'view',
        action: () => ui.togglePanelMode('versions'),
      },
      {
        id: 'new-document',
        keys: 'mod+n',
        description: 'New document',
        category: 'file',
        action: async () => {
          const fs = get(fileSystem);
          if (fs.rootDir) {
            try {
              await fileSystem.createFile(fs.rootDir, 'Untitled');
            } catch (error) {
              console.error('Failed to create document:', error);
            }
          }
        },
        when: () => get(fileSystem).rootDir !== null,
      },
    ];

    shortcuts.registerAll(appShortcuts);
  }

  // Function to open auth modal (can be called from other components)
  export function openAuthModal() {
    showAuthModal = true;
  }
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

<div class="h-screen flex flex-col overflow-hidden bg-background text-foreground">
  <TitleBar />
  {#if !initialized}
    <div class="flex items-center justify-center h-full">
      <div class="text-center space-y-4">
        <div class="w-8 h-8 border-4 border-primary border-t-transparent rounded-full animate-spin mx-auto"></div>
        <p class="text-muted-foreground">Loading...</p>
      </div>
    </div>
  {:else if !$fileSystem.rootDir}
    <!-- Loading default workspace or error fallback -->
    <div class="flex items-center justify-center h-full">
      <div class="text-center space-y-6 max-w-md">
        <div class="w-8 h-8 border-4 border-primary border-t-transparent rounded-full animate-spin mx-auto"></div>
        <p class="text-muted-foreground">Loading workspace...</p>
        <p class="text-sm text-muted-foreground">
          Your documents are stored in ~/Documents/Midlight-docs
        </p>
      </div>
    </div>
  {:else}
    <!-- Main editor layout - TabBar and Toolbar span full width -->
    <div class="flex flex-col flex-1 overflow-hidden">
      <!-- Tab Bar (full width) -->
      <TabBar onOpenFolder={openFolder} />

      <!-- Toolbar (full width) -->
      <Toolbar />

      <!-- Content area with sidebars -->
      <div class="flex flex-1 overflow-hidden">
        <!-- Left Sidebar -->
        <aside
          class="flex-shrink-0 border-r border-border bg-card overflow-hidden"
          style="width: {sidebarWidth}px"
        >
          <Sidebar />
        </aside>

        <!-- Main Editor Area -->
        <main class="flex-1 min-w-0 overflow-hidden">
          <Editor />
        </main>

        <!-- Right Sidebar -->
        {#if $isRightPanelOpen}
          <aside
            class="flex-shrink-0 border-l border-border bg-card overflow-hidden"
            style="width: {rightSidebarWidth}px"
          >
            <RightSidebar mode={$ui.rightPanelMode} onOpenAuth={() => showAuthModal = true} />
          </aside>
        {/if}
      </div>
    </div>
  {/if}
</div>

<!-- Settings Modal -->
<SettingsModal
  open={$settings.isOpen}
  onClose={() => settings.close()}
  onOpenAuthModal={() => showAuthModal = true}
  onOpenUpgradeModal={() => showUpgradeModal = true}
/>

<!-- Auth Modal -->
<AuthModal
  open={showAuthModal}
  onClose={() => showAuthModal = false}
/>

<!-- Upgrade Modal -->
<UpgradeModal
  open={showUpgradeModal}
  onClose={() => showUpgradeModal = false}
/>

<!-- Recovery Dialog -->
{#if $fileSystem.rootDir}
  <RecoveryDialog
    workspaceRoot={$fileSystem.rootDir}
    onRecoverFile={handleRecoverFile}
  />
{/if}

<!-- External Change Dialog -->
<ExternalChangeDialog
  onReloadFile={handleReloadFile}
  onCloseFile={handleCloseFile}
  onRefreshFileTree={handleRefreshFileTree}
/>

<!-- Toast Notifications -->
<ToastContainer />

<!-- Update Dialog -->
<UpdateDialog />

<!-- DOCX Import Dialog -->
{#if $fileSystem.rootDir}
  <DocxImportDialog
    open={showDocxImportDialog}
    onClose={() => showDocxImportDialog = false}
    workspaceRoot={$fileSystem.rootDir}
    onComplete={handleDocxImportComplete}
  />
{/if}
