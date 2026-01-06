<script lang="ts">
  import { marked } from 'marked';

  interface Props {
    content: string;
    class?: string;
  }

  let { content, class: className = '' }: Props = $props();

  // Configure marked for safe rendering
  marked.setOptions({
    breaks: true, // Convert \n to <br>
    gfm: true, // GitHub Flavored Markdown
  });

  // Parse markdown to HTML
  const html = $derived(marked.parse(content) as string);
</script>

<div class="markdown-content {className}">
  {@html html}
</div>

<style>
  .markdown-content {
    line-height: 1.6;
  }

  .markdown-content :global(p) {
    margin: 0;
  }

  .markdown-content :global(p + p) {
    margin-top: 0.75em;
  }

  .markdown-content :global(h1),
  .markdown-content :global(h2),
  .markdown-content :global(h3),
  .markdown-content :global(h4),
  .markdown-content :global(h5),
  .markdown-content :global(h6) {
    margin-top: 1em;
    margin-bottom: 0.5em;
    font-weight: 600;
    line-height: 1.3;
  }

  .markdown-content :global(h1) { font-size: 1.5em; }
  .markdown-content :global(h2) { font-size: 1.3em; }
  .markdown-content :global(h3) { font-size: 1.1em; }

  .markdown-content :global(ul),
  .markdown-content :global(ol) {
    margin: 0.5em 0;
    padding-left: 1.5em;
  }

  .markdown-content :global(li) {
    margin: 0.25em 0;
  }

  .markdown-content :global(code) {
    background-color: var(--muted);
    padding: 0.15em 0.4em;
    border-radius: 0.25em;
    font-size: 0.9em;
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace;
  }

  .markdown-content :global(pre) {
    background-color: var(--muted);
    padding: 0.75em 1em;
    border-radius: 0.5em;
    overflow-x: auto;
    margin: 0.75em 0;
  }

  .markdown-content :global(pre code) {
    background: none;
    padding: 0;
    font-size: 0.85em;
  }

  .markdown-content :global(blockquote) {
    border-left: 3px solid var(--border);
    padding-left: 1em;
    margin: 0.75em 0;
    color: var(--muted-foreground);
  }

  .markdown-content :global(a) {
    color: var(--primary);
    text-decoration: underline;
  }

  .markdown-content :global(a:hover) {
    opacity: 0.8;
  }

  .markdown-content :global(hr) {
    border: none;
    border-top: 1px solid var(--border);
    margin: 1em 0;
  }

  .markdown-content :global(table) {
    border-collapse: collapse;
    width: 100%;
    margin: 0.75em 0;
  }

  .markdown-content :global(th),
  .markdown-content :global(td) {
    border: 1px solid var(--border);
    padding: 0.5em;
    text-align: left;
  }

  .markdown-content :global(th) {
    background-color: var(--muted);
    font-weight: 600;
  }

  .markdown-content :global(img) {
    max-width: 100%;
    height: auto;
    border-radius: 0.5em;
  }

  .markdown-content :global(strong) {
    font-weight: 600;
  }
</style>
