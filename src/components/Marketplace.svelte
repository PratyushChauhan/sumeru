<!--
  @component
  Inputs: servers list, refresh callback.
  Outputs: DCR marketplace browse, one-click install, and extra accounts per provider.
-->
<script>
  import * as api from "$lib/api.js";
  import {
    filterMarketplaceEntries,
    installedServers,
    listMarketplaceEntries,
    nameTaken,
    nextAccountName,
  } from "$lib/marketplace.js";
  import { marketplaceIcon } from "$lib/marketplace-icons.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import ProviderIcon from "./ProviderIcon.svelte";

  let { servers = [], refresh } = $props();

  const entries = listMarketplaceEntries();
  let query = $state("");
  let installingId = $state("");
  let msg = $state("");
  let msgTone = $state("");
  let addOpen = $state(false);
  let adding = $state(null);
  let accountName = $state("");

  const visible = $derived(filterMarketplaceEntries(entries, query));

  /**
   * Inputs: catalog entry. Outputs: account-name dialog prefilled with the next free name.
   */
  function onAddAccount(entry) {
    adding = entry;
    accountName = nextAccountName(entry, servers);
    msg = "";
    addOpen = true;
  }

  /**
   * Inputs: catalog entry, server name.
   * Outputs: OAuth-first install, or the account dialog when the name is taken (checked before sign-in).
   */
  async function onInstall(entry, name) {
    if (installingId) return;
    if (nameTaken(servers, name)) {
      onAddAccount(entry);
      msg = `Another MCP is already named “${name}”. Pick a different name.`;
      msgTone = "err";
      return;
    }
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
        name,
        enabled: true,
        transport: {
          kind: "http",
          url: entry.url,
          header_keys: [],
          has_bearer: true,
        },
        secrets: {},
      });
      addOpen = false;
      msg = `${name} installed`;
      msgTone = "ok";
      try {
        await refresh();
      } catch {
        /* keep success; a stale list only re-offers Install */
      }
    } catch (e) {
      msg = String(e);
      msgTone = "err";
    } finally {
      installingId = "";
    }
  }
</script>

<Card.Root size="sm">
  <Card.Header
    class="flex-row items-center justify-between gap-3 space-y-0 pb-0"
  >
    <Card.Title class="text-base">Marketplace</Card.Title>
    <Input
      class="h-7 max-w-[11rem] text-xs"
      type="search"
      placeholder="Search"
      bind:value={query}
      autocomplete="off"
    />
  </Card.Header>
  <Card.Content class="grid gap-2">
    {#if msg}
      <p
        aria-live="polite"
        role="status"
        class="text-xs m-0 {msgTone === 'err'
          ? 'text-destructive'
          : msgTone === 'ok'
            ? 'text-ok'
            : 'text-muted-foreground'}"
      >
        {msg}
      </p>
    {/if}
    <ul class="m-0 p-0 list-none grid gap-1.5 sm:grid-cols-2">
      {#if !visible.length}
        <li class="text-muted-foreground text-xs sm:col-span-2">
          No matches. Add advanced MCPs under Configure.
        </li>
      {:else}
        {#each visible as entry (entry.id)}
          {@const count = installedServers(entry, servers).length}
          {@const busy = installingId === entry.id}
          <li
            class="flex items-center gap-2 rounded-lg border bg-card px-2 py-1.5"
          >
            <ProviderIcon src={marketplaceIcon(entry.id)} name={entry.name} />
            <div class="min-w-0 flex-1">
              <div class="truncate text-sm font-medium leading-tight">
                {entry.name}
              </div>
              <div class="text-muted-foreground truncate text-[11px] leading-tight">
                {entry.description}
              </div>
            </div>
            {#if count}
              <span class="text-muted-foreground text-[11px] whitespace-nowrap">
                {count} installed
              </span>
              <Button
                variant="outline"
                size="xs"
                disabled={!!installingId}
                onclick={() => onAddAccount(entry)}
              >
                {busy ? "…" : "Add account"}
              </Button>
            {:else}
              <Button
                size="xs"
                disabled={!!installingId}
                onclick={() => onInstall(entry, entry.name)}
              >
                {busy ? "…" : "Install"}
              </Button>
            {/if}
          </li>
        {/each}
      {/if}
    </ul>
  </Card.Content>
</Card.Root>

<Dialog.Root bind:open={addOpen}>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title>Add another {adding?.name} account</Dialog.Title>
    </Dialog.Header>
    <form
      class="grid gap-3"
      onsubmit={(e) => {
        e.preventDefault();
        onInstall(adding, accountName.trim());
      }}
    >
      <div class="grid gap-2">
        <Label for="account-name">Account name</Label>
        <Input id="account-name" bind:value={accountName} required />
        <p class="text-muted-foreground text-xs m-0">
          Agents see this name in <code>list_mcps</code>. Sumeru asks
          {adding?.name} for a fresh login; if you land in the same account,
          sign out of {adding?.name} in your browser first.
        </p>
      </div>
      {#if msgTone === "err"}
        <p role="alert" class="text-destructive text-sm m-0">{msg}</p>
      {/if}
      <Dialog.Footer class="gap-2 sm:justify-end">
        <Button type="button" variant="ghost" onclick={() => (addOpen = false)}>
          Cancel
        </Button>
        <Button type="submit" disabled={!!installingId}>
          {installingId ? "…" : "Sign in"}
        </Button>
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>
