<script lang="ts">
  import { onMount } from 'svelte';
  import { editor, fileSystem, activeFile, isSaving, ui, settings, hasPendingChanges, ai } from '@midlight/stores';
  import { open } from '@tauri-apps/plugin-dialog';
  import { readFile } from '@tauri-apps/plugin-fs';
  import { invoke } from '@tauri-apps/api/core';

  // Toolbar state
  let showTextStyleMenu = $state(false);
  let showFontFamilyMenu = $state(false);
  let showFontSizeMenu = $state(false);
  let showTextColorMenu = $state(false);
  let showHighlightMenu = $state(false);
  let showAlignmentMenu = $state(false);
  let showOverflowMenu = $state(false);

  // Toolbar overflow state
  let toolbarRef: HTMLDivElement | null = $state(null);
  let groupRefs: Record<string, HTMLDivElement | null> = $state({});
  let groupWidths: Record<string, number> = $state({});
  let visibleGroupIds = $state<string[]>([]);
  let overflowGroupIds = $state<string[]>([]);
  let measurementDone = $state(false);

  // Font families available (Merriweather is default)
  const fontFamilies = [
    { name: 'Merriweather', value: 'Merriweather, Georgia, serif' },
    { name: 'System', value: 'system-ui, -apple-system, sans-serif' },
    { name: 'Georgia', value: 'Georgia, serif' },
    { name: 'Arial', value: 'Arial, sans-serif' },
    { name: 'Times', value: 'Times New Roman, serif' },
    { name: 'Courier', value: 'Courier New, monospace' },
  ];

  // Use settings store for font family
  const currentFontFamily = $derived($settings.fontFamily);

  // AI annotations visibility
  let annotationsVisible = $state(true);

  function toggleAnnotations() {
    annotationsVisible = !annotationsVisible;
    ai.setAnnotationsVisible(annotationsVisible);

    // Toggle CSS class on the editor element to show/hide annotations
    const editorElement = document.querySelector('.tiptap');
    if (editorElement) {
      editorElement.classList.toggle('hide-annotations', !annotationsVisible);
    }
  }

  // Toolbar groups with priorities (reordered to match Electron)
  type ToolbarGroupId = 'textStyle' | 'fontFamily' | 'fontSize' | 'format' | 'textColor' | 'highlight' | 'alignment' | 'media';

  interface ToolbarGroup {
    id: ToolbarGroupId;
    priority: 'high' | 'medium' | 'low';
  }

  // Priority order: high = stays visible longest, low = first to overflow
  const toolbarGroups: ToolbarGroup[] = [
    { id: 'textStyle', priority: 'high' },
    { id: 'fontFamily', priority: 'high' },
    { id: 'fontSize', priority: 'high' },
    { id: 'format', priority: 'high' },
    { id: 'textColor', priority: 'medium' },
    { id: 'highlight', priority: 'medium' },
    { id: 'alignment', priority: 'medium' },
    { id: 'media', priority: 'medium' },
  ];

  // Calculate which groups are visible vs overflow
  function recalculateOverflow() {
    if (!toolbarRef || !measurementDone) return;

    const containerWidth = toolbarRef.getBoundingClientRect().width;
    const rightSideWidth = 280; // status + snapshot + save + history + AI + spacing
    const moreButtonWidth = 44;
    let availableWidth = containerWidth - rightSideWidth;

    // Sort by priority (maintain original order within same priority)
    const priorityOrder = { high: 0, medium: 1, low: 2 };
    const sortedGroups = [...toolbarGroups].sort((a, b) => {
      return priorityOrder[a.priority] - priorityOrder[b.priority];
    });

    const visible: ToolbarGroupId[] = [];
    const overflow: ToolbarGroupId[] = [];
    let usedWidth = 0;
    let hasOverflow = false;

    for (const group of sortedGroups) {
      const width = groupWidths[group.id] || 0;
      const effectiveAvailable = hasOverflow ? availableWidth - moreButtonWidth : availableWidth;

      if (usedWidth + width <= effectiveAvailable) {
        visible.push(group.id);
        usedWidth += width;
      } else {
        overflow.push(group.id);
        hasOverflow = true;
      }
    }

    // Restore original order for visible groups
    const originalOrder = toolbarGroups.map(g => g.id);
    visibleGroupIds = visible.sort((a, b) => originalOrder.indexOf(a) - originalOrder.indexOf(b));
    overflowGroupIds = overflow.sort((a, b) => originalOrder.indexOf(a) - originalOrder.indexOf(b));
  }

  // Measure group widths after mount
  function measureGroups() {
    for (const group of toolbarGroups) {
      const ref = groupRefs[group.id];
      if (ref) {
        groupWidths[group.id] = ref.getBoundingClientRect().width;
      }
    }
    measurementDone = true;
    // Initially show all groups
    visibleGroupIds = toolbarGroups.map(g => g.id);
    overflowGroupIds = [];
    // Then calculate overflow
    recalculateOverflow();
  }

  const fontSizes = ['12px', '14px', '16px', '18px', '20px', '24px', '28px', '32px', '36px'];
  const textColors = [
    '#000000', '#374151', '#6B7280', '#9CA3AF',
    '#DC2626', '#EA580C', '#D97706', '#CA8A04',
    '#16A34A', '#059669', '#0891B2', '#0284C7',
    '#2563EB', '#4F46E5', '#7C3AED', '#9333EA',
  ];
  const highlightColors = [
    'transparent', '#FEF08A', '#FDE68A', '#FED7AA',
    '#FECACA', '#E9D5FF', '#C7D2FE', '#BFDBFE',
    '#A5F3FC', '#99F6E4', '#BBF7D0', '#D9F99D',
  ];

  // Set up ResizeObserver for toolbar after it's mounted
  let resizeObserver: ResizeObserver | null = null;

  $effect(() => {
    if (toolbarRef && !resizeObserver) {
      // Measure toolbar groups after DOM is ready
      requestAnimationFrame(() => {
        measureGroups();
      });

      // Set up ResizeObserver
      resizeObserver = new ResizeObserver(() => {
        recalculateOverflow();
      });
      resizeObserver.observe(toolbarRef);
    }

    return () => {
      resizeObserver?.disconnect();
      resizeObserver = null;
    };
  });

  // Close dropdown menus when clicking outside
  function handleClickOutside(event: MouseEvent) {
    const target = event.target as HTMLElement;
    if (!target.closest('.text-style-menu')) showTextStyleMenu = false;
    if (!target.closest('.font-family-menu')) showFontFamilyMenu = false;
    if (!target.closest('.font-size-menu')) showFontSizeMenu = false;
    if (!target.closest('.text-color-menu')) showTextColorMenu = false;
    if (!target.closest('.highlight-menu')) showHighlightMenu = false;
    if (!target.closest('.alignment-menu')) showAlignmentMenu = false;
    if (!target.closest('.overflow-menu')) showOverflowMenu = false;
  }

  // Get current text style label for the dropdown
  function getCurrentTextStyle(): string {
    if ($editor?.isActive('heading', { level: 1 })) return 'Heading 1';
    if ($editor?.isActive('heading', { level: 2 })) return 'Heading 2';
    if ($editor?.isActive('heading', { level: 3 })) return 'Heading 3';
    if ($editor?.isActive('heading', { level: 4 })) return 'Heading 4';
    if ($editor?.isActive('heading', { level: 5 })) return 'Heading 5';
    if ($editor?.isActive('bulletList')) return 'Bullet list';
    if ($editor?.isActive('orderedList')) return 'Numbered list';
    if ($editor?.isActive('blockquote')) return 'Quote';
    if ($editor?.isActive('codeBlock')) return 'Code block';
    return 'Normal text';
  }

  function setTextStyle(style: string) {
    if (!$editor) return;

    // First, clear any existing block formatting
    $editor.chain().focus().clearNodes().run();

    switch (style) {
      case 'heading1':
        $editor.chain().focus().toggleHeading({ level: 1 }).run();
        break;
      case 'heading2':
        $editor.chain().focus().toggleHeading({ level: 2 }).run();
        break;
      case 'heading3':
        $editor.chain().focus().toggleHeading({ level: 3 }).run();
        break;
      case 'heading4':
        $editor.chain().focus().toggleHeading({ level: 4 }).run();
        break;
      case 'heading5':
        $editor.chain().focus().toggleHeading({ level: 5 }).run();
        break;
      case 'bulletList':
        $editor.chain().focus().toggleBulletList().run();
        break;
      case 'orderedList':
        $editor.chain().focus().toggleOrderedList().run();
        break;
      case 'quote':
        $editor.chain().focus().toggleBlockquote().run();
        break;
      case 'codeBlock':
        $editor.chain().focus().toggleCodeBlock().run();
        break;
      // 'normal' - already cleared nodes above
    }
    showTextStyleMenu = false;
  }

  function setFontFamily(name: string, value: string) {
    // Save to settings store (persisted)
    settings.setFontFamily(name);
    // Apply font family to the editor content
    const pageContainer = document.querySelector('.page-container');
    if (pageContainer) {
      (pageContainer as HTMLElement).style.fontFamily = value;
    }
    showFontFamilyMenu = false;
  }

  // Apply font family when settings change or on mount
  function applyFontFamily(fontName: string) {
    const font = fontFamilies.find(f => f.name === fontName);
    if (font) {
      const pageContainer = document.querySelector('.page-container');
      if (pageContainer) {
        (pageContainer as HTMLElement).style.fontFamily = font.value;
      }
    }
  }

  // Watch for settings changes
  $effect(() => {
    applyFontFamily($settings.fontFamily);
  });

  function getCurrentTextColor(): string {
    return $editor?.getAttributes('textStyle').color || '#000000';
  }

  function getCurrentHighlightColor(): string {
    const highlight = $editor?.getAttributes('highlight');
    return highlight?.color || 'transparent';
  }

  async function triggerManualSave() {
    if ($activeFile) {
      await fileSystem.save('manual');
    }
  }

  async function createSnapshot() {
    // TODO: Implement snapshot/version creation
    console.log('Create snapshot');
  }

  // Helper to check if a group is visible
  function isGroupVisible(id: string): boolean {
    return visibleGroupIds.includes(id);
  }

  // Toolbar actions
  function toggleBold() {
    $editor?.chain().focus().toggleBold().run();
  }

  function toggleItalic() {
    $editor?.chain().focus().toggleItalic().run();
  }

  function toggleUnderline() {
    $editor?.chain().focus().toggleUnderline().run();
  }

  function toggleStrike() {
    $editor?.chain().focus().toggleStrike().run();
  }

  function toggleHeading(level: 1 | 2 | 3) {
    $editor?.chain().focus().toggleHeading({ level }).run();
  }

  function toggleBulletList() {
    $editor?.chain().focus().toggleBulletList().run();
  }

  function toggleOrderedList() {
    $editor?.chain().focus().toggleOrderedList().run();
  }

  function toggleBlockquote() {
    $editor?.chain().focus().toggleBlockquote().run();
  }

  function toggleCode() {
    $editor?.chain().focus().toggleCode().run();
  }

  function toggleCodeBlock() {
    $editor?.chain().focus().toggleCodeBlock().run();
  }

  function setTextAlign(align: 'left' | 'center' | 'right') {
    $editor?.chain().focus().setTextAlign(align).run();
  }

  function setFontSize(size: string) {
    $editor?.chain().focus().setFontSize(size).run();
    showFontSizeMenu = false;
  }

  function setTextColor(color: string) {
    $editor?.chain().focus().setTextColor(color).run();
    showTextColorMenu = false;
  }

  function setHighlightColor(color: string) {
    if (color === 'transparent') {
      $editor?.chain().focus().unsetHighlight().run();
    } else {
      $editor?.chain().focus().toggleHighlight({ color }).run();
    }
    showHighlightMenu = false;
  }

  function insertHorizontalRule() {
    $editor?.chain().focus().setHorizontalRule().run();
  }

  function clearFormatting() {
    $editor?.chain().focus().clearNodes().unsetAllMarks().run();
  }

  function undo() {
    $editor?.chain().focus().undo().run();
  }

  function redo() {
    $editor?.chain().focus().redo().run();
  }

  function isActive(name: string, attrs?: Record<string, unknown>): boolean {
    return $editor?.isActive(name, attrs) ?? false;
  }

  function getCurrentFontSize(): string {
    const attrs = $editor?.getAttributes('textStyle');
    return attrs?.fontSize || '16px';
  }

  async function insertImage() {
    try {
      // Open file dialog to select image
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: 'Images',
            extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'],
          },
        ],
      });

      if (!selected || Array.isArray(selected)) return;

      // Read image file
      const imageData = await readFile(selected);

      // Determine mime type from extension
      const ext = selected.split('.').pop()?.toLowerCase() || 'png';
      const mimeTypes: Record<string, string> = {
        png: 'image/png',
        jpg: 'image/jpeg',
        jpeg: 'image/jpeg',
        gif: 'image/gif',
        webp: 'image/webp',
        svg: 'image/svg+xml',
      };
      const mimeType = mimeTypes[ext] || 'image/png';

      // Convert to base64 data URL
      const base64 = btoa(
        Array.from(imageData)
          .map((b) => String.fromCharCode(b))
          .join('')
      );
      const dataUrl = `data:${mimeType};base64,${base64}`;

      // Get workspace root
      const workspaceRoot = $fileSystem.rootDir;
      if (!workspaceRoot) {
        console.error('No workspace root');
        return;
      }

      // Save image to workspace
      const result = await invoke<{ refId: string; success: boolean; error?: string }>(
        'workspace_save_image',
        {
          workspaceRoot,
          dataUrl,
          originalName: selected.split('/').pop(),
        }
      );

      if (!result.success) {
        console.error('Failed to save image:', result.error);
        return;
      }

      // Insert image node into editor
      // For now, use the data URL directly; in production, load from ref
      $editor
        ?.chain()
        .focus()
        .setImage({ src: dataUrl, alt: '', title: result.refId })
        .run();
    } catch (error) {
      console.error('Failed to insert image:', error);
    }
  }
