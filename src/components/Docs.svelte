<!--
  @component
  Inputs: open bindable flag.
  Outputs: in-app docs dialog with Guides + Cookbook navigation.
-->
<script>
  import { DOC_PAGES, docSections, getDocPage, renderDocMarkdown } from "$lib/docs.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Button } from "$lib/components/ui/button/index.js";

  let { open = $bindable(false) } = $props();

  let pageId = $state("overview");

  const page = $derived(getDocPage(pageId) ?? DOC_PAGES[0]);
  const html = $derived(renderDocMarkdown(page?.body));
  const sections = docSections();

  /**
   * Inputs: doc page id.
   * Outputs: selected page for the content pane.
   */
  function selectPage(id) {
    pageId = id;
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    class="sm:max-w-4xl w-[min(920px,calc(100%-2rem))] h-[min(80vh,720px)] max-h-[90vh] grid-rows-[auto_1fr] gap-4 overflow-hidden p-0"
    showCloseButton={true}
  >
    <Dialog.Header class="border-b border-border px-6 py-4 pr-14">
      <Dialog.Title>Docs</Dialog.Title>
    </Dialog.Header>
    <div class="grid min-h-0 grid-cols-[12rem_1fr] max-sm:grid-cols-1">
      <nav
        class="border-border overflow-y-auto border-r px-3 py-3 max-sm:border-r-0 max-sm:border-b"
        aria-label="Docs"
      >
        {#each sections as section (section)}
          <p
            class="text-muted-foreground px-2 pt-2 pb-1 text-[11px] font-medium tracking-wide uppercase first:pt-0"
          >
            {section}
          </p>
          <ul class="mb-2 grid gap-0.5 p-0 m-0 list-none">
            {#each DOC_PAGES.filter((p) => p.section === section) as doc (doc.id)}
              <li>
                <Button
                  variant={pageId === doc.id ? "secondary" : "ghost"}
                  size="sm"
                  class="h-8 w-full justify-start px-2 font-normal"
                  onclick={() => selectPage(doc.id)}
                >
                  {doc.title}
                </Button>
              </li>
            {/each}
          </ul>
        {/each}
      </nav>
      <div class="docs-prose text-foreground overflow-y-auto px-6 py-4">
        {@html html}
      </div>
    </div>
  </Dialog.Content>
</Dialog.Root>

<style>
  .docs-prose :global(:first-child) {
    margin-top: 0;
  }
  .docs-prose :global(h1) {
    font-size: 1.35rem;
    font-weight: 600;
    letter-spacing: -0.02em;
    margin: 0 0 0.75rem;
  }
  .docs-prose :global(h2) {
    font-size: 1.05rem;
    font-weight: 600;
    margin: 1.5rem 0 0.5rem;
  }
  .docs-prose :global(h3) {
    font-size: 0.95rem;
    font-weight: 600;
    margin: 1.25rem 0 0.4rem;
  }
  .docs-prose :global(p),
  .docs-prose :global(ul),
  .docs-prose :global(ol) {
    margin: 0 0 0.75rem;
    line-height: 1.55;
  }
  .docs-prose :global(ul),
  .docs-prose :global(ol) {
    padding-left: 1.25rem;
  }
  .docs-prose :global(li) {
    margin: 0.2rem 0;
  }
  .docs-prose :global(a) {
    color: var(--color-primary, inherit);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .docs-prose :global(code) {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.85em;
    background: color-mix(in oklab, var(--color-muted, #888) 35%, transparent);
    padding: 0.1em 0.35em;
    border-radius: 0.3rem;
  }
  .docs-prose :global(pre) {
    margin: 0 0 0.9rem;
    padding: 0.75rem 1rem;
    overflow-x: auto;
    border-radius: 0.75rem;
    background: color-mix(in oklab, var(--color-muted, #888) 28%, transparent);
    box-shadow: 0 0 0 1px
      color-mix(in oklab, var(--color-foreground) 10%, transparent);
  }
  .docs-prose :global(pre code) {
    background: none;
    padding: 0;
    font-size: 0.8rem;
    line-height: 1.45;
  }
  .docs-prose :global(table) {
    width: 100%;
    border-collapse: collapse;
    margin: 0 0 0.9rem;
    font-size: 0.85rem;
  }
  .docs-prose :global(th),
  .docs-prose :global(td) {
    border: 1px solid color-mix(in oklab, var(--color-foreground) 12%, transparent);
    padding: 0.4rem 0.55rem;
    text-align: left;
    vertical-align: top;
  }
  .docs-prose :global(th) {
    font-weight: 600;
  }
</style>
