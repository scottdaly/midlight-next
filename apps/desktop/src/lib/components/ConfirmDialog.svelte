<script lang="ts">
  interface Props {
    open: boolean;
    title: string;
    message: string;
    confirmText?: string;
    cancelText?: string;
    variant?: 'default' | 'danger';
    onConfirm: () => void;
    onCancel: () => void;
  }

  let {
    open,
    title,
    message,
    confirmText = 'Confirm',
    cancelText = 'Cancel',
    variant = 'default',
    onConfirm,
    onCancel,
  }: Props = $props();

  let dialogRef: HTMLDivElement | null = $state(null);

  // Focus trap and keyboard handling
  function handleKeyDown(e: KeyboardEvent) {
    if (!open) return;

    if (e.key === 'Escape') {
      e.preventDefault();
      onCancel();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      onConfirm();
    }
  }

  // Focus the dialog when opened
  $effect(() => {
    if (open && dialogRef) {
      dialogRef.focus();
    }
  });
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if open}
  <!-- Backdrop -->
  <div
    class="fixed inset-0 bg-black/50 z-50 flex items-center justify-center"
    onclick={onCancel}
    role="presentation"
  >
    <!-- Dialog -->
    <div
      bind:this={dialogRef}
      class="bg-card border border-border rounded-lg shadow-xl max-w-md w-full mx-4 p-6"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      aria-modal="true"
      aria-labelledby="dialog-title"
      tabindex="-1"
    >
      <!-- Title -->
      <h2 id="dialog-title" class="text-lg font-semibold text-foreground mb-2">
        {title}
      </h2>

      <!-- Message -->
      <p class="text-muted-foreground mb-6 text-sm">
        {message}
      </p>

      <!-- Buttons -->
      <div class="flex justify-end gap-3">
        <button
          class="px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors"
          onclick={onCancel}
        >
          {cancelText}
        </button>
        <button
          class="px-4 py-2 text-sm font-medium rounded transition-colors {variant === 'danger'
            ? 'bg-destructive hover:bg-destructive/90 text-destructive-foreground'
            : 'bg-primary hover:bg-primary/90 text-primary-foreground'}"
          onclick={onConfirm}
        >
          {confirmText}
        </button>
      </div>
    </div>
  </div>
{/if}
