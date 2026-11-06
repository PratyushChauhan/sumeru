<!--
  @component
  Inputs: status, API keys, servers, autostart, refresh callback.
  Outputs: endpoint panel UI events (keys, agent setup, autostart).
-->
<script>
  import * as api from "$lib/api.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import { Separator } from "$lib/components/ui/separator/index.js";
  import * as Tabs from "$lib/components/ui/tabs/index.js";
  import { agentInstalls } from "$lib/agents.js";
  import { agentServerName, scopeLabel } from "$lib/keys.js";
  import KeyEditor from "./KeyEditor.svelte";

  let {
    status,
    keys = [],
    servers = [],
    autostart = $bindable(false),
    refresh,
  } = $props();

  let agent = $state("claude-code");
  let keyId = $state("default");
  let editorOpen = $state(false);
  let editing = $state(null);

  const selected = $derived(keys.find((k) => k.id === keyId) ?? keys[0]);

  /**
   * Inputs: bearer token or placeholder. Outputs: selected agent recipe for the selected key.
   */
  const install = (t) =>
    agentInstalls(status?.endpoint ?? "", t, agentServerName(selected)).find(
      (a) => a.id === agent,
    );
  const shown = $derived(install("<TOKEN>"));

  /**
   * Inputs: text. Outputs: clipboard write or user-facing error.
   */
  async function copy(text) {
    try {
      await navigator.clipboard.writeText(text);
    } catch (e) {
      alert(String(e));
    }
  }

  /**
   * Inputs: agent deeplink. Outputs: agent install prompt opened, or user-facing error.
   */
  async function openLink(url) {
    try {
      await api.openUrl(url);
    } catch (e) {
      alert(String(e));
    }
  }

  /**
   * Inputs: checked. Outputs: persisted autostart or revert.
   */
  async function onAutostart(checked) {
    try {
      await api.setAutostart(checked);
    } catch (err) {
      autostart = !checked;
      alert(String(err));
    }
  }

  /**
   * Inputs: key or null (new). Outputs: key editor opened.
   */
  function onEditKey(key) {
    editing = key;
    editorOpen = true;
  }

  /**
   * Inputs: key-changing API call. Outputs: refreshed keys, or user-facing error.
   */
  async function run(call) {
    try {
      await call();
      await refresh();
    } catch (e) {
      alert(String(e));
    }
  }
</script>

<Card.Root>
  <Card.Header>
    <Card.Title>Endpoint</Card.Title>
  </Card.Header>
  <Card.Content class="grid gap-4">
    <div class="grid gap-2">
      <Label for="endpoint-url">URL</Label>
      <div class="flex gap-2 flex-wrap">
        <Input id="endpoint-url" value={status?.endpoint ?? ""} readonly class="flex-1 min-w-48" />
        <Button variant="outline" onclick={() => copy(status?.endpoint ?? "")}>
          Copy
        </Button>
      </div>
    </div>
    <div class="grid gap-2">
      <div class="flex items-center justify-between gap-2">
        <Label>Keys</Label>
        <Button variant="outline" size="sm" onclick={() => onEditKey(null)}>Add key</Button>
      </div>
      <ul class="m-0 p-0 list-none grid gap-2">
        {#each keys as k (k.id)}
          <li
            class="flex items-center justify-between gap-3 rounded-xl border bg-card px-3 py-2 max-sm:flex-col max-sm:items-stretch"
          >
            <div class="min-w-0">
              <strong class="block text-sm font-semibold">{k.name}</strong>
              <div class="text-muted-foreground text-xs truncate">
                {scopeLabel(k, servers)} · installs as {agentServerName(k)}
              </div>
            </div>
            <div class="flex gap-2 flex-wrap">
              <Button variant="outline" size="sm" onclick={() => copy(k.token)}>Copy token</Button>
              <Button variant="outline" size="sm" onclick={() => run(() => api.rotateKey(k.id))}>
                Rotate
              </Button>
              <Button variant="outline" size="sm" onclick={() => onEditKey(k)}>Edit</Button>
              {#if k.id !== "default"}
                <Button
                  variant="destructive"
                  size="sm"
                  onclick={() => confirm(`Delete key ${k.name}?`) && run(() => api.removeKey(k.id))}
                >
                  Delete
                </Button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    </div>
    <div class="grid gap-2">
      <Label>Connect an agent</Label>
      {#if keys.length > 1}
        <Tabs.Root bind:value={keyId}>
          <Tabs.List>
            {#each keys as k (k.id)}
              <Tabs.Trigger value={k.id}>{k.name}</Tabs.Trigger>
            {/each}
          </Tabs.List>
        </Tabs.Root>
      {/if}
      <Tabs.Root bind:value={agent}>
        <Tabs.List>
          {#each agentInstalls("", "") as a (a.id)}
            <Tabs.Trigger value={a.id}>{a.name}</Tabs.Trigger>
          {/each}
        </Tabs.List>
      </Tabs.Root>
      <p class="text-muted-foreground text-xs m-0">{shown.hint}</p>
      <pre
        class="m-0 overflow-auto whitespace-pre-wrap break-words rounded-2xl bg-background p-3 font-mono text-xs leading-relaxed text-foreground ring-1 ring-foreground/10"
      >{shown.text}</pre>
      <div class="flex gap-2 flex-wrap">
        {#if shown.link}
          <Button onclick={() => openLink(install(selected?.token ?? "").link)}>Add to {shown.name}</Button>
        {/if}
        <Button variant="outline" onclick={() => copy(install(selected?.token ?? "").text)}>Copy</Button>
      </div>
    </div>
    <Separator />
    <div class="flex items-center justify-between gap-4">
      <div class="grid gap-0.5">
        <Label for="autostart">Run at system startup</Label>
        <p class="text-muted-foreground text-xs m-0">
          Starts hidden in the tray after login
        </p>
      </div>
      <Switch
        id="autostart"
        bind:checked={autostart}
        onCheckedChange={onAutostart}
      />
    </div>
  </Card.Content>
</Card.Root>

<KeyEditor bind:open={editorOpen} key={editing} {servers} onSaved={refresh} />
