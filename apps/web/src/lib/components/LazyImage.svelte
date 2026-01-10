<script lang="ts">
  import { onMount } from 'svelte';

  interface Props {
    src: string;
    alt: string;
    class?: string;
    width?: number;
    height?: number;
    placeholder?: string;
    threshold?: number;
  }

  let {
    src,
    alt,
    class: className = '',
    width,
    height,
    placeholder = 'data:image/svg+xml,%3Csvg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"%3E%3C/svg%3E',
    threshold = 0.1,
  }: Props = $props();

  let imgElement: HTMLImageElement;
  let isLoaded = $state(false);
  let isInView = $state(false);
  let hasError = $state(false);

  onMount(() => {
    // Use Intersection Observer for lazy loading
    if ('IntersectionObserver' in window) {
      const observer = new IntersectionObserver(
        (entries) => {
          entries.forEach((entry) => {
            if (entry.isIntersecting) {
              isInView = true;
              observer.disconnect();
            }
          });
        },
        { threshold, rootMargin: '50px' }
      );

      observer.observe(imgElement);

      return () => observer.disconnect();
    } else {
      // Fallback for older browsers
      isInView = true;
    }
  });

  function handleLoad() {
    isLoaded = true;
  }

  function handleError() {
    hasError = true;
  }
</script>

<div
  class="lazy-image-container relative overflow-hidden {className}"
  style:width={width ? `${width}px` : undefined}
  style:height={height ? `${height}px` : undefined}
>
  <!-- Placeholder/skeleton -->
  {#if !isLoaded && !hasError}
    <div
      class="absolute inset-0 bg-muted animate-pulse"
      aria-hidden="true"
    />
  {/if}

  <!-- Actual image -->
  <img
    bind:this={imgElement}
    src={isInView ? src : placeholder}
    {alt}
    {width}
    {height}
    loading="lazy"
    decoding="async"
    class="transition-opacity duration-300 {isLoaded ? 'opacity-100' : 'opacity-0'}"
    onload={handleLoad}
    onerror={handleError}
  />

  <!-- Error state -->
  {#if hasError}
    <div class="absolute inset-0 flex items-center justify-center bg-muted text-muted-foreground">
      <svg class="w-8 h-8" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
      </svg>
    </div>
  {/if}
</div>

<style>
  .lazy-image-container img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
