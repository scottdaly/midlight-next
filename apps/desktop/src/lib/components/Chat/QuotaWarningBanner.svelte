<script lang="ts">
  /**
   * QuotaWarningBanner - Shows warning when quota is running low
   * Displays at 75% (warning) and 90% (critical) usage
   */

  import {
    subscription,
    showQuotaWarning,
    quotaWarningSeverity,
    quotaPercentUsed,
  } from '@midlight/stores';

  interface Props {
    onUpgrade?: () => void;
  }

  let { onUpgrade }: Props = $props();

  const percentUsed = $derived(Math.round($quotaPercentUsed));
</script>

{#if $showQuotaWarning && $subscription.quota}
  <div
    class="flex items-center justify-between gap-2 px-3 py-2 text-xs {$quotaWarningSeverity === 'critical'
      ? 'bg-destructive/10 text-destructive border-b border-destructive/20'
      : 'bg-amber-500/10 text-amber-600 dark:text-amber-400 border-b border-amber-500/20'}"
  >
    <div class="flex items-center gap-2">
      {#if $quotaWarningSeverity === 'critical'}
        <!-- Alert circle icon -->
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="10"/>
          <line x1="12" y1="8" x2="12" y2="12"/>
          <line x1="12" y1="16" x2="12.01" y2="16"/>
        </svg>
        <span>
          You've used <strong>{percentUsed}%</strong> of your free messages. Upgrade for unlimited.
        </span>
      {:else}
        <!-- Alert triangle icon -->
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
          <line x1="12" y1="9" x2="12" y2="13"/>
          <line x1="12" y1="17" x2="12.01" y2="17"/>
        </svg>
        <span>
          You've used <strong>{percentUsed}%</strong> of your free messages this month.
        </span>
      {/if}
    </div>

    {#if onUpgrade}
      <button
        onclick={onUpgrade}
        class="shrink-0 px-2 py-1 rounded text-xs font-medium transition-colors
          {$quotaWarningSeverity === 'critical'
            ? 'bg-destructive text-destructive-foreground hover:bg-destructive/90'
            : 'bg-amber-500 text-white hover:bg-amber-600'}"
      >
        Upgrade
      </button>
    {/if}
  </div>
{/if}
