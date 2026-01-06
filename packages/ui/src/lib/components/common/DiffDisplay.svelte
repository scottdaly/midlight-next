<script lang="ts">
  /**
   * DiffDisplay - Visualizes differences between two text versions
   * Supports unified and split view modes
   */

  type DiffViewMode = 'unified' | 'split';

  interface Props {
    originalContent: string;
    newContent: string;
    mode?: DiffViewMode;
    fileName?: string;
  }

  let {
    originalContent,
    newContent,
    mode = 'unified',
    fileName,
  }: Props = $props();

  interface DiffLine {
    type: 'unchanged' | 'added' | 'removed';
    content: string;
    oldLineNum?: number;
    newLineNum?: number;
  }

  // Simple line-by-line diff algorithm
  function computeDiff(oldText: string, newText: string): DiffLine[] {
    const oldLines = oldText.split('\n');
    const newLines = newText.split('\n');
    const result: DiffLine[] = [];

    // Use longest common subsequence for better diffs
    const lcs = computeLCS(oldLines, newLines);

    let oldIdx = 0;
    let newIdx = 0;
    let oldLineNum = 1;
    let newLineNum = 1;

    for (const match of lcs) {
      // Add removed lines (before the match in old)
      while (oldIdx < match.oldIndex) {
        result.push({
          type: 'removed',
          content: oldLines[oldIdx],
          oldLineNum: oldLineNum++,
        });
        oldIdx++;
      }

      // Add added lines (before the match in new)
      while (newIdx < match.newIndex) {
        result.push({
          type: 'added',
          content: newLines[newIdx],
          newLineNum: newLineNum++,
        });
        newIdx++;
      }

      // Add the matching line
      result.push({
        type: 'unchanged',
        content: oldLines[oldIdx],
        oldLineNum: oldLineNum++,
        newLineNum: newLineNum++,
      });
      oldIdx++;
      newIdx++;
    }

    // Add remaining removed lines
    while (oldIdx < oldLines.length) {
      result.push({
        type: 'removed',
        content: oldLines[oldIdx],
        oldLineNum: oldLineNum++,
      });
      oldIdx++;
    }

    // Add remaining added lines
    while (newIdx < newLines.length) {
      result.push({
        type: 'added',
        content: newLines[newIdx],
        newLineNum: newLineNum++,
      });
      newIdx++;
    }

    return result;
  }

  interface LCSMatch {
    oldIndex: number;
    newIndex: number;
  }

  // Compute Longest Common Subsequence
  function computeLCS(oldLines: string[], newLines: string[]): LCSMatch[] {
    const m = oldLines.length;
    const n = newLines.length;

    // Build LCS table
    const dp: number[][] = Array(m + 1)
      .fill(null)
      .map(() => Array(n + 1).fill(0));

    for (let i = 1; i <= m; i++) {
      for (let j = 1; j <= n; j++) {
        if (oldLines[i - 1] === newLines[j - 1]) {
          dp[i][j] = dp[i - 1][j - 1] + 1;
        } else {
          dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1]);
        }
      }
    }

    // Backtrack to find matches
    const matches: LCSMatch[] = [];
    let i = m;
    let j = n;

    while (i > 0 && j > 0) {
      if (oldLines[i - 1] === newLines[j - 1]) {
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

  const diffLines = $derived(computeDiff(originalContent, newContent));

  const stats = $derived(() => {
    let added = 0;
    let removed = 0;
    for (const line of diffLines) {
      if (line.type === 'added') added++;
      else if (line.type === 'removed') removed++;
    }
    return { added, removed };
  });

  // For split view, pair up lines
  interface SplitPair {
    left: DiffLine | null;
    right: DiffLine | null;
  }

  const splitPairs = $derived((): SplitPair[] => {
    if (mode !== 'split') return [];

    const pairs: SplitPair[] = [];
    let i = 0;

    while (i < diffLines.length) {
      const line = diffLines[i];

      if (line.type === 'unchanged') {
        pairs.push({ left: line, right: line });
        i++;
      } else if (line.type === 'removed') {
        // Look ahead for a corresponding added line
        const nextLine = diffLines[i + 1];
        if (nextLine?.type === 'added') {
          pairs.push({ left: line, right: nextLine });
          i += 2;
        } else {
          pairs.push({ left: line, right: null });
          i++;
        }
      } else if (line.type === 'added') {
        pairs.push({ left: null, right: line });
        i++;
      } else {
        i++;
      }
    }

    return pairs;
  });
</script>

<div class="diff-display rounded-lg border border-neutral-700 overflow-hidden bg-neutral-900">
  <!-- Header -->
  {#if fileName}
    <div class="px-3 py-2 border-b border-neutral-700 bg-neutral-800/50 flex items-center justify-between">
      <span class="text-sm font-mono text-neutral-300">{fileName}</span>
      <div class="flex items-center gap-3 text-xs">
        <span class="text-green-400">+{stats().added}</span>
        <span class="text-red-400">-{stats().removed}</span>
      </div>
    </div>
  {/if}

  <!-- Diff Content -->
  <div class="overflow-auto max-h-96">
    {#if mode === 'unified'}
      <!-- Unified View -->
      <table class="w-full text-xs font-mono">
        <tbody>
          {#each diffLines as line, i (i)}
            <tr
              class="hover:bg-neutral-800/30
                {line.type === 'added' ? 'bg-green-500/10' : ''}
                {line.type === 'removed' ? 'bg-red-500/10' : ''}"
            >
              <!-- Old line number -->
              <td class="px-2 py-0.5 text-neutral-500 text-right select-none w-10 border-r border-neutral-800">
                {line.oldLineNum ?? ''}
              </td>
              <!-- New line number -->
              <td class="px-2 py-0.5 text-neutral-500 text-right select-none w-10 border-r border-neutral-800">
                {line.newLineNum ?? ''}
              </td>
              <!-- Indicator -->
              <td class="px-1 py-0.5 select-none w-4
                {line.type === 'added' ? 'text-green-400' : ''}
                {line.type === 'removed' ? 'text-red-400' : 'text-neutral-600'}">
                {#if line.type === 'added'}+{:else if line.type === 'removed'}-{:else}&nbsp;{/if}
              </td>
              <!-- Content -->
              <td class="px-2 py-0.5 whitespace-pre
                {line.type === 'added' ? 'text-green-300' : ''}
                {line.type === 'removed' ? 'text-red-300' : 'text-neutral-300'}">
                {line.content || ' '}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <!-- Split View -->
      <table class="w-full text-xs font-mono">
        <tbody>
          {#each splitPairs() as pair, i (i)}
            <tr class="border-b border-neutral-800/50">
              <!-- Left side (old) -->
              <td class="px-2 py-0.5 text-neutral-500 text-right select-none w-8 border-r border-neutral-800">
                {pair.left?.oldLineNum ?? ''}
              </td>
              <td
                class="w-1/2 px-2 py-0.5 whitespace-pre border-r border-neutral-700
                  {pair.left?.type === 'removed' ? 'bg-red-500/10 text-red-300' : 'text-neutral-300'}"
              >
                {pair.left?.content ?? ''}
              </td>
              <!-- Right side (new) -->
              <td class="px-2 py-0.5 text-neutral-500 text-right select-none w-8 border-r border-neutral-800">
                {pair.right?.newLineNum ?? ''}
              </td>
              <td
                class="w-1/2 px-2 py-0.5 whitespace-pre
                  {pair.right?.type === 'added' ? 'bg-green-500/10 text-green-300' : 'text-neutral-300'}"
              >
                {pair.right?.content ?? ''}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
</div>

<style>
  .diff-display {
    font-variant-ligatures: none;
  }

  table {
    border-collapse: collapse;
  }

  td {
    vertical-align: top;
  }
</style>
