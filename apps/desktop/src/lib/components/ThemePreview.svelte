<script lang="ts">
  import type { Theme } from '@midlight/stores';

  interface Props {
    theme: Theme;
    selected: boolean;
    onclick: () => void;
  }

  let { theme, selected, onclick }: Props = $props();

  // Theme color definitions for previews
  const themeColors: Record<Theme, { bg: string; sidebar: string; text: string; accent: string }> = {
    light: { bg: '#ffffff', sidebar: '#f1f5f9', text: '#0f172a', accent: '#3b82f6' },
    dark: { bg: '#1f2937', sidebar: '#111827', text: '#f9fafb', accent: '#60a5fa' },
    midnight: { bg: '#1e293b', sidebar: '#0f172a', text: '#f0f9ff', accent: '#38bdf8' },
    sepia: { bg: '#fdfbf7', sidebar: '#f5f0e6', text: '#1c1917', accent: '#d97706' },
    forest: { bg: '#15231d', sidebar: '#0d1a14', text: '#f0fdf4', accent: '#22c55e' },
    cyberpunk: { bg: '#0d0514', sidebar: '#1a0a2e', text: '#fae8ff', accent: '#ec4899' },
    coffee: { bg: '#f3eeda', sidebar: '#e8e0d0', text: '#3d3029', accent: '#92400e' },
    system: { bg: '#ffffff', sidebar: '#f1f5f9', text: '#0f172a', accent: '#3b82f6' },
  };

  const themeLabels: Record<Theme, { name: string; description: string }> = {
    light: { name: 'Light', description: 'Clean and bright' },
    dark: { name: 'Dark', description: 'Easy on the eyes' },
    midnight: { name: 'Midnight', description: 'Deep contrast' },
    sepia: { name: 'Sepia', description: 'Warm and reading-focused' },
    forest: { name: 'Forest', description: 'Calming nature tones' },
    cyberpunk: { name: 'Cyberpunk', description: 'High contrast neon' },
    coffee: { name: 'Coffee', description: 'Rich and cozy' },
    system: { name: 'System', description: 'Follows OS settings' },
  };

  const colors = $derived(themeColors[theme]);
  const labels = $derived(themeLabels[theme]);
</script>

<button
  {onclick}
  class="relative text-left rounded-xl border-2 overflow-hidden transition-all
         {selected ? 'border-primary ring-2 ring-primary ring-offset-2 ring-offset-background' : 'border-border hover:border-muted-foreground'}"
>
  <!-- Mini Preview -->
  <div class="aspect-video p-2" style="background: {colors.bg}">
    <div class="h-full flex rounded overflow-hidden border" style="border-color: {colors.text}20">
      <!-- Mini sidebar -->
      <div class="w-1/4" style="background: {colors.sidebar}">
        <div class="p-1 space-y-0.5">
          <div class="w-full h-0.5 rounded" style="background: {colors.text}40"></div>
          <div class="w-3/4 h-0.5 rounded" style="background: {colors.text}20"></div>
          <div class="w-2/3 h-0.5 rounded" style="background: {colors.text}20"></div>
        </div>
      </div>
      <!-- Mini content -->
      <div class="flex-1 p-1.5">
        <div class="w-3/4 h-1 rounded mb-1" style="background: {colors.text}"></div>
        <div class="w-full h-0.5 rounded mb-0.5" style="background: {colors.text}40"></div>
        <div class="w-5/6 h-0.5 rounded mb-0.5" style="background: {colors.text}40"></div>
        <div class="w-1/2 h-0.5 rounded" style="background: {colors.text}40"></div>
      </div>
    </div>
  </div>

  <!-- Label -->
  <div class="p-3 border-t border-border bg-card">
    <div class="font-medium text-sm">{labels.name}</div>
    <div class="text-xs text-muted-foreground">{labels.description}</div>
  </div>

  <!-- Selected checkmark -->
  {#if selected}
    <div class="absolute top-2 right-2 w-5 h-5 bg-primary rounded-full flex items-center justify-center">
      <svg class="w-3 h-3 text-primary-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3">
        <polyline points="20 6 9 17 4 12"></polyline>
      </svg>
    </div>
  {/if}
</button>
