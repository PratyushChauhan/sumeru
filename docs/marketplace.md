# Marketplace

The **Marketplace** tab lists a bundled curated catalog of HTTP MCPs that support Dynamic Client Registration (DCR).

![Marketplace tab with curated providers and Install buttons](/images/marketplace.png)

## Install

1. Open **Marketplace**
2. Find a provider and click **Install**
3. Complete browser OAuth
4. Sumeru saves the MCP only if sign-in succeeds

## Add another account

Once a provider is installed, its row shows **N installed** and **Add account**:

1. Click **Add account** and give the account a name (agents see it in `list_mcps`, so make it distinct — e.g. `Plane · acme`)
2. Click **Sign in**. Sumeru asks the provider for a fresh login (`prompt=login`)
3. If the provider signs you straight back into the first account, sign out of it in your browser (or use another browser profile) and try again

Each account keeps its own OAuth tokens. MCP names must be unique.

## What stays on Configure

Use **Configure → Add MCP** for:

- Non-DCR OAuth HTTP MCPs (bring your own Client ID)
- Stdio MCPs (`npx`, local binaries, etc.)
- Any custom Streamable HTTP URL not in the catalog
