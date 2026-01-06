<script lang="ts">
  // Home page - redirect to editor or show landing
</script>

<main class="flex flex-col items-center justify-center min-h-screen p-8">
  <div class="max-w-2xl text-center space-y-6">
    <h1 class="text-5xl font-bold tracking-tight">
      Midlight
    </h1>
    <p class="text-xl text-muted-foreground">
      AI-native document editor for the web
    </p>

    <div class="flex gap-4 justify-center mt-8">
      <a
        href="/editor"
        class="inline-flex items-center justify-center rounded-md bg-primary px-6 py-3 text-sm font-medium text-primary-foreground shadow transition-colors hover:bg-primary/90"
      >
        Open Editor
      </a>
      <a
        href="https://midlight.ai"
        target="_blank"
        rel="noopener noreferrer"
        class="inline-flex items-center justify-center rounded-md border border-input bg-background px-6 py-3 text-sm font-medium shadow-sm transition-colors hover:bg-accent hover:text-accent-foreground"
      >
        Download Desktop App
      </a>
    </div>

    <p class="text-sm text-muted-foreground mt-8">
      Your documents are stored locally in your browser. No account required.
    </p>
  </div>
</main>
