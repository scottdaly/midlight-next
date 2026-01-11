<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { projectStore, fileSystem, archivedProjects as archivedProjectsStore } from '@midlight/stores';
  import ConfirmDialog from './ConfirmDialog.svelte';

  // Get archived projects from the derived store, sorted by name
  const archivedProjects = $derived(
    [...$archivedProjectsStore]
      .map(p => ({
        path: p.path,
        name: p.config.name || p.path.split('/').pop() || p.path,
      }))
      .sort((a, b) => a.name.localeCompare(b.name))
  );

  // Collapse state
  let isExpanded = $state(false);

  // Delete confirmation dialog
  let deleteDialog = $state<{
    show: boolean;
    project: ArchivedProject | null;
  }>({
    show: false,
    project: null,
  });

  async function restoreProject(project: ArchivedProject) {
    try {
      // Update the store
      projectStore.setProjectStatus(project.path, 'active');

      // Read current config, update status, and write back
      const configPath = `${project.path}/.project.midlight`;
      const configContent = await invoke<string>('read_file', { path: configPath });
      const config = JSON.parse(configContent);
      config.status = 'active';
      await invoke('write_file', { path: configPath, content: JSON.stringify(config, null, 2) });
    } catch (e) {
      console.error('Failed to restore project:', e);
    }
  }

  function showDeleteConfirm(project: ArchivedProject) {
    deleteDialog = { show: true, project };
  }

  async function confirmDelete() {
    if (!deleteDialog.project) return;

    const projectPath = deleteDialog.project.path;
    deleteDialog = { show: false, project: null };

    try {
      // Move to trash
      await invoke('file_trash', { path: projectPath });
      // Remove from project store
      projectStore.removeProject(projectPath);
      // Refresh the file system
      await fileSystem.refresh();
    } catch (e) {
      console.error('Failed to delete project:', e);
    }
  }

  function cancelDelete() {
    deleteDialog = { show: false, project: null };
  }
</script>

{#if archivedProjects.length > 0}
  <div class="border-t border-border">
    <!-- Header button to expand/collapse -->
    <button
      onclick={() => isExpanded = !isExpanded}
      class="w-full flex items-center justify-between px-3 py-2 text-xs text-muted-foreground hover:text-foreground hover:bg-accent/50 transition-colors"
    >
      <div class="flex items-center gap-2">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          class="transition-transform {isExpanded ? 'rotate-90' : ''}"
        >
          <polyline points="9 18 15 12 9 6"/>
        </svg>
        <span>{archivedProjects.length} archived project{archivedProjects.length !== 1 ? 's' : ''}</span>
      </div>
      <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="opacity-50">
        <path d="m21 8-2 2-1.5-3.7A2 2 0 0 0 15.65 5H8.35a2 2 0 0 0-1.85 1.3L5 10 3 8"/>
        <path d="M3.3 12H2a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h20a2 2 0 0 0 2-2v-8a2 2 0 0 0-2-2h-1.3"/>
        <rect x="6" y="8" width="12" height="10" rx="2"/>
      </svg>
    </button>

    <!-- Archived projects list -->
    {#if isExpanded}
      <div class="pb-2 space-y-0.5">
        {#each archivedProjects as project}
          <div class="group flex items-center gap-2 px-3 py-1 hover:bg-accent/30">
            <!-- Project icon -->
            <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="flex-shrink-0 text-muted-foreground opacity-60">
              <rect width="20" height="14" x="2" y="7" rx="2" ry="2"/>
              <path d="M16 21V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v16"/>
            </svg>

            <!-- Project name -->
            <span class="flex-1 text-xs text-muted-foreground truncate" title={project.path}>
              {project.name}
            </span>

            <!-- Actions (visible on hover) -->
            <div class="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
              <!-- Restore button -->
              <button
                onclick={() => restoreProject(project)}
                class="p-1 hover:bg-accent rounded text-muted-foreground hover:text-primary"
                title="Restore project"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/>
                  <path d="M3 3v5h5"/>
                </svg>
              </button>

              <!-- Delete button -->
              <button
                onclick={() => showDeleteConfirm(project)}
                class="p-1 hover:bg-accent rounded text-muted-foreground hover:text-destructive"
                title="Delete permanently"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M3 6h18"/>
                  <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/>
                  <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>
                </svg>
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<!-- Delete Confirmation Dialog -->
<ConfirmDialog
  open={deleteDialog.show}
  title="Delete Project Permanently?"
  message="This will permanently delete '{deleteDialog.project?.name || ''}' and all its contents. This action cannot be undone."
  confirmText="Delete Permanently"
  cancelText="Cancel"
  variant="danger"
  onConfirm={confirmDelete}
  onCancel={cancelDelete}
/>
