<!--
  @component
  Inputs: open (bindable), key (null = new), servers, onSaved callback.
  Outputs: create/update an API key and which MCPs it can reach.
-->
<script>
  import * as api from "$lib/api.js";
  import { keyAllows, scopeIds } from "$lib/keys.js";
  import { serverGroups } from "$lib/marketplace.js";
  import { marketplaceIcon } from "$lib/marketplace-icons.js";
  import ProviderIcon from "./ProviderIcon.svelte";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Tabs from "$lib/components/ui/tabs/index.js";

  let { open = $bindable(false), key = null, servers = [], onSaved } = $props();

  let name = $state("");
  let mode = $state("exclude");
  /** Ids of MCPs the key can reach (switch on). */
  let allowed = $state([]);
  let msg = $state("");

  $effect(() => {
    if (!open) return;
    name = key?.name ?? "";
    mode = key?.mode ?? "exclude";
    allowed = servers.filter((s) => !key || keyAllows(key, s.id)).map((s) => s.id);
    msg = "";
  });

  /**
   * Inputs: mcp ids (one MCP or a whole app), switch state. Outputs: updated reachable set.
   */
  function toggle(ids, on) {
    allowed = [...allowed.filter((x) => !ids.includes(x)), ...(on ? ids : [])];
  }

  /**
   * Inputs: submit event. Outputs: saved key + refresh, or inline error.
   */
  async function onSubmit(e) {
    e.preventDefault();
    try {
      await api.upsertKey({
        id: key?.id ?? null,
        name,
        mode,
        mcpIds: scopeIds(mode, allowed, servers),
      });
      open = false;
      await onSaved();
    } catch (err) {
      msg = String(err);
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-md max-h-[90vh] overflow-y-auto">
    <Dialog.Header>
      <Dialog.Title>{key ? `Edit ${key.name}` : "Add key"}</Dialog.Title>
    </Dialog.Header>
    <form class="grid gap-3" onsubmit={onSubmit}>
      <div class="grid gap-2">
        <Label for="key-name">Name</Label>
        <Input id="key-name" bind:value={name} placeholder="e.g. work repo" required />
      </div>
      <div class="grid gap-2">
        <Label>New MCPs</Label>
        <Tabs.Root bind:value={mode}>
          <Tabs.List>
            <Tabs.Trigger value="exclude">Allowed</Tabs.Trigger>
            <Tabs.Trigger value="include">Blocked</Tabs.Trigger>
          </Tabs.List>
        </Tabs.Root>
        <p class="text-muted-foreground text-xs m-0">
          {mode === "exclude"
            ? "All MCPs except the ones switched off — MCPs you add later are reachable."
            : "Only the MCPs switched on — MCPs you add later stay blocked."}
        </p>
      </div>
      <div class="grid gap-2">
        <Label>Reachable MCPs</Label>
        {#each serverGroups(servers) as g (g.id)}
          {@const ids = g.servers.map((s) => s.id)}
          {@const n = ids.filter((id) => allowed.includes(id)).length}
          <details class="group rounded-xl border">
            <summary class="flex cursor-pointer list-none items-center gap-2.5 p-2.5">
              <ChevronRightIcon class="size-4 shrink-0 text-muted-foreground transition-transform group-open:rotate-90" />
              <ProviderIcon src={marketplaceIcon(g.id)} name={g.name} />
              <div class="min-w-0 flex-1">
                <div class="text-sm font-medium">{g.name}</div>
                <div class="text-muted-foreground text-xs">{n} of {ids.length} reachable</div>
              </div>
              <Switch
                aria-label={`All ${g.name} MCPs`}
                checked={n === ids.length}
                onCheckedChange={(on) => toggle(ids, on)}
              />
            </summary>
            <div class="grid gap-2 border-t py-2.5 pr-2.5 pl-16">
              {#each g.servers as s (s.id)}
                <div class="flex items-center gap-3">
                  <Switch
                    id={`key-mcp-${s.id}`}
                    checked={allowed.includes(s.id)}
                    onCheckedChange={(on) => toggle([s.id], on)}
                  />
                  <Label for={`key-mcp-${s.id}`} class="font-normal">{s.name}</Label>
                </div>
              {/each}
            </div>
          </details>
        {:else}
          <p class="text-muted-foreground text-xs m-0">No MCPs yet.</p>
        {/each}
      </div>
      {#if msg}
        <p role="alert" class="text-destructive text-sm m-0">{msg}</p>
      {/if}
      <Dialog.Footer class="gap-2 sm:justify-end">
        <Button type="button" variant="ghost" onclick={() => (open = false)}>
          Cancel
        </Button>
        <Button type="submit">Save</Button>
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>
