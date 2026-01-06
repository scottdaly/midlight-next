<script lang="ts">
  interface Props {
    open: boolean;
    onSave: (label: string, description: string) => void;
    onCancel: () => void;
  }

  let { open, onSave, onCancel }: Props = $props();

  let label = $state('');
  let description = $state('');
  let dialogRef: HTMLDivElement | null = $state(null);
  let labelInputRef: HTMLInputElement | null = $state(null);

  const labelMaxLength = 50;
  const descriptionMaxLength = 200;

  const isValid = $derived(label.trim().length > 0 && label.length <= labelMaxLength);

  function handleSubmit(e: Event) {
    e.preventDefault();
    if (!isValid) return;
    onSave(label.trim(), description.trim());
    resetForm();
  }

  function handleCancel() {
    onCancel();
    resetForm();
  }

  function resetForm() {
    label = '';
    description = '';
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (!open) return;

    if (e.key === 'Escape') {
      e.preventDefault();
      handleCancel();
    }
  }

  // Focus the label input when opened
  $effect(() => {
    if (open && labelInputRef) {
      setTimeout(() => labelInputRef?.focus(), 0);
    }
  });
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if open}
  <!-- Backdrop -->
  <div
    class="fixed inset-0 bg-black/50 z-50 flex items-center justify-center"
    onclick={handleCancel}
    role="presentation"
  >
    <!-- Dialog -->
    <div
      bind:this={dialogRef}
      class="bg-card border border-border rounded-lg shadow-xl max-w-md w-full mx-4"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      aria-modal="true"
      aria-labelledby="dialog-title"
      tabindex="-1"
    >
      <!-- Header -->
      <div class="px-6 py-4 border-b border-border">
        <h2 id="dialog-title" class="text-lg font-semibold text-foreground">
          Save Version
        </h2>
        <p class="text-sm text-muted-foreground mt-1">
          Create a named version you can restore later
        </p>
      </div>

      <!-- Form -->
      <form onsubmit={handleSubmit} class="p-6 space-y-4">
        <!-- Label -->
        <div>
          <label for="version-label" class="block text-sm font-medium text-foreground mb-1.5">
            Version name <span class="text-destructive">*</span>
          </label>
          <input
            bind:this={labelInputRef}
            bind:value={label}
            type="text"
            id="version-label"
            placeholder="e.g., First draft, Before major edit..."
            maxlength={labelMaxLength}
            class="w-full bg-background border border-input rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring"
          />
          <div class="flex justify-between mt-1">
            <span class="text-xs text-muted-foreground">Required</span>
            <span class="text-xs {label.length > labelMaxLength ? 'text-destructive' : 'text-muted-foreground'}">
              {label.length}/{labelMaxLength}
            </span>
          </div>
        </div>

        <!-- Description -->
        <div>
          <label for="version-description" class="block text-sm font-medium text-foreground mb-1.5">
            Description <span class="text-muted-foreground">(optional)</span>
          </label>
          <textarea
            bind:value={description}
            id="version-description"
            placeholder="What changes does this version contain?"
            maxlength={descriptionMaxLength}
            rows={3}
            class="w-full bg-background border border-input rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring resize-none"
          ></textarea>
          <div class="flex justify-end mt-1">
            <span class="text-xs {description.length > descriptionMaxLength ? 'text-destructive' : 'text-muted-foreground'}">
              {description.length}/{descriptionMaxLength}
            </span>
          </div>
        </div>

        <!-- Buttons -->
        <div class="flex justify-end gap-3 pt-2">
          <button
            type="button"
            class="px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            onclick={handleCancel}
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={!isValid}
            class="px-4 py-2 text-sm font-medium bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Save Version
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}
