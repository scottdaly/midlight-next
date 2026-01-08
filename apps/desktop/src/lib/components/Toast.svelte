<script lang="ts">
  import { toastStore, type Toast, type ToastType } from '@midlight/stores';

  interface Props {
    toast: Toast;
  }

  let { toast }: Props = $props();

  let isExiting = $state(false);

  // Icon and colors by type
  const iconsByType: Record<ToastType, string> = {
    success: 'M5 13l4 4L19 7',
    error: 'M6 18L18 6M6 6l12 12',
    warning: 'M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z',
    info: 'M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
  };

  const colorsByType: Record<ToastType, { bg: string; icon: string; border: string }> = {
    success: {
      bg: 'bg-emerald-500/10',
      icon: 'text-emerald-500',
      border: 'border-emerald-500/20',
    },
    error: {
      bg: 'bg-red-500/10',
      icon: 'text-red-500',
      border: 'border-red-500/20',
    },
    warning: {
      bg: 'bg-amber-500/10',
      icon: 'text-amber-500',
      border: 'border-amber-500/20',
    },
    info: {
      bg: 'bg-blue-500/10',
      icon: 'text-blue-500',
      border: 'border-blue-500/20',
    },
  };

  const colors = $derived(colorsByType[toast.type]);
  const icon = $derived(iconsByType[toast.type]);

  function handleDismiss() {
    isExiting = true;
    setTimeout(() => {
      toastStore.dismiss(toast.id);
    }, 200);
  }

  function handleMouseEnter() {
    toastStore.pause(toast.id);
  }

  function handleMouseLeave() {
    toastStore.resume(toast.id);
  }

  function handleAction() {
    toast.action?.onClick();
    handleDismiss();
  }
</script>

<div
  class="toast flex items-start gap-3 p-4 rounded-lg border shadow-lg backdrop-blur-sm transition-all duration-200 {colors.bg} {colors.border} {isExiting ? 'exiting' : 'visible'}"
  role="alert"
  onmouseenter={handleMouseEnter}
  onmouseleave={handleMouseLeave}
>
  <!-- Icon -->
  <div class="flex-shrink-0 mt-0.5">
    <svg class="w-5 h-5 {colors.icon}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
      <path stroke-linecap="round" stroke-linejoin="round" d={icon} />
    </svg>
  </div>

  <!-- Content -->
  <div class="flex-1 min-w-0">
    <p class="text-sm text-foreground">{toast.message}</p>
    {#if toast.action}
      <button
        class="mt-2 text-sm font-medium text-primary hover:text-primary/80 transition-colors"
        onclick={handleAction}
      >
        {toast.action.label}
      </button>
    {/if}
  </div>

  <!-- Dismiss button -->
  {#if toast.dismissible}
    <button
      class="flex-shrink-0 p-1 rounded hover:bg-foreground/10 transition-colors"
      onclick={handleDismiss}
      aria-label="Dismiss"
    >
      <svg class="w-4 h-4 text-muted-foreground" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
        <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
      </svg>
    </button>
  {/if}
</div>

<style>
  .toast {
    transform: translateX(calc(100% + 16px));
    opacity: 0;
  }

  .toast.visible {
    transform: translateX(0);
    opacity: 1;
  }

  .toast.exiting {
    transform: translateX(calc(100% + 16px));
    opacity: 0;
  }
</style>
