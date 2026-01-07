<script lang="ts">
  /**
   * QuotaBadge - Shows quota usage in chat header
   * Displays "42/100 messages" for free tier or "Unlimited" for paid
   */

  import { subscription, isFreeTier, quotaDisplay } from '@midlight/stores';

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  // Determine color based on usage
  const colorClass = $derived(() => {
    if (!$subscription.quota) return 'text-muted-foreground';
    const { used, limit } = $subscription.quota;
    if (limit === null) return 'text-muted-foreground'; // Unlimited
    const percent = (used / limit) * 100;
    if (percent >= 90) return 'text-destructive';
    if (percent >= 75) return 'text-amber-500';
    return 'text-muted-foreground';
  });
</script>

<div class="flex items-center gap-1 text-xs {colorClass()}">
  {#if compact}
    <span>{$quotaDisplay}</span>
  {:else}
    <!-- Message bubble icon -->
    <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
    </svg>
    <span>{$quotaDisplay}</span>
    {#if $isFreeTier}
      <span class="text-muted-foreground">this month</span>
    {/if}
  {/if}
</div>
