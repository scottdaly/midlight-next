<script lang="ts">
  import { fileSystem } from '@midlight/stores';
  import type { FileNode } from '@midlight/core/types';

  let newFileName = $state('');
  let isCreatingFile = $state(false);

  async function handleFileClick(file: FileNode) {
    if (file.type === 'directory') {
      // Toggle directory expansion (TODO)
      return;
    }
    await fileSystem.openFile(file);
  }

  async function createNewFile() {
    if (!newFileName.trim()) return;

    const fileName = newFileName.endsWith('.md') ? newFileName : `${newFileName}.md`;
    const path = `/${fileName}`;

    // Create empty file
    const adapter = (fileSystem as any).storageAdapter;
    if (adapter) {
      await adapter.createFile(path, `# ${newFileName.replace('.md', '')}\n\n`);
      await fileSystem.refresh();

      // Open the new file
      const newFile = $fileSystem.files.find(f => f.path === path);
      if (newFile) {
        await fileSystem.openFile(newFile);
      }
    }

    newFileName = '';
    isCreatingFile = false;
  }

  function getFileIcon(file: FileNode): string {
    if (file.type === 'directory') return '📁';
    switch (file.category) {
      case 'native': return '📄';
      case 'compatible': return '📝';
      case 'importable': return '📥';
      case 'viewable': return '🖼️';
      default: return '📎';
    }
  }
</script>

<div class="h-full flex flex-col">
  <!-- Header -->
  <div class="h-10 flex items-center justify-between px-3 border-b border-border">
    <span class="text-sm font-medium">Documents</span>
    <button
      onclick={() => isCreatingFile = true}
      class="p-1 hover:bg-accent rounded text-muted-foreground hover:text-foreground transition-colors"
      title="New document"
    >
      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
        <polyline points="14 2 14 8 20 8"/>
        <line x1="12" y1="18" x2="12" y2="12"/>
        <line x1="9" y1="15" x2="15" y2="15"/>
      </svg>
    </button>
  </div>

  <!-- New File Input -->
  {#if isCreatingFile}
    <div class="p-2 border-b border-border">
      <form onsubmit={(e) => { e.preventDefault(); createNewFile(); }}>
        <input
          type="text"
          bind:value={newFileName}
          placeholder="filename.md"
          class="w-full px-2 py-1 text-sm bg-background border border-input rounded focus:outline-none focus:ring-2 focus:ring-ring"
          autofocus
          onblur={() => { if (!newFileName) isCreatingFile = false; }}
          onkeydown={(e) => { if (e.key === 'Escape') isCreatingFile = false; }}
        />
      </form>
    </div>
  {/if}

  <!-- File List -->
  <div class="flex-1 overflow-auto p-2">
    {#if $fileSystem.files.length === 0}
      <div class="text-center text-muted-foreground text-sm py-8">
        <p>No documents yet</p>
        <button
          onclick={() => isCreatingFile = true}
          class="mt-2 text-primary hover:underline"
        >
          Create your first document
        </button>
      </div>
    {:else}
      <ul class="space-y-0.5">
        {#each $fileSystem.files as file (file.id)}
          <li>
            <button
              onclick={() => handleFileClick(file)}
              class="w-full flex items-center gap-2 px-2 py-1.5 text-sm rounded hover:bg-accent transition-colors text-left {$fileSystem.activeFilePath === file.path ? 'bg-accent' : ''}"
            >
              <span>{getFileIcon(file)}</span>
              <span class="truncate">{file.name}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>
