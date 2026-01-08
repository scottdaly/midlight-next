<script lang="ts">
  import { settings, fileSystem, exportStore, activeFile } from '@midlight/stores';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { exportClient, type TiptapDocument } from '$lib/export';

  let openMenu: string | null = $state(null);
  let menuRef: HTMLDivElement | null = $state(null);

  const appWindow = getCurrentWindow();

  function toggleMenu(name: string) {
    if (openMenu === name) {
      openMenu = null;
    } else {
      openMenu = name;
    }
  }

  function closeMenu() {
    openMenu = null;
  }

  function handleClickOutside(event: MouseEvent) {
    if (openMenu && menuRef && !menuRef.contains(event.target as Node)) {
      closeMenu();
    }
  }

  // Handle actions
  async function handleAction(action: string) {
    closeMenu();
    switch (action) {
      case 'open-workspace':
        // Trigger file open in App (via event or store)
        // For now, dispatch event compatible with App.svelte
        // But App.svelte passes openFolder to TabBar...
        // Let's use invoke directly or trigger a global event
        // App.svelte doesn't listen to global events for opening folders currently except via TabBar callback.
        // We might need to expose a store action or modify App.svelte.
        // For now, let's just log or try to access the open function if possible.
        // Actually, we can dispatch a window event that App.svelte listens to, or better, move openFolder logic to a shared place?
        // Let's just emit an event 'midlight:open-workspace' and update App.svelte to listen.
        window.dispatchEvent(new CustomEvent('midlight:open-workspace'));
        break;
      case 'settings':
        settings.open();
        break;
      case 'quit':
        await appWindow.close();
        break;
      case 'reload':
        window.location.reload();
        break;
      case 'minimize':
        await appWindow.minimize();
        break;
      // Theme
      case 'theme-light': settings.setTheme('light'); break;
      case 'theme-dark': settings.setTheme('dark'); break;
      case 'theme-midnight': settings.setTheme('midnight'); break;
      case 'theme-sepia': settings.setTheme('sepia'); break;
      case 'theme-forest': settings.setTheme('forest'); break;
      case 'theme-cyberpunk': settings.setTheme('cyberpunk'); break;
      case 'theme-coffee': settings.setTheme('coffee'); break;
      case 'theme-system': settings.setTheme('system'); break;
      // Import
      case 'import-obsidian':
      case 'import-notion':
      case 'import-docx':
        window.dispatchEvent(new CustomEvent('midlight:open-import'));
        break;
      // Export
      case 'export-pdf':
        await handleExportPdf();
        break;
      case 'export-docx':
        await handleExportDocx();
        break;
      default:
        console.log('Action not implemented:', action);
    }
  }

  async function handleExportPdf() {
    try {
      await exportClient.exportToPdf();
    } catch (e) {
      console.error('PDF export failed:', e);
    }
  }

  async function handleExportDocx() {
    // Get the active file and content
    const file = $activeFile;
    const fsState = $fileSystem;

    if (!file) {
      console.warn('No active file to export');
      return;
    }

    // Get the editor content from fileSystem state (not from FileNode)
    const content = fsState.editorContent;
    if (!content) {
      console.warn('No document content to export');
      return;
    }

    // Get document name without extension
    const docName = file.name.replace(/\.(md|midlight)$/, '') || 'document';

    // Start export
    exportStore.startExport('docx');

    try {
      const result = await exportClient.export(
        content,
        docName,
        'docx',
        (progress) => {
          exportStore.updateProgress(progress);
        }
      );

      if (result.success) {
        exportStore.completeExport();
      } else {
        exportStore.failExport(result.error || 'Export failed');
      }
    } catch (e) {
      exportStore.failExport(e instanceof Error ? e.message : String(e));
    }
  }
</script>

<svelte:window onclick={handleClickOutside} />

