<!--
  @component
  Inputs: servers list, edit/refresh callbacks.
  Outputs: MCP list actions.
-->
<script>
  import * as api from "$lib/api.js";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import { marketplaceIcon } from "$lib/marketplace-icons.js";
  import { groupSummary, serverGroups } from "$lib/marketplace.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import ProviderIcon from "./ProviderIcon.svelte";

  let { servers = [], onEdit, refresh } = $props();

  /**
   * Inputs: server. Outputs: test alert.
   */
  async function onTest(server) {
    try {
      alert(await api.testServer(server.id));
    } catch (e) {
      alert(String(e));
    }
  }

  /**
   * Inputs: server. Outputs: delete + refresh when confirmed.
   */
  async function onDelete(server) {
    if (!confirm(`Delete ${server.name}?`)) return;
    try {
      await api.removeServer(server.id);
      await refresh();
    } catch (e) {
      alert(String(e));
    }
  }

  /**
   * Inputs: server. Outputs: detail line for the list meta.
   */
  function detail(server) {
    const kind = server.transport.kind;
    return kind === "stdio"
      ? `${server.transport.command} ${(server.transport.args || []).join(" ")}`
      : server.transport.url;
  }

</script>

<Card.Root>
  <Card.Header class="flex-row items-center justify-between gap-4 space-y-0">
    <Card.Title>MCPs</Card.Title>
    <Button onclick={() => onEdit(null)}>Add MCP</Button>
  </Card.Header>
  <Card.Content>
    <ul class="m-0 p-0 list-none grid gap-2.5">
      {#if !servers.length}
        <li class="text-muted-foreground text-sm">
          No MCPs yet. Add a stdio command or paste an MCP URL.
        </li>
      {:else}
        {#each serverGroups(servers) as group (group.id)}
          <li class="rounded-xl border bg-card">
            <details class="group">
              <summary class="flex cursor-pointer list-none items-center gap-2.5 p-3">
                <ChevronRightIcon class="size-4 shrink-0 text-muted-foreground transition-transform group-open:rotate-90" />
                <ProviderIcon src={marketplaceIcon(group.id)} name={group.name} />
                <div class="min-w-0">
                  <strong class="block font-semibold">{group.name}</strong>
                  <div class="text-muted-foreground text-xs">{groupSummary(group.servers)}</div>
                </div>
              </summary>
              <ul class="m-0 list-none grid gap-2 pb-3 pl-17 pr-3">
                {#each group.servers as server (server.id)}
                  <li
                    class="flex items-center justify-between gap-3 border-t pt-2 max-sm:flex-col max-sm:items-stretch"
                  >
                    <div class="min-w-0">
                      <strong class="block text-sm font-semibold">{server.name}</strong>
                      <div class="text-muted-foreground text-xs truncate">
                        {server.transport.kind} · {server.enabled ? "enabled" : "disabled"} · {detail(server)}
                      </div>
                    </div>
                    <div class="flex gap-2 flex-wrap">
                      <Button variant="outline" size="sm" onclick={() => onTest(server)}>Test</Button>
                      <Button variant="outline" size="sm" onclick={() => onEdit(server)}>Edit</Button>
                      <Button variant="destructive" size="sm" onclick={() => onDelete(server)}>Delete</Button>
                    </div>
                  </li>
                {/each}
              </ul>
            </details>
          </li>
        {/each}
      {/if}
    </ul>
  </Card.Content>
</Card.Root>
