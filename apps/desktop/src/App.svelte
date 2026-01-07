<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { fileSystem, activeFile, settings, ui, isRightPanelOpen, ai, auth } from '@midlight/stores';
  import { TauriStorageAdapter } from '$lib/tauri';
  import { createTauriLLMClient } from '$lib/llm';
  import { authClient, startAuthEventListeners, stopAuthEventListeners } from '$lib/auth';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import TabBar from '$lib/components/TabBar.svelte';
  import Toolbar from '$lib/components/Toolbar.svelte';
  import Editor from '$lib/components/Editor.svelte';
  import RightSidebar from '$lib/components/RightSidebar.svelte';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import SettingsModal from '$lib/components/SettingsModal.svelte';
  import AuthModal from '$lib/components/AuthModal.svelte';
  import UpgradeModal from '$lib/components/UpgradeModal.svelte';

  let initialized = $state(false);
  let sidebarWidth = $state(240);
  let rightSidebarWidth = $state(320);
  let showAuthModal = $state(false);
  let showUpgradeModal = $state(false);

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
    // Apply theme
    const unsubscribe = settings.subscribe(($settings) => {
      const root = document.documentElement;

      // Remove all theme classes first
      root.classList.remove(...ALL_THEMES);

      if ($settings.theme === 'system') {
        // System theme - just use dark class for dark mode preference
        const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        root.classList.toggle('dark', prefersDark);
      } else {
        // Apply the specific theme class
        root.classList.add($settings.theme);
      }
    });

    // Initialize Tauri storage adapter
    const adapter = new TauriStorageAdapter();
    fileSystem.setStorageAdapter(adapter);

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
        // Initialize auth first (attempt silent refresh)
        await authClient.init();
        await startAuthEventListeners();

        // Load default workspace
        const defaultWorkspace = await invoke<string>('get_default_workspace');
        await fileSystem.loadDir(defaultWorkspace);
        // Set workspace root for agent mode
        ai.setWorkspaceRoot(defaultWorkspace);
      } catch (error) {
        console.error('Failed to initialize:', error);
      }
      initialized = true;
    })();

    // Listen for menu actions
    const handleMenuAction = () => openFolder();
    window.addEventListener('midlight:open-workspace', handleMenuAction);

    return () => {
      unsubscribe();
      stopAuthEventListeners();
      window.removeEventListener('midlight:open-workspace', handleMenuAction);
    };
  });

  onDestroy(() => {
    stopAuthEventListeners();
  });

  async function openFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: 'Open Workspace',
    });

    if (selected && typeof selected === 'string') {
      await fileSystem.loadDir(selected);
    }
  }

  function handleGlobalKeydown(e: KeyboardEvent) {
    // Cmd+, or Ctrl+, to open settings
    if ((e.metaKey || e.ctrlKey) && e.key === ',') {
      e.preventDefault();
      settings.open();
    }
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
