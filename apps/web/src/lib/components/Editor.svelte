<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { Editor } from '@tiptap/core';
  import StarterKit from '@tiptap/starter-kit';
  import Placeholder from '@tiptap/extension-placeholder';
  import TextAlign from '@tiptap/extension-text-align';
  import Underline from '@tiptap/extension-underline';
  import { fileSystem, activeFile } from '@midlight/stores';
  import { debounce } from '@midlight/core/utils';
  import type { TiptapDocument } from '@midlight/core/types';

  let element: HTMLDivElement;
  let editor: Editor | null = null;

  // Debounced save function
  const debouncedSave = debounce(async () => {
    const content = editor?.getJSON();
    if (content) {
      await fileSystem.save('interval');
    }
  }, 3000);

  onMount(() => {
    editor = new Editor({
      element,
      extensions: [
        StarterKit.configure({
          heading: {
            levels: [1, 2, 3, 4, 5, 6],
          },
        }),
        Placeholder.configure({
          placeholder: 'Start writing...',
        }),
        TextAlign.configure({
          types: ['heading', 'paragraph'],
        }),
        Underline,
      ],
      content: $fileSystem.editorContent || '',
      editorProps: {
        attributes: {
          class: 'tiptap prose dark:prose-invert max-w-none focus:outline-none min-h-full',
        },
      },
      onUpdate: ({ editor }) => {
        fileSystem.setEditorContent(editor.getJSON() as TiptapDocument);
        fileSystem.setIsDirty(true);
        debouncedSave();
      },
    });
  });

  onDestroy(() => {
    editor?.destroy();
  });

  // React to content changes from store (e.g., when switching files)
  $effect(() => {
    if (editor && $fileSystem.editorContent) {
      const currentJson = JSON.stringify(editor.getJSON());
      const newJson = JSON.stringify($fileSystem.editorContent);

      if (currentJson !== newJson) {
        editor.commands.setContent($fileSystem.editorContent);
      }
    }
  });

  // Clear editor when no file is active
  $effect(() => {
    if (editor && !$activeFile) {
      editor.commands.clearContent();
    }
  });

  // Keyboard shortcuts
  function handleKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === 's') {
      e.preventDefault();
      fileSystem.save('interval');
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="h-full overflow-auto">
  {#if $activeFile}
    <div class="max-w-3xl mx-auto p-8">
      <div
        bind:this={element}
        class="min-h-[calc(100vh-8rem)]"
      ></div>
    </div>
  {:else}
    <div class="flex items-center justify-center h-full text-muted-foreground">
      <div class="text-center space-y-4">
        <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" class="mx-auto opacity-50">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
          <polyline points="14 2 14 8 20 8"/>
          <line x1="16" y1="13" x2="8" y2="13"/>
          <line x1="16" y1="17" x2="8" y2="17"/>
          <polyline points="10 9 9 9 8 9"/>
        </svg>
        <p class="text-lg">Select a document to start editing</p>
        <p class="text-sm">Or create a new one from the sidebar</p>
      </div>
    </div>
  {/if}
</div>
