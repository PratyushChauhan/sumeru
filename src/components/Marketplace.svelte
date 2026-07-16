<script>
  import * as api from "$lib/api.js";
  import {
    filterMarketplaceEntries,
    isInstalled,
    listMarketplaceEntries,
    marketplaceHost,
  } from "$lib/marketplace.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import * as Card from "$lib/components/ui/card/index.js";

  /**
   * Inputs: servers list, refresh callback.
   * Outputs: DCR marketplace browse + one-click install.
   */
  let { servers = [], refresh } = $props();

  const entries = listMarketplaceEntries();
  let query = $state("");
  let installingId = $state("");
  let msg = $state("");
  let msgTone = $state("");

  const visible = $derived(filterMarketplaceEntries(entries, query));

  /**
   * Inputs: catalog entry. Outputs: OAuth-first install or error message.
   */
  async function onInstall(entry) {
    if (installingId || isInstalled(entry, servers)) return;
    installingId = entry.id;
    msg = "Opening browser sign-in…";
    msgTone = "";
    const id = crypto.randomUUID();
    try {
      const probe = await api.probeMcpAuth({ url: entry.url, id });
      if (!probe?.oauth || !probe.supports_dcr) {
        throw new Error(
          "This server no longer supports automatic registration. Add it from Configure instead.",
        );
      }
      await api.startMcpOauth({ url: entry.url, id });
      await api.upsertServer({
        id,
        name: entry.name,
        enabled: true,
        transport: {
          kind: "http",
          url: entry.url,
          header_keys: [],
          has_bearer: true,
        },
        secrets: {},
      });
      await refresh();
      msg = `${entry.name} installed`;
      msgTone = "ok";
    } catch (e) {
      msg = String(e);
      msgTone = "err";
    } finally {
      installingId = "";
    }
  }
</script>

<Card.Root>
  <Card.Header class="gap-3 space-y-0">
    <div class="flex items-center justify-between gap-4 max-sm:flex-col max-sm:items-stretch">
      <div>
        <Card.Title>Marketplace</Card.Title>
        <p class="text-muted-foreground text-xs m-0 mt-1">
          One-click install for servers that register automatically (DCR).
        </p>
      </div>
      <Input
        class="max-w-xs max-sm:max-w-none"
        type="search"
        placeholder="Search MCPs"
        bind:value={query}
        autocomplete="off"
      />
    </div>
  </Card.Header>
  <Card.Content>
    <p
      class="min-h-5 text-sm m-0 mb-2.5 {msgTone === 'err'
        ? 'text-destructive'
        : msgTone === 'ok'
          ? 'text-ok'
          : 'text-muted-foreground'}"
    >
      {msg}
    </p>
    <ul class="m-0 p-0 list-none grid gap-2.5">
      {#if !visible.length}
        <li class="text-muted-foreground text-sm">
          No matches. Advanced MCPs can still be added under Configure.
        </li>
      {:else}
        {#each visible as entry (entry.id)}
          {@const installed = isInstalled(entry, servers)}
          {@const busy = installingId === entry.id}
          <li
            class="flex items-center justify-between gap-3 rounded-xl border bg-card p-3 max-sm:flex-col max-sm:items-stretch"
          >
            <div class="min-w-0">
              <strong class="block font-semibold">{entry.name}</strong>
              <div class="text-muted-foreground text-xs mt-0.5">
                {entry.description}
              </div>
              <div class="text-muted-foreground text-xs truncate mt-1">
                {marketplaceHost(entry.url)}
              </div>
            </div>
            <div class="flex gap-2 flex-wrap shrink-0">
              {#if installed}
                <Button variant="outline" size="sm" disabled>Installed</Button>
              {:else}
                <Button
                  size="sm"
                  disabled={!!installingId}
                  onclick={() => onInstall(entry)}
                >
                  {busy ? "Signing in…" : "Install"}
                </Button>
              {/if}
            </div>
          </li>
        {/each}
      {/if}
    </ul>
  </Card.Content>
</Card.Root>
