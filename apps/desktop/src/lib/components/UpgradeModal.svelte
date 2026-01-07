<script lang="ts">
  /**
   * UpgradeModal - Shows subscription plans and handles upgrade flow
   */

  import { subscription } from '@midlight/stores';
  import { subscriptionClient } from '$lib/subscription';
  import { onMount } from 'svelte';

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();
  let isLoading = $state(false);
  let error = $state<string | null>(null);

  // Fetch prices when modal opens
  $effect(() => {
    if (open && $subscription.prices.length === 0 && !$subscription.isLoading) {
      subscriptionClient.fetchPrices().catch(console.error);
    }
  });

  async function handleSelectPlan(priceId: string) {
    isLoading = true;
    error = null;

    try {
      await subscriptionClient.openCheckout(priceId);
      // Browser will open Stripe checkout
      onClose();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      isLoading = false;
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  function handleBackdropClick() {
    onClose();
  }

  function handleModalClick(e: MouseEvent) {
    e.stopPropagation();
  }

  // Format price from cents to dollars
  function formatPrice(cents: number, currency: string): string {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: currency.toUpperCase(),
    }).format(cents / 100);
  }

  // Default pricing if prices not loaded from backend
  const defaultPlans = [
    {
      id: 'price_premium_monthly',
      name: 'Premium',
      amount: 999,
      currency: 'usd',
      interval: 'month',
      description: 'For individuals',
      features: [
        'Unlimited AI messages',
        'Priority support',
        'Latest AI models',
      ],
    },
    {
      id: 'price_pro_monthly',
      name: 'Pro',
      amount: 1999,
      currency: 'usd',
      interval: 'month',
      description: 'For power users',
      features: [
        'Everything in Premium',
        'Early access to new features',
        'Custom AI instructions',
      ],
      recommended: true,
    },
  ];

  const plans = $derived($subscription.prices.length > 0 ? $subscription.prices : defaultPlans);
</script>

<svelte:window onkeydown={open ? handleKeyDown : undefined} />

{#if open}
  <!-- Backdrop -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 bg-black/50 z-50 flex items-center justify-center"
    onclick={handleBackdropClick}
  >
    <!-- Modal -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="bg-popover border border-border rounded-lg shadow-xl w-full max-w-2xl mx-4 max-h-[90vh] overflow-y-auto"
      onclick={handleModalClick}
    >
      <!-- Header -->
      <div class="flex items-center justify-between p-4 border-b border-border">
        <div>
          <h2 class="text-lg font-semibold">Upgrade Your Plan</h2>
          <p class="text-sm text-muted-foreground">
            Unlock unlimited AI messages and more features
          </p>
        </div>
        <button
          onclick={onClose}
          class="p-1.5 hover:bg-accent rounded-lg transition-colors"
          title="Close"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18"/>
            <line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>

      <!-- Error message -->
      {#if error}
        <div class="mx-4 mt-4 p-3 bg-destructive/10 text-destructive text-sm rounded-lg">
          {error}
        </div>
      {/if}

      <!-- Pricing cards -->
      <div class="p-4 grid gap-4 {plans.length === 2 ? 'md:grid-cols-2' : 'md:grid-cols-3'}">
        {#each plans as plan}
          <div
            class="relative rounded-lg border p-4 flex flex-col
              {plan.recommended ? 'border-primary bg-primary/5' : 'border-border'}"
          >
            {#if plan.recommended}
              <div class="absolute -top-3 left-1/2 -translate-x-1/2">
                <span class="bg-primary text-primary-foreground text-xs font-medium px-2 py-1 rounded-full">
                  Recommended
                </span>
              </div>
            {/if}

            <div class="mb-4">
              <h3 class="text-lg font-semibold">{plan.name}</h3>
              {#if plan.description}
                <p class="text-sm text-muted-foreground">{plan.description}</p>
              {/if}
            </div>

            <div class="mb-4">
              <span class="text-3xl font-bold">{formatPrice(plan.amount, plan.currency)}</span>
              <span class="text-muted-foreground">/{plan.interval}</span>
            </div>

            <ul class="flex-1 space-y-2 mb-4">
              {#each plan.features || [] as feature}
                <li class="flex items-center gap-2 text-sm">
                  <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-green-500 shrink-0">
                    <polyline points="20 6 9 17 4 12"/>
                  </svg>
                  {feature}
                </li>
              {/each}
            </ul>

            <button
              onclick={() => handleSelectPlan(plan.id)}
              disabled={isLoading || $subscription.isLoading}
              class="w-full py-2 px-4 rounded-lg font-medium transition-colors disabled:opacity-50
                {plan.recommended
                  ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                  : 'bg-accent hover:bg-accent/80'}"
            >
              {isLoading ? 'Loading...' : 'Subscribe'}
            </button>
          </div>
        {/each}
      </div>

      <!-- Footer -->
      <div class="p-4 border-t border-border text-center text-xs text-muted-foreground">
        <p>
          Cancel anytime. Prices shown in USD.
          <a href="https://midlight.ai/terms" target="_blank" rel="noopener" class="underline hover:text-foreground">
            Terms of Service
          </a>
        </p>
      </div>
    </div>
  </div>
{/if}
