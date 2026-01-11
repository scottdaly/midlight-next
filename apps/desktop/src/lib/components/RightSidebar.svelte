<script lang="ts">
  import { versions, ui } from '@midlight/stores';
  import type { RightPanelMode } from '@midlight/stores';
  import ChatPanel from './ChatPanel.svelte';
  import VersionsPanel from './VersionsPanel.svelte';
  import PendingChangesPanel from './Chat/PendingChangesPanel.svelte';
  import ContextPanel from './ContextPanel.svelte';

  interface Props {
    mode?: RightPanelMode;
    onOpenAuth?: () => void;
  }

  let { mode = 'chat', onOpenAuth }: Props = $props();

  // Sync versions store when mode changes
  $effect(() => {
    if (mode === 'versions' && !$versions.isOpen) {
      versions.open();
    } else if (mode === 'chat' && $versions.isOpen) {
      versions.close();
    }
  });

  function closePendingPanel() {
    ui.setRightPanelMode('chat');
  }
</script>

<!-- Panel content - no tab bar, switching done via toolbar icons -->
<div class="h-full overflow-hidden">
  {#if mode === 'chat'}
    <ChatPanel {onOpenAuth} />
  {:else if mode === 'versions'}
    <VersionsPanel />
  {:else if mode === 'pending'}
    <PendingChangesPanel onClose={closePendingPanel} />
  {:else if mode === 'context'}
    <ContextPanel />
  {/if}
</div>