</script>

<svelte:window onclick={handleClickOutside} />

<div bind:this={toolbarRef} class="relative flex flex-nowrap items-center gap-1 p-2 border-b border-border bg-card shrink-0">
    <!-- Formatting controls wrapper - disabled when no file -->
    <div class="contents {!$activeFile ? 'opacity-50 pointer-events-none' : ''}">
    <!-- Text Style dropdown (Tt icon - headings, lists, etc.) -->
    <div bind:this={groupRefs['textStyle']} class="relative text-style-menu {!isGroupVisible('textStyle') && measurementDone ? 'hidden' : ''}">
      <button onclick={() => showTextStyleMenu = !showTextStyleMenu} class="h-7 px-2 rounded hover:bg-accent flex items-center gap-0.5" title="Text Style" disabled={!$activeFile}>
        <span class="text-sm font-medium">Tt</span>
        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="opacity-60"><path d="M6 9l6 6 6-6"/></svg>
      </button>
      {#if showTextStyleMenu}
        <div class="absolute top-full left-0 mt-1 bg-popover border border-border rounded-md shadow-lg z-50 py-1 min-w-[180px]">
          <!-- Normal text -->
          <button onclick={() => setTextStyle('normal')} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm flex items-center gap-3 {getCurrentTextStyle() === 'Normal text' ? 'bg-accent' : ''}">
            <span class="w-5 text-center text-muted-foreground">T</span>
            <span>Normal text</span>
          </button>
          <div class="h-px bg-border my-1"></div>
          <!-- Headings -->
          <button onclick={() => setTextStyle('heading1')} class="w-full px-3 py-1.5 text-left hover:bg-accent flex items-center gap-3 {getCurrentTextStyle() === 'Heading 1' ? 'bg-accent' : ''}">
            <span class="w-5 text-center text-muted-foreground text-xs">H1</span>
            <span class="text-lg font-bold">Heading 1</span>
          </button>
          <button onclick={() => setTextStyle('heading2')} class="w-full px-3 py-1.5 text-left hover:bg-accent flex items-center gap-3 {getCurrentTextStyle() === 'Heading 2' ? 'bg-accent' : ''}">
            <span class="w-5 text-center text-muted-foreground text-xs">H2</span>
            <span class="text-base font-bold">Heading 2</span>
          </button>
          <button onclick={() => setTextStyle('heading3')} class="w-full px-3 py-1.5 text-left hover:bg-accent flex items-center gap-3 {getCurrentTextStyle() === 'Heading 3' ? 'bg-accent' : ''}">
            <span class="w-5 text-center text-muted-foreground text-xs">H3</span>
            <span class="text-sm font-bold">Heading 3</span>
          </button>
          <button onclick={() => setTextStyle('heading4')} class="w-full px-3 py-1.5 text-left hover:bg-accent flex items-center gap-3 {getCurrentTextStyle() === 'Heading 4' ? 'bg-accent' : ''}">
            <span class="w-5 text-center text-muted-foreground text-xs">H4</span>
            <span class="text-sm font-semibold">Heading 4</span>
          </button>
          <button onclick={() => setTextStyle('heading5')} class="w-full px-3 py-1.5 text-left hover:bg-accent flex items-center gap-3 {getCurrentTextStyle() === 'Heading 5' ? 'bg-accent' : ''}">
            <span class="w-5 text-center text-muted-foreground text-xs">H5</span>
            <span class="text-xs font-semibold">Heading 5</span>
          </button>
          <div class="h-px bg-border my-1"></div>
          <!-- Lists -->
          <button onclick={() => setTextStyle('bulletList')} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm flex items-center gap-3 {getCurrentTextStyle() === 'Bullet list' ? 'bg-accent' : ''}">
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="text-muted-foreground"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/></svg>
            <span>Bullet list</span>
          </button>
          <button onclick={() => setTextStyle('orderedList')} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm flex items-center gap-3 {getCurrentTextStyle() === 'Numbered list' ? 'bg-accent' : ''}">
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="text-muted-foreground"><line x1="10" y1="6" x2="21" y2="6"/><line x1="10" y1="12" x2="21" y2="12"/><line x1="10" y1="18" x2="21" y2="18"/><path d="M4 6h1v4"/><path d="M4 10h2"/><path d="M6 18H4c0-1 2-2 2-3s-1-1.5-2-1"/></svg>
            <span>Numbered list</span>
          </button>
          <div class="h-px bg-border my-1"></div>
          <!-- Quote -->
          <button onclick={() => setTextStyle('quote')} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm flex items-center gap-3 {getCurrentTextStyle() === 'Quote' ? 'bg-accent' : ''}">
            <span class="w-5 text-center text-muted-foreground text-lg leading-none">"</span>
            <span>Quote</span>
          </button>
          <!-- Code block -->
          <button onclick={() => setTextStyle('codeBlock')} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm flex items-center gap-3 {getCurrentTextStyle() === 'Code block' ? 'bg-accent' : ''}">
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="text-muted-foreground"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>
            <span>Code block</span>
          </button>
        </div>
      {/if}
    </div>

    <!-- Font Family (name only, no icon) - fixed width to prevent UI shift -->
    <div bind:this={groupRefs['fontFamily']} class="relative font-family-menu ml-1 {!isGroupVisible('fontFamily') && measurementDone ? 'hidden' : ''}">
      <button onclick={() => showFontFamilyMenu = !showFontFamilyMenu} class="h-7 px-1.5 rounded hover:bg-accent text-sm flex items-center gap-0.5 w-[100px]" title="Font Family">
        <span class="flex-1 text-left truncate">{currentFontFamily}</span>
        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="opacity-60 shrink-0"><path d="M6 9l6 6 6-6"/></svg>
      </button>
      {#if showFontFamilyMenu}
        <div class="absolute top-full left-0 mt-1 bg-popover border border-border rounded-md shadow-lg z-50 py-1 min-w-[140px]">
          {#each fontFamilies as font}
            <button onclick={() => setFontFamily(font.name, font.value)} class="w-full px-3 py-1.5 text-left hover:bg-accent text-sm {currentFontFamily === font.name ? 'bg-accent' : ''}" style="font-family: {font.value}">{font.name}</button>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Font Size -->
    <div bind:this={groupRefs['fontSize']} class="relative font-size-menu ml-1 border-r border-border/50 pr-2 {!isGroupVisible('fontSize') && measurementDone ? 'hidden' : ''}">
      <button onclick={() => showFontSizeMenu = !showFontSizeMenu} class="h-7 px-1.5 rounded hover:bg-accent text-sm flex items-center gap-0.5" title="Font Size">
        {getCurrentFontSize().replace('px', '')}
        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="opacity-60"><path d="M6 9l6 6 6-6"/></svg>
      </button>
      {#if showFontSizeMenu}
        <div class="absolute top-full left-0 mt-1 bg-popover border border-border rounded-md shadow-lg z-50 py-1 min-w-[60px]">
          {#each fontSizes as size}
            <button onclick={() => setFontSize(size)} class="w-full px-3 py-1 text-left hover:bg-accent text-sm {getCurrentFontSize() === size ? 'bg-accent' : ''}">{size.replace('px', '')}</button>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Text formatting (B, I) with dropdown arrows like Electron -->
    <div bind:this={groupRefs['format']} class="flex items-center ml-2 border-r border-border/50 pr-2 {!isGroupVisible('format') && measurementDone ? 'hidden' : ''}">
      <button onclick={toggleBold} class="h-7 px-1 rounded hover:bg-accent flex items-center {isActive('bold') ? 'bg-accent' : ''}" title="Bold">
        <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4h8a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z"/><path d="M6 12h9a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z"/></svg>
        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="opacity-40 ml-0.5"><path d="M6 9l6 6 6-6"/></svg>
      </button>
      <button onclick={toggleItalic} class="h-7 px-1 rounded hover:bg-accent flex items-center {isActive('italic') ? 'bg-accent' : ''}" title="Italic">
        <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="4" x2="10" y2="4"/><line x1="14" y1="20" x2="5" y2="20"/><line x1="15" y1="4" x2="9" y2="20"/></svg>
        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="opacity-40 ml-0.5"><path d="M6 9l6 6 6-6"/></svg>
      </button>
    </div>

    <!-- Text color with indicator bar (no dropdown arrow like Electron) -->
    <div bind:this={groupRefs['textColor']} class="relative text-color-menu ml-2 {!isGroupVisible('textColor') && measurementDone ? 'hidden' : ''}">
      <button onclick={() => showTextColorMenu = !showTextColorMenu} class="h-7 w-7 rounded hover:bg-accent flex flex-col items-center justify-center" title="Text Color">
        <span class="font-bold text-sm leading-none" style="color: {getCurrentTextColor()}">A</span>
        <!-- Color indicator bar -->
        <div class="w-4 h-1 rounded-sm" style="background-color: {getCurrentTextColor()}"></div>
      </button>
      {#if showTextColorMenu}
        <div class="absolute top-full left-0 mt-1 bg-popover border border-border rounded-md shadow-lg z-50 p-2 w-[160px]">
          <div class="grid grid-cols-4 gap-1">
            {#each textColors as color}
              <button onclick={() => setTextColor(color)} class="w-8 h-8 rounded border border-border hover:scale-110 transition-transform" style="background-color: {color}" title={color}></button>
            {/each}
          </div>
        </div>
      {/if}
    </div>

    <!-- Highlight with indicator bar (no dropdown arrow like Electron) -->
    <div bind:this={groupRefs['highlight']} class="relative highlight-menu {!isGroupVisible('highlight') && measurementDone ? 'hidden' : ''}">
      <button onclick={() => showHighlightMenu = !showHighlightMenu} class="h-7 w-7 rounded hover:bg-accent flex flex-col items-center justify-center" title="Highlight">
        <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 11-6 6v3h9l3-3"/><path d="m22 12-4.6 4.6a2 2 0 0 1-2.8 0l-5.2-5.2a2 2 0 0 1 0-2.8L14 4"/></svg>
        <!-- Color indicator bar -->
        <div class="w-4 h-1 rounded-sm" style="background-color: {getCurrentHighlightColor() === 'transparent' ? '#FEF08A' : getCurrentHighlightColor()}"></div>
      </button>
      {#if showHighlightMenu}
        <div class="absolute top-full left-0 mt-1 bg-popover border border-border rounded-md shadow-lg z-50 p-2 w-[160px]">
          <div class="grid grid-cols-4 gap-1">
            {#each highlightColors as color}
              <button onclick={() => setHighlightColor(color)} class="w-8 h-8 rounded border border-border hover:scale-110 transition-transform {color === 'transparent' ? 'relative' : ''}" style="background-color: {color === 'transparent' ? '#fff' : color}" title={color === 'transparent' ? 'Remove highlight' : color}>
                {#if color === 'transparent'}<span class="absolute inset-0 flex items-center justify-center text-red-500 text-lg">×</span>{/if}
              </button>
            {/each}
          </div>
        </div>
      {/if}
    </div>

    <!-- Alignment dropdown -->
    <div bind:this={groupRefs['alignment']} class="relative alignment-menu ml-2 border-r border-border/50 pr-2 {!isGroupVisible('alignment') && measurementDone ? 'hidden' : ''}">
      <button onclick={() => showAlignmentMenu = !showAlignmentMenu} class="h-7 px-1 rounded hover:bg-accent flex items-center" title="Alignment">
        <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="17" y1="10" x2="3" y2="10"/><line x1="21" y1="6" x2="3" y2="6"/><line x1="21" y1="14" x2="3" y2="14"/><line x1="17" y1="18" x2="3" y2="18"/></svg>
        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="opacity-40 ml-0.5"><path d="M6 9l6 6 6-6"/></svg>
      </button>
      {#if showAlignmentMenu}
        <div class="absolute top-full left-0 mt-1 bg-popover border border-border rounded-md shadow-lg z-50 p-1">
          <div class="flex items-center gap-0.5">
            <button onclick={() => { setTextAlign('left'); showAlignmentMenu = false; }} class="p-1.5 rounded hover:bg-accent {isActive('paragraph', { textAlign: 'left' }) ? 'bg-accent' : ''}" title="Align Left">
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="17" y1="10" x2="3" y2="10"/><line x1="21" y1="6" x2="3" y2="6"/><line x1="21" y1="14" x2="3" y2="14"/><line x1="17" y1="18" x2="3" y2="18"/></svg>
            </button>
            <button onclick={() => { setTextAlign('center'); showAlignmentMenu = false; }} class="p-1.5 rounded hover:bg-accent {isActive('paragraph', { textAlign: 'center' }) ? 'bg-accent' : ''}" title="Align Center">
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="10" x2="6" y2="10"/><line x1="21" y1="6" x2="3" y2="6"/><line x1="21" y1="14" x2="3" y2="14"/><line x1="18" y1="18" x2="6" y2="18"/></svg>
            </button>
            <button onclick={() => { setTextAlign('right'); showAlignmentMenu = false; }} class="p-1.5 rounded hover:bg-accent {isActive('paragraph', { textAlign: 'right' }) ? 'bg-accent' : ''}" title="Align Right">
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="21" y1="10" x2="7" y2="10"/><line x1="21" y1="6" x2="3" y2="6"/><line x1="21" y1="14" x2="3" y2="14"/><line x1="21" y1="18" x2="7" y2="18"/></svg>
            </button>
          </div>
        </div>
      {/if}
    </div>

    <!-- Media (Image, HR) - always visible like Electron -->
    <div bind:this={groupRefs['media']} class="flex items-center ml-2 {!isGroupVisible('media') && measurementDone ? 'hidden' : ''}">
      <button onclick={insertImage} class="h-7 w-7 rounded hover:bg-accent flex items-center justify-center" title="Insert Image">
        <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>
      </button>
      <button onclick={insertHorizontalRule} class="h-7 w-7 rounded hover:bg-accent flex items-center justify-center" title="Horizontal Rule">
        <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="12" x2="21" y2="12"/></svg>
      </button>
    </div>

    <!-- Overflow Menu -->
    {#if overflowGroupIds.length > 0}
      <div class="relative overflow-menu">
        <button onclick={() => showOverflowMenu = !showOverflowMenu} class="p-1.5 rounded hover:bg-accent" title="More options">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/></svg>
        </button>
        {#if showOverflowMenu}
          <div class="absolute top-full right-0 mt-1 bg-popover border border-border rounded-lg shadow-xl z-50 p-2 min-w-[280px] max-h-[400px] overflow-y-auto">
            <div class="flex flex-col gap-1">
              {#each overflowGroupIds as groupId}
                <div class="flex items-center gap-1 p-1 rounded hover:bg-accent/50">
                  {#if groupId === 'textStyle'}
                    <div class="flex flex-col gap-1 w-full">
                      <span class="text-xs text-muted-foreground">Text Style:</span>
                      <div class="flex flex-wrap gap-1">
                        <button onclick={() => setTextStyle('normal')} class="px-2 py-1 text-xs rounded hover:bg-accent {getCurrentTextStyle() === 'Normal text' ? 'bg-accent font-medium' : ''}">Normal</button>
                        <button onclick={() => setTextStyle('heading1')} class="px-2 py-1 text-xs rounded hover:bg-accent {getCurrentTextStyle() === 'Heading 1' ? 'bg-accent font-medium' : ''}">H1</button>
                        <button onclick={() => setTextStyle('heading2')} class="px-2 py-1 text-xs rounded hover:bg-accent {getCurrentTextStyle() === 'Heading 2' ? 'bg-accent font-medium' : ''}">H2</button>
                        <button onclick={() => setTextStyle('heading3')} class="px-2 py-1 text-xs rounded hover:bg-accent {getCurrentTextStyle() === 'Heading 3' ? 'bg-accent font-medium' : ''}">H3</button>
                        <button onclick={() => setTextStyle('bulletList')} class="px-2 py-1 text-xs rounded hover:bg-accent {getCurrentTextStyle() === 'Bullet list' ? 'bg-accent font-medium' : ''}">Bullet</button>
                        <button onclick={() => setTextStyle('orderedList')} class="px-2 py-1 text-xs rounded hover:bg-accent {getCurrentTextStyle() === 'Numbered list' ? 'bg-accent font-medium' : ''}">Numbered</button>
                        <button onclick={() => setTextStyle('quote')} class="px-2 py-1 text-xs rounded hover:bg-accent {getCurrentTextStyle() === 'Quote' ? 'bg-accent font-medium' : ''}">Quote</button>
                        <button onclick={() => setTextStyle('codeBlock')} class="px-2 py-1 text-xs rounded hover:bg-accent {getCurrentTextStyle() === 'Code block' ? 'bg-accent font-medium' : ''}">Code</button>
                      </div>
                    </div>
                  {:else if groupId === 'fontFamily'}
                    <div class="flex items-center gap-1 flex-wrap">
                      <span class="text-xs text-muted-foreground">Font:</span>
                      {#each fontFamilies as font}
                        <button onclick={() => setFontFamily(font.name, font.value)} class="px-2 py-1 text-xs rounded hover:bg-accent {currentFontFamily === font.name ? 'bg-accent font-medium' : ''}">{font.name}</button>
                      {/each}
                    </div>
                  {:else if groupId === 'fontSize'}
                    <div class="flex items-center gap-1 flex-wrap">
                      <span class="text-xs text-muted-foreground">Size:</span>
                      {#each fontSizes as size}
                        <button onclick={() => setFontSize(size)} class="px-2 py-1 text-xs rounded hover:bg-accent {getCurrentFontSize() === size ? 'bg-accent font-medium' : ''}">{size.replace('px', '')}</button>
                      {/each}
                    </div>
                  {:else if groupId === 'format'}
                    <button onclick={toggleBold} class="p-1.5 rounded hover:bg-accent {isActive('bold') ? 'bg-accent' : ''}" title="Bold">
                      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4h8a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z"/><path d="M6 12h9a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z"/></svg>
                    </button>
                    <button onclick={toggleItalic} class="p-1.5 rounded hover:bg-accent {isActive('italic') ? 'bg-accent' : ''}" title="Italic">
                      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="19" y1="4" x2="10" y2="4"/><line x1="14" y1="20" x2="5" y2="20"/><line x1="15" y1="4" x2="9" y2="20"/></svg>
                    </button>
                  {:else if groupId === 'textColor'}
                    <div class="flex flex-col gap-1">
                      <span class="text-xs text-muted-foreground">Text Color:</span>
                      <div class="grid grid-cols-8 gap-1">
                        {#each textColors as color}
                          <button onclick={() => setTextColor(color)} class="w-5 h-5 rounded border border-border hover:scale-110 transition-transform" style="background-color: {color}" title={color}></button>
                        {/each}
                      </div>
                    </div>
                  {:else if groupId === 'highlight'}
                    <div class="flex flex-col gap-1">
                      <span class="text-xs text-muted-foreground">Highlight:</span>
                      <div class="grid grid-cols-6 gap-1">
                        {#each highlightColors as color}
                          <button onclick={() => setHighlightColor(color)} class="w-5 h-5 rounded border border-border hover:scale-110 transition-transform {color === 'transparent' ? 'relative' : ''}" style="background-color: {color === 'transparent' ? '#fff' : color}" title={color === 'transparent' ? 'Remove' : color}>
                            {#if color === 'transparent'}<span class="absolute inset-0 flex items-center justify-center text-red-500 text-xs">×</span>{/if}
                          </button>
                        {/each}
                      </div>
                    </div>
                  {:else if groupId === 'alignment'}
                    <button onclick={() => setTextAlign('left')} class="p-1.5 rounded hover:bg-accent {isActive('paragraph', { textAlign: 'left' }) ? 'bg-accent' : ''}" title="Align Left">
                      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="17" y1="10" x2="3" y2="10"/><line x1="21" y1="6" x2="3" y2="6"/><line x1="21" y1="14" x2="3" y2="14"/><line x1="17" y1="18" x2="3" y2="18"/></svg>
                    </button>
                    <button onclick={() => setTextAlign('center')} class="p-1.5 rounded hover:bg-accent {isActive('paragraph', { textAlign: 'center' }) ? 'bg-accent' : ''}" title="Align Center">
                      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="10" x2="6" y2="10"/><line x1="21" y1="6" x2="3" y2="6"/><line x1="21" y1="14" x2="3" y2="14"/><line x1="18" y1="18" x2="6" y2="18"/></svg>
                    </button>
                    <button onclick={() => setTextAlign('right')} class="p-1.5 rounded hover:bg-accent {isActive('paragraph', { textAlign: 'right' }) ? 'bg-accent' : ''}" title="Align Right">
                      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="21" y1="10" x2="7" y2="10"/><line x1="21" y1="6" x2="3" y2="6"/><line x1="21" y1="14" x2="3" y2="14"/><line x1="21" y1="18" x2="7" y2="18"/></svg>
                    </button>
                  {:else if groupId === 'media'}
                    <button onclick={insertImage} class="p-1.5 rounded hover:bg-accent" title="Insert Image">
                      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>
                    </button>
                    <button onclick={insertHorizontalRule} class="p-1.5 rounded hover:bg-accent" title="Horizontal Rule">
                      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="3" y1="12" x2="21" y2="12"/></svg>
                    </button>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    {/if}
    </div><!-- End formatting controls wrapper -->

    <!-- Spacer -->
    <div class="flex-1"></div>

    <!-- Save status with cloud icon (like Electron) - only shown when file is open -->
    {#if $activeFile}
    <div class="flex items-center gap-1.5 text-xs text-muted-foreground px-2 shrink-0">
      {#if $isSaving}
        <!-- Cloud with arrow (uploading) -->
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="animate-pulse">
          <path d="M4 14.899A7 7 0 1 1 15.71 8h1.79a4.5 4.5 0 0 1 2.5 8.242"/>
          <path d="M12 12v9"/>
          <path d="m16 16-4-4-4 4"/>
        </svg>
        <span>Saving...</span>
      {:else if $fileSystem.isDirty}
        <!-- Cloud with dot (unsaved) -->
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-amber-500">
          <path d="M4 14.899A7 7 0 1 1 15.71 8h1.79a4.5 4.5 0 0 1 2.5 8.242"/>
          <circle cx="12" cy="17" r="2" fill="currentColor"/>
        </svg>
        <span>Unsaved</span>
      {:else}
        <!-- Cloud with checkmark (saved) -->
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-green-500">
          <path d="M4 14.899A7 7 0 1 1 15.71 8h1.79a4.5 4.5 0 0 1 2.5 8.242"/>
          <path d="m9 15 2 2 4-4"/>
        </svg>
        <span>Saved</span>
      {/if}
    </div>
    {/if}

    <!-- Action buttons (like Electron's right side) -->
    <div class="flex items-center gap-0.5 border-l border-border pl-2 shrink-0">
      {#if $activeFile}
      <!-- Snapshot/Bookmark button -->
      <button onclick={createSnapshot} class="p-1.5 rounded hover:bg-accent text-muted-foreground hover:text-foreground" title="Create Snapshot">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/>
          <circle cx="12" cy="13" r="3"/>
        </svg>
      </button>

      <!-- Manual save button -->
      <button onclick={triggerManualSave} class="p-1.5 rounded hover:bg-accent text-muted-foreground hover:text-foreground disabled:opacity-50" title="Save" disabled={!$fileSystem.isDirty}>
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/>
          <polyline points="17 21 17 13 7 13 7 21"/>
          <polyline points="7 3 7 8 15 8"/>
        </svg>
      </button>
      {/if}

      <!-- Versions/History toggle -->
      <button onclick={() => ui.togglePanelMode('versions')} class="p-1.5 rounded hover:bg-accent {$ui.rightPanelMode === 'versions' ? 'bg-accent text-accent-foreground' : 'text-muted-foreground hover:text-foreground'}" title="Version History">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
      </button>

      <!-- Pending Changes toggle -->
      <button onclick={() => ui.togglePanelMode('pending')} class="p-1.5 rounded hover:bg-accent relative {$ui.rightPanelMode === 'pending' ? 'bg-accent text-accent-foreground' : 'text-muted-foreground hover:text-foreground'}" title="Pending Changes">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/>
          <path d="M14 2v4a2 2 0 0 0 2 2h4"/>
          <path d="M9 15h6"/>
          <path d="M12 18v-6"/>
        </svg>
        {#if $hasPendingChanges}
          <span class="absolute -top-0.5 -right-0.5 w-2 h-2 bg-yellow-500 rounded-full"></span>
        {/if}
      </button>

      <!-- AI Annotations toggle -->
      <button onclick={toggleAnnotations} class="p-1.5 rounded hover:bg-accent {annotationsVisible ? 'bg-accent/50 text-blue-400' : 'text-muted-foreground hover:text-foreground'}" title={annotationsVisible ? 'Hide AI Annotations' : 'Show AI Annotations'}>
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
          <path d="m12 6-.5 1.5a1 1 0 0 1-.6.6L9 9l1.9.6a1 1 0 0 1 .6.6l.5 1.8.5-1.8a1 1 0 0 1 .6-.6L15 9l-1.9-.6a1 1 0 0 1-.6-.6L12 6Z"/>
        </svg>
      </button>

      <!-- AI Chat toggle (sparkle icon like Electron) -->
      <button onclick={() => ui.togglePanelMode('chat')} class="p-1.5 rounded hover:bg-accent {$ui.rightPanelMode === 'chat' ? 'bg-accent text-accent-foreground' : 'text-muted-foreground hover:text-foreground'}" title="AI Assistant">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1 1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275L12 3Z"/>
          <path d="M5 3v4"/>
          <path d="M19 17v4"/>
          <path d="M3 5h4"/>
          <path d="M17 19h4"/>
        </svg>
      </button>
    </div>
  </div>
