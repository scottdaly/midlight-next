<script lang="ts">
  import { workflowStore, availableWorkflows, workflowPhase } from '@midlight/stores';
  import { WORKFLOW_CATEGORIES } from '@midlight/core';

  function handleSelect(workflowId: string) {
    workflowStore.selectWorkflow(workflowId);
  }

  function handleClose() {
    workflowStore.cancel();
  }

  function handleBackdropClick() {
    workflowStore.cancel();
  }

  function handleModalClick(e: MouseEvent) {
    e.stopPropagation();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      workflowStore.cancel();
    }
  }

  // Group workflows by category
  function getWorkflowsByCategory() {
    const grouped: Record<string, typeof $availableWorkflows> = {};
    for (const workflow of $availableWorkflows) {
      if (!grouped[workflow.category]) {
        grouped[workflow.category] = [];
      }
      grouped[workflow.category].push(workflow);
    }
    return grouped;
  }

  function getCategoryName(categoryId: string): string {
    const category = WORKFLOW_CATEGORIES.find((c) => c.id === categoryId);
    return category?.name || categoryId;
  }

  let groupedWorkflows = $derived(getWorkflowsByCategory());
</script>

<svelte:window onkeydown={$workflowPhase === 'selecting' ? handleKeyDown : undefined} />

{#if $workflowPhase === 'selecting'}
  <!-- Backdrop -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4"
    onclick={handleBackdropClick}
  >
    <!-- Modal -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="bg-card border border-border rounded-lg shadow-xl max-w-2xl w-full max-h-[80vh] flex flex-col"
      onclick={handleModalClick}
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-6 py-4 border-b border-border">
        <div>
          <h2 class="text-lg font-semibold">Create New Project</h2>
          <p class="text-sm text-muted-foreground mt-0.5">
            Choose a workflow to get started
          </p>
        </div>
        <button
          onclick={handleClose}
          class="p-1.5 hover:bg-muted rounded text-muted-foreground hover:text-foreground transition-colors"
          aria-label="Close"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M18 6 6 18"/>
            <path d="M6 6 18 18"/>
          </svg>
        </button>
      </div>

      <!-- Content -->
      <div class="flex-1 overflow-y-auto p-6 space-y-6">
        {#each Object.entries(groupedWorkflows) as [category, workflows]}
          <div>
            <h3 class="text-sm font-medium text-muted-foreground mb-3">
              {getCategoryName(category)}
            </h3>
            <div class="grid grid-cols-1 gap-3">
              {#each workflows as workflow}
                <button
                  onclick={() => handleSelect(workflow.id)}
                  class="flex items-start gap-4 p-4 rounded-lg border border-border bg-background hover:bg-muted/50 hover:border-primary/50 transition-all text-left group"
                >
                  <span class="text-3xl" role="img" aria-label={workflow.name}>
                    {workflow.icon}
                  </span>
                  <div class="flex-1 min-w-0">
                    <h4 class="font-medium group-hover:text-primary transition-colors">
                      {workflow.name}
                    </h4>
                    <p class="text-sm text-muted-foreground mt-0.5">
                      {workflow.description}
                    </p>
                    <div class="flex items-center gap-2 mt-2">
                      <span class="text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded">
                        {workflow.interview.length} questions
                      </span>
                      <span class="text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded">
                        {workflow.templates.filter(t => t.type === 'file').length} files
                      </span>
                    </div>
                  </div>
                  <svg
                    xmlns="http://www.w3.org/2000/svg"
                    width="18"
                    height="18"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    class="text-muted-foreground group-hover:text-primary group-hover:translate-x-1 transition-all mt-1"
                  >
                    <path d="m9 18 6-6-6-6"/>
                  </svg>
                </button>
              {/each}
            </div>
          </div>
        {/each}

        <!-- Empty Project Option -->
        <div>
          <h3 class="text-sm font-medium text-muted-foreground mb-3">
            Or start fresh
          </h3>
          <button
            onclick={() => {
              // For empty project, just create a folder with .project.midlight
              workflowStore.cancel();
              // TODO: Trigger empty project creation
            }}
            class="flex items-start gap-4 p-4 rounded-lg border border-dashed border-border bg-background hover:bg-muted/30 hover:border-muted-foreground transition-all text-left group w-full"
          >
            <span class="text-3xl text-muted-foreground">📁</span>
            <div class="flex-1 min-w-0">
              <h4 class="font-medium text-muted-foreground group-hover:text-foreground transition-colors">
                Empty Project
              </h4>
              <p class="text-sm text-muted-foreground mt-0.5">
                Start with a blank project and set it up yourself
              </p>
            </div>
          </button>
        </div>
      </div>

      <!-- Footer -->
      <div class="px-6 py-4 border-t border-border bg-muted/30">
        <p class="text-xs text-muted-foreground">
          Workflows help you get started quickly with a structured interview and pre-made templates.
        </p>
      </div>
    </div>
  </div>
{/if}
