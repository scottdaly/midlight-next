<script lang="ts">
  /**
   * InlineDiff - Shows AI-suggested changes inline with Accept/Reject buttons
   * Displayed as a floating panel near the edited text
   */

  interface Props {
    position: { x: number; y: number };
    originalText: string;
    suggestedText: string;
    isStreaming?: boolean;
    onAccept: () => void;
    onReject: () => void;
  }

  let {
    position,
    originalText,
    suggestedText,
    isStreaming = false,
    onAccept,
    onReject,
  }: Props = $props();

  // Compute simple word-level diff for display
  interface DiffSegment {
    type: 'unchanged' | 'removed' | 'added';
    text: string;
  }

  function computeWordDiff(original: string, suggested: string): DiffSegment[] {
    const originalWords = original.split(/(\s+)/);
    const suggestedWords = suggested.split(/(\s+)/);
    const result: DiffSegment[] = [];

    // Simple LCS-based diff on words
    const lcs = computeLCS(originalWords, suggestedWords);

    let origIdx = 0;
    let suggIdx = 0;

    for (const match of lcs) {
      // Add removed words
      while (origIdx < match.oldIndex) {
        result.push({ type: 'removed', text: originalWords[origIdx] });
        origIdx++;
      }

      // Add added words
      while (suggIdx < match.newIndex) {
        result.push({ type: 'added', text: suggestedWords[suggIdx] });
        suggIdx++;
      }

      // Add matching word
      result.push({ type: 'unchanged', text: originalWords[origIdx] });
      origIdx++;
      suggIdx++;
    }

    // Remaining removed words
    while (origIdx < originalWords.length) {
      result.push({ type: 'removed', text: originalWords[origIdx] });
      origIdx++;
    }

    // Remaining added words
    while (suggIdx < suggestedWords.length) {
      result.push({ type: 'added', text: suggestedWords[suggIdx] });
      suggIdx++;
    }

    return result;
  }

  interface LCSMatch {
    oldIndex: number;
    newIndex: number;
  }

  function computeLCS(oldArr: string[], newArr: string[]): LCSMatch[] {
    const m = oldArr.length;
    const n = newArr.length;

    const dp: number[][] = Array(m + 1)
      .fill(null)
      .map(() => Array(n + 1).fill(0));

    for (let i = 1; i <= m; i++) {
      for (let j = 1; j <= n; j++) {
        if (oldArr[i - 1] === newArr[j - 1]) {
          dp[i][j] = dp[i - 1][j - 1] + 1;
        } else {
          dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1]);
        }
      }
    }

    const matches: LCSMatch[] = [];
    let i = m;
    let j = n;

    while (i > 0 && j > 0) {
      if (oldArr[i - 1] === newArr[j - 1]) {
        matches.unshift({ oldIndex: i - 1, newIndex: j - 1 });
        i--;
        j--;
      } else if (dp[i - 1][j] > dp[i][j - 1]) {
        i--;
      } else {
        j--;
      }
    }

    return matches;
  }

  const diffSegments = $derived(computeWordDiff(originalText, suggestedText));

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      onAccept();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onReject();
    }
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

<div
  class="inline-diff fixed z-50 bg-popover border border-border rounded-lg shadow-xl max-w-[500px]"
  style="left: {position.x}px; top: {position.y}px;"
>
  <!-- Header -->
  <div class="flex items-center gap-2 px-3 py-2 border-b border-border">
    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-primary">
      <path d="m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1 1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275L12 3Z"/>
    </svg>
    <span class="text-xs font-medium text-foreground">AI Suggestion</span>
    {#if isStreaming}
      <span class="text-xs text-muted-foreground animate-pulse">Generating...</span>
    {/if}
  </div>

  <!-- Diff content -->
  <div class="p-3 text-sm leading-relaxed max-h-[200px] overflow-auto">
    {#each diffSegments as segment}
      {#if segment.type === 'unchanged'}
        <span>{segment.text}</span>
      {:else if segment.type === 'removed'}
        <span class="bg-red-500/20 text-red-400 line-through">{segment.text}</span>
      {:else if segment.type === 'added'}
        <span class="bg-green-500/20 text-green-400">{segment.text}</span>
      {/if}
    {/each}
    {#if isStreaming}
      <span class="inline-block w-1.5 h-4 bg-primary animate-pulse ml-0.5"></span>
    {/if}
  </div>

  <!-- Actions -->
  <div class="flex items-center gap-2 px-3 py-2 border-t border-border bg-muted/30">
    <button
      onclick={onAccept}
      disabled={isStreaming}
      class="flex-1 px-3 py-1.5 text-xs rounded-md bg-green-600 hover:bg-green-500 text-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-1"
    >
      <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="20 6 9 17 4 12"/>
      </svg>
      Accept
      <span class="text-green-200/70 ml-1">⌘↵</span>
    </button>
    <button
      onclick={onReject}
      class="flex-1 px-3 py-1.5 text-xs rounded-md bg-neutral-600 hover:bg-neutral-500 text-white transition-colors flex items-center justify-center gap-1"
    >
      <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="18" y1="6" x2="6" y2="18"/>
        <line x1="6" y1="6" x2="18" y2="18"/>
      </svg>
      Reject
      <span class="text-neutral-300/70 ml-1">Esc</span>
    </button>
  </div>
</div>

<style>
  .inline-diff {
    animation: fadeIn 0.15s ease-out;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
