<script lang="ts">
  import { onDestroy } from 'svelte';
  import { Editor } from '@tiptap/core';
  import StarterKit from '@tiptap/starter-kit';
  import Placeholder from '@tiptap/extension-placeholder';
  import Underline from '@tiptap/extension-underline';
  import TextAlign from '@tiptap/extension-text-align';
  import TextStyle from '@tiptap/extension-text-style';
  import { fileSystem, activeFile, editor as editorStore, ai, inlineEditState, stagedEdit, hasStagedEdit } from '@midlight/stores';
  import type { TiptapDocument } from '@midlight/core/types';
  import InlineEditPrompt from './Editor/InlineEditPrompt.svelte';
  import InlineDiff from './Editor/InlineDiff.svelte';
  import AnnotationPopover from './Editor/AnnotationPopover.svelte';
  import StagedEditToolbar from './Editor/StagedEditToolbar.svelte';
  import type { AIAnnotationAttributes } from '@midlight/ui';

  // Import custom extensions from @midlight/ui
  import {
    FontSize,
    TextColor,
    TextHighlight,
    CustomCode,
    ClickableHorizontalRule,
    ResizableImage,
    DiffAdded,
    DiffRemoved,
    AIAnnotation,
  } from '@midlight/ui';

  let element: HTMLDivElement | undefined = $state(undefined);
  let editor: Editor | null = $state(null);
  let saveTimeout: ReturnType<typeof setTimeout> | null = null;

  // Inline edit state
  let showPrompt = $state(false);
  let promptPosition = $state<{ x: number; y: number }>({ x: 0, y: 0 });
  let selectedTextForEdit = $state('');
  let selectionRange = $state<{ from: number; to: number }>({ from: 0, to: 0 });

  // Derived: show diff when we have a suggestion
  const showDiff = $derived($inlineEditState.isActive && $inlineEditState.suggestedText !== '');

  // Annotation popover state
  interface AnnotationPopoverState {
    position: { x: number; y: number };
    attrs: AIAnnotationAttributes;
  }
  let annotationPopover = $state<AnnotationPopoverState | null>(null);

  // Handle Cmd+K keyboard shortcut
  function handleKeyDown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key === 'k') {
      event.preventDefault();

      if (!editor || editor.isDestroyed) return;

      const { from, to } = editor.state.selection;

      // Only trigger if there's a selection
      if (from === to) return;

      const selectedText = editor.state.doc.textBetween(from, to, ' ');
      if (!selectedText.trim()) return;

      // Get position for the floating prompt
      const coords = editor.view.coordsAtPos(from);

      selectedTextForEdit = selectedText;
      selectionRange = { from, to };
      promptPosition = { x: coords.left, y: coords.bottom + 8 };
      showPrompt = true;
    }
  }

  // Handle prompt submission
  async function handlePromptSubmit(instruction: string) {
    showPrompt = false;

    // Start inline edit in the store
    ai.startInlineEdit(promptPosition, selectedTextForEdit, selectionRange.from, selectionRange.to);

    // Send the edit request to the LLM
    await ai.sendInlineEditRequest(instruction);
  }

  // Handle prompt cancel
  function handlePromptCancel() {
    showPrompt = false;
  }

  // Handle accept suggestion
  function handleAcceptSuggestion() {
    if (!editor || editor.isDestroyed) return;

    const suggestedText = ai.acceptInlineEdit();
    const { from } = selectionRange;
    const newTo = from + suggestedText.length;

    // Apply the edit to the editor and add AI annotation
    editor
      .chain()
      .focus()
      .setTextSelection({ from: selectionRange.from, to: selectionRange.to })
      .insertContent(suggestedText)
      // Select the inserted text to apply annotation
      .setTextSelection({ from, to: newTo })
      .setAIAnnotation({
        conversationId: '',
        messageId: '',
        type: 'edit',
        tooltip: 'Edited with inline AI (Cmd+K)',
      })
      // Move cursor to end of insertion
      .setTextSelection(newTo)
      .run();

    fileSystem.setIsDirty(true);
  }

  // Handle reject suggestion
  function handleRejectSuggestion() {
    ai.cancelInlineEdit();
  }

  // Initialize editor when element becomes available
  $effect(() => {
    if (!element || editor) return;

    editor = new Editor({
      element,
      extensions: [
        StarterKit.configure({
          // Disable built-in code to use CustomCode instead
          code: false,
          horizontalRule: false,
        }),
        Placeholder.configure({
          placeholder: 'Start writing...',
        }),
        Underline,
        TextAlign.configure({
          types: ['heading', 'paragraph'],
        }),
        TextStyle,
        FontSize,
        TextColor,
        TextHighlight,
        CustomCode,
        ClickableHorizontalRule,
        ResizableImage,
        DiffAdded,
        DiffRemoved,
        AIAnnotation,
      ],
      content: $fileSystem.editorContent || '',
      editorProps: {
        attributes: {
          class: 'tiptap prose prose-sm sm:prose lg:prose-lg xl:prose-xl max-w-none focus:outline-none',
        },
      },
      onUpdate: ({ editor: ed }) => {
        const json = ed.getJSON() as TiptapDocument;
        fileSystem.setEditorContent(json);
        fileSystem.setIsDirty(true);

        // Debounced auto-save
        if (saveTimeout) clearTimeout(saveTimeout);
        saveTimeout = setTimeout(async () => {
          if ($activeFile) {
            await fileSystem.save('interval');
          }
        }, 3000);
      },
    });

    // Set editor in store for Toolbar to use
    editorStore.set(editor);
  });

  // Cleanup on destroy
  onDestroy(() => {
    if (saveTimeout) clearTimeout(saveTimeout);
    editorStore.set(null);
    editor?.destroy();
  });

  // Update editor content when active file changes or content is reloaded
  $effect(() => {
    const content = $fileSystem.editorContent;
    const revision = $fileSystem.contentRevision; // Watch revision to force updates
    if (editor && content && !editor.isDestroyed) {
      const currentContent = editor.getJSON();
      // Update if content is different OR revision changed (force reload from agent edits)
      if (JSON.stringify(currentContent) !== JSON.stringify(content)) {
        console.log('[Editor] Updating content, revision:', revision);
        editor.commands.setContent(content, false);
      }
    }
  });

  // Handle clicks on empty space to focus editor at end
  function handleContainerClick(event: MouseEvent) {
    if (!editor) return;

    const target = event.target as HTMLElement;

    // Check if clicking on an AI annotation
    const annotationElement = target.closest('[data-ai-annotation]') as HTMLElement | null;
    if (annotationElement) {
      event.stopPropagation();

      // Extract annotation attributes
      const attrs: AIAnnotationAttributes = {
        conversationId: annotationElement.getAttribute('data-conversation-id') || '',
        messageId: annotationElement.getAttribute('data-message-id') || '',
        type: (annotationElement.getAttribute('data-annotation-type') as AIAnnotationAttributes['type']) || 'edit',
        tooltip: annotationElement.getAttribute('data-tooltip') || undefined,
      };

      // Get position for the popover
      const rect = annotationElement.getBoundingClientRect();

      annotationPopover = {
        position: { x: rect.left, y: rect.bottom + 8 },
        attrs,
      };
      return;
    }

    // Close annotation popover if clicking elsewhere
    if (annotationPopover) {
      annotationPopover = null;
    }

    // If clicking directly on the container/page area (not on editor content),
    // focus the editor and move cursor to the end
    if (
      target.classList.contains('bg-canvas') ||
      target.classList.contains('page-container') ||
      target === element
    ) {
      editor.commands.focus('end');
    }
  }

  // Close annotation popover
  function closeAnnotationPopover() {
    annotationPopover = null;
  }

  // Handle going to conversation from annotation popover
  function handleGoToConversation() {
    // Open the chat panel via UI store
    // This will be wired up when we integrate with the UI
  }

  // Handle accept staged edit
  async function handleAcceptStagedEdit() {
    await fileSystem.acceptStagedEdit();
  }

  // Handle reject staged edit
  function handleRejectStagedEdit() {
    fileSystem.rejectStagedEdit();
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

<div class="h-full flex flex-col">
  {#if $activeFile}
    <!-- Editor Content -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="flex-1 overflow-auto bg-canvas p-8 cursor-text" onclick={handleContainerClick}>
      <div class="page-container mx-auto">
        <div bind:this={element} class="min-h-full"></div>
      </div>
    </div>

    <!-- Inline Edit Prompt (Cmd+K) -->
    {#if showPrompt}
      <InlineEditPrompt
        position={promptPosition}
        selectedText={selectedTextForEdit}
        onSubmit={handlePromptSubmit}
        onCancel={handlePromptCancel}
      />
    {/if}

    <!-- Inline Diff (AI Suggestion) -->
    {#if showDiff}
      <InlineDiff
        position={$inlineEditState.position || { x: 0, y: 0 }}
        originalText={$inlineEditState.selectedText}
        suggestedText={$inlineEditState.suggestedText}
        isStreaming={$inlineEditState.isGenerating}
        onAccept={handleAcceptSuggestion}
        onReject={handleRejectSuggestion}
      />
    {/if}

    <!-- Annotation Popover -->
    {#if annotationPopover}
      <AnnotationPopover
        position={annotationPopover.position}
        conversationId={annotationPopover.attrs.conversationId}
        messageId={annotationPopover.attrs.messageId}
        type={annotationPopover.attrs.type}
        tooltip={annotationPopover.attrs.tooltip}
        onClose={closeAnnotationPopover}
        onGoToConversation={handleGoToConversation}
      />
    {/if}

    <!-- Staged Edit Toolbar (Accept/Reject) -->
    {#if $hasStagedEdit}
      <StagedEditToolbar
        onAccept={handleAcceptStagedEdit}
        onReject={handleRejectStagedEdit}
      />
    {/if}
  {:else}
    <!-- No file selected -->
    <div class="flex-1 flex items-center justify-center text-muted-foreground">
      <div class="text-center">
        <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" class="mx-auto mb-4 opacity-50">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
          <path d="M14 2v6h6"/>
          <line x1="16" y1="13" x2="8" y2="13"/>
          <line x1="16" y1="17" x2="8" y2="17"/>
          <line x1="10" y1="9" x2="8" y2="9"/>
        </svg>
        <p>Select a file to start editing</p>
      </div>
    </div>
  {/if}
</div>

<style>
  :global(.tiptap) {
    min-height: 100%;
  }

  :global(.tiptap p.is-editor-empty:first-child::before) {
    color: var(--muted-foreground);
    opacity: 0.5;
    content: attr(data-placeholder);
    float: left;
    height: 0;
    pointer-events: none;
  }

  :global(.hr-wrapper) {
    padding: 0.5rem 0;
    cursor: pointer;
  }

  :global(.hr-wrapper hr) {
    border: none;
    border-top: 2px solid var(--border);
  }

  :global(.hr-wrapper:hover hr) {
    border-color: var(--primary);
  }

  :global(.ProseMirror-selectednode .hr-wrapper hr) {
    border-color: var(--primary);
    border-width: 3px;
  }

  :global(.resizable-image-wrapper) {
    margin: 1rem 0;
  }

  :global(.diff-added) {
    background-color: rgba(34, 197, 94, 0.3);
  }

  :global(.diff-removed) {
    background-color: rgba(239, 68, 68, 0.3);
    text-decoration: line-through;
  }

  :global(.ai-annotation) {
    cursor: pointer;
  }

  :global(.ai-annotation:hover) {
    filter: brightness(0.95);
  }

  /* Hide annotations when toggle is off */
  :global(.tiptap.hide-annotations .ai-annotation) {
    background-color: transparent !important;
    border-bottom: none !important;
    cursor: text;
  }

  :global(.tiptap.hide-annotations .ai-annotation:hover) {
    filter: none;
  }
</style>