<div bind:this={menuRef} class="flex items-center gap-0.5 ml-2 -my-1">
  <!-- File Menu -->
  <div class="relative">
    <button
      onclick={() => toggleMenu('file')}
      class="px-2.5 py-1 text-sm text-muted-foreground hover:bg-accent hover:text-accent-foreground rounded transition-colors {openMenu === 'file' ? 'bg-accent text-accent-foreground' : ''}"
    >
      File
    </button>
    {#if openMenu === 'file'}
      <div class="absolute top-full left-0 mt-1 bg-popover border border-border rounded-md shadow-lg z-50 py-1 min-w-[200px]">
        <button onclick={() => handleAction('open-workspace')} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Open Workspace...</span>
          <span class="text-xs text-muted-foreground">Ctrl+O</span>
        </button>
        <div class="h-px bg-border my-1"></div>
        <div class="px-3 py-1.5 text-xs text-muted-foreground font-medium">Import</div>
        <button onclick={() => handleAction('import-obsidian')} class="w-full px-3 py-1.5 pl-6 text-left hover:bg-accent text-sm">
          From Obsidian Vault...
        </button>
        <button onclick={() => handleAction('import-notion')} class="w-full px-3 py-1.5 pl-6 text-left hover:bg-accent text-sm">
          From Notion Export...
        </button>
        <button onclick={() => handleAction('import-docx')} class="w-full px-3 py-1.5 pl-6 text-left hover:bg-accent text-sm">
          From DOCX File...
        </button>
        <div class="h-px bg-border my-1"></div>
        <div class="px-3 py-1.5 text-xs text-muted-foreground font-medium">Export</div>
        <button onclick={() => handleAction('export-pdf')} class="w-full px-3 py-1.5 pl-6 text-left hover:bg-accent text-sm">
          To PDF...
        </button>
        <button onclick={() => handleAction('export-docx')} class="w-full px-3 py-1.5 pl-6 text-left hover:bg-accent text-sm">
          To DOCX...
        </button>
        <div class="h-px bg-border my-1"></div>
        <button onclick={() => handleAction('settings')} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Settings</span>
          <span class="text-xs text-muted-foreground">Ctrl+,</span>
        </button>
        <div class="h-px bg-border my-1"></div>
        <button onclick={() => handleAction('quit')} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm">
          Quit
        </button>
      </div>
    {/if}
  </div>

  <!-- Edit Menu (Basic web actions usually handled by OS/Browser, but we can try execCommand) -->
  <div class="relative">
    <button
      onclick={() => toggleMenu('edit')}
      class="px-2.5 py-1 text-sm text-muted-foreground hover:bg-accent hover:text-accent-foreground rounded transition-colors {openMenu === 'edit' ? 'bg-accent text-accent-foreground' : ''}"
    >
      Edit
    </button>
    {#if openMenu === 'edit'}
      <div class="absolute top-full left-0 mt-1 bg-popover border border-border rounded-md shadow-lg z-50 py-1 min-w-[150px]">
        <button onclick={() => { document.execCommand('undo'); closeMenu(); }} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Undo</span>
          <span class="text-xs text-muted-foreground">Ctrl+Z</span>
        </button>
        <button onclick={() => { document.execCommand('redo'); closeMenu(); }} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Redo</span>
          <span class="text-xs text-muted-foreground">Ctrl+Y</span>
        </button>
        <div class="h-px bg-border my-1"></div>
        <button onclick={() => { document.execCommand('cut'); closeMenu(); }} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Cut</span>
          <span class="text-xs text-muted-foreground">Ctrl+X</span>
        </button>
        <button onclick={() => { document.execCommand('copy'); closeMenu(); }} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Copy</span>
          <span class="text-xs text-muted-foreground">Ctrl+C</span>
        </button>
        <button onclick={() => { document.execCommand('paste'); closeMenu(); }} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Paste</span>
          <span class="text-xs text-muted-foreground">Ctrl+V</span>
        </button>
        <div class="h-px bg-border my-1"></div>
        <button onclick={() => { document.execCommand('selectAll'); closeMenu(); }} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Select All</span>
          <span class="text-xs text-muted-foreground">Ctrl+A</span>
        </button>
      </div>
    {/if}
  </div>

  <!-- View Menu -->
  <div class="relative">
    <button
      onclick={() => toggleMenu('view')}
      class="px-2.5 py-1 text-sm text-muted-foreground hover:bg-accent hover:text-accent-foreground rounded transition-colors {openMenu === 'view' ? 'bg-accent text-accent-foreground' : ''}"
    >
      View
    </button>
    {#if openMenu === 'view'}
      <div class="absolute top-full left-0 mt-1 bg-popover border border-border rounded-md shadow-lg z-50 py-1 min-w-[150px]">
        <button onclick={() => handleAction('reload')} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Reload</span>
          <span class="text-xs text-muted-foreground">Ctrl+R</span>
        </button>
        <div class="h-px bg-border my-1"></div>
        <div class="px-3 py-1.5 text-xs text-muted-foreground font-medium">Theme</div>
        <button onclick={() => handleAction('theme-light')} class="w-full px-3 py-1.5 pl-6 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Light</span>
          {#if $settings.theme === 'light'}<span class="text-xs">✓</span>{/if}
        </button>
        <button onclick={() => handleAction('theme-dark')} class="w-full px-3 py-1.5 pl-6 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Dark</span>
          {#if $settings.theme === 'dark'}<span class="text-xs">✓</span>{/if}
        </button>
        <button onclick={() => handleAction('theme-midnight')} class="w-full px-3 py-1.5 pl-6 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Midnight</span>
          {#if $settings.theme === 'midnight'}<span class="text-xs">✓</span>{/if}
        </button>
        <button onclick={() => handleAction('theme-sepia')} class="w-full px-3 py-1.5 pl-6 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Sepia</span>
          {#if $settings.theme === 'sepia'}<span class="text-xs">✓</span>{/if}
        </button>
        <button onclick={() => handleAction('theme-forest')} class="w-full px-3 py-1.5 pl-6 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Forest</span>
          {#if $settings.theme === 'forest'}<span class="text-xs">✓</span>{/if}
        </button>
        <button onclick={() => handleAction('theme-cyberpunk')} class="w-full px-3 py-1.5 pl-6 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Cyberpunk</span>
          {#if $settings.theme === 'cyberpunk'}<span class="text-xs">✓</span>{/if}
        </button>
        <button onclick={() => handleAction('theme-coffee')} class="w-full px-3 py-1.5 pl-6 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>Coffee</span>
          {#if $settings.theme === 'coffee'}<span class="text-xs">✓</span>{/if}
        </button>
        <div class="h-px bg-border my-1"></div>
        <button onclick={() => handleAction('theme-system')} class="w-full px-3 py-1.5 pl-6 text-left hover:bg-accent text-sm flex items-center justify-between">
          <span>System</span>
          {#if $settings.theme === 'system'}<span class="text-xs">✓</span>{/if}
        </button>
      </div>
    {/if}
  </div>

  <!-- Window Menu -->
  <div class="relative">
    <button
      onclick={() => toggleMenu('window')}
      class="px-2.5 py-1 text-sm text-muted-foreground hover:bg-accent hover:text-accent-foreground rounded transition-colors {openMenu === 'window' ? 'bg-accent text-accent-foreground' : ''}"
    >
      Window
    </button>
    {#if openMenu === 'window'}
      <div class="absolute top-full left-0 mt-1 bg-popover border border-border rounded-md shadow-lg z-50 py-1 min-w-[150px]">
        <button onclick={() => handleAction('minimize')} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm">
          Minimize
        </button>
        <button onclick={() => handleAction('quit')} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm">
          Close
        </button>
      </div>
    {/if}
  </div>

  <!-- Help Menu -->
  <div class="relative">
    <button
      onclick={() => toggleMenu('help')}
      class="px-2.5 py-1 text-sm text-muted-foreground hover:bg-accent hover:text-accent-foreground rounded transition-colors {openMenu === 'help' ? 'bg-accent text-accent-foreground' : ''}"
    >
      Help
    </button>
    {#if openMenu === 'help'}
      <div class="absolute top-full left-0 mt-1 bg-popover border border-border rounded-md shadow-lg z-50 py-1 min-w-[150px]">
        <button onclick={() => { open('https://electronjs.org'); closeMenu(); }} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm">
          Learn More
        </button>
      </div>
    {/if}
  </div>
</div>
